//! v2ray-compatible StatsService (Phase 7 — wiring stub).
//!
//! Wire format and RPC names follow Xray-core `app/stats/command`.

/// gRPC package path used by v2ray panels (placeholder until prost stubs land).
pub const STATS_SERVICE_NAME: &str = "v2ray.core.app.stats.command.StatsService";

/// Handler API package path (placeholder).
pub const HANDLER_SERVICE_NAME: &str = "v2ray.core.app.proxyman.command.HandlerService";

/// Whether the management API is enabled in the running process.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiStatus {
    pub enabled: bool,
}

impl ApiStatus {
    pub fn disabled() -> Self {
        Self { enabled: false }
    }
}
