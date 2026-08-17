//! TUIC v5 glue used by the instance builder.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::user_limits::UserConnectionLimiter;
use blackwire_config::schema::{EndpointSettings, InboundConfig, OutboundConfig, QuicConfig};
use blackwire_transport::{
    QuicSocketConfig, TuicAuthStore, TuicClientConfig, TuicOutboundHandler, TuicServer,
    TuicServerConfig, TuicUser,
};
use dashmap::DashMap;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::hysteria2::socket_config_from_quic;
use crate::net::{listen_socket_addr, socket_addr_from_address_port};

pub(crate) fn start_tuic_inbound(
    cfg: &InboundConfig,
    auth_stores: &Arc<DashMap<String, Arc<TuicAuthStore>>>,
    quic: Option<&QuicConfig>,
    default_max_connections: Option<usize>,
    shared_limiter: Option<Arc<Semaphore>>,
    user_limiter: Option<Arc<UserConnectionLimiter>>,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<tokio::task::JoinHandle<()>> {
    let server_config = parse_server_config(
        cfg,
        auth_stores,
        quic,
        default_max_connections,
        shared_limiter,
        user_limiter,
    )?;
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

fn parse_server_config(
    cfg: &InboundConfig,
    auth_stores: &Arc<DashMap<String, Arc<TuicAuthStore>>>,
    quic: Option<&QuicConfig>,
    default_max_connections: Option<usize>,
    shared_limiter: Option<Arc<Semaphore>>,
    user_limiter: Option<Arc<UserConnectionLimiter>>,
) -> Result<TuicServerConfig> {
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

    let addr = listen_socket_addr(cfg.listen, cfg.port);
    let users = parse_users(&cfg.settings)?;
    if users.is_empty() {
        anyhow::bail!("TUIC inbound '{}' requires users", cfg.tag);
    }
    #[allow(clippy::unwrap_or_default)]
    let auth = auth_stores
        .entry(cfg.tag.clone())
        .or_insert_with(TuicAuthStore::new)
        .clone();
    auth.replace_users(users);

    Ok(TuicServerConfig {
        tag: cfg.tag.clone(),
        addr,
        auth,
        cert_pem,
        key_pem,
        server_name: Some(tls.server_name.clone()).filter(|s| !s.is_empty()),
        max_connections: cfg
            .limits
            .as_ref()
            .and_then(|l| l.max_connections)
            .or(default_max_connections),
        shared_limiter,
        user_limiter,
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
    let server = if let Some(server) = s.server.as_deref() {
        server
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid TUIC server address '{server}'"))?
    } else {
        let address = s.address.as_deref().ok_or_else(|| {
            anyhow::anyhow!("TUIC outbound '{}' missing 'server' or 'address'", cfg.tag)
        })?;
        let port = s
            .port
            .map(u64::from)
            .ok_or_else(|| anyhow::anyhow!("TUIC outbound '{}' missing 'port'", cfg.tag))?;
        socket_addr_from_address_port(
            address,
            port,
            &format!("invalid TUIC server address for outbound '{}'", cfg.tag),
        )?
    };
    let uuid = s
        .uuid
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TUIC outbound '{}' missing 'uuid'", cfg.tag))?;
    let uuid = Uuid::parse_str(uuid).with_context(|| format!("invalid TUIC uuid '{uuid}'"))?;
    let password = s
        .password
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TUIC outbound '{}' missing 'password'", cfg.tag))?
        .to_string();
    let server_name = s
        .server_name
        .as_deref()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| server.ip().to_string());
    let endpoint_shards = s.endpoint_shards.map(|v| v.clamp(1, 64)).unwrap_or(1);
    let skip_cert_verify = s.skip_cert_verify;

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

pub(crate) fn parse_users(settings: &EndpointSettings) -> Result<Vec<TuicUser>> {
    if settings.users.is_empty() {
        let uuid = settings.uuid.as_deref();
        let password = settings.password.as_deref();
        return match (uuid, password) {
            (Some(uuid), Some(password)) => Ok(vec![TuicUser {
                uuid: Uuid::parse_str(uuid)
                    .with_context(|| format!("invalid TUIC uuid '{uuid}'"))?,
                password: password.to_string(),
                label: Some(
                    settings
                        .email
                        .as_deref()
                        .or(settings.name.as_deref())
                        .unwrap_or(uuid)
                        .to_string(),
                ),
            }]),
            _ => Ok(vec![]),
        };
    }

    settings
        .users
        .iter()
        .map(|user| {
            let uuid = user
                .identifier()
                .ok_or_else(|| anyhow::anyhow!("TUIC user missing uuid"))?;
            let password = user
                .password
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("TUIC user missing password"))?;
            Ok(TuicUser {
                uuid: Uuid::parse_str(uuid)
                    .with_context(|| format!("invalid TUIC uuid '{uuid}'"))?,
                password: password.to_string(),
                label: Some(user.label().unwrap_or(uuid).to_string()),
            })
        })
        .collect()
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
    socket
}

fn parse_duration_ms(settings: &EndpointSettings, _key: &str, default_ms: u64) -> Duration {
    Duration::from_millis(settings.auth_timeout_ms.unwrap_or(default_ms))
}

fn network_allows_udp(settings: &EndpointSettings) -> bool {
    settings
        .network
        .as_deref()
        .map(|network| network.split(',').any(|part| part.trim() == "udp"))
        .unwrap_or(true)
}

fn require_field<'a>(value: &'a str, name: &str) -> Result<&'a str> {
    if value.trim().is_empty() {
        anyhow::bail!("missing required field {name}");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings(value: serde_json::Value) -> EndpointSettings {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn parse_tuic_array_users_keeps_email_label() {
        let settings = settings(json!({
            "users": [{
                "uuid": "11111111-1111-4111-8111-111111111111",
                "password": "secret",
                "email": "tuic@example.local"
            }]
        }));

        let users = parse_users(&settings).unwrap();
        assert_eq!(users[0].label.as_deref(), Some("tuic@example.local"));
    }

    #[test]
    fn parse_tuic_users_use_name_when_email_is_absent() {
        let uuid = "11111111-1111-4111-8111-111111111111";
        let settings = settings(json!({
            "users": [{ "uuid": uuid, "password": "secret", "name": uuid }]
        }));

        let users = parse_users(&settings).unwrap();
        assert_eq!(users[0].label.as_deref(), Some(uuid));
    }
}
