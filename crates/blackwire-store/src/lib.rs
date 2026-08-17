//! MySQL-only persistence for Blackwire's configuration control plane.
//!
//! This crate is the sole owner of production configuration SQL. Runtime,
//! command-line, and panel code consume typed snapshots and revision results
//! instead of reading configuration files or issuing ad-hoc queries.

mod connection;
mod error;
mod panel;
mod resources;
mod revision;
mod routing;
mod runtime;
mod snapshot;

pub use connection::{Database, DatabaseOptions, EXPECTED_SCHEMA_VERSION};
pub use error::{StoreError, StoreResult};
pub use panel::{AdminRecord, PanelSettings};
pub use resources::{
    InboundRecord, InboundWrite, OutboundRecord, OutboundWrite, SubscriptionRecord, UserRecord,
    UserWrite,
};
pub use revision::{
    ActivationClass, ActivationState, ConfigurationState, MutationResult, Revision, RevisionSummary,
};
pub use routing::{RouteWrite, RoutingDnsRecord, RoutingDnsWrite};
pub use runtime::{InboundTrafficRecord, UserTrafficRecord};
pub use snapshot::StoredConfig;
