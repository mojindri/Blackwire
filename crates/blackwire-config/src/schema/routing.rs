use serde::{Deserialize, Serialize};

/// Routing configuration: rules for deciding which outbound to use.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    /// Outbound tag to use when no rule matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_strategy: Option<String>,

    /// Optional path to v2ray-compatible `geoip.dat`.
    #[serde(
        default,
        rename = "geoipFile",
        alias = "geoip_file",
        skip_serializing_if = "Option::is_none"
    )]
    pub geoip_file: Option<String>,

    /// Optional path to v2ray-compatible `geosite.dat`.
    #[serde(
        default,
        rename = "geositeFile",
        alias = "geosite_file",
        skip_serializing_if = "Option::is_none"
    )]
    pub geosite_file: Option<String>,

    /// Routing rules, evaluated in order. First match wins.
    #[serde(default)]
    pub rules: Vec<RoutingRule>,

    /// Load-balancer configurations.
    #[serde(default)]
    pub balancers: Vec<BalancerConfig>,
}

/// A single routing rule.
///
/// The rule matches only when every populated condition matches.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingRule {
    /// Rule type. Current implementation uses "field".
    #[serde(rename = "type", default = "default_rule_type")]
    pub rule_type: String,

    /// Domain matching patterns like `domain:example.com` or `suffix:example.com`.
    #[serde(default)]
    pub domain: Vec<String>,

    /// IP matching patterns like CIDR ranges or `geoip:CN`.
    #[serde(default)]
    pub ip: Vec<String>,

    /// Port matching examples: "443", "80,443", or "8000-9000".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,

    /// Only apply this rule to connections arriving on these inbound tags.
    #[serde(default, rename = "inboundTag", skip_serializing_if = "Vec::is_empty")]
    pub inbound_tag: Vec<String>,

    /// Sniffed protocol match (`http`, `tls`, …) — Xray `protocol` field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol: Vec<String>,

    /// Outbound tag to use when this rule matches.
    #[serde(rename = "outboundTag")]
    pub outbound_tag: String,
}

fn default_rule_type() -> String {
    "field".to_string()
}

/// Load balancer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalancerConfig {
    /// Unique name for this balancer.
    pub tag: String,

    /// Outbound tags this balancer distributes traffic across.
    #[serde(default)]
    pub selector: Vec<String>,

    /// Selection strategy: "random", "roundRobin", "latency", or "adaptive".
    #[serde(default = "default_balancer_strategy")]
    pub strategy: String,

    /// Optional named profiles for adaptive selection. Each profile maps to an
    /// existing outbound tag; when present, this list takes precedence over
    /// `selector` for balancer members.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<BalancerProfileConfig>,

    /// Adaptive scoring settings used when `strategy = "adaptive"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive: Option<AdaptiveBalancerConfig>,

    /// Health check settings for this balancer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check: Option<HealthCheckConfig>,
}

fn default_balancer_strategy() -> String {
    "latency".to_string()
}

/// Named adaptive profile backed by an outbound tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalancerProfileConfig {
    /// Human-readable profile name used in metrics and runtime stats.
    pub name: String,

    /// Outbound tag used when this profile is selected.
    #[serde(rename = "outboundTag", alias = "outbound_tag")]
    pub outbound_tag: String,
}

/// Conservative adaptive scoring knobs for a balancer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AdaptiveBalancerConfig {
    /// Consecutive connect/probe failures before a profile enters cooldown.
    #[serde(
        default = "default_adaptive_failure_threshold",
        rename = "failureThreshold",
        alias = "failure_threshold"
    )]
    pub failure_threshold: u32,

    /// Cooldown duration in seconds after repeated failures.
    #[serde(
        default = "default_adaptive_cooldown_secs",
        rename = "cooldownSecs",
        alias = "cooldown_secs"
    )]
    pub cooldown_secs: u64,

    /// EWMA smoothing factor for outbound connect latency.
    #[serde(
        default = "default_adaptive_ewma_alpha",
        rename = "ewmaAlpha",
        alias = "ewma_alpha"
    )]
    pub ewma_alpha: f64,

    /// Minimum score delta required before switching away from the current profile.
    #[serde(
        default = "default_adaptive_switch_margin",
        rename = "switchMargin",
        alias = "switch_margin"
    )]
    pub switch_margin: f64,
}

impl Default for AdaptiveBalancerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_adaptive_failure_threshold(),
            cooldown_secs: default_adaptive_cooldown_secs(),
            ewma_alpha: default_adaptive_ewma_alpha(),
            switch_margin: default_adaptive_switch_margin(),
        }
    }
}

fn default_adaptive_failure_threshold() -> u32 {
    2
}
fn default_adaptive_cooldown_secs() -> u64 {
    30
}
fn default_adaptive_ewma_alpha() -> f64 {
    0.2
}
fn default_adaptive_switch_margin() -> f64 {
    0.15
}

/// Health check configuration for a load balancer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// URL to check. A 204 response means the outbound is healthy.
    #[serde(default = "default_health_check_url")]
    pub url: String,

    /// How often to run health checks, in seconds.
    #[serde(default = "default_health_check_interval")]
    pub interval_secs: u64,

    /// Timeout before considering a health check failed, in seconds.
    #[serde(default = "default_health_check_timeout")]
    pub timeout_secs: u64,

    /// Consecutive failures before marking the outbound dead.
    #[serde(default = "default_max_failures")]
    pub max_failures: u32,
}

fn default_health_check_url() -> String {
    "http://www.gstatic.com/generate_204".to_string()
}
fn default_health_check_interval() -> u64 {
    30
}
fn default_health_check_timeout() -> u64 {
    5
}
fn default_max_failures() -> u32 {
    3
}
