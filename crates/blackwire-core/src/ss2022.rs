//! Shadowsocks-2022 protocol wiring for `instance.rs`.
//!
//! Reads SS-2022-specific settings from config JSON and builds the
//! `Ss2022Inbound` / `Ss2022Outbound` handlers.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use dashmap::DashMap;

use blackwire_app::features::{InboundHandler, OutboundHandler};
use blackwire_app::user_limits::UserConnectionLimiter;
use blackwire_protocol::ss2022::{
    inbound::{Ss2022AuthStore, Ss2022Inbound},
    outbound::Ss2022Outbound,
};

use crate::net::socket_addr_from_address_port;

/// Build an SS-2022 inbound handler from config.
///
/// Expected config shape:
/// ```json
/// {
///   "settings": {
///     "method": "2022-blake3-aes-256-gcm",
///     "password": "your-password"
///   }
/// }
/// ```
pub(crate) fn build_ss2022_inbound(
    cfg: &blackwire_config::schema::InboundConfig,
    auth_stores: &Arc<DashMap<String, Arc<Ss2022AuthStore>>>,
    handshake_timeout: Option<Duration>,
    user_limiter: Option<Arc<UserConnectionLimiter>>,
) -> Result<Arc<dyn InboundHandler>> {
    #[allow(clippy::unwrap_or_default)]
    let auth = auth_stores
        .entry(cfg.tag.clone())
        .or_insert_with(Ss2022AuthStore::new)
        .clone();
    populate_ss2022_auth_store(&auth, cfg)?;
    Ok(Ss2022Inbound::new_with_auth_store(
        cfg.tag.as_str(),
        auth,
        handshake_timeout,
        user_limiter,
    ))
}

pub(crate) fn populate_ss2022_auth_store(
    auth: &Ss2022AuthStore,
    cfg: &blackwire_config::schema::InboundConfig,
) -> Result<()> {
    let password = cfg
        .settings
        .password
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("SS-2022 inbound '{}' missing 'password'", cfg.tag))?
        .to_string();
    let user = ss2022_user_label(&cfg.settings, &password);
    auth.replace_password(&password, user);
    Ok(())
}

pub(crate) fn ss2022_user_label(
    settings: &blackwire_config::schema::EndpointSettings,
    password: &str,
) -> Option<String> {
    let direct = settings
        .email
        .as_deref()
        .or(settings.name.as_deref())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    if direct.is_some() {
        return direct;
    }

    settings.clients.iter().find_map(|client| {
        let client_password = client.password.as_deref()?;
        if client_password != password {
            return None;
        }
        client
            .label()
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

/// Build an SS-2022 outbound handler from config.
///
/// Expected config shape:
/// ```json
/// {
///   "settings": {
///     "address": "1.2.3.4",
///     "port": 8388,
///     "method": "2022-blake3-aes-256-gcm",
///     "password": "your-password"
///   }
/// }
/// ```
pub(crate) fn build_ss2022_outbound(
    cfg: &blackwire_config::schema::OutboundConfig,
) -> Result<Arc<dyn OutboundHandler>> {
    let settings = &cfg.settings;

    let server_str = settings
        .address
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("SS-2022 outbound '{}' missing 'address'", cfg.tag))?;
    let port = settings
        .port
        .map(u64::from)
        .ok_or_else(|| anyhow::anyhow!("SS-2022 outbound '{}' missing 'port'", cfg.tag))?;
    let server = socket_addr_from_address_port(
        server_str,
        port,
        &format!("invalid SS-2022 server address for outbound '{}'", cfg.tag),
    )?;

    let password = settings
        .password
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("SS-2022 outbound '{}' missing 'password'", cfg.tag))?
        .to_string();

    Ok(Ss2022Outbound::new(&cfg.tag, server, &password))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blackwire_config::schema::EndpointSettings;
    use serde_json::json;

    fn settings(value: serde_json::Value) -> EndpointSettings {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn ss2022_user_label_prefers_direct_label() {
        let settings = settings(json!({
            "password": "secret",
            "email": "ss@example.local",
            "clients": [{"password": "secret", "email": "client@example.local"}]
        }));

        assert_eq!(
            ss2022_user_label(&settings, "secret").as_deref(),
            Some("ss@example.local")
        );
    }

    #[test]
    fn ss2022_user_label_matches_client_password() {
        let settings = settings(json!({
            "clients": [
                {"password": "other", "email": "other@example.local"},
                {"password": "secret", "name": "ss-user"}
            ]
        }));

        assert_eq!(
            ss2022_user_label(&settings, "secret").as_deref(),
            Some("ss-user")
        );
    }
}
