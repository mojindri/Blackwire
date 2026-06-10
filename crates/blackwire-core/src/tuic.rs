//! TUIC v5 glue used by the instance builder.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use blackwire_app::dispatcher::Dispatcher;
use blackwire_config::schema::{InboundConfig, OutboundConfig, QuicConfig};
use blackwire_transport::{
    QuicSocketConfig, TuicClientConfig, TuicOutboundHandler, TuicServer, TuicServerConfig, TuicUser,
};
use uuid::Uuid;

use crate::hysteria2::socket_config_from_quic;

pub(crate) fn start_tuic_inbound(
    cfg: &InboundConfig,
    quic: Option<&QuicConfig>,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<tokio::task::JoinHandle<()>> {
    let server_config = parse_server_config(cfg, quic)?;
    let tag = cfg.tag.clone();

    let handle = tokio::spawn(async move {
        let server = TuicServer::new(server_config);
        if let Err(e) = server.serve(dispatcher).await {
            tracing::error!(tag = %tag, error = %e, "TUIC v5 server failed");
        }
    });

    Ok(handle)
}

pub(crate) fn build_tuic_outbound(
    cfg: &OutboundConfig,
    quic: Option<&QuicConfig>,
) -> Result<Arc<dyn blackwire_app::features::OutboundHandler>> {
    let client_config = parse_client_config(cfg, quic)?;
    Ok(TuicOutboundHandler::new(client_config, cfg.tag.clone()))
}

fn parse_server_config(cfg: &InboundConfig, quic: Option<&QuicConfig>) -> Result<TuicServerConfig> {
    let stream = cfg
        .stream_settings
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TUIC inbound '{}' missing streamSettings", cfg.tag))?;
    let tls = stream
        .tls_settings
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TUIC inbound '{}' missing tlsSettings", cfg.tag))?;
    let cert_path = require_field(&tls.certificate_file, "tlsSettings.certificateFile")?;
    let key_path = require_field(&tls.key_file, "tlsSettings.keyFile")?;
    let cert_pem = std::fs::read_to_string(cert_path)
        .with_context(|| format!("reading TUIC cert '{cert_path}'"))?;
    let key_pem = std::fs::read_to_string(key_path)
        .with_context(|| format!("reading TUIC key '{key_path}'"))?;

    let addr: SocketAddr = format!("{}:{}", cfg.listen, cfg.port)
        .parse()
        .with_context(|| format!("invalid TUIC listen address '{}:{}'", cfg.listen, cfg.port))?;
    let users = parse_users(&cfg.settings)?;
    if users.is_empty() {
        anyhow::bail!("TUIC inbound '{}' requires users", cfg.tag);
    }

    Ok(TuicServerConfig {
        tag: cfg.tag.clone(),
        addr,
        users,
        cert_pem,
        key_pem,
        server_name: Some(tls.server_name.clone()).filter(|s| !s.is_empty()),
        max_connections: cfg.limits.as_ref().and_then(|l| l.max_connections),
        auth_timeout: parse_duration_ms(&cfg.settings, "authTimeoutMs", 3_000),
        socket: parse_socket_config(&cfg.settings, quic),
        enable_udp: network_allows_udp(&cfg.settings),
    })
}

fn parse_client_config(
    cfg: &OutboundConfig,
    quic: Option<&QuicConfig>,
) -> Result<TuicClientConfig> {
    let s = &cfg.settings;
    let server = if let Some(server) = s.get("server").and_then(serde_json::Value::as_str) {
        server
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid TUIC server address '{server}'"))?
    } else {
        let address = s
            .get("address")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!("TUIC outbound '{}' missing 'server' or 'address'", cfg.tag)
            })?;
        let port = s
            .get("port")
            .or_else(|| s.get("server_port"))
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| anyhow::anyhow!("TUIC outbound '{}' missing 'port'", cfg.tag))?;
        format!("{address}:{port}")
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid TUIC server address '{address}:{port}'"))?
    };
    let uuid = s
        .get("uuid")
        .or_else(|| s.get("id"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("TUIC outbound '{}' missing 'uuid'", cfg.tag))?;
    let uuid = Uuid::parse_str(uuid).with_context(|| format!("invalid TUIC uuid '{uuid}'"))?;
    let password = s
        .get("password")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("TUIC outbound '{}' missing 'password'", cfg.tag))?
        .to_string();
    let server_name = s
        .get("serverName")
        .or_else(|| s.get("server_name"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| server.ip().to_string());
    let endpoint_shards = s
        .get("endpointShards")
        .or_else(|| s.get("endpoint_shards"))
        .and_then(serde_json::Value::as_u64)
        .map(|v| v.clamp(1, 64) as usize)
        .unwrap_or(1);
    let skip_cert_verify = s
        .get("skipCertVerify")
        .or_else(|| s.get("allowInsecure"))
        .or_else(|| s.get("insecure"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    Ok(TuicClientConfig {
        server,
        server_name,
        uuid,
        password,
        skip_cert_verify,
        endpoint_shards,
        socket: parse_socket_config(s, quic),
        enable_udp: network_allows_udp(s),
    })
}

fn parse_users(settings: &serde_json::Value) -> Result<Vec<TuicUser>> {
    let Some(users) = settings.get("users") else {
        let uuid = settings
            .get("uuid")
            .or_else(|| settings.get("id"))
            .and_then(serde_json::Value::as_str);
        let password = settings.get("password").and_then(serde_json::Value::as_str);
        return match (uuid, password) {
            (Some(uuid), Some(password)) => Ok(vec![TuicUser {
                uuid: Uuid::parse_str(uuid)
                    .with_context(|| format!("invalid TUIC uuid '{uuid}'"))?,
                password: password.to_string(),
            }]),
            _ => Ok(vec![]),
        };
    };

    if let Some(array) = users.as_array() {
        return array
            .iter()
            .map(|user| {
                let uuid = user
                    .get("uuid")
                    .or_else(|| user.get("id"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("TUIC user missing uuid"))?;
                let password = user
                    .get("password")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("TUIC user missing password"))?;
                Ok(TuicUser {
                    uuid: Uuid::parse_str(uuid)
                        .with_context(|| format!("invalid TUIC uuid '{uuid}'"))?,
                    password: password.to_string(),
                })
            })
            .collect();
    }

    if let Some(object) = users.as_object() {
        return object
            .iter()
            .map(|(uuid, password)| {
                let password = password.as_str().ok_or_else(|| {
                    anyhow::anyhow!("TUIC user '{uuid}' password must be a string")
                })?;
                Ok(TuicUser {
                    uuid: Uuid::parse_str(uuid)
                        .with_context(|| format!("invalid TUIC uuid '{uuid}'"))?,
                    password: password.to_string(),
                })
            })
            .collect();
    }

    anyhow::bail!("TUIC users must be an array or object")
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
    if let Some(endpoints) = overrides
        .get("endpoints")
        .and_then(serde_json::Value::as_u64)
    {
        socket.endpoint_count = endpoints.clamp(1, 64) as usize;
    }
    socket
}

fn parse_duration_ms(settings: &serde_json::Value, key: &str, default_ms: u64) -> Duration {
    Duration::from_millis(
        settings
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(default_ms),
    )
}

fn network_allows_udp(settings: &serde_json::Value) -> bool {
    settings
        .get("network")
        .and_then(serde_json::Value::as_str)
        .map(|network| network.split(',').any(|part| part.trim() == "udp"))
        .unwrap_or(true)
}

fn require_field<'a>(value: &'a str, name: &str) -> Result<&'a str> {
    if value.trim().is_empty() {
        anyhow::bail!("missing required field {name}");
    }
    Ok(value)
}
