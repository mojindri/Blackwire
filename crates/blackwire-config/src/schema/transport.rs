use serde::{Deserialize, Serialize};

use super::{NetworkType, SecurityType};

/// Transport layer settings: how to wrap or protect the connection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamSettingsConfig {
    /// Transport to use: "tcp", "ws", "grpc", "quic", or "splithttp".
    #[serde(default)]
    pub network: NetworkType,

    /// Whether to use TLS, REALITY, or no security wrapper.
    #[serde(default)]
    pub security: SecurityType,

    /// TLS-specific settings.
    #[serde(
        default,
        rename = "tlsSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub tls_settings: Option<TlsConfig>,

    /// REALITY-specific settings.
    #[serde(
        default,
        rename = "realitySettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub reality_settings: Option<RealityConfig>,

    /// WebSocket-specific settings.
    #[serde(
        default,
        rename = "wsSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub ws_settings: Option<WsConfig>,

    /// HTTPUpgrade-specific settings (same shape as WebSocket path/headers).
    #[serde(
        default,
        rename = "httpupgradeSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub httpupgrade_settings: Option<WsConfig>,

    /// SplitHTTP / xHTTP settings.
    #[serde(
        default,
        rename = "splithttpSettings",
        alias = "xhttpSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub splithttp_settings: Option<SplitHttpConfig>,

    /// gRPC-specific settings.
    #[serde(
        default,
        rename = "grpcSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub grpc_settings: Option<GrpcConfig>,

    /// ShadowTLS-specific settings.
    #[serde(
        default,
        rename = "shadowTlsSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub shadow_tls_settings: Option<ShadowTlsConfig>,
}

/// TLS configuration used when `security = "tls"`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsConfig {
    /// Server name (SNI) to present during the TLS handshake.
    #[serde(
        default,
        rename = "serverName",
        skip_serializing_if = "String::is_empty"
    )]
    pub server_name: String,

    /// Skip certificate verification. Use only for development.
    #[serde(default, rename = "allowInsecure")]
    pub allow_insecure: bool,

    /// ALPN protocols to offer.
    #[serde(default)]
    pub alpn: Vec<String>,

    /// Path to the TLS certificate file. Server-side only.
    #[serde(
        default,
        rename = "certificateFile",
        skip_serializing_if = "String::is_empty"
    )]
    pub certificate_file: String,

    /// Path to the TLS private key file. Server-side only.
    #[serde(default, rename = "keyFile", skip_serializing_if = "String::is_empty")]
    pub key_file: String,
}

/// REALITY configuration for TLS camouflage.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RealityConfig {
    /// Whether this is a server config.
    #[serde(default)]
    pub show: bool,

    /// Real destination used when authentication fails.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dest: String,

    /// Server's X25519 private key. Server-side only.
    #[serde(
        default,
        rename = "privateKey",
        skip_serializing_if = "String::is_empty"
    )]
    pub private_key: String,

    /// Short IDs clients may use to authenticate.
    #[serde(default, rename = "shortIds")]
    pub short_ids: Vec<String>,

    /// Server's X25519 public key. Client-side only.
    #[serde(
        default,
        rename = "publicKey",
        skip_serializing_if = "String::is_empty"
    )]
    pub public_key: String,

    /// Client short ID. Must match one of the server short IDs.
    #[serde(default, rename = "shortId", skip_serializing_if = "String::is_empty")]
    pub short_id: String,

    /// TLS fingerprint to mimic.
    #[serde(default = "default_fingerprint")]
    pub fingerprint: String,

    /// Server name (SNI) to use in the ClientHello.
    #[serde(
        default,
        rename = "serverName",
        alias = "server_name",
        skip_serializing_if = "String::is_empty"
    )]
    pub server_name: String,

    /// Server-side allowed SNI values. Wildcards are not supported.
    #[serde(
        default,
        rename = "serverNames",
        alias = "server_names",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub server_names: Vec<String>,

    /// Maximum allowed time difference in seconds.
    #[serde(default, rename = "maxTimeDiff")]
    pub max_time_diff: u64,

    /// Explicit maximum allowed time difference in seconds.
    #[serde(
        default,
        rename = "maxTimeDiffSeconds",
        alias = "max_time_diff_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_time_diff_seconds: Option<u64>,

    /// Optional pacing for unauthenticated bytes sent to the fallback.
    #[serde(
        default,
        rename = "limitFallbackUpload",
        alias = "limit_fallback_upload",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit_fallback_upload: Option<RealityFallbackLimitConfig>,

    /// Optional pacing for fallback bytes returned to unauthenticated clients.
    #[serde(
        default,
        rename = "limitFallbackDownload",
        alias = "limit_fallback_download",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit_fallback_download: Option<RealityFallbackLimitConfig>,
}

/// Xray-compatible fallback pacing controls. Zero `bytesPerSec` disables it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct RealityFallbackLimitConfig {
    /// Number of bytes relayed before pacing begins.
    #[serde(default, rename = "afterBytes", alias = "after_bytes")]
    pub after_bytes: u64,
    /// Sustained bytes per second; zero disables pacing.
    #[serde(default, rename = "bytesPerSec", alias = "bytes_per_sec")]
    pub bytes_per_sec: u64,
    /// Additional bytes allowed immediately before sustained pacing.
    #[serde(default, rename = "burstBytesPerSec", alias = "burst_bytes_per_sec")]
    pub burst_bytes_per_sec: u64,
}

fn default_fingerprint() -> String {
    "chrome".to_string()
}

/// WebSocket transport settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WsConfig {
    /// HTTP path for the WebSocket upgrade request.
    #[serde(default = "default_ws_path")]
    pub path: String,

    /// Additional HTTP headers for the upgrade request.
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
}

fn default_ws_path() -> String {
    "/".to_string()
}

/// SplitHTTP transport settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SplitHttpConfig {
    /// HTTP path used by the transport.
    #[serde(default = "default_ws_path")]
    pub path: String,

    /// Optional host override(s).
    #[serde(default)]
    pub host: Vec<String>,

    /// HTTP method to use for the upload request (legacy field; Xray uses `uplinkHTTPMethod`).
    #[serde(default = "default_splithttp_method")]
    pub method: String,

    /// XHTTP mode: `stream-one`, `packet-up`, `stream-up`, or `auto` (empty = `stream-one` on server).
    #[serde(default)]
    pub mode: String,

    /// Uplink HTTP method for XHTTP (`POST` in Xray when unset).
    #[serde(default, rename = "uplinkHTTPMethod")]
    pub uplink_http_method: String,

    /// Extra HTTP headers.
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,

    /// Xray xHTTP padding byte count or range.
    #[serde(
        default,
        rename = "xPaddingBytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub x_padding_bytes: Option<PaddingBytes>,

    /// Xray xHTTP padding method (`repeat-x`, `tokenish`, etc.).
    #[serde(default, rename = "xPaddingMethod")]
    pub x_padding_method: String,

    /// Xray xHTTP padding header name.
    #[serde(default, rename = "xPaddingHeader")]
    pub x_padding_header: String,

    /// Xray xHTTP padding key used by header/cookie placements.
    #[serde(default, rename = "xPaddingKey")]
    pub x_padding_key: String,

    /// Xray xHTTP padding placement (`header`, `cookie`, etc.).
    #[serde(default, rename = "xPaddingPlacement")]
    pub x_padding_placement: String,

    /// Xray xHTTP upload session placement.
    #[serde(default, rename = "sessionPlacement")]
    pub session_placement: String,

    /// Xray xHTTP upload session key.
    #[serde(default, rename = "sessionKey")]
    pub session_key: String,

    /// Xray xHTTP upload sequence placement.
    #[serde(default, rename = "seqPlacement")]
    pub seq_placement: String,

    /// Xray xHTTP upload sequence key.
    #[serde(default, rename = "seqKey")]
    pub seq_key: String,

    /// Xray xHTTP upload data placement.
    #[serde(default, rename = "uplinkDataPlacement")]
    pub uplink_data_placement: String,

    /// Xray xHTTP upload data key.
    #[serde(default, rename = "uplinkDataKey")]
    pub uplink_data_key: String,

    /// Xray xHTTP upload chunk size hint.
    #[serde(default, rename = "uplinkChunkSize")]
    pub uplink_chunk_size: u32,

    /// Server-side maximum buffered packet-up POST bodies.
    #[serde(default, rename = "scMaxBufferedPosts")]
    pub sc_max_buffered_posts: usize,

    /// Xray Xmux settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xmux: Option<XmuxConfig>,

    /// Xray download-side transport selection.
    #[serde(
        default,
        rename = "downloadSettings",
        skip_serializing_if = "Option::is_none"
    )]
    pub download_settings: Option<DownloadSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
/// SplitHTTP padding expressed as bytes, a range string, or explicit bounds.
pub enum PaddingBytes {
    /// Fixed padding size in bytes.
    Fixed(usize),
    /// Text range accepted for Xray configuration compatibility.
    Range(String),
    /// Structured minimum and maximum padding bounds.
    Bounds(PaddingBounds),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Structured SplitHTTP padding bounds.
pub struct PaddingBounds {
    /// Minimum padding size in bytes.
    #[serde(alias = "Min", alias = "minLength")]
    pub min: Option<usize>,
    /// Maximum padding size in bytes.
    #[serde(alias = "Max", alias = "maxLength")]
    pub max: Option<usize>,
    /// Alternate lower bound accepted for compatibility.
    pub from: Option<usize>,
    /// Alternate upper bound accepted for compatibility.
    pub to: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// Xray Xmux connection and request reuse limits.
pub struct XmuxConfig {
    /// Maximum concurrent requests per multiplexed connection.
    pub max_concurrency: Option<usize>,
    /// Maximum number of multiplexed connections.
    pub max_connections: Option<usize>,
    /// Maximum reuse count for client connections.
    pub c_max_reuse_times: Option<usize>,
    /// Maximum requests served by one HTTP connection.
    pub h_max_request_times: Option<usize>,
    /// Maximum lifetime of a reusable HTTP connection in seconds.
    pub h_max_reusable_secs: Option<u64>,
    /// HTTP keep-alive interval in seconds.
    pub h_keep_alive_period: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
/// SplitHTTP download-side transport selection.
pub struct DownloadSettings {
    /// Download transport network override.
    pub network: Option<NetworkType>,
    /// Download transport security override.
    pub security: Option<SecurityType>,
}

fn default_splithttp_method() -> String {
    String::new()
}

/// gRPC transport settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GrpcConfig {
    /// gRPC service name.
    #[serde(default = "default_grpc_service", rename = "serviceName")]
    pub service_name: String,

    /// Whether to open multiple parallel gRPC streams over one HTTP/2 connection.
    #[serde(default, rename = "multiMode")]
    pub multi_mode: bool,
}

fn default_grpc_service() -> String {
    "GunService".to_string()
}

/// Sniffing settings — detect the inner protocol of a connection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SniffingConfig {
    /// Whether sniffing is enabled.
    pub enabled: bool,

    /// Protocols to sniff for: "http", "tls", or "fakedns".
    #[serde(default, rename = "destOverride")]
    pub dest_override: Vec<String>,

    /// When true, only sniff connection metadata (no payload peek). Xray `metadataOnly`.
    #[serde(default, rename = "metadataOnly")]
    pub metadata_only: bool,

    /// When true, use sniffed domain for routing but keep the original dial target (IP). Xray `routeOnly`.
    #[serde(default, rename = "routeOnly")]
    pub route_only: bool,
}

/// Hysteria2 protocol configuration.
///
/// Hysteria2 uses QUIC/HTTP3. Standard QUIC congestion is the safe default;
/// Brutal/bad-network modes are available for deployments that explicitly opt
/// into more aggressive pacing. This struct is used both for server inbound and
/// client outbound configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Hysteria2Config {
    /// Authentication password (both client and server must use the same value).
    #[serde(default)]
    pub auth: String,

    /// Target upstream bandwidth in Mbps (client → server direction).
    ///
    /// Used to size QUIC flow-control windows and, for non-standard congestion
    /// modes, pacing targets. Standard mode does not advertise a fixed Hysteria2
    /// auth bandwidth cap.
    #[serde(default = "default_mbps", rename = "upMbps")]
    pub up_mbps: u64,

    /// Target downstream bandwidth in Mbps (server → client direction).
    ///
    /// Used to size QUIC flow-control windows and non-standard pacing targets.
    #[serde(default = "default_mbps", rename = "downMbps")]
    pub down_mbps: u64,

    /// Server address for client config (e.g. "example.com:443" or "1.2.3.4:443").
    ///
    /// Not required for server-side config.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server: String,

    /// Skip TLS certificate verification.
    ///
    /// WARNING: Only use this for development and testing. In production, always
    /// verify the server certificate to prevent man-in-the-middle attacks.
    #[serde(default, rename = "skipCertVerify")]
    pub skip_cert_verify: bool,

    /// Optional bad-network congestion policy. Omitted configs use standard
    /// QUIC congestion for safer client compatibility.
    #[serde(default)]
    pub congestion: Hysteria2CongestionConfig,

    /// Number of local QUIC client endpoint shards to keep available for this
    /// outbound. A value of 1 preserves the default single-endpoint behavior.
    #[serde(default = "default_endpoint_shards", rename = "endpointShards")]
    pub endpoint_shards: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hysteria2CongestionConfig {
    #[serde(default = "default_hysteria2_congestion_mode")]
    pub mode: String,

    #[serde(default = "default_min_ack_rate", rename = "minAckRate")]
    pub min_ack_rate: f64,

    #[serde(default = "default_max_queue_delay_ms", rename = "maxQueueDelayMs")]
    pub max_queue_delay_ms: u64,

    #[serde(default = "default_pacing_gain", rename = "pacingGain")]
    pub pacing_gain: f64,

    #[serde(default = "default_loss_compensation", rename = "lossCompensation")]
    pub loss_compensation: bool,
}

impl Default for Hysteria2CongestionConfig {
    fn default() -> Self {
        Self {
            mode: default_hysteria2_congestion_mode(),
            min_ack_rate: default_min_ack_rate(),
            max_queue_delay_ms: default_max_queue_delay_ms(),
            pacing_gain: default_pacing_gain(),
            loss_compensation: default_loss_compensation(),
        }
    }
}

/// Default bandwidth in Mbps when none is specified.
///
/// 100 Mbps is a reasonable default for most modern connections.
fn default_mbps() -> u64 {
    100
}

fn default_endpoint_shards() -> usize {
    1
}

fn default_hysteria2_congestion_mode() -> String {
    "brutal-compatible".to_string()
}

fn default_min_ack_rate() -> f64 {
    0.8
}

fn default_max_queue_delay_ms() -> u64 {
    80
}

fn default_pacing_gain() -> f64 {
    1.25
}

fn default_loss_compensation() -> bool {
    true
}

/// ShadowTLS v3 configuration.
///
/// ShadowTLS wraps a real TLS handshake in front of another proxy protocol so
/// that it looks like a legitimate HTTPS connection to an external observer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowTlsConfig {
    /// Pre-shared key (password) used to derive the HMAC marker.
    pub password: String,

    /// Real TLS backend the server relays the handshake to, e.g. `"www.apple.com:443"`.
    pub dest: String,

    /// Protocol version. This implementation only supports version 3.
    #[serde(default = "default_shadowtls_version")]
    pub version: u8,
}

fn default_shadowtls_version() -> u8 {
    3
}
