#![allow(
    missing_docs,
    reason = "database record fields and query methods mirror the relational schema"
)]

//! MySQL-only persistence for Blackwire's configuration control plane.
//!
//! This crate is the sole owner of production configuration SQL. Runtime,
//! command-line, and panel code consume typed snapshots and revision results
//! instead of reading configuration files or issuing ad-hoc queries.

mod connection;
mod core_settings;
mod error;
mod panel;
mod resources;
mod revision;
mod routing;
mod runtime;
mod snapshot;

// Depend on the MySQL driver crates directly. The public `sqlx` facade records
// every optional database backend in Cargo.lock even when only MySQL is built.
mod sqlx {
    pub(crate) use sqlx_core::error::Error;
    pub(crate) use sqlx_core::executor::Executor;
    pub(crate) use sqlx_core::pool;
    pub(crate) use sqlx_core::query::query;
    pub(crate) use sqlx_core::query_scalar::query_scalar;
    pub(crate) use sqlx_core::row::Row;
    pub(crate) use sqlx_core::transaction::Transaction;
    pub(crate) use sqlx_mysql::{MySql, MySqlPool};

    pub(crate) mod mysql {
        pub(crate) use sqlx_mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow};
    }
}

pub use connection::{Database, DatabaseOptions, EXPECTED_SCHEMA_VERSION};
pub use core_settings::CoreSettings;
pub use error::{StoreError, StoreResult};
pub use panel::{AdminRecord, PanelSettings};
pub use resources::{
    InboundRecord, InboundWrite, OutboundRecord, OutboundWrite, SubscriptionRecord, UserRecord,
    UserWrite,
};
pub use revision::{
    ActivationClass, ActivationState, ConfigurationState, MutationResult, Revision, RevisionSummary,
};
pub use routing::{
    AdaptiveBalancerWrite, BalancerMemberWrite, BalancerWrite, HealthCheckWrite, RouteWrite,
    RoutingDnsRecord, RoutingDnsWrite,
};
pub use runtime::{InboundTrafficRecord, UserTrafficRecord};
pub use snapshot::StoredConfig;
