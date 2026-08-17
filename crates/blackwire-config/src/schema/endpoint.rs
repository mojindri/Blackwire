use std::net::{IpAddr, SocketAddr};

use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError, ValidationErrors};

use super::{Protocol, SecurityType, SniffingConfig, StreamSettingsConfig};

fn reject_shadowtls_protocol(protocol: &Protocol) -> Result<(), ValidationError> {
    if *protocol == Protocol::ShadowTls {
        let mut error = ValidationError::new("unsupported_protocol");
        error.message = Some(
            "protocol 'shadowtls' is not a standalone proxy protocol; \
             use 'security: shadowtls' in streamSettings on a VLESS, Trojan, or VMess endpoint"
                .into(),
        );
        return Err(error);
    }
    Ok(())
}

/// An inbound handler: a port and protocol the proxy listens on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundConfig {
    /// Unique name used in routing rules and logs.
    pub tag: String,

    /// Proxy protocol: "socks", "http", "vless", and so on.
    pub protocol: Protocol,

    /// IP address to listen on.
    pub listen: IpAddr,

    /// Port to listen on. Must be between 1 and 65535.
    pub port: u16,

    /// Typed protocol-specific settings.
    #[serde(default)]
    pub settings: EndpointSettings,

    /// Transport settings: TLS, WebSocket, REALITY, etc.
    #[serde(
        default,
        rename = "streamSettings",
        alias = "stream_settings",
        skip_serializing_if = "Option::is_none"
    )]
    pub stream_settings: Option<StreamSettingsConfig>,

    /// Per-inbound runtime safety limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<InboundLimitsConfig>,

    /// Sniffing settings for detecting inner protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sniffing: Option<SniffingConfig>,
}

impl Validate for InboundConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if let Err(error) = reject_shadowtls_protocol(&self.protocol) {
            errors.add("protocol", error);
        }

        if self.port == 0 {
            let mut error = ValidationError::new("range");
            error.message = Some("inbound port must be between 1 and 65535".into());
            errors.add("port", error);
        }

        if let Some(stream) = &self.stream_settings {
            if stream.security == SecurityType::Reality {
                match stream.reality_settings.as_ref() {
                    Some(reality) if reality.dest.trim().is_empty() => {
                        let mut error = ValidationError::new("required");
                        error.message =
                            Some("REALITY inbound requires realitySettings.dest".into());
                        errors.add("streamSettings.realitySettings.dest", error);
                    }
                    Some(reality) if reality.dest.parse::<SocketAddr>().is_err() => {
                        let mut error = ValidationError::new("socket_address");
                        error.message = Some(
                            "REALITY inbound realitySettings.dest must be an IP socket address like 93.184.216.34:443".into(),
                        );
                        errors.add("streamSettings.realitySettings.dest", error);
                    }
                    Some(_) => {}
                    None => {
                        let mut error = ValidationError::new("required");
                        error.message =
                            Some("REALITY inbound requires streamSettings.realitySettings".into());
                        errors.add("streamSettings.realitySettings", error);
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// An outbound handler: a protocol used to forward traffic to the destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundConfig {
    /// Unique name referenced by routing rules.
    pub tag: String,

    /// Proxy protocol: "freedom", "vless", "vmess", and so on.
    pub protocol: Protocol,

    /// Typed protocol-specific settings.
    #[serde(default)]
    pub settings: EndpointSettings,

    /// Transport settings: TLS, WebSocket, REALITY, etc.
    #[serde(
        default,
        rename = "streamSettings",
        alias = "stream_settings",
        skip_serializing_if = "Option::is_none"
    )]
    pub stream_settings: Option<StreamSettingsConfig>,
}

impl Validate for OutboundConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if self.protocol == Protocol::ShadowTls {
            let mut error = ValidationError::new("unsupported_protocol");
            error.message = Some(
                "protocol 'shadowtls' is not a standalone proxy protocol; \
                 use 'security: shadowtls' in streamSettings on a VLESS, Trojan, or VMess endpoint"
                    .into(),
            );
            errors.add("protocol", error);
        }

        if self.settings.port == Some(0) {
            let mut error = ValidationError::new("range");
            error.message = Some("outbound settings.port must be between 1 and 65535".into());
            errors.add("settings.port", error);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointSettings {
    pub address: Option<String>,
    pub port: Option<u16>,
    pub server: Option<String>,
    #[serde(alias = "id")]
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub auth: Option<String>,
    pub method: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub flow: String,
    #[serde(default)]
    pub clients: Vec<EndpointUser>,
    #[serde(default)]
    pub users: Vec<EndpointUser>,
    pub decryption: Option<String>,
    pub fallback: Option<FallbackSettings>,
    pub network: Option<String>,
    pub server_name: Option<String>,
    #[serde(default, alias = "allowInsecure", alias = "insecure")]
    pub skip_cert_verify: bool,
    pub endpoint_shards: Option<usize>,
    pub auth_timeout_ms: Option<u64>,
    pub up_mbps: Option<u64>,
    pub down_mbps: Option<u64>,
    pub domain_strategy: Option<String>,
    pub ip_strategy: Option<String>,
    #[serde(default)]
    pub deny_loopback: bool,
    #[serde(default)]
    pub reject_ipv6_literal: bool,
    pub pool: Option<PoolSettings>,
    pub pool_enabled: Option<bool>,
    pub congestion: Option<CongestionSettings>,
    pub quic: Option<QuicSocketOverrides>,
    pub datagram: Option<DatagramOverrides>,
    pub fec: Option<FecOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointUser {
    #[serde(alias = "uuid")]
    pub id: Option<String>,
    pub password: Option<String>,
    pub auth: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub flow: String,
    pub up_mbps: Option<u64>,
    pub down_mbps: Option<u64>,
}

impl EndpointUser {
    pub fn identifier(&self) -> Option<&str> {
        self.id.as_deref()
    }
    pub fn label(&self) -> Option<&str> {
        self.email
            .as_deref()
            .or(self.name.as_deref())
            .filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FallbackSettings {
    pub dest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolSettings {
    #[serde(default = "default_pool_mode")]
    pub mode: String,
    pub max_per_dest: Option<usize>,
    pub max_global_idle: Option<usize>,
    pub max_dests: Option<usize>,
    pub idle_ttl_ms: Option<u64>,
    pub hotness_window_ms: Option<u64>,
    pub min_hotness_for_pool: Option<u64>,
}
fn default_pool_mode() -> String {
    "adaptive".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CongestionSettings {
    #[serde(default = "default_congestion_mode")]
    pub mode: String,
    pub min_ack_rate: Option<f64>,
    pub max_queue_delay_ms: Option<u64>,
    pub pacing_gain: Option<f64>,
    pub loss_compensation: Option<bool>,
}
fn default_congestion_mode() -> String {
    "standard".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuicSocketOverrides {
    pub reuse_port: Option<bool>,
    pub endpoints: Option<EndpointCount>,
    pub recv_buffer_bytes: Option<usize>,
    pub send_buffer_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum EndpointCount {
    Fixed(usize),
    Named(String),
}
impl EndpointCount {
    pub fn resolve(&self) -> usize {
        match self {
            Self::Fixed(value) => *value,
            Self::Named(value) if value.eq_ignore_ascii_case("cpu") => {
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1)
            }
            Self::Named(value) => value.parse().unwrap_or(1),
        }
        .clamp(1, 64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatagramOverrides {
    pub enabled: Option<bool>,
    pub udp_over_datagram: Option<bool>,
    pub policy: Option<String>,
    pub max_queue_delay_ms: Option<u64>,
    pub fast_dns_retry: Option<bool>,
    pub fast_dns_retry_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FecOverrides {
    pub mode: Option<String>,
    pub max_overhead_percent: Option<u8>,
    pub avoid_bulk_tcp: Option<bool>,
    pub disable_for_sequential_dns: Option<bool>,
    pub min_concurrency_for_block_fec: Option<usize>,
    pub max_generation_packets: Option<u8>,
    pub max_generation_delay_ms: Option<u64>,
    pub recovery_deadline_ms: Option<u64>,
    pub dedup_window_packets: Option<usize>,
}

/// Per-inbound runtime safety limits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InboundLimitsConfig {
    /// Max concurrent connections on this inbound only (overrides global default).
    #[serde(
        default,
        rename = "maxConnections",
        alias = "max_connections",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_connections: Option<usize>,

    /// Handshake timeout for this inbound (seconds). Overrides global `limits.maxHandshakeSeconds`.
    /// Applies to REALITY/TLS/VLESS header phases only — not the relay body.
    #[serde(
        default,
        rename = "maxHandshakeSeconds",
        alias = "max_handshake_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_handshake_seconds: Option<u64>,

    /// Idle timeout for this inbound (reserved; not wired yet).
    #[serde(
        default,
        rename = "maxIdleSeconds",
        alias = "max_idle_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_idle_seconds: Option<u64>,
}
