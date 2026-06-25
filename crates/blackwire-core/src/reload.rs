//! Hot-reload helpers — update routing, inbound auth state, per-user limits, and bandwidth policy
//! without restarting listeners.
//!
//! # What gets hot-reloaded?
//!
//! When an operator edits `config.json` on disk, blackwire can pick up some
//! changes **without** dropping live connections or rebinding ports:
//!
//!   - **Routing rules** — which outbound each destination uses
//!   - **GeoIP / geosite matchers** — country and domain lists
//!   - **Inbound auth state** — VLESS, VMess, Trojan, SS-2022, Hysteria2, and TUIC user material
//!   - **Per-user connection cap** — `limits.maxConnectionsPerUser`
//!   - **Per-user bandwidth policy** — `upMbps` / `downMbps` fields on client entries
//!
//! # What does NOT hot-reload (yet)?
//!
//! These require a process restart because they are wired at startup:
//!
//!   - Inbound listen addresses / ports
//!   - Outbound server addresses
//!   - TLS / REALITY key material on existing listeners
//!   - New inbound or outbound tags (handlers are not created on the fly)
//!
//! # How it works
//!
//! 1. `ConfigManager::watch()` detects the file change and validates the new JSON.
//! 2. If valid, it stores the new config and pings subscribers via `subscribe()`.
//! 3. `blackwire run` listens on that channel and calls `ReloadState::apply()`.
//! 4. `apply()` atomically swaps the router (`LiveRouter::swap`) and refreshes
//!    each supported inbound auth store in place. Connections already in flight keep using
//!    the router snapshot they picked up at dispatch time; new connections see
//!    the updated rules and UUID lists immediately.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use arc_swap::ArcSwap;
use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::Mutex;
use serde_json::Value;
use tracing::info;

use blackwire_app::geo::{GeoIpMatcher, GeoSiteMatcher};
use blackwire_app::router::LiveRouter;
use blackwire_app::user_limits::UserConnectionLimiter;
use blackwire_app::{set_user_bandwidth_policies, set_user_bandwidth_policy};
use blackwire_config::schema::{Config, Protocol};
use blackwire_protocol::ss2022::inbound::Ss2022AuthStore;
use blackwire_protocol::trojan::inbound::TrojanAuthStore;
use blackwire_protocol::vless::VlessUserRegistry;
use blackwire_protocol::vmess::VmessUserRegistry;
use blackwire_transport::{Hysteria2AuthStore, TuicAuthStore};

use crate::instance::{
    build_rules, build_sniffing_map, build_user_bandwidth_policies, load_geo_data,
    populate_ss2022_auth_store, populate_trojan_auth_store, populate_vless_registry,
    populate_vmess_registry,
};

/// Cached geo data: skip rebuilding matchers when the file hasn't changed.
#[derive(Default)]
struct GeoCache {
    geoip_path: Option<String>,
    geoip_fingerprint: Option<(u64, SystemTime)>,
    geoip: HashMap<String, GeoIpMatcher>,

    geosite_path: Option<String>,
    geosite_fingerprint: Option<(u64, SystemTime)>,
    geosite: HashMap<String, GeoSiteMatcher>,
}

fn file_fingerprint(path: &str) -> Option<(u64, SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// Shared reload handles created at startup and updated on each config reload.
///
/// Clone this cheaply — it only bumps reference counts on the inner `Arc`s.
#[derive(Clone)]
pub struct ReloadState {
    /// Live routing table. Swapped atomically via `LiveRouter::swap`.
    pub router: Arc<LiveRouter>,
    /// One VLESS user registry per inbound tag (key = inbound `tag`).
    pub vless_registries: Arc<DashMap<String, Arc<VlessUserRegistry>>>,
    /// One VMess user registry per inbound tag (key = inbound `tag`).
    pub vmess_registries: Arc<DashMap<String, Arc<VmessUserRegistry>>>,
    /// One Trojan auth store per inbound tag.
    pub trojan_auth_stores: Arc<DashMap<String, Arc<TrojanAuthStore>>>,
    /// One SS-2022 auth store per inbound tag.
    pub ss2022_auth_stores: Arc<DashMap<String, Arc<Ss2022AuthStore>>>,
    /// One Hysteria2 auth store per inbound tag.
    pub hysteria2_auth_stores: Arc<DashMap<String, Arc<Hysteria2AuthStore>>>,
    /// One TUIC auth store per inbound tag.
    pub tuic_auth_stores: Arc<DashMap<String, Arc<TuicAuthStore>>>,
    /// Per-inbound sniffing map (hot-swapped on reload via lock-free ArcSwap).
    pub sniffing: Arc<
        ArcSwap<std::collections::HashMap<String, Arc<blackwire_config::schema::SniffingConfig>>>,
    >,
    /// Shared per-user connection limiter updated in place on reload.
    pub user_connection_limiter: Arc<UserConnectionLimiter>,
    /// Inbound tags from the active config (HandlerService ListInbounds).
    pub inbound_tags: Arc<std::sync::RwLock<Vec<String>>>,
    /// Outbound tags from the active config (HandlerService ListOutbounds).
    pub outbound_tags: Arc<std::sync::RwLock<Vec<String>>>,
    /// Cached geo matchers; skips file re-read when path and mtime are unchanged.
    geo_cache: Arc<Mutex<GeoCache>>,
}

impl ReloadState {
    /// Create a new `ReloadState` with the given router, registries and sniffing map.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        router: Arc<LiveRouter>,
        vless_registries: Arc<DashMap<String, Arc<VlessUserRegistry>>>,
        vmess_registries: Arc<DashMap<String, Arc<VmessUserRegistry>>>,
        trojan_auth_stores: Arc<DashMap<String, Arc<TrojanAuthStore>>>,
        ss2022_auth_stores: Arc<DashMap<String, Arc<Ss2022AuthStore>>>,
        hysteria2_auth_stores: Arc<DashMap<String, Arc<Hysteria2AuthStore>>>,
        tuic_auth_stores: Arc<DashMap<String, Arc<TuicAuthStore>>>,
        sniffing: Arc<
            ArcSwap<
                std::collections::HashMap<String, Arc<blackwire_config::schema::SniffingConfig>>,
            >,
        >,
        inbound_tags: Arc<std::sync::RwLock<Vec<String>>>,
        outbound_tags: Arc<std::sync::RwLock<Vec<String>>>,
        user_connection_limiter: Arc<UserConnectionLimiter>,
    ) -> Self {
        Self {
            router,
            vless_registries,
            vmess_registries,
            trojan_auth_stores,
            ss2022_auth_stores,
            hysteria2_auth_stores,
            tuic_auth_stores,
            sniffing,
            inbound_tags,
            outbound_tags,
            user_connection_limiter,
            geo_cache: Arc::new(Mutex::new(GeoCache::default())),
        }
    }

    /// Apply routing rules and reloadable inbound auth state from a freshly validated config.
    ///
    /// Inbound listeners and outbound handlers are not recreated here — only data
    /// consulted per connection (router + auth stores + user limit/bandwidth state) is refreshed.
    pub fn apply(&self, config: &Config) -> Result<()> {
        let outbound_tags = collect_outbound_tags(config);
        let default_tag = config
            .outbounds
            .first()
            .map(|o| o.tag.as_str())
            .unwrap_or("direct");

        let rules = if let Some(routing) = &config.routing {
            build_rules(&routing.rules, &outbound_tags)?
        } else {
            Vec::new()
        };

        let (geoip, geosite) = self.load_geo_data_cached(config);
        let domain_strategy = config
            .routing
            .as_ref()
            .and_then(|r| r.domain_strategy.clone());
        self.router
            .swap(rules, default_tag, geoip, geosite, domain_strategy);
        info!("routing rules hot-swapped");

        let new_sniffing = build_sniffing_map(&config.inbounds);
        let count = new_sniffing.len();
        self.sniffing.store(Arc::new(new_sniffing));
        info!(count, "sniffing map hot-swapped");

        if let Ok(mut tags) = self.inbound_tags.write() {
            let mut next = Vec::with_capacity(config.inbounds.len());
            next.extend(config.inbounds.iter().map(|i| i.tag.clone()));
            *tags = next;
        }
        if let Ok(mut tags) = self.outbound_tags.write() {
            let mut next = Vec::with_capacity(config.outbounds.len());
            next.extend(config.outbounds.iter().map(|o| o.tag.clone()));
            *tags = next;
        }

        set_user_bandwidth_policies(build_user_bandwidth_policies(&config.inbounds));
        info!("per-user bandwidth policy hot-swapped");
        self.user_connection_limiter.set_max_connections_per_user(
            config.limits.max_connections_per_user.unwrap_or(usize::MAX),
        );
        info!(
            max = self.user_connection_limiter.max_connections_per_user(),
            "per-user connection cap hot-swapped"
        );

        for in_cfg in &config.inbounds {
            match in_cfg.protocol {
                Protocol::Vless => {
                    if let Some(registry) = self.vless_registries.get(&in_cfg.tag) {
                        populate_vless_registry(&registry, in_cfg)?;
                        info!(
                            tag = %in_cfg.tag,
                            users = registry.len(),
                            "VLESS user registry refreshed"
                        );
                    }
                }
                Protocol::Vmess => {
                    if let Some(registry) = self.vmess_registries.get(&in_cfg.tag) {
                        populate_vmess_registry(&registry, in_cfg)?;
                        info!(
                            tag = %in_cfg.tag,
                            users = registry.len(),
                            "VMess user registry refreshed"
                        );
                    }
                }
                Protocol::Trojan => {
                    if let Some(auth) = self.trojan_auth_stores.get(&in_cfg.tag) {
                        populate_trojan_auth_store(&auth, in_cfg)?;
                        info!(
                            tag = %in_cfg.tag,
                            users = auth.len(),
                            "Trojan auth store refreshed"
                        );
                    }
                }
                Protocol::Shadowsocks => {
                    if let Some(auth) = self.ss2022_auth_stores.get(&in_cfg.tag) {
                        populate_ss2022_auth_store(&auth, in_cfg)?;
                        info!(tag = %in_cfg.tag, "SS-2022 auth store refreshed");
                    }
                }
                Protocol::Hysteria2 => {
                    if let Some(auth) = self.hysteria2_auth_stores.get(&in_cfg.tag) {
                        let settings = &in_cfg.settings;
                        let password = settings["auth"].as_str().unwrap_or_default().to_string();
                        let user = crate::hysteria2::hysteria2_user_label(settings, &password);
                        auth.replace(password, user);
                        info!(tag = %in_cfg.tag, "Hysteria2 auth store refreshed");
                    }
                }
                Protocol::Tuic => {
                    if let Some(auth) = self.tuic_auth_stores.get(&in_cfg.tag) {
                        let users = crate::tuic::parse_users(&in_cfg.settings)?;
                        auth.replace_users(users);
                        info!(tag = %in_cfg.tag, "TUIC auth store refreshed");
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[async_trait]
impl blackwire_api::management::InboundManagement for ReloadState {
    async fn list_inbound_tags(&self) -> Vec<String> {
        self.inbound_tags
            .read()
            .map(|t| t.clone())
            .unwrap_or_default()
    }

    async fn list_outbound_tags(&self) -> Vec<String> {
        self.outbound_tags
            .read()
            .map(|t| t.clone())
            .unwrap_or_default()
    }

    async fn vless_user_count(&self, inbound_tag: &str) -> Option<i64> {
        self.vless_registry(inbound_tag).map(|r| r.len() as i64)
    }

    async fn list_vless_users(
        &self,
        inbound_tag: &str,
        email: &str,
    ) -> Result<Vec<blackwire_api::management::VlessUserRecord>, String> {
        let registry = self
            .vless_registry(inbound_tag)
            .ok_or_else(|| format!("inbound '{inbound_tag}' has no VLESS user registry"))?;
        Ok(registry
            .list_users(email)
            .into_iter()
            .map(|u| blackwire_api::management::VlessUserRecord {
                email: u.email.to_string(),
                uuid: uuid::Uuid::from_bytes(u.uuid).to_string(),
                flow: u.flow.clone(),
                level: 0,
            })
            .collect())
    }

    async fn add_vless_user(
        &self,
        inbound_tag: &str,
        email: &str,
        uuid_str: &str,
        flow: &str,
    ) -> Result<(), String> {
        let registry = self
            .vless_registry(inbound_tag)
            .ok_or_else(|| format!("inbound '{inbound_tag}' has no VLESS user registry"))?;
        let uuid = crate::instance::parse_uuid(uuid_str).map_err(|e| e.to_string())?;
        registry.add_user(blackwire_protocol::vless::VlessUser {
            email: email.into(),
            uuid,
            flow: flow.to_string(),
        });
        set_user_bandwidth_policy(Arc::<str>::from(email), None);
        Ok(())
    }

    async fn remove_vless_user(&self, inbound_tag: &str, email: &str) -> Result<(), String> {
        let registry = self
            .vless_registry(inbound_tag)
            .ok_or_else(|| format!("inbound '{inbound_tag}' has no VLESS user registry"))?;
        if registry.remove_user_by_email(email) {
            set_user_bandwidth_policy(Arc::<str>::from(email), None);
            Ok(())
        } else {
            Err(format!(
                "no VLESS user with email '{email}' on inbound '{inbound_tag}'"
            ))
        }
    }

    async fn list_connections(&self) -> Vec<blackwire_connmgr::ConnectionSnapshot> {
        blackwire_connmgr::global_manager().list()
    }

    async fn close_connections(
        &self,
        selector: blackwire_connmgr::CloseSelector,
    ) -> Result<usize, String> {
        Ok(blackwire_connmgr::global_manager().close(selector).matched)
    }
}

impl ReloadState {
    fn vless_registry(&self, inbound_tag: &str) -> Option<Arc<VlessUserRegistry>> {
        if !self
            .inbound_tags
            .read()
            .map(|tags| tags.iter().any(|t| t == inbound_tag))
            .unwrap_or(false)
        {
            return None;
        }
        self.vless_registries
            .get(inbound_tag)
            .map(|r| Arc::clone(r.value()))
    }

    /// Load geo data, reusing the cached matchers when the files haven't changed.
    ///
    /// Checks file size + mtime before re-reading. The expensive part (protobuf
    /// decode + AhoCorasick/regex compilation) is only done when a file changes.
    fn load_geo_data_cached(
        &self,
        config: &Config,
    ) -> (
        HashMap<String, GeoIpMatcher>,
        HashMap<String, GeoSiteMatcher>,
    ) {
        let routing = config.routing.as_ref();
        let geoip_path = routing
            .and_then(|r| r.geoip_file.as_deref())
            .map(str::to_owned);
        let geosite_path = routing
            .and_then(|r| r.geosite_file.as_deref())
            .map(str::to_owned);

        let geoip_fp = geoip_path.as_deref().and_then(file_fingerprint);
        let geosite_fp = geosite_path.as_deref().and_then(file_fingerprint);

        let mut cache = self.geo_cache.lock();

        let geoip_hit = geoip_path == cache.geoip_path
            && (geoip_fp.is_some() && geoip_fp == cache.geoip_fingerprint || geoip_path.is_none());
        let geosite_hit = geosite_path == cache.geosite_path
            && (geosite_fp.is_some() && geosite_fp == cache.geosite_fingerprint
                || geosite_path.is_none());

        // Load from disk only when at least one file needs rebuilding.
        let (fresh_ip, fresh_site) = if !geoip_hit || !geosite_hit {
            load_geo_data(routing)
        } else {
            (HashMap::new(), HashMap::new())
        };

        let geoip = if geoip_hit {
            info!("geo: geoip.dat unchanged; reusing cached matchers");
            cache.geoip.clone()
        } else {
            cache.geoip_fingerprint = geoip_fp;
            cache.geoip_path = geoip_path;
            let cloned = fresh_ip.clone();
            cache.geoip = fresh_ip;
            cloned
        };

        let geosite = if geosite_hit {
            info!("geo: geosite.dat unchanged; reusing cached matchers");
            cache.geosite.clone()
        } else {
            cache.geosite_fingerprint = geosite_fp;
            cache.geosite_path = geosite_path;
            let cloned = fresh_site.clone();
            cache.geosite = fresh_site;
            cloned
        };

        (geoip, geosite)
    }
}

/// Returns inbound tags whose listen address/port changed (requires process restart).
///
/// Matches Xray behavior: listener sockets are not recreated on `reload`.
pub fn inbound_listener_changes(old: &Config, new: &Config) -> Vec<String> {
    let mut changed = Vec::new();
    for new_in in &new.inbounds {
        let Some(old_in) = old.inbounds.iter().find(|i| i.tag == new_in.tag) else {
            changed.push(new_in.tag.clone());
            continue;
        };
        if old_in.listen != new_in.listen || old_in.port != new_in.port {
            changed.push(new_in.tag.clone());
        }
    }
    for new_in in &new.inbounds {
        if !old.inbounds.iter().any(|i| i.tag == new_in.tag) {
            changed.push(new_in.tag.clone());
        }
    }
    changed
}

/// Returns `true` when a validated config change requires rebuilding the running instance.
///
/// Routing, DNS, sniffing, and supported inbound auth lists are hot-swappable via [`ReloadState::apply`].
/// Structural changes such as listeners, transport wrappers, and outbound definitions
/// need a fresh `Instance` because the handler graph is built at startup.
pub fn requires_instance_restart(old: &Config, new: &Config) -> bool {
    if !inbound_listener_changes(old, new).is_empty() {
        return true;
    }

    if old.metrics_addr != new.metrics_addr
        || old.api != new.api
        || old.quic != new.quic
        || old.datagram != new.datagram
        || old.fec != new.fec
    {
        return true;
    }

    if normalized_limits_value(&old.limits) != normalized_limits_value(&new.limits) {
        return true;
    }

    match (
        serde_json::to_value(&old.tun),
        serde_json::to_value(&new.tun),
    ) {
        (Ok(a), Ok(b)) if a != b => return true,
        (Err(_), _) | (_, Err(_)) => return true,
        _ => {}
    }

    match (
        serde_json::to_value(&old.outbounds),
        serde_json::to_value(&new.outbounds),
    ) {
        (Ok(a), Ok(b)) if a != b => return true,
        (Err(_), _) | (_, Err(_)) => return true,
        _ => {}
    }

    if old.inbounds.len() != new.inbounds.len() {
        return true;
    }

    for new_in in &new.inbounds {
        let Some(old_in) = old.inbounds.iter().find(|i| i.tag == new_in.tag) else {
            return true;
        };
        if normalized_inbound_value(old_in) != normalized_inbound_value(new_in) {
            return true;
        }
    }

    false
}

fn normalized_limits_value(limits: &blackwire_config::schema::LimitsConfig) -> Value {
    let mut value = serde_json::to_value(limits).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.remove("maxConnectionsPerUser");
        obj.remove("max_connections_per_user");
    }
    value
}

fn normalized_inbound_value(inbound: &blackwire_config::schema::InboundConfig) -> Value {
    let mut value = serde_json::to_value(inbound).unwrap_or(Value::Null);
    let Some(obj) = value.as_object_mut() else {
        return value;
    };

    // Sniffing is hot-swapped separately and supported inbound auth material is
    // refreshed in place.
    obj.remove("sniffing");
    if let Some(settings) = obj.get_mut("settings").and_then(|v| v.as_object_mut()) {
        match inbound.protocol {
            Protocol::Vless | Protocol::Vmess | Protocol::Trojan => {
                settings.remove("clients");
            }
            Protocol::Shadowsocks => {
                settings.remove("password");
                settings.remove("email");
                settings.remove("name");
                settings.remove("clients");
            }
            Protocol::Hysteria2 => {
                settings.remove("auth");
                settings.remove("password");
                settings.remove("clients");
            }
            Protocol::Tuic => {
                settings.remove("users");
                settings.remove("uuid");
                settings.remove("id");
                settings.remove("password");
                settings.remove("email");
                settings.remove("name");
            }
            _ => {}
        }
        if inbound.protocol != Protocol::Vless
            && inbound.protocol != Protocol::Vmess
            && inbound.protocol != Protocol::Trojan
        {
            strip_client_bandwidth_fields(settings.get_mut("clients"));
            strip_client_bandwidth_fields(settings.get_mut("users"));
        } else {
            strip_client_bandwidth_fields(settings.get_mut("clients"));
        }
    }

    value
}

fn strip_client_bandwidth_fields(value: Option<&mut Value>) {
    let Some(Value::Array(entries)) = value else {
        return;
    };
    for entry in entries {
        let Some(obj) = entry.as_object_mut() else {
            continue;
        };
        for key in [
            "upMbps",
            "up_mbps",
            "uploadMbps",
            "upload_mbps",
            "downMbps",
            "down_mbps",
            "downloadMbps",
            "download_mbps",
        ] {
            obj.remove(key);
        }
    }
}

/// Collect every outbound tag referenced in the config so routing rules can be validated.
fn collect_outbound_tags(config: &Config) -> HashSet<String> {
    let mut tags: HashSet<String> = config.outbounds.iter().map(|o| o.tag.clone()).collect();
    if let Some(routing) = &config.routing {
        for balancer in &routing.balancers {
            tags.insert(balancer.tag.clone());
        }
    }
    tags
}
