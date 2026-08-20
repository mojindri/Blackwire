//! Typed runtime configuration reconstructed from relational MySQL revisions.
//!
//! The schema is split by responsibility so each file stays small:
//! - `logging_dns` handles logging and DNS/FakeIP settings.
//! - `routing` handles route rules and load balancers.
//! - `endpoint` handles inbound and outbound entries.
//! - `transport` handles TCP/TLS/REALITY/WebSocket/gRPC wrappers.
//! - `protocol` holds shared protocol enums.

mod endpoint;
mod logging_dns;
mod profile;
mod protocol;
mod routing;
mod transport;
mod vision;

pub use endpoint::{
    CongestionSettings, EndpointCount, EndpointSettings, EndpointUser, FallbackSettings,
    InboundConfig, InboundLimitsConfig, OutboundConfig, PoolSettings, QuicSocketOverrides,
};
pub use logging_dns::{DnsConfig, FakeIpConfig, LogConfig};
pub use profile::{
    explain_cost, validate_fast_profile, CopyMode, CostClass, CostReport, FastConfig,
    FastExperimentalBackendPolicy, FastLinuxConfig, FastPoolPolicy, FastRelayConfig,
    FastRelayEngine, FastRelayFlushPolicy, FastSplicePolicy, FastZerocopyPolicy,
    FirstPacketBoostConfig, ProfileMode, ProfileViolation, ProtocolCost,
};
pub use protocol::{NetworkType, Protocol, SecurityType};
pub use routing::{
    AdaptiveBalancerConfig, BalancerConfig, BalancerProfileConfig, HealthCheckConfig,
    RoutingConfig, RoutingRule,
};
pub use transport::{
    DownloadSettings, GrpcConfig, Hysteria2Config, PaddingBounds, PaddingBytes, RealityConfig,
    RealityFallbackLimitConfig, ShadowTlsConfig, SniffingConfig, SplitHttpConfig,
    StreamSettingsConfig, TlsConfig, WsConfig, XmuxConfig,
};
pub use vision::{VisionConfig, VisionDirectCopyPolicy};

use serde::{Deserialize, Serialize};
use validator::Validate;

/// The top-level configuration object.
///
/// The MySQL store reconstructs this snapshot before validation and activation.
/// Every field is optional except `inbounds` and `outbounds`.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Config {
    /// Operating profile. `"compat"` (default) enables all features.
    /// `"fast"` enforces a strict latency-first subset.
    #[serde(default)]
    pub profile: ProfileMode,

    /// Extra settings that apply only when `profile = "fast"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast: Option<FastConfig>,

    /// XTLS Vision optimization policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<VisionConfig>,

    /// First-packet latency acceleration policy.
    #[serde(
        default,
        rename = "firstPacketBoost",
        alias = "first_packet_boost",
        skip_serializing_if = "Option::is_none"
    )]
    pub first_packet_boost: Option<FirstPacketBoostConfig>,

    /// QUIC socket tuning used by QUIC/Hysteria2 endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quic: Option<QuicConfig>,

    /// Logging settings.
    #[serde(default)]
    pub log: LogConfig,

    /// DNS resolver settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsConfig>,

    /// Routing rules for outbound selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingConfig>,

    /// TUN interception settings.
    ///
    /// Linux, macOS, and Windows have active full-device runtimes today.
    /// Windows uses Wintun split routes plus a packet-level TCP bridge to the
    /// local SOCKS listener because Windows does not provide an iptables/PF
    /// equivalent for arbitrary original-destination redirects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tun: Option<TunConfig>,

    /// Runtime safety limits.
    #[serde(default)]
    pub limits: LimitsConfig,

    /// Ports and protocols the proxy listens on.
    // Zero inbounds is a valid idle control-plane state.
    #[validate(nested)]
    pub inbounds: Vec<InboundConfig>,

    /// Protocols used to forward traffic.
    #[validate(length(min = 1, message = "at least one outbound is required"), nested)]
    pub outbounds: Vec<OutboundConfig>,

    /// Statistics collection settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<StatsConfig>,

    /// Metrics/health HTTP server listen address, e.g. `"127.0.0.1:8080"`.
    ///
    /// When set, the proxy starts a Prometheus metrics endpoint at this address.
    #[serde(
        default,
        rename = "metricsAddr",
        alias = "metrics_addr",
        skip_serializing_if = "Option::is_none"
    )]
    pub metrics_addr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
/// Controls whether runtime traffic statistics are collected.
pub struct StatsConfig {
    /// Enables statistics collection.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Runtime safety limits.
///
/// These are intentionally conservative knobs for production hardening.
/// `max_connections` is a process-wide cap shared by TCP and QUIC inbound
/// accept loops that support admission limiting.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ts_rs::TS)]
#[ts(rename_all = "camelCase")]
pub struct LimitsConfig {
    /// Maximum concurrent connections for the whole process (optional).
    #[serde(
        default,
        rename = "maxConnections",
        alias = "max_connections",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_connections: Option<usize>,

    /// Default per-inbound connection cap when an inbound has no own `limits` block.
    #[serde(
        default,
        rename = "maxConnectionsPerInbound",
        alias = "max_connections_per_inbound",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_connections_per_inbound: Option<usize>,

    /// Maximum concurrent authenticated connections per user across all inbounds.
    #[serde(
        default,
        rename = "maxConnectionsPerUser",
        alias = "max_connections_per_user",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_connections_per_user: Option<usize>,

    /// Wall-clock limit for inbound **handshake only** (REALITY/TLS/VLESS header).
    /// Does not cut off an established relay. Omitted = no limit.
    #[serde(
        default,
        rename = "maxHandshakeSeconds",
        alias = "max_handshake_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "number | null")]
    pub max_handshake_seconds: Option<u64>,

    /// Close idle connections after this many seconds (reserved; not wired yet).
    #[serde(
        default,
        rename = "maxIdleSeconds",
        alias = "max_idle_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "number | null")]
    pub max_idle_seconds: Option<u64>,
}

/// QUIC UDP socket tuning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct QuicConfig {
    /// Enable SO_REUSEPORT where supported so multiple server endpoints can bind the same UDP port.
    #[serde(default)]
    pub reuse_port: bool,

    /// Endpoint shard count: integer string/number or "cpu".
    #[serde(default = "QuicConfig::default_endpoints")]
    pub endpoints: EndpointCount,

    /// Requested UDP receive buffer size.
    #[serde(default = "QuicConfig::default_buffer_bytes")]
    pub recv_buffer_bytes: usize,

    /// Requested UDP send buffer size.
    #[serde(default = "QuicConfig::default_buffer_bytes")]
    pub send_buffer_bytes: usize,

    /// Maximum datagram size hint. Current transport accepts the field for config parity.
    #[serde(default = "QuicConfig::default_max_datagram_size")]
    pub max_datagram_size: DatagramSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(untagged)]
/// Maximum QUIC datagram size expressed as bytes or a named policy.
pub enum DatagramSize {
    /// Fixed datagram size in bytes.
    Fixed(usize),
    /// Named size policy such as `"auto"`.
    Named(String),
}

impl QuicConfig {
    fn default_endpoints() -> EndpointCount {
        EndpointCount::Fixed(1)
    }

    fn default_buffer_bytes() -> usize {
        8 * 1024 * 1024
    }

    fn default_max_datagram_size() -> DatagramSize {
        DatagramSize::Named("auto".into())
    }

    /// Returns the number of QUIC endpoints, clamped to 1–64.
    pub fn endpoint_count(&self) -> usize {
        self.endpoints.resolve()
    }
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            reuse_port: false,
            endpoints: Self::default_endpoints(),
            recv_buffer_bytes: Self::default_buffer_bytes(),
            send_buffer_bytes: Self::default_buffer_bytes(),
            max_datagram_size: Self::default_max_datagram_size(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Top-level TUN interception settings.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct TunConfig {
    /// TUN interface name (e.g. `"tun0"`).
    #[serde(default = "default_tun_name")]
    pub name: String,
    /// IPv4 address assigned to the TUN device.
    #[serde(default = "default_tun_address")]
    pub address: String,
    /// Netmask for the TUN IPv4 network.
    #[serde(default = "default_tun_netmask")]
    pub netmask: String,
    /// MTU for the TUN interface.
    #[serde(default = "default_tun_mtu")]
    pub mtu: u16,
    /// Linux packet mark for packets that should bypass the TUN path.
    #[serde(
        default = "default_tun_bypass_mark",
        rename = "bypassMark",
        alias = "bypass_mark"
    )]
    pub bypass_mark: u32,
    /// Physical interface used by protected outbound sockets on macOS/Windows.
    ///
    /// Examples: `"en0"` on macOS or `"Ethernet"` on Windows. macOS requires
    /// this so Blackwire's own outbound sockets can bypass utun capture.
    /// Windows uses it when set; otherwise it falls back to the OS route table
    /// and the configured Wintun split routes.
    #[serde(
        default,
        rename = "outboundInterface",
        alias = "outbound_interface",
        skip_serializing_if = "Option::is_none"
    )]
    pub outbound_interface: Option<String>,
    /// Local port where redirected TCP connections are accepted.
    #[serde(
        default = "default_tun_redirect_port",
        rename = "redirectPort",
        alias = "redirect_port"
    )]
    pub redirect_port: u16,
    /// Local DNS port used by the transparent-proxy DNS path.
    #[serde(
        default = "default_tun_dns_port",
        rename = "dnsPort",
        alias = "dns_port"
    )]
    pub dns_port: u16,
    /// Windows-only path to `wintun.dll`.
    ///
    /// When unset, the Windows backend uses the `tun` crate default
    /// (`wintun.dll` in the process DLL search path).
    #[serde(
        default,
        rename = "wintunFile",
        alias = "wintun_file",
        skip_serializing_if = "Option::is_none"
    )]
    pub wintun_file: Option<String>,
    /// Packet batching controls for TUN writeback.
    #[serde(default, rename = "batch")]
    pub batch: TunBatchConfig,
    /// TUN session/NAT table limits and timeouts.
    #[serde(default, rename = "sessions")]
    pub sessions: TunSessionConfig,
}

/// Packet batching controls for TUN writeback.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(rename_all = "camelCase")]
pub struct TunBatchConfig {
    /// Enable TUN writeback batching.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(
        default = "default_tun_batch_max_packets",
        rename = "maxPackets",
        alias = "max_packets"
    )]
    /// Maximum number of packets to batch before flushing.
    pub max_packets: usize,
    #[serde(
        default = "default_tun_batch_max_delay_us",
        rename = "maxDelayUs",
        alias = "max_delay_us"
    )]
    #[ts(type = "number")]
    /// Maximum time in microseconds to hold a batch before flushing.
    pub max_delay_us: u64,
    #[serde(
        default = "default_tun_batch_latency_flush_bytes",
        rename = "latencyFlushBytes",
        alias = "latency_flush_bytes"
    )]
    /// Flush the batch immediately when buffered bytes exceed this threshold.
    pub latency_flush_bytes: usize,
}

impl Default for TunBatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_packets: default_tun_batch_max_packets(),
            max_delay_us: default_tun_batch_max_delay_us(),
            latency_flush_bytes: default_tun_batch_latency_flush_bytes(),
        }
    }
}

/// TUN session and NAT table sizing.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(rename_all = "camelCase")]
pub struct TunSessionConfig {
    #[serde(
        default = "default_tun_udp_max_sessions",
        rename = "udpMax",
        alias = "udp_max"
    )]
    /// Maximum concurrent UDP NAT sessions.
    pub udp_max: usize,
    #[serde(
        default = "default_tun_udp_idle_timeout_secs",
        rename = "udpIdleTimeoutSec",
        alias = "udp_idle_timeout_sec"
    )]
    #[ts(type = "number")]
    /// Idle timeout in seconds before a UDP session is evicted.
    pub udp_idle_timeout_sec: u64,
    #[serde(
        default = "default_tun_tcp_max_sessions",
        rename = "tcpMax",
        alias = "tcp_max"
    )]
    /// Maximum concurrent TCP proxy sessions.
    pub tcp_max: usize,
}

impl Default for TunSessionConfig {
    fn default() -> Self {
        Self {
            udp_max: default_tun_udp_max_sessions(),
            udp_idle_timeout_sec: default_tun_udp_idle_timeout_secs(),
            tcp_max: default_tun_tcp_max_sessions(),
        }
    }
}

fn default_tun_name() -> String {
    "blackwire-tun".to_string()
}

fn default_tun_address() -> String {
    "198.18.0.1".to_string()
}

fn default_tun_netmask() -> String {
    "255.255.0.0".to_string()
}

fn default_tun_mtu() -> u16 {
    1500
}

fn default_tun_bypass_mark() -> u32 {
    0x1234
}

fn default_tun_redirect_port() -> u16 {
    7890
}

fn default_tun_dns_port() -> u16 {
    5300
}

fn default_tun_batch_max_packets() -> usize {
    32
}

fn default_tun_batch_max_delay_us() -> u64 {
    750
}

fn default_tun_batch_latency_flush_bytes() -> usize {
    256
}

fn default_tun_udp_max_sessions() -> usize {
    4096
}

fn default_tun_udp_idle_timeout_secs() -> u64 {
    60
}

fn default_tun_tcp_max_sessions() -> usize {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_config_deserialises() {
        let json = r#"{
            "inbounds": [{
                "tag": "socks",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 1080
            }],
            "outbounds": [{
                "tag": "direct",
                "protocol": "freedom"
            }]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.inbounds.len(), 1);
        assert_eq!(cfg.outbounds.len(), 1);
        assert_eq!(cfg.inbounds[0].tag, "socks");
        assert_eq!(cfg.outbounds[0].tag, "direct");
    }

    #[test]
    fn vision_policy_deserialises() {
        let json = r#"{
            "vision": {
                "directCopy": "disabled",
                "maxPacketsToFilter": 4,
                "allowSpliceAfterDirect": false
            },
            "inbounds": [{
                "tag": "socks",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 1080
            }],
            "outbounds": [{
                "tag": "direct",
                "protocol": "freedom"
            }]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        let vision = cfg.vision.unwrap();
        assert_eq!(vision.direct_copy, VisionDirectCopyPolicy::Disabled);
        assert_eq!(vision.max_packets_to_filter, 4);
        assert!(!vision.allow_splice_after_direct);
    }

    #[test]
    fn tun_platform_fields_accept_camel_and_snake_case() {
        let camel: TunConfig = serde_json::from_str(
            r#"{
                "outboundInterface": "en0",
                "wintunFile": "C:\\Program Files\\Blackwire\\wintun.dll"
            }"#,
        )
        .unwrap();
        assert_eq!(camel.outbound_interface.as_deref(), Some("en0"));
        assert_eq!(
            camel.wintun_file.as_deref(),
            Some(r#"C:\Program Files\Blackwire\wintun.dll"#)
        );
        assert!(camel.batch.enabled);
        assert_eq!(camel.batch.max_packets, 32);
        assert_eq!(camel.batch.latency_flush_bytes, 256);
        assert_eq!(camel.sessions.udp_max, 4096);
        let snake: TunConfig = serde_json::from_str(
            r#"{
                "outbound_interface": "Ethernet",
                "wintun_file": ".\\wintun.dll",
                "bypass_mark": 77,
                "redirect_port": 12346,
                "dns_port": 5353,
                "batch": {
                    "enabled": false,
                    "max_packets": 16,
                    "max_delay_us": 500,
                    "latency_flush_bytes": 128
                },
                "sessions": {
                    "udp_max": 128,
                    "udp_idle_timeout_sec": 30,
                    "tcp_max": 256
                }
            }"#,
        )
        .unwrap();
        assert_eq!(snake.outbound_interface.as_deref(), Some("Ethernet"));
        assert_eq!(snake.wintun_file.as_deref(), Some(r#".\wintun.dll"#));
        assert_eq!(snake.bypass_mark, 77);
        assert_eq!(snake.redirect_port, 12346);
        assert_eq!(snake.dns_port, 5353);
        assert!(!snake.batch.enabled);
        assert_eq!(snake.batch.max_packets, 16);
        assert_eq!(snake.batch.max_delay_us, 500);
        assert_eq!(snake.batch.latency_flush_bytes, 128);
        assert_eq!(snake.sessions.udp_max, 128);
        assert_eq!(snake.sessions.udp_idle_timeout_sec, 30);
        assert_eq!(snake.sessions.tcp_max, 256);
    }

    #[test]
    fn invalid_port_fails_validation() {
        let json = r#"{
            "inbounds": [{
                "tag": "bad",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 0
            }],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn reality_inbound_requires_socket_destination_at_config_validation() {
        let json = r#"{
            "inbounds": [{
                "tag": "reality",
                "protocol": "vless",
                "listen": "127.0.0.1",
                "port": 443,
                "streamSettings": {
                    "network": "tcp",
                    "security": "reality",
                    "realitySettings": {
                        "privateKey": "769aa4a053f2c8af7a27bb1d79fc0067f39b6c1ce6743543bb3f7584aa68223c",
                        "shortIds": ["feedbeef"],
                        "serverNames": ["www.microsoft.com"]
                    }
                }
            }],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("realitySettings.dest"));
    }

    #[test]
    fn reality_inbound_rejects_domain_destination_at_config_validation() {
        let json = r#"{
            "inbounds": [{
                "tag": "reality",
                "protocol": "vless",
                "listen": "127.0.0.1",
                "port": 443,
                "streamSettings": {
                    "network": "tcp",
                    "security": "reality",
                    "realitySettings": {
                        "dest": "www.microsoft.com:443",
                        "privateKey": "769aa4a053f2c8af7a27bb1d79fc0067f39b6c1ce6743543bb3f7584aa68223c",
                        "shortIds": ["feedbeef"],
                        "serverNames": ["www.microsoft.com"]
                    }
                }
            }],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        let err = cfg.validate().unwrap_err();
        assert!(format!("{err:?}").contains("socket_address"));
    }

    #[test]
    fn empty_inbounds_is_valid_idle_control_plane() {
        let json = r#"{
            "inbounds": [],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn log_defaults_applied() {
        let json = r#"{
            "inbounds": [{"tag":"i","protocol":"socks","listen":"127.0.0.1","port":1080}],
            "outbounds": [{"tag":"o","protocol":"freedom"}]
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.log.level, "info");
        assert!(!cfg.log.json);
    }

    #[test]
    fn network_and_security_type_deserialise() {
        let json = r#"{"network": "ws", "security": "reality"}"#;
        let s: StreamSettingsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(s.network, NetworkType::Ws);
        assert_eq!(s.security, SecurityType::Reality);
    }

    #[test]
    fn xhttp_network_alias_deserialises_as_splithttp() {
        let json = r#"{"network": "xhttp"}"#;
        let s: StreamSettingsConfig = serde_json::from_str(json).unwrap();
        assert_eq!(s.network, NetworkType::SplitHttp);
    }

    #[test]
    fn splithttp_xhttp_extras_deserialise() {
        let json = r#"{
            "network": "splithttp",
            "splithttpSettings": {
                "path": "/split",
                "mode": "packet-up",
                "xPaddingBytes": "16-32",
                "xPaddingMethod": "repeat-x",
                "xPaddingHeader": "X-Test-Padding",
                "scMaxBufferedPosts": 12,
                "xmux": { "maxConcurrency": 4 },
                "downloadSettings": { "network": "tcp" }
            }
        }"#;
        let s: StreamSettingsConfig = serde_json::from_str(json).unwrap();
        let cfg = s.splithttp_settings.expect("splithttp settings");
        assert_eq!(cfg.mode, "packet-up");
        assert_eq!(cfg.x_padding_method, "repeat-x");
        assert_eq!(cfg.x_padding_header, "X-Test-Padding");
        assert_eq!(cfg.sc_max_buffered_posts, 12);
        assert!(cfg.xmux.is_some());
        assert!(cfg.download_settings.is_some());
    }

    #[test]
    fn quic_socket_tuning_deserialises() {
        let json = r#"{
            "quic": {
                "reusePort": true,
                "endpoints": "cpu",
                "recvBufferBytes": 8388608,
                "sendBufferBytes": 8388608,
                "maxDatagramSize": "auto"
            },
            "inbounds": [{
                "tag": "socks",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 1080
            }],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        let quic = cfg.quic.expect("quic config");
        assert!(quic.reuse_port);
        assert!(quic.endpoint_count() >= 1);
        assert_eq!(quic.recv_buffer_bytes, 8 * 1024 * 1024);
        assert_eq!(quic.send_buffer_bytes, 8 * 1024 * 1024);
    }

    /// `protocol: shadowtls` on an inbound must be rejected with a clear error
    /// pointing users to `security: shadowtls` instead.
    #[test]
    fn shadowtls_as_inbound_protocol_is_rejected() {
        let json = r#"{
            "inbounds": [{
                "tag": "bad",
                "protocol": "shadowtls",
                "listen": "127.0.0.1",
                "port": 8443
            }],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        let err = cfg
            .validate()
            .expect_err("shadowtls inbound should fail validation");
        let msg = err.to_string();
        assert!(
            msg.contains("shadowtls") || msg.contains("streamSettings"),
            "expected a message referencing shadowtls or streamSettings, got: {msg}"
        );
    }

    /// `protocol: shadowtls` on an outbound must be rejected with a clear error.
    #[test]
    fn shadowtls_as_outbound_protocol_is_rejected() {
        let json = r#"{
            "inbounds": [{
                "tag": "socks",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 1080
            }],
            "outbounds": [{"tag": "bad", "protocol": "shadowtls"}]
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        let err = cfg
            .validate()
            .expect_err("shadowtls outbound should fail validation");
        let msg = err.to_string();
        assert!(
            msg.contains("shadowtls") || msg.contains("streamSettings"),
            "expected a message referencing shadowtls or streamSettings, got: {msg}"
        );
    }
}
