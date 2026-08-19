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
/// Typed protocol-specific settings shared by inbound and outbound endpoints.
pub struct EndpointSettings {
    /// Remote address used by outbound endpoints.
    pub address: Option<String>,
    /// Remote port used by outbound endpoints.
    pub port: Option<u16>,
    /// Alternate remote server address used by supported protocols.
    pub server: Option<String>,
    /// Endpoint-level UUID or user identifier.
    #[serde(alias = "id")]
    pub uuid: Option<String>,
    /// Endpoint-level password or shared secret.
    pub password: Option<String>,
    /// Endpoint-level authentication token.
    pub auth: Option<String>,
    /// Cipher or authentication method name.
    pub method: Option<String>,
    /// Optional operator-facing email label.
    pub email: Option<String>,
    /// Optional operator-facing name.
    pub name: Option<String>,
    /// Protocol flow mode, such as VLESS Vision.
    #[serde(default)]
    pub flow: String,
    /// Protocol client accounts.
    #[serde(default)]
    pub clients: Vec<EndpointUser>,
    /// Protocol user accounts for schemas that use the `users` key.
    #[serde(default)]
    pub users: Vec<EndpointUser>,
    /// VLESS decryption policy.
    pub decryption: Option<String>,
    /// Optional fallback destination.
    pub fallback: Option<FallbackSettings>,
    /// Protocol-specific network selection.
    pub network: Option<String>,
    /// TLS server name used by outbound endpoints.
    pub server_name: Option<String>,
    /// Disables remote certificate verification when explicitly enabled.
    #[serde(default, alias = "allowInsecure", alias = "insecure")]
    pub skip_cert_verify: bool,
    /// Number of transport endpoint shards.
    pub endpoint_shards: Option<usize>,
    /// Authentication timeout in milliseconds.
    pub auth_timeout_ms: Option<u64>,
    /// Upload bandwidth policy in megabits per second.
    pub up_mbps: Option<u64>,
    /// Download bandwidth policy in megabits per second.
    pub down_mbps: Option<u64>,
    /// Domain-resolution strategy.
    pub domain_strategy: Option<String>,
    /// IP-selection strategy.
    pub ip_strategy: Option<String>,
    /// Rejects loopback destinations when enabled.
    #[serde(default)]
    pub deny_loopback: bool,
    /// Rejects literal IPv6 destinations when enabled.
    #[serde(default)]
    pub reject_ipv6_literal: bool,
    /// Adaptive connection-pool settings.
    pub pool: Option<PoolSettings>,
    /// Explicit connection-pool enablement override.
    pub pool_enabled: Option<bool>,
    /// Congestion-control tuning.
    pub congestion: Option<CongestionSettings>,
    /// Per-endpoint QUIC socket overrides.
    pub quic: Option<QuicSocketOverrides>,
    /// Per-endpoint datagram overrides.
    pub datagram: Option<DatagramOverrides>,
    /// Per-endpoint forward-error-correction overrides.
    pub fec: Option<FecOverrides>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Authentication and traffic-policy fields for one endpoint user.
pub struct EndpointUser {
    /// UUID or other protocol user identifier.
    #[serde(alias = "uuid")]
    pub id: Option<String>,
    /// User password or shared secret.
    pub password: Option<String>,
    /// User authentication token.
    pub auth: Option<String>,
    /// Optional email label.
    pub email: Option<String>,
    /// Optional display name.
    pub name: Option<String>,
    /// Protocol flow mode assigned to this user.
    #[serde(default)]
    pub flow: String,
    /// Upload limit in megabits per second.
    pub up_mbps: Option<u64>,
    /// Download limit in megabits per second.
    pub down_mbps: Option<u64>,
}

impl EndpointUser {
    /// Returns the configured user identifier.
    pub fn identifier(&self) -> Option<&str> {
        self.id.as_deref()
    }
    /// Returns the first non-empty operator-facing user label.
    pub fn label(&self) -> Option<&str> {
        self.email
            .as_deref()
            .or(self.name.as_deref())
            .filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Fallback destination used when an inbound cannot handle a connection.
pub struct FallbackSettings {
    /// Destination address or socket.
    pub dest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Adaptive outbound connection-pool tuning.
pub struct PoolSettings {
    /// Pool strategy name.
    #[serde(default = "default_pool_mode")]
    pub mode: String,
    /// Maximum idle connections retained per destination.
    pub max_per_dest: Option<usize>,
    /// Maximum idle connections retained globally.
    pub max_global_idle: Option<usize>,
    /// Maximum number of destinations tracked by the pool.
    pub max_dests: Option<usize>,
    /// Idle connection lifetime in milliseconds.
    pub idle_ttl_ms: Option<u64>,
    /// Activity window used to determine destination hotness.
    pub hotness_window_ms: Option<u64>,
    /// Minimum activity required before pooling a destination.
    pub min_hotness_for_pool: Option<u64>,
}
fn default_pool_mode() -> String {
    "adaptive".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Congestion-control policy overrides for a transport endpoint.
pub struct CongestionSettings {
    /// Congestion-control mode name.
    #[serde(default = "default_congestion_mode")]
    pub mode: String,
    /// Minimum observed acknowledgement rate.
    pub min_ack_rate: Option<f64>,
    /// Maximum tolerated queue delay in milliseconds.
    pub max_queue_delay_ms: Option<u64>,
    /// Pacing multiplier applied by the controller.
    pub pacing_gain: Option<f64>,
    /// Enables loss compensation when supported.
    pub loss_compensation: Option<bool>,
}
fn default_congestion_mode() -> String {
    "standard".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Endpoint-specific QUIC socket tuning.
pub struct QuicSocketOverrides {
    /// Enables address reuse where supported.
    pub reuse_port: Option<bool>,
    /// Number of endpoint shards.
    pub endpoints: Option<EndpointCount>,
    /// Requested receive buffer size in bytes.
    pub recv_buffer_bytes: Option<usize>,
    /// Requested send buffer size in bytes.
    pub send_buffer_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ts_rs::TS)]
#[serde(untagged)]
/// Endpoint shard count expressed directly or as a named policy.
pub enum EndpointCount {
    /// Fixed shard count.
    Fixed(usize),
    /// Named shard policy such as `"cpu"`.
    Named(String),
}
impl EndpointCount {
    /// Resolves the configured policy to a shard count between 1 and 64.
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
/// Per-endpoint datagram transport overrides.
pub struct DatagramOverrides {
    /// Enables datagram transport.
    pub enabled: Option<bool>,
    /// Enables UDP tunnelling over the datagram transport.
    pub udp_over_datagram: Option<bool>,
    /// Datagram scheduling policy name.
    pub policy: Option<String>,
    /// Maximum queue delay in milliseconds.
    pub max_queue_delay_ms: Option<u64>,
    /// Enables accelerated DNS retry handling.
    pub fast_dns_retry: Option<bool>,
    /// Delay before an accelerated DNS retry, in milliseconds.
    pub fast_dns_retry_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Per-endpoint forward-error-correction overrides.
pub struct FecOverrides {
    /// FEC mode name.
    pub mode: Option<String>,
    /// Maximum parity overhead as a percentage.
    pub max_overhead_percent: Option<u8>,
    /// Disables FEC for bulk TCP-like traffic when enabled.
    pub avoid_bulk_tcp: Option<bool>,
    /// Disables FEC for sequential DNS exchanges when enabled.
    pub disable_for_sequential_dns: Option<bool>,
    /// Minimum concurrency required for block FEC.
    pub min_concurrency_for_block_fec: Option<usize>,
    /// Maximum packets in one FEC generation.
    pub max_generation_packets: Option<u8>,
    /// Maximum time to assemble a generation, in milliseconds.
    pub max_generation_delay_ms: Option<u64>,
    /// Recovery deadline in milliseconds.
    pub recovery_deadline_ms: Option<u64>,
    /// Number of packets retained for duplicate detection.
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
