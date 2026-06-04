//! Active connection manager and observability.

/// Commands for closing active connections by selector.
pub mod commands;
/// Global connection manager and per-connection guard.
pub mod manager;
/// Connection metadata types: Protocol, Transport, CloseReason, ConnectionMeta, ConnectionSnapshot.
pub mod meta;
/// Metrics helpers for connection lifecycle events.
pub mod metrics;

pub use commands::{CloseSelector, ConnectionCommandResult};
pub use manager::{global_manager, ConnectionGuard, ConnectionManager};
pub use meta::{CloseReason, ConnectionMeta, ConnectionSnapshot, Protocol, RelayPath, Transport};
