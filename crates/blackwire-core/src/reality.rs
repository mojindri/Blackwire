//! REALITY glue used by the instance builder.
//!
//! Protocol crates own VLESS, transport crates own REALITY, and this module
//! wires them together when config asks for `security = "reality"`.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use base64::Engine as _;

use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::features::{
    ConnectionHandler, InboundHandler, OutboundConnectResult, OutboundHandler,
};
use blackwire_common::{with_handshake_timeout, BoxedStream, ProxyError, RelayRateLimit};
use blackwire_config::schema::{SecurityType, StreamSettingsConfig};
use blackwire_protocol::vless::codec::Command;
use blackwire_protocol::vless::{
    connect_vless_on_stream, connect_vless_on_stream_with_early_payload,
};
use tracing::warn;

use blackwire_transport::{
    complete_tls13_server_handshake, RealityClient, RealityClientConfig, RealityServer,
    RealityServerConfig, Tls13Stream,
};

const MAX_LEGACY_REALITY_TIME_DIFF_SECONDS: u64 = 3600;

/// Return true when a config section asks for REALITY transport.
pub(crate) fn uses_reality(stream_settings: &Option<StreamSettingsConfig>) -> bool {
    stream_settings
        .as_ref()
        .is_some_and(|settings| settings.security == SecurityType::Reality)
}

/// Connection adapter that unwraps REALITY before handing bytes to VLESS.
pub(crate) struct RealityConnectionHandler {
    reality: Arc<RealityServer>,
    cover_sni: String,
    handshake_timeout: Option<std::time::Duration>,
    inbound: Arc<dyn InboundHandler>,
    dispatcher: Arc<dyn Dispatcher>,
}

impl RealityConnectionHandler {
    pub(crate) fn new(
        reality: Arc<RealityServer>,
        cover_sni: &str,
        handshake_timeout: Option<std::time::Duration>,
        inbound: Arc<dyn InboundHandler>,
        dispatcher: Arc<dyn Dispatcher>,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            reality,
            cover_sni: if cover_sni.is_empty() {
                "localhost".to_string()
            } else {
                cover_sni.to_string()
            },
            handshake_timeout,
            inbound,
            dispatcher,
        }))
    }
}

#[async_trait::async_trait]
impl ConnectionHandler for RealityConnectionHandler {
    async fn handle_connection(
        &self,
        stream: BoxedStream,
        source: SocketAddr,
    ) -> Result<(), ProxyError> {
        let accepted =
            with_handshake_timeout(self.handshake_timeout, self.reality.accept_with_key(stream))
                .await?;
        let cover_profile = accepted.cover_profile;
        let mut stream = accepted.stream;
        // Keep this on the custom TLS path: rustls does not currently negotiate with uTLS REALITY clients.
        let app_keys = with_handshake_timeout(
            self.handshake_timeout,
            complete_tls13_server_handshake(
                &mut stream,
                &accepted.auth_key,
                &self.cover_sni,
                cover_profile.as_ref(),
            ),
        )
        .await
        .map_err(|e| {
            warn!(error = %e, sni = %self.cover_sni, "REALITY post-auth TLS handshake failed");
            e
        })?;
        let stream = Box::new(Tls13Stream::new_server(stream, app_keys));
        self.inbound
            .handle(stream, source, Arc::clone(&self.dispatcher))
            .await
            .map_err(|e| {
                warn!(error = %e, "REALITY VLESS inbound failed after TLS");
                e
            })
    }
}

/// VLESS outbound over a REALITY-authenticated TCP stream.
pub(crate) struct RealityVlessOutbound {
    tag: String,
    reality: RealityClient,
    uuid: [u8; 16],
    flow: String,
}

impl RealityVlessOutbound {
    pub(crate) fn new(
        tag: impl Into<String>,
        reality: RealityClient,
        uuid: [u8; 16],
        flow: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            tag: tag.into(),
            reality,
            uuid,
            flow,
        })
    }
}

#[async_trait::async_trait]
impl OutboundHandler for RealityVlessOutbound {
    fn tag(&self) -> &str {
        &self.tag
    }

    async fn connect(
        &self,
        _ctx: &blackwire_app::context::Context,
        dest: &blackwire_common::Address,
    ) -> Result<BoxedStream, ProxyError> {
        let stream = self.reality.dial().await?;
        connect_vless_on_stream(stream, &self.uuid, &self.flow, Command::Tcp, dest).await
    }

    async fn connect_with_early_payload(
        &self,
        _ctx: &blackwire_app::context::Context,
        dest: &blackwire_common::Address,
        early_payload: Option<&[u8]>,
    ) -> Result<OutboundConnectResult, ProxyError> {
        let stream = self.reality.dial().await?;
        connect_vless_on_stream_with_early_payload(
            stream,
            &self.uuid,
            &self.flow,
            Command::Tcp,
            dest,
            early_payload,
        )
        .await
    }
}

pub(crate) fn build_reality_client(
    cfg: &blackwire_config::schema::OutboundConfig,
    server: SocketAddr,
) -> Result<RealityClient> {
    let reality = cfg
        .stream_settings
        .as_ref()
        .and_then(|settings| settings.reality_settings.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!("REALITY outbound missing streamSettings.realitySettings")
        })?;

    Ok(RealityClient::new(RealityClientConfig {
        server,
        server_public_key: parse_reality_key_32(&reality.public_key, "publicKey")?,
        short_id: parse_short_id(&reality.short_id, "shortId")?,
        sni: require_non_empty(&reality.server_name, "serverName")?.to_string(),
        fingerprint: reality.fingerprint.clone(),
    }))
}

pub(crate) fn build_reality_server(
    cfg: &blackwire_config::schema::InboundConfig,
) -> Result<Arc<RealityServer>> {
    let reality = cfg
        .stream_settings
        .as_ref()
        .and_then(|settings| settings.reality_settings.as_ref())
        .ok_or_else(|| anyhow::anyhow!("REALITY inbound missing streamSettings.realitySettings"))?;

    let fallback = require_non_empty(&reality.dest, "dest")?
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid REALITY fallback dest '{}'", reality.dest))?;

    let short_ids = reality
        .short_ids
        .iter()
        .map(|short_id| parse_short_id(short_id, "shortIds[]"))
        .collect::<Result<Vec<_>>>()?;

    if short_ids.is_empty() {
        anyhow::bail!("REALITY inbound requires at least one shortIds entry");
    }

    let server_names = reality_server_names(reality)?;
    let max_time_diff = reality_max_time_diff_seconds(reality)?;
    warn_for_risky_reality_cover(&server_names);

    let upload_limit = reality.limit_fallback_upload.map(reality_fallback_limit);
    let download_limit = reality.limit_fallback_download.map(reality_fallback_limit);
    Ok(Arc::new(
        RealityServer::new(RealityServerConfig {
            private_key: parse_reality_key_32(&reality.private_key, "privateKey")?,
            short_ids,
            server_names,
            fallback,
            max_time_diff: max_time_diff as i64,
        })
        .with_fallback_limits(upload_limit, download_limit),
    ))
}

fn reality_fallback_limit(
    value: blackwire_config::schema::RealityFallbackLimitConfig,
) -> RelayRateLimit {
    RelayRateLimit {
        after_bytes: value.after_bytes,
        bytes_per_second: value.bytes_per_sec,
        burst_bytes: value.burst_bytes_per_sec,
    }
}

fn warn_for_risky_reality_cover(server_names: &[String]) {
    const HIGH_RISK_SUFFIXES: &[&str] = &[
        "microsoft.com",
        "apple.com",
        "icloud.com",
        ".ru",
        ".ir",
        ".cn",
    ];
    for name in server_names {
        let normalized = name.trim_end_matches('.').to_ascii_lowercase();
        if HIGH_RISK_SUFFIXES.iter().any(|suffix| {
            if suffix.starts_with('.') {
                normalized.ends_with(suffix)
            } else {
                normalized == *suffix
                    || normalized
                        .strip_suffix(suffix)
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
        }) {
            warn!(
                cover = %name,
                "REALITY cover may attract blocking or be unsuitable; prefer a stable, reachable TLS 1.3 site outside commonly targeted domains"
            );
        }
    }
}

fn reality_server_names(reality: &blackwire_config::schema::RealityConfig) -> Result<Vec<String>> {
    let mut names = reality
        .server_names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let legacy = reality.server_name.trim();
    if !legacy.is_empty() && !names.iter().any(|name| name == legacy) {
        names.push(legacy.to_string());
    }

    if names.is_empty() {
        anyhow::bail!("REALITY inbound requires non-empty serverNames");
    }
    if let Some(name) = names.iter().find(|name| name.contains('*')) {
        anyhow::bail!("REALITY serverNames does not support wildcards: '{name}'");
    }

    names.sort();
    names.dedup();
    Ok(names)
}

fn reality_max_time_diff_seconds(reality: &blackwire_config::schema::RealityConfig) -> Result<u64> {
    if let Some(explicit) = reality.max_time_diff_seconds {
        return Ok(explicit);
    }

    let legacy = reality.max_time_diff;
    if legacy > MAX_LEGACY_REALITY_TIME_DIFF_SECONDS {
        anyhow::bail!(
            "REALITY maxTimeDiff is interpreted as seconds by Blackwire; \
             value {legacy} is suspiciously large, use maxTimeDiffSeconds explicitly"
        );
    }
    Ok(legacy)
}

fn parse_reality_key_32(value: &str, field: &str) -> Result<[u8; 32]> {
    let value = require_non_empty(value, field)?;
    let bytes = if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        hex::decode(value).with_context(|| format!("{field} must be hex or base64url"))?
    } else {
        let unpadded = value.trim_end_matches('=');
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(unpadded)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(unpadded))
            .with_context(|| format!("{field} must be hex, base64url, or standard base64"))?
    };
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow::anyhow!("{field} must be 32 bytes, got {}", bytes.len()))
}

fn parse_short_id(value: &str, field: &str) -> Result<Vec<u8>> {
    let bytes = hex::decode(require_non_empty(value, field)?)
        .with_context(|| format!("{field} must be hex"))?;
    if bytes.len() > 8 {
        anyhow::bail!("{field} must be at most 8 bytes");
    }
    Ok(bytes)
}

fn require_non_empty<'a>(value: &'a str, field: &str) -> Result<&'a str> {
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    use super::parse_reality_key_32;

    #[test]
    fn reality_keys_accept_hex_and_base64() {
        let key = [0xa5; 32];
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
        assert_eq!(parse_reality_key_32(&hex::encode(key), "key").unwrap(), key);
        assert_eq!(parse_reality_key_32(&encoded, "key").unwrap(), key);
        assert_eq!(
            parse_reality_key_32(&format!("{encoded}="), "key").unwrap(),
            key
        );

        let key_with_standard_alphabet = [0xff; 32];
        let standard = base64::engine::general_purpose::STANDARD.encode(key_with_standard_alphabet);
        assert_eq!(
            parse_reality_key_32(&standard, "key").unwrap(),
            key_with_standard_alphabet
        );
    }
}
