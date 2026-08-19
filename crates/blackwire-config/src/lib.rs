//! Typed Blackwire runtime configuration reconstructed by `blackwire-store`
//! from normalized MySQL revisions. This crate contains no persistence or
//! configuration-file lifecycle.

pub mod schema;

pub use schema::{
    Config, CostReport, DatagramConfig, FecConfig, FecMode, Hysteria2Config, InboundConfig,
    LogConfig, NetworkType, OutboundConfig, Protocol, QuicConfig, SecurityType,
    StreamSettingsConfig,
};
