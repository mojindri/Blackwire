//! Hysteria2 glue used by the instance builder.
//!
//! This module wires together the Hysteria2 transport (from blackwire-transport)
//! with the instance lifecycle. It reads typed endpoint settings and
//! constructs `Hysteria2ServerConfig` / `Hysteria2ClientConfig`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use dashmap::DashMap;

use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::user_limits::UserConnectionLimiter;
use blackwire_config::schema::{
    DatagramConfig, DatagramPolicy as ConfigDatagramPolicy, EndpointSettings, FecConfig,
    FecMode as ConfigFecMode, InboundConfig, OutboundConfig, QuicConfig,
};
use blackwire_transport::{
    CongestionConfig, CongestionMode, Hysteria2AuthStore, Hysteria2ClientConfig,
    Hysteria2OutboundHandler, Hysteria2Server, Hysteria2ServerConfig, QuicSocketConfig,
};
use tokio::sync::Semaphore;

use crate::net::listen_socket_addr;

const HYSTERIA2_DEFAULT_STABLE_MBPS: u64 = 100;
const HYSTERIA2_DEFAULT_THROUGHPUT_MBPS: u64 = 300;

/// Build and launch a Hysteria2 server inbound, returning a join handle for
/// the server task.
///
/// The server runs on a QUIC UDP socket (not TCP), so it does not go through
/// the normal `TcpServerTransport` path. Instead, it spawns its own task here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn start_hysteria2_inbound(
    cfg: &InboundConfig,
    auth_stores: &Arc<DashMap<String, Arc<Hysteria2AuthStore>>>,
    quic: Option<&QuicConfig>,
    datagram: Option<&DatagramConfig>,
    fec: Option<&FecConfig>,
    default_max_connections: Option<usize>,
    shared_limiter: Option<Arc<Semaphore>>,
    user_limiter: Option<Arc<UserConnectionLimiter>>,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<tokio::task::JoinHandle<()>> {
    let server_config = parse_server_config(
        cfg,
        auth_stores,
        quic,
        datagram,
        fec,
        default_max_connections,
        shared_limiter,
        user_limiter,
    )?;
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
#[allow(clippy::too_many_arguments)]
fn parse_server_config(
    cfg: &InboundConfig,
    auth_stores: &Arc<DashMap<String, Arc<Hysteria2AuthStore>>>,
    quic: Option<&QuicConfig>,
    datagram: Option<&DatagramConfig>,
    fec: Option<&FecConfig>,
    default_max_connections: Option<usize>,
    shared_limiter: Option<Arc<Semaphore>>,
    user_limiter: Option<Arc<UserConnectionLimiter>>,
) -> Result<Hysteria2ServerConfig> {
    let s = &cfg.settings;

    let password = require_hysteria2_auth(s, &cfg.tag)?.to_string();
    let user = hysteria2_user_label(s, &password);
    #[allow(clippy::unwrap_or_default)]
    let auth = auth_stores
        .entry(cfg.tag.clone())
        .or_insert_with(Hysteria2AuthStore::new)
        .clone();
    auth.replace(password.clone(), user);

    let congestion = parse_congestion_config(s)?;
    let up_mbps = congestion.up_mbps;
    let down_mbps = congestion.down_mbps;

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

    let addr = listen_socket_addr(cfg.listen, cfg.port);

    let max_connections = cfg
        .limits
        .as_ref()
        .and_then(|l| l.max_connections)
        .or(default_max_connections);
    let socket = parse_socket_config(s, quic);
    let datagram_enabled = datagram_enabled(s, datagram);
    let fec = parse_fec_policy(s, fec);
    let datagram_policy = parse_datagram_policy(s, datagram);

    Ok(Hysteria2ServerConfig {
        tag: cfg.tag.clone(),
        addr,
        auth,
        up_mbps,
        down_mbps,
        cert_pem,
        key_pem,
        max_connections,
        shared_limiter,
        user_limiter,
        congestion,
        socket,
        datagram_enabled,
        fec,
        datagram_policy,
    })
}

pub(crate) fn hysteria2_user_label(settings: &EndpointSettings, password: &str) -> Option<String> {
    settings.clients.iter().find_map(|client| {
        let auth = client.auth.as_deref().or(client.password.as_deref())?;
        if auth != password {
            return None;
        }
        client.label().map(ToOwned::to_owned)
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

    let server_str = s
        .server
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Hysteria2 outbound '{}' missing 'server'", cfg.tag))?;
    let server: SocketAddr = server_str
        .parse()
        .with_context(|| format!("invalid Hysteria2 server address '{server_str}'"))?;

    let password = s.auth.clone().unwrap_or_default();

    let skip_cert_verify = s.skip_cert_verify;
    let congestion = parse_congestion_config(s)?;
    let up_mbps = congestion.up_mbps;
    let down_mbps = congestion.down_mbps;
    let endpoint_shards = s.endpoint_shards.map(|v| v.clamp(1, 64)).unwrap_or(1);
    let socket = parse_socket_config(s, quic);
    let datagram_enabled = datagram_enabled(s, datagram);
    let fec = parse_fec_policy(s, fec);
    let datagram_policy = parse_datagram_policy(s, datagram);

    // Use the server address host as SNI if not explicitly configured.
    let server_name = s
        .server_name
        .clone()
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
    let Some(quic) = quic else {
        return automatic_quic_socket_config(detected_cpu_count(), detected_memory_bytes());
    };
    QuicSocketConfig {
        reuse_port: quic.reuse_port,
        endpoint_count: quic.endpoint_count(),
        recv_buffer_bytes: quic.recv_buffer_bytes,
        send_buffer_bytes: quic.send_buffer_bytes,
    }
}

/// Conservative automatic QUIC sizing used only when no explicit global QUIC
/// override exists. Caps keep per-process socket memory predictable even on
/// large hosts; endpoint-specific settings still take precedence afterward.
fn automatic_quic_socket_config(cpu_count: usize, memory_bytes: Option<u64>) -> QuicSocketConfig {
    const MIB: usize = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;

    let memory = memory_bytes.unwrap_or(2 * GIB);
    let buffer_bytes = match memory {
        bytes if bytes < GIB => MIB,
        bytes if bytes < 4 * GIB => 2 * MIB,
        bytes if bytes < 16 * GIB => 4 * MIB,
        _ => 8 * MIB,
    };
    let memory_limited_shards = match memory {
        bytes if bytes < GIB => 1,
        bytes if bytes < 4 * GIB => 2,
        _ => 4,
    };
    let cpu_limited_shards = match cpu_count.max(1) {
        1..=2 => 1,
        3..=8 => 2,
        _ => 4,
    };
    #[cfg(unix)]
    let endpoint_count = cpu_limited_shards.min(memory_limited_shards);
    #[cfg(not(unix))]
    let endpoint_count = 1;

    QuicSocketConfig {
        reuse_port: endpoint_count > 1,
        endpoint_count,
        recv_buffer_bytes: buffer_bytes,
        send_buffer_bytes: buffer_bytes,
    }
}

fn detected_cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

#[cfg(unix)]
fn detected_memory_bytes() -> Option<u64> {
    let pages = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) };
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    (pages > 0 && page_size > 0).then(|| (pages as u64).saturating_mul(page_size as u64))
}

#[cfg(windows)]
fn detected_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys)
}

#[cfg(not(any(unix, windows)))]
fn detected_memory_bytes() -> Option<u64> {
    None
}

fn parse_socket_config(settings: &EndpointSettings, quic: Option<&QuicConfig>) -> QuicSocketConfig {
    let mut socket = socket_config_from_quic(quic);
    let Some(overrides) = settings.quic.as_ref() else {
        return socket;
    };
    if let Some(reuse_port) = overrides.reuse_port {
        socket.reuse_port = reuse_port;
    }
    if let Some(endpoints) = overrides.endpoints.as_ref() {
        socket.endpoint_count = endpoints.resolve();
    }
    if let Some(bytes) = overrides.recv_buffer_bytes {
        socket.recv_buffer_bytes = bytes;
    }
    if let Some(bytes) = overrides.send_buffer_bytes {
        socket.send_buffer_bytes = bytes;
    }
    socket
}

fn datagram_enabled(settings: &EndpointSettings, datagram: Option<&DatagramConfig>) -> bool {
    let mut enabled = datagram
        .map(|cfg| cfg.enabled && cfg.udp_over_datagram)
        .unwrap_or(false);
    if let Some(overrides) = settings.datagram.as_ref() {
        if let Some(value) = overrides.enabled {
            enabled = value;
        }
        if let Some(value) = overrides.udp_over_datagram {
            enabled &= value;
        }
    }
    enabled
}

fn parse_fec_policy(
    settings: &EndpointSettings,
    fec: Option<&FecConfig>,
) -> blackwire_transport::FecPolicy {
    let mut cfg = fec.cloned().unwrap_or_default();
    if let Some(overrides) = settings.fec.as_ref() {
        if let Some(mode) = overrides.mode.as_deref() {
            cfg.mode = parse_config_fec_mode(mode);
        }
        if let Some(max) = overrides.max_overhead_percent {
            cfg.max_overhead_percent = max;
        }
        if let Some(avoid) = overrides.avoid_bulk_tcp {
            cfg.avoid_bulk_tcp = avoid;
        }
        if let Some(disable) = overrides.disable_for_sequential_dns {
            cfg.disable_for_sequential_dns = disable;
        }
        if let Some(min) = overrides.min_concurrency_for_block_fec {
            cfg.min_concurrency_for_block_fec = min;
        }
        if let Some(max) = overrides.max_generation_packets {
            cfg.max_generation_packets = max;
        }
        if let Some(delay) = overrides.max_generation_delay_ms {
            cfg.max_generation_delay_ms = delay;
        }
        if let Some(deadline) = overrides.recovery_deadline_ms {
            cfg.recovery_deadline_ms = deadline;
        }
        if let Some(window) = overrides.dedup_window_packets {
            cfg.dedup_window_packets = window;
        }
    }
    blackwire_transport::FecPolicy {
        mode: map_fec_mode(cfg.effective_mode()),
        max_overhead_percent: cfg.max_overhead_percent,
        group_size: cfg.max_generation_packets.max(2),
        disable_for_sequential_dns: cfg.disable_for_sequential_dns,
        min_concurrency_for_block_fec: cfg.min_concurrency_for_block_fec,
        max_generation_delay: std::time::Duration::from_millis(cfg.max_generation_delay_ms),
        recovery_deadline: std::time::Duration::from_millis(cfg.recovery_deadline_ms),
        dedup_window_packets: cfg.dedup_window_packets,
    }
}

fn parse_datagram_policy(
    settings: &EndpointSettings,
    datagram: Option<&DatagramConfig>,
) -> blackwire_transport::DatagramPolicy {
    let cfg = datagram.cloned().unwrap_or_default();
    let mut policy = map_datagram_policy(cfg.policy);
    let mut max_queue_delay_ms = cfg.max_queue_delay_ms;
    let mut fast_dns_retry = cfg.fast_dns_retry;
    let mut fast_dns_retry_delay_ms = cfg.fast_dns_retry_delay_ms;

    if let Some(overrides) = settings.datagram.as_ref() {
        if let Some(value) = overrides.policy.as_deref() {
            policy = parse_config_datagram_policy(value);
        }
        if let Some(value) = overrides.max_queue_delay_ms {
            max_queue_delay_ms = value;
        }
        if let Some(value) = overrides.fast_dns_retry {
            fast_dns_retry = value;
        }
        if let Some(value) = overrides.fast_dns_retry_delay_ms {
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

fn parse_congestion_config(settings: &EndpointSettings) -> Result<CongestionConfig> {
    let congestion = settings.congestion.as_ref();
    let mode = congestion
        .map(|value| value.mode.as_str())
        .unwrap_or("standard")
        .parse::<CongestionMode>()
        .map_err(anyhow::Error::msg)?;
    let default_mbps = match mode {
        CongestionMode::BrutalCompatible | CongestionMode::BadNetThroughput => {
            HYSTERIA2_DEFAULT_THROUGHPUT_MBPS
        }
        _ => HYSTERIA2_DEFAULT_STABLE_MBPS,
    };
    let up_mbps = settings.up_mbps.unwrap_or(default_mbps).clamp(1, 10_000);
    let down_mbps = settings.down_mbps.unwrap_or(default_mbps).clamp(1, 10_000);
    let min_ack_rate = congestion
        .and_then(|value| value.min_ack_rate)
        .unwrap_or(0.8)
        .clamp(0.05, 1.0);
    let max_queue_delay_ms = congestion
        .and_then(|value| value.max_queue_delay_ms)
        .unwrap_or(80)
        .clamp(1, 10_000);
    let pacing_gain = congestion
        .and_then(|value| value.pacing_gain)
        .unwrap_or(1.25)
        .clamp(0.1, 5.0);
    let loss_compensation = congestion
        .and_then(|value| value.loss_compensation)
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

fn require_hysteria2_auth<'a>(settings: &'a EndpointSettings, tag: &str) -> Result<&'a str> {
    let auth = settings.auth.as_deref();
    match auth {
        Some(auth) if !auth.is_empty() => Ok(auth),
        Some(_) => anyhow::bail!("Hysteria2 inbound '{tag}' settings.auth must not be empty"),
        None => anyhow::bail!("Hysteria2 inbound '{tag}' missing string settings.auth"),
    }
}

#[cfg(test)]
mod tests {
    use blackwire_config::schema::{DatagramConfig, EndpointSettings};
    use serde_json::json;

    use super::{
        automatic_quic_socket_config, datagram_enabled, hysteria2_user_label,
        parse_congestion_config, require_hysteria2_auth, socket_config_from_quic,
    };

    fn settings(value: serde_json::Value) -> EndpointSettings {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn hysteria2_user_label_matches_auth_client() {
        let settings = settings(json!({
            "auth": "secret",
            "clients": [
                { "email": "alice@example.test", "auth": "secret" },
                { "email": "bob@example.test", "auth": "other" }
            ]
        }));

        assert_eq!(
            hysteria2_user_label(&settings, "secret").as_deref(),
            Some("alice@example.test")
        );
    }

    #[test]
    fn hysteria2_user_label_accepts_password_alias() {
        let settings = settings(json!({
            "clients": [{ "name": "mobile", "password": "secret" }]
        }));

        assert_eq!(
            hysteria2_user_label(&settings, "secret").as_deref(),
            Some("mobile")
        );
    }

    #[test]
    fn hysteria2_user_label_ignores_nonmatching_clients() {
        let settings = settings(json!({
            "clients": [{ "email": "alice@example.test", "auth": "other" }]
        }));

        assert!(hysteria2_user_label(&settings, "secret").is_none());
    }

    #[test]
    fn require_hysteria2_auth_rejects_missing_non_string_and_empty_values() {
        for settings in [
            settings(json!({})),
            settings(json!({ "auth": "" })),
            settings(json!({ "auth": null })),
        ] {
            assert!(require_hysteria2_auth(&settings, "h2-public").is_err());
        }
    }

    #[test]
    fn require_hysteria2_auth_accepts_non_empty_string() {
        let settings = settings(json!({ "auth": "secret" }));

        assert_eq!(
            require_hysteria2_auth(&settings, "h2-public").unwrap(),
            "secret"
        );
    }

    #[test]
    fn hysteria2_standard_defaults_to_stable_windows_without_fixed_rate_auth() {
        let cfg = parse_congestion_config(&EndpointSettings::default()).unwrap();

        assert_eq!(cfg.mode, blackwire_transport::CongestionMode::StandardQuic);
        assert_eq!(cfg.up_mbps, 100);
        assert_eq!(cfg.down_mbps, 100);
        assert_eq!(cfg.auth_rx_bps(), 0);
    }

    #[test]
    fn hysteria2_throughput_mode_uses_explicit_or_throughput_defaults() {
        let cfg = parse_congestion_config(&settings(json!({
            "congestion": { "mode": "brutal-compatible" }
        })))
        .unwrap();

        assert_eq!(cfg.up_mbps, 300);
        assert_eq!(cfg.down_mbps, 300);
        assert!(cfg.auth_rx_bps() > 0);
    }

    #[test]
    fn hysteria2_bandwidth_fields_are_honored_and_clamped() {
        let cfg = parse_congestion_config(&settings(json!({
            "upMbps": 250,
            "downMbps": 50_000,
            "congestion": { "mode": "badnet-throughput" }
        })))
        .unwrap();

        assert_eq!(cfg.up_mbps, 250);
        assert_eq!(cfg.down_mbps, 10_000);
    }

    #[test]
    fn datagram_defaults_to_disabled_without_explicit_policy() {
        assert!(!datagram_enabled(&EndpointSettings::default(), None));
    }

    #[test]
    fn datagram_can_be_enabled_explicitly_per_inbound() {
        assert!(datagram_enabled(
            &settings(json!({ "datagram": { "enabled": true, "udpOverDatagram": true } })),
            None
        ));
    }

    #[test]
    fn datagram_respects_top_level_policy_when_present() {
        let cfg = DatagramConfig {
            enabled: true,
            udp_over_datagram: true,
            ..DatagramConfig::default()
        };

        assert!(datagram_enabled(&EndpointSettings::default(), Some(&cfg)));
    }

    #[test]
    fn automatic_quic_tuning_is_bounded_by_cpu_and_memory() {
        let tiny = automatic_quic_socket_config(32, Some(512 * 1024 * 1024));
        assert_eq!(tiny.endpoint_count, 1);
        assert_eq!(tiny.recv_buffer_bytes, 1024 * 1024);

        let large = automatic_quic_socket_config(32, Some(32 * 1024 * 1024 * 1024));
        #[cfg(unix)]
        assert_eq!(large.endpoint_count, 4);
        #[cfg(not(unix))]
        assert_eq!(large.endpoint_count, 1);
        assert_eq!(large.recv_buffer_bytes, 8 * 1024 * 1024);
        assert_eq!(large.send_buffer_bytes, 8 * 1024 * 1024);
    }

    #[test]
    fn explicit_quic_settings_override_automatic_tuning() {
        let explicit = blackwire_config::schema::QuicConfig {
            reuse_port: false,
            endpoints: blackwire_config::schema::EndpointCount::Fixed(3),
            recv_buffer_bytes: 3 * 1024 * 1024,
            send_buffer_bytes: 5 * 1024 * 1024,
            max_datagram_size: blackwire_config::schema::DatagramSize::Named("auto".into()),
        };
        let socket = socket_config_from_quic(Some(&explicit));
        assert_eq!(socket.endpoint_count, 3);
        assert_eq!(socket.recv_buffer_bytes, 3 * 1024 * 1024);
        assert_eq!(socket.send_buffer_bytes, 5 * 1024 * 1024);
    }
}
