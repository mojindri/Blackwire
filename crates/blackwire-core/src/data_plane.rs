//! Immutable hot-path data-plane snapshots and compiled connection plans.

use std::sync::Arc;

use arc_swap::ArcSwap;
use blackwire_config::schema::{
    explain_cost, Config, CopyMode, InboundConfig, NetworkType, OutboundConfig, ProfileMode,
    Protocol, ProtocolCost, SecurityType, StreamSettingsConfig,
};

#[derive(Debug, Clone)]
pub struct DataPlane {
    pub listeners: Arc<[ListenerPlan]>,
    pub route_table: Arc<str>,
    pub outbound_table: Arc<[OutboundPlan]>,
    pub user_table: Arc<UserTable>,
    pub protocol_costs: Arc<[ProtocolCost]>,
    pub connection_plans: Arc<[ConnectionPlan]>,
}

#[derive(Debug)]
pub struct DataPlaneStore {
    inner: ArcSwap<DataPlane>,
}

impl DataPlaneStore {
    pub fn new(data_plane: Arc<DataPlane>) -> Self {
        Self {
            inner: ArcSwap::from(data_plane),
        }
    }

    pub fn load(&self) -> Arc<DataPlane> {
        self.inner.load_full()
    }

    pub fn swap(&self, data_plane: Arc<DataPlane>) -> Arc<DataPlane> {
        self.inner.swap(data_plane)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerPlan {
    pub tag: Arc<str>,
    pub listen: Arc<str>,
    pub inbound: InboundKind,
    pub transport: TransportKind,
    pub limits: LimitPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundPlan {
    pub tag: Arc<str>,
    pub outbound: OutboundKind,
    pub transport: TransportKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTable {
    pub inbound_tags: Arc<[Arc<str>]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionPlan {
    pub label: Arc<str>,
    pub inbound_tag: Arc<str>,
    pub inbound: InboundKind,
    pub transport: TransportKind,
    pub sniffing: SniffPlan,
    pub routing: RoutePlan,
    pub outbound: OutboundKind,
    pub relay: RelayPlan,
    pub limits: LimitPlan,
    pub cost: ProtocolCost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundKind {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Socks,
    Http,
    Freedom,
    ShadowTls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundKind {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Freedom,
    Socks,
    Http,
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
