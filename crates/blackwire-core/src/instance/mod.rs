//! The running proxy instance.
//!
//! `Instance` is the top-level object that holds all the running components:
//! inbound listeners, outbound handlers, dispatcher, and router. Creating and
//! starting an `Instance` is what actually makes the proxy serve traffic.
//!
//! # How it works
//!
//! 1. `Instance::from_config()` reads the config and builds all the handlers.
//! 2. `instance.start()` spawns one Tokio task per inbound listener. Each task
//!    runs a TCP accept loop and calls the inbound handler for each connection.
//! 3. The instance holds `JoinHandle`s for all tasks. If any task panics,
//!    the error is logged but the other tasks keep running.
//!
//! # Transport layering
//!
//! Each inbound now goes through a layered handler stack:
//!
//!   TCP accept → \[TLS\] → \[WebSocket\] → Protocol handler
//!
//! The layers are applied based on `streamSettings.security` and
//! `streamSettings.network` in the config. If neither is set, it is plain TCP.
//!
//! # Hot-reload
//!
//! When the runtime observes a new desired MySQL revision, the store
//! reconstructs and validates a typed snapshot. `ReloadState::apply()` (in
//! `reload.rs`) swaps routing rules and supported inbound auth state without
//! restarting listeners. Structural changes are applied through a prepared
//! instance handover.

use anyhow::{Context as _, Result};
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::info;

use blackwire_app::dispatcher::{DefaultDispatcher, Dispatcher};
use blackwire_app::features::{ConnectionHandler, InboundHandler, OutboundHandler};
use blackwire_app::health::HealthChecker;
use blackwire_app::router::LiveRouter;
use blackwire_app::set_user_bandwidth_policies;
use blackwire_app::user_limits::UserConnectionLimiter;
use blackwire_app::{Balancer, ADAPTIVE_SPLICE_LONG_STREAM_AFTER, ADAPTIVE_SPLICE_MIN_BYTES};
use blackwire_config::schema::{
    Config, EndpointSettings, FastPoolPolicy, PoolSettings, ProfileMode, Protocol,
};
use blackwire_protocol::freedom::{FreedomIpStrategy, FreedomOutbound, PoolConfig};
use blackwire_protocol::socks::Socks5Inbound;
use tokio::net::UdpSocket as TokioUdpSocket;

use crate::data_plane::{compile_data_plane, DataPlaneStore};
use crate::http::build_http_inbound;
use crate::hysteria2::{build_hysteria2_outbound, start_hysteria2_inbound};
use crate::net::listen_socket_addr;
use crate::tuic::{build_tuic_outbound, start_tuic_inbound};
mod helpers;

pub(crate) use crate::ss2022::populate_ss2022_auth_store;
pub(crate) use crate::trojan::populate_trojan_auth_store;
pub(crate) use crate::vmess::populate_vmess_registry;
pub(crate) use helpers::{
    build_rules, build_sniffing_map, build_user_bandwidth_policies, load_geo_data,
    populate_vless_registry,
};

use crate::reality::{build_reality_server, uses_reality, RealityConnectionHandler};
use crate::reload::ReloadState;
use crate::ss2022::{build_ss2022_inbound, build_ss2022_outbound};
use crate::trojan::{build_trojan_inbound, build_trojan_outbound};
use crate::vmess::{build_vmess_inbound, build_vmess_outbound};
use helpers::{
    build_dns_module, build_vless_inbound, build_vless_outbound, handshake_timeout_for,
    initial_health_states, reject_unfinished_transport_settings, select_balancer_outbounds,
    InboundConnectionHandler,
};

fn apply_pool_overrides(mut cfg: PoolConfig, source: &PoolSettings) -> PoolConfig {
    if let Some(v) = source.max_per_dest {
        cfg.max_per_dest = v.max(1);
    }
    if let Some(v) = source.max_global_idle {
        cfg.max_global_idle = v.max(1);
    }
    if let Some(v) = source.max_dests {
        cfg.max_dests = v.max(1);
    }
    if let Some(ms) = source.idle_ttl_ms {
        cfg.idle_ttl = std::time::Duration::from_millis(ms.max(1));
    }
    if let Some(ms) = source.hotness_window_ms {
        cfg.hotness_window = std::time::Duration::from_millis(ms.max(1));
    }
    if let Some(v) = source.min_hotness_for_pool {
        cfg.min_hotness_for_pool = v.max(1);
    }
    cfg
}

fn pool_config_from_mode(mode: &str) -> Option<PoolConfig> {
    match mode {
        "adaptive" => Some(PoolConfig::fast_profile()),
        "disabled" | "off" | "none" => None,
        "fixed" => Some(PoolConfig::fixed(8)),
        _ => None,
    }
}

fn freedom_deny_loopback(settings: &EndpointSettings) -> bool {
    settings.deny_loopback
}

fn freedom_reject_ipv6_literal(settings: &EndpointSettings) -> bool {
    settings.reject_ipv6_literal
}

fn freedom_ip_strategy(settings: &EndpointSettings) -> FreedomIpStrategy {
    let Some(raw) = settings
        .domain_strategy
        .as_deref()
        .or(settings.ip_strategy.as_deref())
    else {
        return FreedomIpStrategy::Auto;
    };

    match raw
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "")
        .as_str()
    {
        "" | "auto" | "asis" => FreedomIpStrategy::Auto,
        "useip" | "ip" => FreedomIpStrategy::UseIp,
        "preferipv4" | "preferip4" => FreedomIpStrategy::PreferIpv4,
        "preferipv6" | "preferip6" => FreedomIpStrategy::PreferIpv6,
        "useipv4" | "useip4" | "ipv4" | "ip4" => FreedomIpStrategy::UseIpv4,
        "useipv6" | "useip6" | "ipv6" | "ip6" => FreedomIpStrategy::UseIpv6,
        _ => FreedomIpStrategy::Auto,
    }
}

fn freedom_pool_config(config: &Config, settings: &EndpointSettings) -> Option<PoolConfig> {
    if let Some(pool) = settings.pool.as_ref() {
        let base = pool_config_from_mode(&pool.mode.trim().to_ascii_lowercase())?;
        return Some(apply_pool_overrides(base, pool));
    }

    if settings.pool_enabled == Some(false) {
        return None;
    }

    if config.profile != ProfileMode::Fast {
        return None;
    }

    match config.fast.as_ref().map(|f| f.pool).unwrap_or_default() {
        FastPoolPolicy::Adaptive => Some(PoolConfig::fast_profile()),
        FastPoolPolicy::Disabled => None,
        FastPoolPolicy::Fixed => Some(PoolConfig::fixed(8)),
    }
}

use crate::ws_tls::{
    build_conn_handler, uses_grpc, uses_httpupgrade, uses_shadowtls, uses_splithttp, uses_tls,
    uses_ws,
};

/// Running proxy instance plus reload handles for live config updates.
pub struct Instance {
    /// Background task handles. Kept alive as long as `Instance` is alive.
    tasks: Vec<JoinHandle<()>>,
    /// Hot-reload state shared with the config watcher.
    pub reload: ReloadState,
    /// Immutable hot-path data-plane snapshot.
    pub data_plane: DataPlaneStore,
}

impl fmt::Debug for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance")
            .field("task_count", &self.tasks.len())
            .finish()
    }
}

impl Instance {
    /// Build and start a proxy instance from a validated config.
    ///
    /// This function:
    ///   1. Builds outbound handlers from `config.outbounds`
    ///   2. Builds the router from `config.routing`
    ///   3. Creates the dispatcher
    ///   4. Builds inbound handlers from `config.inbounds`
    ///   5. Starts all inbound listeners
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///   - A listen address is invalid
    ///   - A required config field is missing or malformed
    pub async fn from_config(config: Arc<Config>) -> Result<Self> {
        anyhow::ensure!(
            config.tun.is_none(),
            "TUN configuration is client-owned; start it through blackwire-client"
        );
        let mut tasks = Vec::with_capacity(config.inbounds.len().saturating_add(4));
        let data_plane = compile_data_plane(config.as_ref());

        // ── Step 1: DNS module (shared by dispatcher + freedom outbounds) ─────
        let dns = build_dns_module(config.dns.as_ref()).await?;

        // ── Step 2: Build outbound handlers ─────────────────────────────────
        let balancer_count = config
            .routing
            .as_ref()
            .map_or(0, |routing| routing.balancers.len());
        let mut outbound_map: HashMap<String, Arc<dyn OutboundHandler>> =
            HashMap::with_capacity(config.outbounds.len().saturating_add(balancer_count));

        for out_cfg in &config.outbounds {
            reject_unfinished_transport_settings(
                "outbound",
                &out_cfg.tag,
                out_cfg.protocol.clone(),
                &out_cfg.stream_settings,
            )?;
            let handler: Arc<dyn OutboundHandler> = match out_cfg.protocol {
                Protocol::Freedom => {
                    let deny_loopback = freedom_deny_loopback(&out_cfg.settings);
                    let ip_strategy = freedom_ip_strategy(&out_cfg.settings);
                    let reject_ipv6_literal = freedom_reject_ipv6_literal(&out_cfg.settings);
                    let pool_cfg = freedom_pool_config(config.as_ref(), &out_cfg.settings);
                    let outbound = if let Some(cfg) = pool_cfg {
                        match &dns {
                            Some(module) => FreedomOutbound::new_with_dns_pooled(
                                &out_cfg.tag,
                                Arc::clone(module),
                                cfg,
                            ),
                            None => FreedomOutbound::new_pooled(&out_cfg.tag, cfg),
                        }
                    } else {
                        match &dns {
                            Some(module) => {
                                FreedomOutbound::new_with_dns(&out_cfg.tag, Arc::clone(module))
                            }
                            None => FreedomOutbound::new(&out_cfg.tag),
                        }
                    };
                    outbound
                        .with_deny_loopback(deny_loopback)
                        .with_ip_strategy(ip_strategy)
                        .with_reject_ipv6_literal(reject_ipv6_literal)
                }
                Protocol::Vless => build_vless_outbound(out_cfg)
                    .with_context(|| format!("building VLESS outbound '{}'", out_cfg.tag))?,
                Protocol::Hysteria2 => build_hysteria2_outbound(
                    out_cfg,
                    config.quic.as_ref(),
                    config.datagram.as_ref(),
                    config.fec.as_ref(),
                )
                .with_context(|| format!("building Hysteria2 outbound '{}'", out_cfg.tag))?,
                Protocol::Tuic => build_tuic_outbound(out_cfg, config.quic.as_ref())
                    .with_context(|| format!("building TUIC outbound '{}'", out_cfg.tag))?,
                Protocol::Trojan => build_trojan_outbound(out_cfg)
                    .with_context(|| format!("building Trojan outbound '{}'", out_cfg.tag))?,
                Protocol::Vmess => build_vmess_outbound(out_cfg)
                    .with_context(|| format!("building VMess outbound '{}'", out_cfg.tag))?,
                Protocol::Shadowsocks => build_ss2022_outbound(out_cfg)
                    .with_context(|| format!("building SS-2022 outbound '{}'", out_cfg.tag))?,
                ref p => {
                    anyhow::bail!("outbound protocol {:?} not yet implemented", p)
                }
            };
            info!(tag = %handler.tag(), "registered outbound");
            outbound_map.insert(out_cfg.tag.clone(), handler);
        }

        // ── Step 1b: Build balancer outbounds and health-check tasks ────────
        if let Some(routing) = &config.routing {
            for balancer_cfg in &routing.balancers {
                if outbound_map.contains_key(&balancer_cfg.tag) {
                    anyhow::bail!(
                        "balancer tag '{}' conflicts with an existing outbound",
                        balancer_cfg.tag
                    );
                }

                let selected = select_balancer_outbounds(balancer_cfg, &outbound_map)?;
                let states = if let Some(health_cfg) = &balancer_cfg.health_check {
                    let (checker, states) =
                        HealthChecker::new(selected.clone(), health_cfg.clone()).map_err(|e| {
                            anyhow::anyhow!(
                                "invalid health check for balancer '{}': {e}",
                                balancer_cfg.tag
                            )
                        })?;
                    tasks.push(tokio::spawn(checker.run()));
                    states
                } else {
                    initial_health_states(&selected)
                };

                let balancer = Balancer::new(balancer_cfg, selected, states);
                info!(tag = %balancer.tag(), "registered balancer outbound");
                outbound_map.insert(balancer_cfg.tag.clone(), balancer);
            }
        }

        // ── Step 2: Build router ─────────────────────────────────────────────
        let default_tag = config
            .outbounds
            .first()
            .map(|o| o.tag.clone())
            .unwrap_or_else(|| "direct".into());

        let outbound_tags: HashSet<String> = outbound_map.keys().cloned().collect();

        let rules = if let Some(routing) = &config.routing {
            build_rules(&routing.rules, &outbound_tags)?
        } else {
            vec![]
        };

        let (geoip, geosite) = load_geo_data(config.routing.as_ref());
        let domain_strategy = config
            .routing
            .as_ref()
            .and_then(|r| r.domain_strategy.clone());
        let router = LiveRouter::new(rules, default_tag, geoip, geosite, domain_strategy.clone());
        let sniffing_shared = Arc::new(ArcSwap::from_pointee(build_sniffing_map(&config.inbounds)));
        // Shared with the config watcher: router swap + inbound auth refresh on reload.
        let user_connection_limiter = Arc::new(UserConnectionLimiter::new(
            config.limits.max_connections_per_user.unwrap_or(usize::MAX),
        ));
        let reload = ReloadState::new(
            Arc::clone(&router),
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            Arc::new(DashMap::new()),
            Arc::clone(&sniffing_shared),
            Arc::clone(&user_connection_limiter),
        );
        let vless_registries = Arc::clone(&reload.vless_registries);
        let connection_plan_labels: HashMap<String, Arc<str>> = data_plane
            .connection_plans
            .iter()
            .map(|plan| (plan.inbound_tag.to_string(), Arc::clone(&plan.label)))
            .collect();

        // ── Step 4: Create dispatcher ────────────────────────────────────────
        let dispatcher = match &dns {
            Some(dns) => DefaultDispatcher::new_with_dns_and_sniffing(
                router,
                outbound_map,
                Arc::clone(dns),
                Arc::clone(&sniffing_shared),
            ),
            None => DefaultDispatcher::new_with_sniffing(router, outbound_map, sniffing_shared),
        }
        .with_profile_fast_and_vision(config.profile, config.fast.as_ref(), config.vision.as_ref())
        .with_first_packet_boost(config.first_packet_boost.unwrap_or_default())
        .with_connection_plans(Arc::new(connection_plan_labels));

        if config.profile == ProfileMode::Fast {
            let fast = config.fast.as_ref().cloned().unwrap_or_default();
            let adaptive_pool = match fast.pool {
                FastPoolPolicy::Adaptive => PoolConfig::fast_profile(),
                FastPoolPolicy::Fixed => PoolConfig::fixed(8),
                FastPoolPolicy::Disabled => PoolConfig::default(),
            };
            info!(
                pool_policy = ?fast.pool,
                adaptive_pool_max_per_dest = adaptive_pool.max_per_dest,
                adaptive_pool_min_hotness = adaptive_pool.min_hotness_for_pool,
                adaptive_pool_idle_ttl_ms = adaptive_pool.idle_ttl.as_millis(),
                splice_policy = ?fast.splice,
                vision_policy = ?config.vision.unwrap_or_default(),
                adaptive_splice_min_bytes = ADAPTIVE_SPLICE_MIN_BYTES,
                adaptive_splice_long_stream_ms = ADAPTIVE_SPLICE_LONG_STREAM_AFTER.as_millis(),
                strict_production = fast.strict_production,
                "fast profile policy active"
            );
        }

        // ── Step 4 & 5: Build inbounds and start listeners ───────────────────
        let global_connection_limiter = config
            .limits
            .max_connections
            .map(|n| Arc::new(Semaphore::new(n)));
        set_user_bandwidth_policies(build_user_bandwidth_policies(&config.inbounds));
        for in_cfg in &config.inbounds {
            reject_unfinished_transport_settings(
                "inbound",
                &in_cfg.tag,
                in_cfg.protocol.clone(),
                &in_cfg.stream_settings,
            )?;
            let addr = listen_socket_addr(in_cfg.listen, in_cfg.port);

            // Hysteria2 and TUIC run their own QUIC servers — they do not use TcpServerTransport.
            if in_cfg.protocol == Protocol::Hysteria2 {
                info!(tag = %in_cfg.tag, addr = %addr, "starting Hysteria2 inbound listener");
                let dispatcher_for_h2 = Arc::clone(&dispatcher) as Arc<dyn Dispatcher>;
                let task = start_hysteria2_inbound(
                    in_cfg,
                    &reload.hysteria2_auth_stores,
                    config.quic.as_ref(),
                    config.datagram.as_ref(),
                    config.fec.as_ref(),
                    config.limits.max_connections_per_inbound,
                    global_connection_limiter.as_ref().map(Arc::clone),
                    Some(Arc::clone(&user_connection_limiter)),
                    dispatcher_for_h2,
                )
                .with_context(|| format!("starting Hysteria2 inbound '{}'", in_cfg.tag))?;
                tasks.push(task);
                continue;
            }
            if in_cfg.protocol == Protocol::Tuic {
                info!(tag = %in_cfg.tag, addr = %addr, "starting TUIC v5 inbound listener");
                let dispatcher_for_tuic = Arc::clone(&dispatcher) as Arc<dyn Dispatcher>;
                let task = start_tuic_inbound(
                    in_cfg,
                    &reload.tuic_auth_stores,
                    config.quic.as_ref(),
                    config.limits.max_connections_per_inbound,
                    global_connection_limiter.as_ref().map(Arc::clone),
                    Some(Arc::clone(&user_connection_limiter)),
                    dispatcher_for_tuic,
                )
                .with_context(|| format!("starting TUIC inbound '{}'", in_cfg.tag))?;
                tasks.push(task);
                continue;
            }

            // SS-2022 UDP: standalone UDP listener (SIP022).
            if in_cfg.protocol == Protocol::Shadowsocks {
                let net = in_cfg.settings.network.as_deref().unwrap_or("tcp");
                if net == "udp" || net == "tcp,udp" || net == "udp,tcp" {
                    let auth = reload
                        .ss2022_auth_stores
                        .entry(in_cfg.tag.clone())
                        .or_default()
                        .clone();
                    crate::ss2022::populate_ss2022_auth_store(&auth, in_cfg)?;
                    let socket = TokioUdpSocket::bind(addr).await.with_context(|| {
                        format!("binding SS-2022 UDP inbound '{}' on {}", in_cfg.tag, addr)
                    })?;
                    let socket = std::sync::Arc::new(socket);
                    info!(tag = %in_cfg.tag, addr = %addr, "starting SS-2022 UDP inbound");
                    let dns_for_udp = dns.clone();
                    let udp_tag = std::sync::Arc::<str>::from(in_cfg.tag.clone());
                    let task = tokio::spawn(async move {
                        blackwire_protocol::ss2022::udp::relay_ss2022_udp(
                            socket,
                            auth,
                            dns_for_udp,
                            udp_tag,
                        )
                        .await;
                    });
                    tasks.push(task);
                    if net == "udp" {
                        continue; // UDP-only: skip TCP listener below
                    }
                }
            }

            let handshake_timeout = handshake_timeout_for(in_cfg, &config.limits);

            let handler: Arc<dyn InboundHandler> = match in_cfg.protocol {
                Protocol::Socks => Socks5Inbound::new(in_cfg.tag.as_str(), handshake_timeout),
                Protocol::Vless => build_vless_inbound(
                    in_cfg,
                    &vless_registries,
                    handshake_timeout,
                    dns.clone(),
                    Some(Arc::clone(&user_connection_limiter)),
                )
                .with_context(|| format!("building VLESS inbound '{}'", in_cfg.tag))?,
                Protocol::Trojan => build_trojan_inbound(
                    in_cfg,
                    &reload.trojan_auth_stores,
                    dns.clone(),
                    handshake_timeout,
                    Some(Arc::clone(&user_connection_limiter)),
                )
                .with_context(|| format!("building Trojan inbound '{}'", in_cfg.tag))?,
                Protocol::Vmess => build_vmess_inbound(
                    in_cfg,
                    &reload.vmess_registries,
                    handshake_timeout,
                    Some(Arc::clone(&user_connection_limiter)),
                )
                .with_context(|| format!("building VMess inbound '{}'", in_cfg.tag))?,
                Protocol::Http => build_http_inbound(in_cfg, handshake_timeout)
                    .with_context(|| format!("building HTTP CONNECT inbound '{}'", in_cfg.tag))?,
                Protocol::Shadowsocks => build_ss2022_inbound(
                    in_cfg,
                    &reload.ss2022_auth_stores,
                    handshake_timeout,
                    Some(Arc::clone(&user_connection_limiter)),
                )
                .with_context(|| format!("building SS-2022 inbound '{}'", in_cfg.tag))?,
                ref p => {
                    anyhow::bail!("inbound protocol {:?} not yet implemented", p)
                }
            };

            info!(tag = %handler.tag(), addr = %addr, "starting inbound listener");

            let dispatcher_for_handler = Arc::clone(&dispatcher) as Arc<dyn Dispatcher>;

            // Choose the connection handler stack based on stream settings.
            let conn_handler: Arc<dyn ConnectionHandler> = if uses_reality(&in_cfg.stream_settings)
            {
                // REALITY: unwrap REALITY TLS camouflage first.
                let reality = build_reality_server(in_cfg)
                    .with_context(|| format!("building REALITY inbound '{}'", in_cfg.tag))?;
                let cover_sni = in_cfg
                    .stream_settings
                    .as_ref()
                    .and_then(|s| s.reality_settings.as_ref())
                    .and_then(|r| {
                        r.server_names.first().map(String::as_str).or_else(|| {
                            (!r.server_name.is_empty()).then_some(r.server_name.as_str())
                        })
                    })
                    .unwrap_or("localhost");
                RealityConnectionHandler::new(
                    reality,
                    in_cfg.tag.clone(),
                    cover_sni,
                    handshake_timeout,
                    Arc::clone(&handler),
                    dispatcher_for_handler,
                )
                .with_context(|| {
                    format!(
                        "building REALITY connection handler for inbound '{}'",
                        in_cfg.tag
                    )
                })?
            } else if uses_tls(&in_cfg.stream_settings)
                || uses_shadowtls(&in_cfg.stream_settings)
                || uses_ws(&in_cfg.stream_settings)
                || uses_grpc(&in_cfg.stream_settings)
                || uses_splithttp(&in_cfg.stream_settings)
                || uses_httpupgrade(&in_cfg.stream_settings)
            {
                // Layered transports: TLS, WebSocket, HTTPUpgrade, and/or gRPC.
                build_conn_handler(
                    handler,
                    dispatcher_for_handler,
                    &in_cfg.stream_settings,
                    handshake_timeout,
                )
                .with_context(|| {
                    format!(
                        "building TLS/WS connection handler for inbound '{}'",
                        in_cfg.tag
                    )
                })?
            } else {
                // Plain TCP: no transport wrapping.
                Arc::new(InboundConnectionHandler {
                    inbound: Arc::clone(&handler),
                    dispatcher: dispatcher_for_handler,
                })
            };

            // Start the TCP accept loop for this inbound.
            let tcp_config = blackwire_transport::tcp::TcpConfig {
                max_connections: in_cfg
                    .limits
                    .as_ref()
                    .and_then(|limits| limits.max_connections)
                    .or(config.limits.max_connections_per_inbound),
                tcp_fast_open: true,
                ..Default::default()
            };

            let transport = std::sync::Arc::new(
                blackwire_transport::TcpServerTransport::new(tcp_config)
                    .with_shared_limiter(global_connection_limiter.as_ref().map(Arc::clone)),
            );
            // One accept-loop shard per logical CPU; the kernel distributes
            // incoming SYNs across them via SO_REUSEPORT.
            let shards = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            let mut shard_tasks = transport
                .serve_multi(addr, shards, conn_handler)
                .with_context(|| format!("binding inbound listener '{}'", in_cfg.tag))?;
            tasks.append(&mut shard_tasks);
        }

        // ── Optional: start metrics/health HTTP server ───────────────────────
        if let Some(metrics_addr) = &config.metrics_addr {
            let handle = blackwire_app::metrics::start_metrics_server(metrics_addr)
                .with_context(|| format!("starting metrics server on '{metrics_addr}'"))?;
            info!(addr = %metrics_addr, "metrics server started");
            tasks.push(handle);
        }

        Ok(Self {
            tasks,
            reload,
            data_plane: DataPlaneStore::new(data_plane),
        })
    }

    /// Wait for all inbound listeners to exit.
    ///
    /// In normal operation this runs forever. It only returns if all listeners
    /// have exited (e.g. due to an error).
    ///
    /// After this returns, the `Instance` is empty — tasks have already
    /// completed so `Drop` will call `abort()` on zero handles (no-op).
    pub async fn wait(mut self) {
        // Drain the task list before awaiting. This way the Drop impl
        // (which calls abort on remaining tasks) sees an empty list,
        // which is safe and correct.
        let tasks = std::mem::take(&mut self.tasks);
        for task in tasks {
            let _ = task.await;
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
