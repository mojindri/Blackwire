//! Trojan protocol wiring for `instance.rs`.
//!
//! Reads Trojan-specific settings from config JSON and builds the
//! `TrojanInbound` / `TrojanOutbound` handlers.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use dashmap::DashMap;

use blackwire_app::dns::DnsModule;
use blackwire_app::features::{InboundHandler, OutboundHandler};
use blackwire_app::user_limits::UserConnectionLimiter;
use blackwire_protocol::trojan::{
    inbound::TrojanAuthStore, TrojanInbound, TrojanOutbound, TrojanOutboundConfig, TrojanUser,
};

use crate::net::socket_addr_from_address_port;
use crate::outbound_transport::{uses_outbound_transport, TransportTrojanOutbound};

/// Build a Trojan inbound handler from config.
pub(crate) fn build_trojan_inbound(
    cfg: &blackwire_config::schema::InboundConfig,
    auth_stores: &Arc<DashMap<String, Arc<TrojanAuthStore>>>,
    dns: Option<Arc<DnsModule>>,
    handshake_timeout: Option<Duration>,
    user_limiter: Option<Arc<UserConnectionLimiter>>,
) -> Result<Arc<dyn InboundHandler>> {
    #[allow(clippy::unwrap_or_default)]
    let auth = auth_stores
        .entry(cfg.tag.clone())
        .or_insert_with(TrojanAuthStore::new)
        .clone();
    populate_trojan_auth_store(&auth, cfg)?;

    Ok(TrojanInbound::new_with_auth_store(
        cfg.tag.as_str(),
        auth,
        dns,
        handshake_timeout,
        user_limiter,
    ))
}

pub(crate) fn populate_trojan_auth_store(
    auth: &TrojanAuthStore,
    cfg: &blackwire_config::schema::InboundConfig,
) -> Result<()> {
    let clients = &cfg.settings.clients;

    let users: Vec<TrojanUser> = clients
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let password = c
                .password
                .as_deref()
                .ok_or_else(|| {
                    anyhow::anyhow!("Trojan client #{} in '{}' missing 'password'", i, cfg.tag)
                })
                .map(|s| s.to_string())?;
            let label = c.label().map(ToOwned::to_owned);
            Ok(TrojanUser { password, label })
        })
        .collect::<Result<_>>()?;

    if users.is_empty() {
        anyhow::bail!("Trojan inbound '{}' has no configured clients", cfg.tag);
    }

    auth.replace_users(&users);
    Ok(())
}

/// Build a Trojan outbound handler from config.
pub(crate) fn build_trojan_outbound(
    cfg: &blackwire_config::schema::OutboundConfig,
) -> Result<Arc<dyn OutboundHandler>> {
    let settings = &cfg.settings;

    let server_str = settings
        .address
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Trojan outbound '{}' missing 'address'", cfg.tag))?;
    let port = settings
        .port
        .map(u64::from)
        .ok_or_else(|| anyhow::anyhow!("Trojan outbound '{}' missing 'port'", cfg.tag))?;
    let server = socket_addr_from_address_port(
        server_str,
        port,
        &format!("invalid Trojan server address for outbound '{}'", cfg.tag),
    )?;

    let password = settings
        .password
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Trojan outbound '{}' missing 'password'", cfg.tag))?
        .to_string();

    if uses_outbound_transport(&cfg.stream_settings) {
        Ok(TransportTrojanOutbound::new(
            &cfg.tag,
            server,
            password,
            cfg.stream_settings.clone(),
        ))
    } else {
        Ok(TrojanOutbound::new(
            &cfg.tag,
            TrojanOutboundConfig { server, password },
        ))
    }
}
