use serde::{Deserialize, Serialize};

use super::{Config, NetworkType, Protocol, SecurityType};

/// Operating profile for the proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileMode {
    /// Broad compatibility mode: all protocols, transports, and features enabled.
    /// The default — prioritises interoperability with Xray / sing-box configs.
    #[default]
    Compat,
    /// Latency-first production path: narrow protocol/transport matrix, strict
    /// defaults, and active rejection of features that add per-connection overhead.
    Fast,
    /// Latency budget profile. This is less restrictive than `fast` and is
    /// evaluated by the cost-budget/explain-cost layer.
    Latency,
    /// Throughput budget profile for bulk-transfer oriented deployments.
    Throughput,
    /// Bad-network profile for lossy/high-RTT links.
    Badnet,
    /// Mobile profile for roaming/radio-pause sensitive links.
    Mobile,
    /// Stealth profile for compatibility/camouflage-heavy paths.
    Stealth,
}

/// First-packet acceleration knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstPacketBoostConfig {
    /// Master switch for first-packet acceleration.
    #[serde(default = "FirstPacketBoostConfig::default_enabled")]
    pub enabled: bool,
    /// Pre-resolve DNS where route strategy can use IP rules.
    #[serde(default = "FirstPacketBoostConfig::default_enabled")]
    pub dns: bool,
    /// Treat TLS ClientHello forwarding as an eligible first-packet boost.
    #[serde(default = "FirstPacketBoostConfig::default_enabled")]
    pub tls_client_hello: bool,
    /// Forward first data bytes as early payload where protocol handlers support it.
    #[serde(default = "FirstPacketBoostConfig::default_enabled")]
    pub send_early_payload: bool,
    /// Duplicate first control packet on bad-network paths when supported.
    #[serde(default)]
    pub duplicate_control_on_badnet: bool,
    /// Packet scheduling priority for first-packet work.
    #[serde(default)]
    pub priority: FirstPacketPriority,
}

impl FirstPacketBoostConfig {
    fn default_enabled() -> bool {
        true
    }
}

impl Default for FirstPacketBoostConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dns: true,
            tls_client_hello: true,
            send_early_payload: true,
            duplicate_control_on_badnet: false,
            priority: FirstPacketPriority::High,
        }
    }
}

/// Scheduling priority assigned to first-packet work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FirstPacketPriority {
    /// Standard OS scheduling priority.
    Normal,
    #[default]
    /// Elevated scheduling priority (default).
    High,
    /// Highest scheduling priority; use for latency-critical deployments.
    Critical,
}

impl std::fmt::Display for ProfileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileMode::Compat => f.write_str("compat"),
            ProfileMode::Fast => f.write_str("fast"),
            ProfileMode::Latency => f.write_str("latency"),
            ProfileMode::Throughput => f.write_str("throughput"),
            ProfileMode::Badnet => f.write_str("badnet"),
            ProfileMode::Mobile => f.write_str("mobile"),
            ProfileMode::Stealth => f.write_str("stealth"),
        }
    }
}

impl std::str::FromStr for ProfileMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "compat" => Ok(ProfileMode::Compat),
            "fast" => Ok(ProfileMode::Fast),
            "latency" => Ok(ProfileMode::Latency),
            "throughput" => Ok(ProfileMode::Throughput),
            "badnet" => Ok(ProfileMode::Badnet),
            "mobile" => Ok(ProfileMode::Mobile),
            "stealth" => Ok(ProfileMode::Stealth),
            other => Err(format!(
                "unknown profile '{other}'; expected compat, fast, latency, throughput, badnet, mobile, or stealth"
            )),
        }
    }
}

/// Performance budget constraints used by `blackwire explain-cost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetConfig {
    #[serde(default = "BudgetConfig::default_max_protocol_layers")]
    /// Maximum number of hot-path protocol layers before a violation is raised.
    pub max_protocol_layers: usize,
    #[serde(default)]
    /// Whether protocol sniffing is permitted within budget.
    pub allow_sniffing: bool,
    #[serde(default)]
    /// Whether fake-IP DNS is permitted within budget.
    pub allow_fake_ip: bool,
    #[serde(default = "BudgetConfig::default_max_route_rules")]
    /// Maximum number of routing rules before a violation is raised.
    pub max_route_rules: usize,
    #[serde(default = "BudgetConfig::default_max_handshake_ms")]
    /// Maximum acceptable TLS/QUIC handshake time in milliseconds.
    pub max_handshake_ms: u64,
    #[serde(default = "BudgetConfig::default_true")]
    /// Prefer zero-copy / splice paths when available.
    pub prefer_direct_copy: bool,
    #[serde(default = "BudgetConfig::default_true")]
    /// Prefer QUIC datagram lane for UDP flows when available.
    pub prefer_datagram_for_udp: bool,
}

impl BudgetConfig {
    fn default_max_protocol_layers() -> usize {
        3
    }

    fn default_max_route_rules() -> usize {
        50
    }

    fn default_max_handshake_ms() -> u64 {
        300
    }

    fn default_true() -> bool {
        true
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_protocol_layers: Self::default_max_protocol_layers(),
            allow_sniffing: false,
            allow_fake_ip: false,
            max_route_rules: Self::default_max_route_rules(),
            max_handshake_ms: Self::default_max_handshake_ms(),
            prefer_direct_copy: true,
            prefer_datagram_for_udp: true,
        }
    }
}

/// Extra settings that only apply when `profile = "fast"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastConfig {
    /// Reject `security = none` when `true` (default). Set `false` only in lab /
    /// benchmark environments where unencrypted VLESS TCP is acceptable.
    #[serde(default = "FastConfig::default_strict_production")]
    pub strict_production: bool,

    /// TCP preconnect pooling policy for Freedom outbounds.
    ///
    /// `disabled` avoids preconnect pooling entirely. `adaptive` starts
    /// conservative and enables pooling only for hot destinations. `fixed`
    /// keeps legacy numeric `poolSize` behavior for lab/debug configs.
    #[serde(default)]
    pub pool: FastPoolPolicy,

    /// Raw TCP relay policy. `adaptive` currently means "use splice when both
    /// streams are raw TCP and record the decision"; policy hooks are kept here
    /// so future payload-aware thresholds do not change config shape.
    #[serde(default)]
    pub splice: FastSplicePolicy,

    /// Userspace relay engine used when splice is unavailable or disabled.
    #[serde(default)]
    pub relay: FastRelayConfig,

    /// Linux-only extreme-path options. Non-Linux builds accept these settings
    /// but ignore them at runtime.
    #[serde(default)]
    pub linux: FastLinuxConfig,
}

impl FastConfig {
    fn default_strict_production() -> bool {
        true
    }
}

impl Default for FastConfig {
    fn default() -> Self {
        Self {
            strict_production: true,
            pool: FastPoolPolicy::default(),
            splice: FastSplicePolicy::default(),
            relay: FastRelayConfig::default(),
            linux: FastLinuxConfig::default(),
        }
    }
}

/// Linux-only relay extensions for bulk TCP paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastLinuxConfig {
    /// Optional MSG_ZEROCOPY use for raw TCP userspace bulk writes.
    #[serde(default)]
    pub zerocopy: FastZerocopyPolicy,

    /// Minimum payload size before MSG_ZEROCOPY is attempted.
    #[serde(default = "FastLinuxConfig::default_zerocopy_min_bytes")]
    pub zerocopy_min_bytes: usize,

    /// io_uring backend preference for the splice relay.
    /// Default is `disabled` (epoll splice). Set to `auto` to probe io_uring at
    /// runtime; set to `require` to mandate it (startup fails if unavailable).
    /// Note: benchmarks on commodity VPS hardware showed io_uring splice incurred
    /// a severe throughput regression vs epoll splice (−66% churn, −25% keepalive),
    /// so `disabled` is the safe default. Enable only after validating on your
    /// specific host.
    #[serde(default = "FastLinuxConfig::default_io_uring")]
    pub io_uring: FastExperimentalBackendPolicy,

    /// AF_XDP backend preference. This is intentionally experimental and is not
    /// selected automatically for normal proxy streams.
    #[serde(default)]
    pub af_xdp: FastExperimentalBackendPolicy,
}

impl FastLinuxConfig {
    fn default_zerocopy_min_bytes() -> usize {
        16 * 1024
    }

    fn default_io_uring() -> FastExperimentalBackendPolicy {
        FastExperimentalBackendPolicy::Disabled
    }
}

impl Default for FastLinuxConfig {
    fn default() -> Self {
        Self {
            zerocopy: FastZerocopyPolicy::default(),
            zerocopy_min_bytes: Self::default_zerocopy_min_bytes(),
            io_uring: Self::default_io_uring(),
            af_xdp: FastExperimentalBackendPolicy::default(),
        }
    }
}

/// MSG_ZEROCOPY policy for raw TCP userspace writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FastZerocopyPolicy {
    /// Do not use MSG_ZEROCOPY.
    #[default]
    Disabled,
    /// Use MSG_ZEROCOPY only on bulk writes that exceed the configured floor.
    Bulk,
    /// Attempt MSG_ZEROCOPY for every raw TCP userspace write.
    Always,
}

/// Selector for Linux experimental backends that need privileged/kernel support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FastExperimentalBackendPolicy {
    /// Do not select this backend.
    Disabled,
    /// Try this backend where supported, then fall back safely.
    #[default]
    Auto,
    /// Require this backend. Startup/runtime validation may fail if unsupported.
    Require,
}

/// Relay engine and buffer policy for Fast Profile userspace copy paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FastRelayConfig {
    /// Userspace relay implementation. `legacy` preserves the current pooled
    /// two-loop relay. `v2` enables one-task ring-buffer relay.
    #[serde(default)]
    pub engine: FastRelayEngine,

    /// Write flush behavior for Relay Engine v2.
    #[serde(default)]
    pub flush: FastRelayFlushPolicy,

    /// Initial per-direction v2 buffer size in bytes.
    #[serde(default = "FastRelayConfig::default_initial_buffer")]
    pub initial_buffer: usize,

    /// Maximum per-direction v2 buffer size in bytes.
    #[serde(default = "FastRelayConfig::default_max_buffer")]
    pub max_buffer: usize,
}

impl FastRelayConfig {
    fn default_initial_buffer() -> usize {
        16 * 1024
    }

    fn default_max_buffer() -> usize {
        256 * 1024
    }
}

impl Default for FastRelayConfig {
    fn default() -> Self {
        Self {
            engine: FastRelayEngine::default(),
            flush: FastRelayFlushPolicy::default(),
            initial_buffer: Self::default_initial_buffer(),
            max_buffer: Self::default_max_buffer(),
        }
    }
}

/// Userspace relay engine selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FastRelayEngine {
    /// Existing pooled relay implementation.
    #[default]
    Legacy,
    /// Relay Engine v2: one-task duplex relay with growable ring buffers.
    V2,
}

/// Flush policy for Relay Engine v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FastRelayFlushPolicy {
    /// Flush after each write, matching legacy semantics.
    #[default]
    Immediate,
    /// Flush when a direction reaches EOF/shutdown.
    Deferred,
    /// Coalesce flushes per burst: flush only when the source pauses or reaches
    /// EOF. Keeps bulk syscall pressure low without delaying interactive writes.
    Adaptive,
}

/// TCP connection pool strategy for the Fast Profile outbound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FastPoolPolicy {
    /// Ramp pool size based on destination hotness (default).
    /// Pooling only activates for destinations that exceed `min_hotness_for_pool`
    /// recent requests; one-shot destinations are never pooled.
    #[default]
    Adaptive,
    /// Disable pooling entirely.
    Disabled,
    /// Use a fixed pool size set by `poolSize`.
    Fixed,
}

/// Splice relay strategy for the Fast Profile dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FastSplicePolicy {
    /// Use splice only after `ADAPTIVE_SPLICE_MIN_BYTES` have been relayed (default).
    #[default]
    Adaptive,
    /// Never use splice; always use the configured userspace relay engine.
    Disabled,
    /// Always use splice for eligible (raw TCP) streams.
    Always,
}

/// A validation finding returned by [`validate_fast_profile`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileViolation {
    /// Hard error — this configuration cannot run under Fast Profile.
    Error(String),
    /// Warning — configuration will work but may hurt the latency story.
    Warning(String),
}

/// Relative cost class for a protocol dimension (cpu, allocations, latency).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostClass {
    /// Negligible overhead.
    Low,
    /// Moderate overhead acceptable for most deployments.
    Medium,
    /// Significant overhead; may violate a strict budget.
    High,
}

impl std::fmt::Display for CostClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => f.write_str("low"),
            Self::Medium => f.write_str("medium"),
            Self::High => f.write_str("high"),
        }
    }
}

/// Data-copy strategy used by a protocol layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMode {
    /// Zero-copy via splice / sendfile.
    Direct,
    /// One extra copy through a wrapper buffer.
    Wrapped,
    /// Copy through a length-prefixed framing layer.
    Framed,
    /// Packet-by-packet copy (e.g. UDP datagram relay).
    Packet,
}

impl std::fmt::Display for CopyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Direct => f.write_str("direct"),
            Self::Wrapped => f.write_str("wrapped"),
            Self::Framed => f.write_str("framed"),
            Self::Packet => f.write_str("packet"),
        }
    }
}

/// Aggregate cost estimate for a protocol configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolCost {
    /// CPU overhead class for the hot path.
    pub cpu: CostClass,
    /// Memory allocation overhead class.
    pub allocations: CostClass,
    /// Added per-packet latency class.
    pub latency: CostClass,
    /// Copy strategy used by the innermost data layer.
    pub copy_mode: CopyMode,
    /// Whether zero-copy (splice/sendfile) is available for this config.
    pub supports_direct_copy: bool,
    /// Whether `splice(2)` can be used on this platform/config.
    pub supports_splice: bool,
    /// Whether TLS early data (0-RTT) is enabled.
    pub supports_early_data: bool,
    /// Whether QUIC datagram relay is available.
    pub supports_datagram: bool,
}

/// Cost and compliance report produced by [`explain_cost`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostReport {
    /// Active performance profile.
    pub profile: ProfileMode,
    /// Budget constraints applied during analysis.
    pub budget: BudgetConfig,
    /// Number of hot-path protocol layers in the config.
    pub layer_count: usize,
    /// Names of the hot-path protocol layers.
    pub layers: Vec<String>,
    /// Aggregate protocol cost for the active config.
    pub cost: ProtocolCost,
    /// Profile violations found, if any.
    pub findings: Vec<ProfileViolation>,
    /// Suggested changes to bring the config within profile.
    pub suggestions: Vec<String>,
}

impl CostReport {
    /// Render a human-readable cost report summary.
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Profile: {}\n", self.profile));
        out.push_str("Hot-path layers:\n");
        for layer in &self.layers {
            out.push_str(&format!("  - {layer}\n"));
        }
        out.push_str(&format!("Layer count: {}\n", self.layer_count));
        out.push_str(&format!(
            "Cost: cpu={}, allocations={}, latency={}, copyMode={}\n",
            self.cost.cpu, self.cost.allocations, self.cost.latency, self.cost.copy_mode
        ));
        out.push_str(&format!(
            "Direct copy: {}\n",
            if self.cost.supports_direct_copy {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "Splice: {}\n",
            if self.cost.supports_splice {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "Early payload: {}\n",
            if self.cost.supports_early_data {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "Datagram: {}\n",
            if self.cost.supports_datagram {
                "yes"
            } else {
                "no"
            }
        ));
        if self.findings.is_empty() {
            out.push_str("Findings: none\n");
        } else {
            out.push_str("Findings:\n");
            for finding in &self.findings {
                out.push_str(&format!("  - {finding}\n"));
            }
        }
        if !self.suggestions.is_empty() {
            out.push_str("Suggested fixes:\n");
            for suggestion in &self.suggestions {
                out.push_str(&format!("  - {suggestion}\n"));
            }
        }
        out
    }
}

impl ProfileViolation {
    /// Returns `true` if this is a hard error that should abort startup.
    pub fn is_error(&self) -> bool {
        matches!(self, ProfileViolation::Error(_))
    }

    /// The human-readable violation message, without the severity prefix.
    pub fn message(&self) -> &str {
        match self {
            ProfileViolation::Error(m) | ProfileViolation::Warning(m) => m,
        }
    }
}

impl std::fmt::Display for ProfileViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileViolation::Error(m) => write!(f, "error: {m}"),
            ProfileViolation::Warning(m) => write!(f, "warning: {m}"),
        }
    }
}

fn protocol_name(p: &Protocol) -> &'static str {
    match p {
        Protocol::Vless => "vless",
        Protocol::Vmess => "vmess",
        Protocol::Trojan => "trojan",
        Protocol::Shadowsocks => "shadowsocks",
        Protocol::Hysteria2 => "hysteria2",
        Protocol::Tuic => "tuic",
        Protocol::ShadowTls => "shadowtls",
        Protocol::Socks => "socks",
        Protocol::Http => "http",
        Protocol::Freedom => "freedom",
    }
}

fn network_name(n: &NetworkType) -> &'static str {
    match n {
        NetworkType::Tcp => "tcp",
        NetworkType::Ws => "ws",
        NetworkType::HttpUpgrade => "httpupgrade",
        NetworkType::Grpc => "grpc",
        NetworkType::Quic => "quic",
        NetworkType::Kcp => "kcp",
        NetworkType::SplitHttp => "splithttp",
    }
}

fn security_name(s: &SecurityType) -> &'static str {
    match s {
        SecurityType::None => "none",
        SecurityType::Tls => "tls",
        SecurityType::Reality => "reality",
        SecurityType::ShadowTls => "shadowtls",
    }
}

fn bump_cost(current: CostClass, candidate: CostClass) -> CostClass {
    match (current, candidate) {
        (CostClass::High, _) | (_, CostClass::High) => CostClass::High,
        (CostClass::Medium, _) | (_, CostClass::Medium) => CostClass::Medium,
        _ => CostClass::Low,
    }
}

fn add_unique(layers: &mut Vec<String>, layer: impl Into<String>) {
    let layer = layer.into();
    if !layers.iter().any(|existing| existing == &layer) {
        layers.push(layer);
    }
}

fn stream_settings_for_cost<'a>(
    inbound: &'a super::InboundConfig,
    outbound: &'a super::OutboundConfig,
) -> Option<&'a super::StreamSettingsConfig> {
    inbound
        .stream_settings
        .as_ref()
        .or(outbound.stream_settings.as_ref())
}

fn protocol_cost(config: &Config) -> ProtocolCost {
    let mut cpu = CostClass::Low;
    let mut allocations = CostClass::Low;
    let mut latency = CostClass::Low;
    let mut copy_mode = CopyMode::Direct;
    let mut supports_direct_copy = true;
    let mut supports_splice = true;
    let mut supports_early_data = true;
    let mut supports_datagram = false;

    for endpoint in config
        .inbounds
        .iter()
        .map(|i| (&i.protocol, &i.stream_settings))
    {
        match endpoint.0 {
            Protocol::Vless | Protocol::Freedom | Protocol::Socks | Protocol::Http => {}
            Protocol::Trojan | Protocol::Shadowsocks => {
                cpu = bump_cost(cpu, CostClass::Medium);
                copy_mode = CopyMode::Wrapped;
                supports_splice = false;
            }
            Protocol::Vmess => {
                cpu = bump_cost(cpu, CostClass::High);
                allocations = bump_cost(allocations, CostClass::Medium);
                latency = bump_cost(latency, CostClass::Medium);
                copy_mode = CopyMode::Wrapped;
                supports_direct_copy = false;
                supports_splice = false;
                supports_early_data = false;
            }
            Protocol::Hysteria2 | Protocol::Tuic => {
                cpu = bump_cost(cpu, CostClass::Medium);
                latency = bump_cost(latency, CostClass::Medium);
                copy_mode = CopyMode::Framed;
                supports_splice = false;
                supports_datagram = true;
            }
            Protocol::ShadowTls => {
                cpu = bump_cost(cpu, CostClass::High);
                latency = bump_cost(latency, CostClass::High);
                copy_mode = CopyMode::Wrapped;
                supports_direct_copy = false;
                supports_splice = false;
            }
        }
        if let Some(ss) = endpoint.1 {
            apply_transport_cost(
                ss,
                &mut cpu,
                &mut allocations,
                &mut latency,
                &mut copy_mode,
                &mut supports_direct_copy,
                &mut supports_splice,
                &mut supports_datagram,
            );
        }
    }

    for endpoint in config
        .outbounds
        .iter()
        .map(|o| (&o.protocol, &o.stream_settings))
    {
        match endpoint.0 {
            Protocol::Vless | Protocol::Freedom => {}
            Protocol::Socks | Protocol::Http | Protocol::Trojan | Protocol::Shadowsocks => {
                cpu = bump_cost(cpu, CostClass::Medium);
                copy_mode = CopyMode::Wrapped;
                supports_splice = false;
            }
            Protocol::Vmess => {
                cpu = bump_cost(cpu, CostClass::High);
                allocations = bump_cost(allocations, CostClass::Medium);
                latency = bump_cost(latency, CostClass::Medium);
                copy_mode = CopyMode::Wrapped;
                supports_direct_copy = false;
                supports_splice = false;
                supports_early_data = false;
            }
            Protocol::Hysteria2 | Protocol::Tuic => {
                cpu = bump_cost(cpu, CostClass::Medium);
                latency = bump_cost(latency, CostClass::Medium);
                copy_mode = CopyMode::Framed;
                supports_splice = false;
                supports_datagram = true;
            }
            Protocol::ShadowTls => {
                cpu = bump_cost(cpu, CostClass::High);
                latency = bump_cost(latency, CostClass::High);
                copy_mode = CopyMode::Wrapped;
                supports_direct_copy = false;
                supports_splice = false;
            }
        }
        if let Some(ss) = endpoint.1 {
            apply_transport_cost(
                ss,
                &mut cpu,
                &mut allocations,
                &mut latency,
                &mut copy_mode,
                &mut supports_direct_copy,
                &mut supports_splice,
                &mut supports_datagram,
            );
        }
    }

    ProtocolCost {
        cpu,
        allocations,
        latency,
        copy_mode,
        supports_direct_copy,
        supports_splice,
        supports_early_data,
        supports_datagram,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_transport_cost(
    ss: &super::StreamSettingsConfig,
    cpu: &mut CostClass,
    allocations: &mut CostClass,
    latency: &mut CostClass,
    copy_mode: &mut CopyMode,
    supports_direct_copy: &mut bool,
    supports_splice: &mut bool,
    supports_datagram: &mut bool,
) {
    match ss.network {
        NetworkType::Tcp => {}
        NetworkType::Ws | NetworkType::HttpUpgrade | NetworkType::Grpc | NetworkType::SplitHttp => {
            *cpu = bump_cost(*cpu, CostClass::High);
            *allocations = bump_cost(*allocations, CostClass::High);
            *latency = bump_cost(*latency, CostClass::High);
            *copy_mode = CopyMode::Framed;
            *supports_direct_copy = false;
            *supports_splice = false;
        }
        NetworkType::Quic | NetworkType::Kcp => {
            *cpu = bump_cost(*cpu, CostClass::Medium);
            *latency = bump_cost(*latency, CostClass::Medium);
            *copy_mode = CopyMode::Packet;
            *supports_splice = false;
            *supports_datagram = true;
        }
    }

    match ss.security {
        SecurityType::None => {}
        SecurityType::Tls | SecurityType::Reality => {
            *cpu = bump_cost(*cpu, CostClass::Medium);
            *copy_mode = CopyMode::Wrapped;
            *supports_splice = false;
        }
        SecurityType::ShadowTls => {
            *cpu = bump_cost(*cpu, CostClass::High);
            *latency = bump_cost(*latency, CostClass::High);
            *copy_mode = CopyMode::Wrapped;
            *supports_direct_copy = false;
            *supports_splice = false;
        }
    }
}

/// Compute a [`CostReport`] summarising the protocol cost and profile compliance of `config`.
pub fn explain_cost(config: &Config) -> CostReport {
    let budget = config.budget.unwrap_or_else(|| match config.profile {
        ProfileMode::Latency | ProfileMode::Fast => BudgetConfig {
            max_protocol_layers: 3,
            allow_sniffing: false,
            allow_fake_ip: false,
            max_route_rules: 50,
            max_handshake_ms: 300,
            prefer_direct_copy: true,
            prefer_datagram_for_udp: true,
        },
        ProfileMode::Throughput => BudgetConfig {
            max_protocol_layers: 4,
            max_route_rules: 200,
            ..BudgetConfig::default()
        },
        ProfileMode::Badnet | ProfileMode::Mobile => BudgetConfig {
            max_protocol_layers: 4,
            max_handshake_ms: 700,
            prefer_datagram_for_udp: true,
            ..BudgetConfig::default()
        },
        ProfileMode::Compat | ProfileMode::Stealth => BudgetConfig {
            max_protocol_layers: 8,
            allow_sniffing: true,
            allow_fake_ip: true,
            max_route_rules: 1000,
            max_handshake_ms: 1500,
            prefer_direct_copy: false,
            prefer_datagram_for_udp: false,
        },
    });

    let mut layers = Vec::new();
    add_unique(&mut layers, "TCP accept");

    for inbound in &config.inbounds {
        add_unique(
            &mut layers,
            format!(
                "inbound {} protocol {}",
                inbound.tag,
                protocol_name(&inbound.protocol)
            ),
        );
        if let Some(ss) = &inbound.stream_settings {
            add_unique(
                &mut layers,
                format!("transport {}", network_name(&ss.network)),
            );
            if ss.security != SecurityType::None {
                add_unique(
                    &mut layers,
                    format!("security {}", security_name(&ss.security)),
                );
            }
        }
        if inbound.sniffing.as_ref().is_some_and(|s| s.enabled) {
            add_unique(&mut layers, "sniffing");
        }
    }

    if let Some(routing) = &config.routing {
        add_unique(
            &mut layers,
            format!("routing {} rules", routing.rules.len()),
        );
        if routing
            .domain_strategy
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("IpOnDemand"))
        {
            add_unique(&mut layers, "routing DNS lookup");
        }
    } else {
        add_unique(&mut layers, "routing default");
    }

    if config
        .dns
        .as_ref()
        .and_then(|d| d.fake_ip.as_ref())
        .is_some_and(|f| f.enabled)
    {
        add_unique(&mut layers, "FakeIP");
    }

    for outbound in &config.outbounds {
        add_unique(
            &mut layers,
            format!(
                "outbound {} protocol {}",
                outbound.tag,
                protocol_name(&outbound.protocol)
            ),
        );
        if let Some(first_inbound) = config.inbounds.first() {
            if let Some(ss) = stream_settings_for_cost(first_inbound, outbound) {
                if outbound.stream_settings.is_some() {
                    add_unique(
                        &mut layers,
                        format!("outbound transport {}", network_name(&ss.network)),
                    );
                    if ss.security != SecurityType::None {
                        add_unique(
                            &mut layers,
                            format!("outbound security {}", security_name(&ss.security)),
                        );
                    }
                }
            }
        }
    }

    let cost = protocol_cost(config);
    let mut findings = Vec::new();
    let mut suggestions = Vec::new();

    if layers.len() > budget.max_protocol_layers {
        findings.push(ProfileViolation::Warning(format!(
            "hot path has {} layers; budget allows {}",
            layers.len(),
            budget.max_protocol_layers
        )));
        suggestions.push("remove one wrapper layer or move to a less strict profile".into());
    }

    if !budget.allow_sniffing
        && config
            .inbounds
            .iter()
            .any(|i| i.sniffing.as_ref().is_some_and(|s| s.enabled))
    {
        findings.push(ProfileViolation::Warning(
            "sniffing is enabled but this profile budget disallows it".into(),
        ));
        suggestions.push("disable inbound sniffing or use compat/stealth profile".into());
    }

    if !budget.allow_fake_ip
        && config
            .dns
            .as_ref()
            .and_then(|d| d.fake_ip.as_ref())
            .is_some_and(|f| f.enabled)
    {
        findings.push(ProfileViolation::Warning(
            "FakeIP is enabled but this profile budget disallows it".into(),
        ));
        suggestions.push("disable dns.fakeIp for latency profiles".into());
    }

    if let Some(routing) = &config.routing {
        if routing.rules.len() > budget.max_route_rules {
            findings.push(ProfileViolation::Warning(format!(
                "routing has {} rules; budget allows {}",
                routing.rules.len(),
                budget.max_route_rules
            )));
            suggestions.push("compile/prune routing rules or raise budget.maxRouteRules".into());
        }
    }

    if budget.prefer_direct_copy && !cost.supports_direct_copy {
        findings.push(ProfileViolation::Warning(
            "direct copy is preferred but this path cannot lower to direct copy".into(),
        ));
        suggestions.push(
            "prefer VLESS+REALITY over TCP, avoid WebSocket/gRPC/SplitHTTP on latency paths".into(),
        );
    }

    if budget.prefer_direct_copy && !cost.supports_splice {
        suggestions
            .push("use relay.engine=v2 for wrapped streams where splice is unavailable".into());
    }

    CostReport {
        profile: config.profile,
        budget,
        layer_count: layers.len(),
        layers,
        cost,
        findings,
        suggestions,
    }
}

/// Validate `config` against Fast Profile constraints.
///
/// Returns an empty `Vec` when `config.profile` is `Compat` — no restrictions
/// apply. When `Fast`, returns a list of findings:
/// - [`ProfileViolation::Error`] — the config must not start; caller should abort.
/// - [`ProfileViolation::Warning`] — caller should print and continue.
pub fn validate_fast_profile(config: &Config) -> Vec<ProfileViolation> {
    if config.profile != ProfileMode::Fast {
        return vec![];
    }

    let strict = config
        .fast
        .as_ref()
        .map(|f| f.strict_production)
        .unwrap_or(true);

    let mut v: Vec<ProfileViolation> = Vec::new();

    // ── Inbounds ──────────────────────────────────────────────────────────────
    for ib in &config.inbounds {
        if ib.protocol != Protocol::Vless {
            v.push(ProfileViolation::Error(format!(
                "inbound '{}': protocol '{}' is not allowed in Fast Profile (only vless)",
                ib.tag,
                protocol_name(&ib.protocol)
            )));
        }

        match &ib.stream_settings {
            Some(ss) => {
                if ss.network != NetworkType::Tcp {
                    v.push(ProfileViolation::Error(format!(
                        "inbound '{}': network='{}' is not allowed in Fast Profile (only tcp)",
                        ib.tag,
                        network_name(&ss.network)
                    )));
                }

                if ss.security == SecurityType::None {
                    let msg = format!(
                        "inbound '{}': security=none; use reality or tls in Fast Profile",
                        ib.tag
                    );
                    if strict {
                        v.push(ProfileViolation::Error(msg));
                    } else {
                        v.push(ProfileViolation::Warning(msg));
                    }
                }
            }
            None => {
                // Absent streamSettings implies no TLS/REALITY (security=none).
                let msg = format!(
                    "inbound '{}': no streamSettings (security=none); use reality or tls in Fast Profile",
                    ib.tag
                );
                if strict {
                    v.push(ProfileViolation::Error(msg));
                } else {
                    v.push(ProfileViolation::Warning(msg));
                }
            }
        }

        if ib.sniffing.as_ref().is_some_and(|s| s.enabled) {
            v.push(ProfileViolation::Error(format!(
                "inbound '{}': sniffing=true is not allowed in Fast Profile (adds per-connection overhead)",
                ib.tag
            )));
        }
    }

    // ── Outbounds ─────────────────────────────────────────────────────────────
    for ob in &config.outbounds {
        if ob.protocol != Protocol::Vless && ob.protocol != Protocol::Freedom {
            v.push(ProfileViolation::Error(format!(
                "outbound '{}': protocol '{}' is not allowed in Fast Profile (only vless, freedom)",
                ob.tag,
                protocol_name(&ob.protocol)
            )));
        }
    }

    // ── DNS ───────────────────────────────────────────────────────────────────
    if config
        .dns
        .as_ref()
        .and_then(|d| d.fake_ip.as_ref())
        .is_some_and(|f| f.enabled)
    {
        v.push(ProfileViolation::Error(
            "dns.fakeIp=true is not allowed in Fast Profile (adds per-query overhead)".into(),
        ));
    }

    // ── Routing ───────────────────────────────────────────────────────────────
    if let Some(routing) = &config.routing {
        if routing
            .domain_strategy
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("IpOnDemand"))
        {
            v.push(ProfileViolation::Error(
                "routing.domainStrategy=IpOnDemand is not allowed in Fast Profile \
                 (forces a DNS lookup on every connection)"
                    .into(),
            ));
        }

        if routing.rules.len() > 50 {
            v.push(ProfileViolation::Warning(format!(
                "routing has {} rules; large rule sets add routing latency (consider pruning to ≤ 50)",
                routing.rules.len()
            )));
        }

        let geo_count: usize = routing
            .rules
            .iter()
            .flat_map(|r| r.domain.iter().chain(r.ip.iter()))
            .filter(|p| p.starts_with("geosite:") || p.starts_with("geoip:"))
            .count();

        if geo_count > 20 {
            v.push(ProfileViolation::Warning(format!(
                "{geo_count} GeoSite/GeoIP patterns across routing rules; \
                 large geo sets increase per-connection routing time"
            )));
        }
    }

    v
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;
    use crate::schema::{
        Config, InboundConfig, LimitsConfig, LogConfig, OutboundConfig, RoutingConfig, RoutingRule,
        SniffingConfig, StreamSettingsConfig,
    };

    fn fast_vless_config() -> Config {
        Config {
            profile: ProfileMode::Fast,
            fast: None,
            budget: None,
            vision: None,
            first_packet_boost: None,
            quic: None,
            datagram: None,
            fec: None,
            log: LogConfig::default(),
            dns: None,
            routing: None,
            tun: None,
            limits: LimitsConfig::default(),
            inbounds: vec![InboundConfig {
                tag: "in-vless".into(),
                listen: "127.0.0.1".parse::<IpAddr>().unwrap(),
                port: 443,
                protocol: Protocol::Vless,
                settings: serde_json::json!({}),
                stream_settings: Some(StreamSettingsConfig {
                    security: SecurityType::Reality,
                    ..Default::default()
                }),
                limits: None,
                sniffing: None,
            }],
            outbounds: vec![OutboundConfig {
                tag: "direct".into(),
                protocol: Protocol::Freedom,
                settings: serde_json::json!({}),
                stream_settings: None,
            }],
            stats: None,
            api: None,
            metrics_addr: None,
        }
    }

    #[test]
    fn first_packet_boost_config_parses_camel_case() {
        let cfg: Config = serde_json::from_value(serde_json::json!({
            "firstPacketBoost": {
                "enabled": true,
                "dns": false,
                "tlsClientHello": true,
                "sendEarlyPayload": true,
                "duplicateControlOnBadnet": true,
                "priority": "critical"
            },
            "inbounds": [{
                "tag": "in",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 1080
            }],
            "outbounds": [{
                "tag": "direct",
                "protocol": "freedom"
            }]
        }))
        .unwrap();

        let boost = cfg.first_packet_boost.unwrap();
        assert!(boost.enabled);
        assert!(!boost.dns);
        assert!(boost.tls_client_hello);
        assert!(boost.send_early_payload);
        assert!(boost.duplicate_control_on_badnet);
        assert_eq!(boost.priority, FirstPacketPriority::Critical);
    }

    #[test]
    fn compat_profile_skips_all_checks() {
        let mut cfg = fast_vless_config();
        cfg.profile = ProfileMode::Compat;
        // Even a vmess inbound should not trigger any violations in Compat mode.
        cfg.inbounds[0].protocol = Protocol::Vmess;
        assert!(validate_fast_profile(&cfg).is_empty());
    }

    #[test]
    fn valid_fast_profile_has_no_violations() {
        let cfg = fast_vless_config();
        assert!(validate_fast_profile(&cfg).is_empty());
    }

    #[test]
    fn vmess_inbound_is_rejected() {
        let mut cfg = fast_vless_config();
        cfg.inbounds[0].protocol = Protocol::Vmess;
        let violations = validate_fast_profile(&cfg);
        assert!(violations.iter().any(|v| v.is_error()));
        assert!(violations
            .iter()
            .any(|v| v.message().contains("vmess") && v.message().contains("inbound")));
    }

    #[test]
    fn ws_transport_is_rejected() {
        let mut cfg = fast_vless_config();
        cfg.inbounds[0].stream_settings = Some(StreamSettingsConfig {
            network: NetworkType::Ws,
            security: SecurityType::Tls,
            ..Default::default()
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| v.is_error() && v.message().contains("ws")));
    }

    #[test]
    fn security_none_strict_production_is_error() {
        let mut cfg = fast_vless_config();
        cfg.fast = Some(FastConfig {
            strict_production: true,
            ..Default::default()
        });
        cfg.inbounds[0].stream_settings = Some(StreamSettingsConfig {
            security: SecurityType::None,
            ..Default::default()
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| v.is_error() && v.message().contains("security=none")));
    }

    #[test]
    fn security_none_lab_mode_is_warning() {
        let mut cfg = fast_vless_config();
        cfg.fast = Some(FastConfig {
            strict_production: false,
            ..Default::default()
        });
        cfg.inbounds[0].stream_settings = Some(StreamSettingsConfig {
            security: SecurityType::None,
            ..Default::default()
        });
        let violations = validate_fast_profile(&cfg);
        assert!(!violations.iter().any(|v| v.is_error()));
        assert!(violations
            .iter()
            .any(|v| matches!(v, ProfileViolation::Warning(_))
                && v.message().contains("security=none")));
    }

    #[test]
    fn sniffing_enabled_is_rejected() {
        let mut cfg = fast_vless_config();
        cfg.inbounds[0].sniffing = Some(SniffingConfig {
            enabled: true,
            dest_override: vec![],
            metadata_only: false,
            route_only: false,
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| v.is_error() && v.message().contains("sniffing")));
    }

    #[test]
    fn vmess_outbound_is_rejected() {
        let mut cfg = fast_vless_config();
        cfg.outbounds[0].protocol = Protocol::Vmess;
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| v.is_error() && v.message().contains("outbound")));
    }

    #[test]
    fn fake_ip_is_rejected() {
        use crate::schema::{DnsConfig, FakeIpConfig};
        let mut cfg = fast_vless_config();
        cfg.dns = Some(DnsConfig {
            servers: vec![],
            fake_ip: Some(FakeIpConfig {
                enabled: true,
                pool: "198.18.0.0/15".into(),
            }),
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| v.is_error() && v.message().contains("fakeIp")));
    }

    #[test]
    fn ip_on_demand_is_rejected() {
        let mut cfg = fast_vless_config();
        cfg.routing = Some(RoutingConfig {
            domain_strategy: Some("IpOnDemand".into()),
            ..Default::default()
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| v.is_error() && v.message().contains("IpOnDemand")));
    }

    #[test]
    fn large_rule_set_warns() {
        let mut cfg = fast_vless_config();
        let rules: Vec<RoutingRule> = (0..=50)
            .map(|i| RoutingRule {
                outbound_tag: "direct".into(),
                domain: vec![format!("domain:example{i}.com")],
                ..Default::default()
            })
            .collect();
        cfg.routing = Some(RoutingConfig {
            rules,
            ..Default::default()
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| matches!(v, ProfileViolation::Warning(_)) && v.message().contains("rules")));
    }

    #[test]
    fn large_geo_set_warns() {
        let mut cfg = fast_vless_config();
        let rules: Vec<RoutingRule> = (0..=20)
            .map(|i| RoutingRule {
                outbound_tag: "direct".into(),
                ip: vec![format!("geoip:CN{i}")],
                ..Default::default()
            })
            .collect();
        cfg.routing = Some(RoutingConfig {
            rules,
            ..Default::default()
        });
        let violations = validate_fast_profile(&cfg);
        assert!(violations
            .iter()
            .any(|v| matches!(v, ProfileViolation::Warning(_))
                && v.message().contains("GeoSite/GeoIP")));
    }

    #[test]
    fn profile_deserialises_from_json() {
        let json = r#"{"profile": "fast"}"#;
        let m: serde_json::Value = serde_json::from_str(json).unwrap();
        let mode: ProfileMode = serde_json::from_value(m["profile"].clone()).unwrap();
        assert_eq!(mode, ProfileMode::Fast);
    }

    #[test]
    fn budget_profiles_deserialise_from_json() {
        for (raw, expected) in [
            ("latency", ProfileMode::Latency),
            ("throughput", ProfileMode::Throughput),
            ("badnet", ProfileMode::Badnet),
            ("mobile", ProfileMode::Mobile),
            ("stealth", ProfileMode::Stealth),
        ] {
            let json = format!(r#"{{"profile": "{raw}"}}"#);
            let m: serde_json::Value = serde_json::from_str(&json).unwrap();
            let mode: ProfileMode = serde_json::from_value(m["profile"].clone()).unwrap();
            assert_eq!(mode, expected);
        }
    }

    #[test]
    fn profile_defaults_to_compat() {
        let mode = ProfileMode::default();
        assert_eq!(mode, ProfileMode::Compat);
    }

    #[test]
    fn explain_cost_flags_expensive_latency_path() {
        let mut cfg = fast_vless_config();
        cfg.profile = ProfileMode::Latency;
        cfg.budget = Some(BudgetConfig::default());
        cfg.inbounds[0].sniffing = Some(SniffingConfig {
            enabled: true,
            dest_override: vec!["tls".into()],
            metadata_only: false,
            route_only: false,
        });
        cfg.inbounds[0].stream_settings = Some(StreamSettingsConfig {
            network: NetworkType::Ws,
            security: SecurityType::Reality,
            ..Default::default()
        });
        cfg.outbounds[0].protocol = Protocol::Vmess;

        let report = explain_cost(&cfg);
        let rendered = report.render_text();
        assert_eq!(report.profile, ProfileMode::Latency);
        assert_eq!(report.cost.cpu, CostClass::High);
        assert!(!report.cost.supports_direct_copy);
        assert!(rendered.contains("sniffing is enabled"));
        assert!(rendered.contains("direct copy is preferred"));
        assert!(rendered.contains("relay.engine=v2"));
    }

    #[test]
    fn explain_cost_accepts_simple_direct_path() {
        let mut cfg = fast_vless_config();
        cfg.profile = ProfileMode::Latency;
        cfg.budget = Some(BudgetConfig {
            max_protocol_layers: 8,
            ..BudgetConfig::default()
        });
        cfg.inbounds[0].stream_settings = Some(StreamSettingsConfig {
            security: SecurityType::None,
            ..Default::default()
        });
        let report = explain_cost(&cfg);
        assert_eq!(report.cost.copy_mode, CopyMode::Direct);
        assert!(report.cost.supports_splice);
        assert!(report.render_text().contains("Findings: none"));
    }
}
