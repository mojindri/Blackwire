//! Trojan protocol wiring for `instance.rs`.
//!
//! Reads Trojan-specific settings from config JSON and builds the
//! `TrojanInbound` / `TrojanOutbound` handlers.

use std::sync::Arc;

use anyhow::Result;

use blackwire_app::dns::DnsModule;
use blackwire_app::features::{InboundHandler, OutboundHandler};
use blackwire_protocol::trojan::{TrojanInbound, TrojanOutbound, TrojanOutboundConfig, TrojanUser};

use crate::net::socket_addr_from_address_port;
use crate::outbound_transport::{uses_outbound_transport, TransportTrojanOutbound};

/// Build a Trojan inbound handler from config.
pub(crate) fn build_trojan_inbound(
    cfg: &blackwire_config::schema::InboundConfig,
    dns: Option<Arc<DnsModule>>,
) -> Result<Arc<dyn InboundHandler>> {
    // Collect passwords from config JSON.
    // Expected shape: { "clients": [{ "password": "..." }, ...] }
    let clients = cfg.settings["clients"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Trojan inbound '{}' missing 'clients' array", cfg.tag))?;

    let users: Vec<TrojanUser> = clients
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let password = c["password"]
                .as_str()
                .ok_or_else(|| {
                    anyhow::anyhow!("Trojan client #{} in '{}' missing 'password'", i, cfg.tag)
                })
                .map(|s| s.to_string())?;
            let label = c
                .get("email")
                .or_else(|| c.get("name"))
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(TrojanUser { password, label })
        })
        .collect::<Result<_>>()?;

    if users.is_empty() {
        anyhow::bail!("Trojan inbound '{}' has no configured clients", cfg.tag);
    }

    Ok(TrojanInbound::new_with_users(cfg.tag.as_str(), &users, dns))
}

/// Build a Trojan outbound handler from config.
pub(crate) fn build_trojan_outbound(
    cfg: &blackwire_config::schema::OutboundConfig,
) -> Result<Arc<dyn OutboundHandler>> {
    let settings = &cfg.settings;

    let server_str = settings["address"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Trojan outbound '{}' missing 'address'", cfg.tag))?;
    let port = settings["port"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Trojan outbound '{}' missing 'port'", cfg.tag))?;
    let server = socket_addr_from_address_port(
        server_str,
        port,
        &format!("invalid Trojan server address for outbound '{}'", cfg.tag),
    )?;

    let password = settings["password"]
        .as_str()
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
