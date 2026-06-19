//! Immutable hot-path data-plane snapshots and compiled connection plans.

use std::sync::Arc;

use arc_swap::ArcSwap;
use blackwire_config::schema::{
    explain_cost, Config, CopyMode, InboundConfig, NetworkType, OutboundConfig, ProfileMode,
    Protocol, ProtocolCost, SecurityType, StreamSettingsConfig,
};

/// Compiled hot-path snapshot of all listeners, routes, and connection plans.
#[derive(Debug, Clone)]
pub struct DataPlane {
    /// Compiled plan for every inbound listener.
    pub listeners: Arc<[ListenerPlan]>,
    /// Active routing strategy tag (e.g. `"IPIfNonMatch"`).
    pub route_table: Arc<str>,
    /// Compiled plan for every outbound handler.
    pub outbound_table: Arc<[OutboundPlan]>,
    /// Per-inbound user/authentication table.
    pub user_table: Arc<UserTable>,
    /// Per-protocol cost weights used for load-balancing decisions.
    pub protocol_costs: Arc<[ProtocolCost]>,
    /// Pre-compiled per-connection plans derived from listener × outbound pairs.
    pub connection_plans: Arc<[ConnectionPlan]>,
}

/// Atomic store wrapping a `DataPlane` that supports lock-free hot-reload.
#[derive(Debug)]
pub struct DataPlaneStore {
    inner: ArcSwap<DataPlane>,
}

impl DataPlaneStore {
    /// Create a new store from an initial `DataPlane`.
    pub fn new(data_plane: Arc<DataPlane>) -> Self {
        Self {
            inner: ArcSwap::from(data_plane),
        }
    }

    /// Return a reference-counted handle to the current `DataPlane`.
    pub fn load(&self) -> Arc<DataPlane> {
        self.inner.load_full()
    }

    /// Atomically replace the current `DataPlane` and return the old one.
    pub fn swap(&self, data_plane: Arc<DataPlane>) -> Arc<DataPlane> {
        self.inner.swap(data_plane)
    }
}

/// Compiled configuration for a single inbound listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerPlan {
    /// Unique tag identifying this listener.
    pub tag: Arc<str>,
    /// Address and port the listener is bound to.
    pub listen: Arc<str>,
    /// Protocol handled by this listener.
    pub inbound: InboundKind,
    /// Transport and security layer combination for this listener.
    pub transport: TransportKind,
    /// Resource limits applied to connections accepted by this listener.
    pub limits: LimitPlan,
}

/// Compiled configuration for a single outbound connection handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundPlan {
    /// Unique tag identifying this outbound.
    pub tag: Arc<str>,
    /// Protocol used by this outbound.
    pub outbound: OutboundKind,
    /// Transport and security layer combination for this outbound.
    pub transport: TransportKind,
}

/// Mapping from inbound listener tags to their user/authentication records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTable {
    /// Tags of all inbound listeners that have user tables configured.
    pub inbound_tags: Arc<[Arc<str>]>,
}

/// Pre-compiled end-to-end plan for a single inbound → outbound connection path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionPlan {
    /// Human-readable label for this connection path (used in logs and metrics).
    pub label: Arc<str>,
    /// Tag of the inbound listener this plan was derived from.
    pub inbound_tag: Arc<str>,
    /// Protocol of the inbound leg.
    pub inbound: InboundKind,
    /// Transport and security layer of the inbound leg.
    pub transport: TransportKind,
    /// Protocol sniffing behaviour for this connection.
    pub sniffing: SniffPlan,
    /// Routing configuration applied when selecting an outbound.
    pub routing: RoutePlan,
    /// Protocol of the outbound leg.
    pub outbound: OutboundKind,
    /// Relay strategy and capability flags for byte-forwarding.
    pub relay: RelayPlan,
    /// Resource limits applied to this connection.
    pub limits: LimitPlan,
    /// Protocol cost weight used for load-balancing.
    pub cost: ProtocolCost,
}

/// Protocol kind for an inbound listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundKind {
    /// VLESS inbound listener.
    Vless,
    /// VMess inbound listener.
    Vmess,
    /// Trojan inbound listener.
    Trojan,
    /// Shadowsocks inbound listener.
    Shadowsocks,
    /// Hysteria2 inbound listener.
    Hysteria2,
    /// TUIC v5 inbound listener.
    Tuic,
    /// SOCKS5 inbound listener.
    Socks,
    /// HTTP CONNECT proxy inbound listener.
    Http,
    /// Direct/freedom inbound listener.
    Freedom,
    /// ShadowTLS inbound listener.
    ShadowTls,
}

/// Protocol kind for an outbound connection handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundKind {
    /// VLESS outbound.
    Vless,
    /// VMess outbound.
    Vmess,
    /// Trojan outbound.
    Trojan,
    /// Shadowsocks outbound.
    Shadowsocks,
    /// Hysteria2 outbound.
    Hysteria2,
    /// TUIC v5 outbound.
    Tuic,
    /// Direct/freedom outbound (no proxy).
    Freedom,
    /// SOCKS5 outbound.
    Socks,
    /// HTTP CONNECT proxy outbound.
    Http,
    /// ShadowTLS outbound.
    ShadowTls,
}

/// The combined network and security layer kind for a transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportKind {
    /// Underlying network transport (TCP, WebSocket, gRPC, etc.).
    pub network: NetworkType,
    /// Security layer applied on top of the network transport (TLS, REALITY, etc.).
    pub security: SecurityType,
}

/// Whether protocol sniffing is active and how its results are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SniffPlan {
    /// True if deep-packet-inspection sniffing is enabled for this listener.
    pub enabled: bool,
    /// When true, sniffed domain names are used only for routing, not rewriting the target.
    pub route_only: bool,
}

/// Compiled routing configuration derived from the active rule set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    /// Domain-resolution strategy used during route evaluation (e.g. `"IPIfNonMatch"`).
    pub strategy: Arc<str>,
    /// Number of routing rules compiled from the configuration.
    pub rule_count: usize,
}

/// Relay-layer capabilities negotiated for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayPlan {
    /// I/O copy strategy to use when forwarding bytes between the two halves.
    pub copy_mode: CopyMode,
    /// Whether the kernel `splice(2)` zero-copy path is available for this relay.
    pub supports_splice: bool,
    /// Whether 0-RTT / TLS early-data can be used on the outbound leg.
    pub supports_early_data: bool,
    /// Whether the outbound supports UDP datagram forwarding.
    pub supports_datagram: bool,
}

/// Connection-level resource limits applied to inbound sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitPlan {
    /// Maximum number of simultaneous inbound connections; `None` means unlimited.
    pub max_connections: Option<usize>,
    /// Maximum seconds allowed for a TLS or protocol handshake to complete.
    pub max_handshake_seconds: Option<u64>,
    /// Maximum seconds a connection may be idle before being force-closed.
    pub max_idle_seconds: Option<u64>,
}

/// Build a `DataPlane` from the given configuration, computing listener and outbound plans.
pub fn compile_data_plane(config: &Config) -> Arc<DataPlane> {
    let cost = explain_cost(config).cost;
    let route_strategy: Arc<str> = config
        .routing
        .as_ref()
        .and_then(|r| r.domain_strategy.as_deref())
        .unwrap_or("asIs")
        .into();
    let route_rule_count = config.routing.as_ref().map_or(0, |r| r.rules.len());
    let default_outbound = config
        .outbounds
        .first()
        .map(|out| outbound_kind(&out.protocol))
        .unwrap_or(OutboundKind::Freedom);

    let listeners: Vec<_> = config.inbounds.iter().map(listener_plan).collect();
    let outbounds: Vec<_> = config.outbounds.iter().map(outbound_plan).collect();
    let inbound_tags: Vec<Arc<str>> = config
        .inbounds
        .iter()
        .map(|inbound| Arc::from(inbound.tag.as_str()))
        .collect();
    let connection_plans: Vec<_> = config
        .inbounds
        .iter()
        .map(|inbound| {
            connection_plan(
                inbound,
                config.profile,
                &route_strategy,
                route_rule_count,
                default_outbound,
                cost.clone(),
            )
        })
        .collect();

    Arc::new(DataPlane {
        listeners: Arc::from(listeners),
        route_table: route_strategy,
        outbound_table: Arc::from(outbounds),
        user_table: Arc::new(UserTable {
            inbound_tags: Arc::from(inbound_tags),
        }),
        protocol_costs: Arc::from(vec![cost]),
        connection_plans: Arc::from(connection_plans),
    })
}

fn listener_plan(inbound: &InboundConfig) -> ListenerPlan {
    ListenerPlan {
        tag: Arc::from(inbound.tag.as_str()),
        listen: Arc::from(format!("{}:{}", inbound.listen, inbound.port)),
        inbound: inbound_kind(&inbound.protocol),
        transport: transport_kind(inbound.stream_settings.as_ref()),
        limits: LimitPlan {
            max_connections: inbound.limits.as_ref().and_then(|l| l.max_connections),
            max_handshake_seconds: inbound
                .limits
                .as_ref()
                .and_then(|l| l.max_handshake_seconds),
            max_idle_seconds: inbound.limits.as_ref().and_then(|l| l.max_idle_seconds),
        },
    }
}

fn outbound_plan(outbound: &OutboundConfig) -> OutboundPlan {
    OutboundPlan {
        tag: Arc::from(outbound.tag.as_str()),
        outbound: outbound_kind(&outbound.protocol),
        transport: transport_kind(outbound.stream_settings.as_ref()),
    }
}

fn connection_plan(
    inbound: &InboundConfig,
    profile: ProfileMode,
    route_strategy: &Arc<str>,
    route_rule_count: usize,
    default_outbound: OutboundKind,
    cost: ProtocolCost,
) -> ConnectionPlan {
    let transport = transport_kind(inbound.stream_settings.as_ref());
    let inbound_kind = inbound_kind(&inbound.protocol);
    let label = plan_label(inbound_kind, &transport, default_outbound, profile);
    ConnectionPlan {
        label: Arc::from(label),
        inbound_tag: Arc::from(inbound.tag.as_str()),
        inbound: inbound_kind,
        transport,
        sniffing: SniffPlan {
            enabled: inbound.sniffing.as_ref().is_some_and(|s| s.enabled),
            route_only: inbound.sniffing.as_ref().is_some_and(|s| s.route_only),
        },
        routing: RoutePlan {
            strategy: Arc::clone(route_strategy),
            rule_count: route_rule_count,
        },
        outbound: default_outbound,
        relay: RelayPlan {
            copy_mode: cost.copy_mode,
            supports_splice: cost.supports_splice,
            supports_early_data: cost.supports_early_data,
            supports_datagram: cost.supports_datagram,
        },
        limits: LimitPlan {
            max_connections: inbound.limits.as_ref().and_then(|l| l.max_connections),
            max_handshake_seconds: inbound
                .limits
                .as_ref()
                .and_then(|l| l.max_handshake_seconds),
            max_idle_seconds: inbound.limits.as_ref().and_then(|l| l.max_idle_seconds),
        },
        cost,
    }
}

fn transport_kind(settings: Option<&StreamSettingsConfig>) -> TransportKind {
    settings.map_or_else(
        || TransportKind {
            network: NetworkType::Tcp,
            security: SecurityType::None,
        },
        |settings| TransportKind {
            network: settings.network.clone(),
            security: settings.security.clone(),
        },
    )
}

fn inbound_kind(protocol: &Protocol) -> InboundKind {
    match protocol {
        Protocol::Vless => InboundKind::Vless,
        Protocol::Vmess => InboundKind::Vmess,
        Protocol::Trojan => InboundKind::Trojan,
        Protocol::Shadowsocks => InboundKind::Shadowsocks,
        Protocol::Hysteria2 => InboundKind::Hysteria2,
        Protocol::Tuic => InboundKind::Tuic,
        Protocol::ShadowTls => InboundKind::ShadowTls,
        Protocol::Socks => InboundKind::Socks,
        Protocol::Http => InboundKind::Http,
        Protocol::Freedom => InboundKind::Freedom,
    }
}

fn outbound_kind(protocol: &Protocol) -> OutboundKind {
    match protocol {
        Protocol::Vless => OutboundKind::Vless,
        Protocol::Vmess => OutboundKind::Vmess,
        Protocol::Trojan => OutboundKind::Trojan,
        Protocol::Shadowsocks => OutboundKind::Shadowsocks,
        Protocol::Hysteria2 => OutboundKind::Hysteria2,
        Protocol::Tuic => OutboundKind::Tuic,
        Protocol::ShadowTls => OutboundKind::ShadowTls,
        Protocol::Socks => OutboundKind::Socks,
        Protocol::Http => OutboundKind::Http,
        Protocol::Freedom => OutboundKind::Freedom,
    }
}

fn plan_label(
    inbound: InboundKind,
    transport: &TransportKind,
    outbound: OutboundKind,
    profile: ProfileMode,
) -> &'static str {
    match (
        inbound,
        &transport.network,
        &transport.security,
        outbound,
        profile,
    ) {
        (InboundKind::Vless, NetworkType::Tcp, SecurityType::None, _, ProfileMode::Fast) => {
            "vless-tcp-fast"
        }
        (InboundKind::Vless, NetworkType::Tcp, SecurityType::Reality, _, _) => {
            "vless-reality-vision-direct"
        }
        (InboundKind::Socks, NetworkType::Tcp, SecurityType::None, OutboundKind::Freedom, _) => {
            "socks-freedom-direct"
        }
        (InboundKind::Hysteria2, _, _, _, _) => "hysteria2-datagram",
        (InboundKind::Tuic, _, _, _, _) => "tuic-v5-quic",
        (InboundKind::Freedom, _, _, _, _) => "tun-packet-nat",
        (_, NetworkType::Ws, _, _, _) => "ws-wrapped-copy",
        (_, NetworkType::Grpc, _, _, _) => "grpc-h2-data",
        _ => "dynamic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blackwire_config::schema::{InboundConfig, OutboundConfig};
    use std::net::{IpAddr, Ipv4Addr};

    fn config_with_inbound_tag(tag: &str) -> Config {
        Config {
            profile: ProfileMode::Fast,
            fast: None,
            budget: None,
            vision: None,
            first_packet_boost: None,
            quic: None,
            datagram: None,
            fec: None,
            log: Default::default(),
            dns: None,
            routing: None,
            tun: None,
            limits: Default::default(),
            inbounds: vec![InboundConfig {
                tag: tag.into(),
                protocol: Protocol::Socks,
                listen: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 1080,
                settings: serde_json::json!({}),
                stream_settings: None,
                limits: None,
                sniffing: None,
            }],
            outbounds: vec![OutboundConfig {
                tag: "direct".into(),
                protocol: Protocol::Freedom,
                settings: serde_json::json!({}),
                stream_settings: None,
            }],
            stats: None,
            api: None,
            metrics_addr: None,
        }
    }

    #[test]
    fn compiles_connection_plan_labels() {
        let plane = compile_data_plane(&config_with_inbound_tag("socks-in"));
        assert_eq!(plane.listeners.len(), 1);
        assert_eq!(plane.outbound_table.len(), 1);
        assert_eq!(
            plane.connection_plans[0].label.as_ref(),
            "socks-freedom-direct"
        );
    }

    #[test]
    fn data_plane_store_swaps_without_mutating_old_snapshot() {
        let first = compile_data_plane(&config_with_inbound_tag("old"));
        let second = compile_data_plane(&config_with_inbound_tag("new"));
        let store = DataPlaneStore::new(Arc::clone(&first));
        let old = store.swap(Arc::clone(&second));
        assert_eq!(old.listeners[0].tag.as_ref(), "old");
        assert_eq!(store.load().listeners[0].tag.as_ref(), "new");
        assert_eq!(first.listeners[0].tag.as_ref(), "old");
    }
}
