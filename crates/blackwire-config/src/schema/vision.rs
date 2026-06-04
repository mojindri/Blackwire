use serde::{Deserialize, Serialize};

/// Vision direct-copy policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisionDirectCopyPolicy {
    /// Enable direct-copy lowering when the stream state proves it is safe.
    #[default]
    Auto,
    /// Keep Vision processing on the wrapped userspace relay path.
    Disabled,
    /// Require direct-copy lowering for eligible Vision streams.
    Require,
}

/// XTLS Vision optimization settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisionConfig {
    /// Direct-copy lowering policy.
    #[serde(default)]
    pub direct_copy: VisionDirectCopyPolicy,

    /// Maximum early packets to filter before falling back to wrapped relay.
    #[serde(default = "VisionConfig::default_max_packets_to_filter")]
    pub max_packets_to_filter: u8,

    /// Permit Linux splice after Vision has lowered both sides to raw TCP.
    #[serde(default = "VisionConfig::default_allow_splice_after_direct")]
    pub allow_splice_after_direct: bool,
}

impl VisionConfig {
    fn default_max_packets_to_filter() -> u8 {
        8
    }

    fn default_allow_splice_after_direct() -> bool {
        true
    }
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            direct_copy: VisionDirectCopyPolicy::Auto,
            max_packets_to_filter: Self::default_max_packets_to_filter(),
            allow_splice_after_direct: Self::default_allow_splice_after_direct(),
        }
    }
}
