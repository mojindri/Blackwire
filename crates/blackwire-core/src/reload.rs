//! Hot-reload helpers — update routing, inbound auth state, per-user limits, and bandwidth policy
//! without restarting listeners.
//!
//! # What gets hot-reloaded?
//!
//! When an operator commits a desired MySQL revision, Blackwire can apply some
//! changes **without** dropping live connections or rebinding ports:
//!
//!   - **Routing rules** — which outbound each destination uses
//!   - **GeoIP / geosite matchers** — country and domain lists
//!   - **Inbound auth state** — VLESS, VMess, Trojan, SS-2022, Hysteria2, and TUIC user material
//!   - **Per-user connection cap** — `limits.maxConnectionsPerUser`
//!   - **Per-user bandwidth policy** — `upMbps` / `downMbps` fields on client entries
//!
//! # What uses prepared handover?
//!
//! Structural settings are applied automatically by building a replacement
//! instance inside the running process:
//!
//!   - Inbound listen addresses / ports
//!   - Outbound server addresses
//!   - TLS / REALITY key material on existing listeners
//!   - New inbound or outbound tags
//!
//! # How it works
//!
//! 1. The reconciler detects and reconstructs the desired relational revision.
//! 2. It validates the typed configuration before activation.
//! 3. `blackwire run` calls `ReloadState::apply()` for hot-swappable state.
//! 4. `apply()` atomically swaps the router (`LiveRouter::swap`) and refreshes
//!    each supported inbound auth store in place. Connections already in flight keep using
//!    the router snapshot they picked up at dispatch time; new connections see
//!    the updated rules and UUID lists immediately.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use parking_lot::Mutex;
use tracing::info;

use blackwire_app::geo::{GeoIpMatcher, GeoSiteMatcher};
use blackwire_app::router::LiveRouter;
use blackwire_app::set_user_bandwidth_policies;
use blackwire_app::user_limits::UserConnectionLimiter;
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
                        let password = settings.auth.clone().unwrap_or_default();
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

impl ReloadState {
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

/// Returns inbound tags whose listen address/port changed and need instance handover.
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
    for old_in in &old.inbounds {
        if !new.inbounds.iter().any(|i| i.tag == old_in.tag) {
            changed.push(old_in.tag.clone());
        }
    }
    changed
}

/// Returns `true` when a validated config change needs a prepared in-process handover.
///
/// Routing, DNS, sniffing, and supported inbound auth lists are hot-swappable via [`ReloadState::apply`].
/// Structural changes such as listeners, transport wrappers, and outbound definitions
/// use a fresh `Instance` because the handler graph is built at startup. The runtime
/// prepares it before atomically swapping, without restarting the process.
pub fn requires_instance_handover(old: &Config, new: &Config) -> bool {
    if !inbound_listener_changes(old, new).is_empty() {
        return true;
    }

    if old.metrics_addr != new.metrics_addr
        || old.profile != new.profile
        || serialized_value_changed(&old.fast, &new.fast)
        || old.vision != new.vision
        || old.first_packet_boost != new.first_packet_boost
        || serialized_value_changed(&old.log, &new.log)
        || old.stats != new.stats
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

fn serialized_value_changed<T: serde::Serialize>(old: &T, new: &T) -> bool {
    match (serde_json::to_value(old), serde_json::to_value(new)) {
        (Ok(old), Ok(new)) => old != new,
        _ => true,
    }
}

fn normalized_limits_value(limits: &blackwire_config::schema::LimitsConfig) -> Vec<u8> {
    let mut limits = limits.clone();
    limits.max_connections_per_user = None;
    serde_json::to_vec(&limits).unwrap_or_default()
}

fn normalized_inbound_value(inbound: &blackwire_config::schema::InboundConfig) -> Vec<u8> {
    let mut inbound = inbound.clone();
    // Sniffing is hot-swapped separately and supported inbound auth material is
    // refreshed in place.
    inbound.sniffing = None;
    match inbound.protocol {
        Protocol::Vless | Protocol::Vmess | Protocol::Trojan => {
            inbound.settings.clients.clear();
        }
        Protocol::Shadowsocks => {
            inbound.settings.password = None;
            inbound.settings.email = None;
            inbound.settings.name = None;
            inbound.settings.clients.clear();
        }
        Protocol::Hysteria2 => {
            inbound.settings.auth = None;
            inbound.settings.password = None;
            inbound.settings.clients.clear();
        }
        Protocol::Tuic => {
            inbound.settings.users.clear();
            inbound.settings.uuid = None;
            inbound.settings.password = None;
            inbound.settings.email = None;
            inbound.settings.name = None;
        }
        _ => {
            strip_client_bandwidth_fields(&mut inbound.settings.clients);
            strip_client_bandwidth_fields(&mut inbound.settings.users);
        }
    }
    serde_json::to_vec(&inbound).unwrap_or_default()
}

fn strip_client_bandwidth_fields(entries: &mut [blackwire_config::schema::EndpointUser]) {
    for entry in entries {
        entry.up_mbps = None;
        entry.down_mbps = None;
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
