//! Active connection manager and observability.

pub mod commands;
pub mod manager;
pub mod meta;
pub mod metrics;

pub use commands::{CloseSelector, ConnectionCommandResult};
pub use manager::{global_manager, ConnectionGuard, ConnectionManager};
pub use meta::{CloseReason, ConnectionMeta, ConnectionSnapshot, Protocol, RelayPath, Transport};
