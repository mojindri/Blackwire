//! Configuration schema — Rust structs that map to the JSON config file.
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

pub use endpoint::{InboundConfig, InboundLimitsConfig, OutboundConfig};
pub use logging_dns::{DnsConfig, FakeIpConfig, LogConfig};
pub use profile::{
    explain_cost, validate_fast_profile, BudgetConfig, CopyMode, CostClass, CostReport, FastConfig,
    FastPoolPolicy, FastRelayConfig, FastRelayEngine, FastRelayFlushPolicy, FastSplicePolicy,
    ProfileMode, ProfileViolation, ProtocolCost,
};
pub use protocol::{NetworkType, Protocol, SecurityType};
pub use routing::{
    AdaptiveBalancerConfig, BalancerConfig, BalancerProfileConfig, HealthCheckConfig,
    RoutingConfig, RoutingRule,
};
pub use transport::{
    GrpcConfig, Hysteria2Config, KcpConfig, RealityConfig, ShadowTlsConfig, SniffingConfig,
    SplitHttpConfig, StreamSettingsConfig, TlsConfig, WsConfig,
};
pub use vision::{VisionConfig, VisionDirectCopyPolicy};

use serde::{Deserialize, Serialize};
use validator::Validate;

/// The top-level configuration object.
///
/// This is what gets deserialised from the JSON config file. Every field is
/// optional except `inbounds` and `outbounds`.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Config {
    /// Operating profile. `"compat"` (default) enables all features.
    /// `"fast"` enforces a strict latency-first subset.
    #[serde(default)]
    pub profile: ProfileMode,

    /// Extra settings that apply only when `profile = "fast"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast: Option<FastConfig>,

    /// Performance budget used by `blackwire explain-cost`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetConfig>,

    /// XTLS Vision optimization policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<VisionConfig>,

    /// QUIC socket tuning used by QUIC/Hysteria2 endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quic: Option<QuicConfig>,

    /// QUIC DATAGRAM lane policy for unreliable traffic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datagram: Option<DatagramConfig>,

    /// Forward error correction policy for lossy/mobile datagram traffic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fec: Option<FecConfig>,

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
    #[validate(length(min = 1, message = "at least one inbound is required"), nested)]
    pub inbounds: Vec<InboundConfig>,

    /// Protocols used to forward traffic.
    #[validate(length(min = 1, message = "at least one outbound is required"), nested)]
    pub outbounds: Vec<OutboundConfig>,

    /// Statistics collection settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<serde_json::Value>,

    /// Management API settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<serde_json::Value>,

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

/// Runtime safety limits.
///
/// These are intentionally conservative knobs for production hardening.
/// `max_connections` is currently applied per TCP listener unless a more
/// specific inbound limit is set. Global cross-listener accounting can be
/// added later without changing the config shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LimitsConfig {
    /// Maximum concurrent connections for the whole process (optional).
    /// Applied per TCP listener unless overridden by per-inbound limits.
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

    /// Wall-clock limit for inbound **handshake only** (REALITY/TLS/VLESS header).
    /// Does not cut off an established relay. Omitted = no limit.
    #[serde(
        default,
        rename = "maxHandshakeSeconds",
        alias = "max_handshake_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_handshake_seconds: Option<u64>,

    /// Close idle connections after this many seconds (reserved; not wired yet).
    #[serde(
        default,
        rename = "maxIdleSeconds",
        alias = "max_idle_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_idle_seconds: Option<u64>,
}

/// QUIC UDP socket tuning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuicConfig {
    /// Enable SO_REUSEPORT where supported so multiple server endpoints can bind the same UDP port.
    #[serde(default)]
    pub reuse_port: bool,

    /// Endpoint shard count: integer string/number or "cpu".
    #[serde(default = "QuicConfig::default_endpoints")]
    pub endpoints: serde_json::Value,

    /// Requested UDP receive buffer size.
    #[serde(default = "QuicConfig::default_buffer_bytes")]
    pub recv_buffer_bytes: usize,

    /// Requested UDP send buffer size.
    #[serde(default = "QuicConfig::default_buffer_bytes")]
    pub send_buffer_bytes: usize,

    /// Maximum datagram size hint. Current transport accepts the field for config parity.
    #[serde(default = "QuicConfig::default_max_datagram_size")]
    pub max_datagram_size: serde_json::Value,
}

impl QuicConfig {
    fn default_endpoints() -> serde_json::Value {
        serde_json::Value::String("1".into())
    }

    fn default_buffer_bytes() -> usize {
        8 * 1024 * 1024
    }

    fn default_max_datagram_size() -> serde_json::Value {
        serde_json::Value::String("auto".into())
    }

    pub fn endpoint_count(&self) -> usize {
        match &self.endpoints {
            serde_json::Value::String(s) if s.eq_ignore_ascii_case("cpu") => {
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1)
            }
            serde_json::Value::String(s) => s.parse::<usize>().unwrap_or(1),
            serde_json::Value::Number(n) => n.as_u64().unwrap_or(1) as usize,
            _ => 1,
        }
        .clamp(1, 64)
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

/// QUIC DATAGRAM lane policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatagramConfig {
    /// Enable QUIC DATAGRAM support for unreliable traffic.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Send UDP relay payloads over QUIC DATAGRAM instead of reliable streams.
    #[serde(default = "default_true")]
    pub udp_over_datagram: bool,

    /// Reserved for TUN packet DATAGRAM mode.
    #[serde(default = "default_true")]
    pub tun_packets_over_datagram: bool,

    /// H2+ lane policy (standard = unchanged behavior, h2-plus = priority lane + DNS retry knobs).
    #[serde(default)]
    pub policy: DatagramPolicy,

    /// H2+ queue delay budget for delayed non-priority packets.
    #[serde(default = "DatagramConfig::default_max_queue_delay_ms")]
    pub max_queue_delay_ms: u64,

    /// Enable DNS shadow retry in H2+ mode.
    #[serde(default)]
    pub fast_dns_retry: bool,

    /// DNS shadow retry delay in H2+ mode.
    #[serde(default = "DatagramConfig::default_fast_dns_retry_delay_ms")]
    pub fast_dns_retry_delay_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DatagramPolicy {
    Standard,
    H2Plus,
}

impl Default for DatagramPolicy {
    fn default() -> Self {
        Self::Standard
    }
}

impl DatagramConfig {
    fn default_max_queue_delay_ms() -> u64 {
        25
    }

    fn default_fast_dns_retry_delay_ms() -> u64 {
        20
    }
}

impl Default for DatagramConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            udp_over_datagram: true,
            tun_packets_over_datagram: true,
            policy: DatagramPolicy::Standard,
            max_queue_delay_ms: Self::default_max_queue_delay_ms(),
            fast_dns_retry: false,
            fast_dns_retry_delay_ms: Self::default_fast_dns_retry_delay_ms(),
        }
    }
}

/// Forward error correction mode for QUIC DATAGRAM traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FecMode {
    #[default]
    Off,
    Xor1OfN,
    ReedSolomon,
    RaptorLike,
    Auto,
}

/// FEC policy for unreliable datagram classes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FecConfig {
    #[serde(default)]
    pub mode: FecMode,

    #[serde(default = "FecConfig::default_max_overhead_percent")]
    pub max_overhead_percent: u8,

    #[serde(default = "FecConfig::default_protect_classes")]
    pub protect_classes: Vec<String>,

    #[serde(default = "default_true")]
    pub avoid_bulk_tcp: bool,
}

impl FecConfig {
    fn default_max_overhead_percent() -> u8 {
        20
    }

    fn default_protect_classes() -> Vec<String> {
        vec!["dns".into(), "interactive".into(), "control".into()]
    }

    pub fn effective_mode(&self) -> FecMode {
        match self.mode {
            FecMode::Auto if self.max_overhead_percent >= 20 => FecMode::Xor1OfN,
            FecMode::Auto => FecMode::Off,
            mode => mode,
        }
    }

    pub fn mode_for_loss(&self, loss_percent: f64, packet_class: &str, bulk_tcp: bool) -> FecMode {
        if bulk_tcp && self.avoid_bulk_tcp {
            return FecMode::Off;
        }
        if !self
            .protect_classes
            .iter()
            .any(|class| class.eq_ignore_ascii_case(packet_class))
        {
            return FecMode::Off;
        }
        if self.mode != FecMode::Auto {
            return self.effective_mode();
        }
        if loss_percent < 1.0 || self.max_overhead_percent < 20 {
            FecMode::Off
        } else if loss_percent < 3.0 {
            FecMode::Xor1OfN
        } else if loss_percent <= 8.0 {
            FecMode::ReedSolomon
        } else {
            FecMode::RaptorLike
        }
    }
}

impl Default for FecConfig {
    fn default() -> Self {
        Self {
            mode: FecMode::Off,
            max_overhead_percent: Self::default_max_overhead_percent(),
            protect_classes: Self::default_protect_classes(),
            avoid_bulk_tcp: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Top-level TUN interception settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_tun_bypass_mark")]
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
    #[serde(default = "default_tun_redirect_port")]
    pub redirect_port: u16,
    /// Local DNS port used by the transparent-proxy DNS path.
    #[serde(default = "default_tun_dns_port")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunBatchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(
        default = "default_tun_batch_max_packets",
        rename = "maxPackets",
        alias = "max_packets"
    )]
    pub max_packets: usize,
    #[serde(
        default = "default_tun_batch_max_delay_us",
        rename = "maxDelayUs",
        alias = "max_delay_us"
    )]
    pub max_delay_us: u64,
    #[serde(
        default = "default_tun_batch_latency_flush_bytes",
        rename = "latencyFlushBytes",
        alias = "latency_flush_bytes"
    )]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunSessionConfig {
    #[serde(
        default = "default_tun_udp_max_sessions",
        rename = "udpMax",
        alias = "udp_max"
    )]
    pub udp_max: usize,
    #[serde(
        default = "default_tun_udp_idle_timeout_secs",
        rename = "udpIdleTimeoutSec",
        alias = "udp_idle_timeout_sec"
    )]
    pub udp_idle_timeout_sec: u64,
    #[serde(
        default = "default_tun_tcp_max_sessions",
        rename = "tcpMax",
        alias = "tcp_max"
    )]
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
    fn mkcp_header_accepts_xray_object_form() {
        let json = r#"{
            "header": { "type": "none" },
            "tti": 10
        }"#;
        let kcp: super::transport::KcpConfig = serde_json::from_str(json).unwrap();
        assert_eq!(kcp.header, "none");
    }

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
    fn empty_inbounds_fails_validation() {
        let json = r#"{
            "inbounds": [],
            "outbounds": [{"tag": "d", "protocol": "freedom"}]
        }"#;

        let cfg: Config = serde_json::from_str(json).unwrap();
        assert!(cfg.validate().is_err());
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

    #[test]
    fn datagram_and_fec_policy_deserialise() {
        let json = r#"{
            "datagram": {
                "enabled": true,
                "udpOverDatagram": true,
                "tunPacketsOverDatagram": true
            },
            "fec": {
                "mode": "auto",
                "maxOverheadPercent": 20,
                "protectClasses": ["dns", "interactive", "control"],
                "avoidBulkTcp": true
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
        let datagram = cfg.datagram.expect("datagram config");
        assert!(datagram.enabled);
        assert!(datagram.udp_over_datagram);
        let fec = cfg.fec.expect("fec config");
        assert_eq!(fec.mode, FecMode::Auto);
        assert_eq!(fec.effective_mode(), FecMode::Xor1OfN);
        assert_eq!(fec.max_overhead_percent, 20);
        assert!(fec.avoid_bulk_tcp);
    }

    #[test]
    fn fec_auto_policy_tracks_loss_and_packet_class() {
        let fec = FecConfig {
            mode: FecMode::Auto,
            ..FecConfig::default()
        };
        assert_eq!(fec.mode_for_loss(0.5, "dns", false), FecMode::Off);
        assert_eq!(fec.mode_for_loss(2.0, "dns", false), FecMode::Xor1OfN);
        assert_eq!(
            fec.mode_for_loss(5.0, "interactive", false),
            FecMode::ReedSolomon
        );
        assert_eq!(
            fec.mode_for_loss(10.0, "control", false),
            FecMode::RaptorLike
        );
        assert_eq!(fec.mode_for_loss(5.0, "bulk", false), FecMode::Off);
        assert_eq!(fec.mode_for_loss(5.0, "dns", true), FecMode::Off);
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
