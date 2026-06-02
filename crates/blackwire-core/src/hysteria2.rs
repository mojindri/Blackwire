//! Hysteria2 glue used by the instance builder.
//!
//! This module wires together the Hysteria2 transport (from blackwire-transport)
//! with the instance lifecycle. It reads the config settings JSON and
//! constructs `Hysteria2ServerConfig` / `Hysteria2ClientConfig`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};

use blackwire_app::dispatcher::Dispatcher;
use blackwire_config::schema::{
    DatagramConfig, DatagramPolicy as ConfigDatagramPolicy, FecConfig, FecMode as ConfigFecMode,
    InboundConfig, OutboundConfig, QuicConfig,
};
use blackwire_transport::{
    CongestionConfig, CongestionMode, Hysteria2ClientConfig, Hysteria2OutboundHandler,
    Hysteria2Server, Hysteria2ServerConfig, QuicSocketConfig,
};

/// Build and launch a Hysteria2 server inbound, returning a join handle for
/// the server task.
///
/// The server runs on a QUIC UDP socket (not TCP), so it does not go through
/// the normal `TcpServerTransport` path. Instead, it spawns its own task here.
pub(crate) fn start_hysteria2_inbound(
    cfg: &InboundConfig,
    quic: Option<&QuicConfig>,
    datagram: Option<&DatagramConfig>,
    fec: Option<&FecConfig>,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<tokio::task::JoinHandle<()>> {
    let server_config = parse_server_config(cfg, quic, datagram, fec)?;
    let tag = cfg.tag.clone();

    let handle = tokio::spawn(async move {
        let server = Hysteria2Server::new(server_config);
        if let Err(e) = server.serve(dispatcher).await {
            tracing::error!(tag = %tag, error = %e, "Hysteria2 server failed");
        }
    });

    Ok(handle)
}

/// Build a `Hysteria2OutboundHandler` from the outbound config.
pub(crate) fn build_hysteria2_outbound(
    cfg: &OutboundConfig,
    quic: Option<&QuicConfig>,
    datagram: Option<&DatagramConfig>,
    fec: Option<&FecConfig>,
) -> Result<Arc<dyn blackwire_app::features::OutboundHandler>> {
    let client_config = parse_client_config(cfg, quic, datagram, fec)?;
    Ok(Hysteria2OutboundHandler::new(
        client_config,
        cfg.tag.clone(),
    ))
}

// ── Config parsing ────────────────────────────────────────────────────────────

/// Parse Hysteria2 server settings from inbound config.
fn parse_server_config(
    cfg: &InboundConfig,
    quic: Option<&QuicConfig>,
    datagram: Option<&DatagramConfig>,
    fec: Option<&FecConfig>,
) -> Result<Hysteria2ServerConfig> {
    let s = &cfg.settings;

    let password = s["auth"].as_str().unwrap_or_default().to_string();

    let up_mbps = s["upMbps"].as_u64().unwrap_or(100);
    let down_mbps = s["downMbps"].as_u64().unwrap_or(100);
    let congestion = parse_congestion_config(s, up_mbps, down_mbps)?;

    // Read TLS cert+key from stream_settings.tlsSettings.
    let stream = cfg.stream_settings.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Hysteria2 inbound '{tag}' missing streamSettings",
            tag = cfg.tag
        )
    })?;

    let tls = stream.tls_settings.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Hysteria2 inbound '{tag}' missing tlsSettings",
            tag = cfg.tag
        )
    })?;

    let cert_path = require_field(&tls.certificate_file, "tlsSettings.certificateFile")?;
    let key_path = require_field(&tls.key_file, "tlsSettings.keyFile")?;

    let cert_pem = std::fs::read_to_string(cert_path)
        .with_context(|| format!("reading Hysteria2 cert '{cert_path}'"))?;
    let key_pem = std::fs::read_to_string(key_path)
        .with_context(|| format!("reading Hysteria2 key '{key_path}'"))?;

    let addr: SocketAddr = format!("{}:{}", cfg.listen, cfg.port)
        .parse()
        .with_context(|| {
            format!(
                "invalid Hysteria2 listen address '{}:{}'",
                cfg.listen, cfg.port
            )
        })?;

    let max_connections = cfg.limits.as_ref().and_then(|l| l.max_connections);
    let socket = parse_socket_config(s, quic);
    let datagram_enabled = datagram_enabled(s, datagram);
    let fec = parse_fec_policy(s, fec);
    let datagram_policy = parse_datagram_policy(s, datagram);

    Ok(Hysteria2ServerConfig {
        tag: cfg.tag.clone(),
        addr,
        password,
        up_mbps,
        down_mbps,
        cert_pem,
        key_pem,
        max_connections,
        congestion,
        socket,
        datagram_enabled,
        fec,
        datagram_policy,
    })
}

/// Parse Hysteria2 client settings from outbound config.
fn parse_client_config(
    cfg: &OutboundConfig,
    quic: Option<&QuicConfig>,
    datagram: Option<&DatagramConfig>,
    fec: Option<&FecConfig>,
) -> Result<Hysteria2ClientConfig> {
    let s = &cfg.settings;

    let server_str = s["server"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Hysteria2 outbound '{}' missing 'server'", cfg.tag))?;
    let server: SocketAddr = server_str
        .parse()
        .with_context(|| format!("invalid Hysteria2 server address '{server_str}'"))?;

    let password = s["auth"].as_str().unwrap_or_default().to_string();

    let up_mbps = s["upMbps"].as_u64().unwrap_or(100);
    let down_mbps = s["downMbps"].as_u64().unwrap_or(100);
    let skip_cert_verify = s["skipCertVerify"].as_bool().unwrap_or(false);
    let congestion = parse_congestion_config(s, up_mbps, down_mbps)?;
    let endpoint_shards = s["endpointShards"]
        .as_u64()
        .map(|v| v.clamp(1, 64) as usize)
        .unwrap_or(1);
    let socket = parse_socket_config(s, quic);
    let datagram_enabled = datagram_enabled(s, datagram);
    let fec = parse_fec_policy(s, fec);
    let datagram_policy = parse_datagram_policy(s, datagram);

    // Use the server address host as SNI if not explicitly configured.
    let server_name = s["serverName"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| server.ip().to_string());

    Ok(Hysteria2ClientConfig {
        server,
        server_name,
        password,
        up_mbps,
        down_mbps,
        skip_cert_verify,
        congestion,
        endpoint_shards,
        socket,
        datagram_enabled,
        fec,
        datagram_policy,
    })
}

pub(crate) fn socket_config_from_quic(quic: Option<&QuicConfig>) -> QuicSocketConfig {
    let quic = quic.cloned().unwrap_or_default();
    QuicSocketConfig {
        reuse_port: quic.reuse_port,
        endpoint_count: quic.endpoint_count(),
        recv_buffer_bytes: quic.recv_buffer_bytes,
        send_buffer_bytes: quic.send_buffer_bytes,
    }
}

fn parse_socket_config(
    settings: &serde_json::Value,
    quic: Option<&QuicConfig>,
) -> QuicSocketConfig {
    let mut socket = socket_config_from_quic(quic);
    let Some(overrides) = settings.get("quic") else {
        return socket;
    };
    if let Some(reuse_port) = overrides
        .get("reusePort")
        .and_then(serde_json::Value::as_bool)
    {
        socket.reuse_port = reuse_port;
    }
    if let Some(endpoints) = overrides.get("endpoints").and_then(parse_endpoint_count) {
        socket.endpoint_count = endpoints.clamp(1, 64);
    }
    if let Some(bytes) = overrides
        .get("recvBufferBytes")
        .and_then(serde_json::Value::as_u64)
    {
        socket.recv_buffer_bytes = bytes as usize;
    }
    if let Some(bytes) = overrides
        .get("sendBufferBytes")
        .and_then(serde_json::Value::as_u64)
    {
        socket.send_buffer_bytes = bytes as usize;
    }
    socket
}

fn parse_endpoint_count(value: &serde_json::Value) -> Option<usize> {
    match value {
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("cpu") => Some(
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        ),
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.as_u64().map(|v| v as usize),
        _ => None,
    }
}

fn datagram_enabled(settings: &serde_json::Value, datagram: Option<&DatagramConfig>) -> bool {
    let mut enabled = datagram.cloned().unwrap_or_default().enabled
        && datagram.cloned().unwrap_or_default().udp_over_datagram;
    if let Some(overrides) = settings.get("datagram") {
        if let Some(value) = overrides
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
        {
            enabled = value;
        }
        if let Some(value) = overrides
            .get("udpOverDatagram")
            .and_then(serde_json::Value::as_bool)
        {
            enabled &= value;
        }
    }
    enabled
}

fn parse_fec_policy(
    settings: &serde_json::Value,
    fec: Option<&FecConfig>,
) -> blackwire_transport::FecPolicy {
    let mut cfg = fec.cloned().unwrap_or_default();
    if let Some(overrides) = settings.get("fec") {
        if let Some(mode) = overrides.get("mode").and_then(serde_json::Value::as_str) {
            cfg.mode = parse_config_fec_mode(mode);
        }
        if let Some(max) = overrides
            .get("maxOverheadPercent")
            .and_then(serde_json::Value::as_u64)
        {
            cfg.max_overhead_percent = max.min(u8::MAX as u64) as u8;
        }
        if let Some(avoid) = overrides
            .get("avoidBulkTcp")
            .and_then(serde_json::Value::as_bool)
        {
            cfg.avoid_bulk_tcp = avoid;
        }
    }
    blackwire_transport::FecPolicy {
        mode: map_fec_mode(cfg.effective_mode()),
        max_overhead_percent: cfg.max_overhead_percent,
        group_size: 4,
    }
}

fn parse_datagram_policy(
    settings: &serde_json::Value,
    datagram: Option<&DatagramConfig>,
) -> blackwire_transport::DatagramPolicy {
    let cfg = datagram.cloned().unwrap_or_default();
    let mut policy = map_datagram_policy(cfg.policy);
    let mut max_queue_delay_ms = cfg.max_queue_delay_ms;
    let mut fast_dns_retry = cfg.fast_dns_retry;
    let mut fast_dns_retry_delay_ms = cfg.fast_dns_retry_delay_ms;

    if let Some(overrides) = settings.get("datagram") {
        if let Some(value) = overrides.get("policy").and_then(serde_json::Value::as_str) {
            policy = parse_config_datagram_policy(value);
        }
        if let Some(value) = overrides
            .get("maxQueueDelayMs")
            .and_then(serde_json::Value::as_u64)
        {
            max_queue_delay_ms = value;
        }
        if let Some(value) = overrides
            .get("fastDnsRetry")
            .and_then(serde_json::Value::as_bool)
        {
            fast_dns_retry = value;
        }
        if let Some(value) = overrides
            .get("fastDnsRetryDelayMs")
            .and_then(serde_json::Value::as_u64)
        {
            fast_dns_retry_delay_ms = value;
        }
    }

    blackwire_transport::DatagramPolicy {
        mode: policy,
        max_queue_delay_ms: max_queue_delay_ms.max(1),
        fast_dns_retry,
        fast_dns_retry_delay_ms,
    }
}

fn parse_config_datagram_policy(value: &str) -> blackwire_transport::DatagramPriorityMode {
    match value {
        "h2-plus" | "h2plus" | "h2_plus" => blackwire_transport::DatagramPriorityMode::H2Plus,
        _ => blackwire_transport::DatagramPriorityMode::Standard,
    }
}

fn map_datagram_policy(policy: ConfigDatagramPolicy) -> blackwire_transport::DatagramPriorityMode {
    match policy {
        ConfigDatagramPolicy::Standard => blackwire_transport::DatagramPriorityMode::Standard,
        ConfigDatagramPolicy::H2Plus => blackwire_transport::DatagramPriorityMode::H2Plus,
    }
}

fn parse_config_fec_mode(value: &str) -> ConfigFecMode {
    match value {
        "xor1-of-n" | "xor1OfN" | "xor" => ConfigFecMode::Xor1OfN,
        "reed-solomon" | "reedSolomon" => ConfigFecMode::ReedSolomon,
        "raptor-like" | "raptorLike" => ConfigFecMode::RaptorLike,
        "auto" => ConfigFecMode::Auto,
        _ => ConfigFecMode::Off,
    }
}

fn map_fec_mode(mode: ConfigFecMode) -> blackwire_transport::FecMode {
    match mode {
        ConfigFecMode::Off => blackwire_transport::FecMode::Off,
        ConfigFecMode::Xor1OfN => blackwire_transport::FecMode::Xor1OfN,
        ConfigFecMode::ReedSolomon => blackwire_transport::FecMode::ReedSolomon,
        ConfigFecMode::RaptorLike => blackwire_transport::FecMode::RaptorLike,
        ConfigFecMode::Auto => blackwire_transport::FecMode::Auto,
    }
}

fn parse_congestion_config(
    settings: &serde_json::Value,
    up_mbps: u64,
    down_mbps: u64,
) -> Result<CongestionConfig> {
    let congestion = settings
        .get("congestion")
        .unwrap_or(&serde_json::Value::Null);
    let mode = congestion
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("brutal-compatible")
        .parse::<CongestionMode>()
        .map_err(anyhow::Error::msg)?;
    let min_ack_rate = congestion
        .get("minAckRate")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8)
        .clamp(0.05, 1.0);
    let max_queue_delay_ms = congestion
        .get("maxQueueDelayMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(80)
        .clamp(1, 10_000);
    let pacing_gain = congestion
        .get("pacingGain")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.25)
        .clamp(0.1, 5.0);
    let loss_compensation = congestion
        .get("lossCompensation")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    Ok(CongestionConfig {
        mode,
        up_mbps,
        down_mbps,
        min_ack_rate,
        max_queue_delay: Duration::from_millis(max_queue_delay_ms),
        pacing_gain,
        loss_compensation,
    })
}

fn require_field<'a>(value: &'a str, field: &str) -> Result<&'a str> {
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(value)
}
