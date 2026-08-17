//! Runtime management surface for Xray-compatible HandlerService gRPC.

use std::sync::Arc;

use async_trait::async_trait;
use blackwire_connmgr::{CloseSelector, ConnectionSnapshot};

/// VLESS user row returned by HandlerService queries.
#[derive(Debug, Clone)]
pub struct VlessUserRecord {
    /// Panel email / identifier.
    pub email: String,
    /// VLESS UUID string.
    pub uuid: String,
    /// VLESS flow (e.g. `xtls-rprx-vision`).
    pub flow: String,
    /// User level from Handler `User` message.
    pub level: u32,
}

/// Snapshot of inbound/outbound tags and VLESS user management for the API layer.
#[async_trait]
pub trait InboundManagement: Send + Sync {
    /// Tags of inbounds in the active config.
    async fn list_inbound_tags(&self) -> Vec<String>;
    /// Tags of outbounds in the active config.
    async fn list_outbound_tags(&self) -> Vec<String>;
    /// VLESS user count for an inbound tag, or `None` if the tag is unknown.
    async fn vless_user_count(&self, inbound_tag: &str) -> Option<i64>;
    /// List VLESS users on an inbound (optional email filter).
    async fn list_vless_users(
        &self,
        inbound_tag: &str,
        email: &str,
    ) -> Result<Vec<VlessUserRecord>, String>;
    /// List active managed connections.
    async fn list_connections(&self) -> Vec<ConnectionSnapshot> {
        Vec::new()
    }

    /// Close managed connections matching the selector.
    async fn close_connections(&self, _selector: CloseSelector) -> Result<usize, String> {
        Err("CloseConnections is not available from this management handle".into())
    }
}

/// Shared handle passed into [`crate::server::start_api_server`].
pub type ManagementHandle = Arc<dyn InboundManagement>;
