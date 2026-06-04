//! Dispatcher: the connection between inbounds and outbounds.
//!
//! After an inbound handler decodes a connection's destination address, it
//! hands the connection to the dispatcher. The dispatcher:
//!
//!   1. Asks the router which outbound to use.
//!   2. Calls `OutboundHandler::connect()` to open a connection to the destination.
//!   3. Relays bytes bidirectionally between the inbound and outbound connections.
//!   4. Records statistics (bytes transferred, connection duration).
//!
//! # The relay loop
//!
//! The default relay is implemented using `tokio::io::copy_bidirectional`. This
//! runs two concurrent copy loops:
//!   - Inbound → Outbound: read from the client, write to the server
//!   - Outbound → Inbound: read from the server, write to the client
//!
//! Both loops run until either side closes the connection or an error occurs.
//!
//! # Linux splice(2)
//!
//! On Linux, raw TCP-to-TCP relays use `splice(2)`, which moves bytes through
//! kernel pipes without copying them into userspace. Non-Linux builds and
//! non-raw streams keep using `copy_bidirectional`.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use blackwire_connmgr::{global_manager, CloseReason, Protocol, RelayPath, Transport};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};

macro_rules! relay_log {
    ($profile:expr, $($args:tt)*) => {
        if $profile == ProfileMode::Fast {
            debug!($($args)*);
        } else {
            info!($($args)*);
        }
    };
}

/// DNS resolution budget for routing decisions (IPOnDemand / IPIfNonMatch).
///
/// Slow DNS during routing would stall the entire connection dispatch, so we cap
/// the budget well below the connection handshake timeout.
const ROUTING_DNS_TIMEOUT: Duration = Duration::from_secs(3);
const OUTBOUND_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum time Fast Profile waits for client bytes before validating a pooled socket.
///
/// The first-use guard needs client bytes so it can retry with a fresh dial if
/// a pooled socket is stale. Server-first protocols do not send client bytes
/// immediately, so this guard must be bounded to avoid blocking the relay.
const POOLED_FIRST_WRITE_GUARD_TIMEOUT: Duration = Duration::from_millis(1);
const POOLED_FIRST_WRITE_GUARD_BUF_SIZE: usize = 2048;

use std::collections::HashMap;

use blackwire_common::{
    tcp_connect, Address, BoxedStream, PooledStream, PrependedStream, ProxyError,
};
use blackwire_config::schema::{
    FastConfig, FastLinuxConfig, FastRelayConfig, FastSplicePolicy, FirstPacketBoostConfig,
    ProfileMode, SniffingConfig, VisionConfig,
};
use smallvec::SmallVec;
use tokio::net::TcpStream;

use crate::context::Context;
use crate::dns::DnsModule;
use crate::features::OutboundHandler;
use crate::metrics::{
    record_connection_accepted, record_connection_closed, record_connection_plan_selected,
    record_dns, record_dns_prefetch, record_early_payload, record_early_payload_written,
    record_first_byte_latency, record_first_packet_boost, record_handshake_kick,
    record_outbound_connect, record_relay_error, record_route,
};
use crate::router::{normalize_routing_domain_strategy, Router, RoutingDomainStrategy};
use crate::runtime_stats;

struct RoutedOutboundConnectResult {
    connect: crate::features::OutboundConnectResult,
    outbound_tag: Arc<str>,
}

/// The dispatcher connects inbounds to outbounds by consulting the router
/// and relaying bytes.
#[async_trait]
pub trait Dispatcher: Send + Sync + 'static {
    /// Dispatch a connection to the appropriate outbound.
    ///
    /// # Arguments
    /// * `ctx` — connection context (inbound tag, user, source address)
    /// * `dest` — the destination the client wants to reach
    /// * `inbound_stream` — the byte stream from the inbound side
    async fn dispatch(
        &self,
        ctx: Context,
        dest: Address,
        inbound_stream: BoxedStream,
    ) -> Result<(), ProxyError>;

    /// Dispatch with bytes already read past the inbound handshake.
    async fn dispatch_with_early_payload(
        &self,
        ctx: Context,
        dest: Address,
        inbound_stream: BoxedStream,
        early_payload: Option<Vec<u8>>,
    ) -> Result<(), ProxyError> {
        let inbound_stream = match early_payload {
            Some(payload) if !payload.is_empty() => {
                Box::new(PrependedStream::new(inbound_stream, payload)) as BoxedStream
            }
            _ => inbound_stream,
        };
        self.dispatch(ctx, dest, inbound_stream).await
    }

    /// Route and open an outbound stream without relaying (Mux.Cool sub-connections).
    async fn connect_outbound(
        &self,
        ctx: &Context,
        dest: &Address,
    ) -> Result<BoxedStream, ProxyError>;
}

/// The standard dispatcher implementation.
///
/// Uses the router to pick an outbound, then relays bytes between
/// the inbound and outbound connections.
pub struct DefaultDispatcher {
    router: Arc<dyn Router>,
    outbounds: std::collections::HashMap<String, Arc<dyn OutboundHandler>>,
    dns: Option<Arc<DnsModule>>,
    sniffing: Arc<ArcSwap<HashMap<String, Arc<SniffingConfig>>>>,
    /// Operating profile. Under `Fast`, per-connection relay logs are emitted at
    /// `debug` level rather than `info` to reduce log overhead on hot paths.
    profile: ProfileMode,
    splice_policy: FastSplicePolicy,
    relay_policy: FastRelayConfig,
    linux_policy: FastLinuxConfig,
    vision_policy: VisionConfig,
    first_packet_boost: FirstPacketBoostConfig,
    connection_plans: Arc<HashMap<String, Arc<str>>>,
}

fn splice_policy_for_profile(profile: ProfileMode, fast: Option<&FastConfig>) -> FastSplicePolicy {
    if profile == ProfileMode::Fast {
        fast.map(|f| f.splice).unwrap_or_default()
    } else {
        FastSplicePolicy::Adaptive
    }
}

fn relay_policy_for_profile(profile: ProfileMode, fast: Option<&FastConfig>) -> FastRelayConfig {
    if profile == ProfileMode::Fast {
        fast.map(|f| f.relay).unwrap_or_default()
    } else {
        FastRelayConfig::default()
    }
}

fn linux_policy_for_profile(profile: ProfileMode, fast: Option<&FastConfig>) -> FastLinuxConfig {
    if profile == ProfileMode::Fast {
        fast.map(|f| f.linux).unwrap_or_default()
    } else {
        FastLinuxConfig::default()
    }
}

impl DefaultDispatcher {
    /// Create a new dispatcher with the given router and outbounds map.
    ///
    /// # Arguments
    /// * `router` — the routing engine
    /// * `outbounds` — map from outbound tag to outbound handler
    pub fn new(
        router: Arc<dyn Router>,
        outbounds: std::collections::HashMap<String, Arc<dyn OutboundHandler>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            router,
            outbounds,
            dns: None,
            sniffing: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            profile: ProfileMode::default(),
            splice_policy: splice_policy_for_profile(ProfileMode::default(), None),
            relay_policy: relay_policy_for_profile(ProfileMode::default(), None),
            linux_policy: linux_policy_for_profile(ProfileMode::default(), None),
            vision_policy: VisionConfig::default(),
            first_packet_boost: FirstPacketBoostConfig::default(),
            connection_plans: Arc::new(HashMap::new()),
        })
    }

    /// Create a dispatcher with per-inbound sniffing settings (Xray `sniffing`).
    pub fn new_with_sniffing(
        router: Arc<dyn Router>,
        outbounds: std::collections::HashMap<String, Arc<dyn OutboundHandler>>,
        sniffing: Arc<ArcSwap<HashMap<String, Arc<SniffingConfig>>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            router,
            outbounds,
            dns: None,
            sniffing,
            profile: ProfileMode::default(),
            splice_policy: splice_policy_for_profile(ProfileMode::default(), None),
            relay_policy: relay_policy_for_profile(ProfileMode::default(), None),
            linux_policy: linux_policy_for_profile(ProfileMode::default(), None),
            vision_policy: VisionConfig::default(),
            first_packet_boost: FirstPacketBoostConfig::default(),
            connection_plans: Arc::new(HashMap::new()),
        })
    }

    /// Create a dispatcher with DNS/FakeIP support.
    ///
    /// If a destination IP is in the configured FakeIP pool, the dispatcher
    /// restores the original domain before routing and outbound connection.
    pub fn new_with_dns(
        router: Arc<dyn Router>,
        outbounds: std::collections::HashMap<String, Arc<dyn OutboundHandler>>,
        dns: Arc<DnsModule>,
    ) -> Arc<Self> {
        Arc::new(Self {
            router,
            outbounds,
            dns: Some(dns),
            sniffing: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            profile: ProfileMode::default(),
            splice_policy: splice_policy_for_profile(ProfileMode::default(), None),
            relay_policy: relay_policy_for_profile(ProfileMode::default(), None),
            linux_policy: linux_policy_for_profile(ProfileMode::default(), None),
            vision_policy: VisionConfig::default(),
            first_packet_boost: FirstPacketBoostConfig::default(),
            connection_plans: Arc::new(HashMap::new()),
        })
    }

    /// Dispatcher with DNS and sniffing.
    pub fn new_with_dns_and_sniffing(
        router: Arc<dyn Router>,
        outbounds: std::collections::HashMap<String, Arc<dyn OutboundHandler>>,
        dns: Arc<DnsModule>,
        sniffing: Arc<ArcSwap<HashMap<String, Arc<SniffingConfig>>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            router,
            outbounds,
            dns: Some(dns),
            sniffing,
            profile: ProfileMode::default(),
            splice_policy: splice_policy_for_profile(ProfileMode::default(), None),
            relay_policy: relay_policy_for_profile(ProfileMode::default(), None),
            linux_policy: linux_policy_for_profile(ProfileMode::default(), None),
            vision_policy: VisionConfig::default(),
            first_packet_boost: FirstPacketBoostConfig::default(),
            connection_plans: Arc::new(HashMap::new()),
        })
    }

    /// Set the operating profile, returning the same `Arc`.
    ///
    /// Call this after construction to apply a non-default profile from config.
    pub fn with_profile(self: Arc<Self>, profile: ProfileMode) -> Arc<Self> {
        self.with_profile_and_fast(profile, None)
    }

    /// Set profile and Fast Profile config together, returning the same `Arc`.
    pub fn with_profile_and_fast(
        self: Arc<Self>,
        profile: ProfileMode,
        fast: Option<&FastConfig>,
    ) -> Arc<Self> {
        self.with_profile_fast_and_vision(profile, fast, None)
    }

    /// Set profile, Fast Profile config, and Vision policy together.
    pub fn with_profile_fast_and_vision(
        self: Arc<Self>,
        profile: ProfileMode,
        fast: Option<&FastConfig>,
        vision: Option<&VisionConfig>,
    ) -> Arc<Self> {
        let splice_policy = splice_policy_for_profile(profile, fast);
        let relay_policy = relay_policy_for_profile(profile, fast);
        let linux_policy = linux_policy_for_profile(profile, fast);
        let vision_policy = vision.copied().unwrap_or_default();
        if self.profile == profile
            && self.splice_policy == splice_policy
            && self.relay_policy == relay_policy
            && self.linux_policy == linux_policy
            && self.vision_policy == vision_policy
        {
            return self;
        }
        // We own the only Arc reference here (just constructed), so unwrap is safe.
        // If multiple references exist, clone the inner value.
        match Arc::try_unwrap(self) {
            Ok(mut inner) => {
                inner.profile = profile;
                inner.splice_policy = splice_policy;
                inner.relay_policy = relay_policy;
                inner.linux_policy = linux_policy;
                inner.vision_policy = vision_policy;
                Arc::new(inner)
            }
            Err(arc) => Arc::new(Self {
                router: Arc::clone(&arc.router),
                outbounds: arc.outbounds.clone(),
                dns: arc.dns.clone(),
                sniffing: Arc::clone(&arc.sniffing),
                profile,
                splice_policy,
                relay_policy,
                linux_policy,
                vision_policy,
                first_packet_boost: arc.first_packet_boost,
                connection_plans: Arc::clone(&arc.connection_plans),
            }),
        }
    }

    /// Set first-packet acceleration policy.
    pub fn with_first_packet_boost(
        self: Arc<Self>,
        first_packet_boost: FirstPacketBoostConfig,
    ) -> Arc<Self> {
        if self.first_packet_boost == first_packet_boost {
            return self;
        }
        match Arc::try_unwrap(self) {
            Ok(mut inner) => {
                inner.first_packet_boost = first_packet_boost;
                Arc::new(inner)
            }
            Err(arc) => Arc::new(Self {
                router: Arc::clone(&arc.router),
                outbounds: arc.outbounds.clone(),
                dns: arc.dns.clone(),
                sniffing: Arc::clone(&arc.sniffing),
                profile: arc.profile,
                splice_policy: arc.splice_policy,
                relay_policy: arc.relay_policy,
                linux_policy: arc.linux_policy,
                vision_policy: arc.vision_policy,
                first_packet_boost,
                connection_plans: Arc::clone(&arc.connection_plans),
            }),
        }
    }

    /// Set compiled connection-plan labels indexed by inbound tag.
    pub fn with_connection_plans(
        self: Arc<Self>,
        plans: Arc<HashMap<String, Arc<str>>>,
    ) -> Arc<Self> {
        match Arc::try_unwrap(self) {
            Ok(mut inner) => {
                inner.connection_plans = plans;
                Arc::new(inner)
            }
            Err(arc) => Arc::new(Self {
                router: Arc::clone(&arc.router),
                outbounds: arc.outbounds.clone(),
                dns: arc.dns.clone(),
                sniffing: Arc::clone(&arc.sniffing),
                profile: arc.profile,
                splice_policy: arc.splice_policy,
                relay_policy: arc.relay_policy,
                linux_policy: arc.linux_policy,
                vision_policy: arc.vision_policy,
                first_packet_boost: arc.first_packet_boost,
                connection_plans: plans,
            }),
        }
    }
}

#[async_trait]
impl Dispatcher for DefaultDispatcher {
    async fn dispatch(
        &self,
        ctx: Context,
        dest: Address,
        inbound_stream: BoxedStream,
    ) -> Result<(), ProxyError> {
        self.dispatch_with_early_payload(ctx, dest, inbound_stream, None)
            .await
    }

    async fn dispatch_with_early_payload(
        &self,
        mut ctx: Context,
        mut dest: Address,
        mut inbound_stream: BoxedStream,
        early_payload: Option<Vec<u8>>,
    ) -> Result<(), ProxyError> {
        let sniff_cfg = self.sniffing.load().get(&*ctx.inbound_tag).cloned();
        if let Some(cfg) = sniff_cfg {
            if cfg.enabled {
                let (stream, mut sniff) = crate::sniff::sniff_stream(inbound_stream, &cfg).await?;
                inbound_stream = stream;
                // FakeDNS sniffing: metadata-only (no byte peek) — check if dest is a fake IP.
                if sniff.domain.is_none() && cfg.dest_override.iter().any(|p| p == "fakedns") {
                    if let Some(dns) = &self.dns {
                        sniff = crate::sniff::sniff_fakedns(&dest, dns);
                    }
                }
                dest = crate::sniff::apply_dest_override(dest, &sniff, &cfg);
                ctx = ctx.with_sniff(sniff.protocol, sniff.domain);
            }
        }

        let start = Instant::now();
        let early_payload_len = early_payload.as_ref().map_or(0, Vec::len);
        if early_payload_len > 0 {
            record_early_payload(&ctx.inbound_tag, early_payload_len as u64);
            if self.first_packet_boost.enabled && self.first_packet_boost.send_early_payload {
                record_first_packet_boost("early_payload");
            }
        }
        let outbound = match self
            .connect_outbound_result(&ctx, &dest, early_payload.clone())
            .await
        {
            Ok(outbound) => outbound,
            Err(e) => {
                let _ = inbound_stream.shutdown().await;
                let elapsed = start.elapsed();
                metrics::counter!(
                    "proxy_relay_first_byte_failures_total",
                    "inbound" => ctx.inbound_tag.clone()
                )
                .increment(1);
                record_relay_error(&ctx.inbound_tag);
                record_connection_closed(&ctx.inbound_tag, 0, 0, elapsed);
                return Err(e);
            }
        };
        if outbound.connect.wrote_early_payload {
            record_handshake_kick(&ctx.inbound_tag, "upstream", "written");
        } else if let Some(payload) = early_payload {
            if !payload.is_empty() {
                inbound_stream = Box::new(PrependedStream::new(inbound_stream, payload));
            }
        }
        let prewritten_early_up = if outbound.connect.wrote_early_payload {
            early_payload_len as u64
        } else {
            0
        };
        let outbound_stream = match outbound.connect.returned_early_response {
            Some(response) if !response.is_empty() => {
                Box::new(PrependedStream::new(outbound.connect.stream, response)) as BoxedStream
            }
            _ => outbound.connect.stream,
        };
        let (inbound_stream, outbound_stream, prewritten_up) = self
            .guard_pooled_first_write(&ctx, &dest, inbound_stream, outbound_stream)
            .await?;

        relay_log!(self.profile, dest = %dest, inbound = %ctx.inbound_tag, "relay started");
        let protocol = ctx
            .sniffed_protocol
            .as_deref()
            .map(Protocol::from)
            .unwrap_or(Protocol::Tcp);
        let conn = global_manager().track(
            Arc::clone(&ctx.inbound_tag),
            Arc::clone(&outbound.outbound_tag),
            ctx.user.as_ref().map(Arc::clone),
            protocol,
            Transport::Tcp,
            RelayPath::Adaptive,
        );
        let cancel = conn.cancellation_token();

        // Relay bytes bidirectionally until either side closes.
        //
        // The relay helper uses Linux splice(2) for raw TCP-to-TCP streams and
        // falls back to copy_bidirectional for every other stream type.
        //
        // Both paths run two concurrent copy loops:
        //   inbound → outbound (client sending data to the server)
        //   outbound → inbound (server sending data back to the client)
        //
        // It returns the total bytes sent in each direction when finished.
        let relay = crate::relay::relay_bidirectional_with_policies(
            inbound_stream,
            outbound_stream,
            self.splice_policy,
            self.relay_policy,
            self.linux_policy,
            self.vision_policy,
        );
        tokio::pin!(relay);
        let result = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(ProxyError::Transport("connection closed by manager".into())),
            result = &mut relay => result
                .map(|(up, down)| (up + prewritten_up + prewritten_early_up, down))
                .map_err(ProxyError::Io),
        };

        let elapsed = start.elapsed();
        let close_reason = if cancel.is_cancelled() {
            CloseReason::ClosedById
        } else if result.is_ok() {
            CloseReason::Completed
        } else {
            CloseReason::Error
        };

        match &result {
            Ok((up, down)) => {
                relay_log!(
                    self.profile,
                    dest = %dest,
                    inbound = %ctx.inbound_tag,
                    uplink_bytes = up,
                    downlink_bytes = down,
                    duration_ms = elapsed.as_millis(),
                    "relay finished"
                );
            }
            Err(e) => {
                metrics::counter!(
                    "proxy_relay_first_byte_failures_total",
                    "inbound" => ctx.inbound_tag.clone()
                )
                .increment(1);
                debug!(
                    dest = %dest,
                    inbound = %ctx.inbound_tag,
                    error = %e,
                    "relay error"
                );
                record_relay_error(&ctx.inbound_tag);
            }
        }

        let (rx_bytes, tx_bytes) = result.unwrap_or((0, 0));
        conn.finish(rx_bytes, tx_bytes, close_reason);
        record_connection_closed(&ctx.inbound_tag, rx_bytes, tx_bytes, elapsed);
        if let Some(user) = ctx.user.as_deref() {
            runtime_stats::record_user_traffic(user, rx_bytes, tx_bytes);
        }

        Ok(())
    }

    async fn connect_outbound(
        &self,
        ctx: &Context,
        dest: &Address,
    ) -> Result<BoxedStream, ProxyError> {
        DefaultDispatcher::connect_outbound(self, ctx, dest).await
    }
}

impl DefaultDispatcher {
    async fn guard_pooled_first_write(
        &self,
        ctx: &Context,
        dest: &Address,
        mut inbound_stream: BoxedStream,
        outbound_stream: BoxedStream,
    ) -> Result<(BoxedStream, BoxedStream, u64), ProxyError> {
        let inbound_tag: &str = &ctx.inbound_tag;
        if self.profile != ProfileMode::Fast
            || !(*outbound_stream).as_any().is::<PooledStream<TcpStream>>()
        {
            return Ok((inbound_stream, outbound_stream, 0));
        }

        let any = outbound_stream.into_any();
        let pooled = any
            .downcast::<PooledStream<TcpStream>>()
            .expect("stream type checked as PooledStream<TcpStream> before downcast");
        let (mut outbound, pool_tag, peer_addr) = pooled.into_metadata_parts();
        let pool_label = pool_tag.as_deref().unwrap_or("unknown");

        // Keep this small to avoid a per-connection heap allocation here.
        let mut first = [0u8; POOLED_FIRST_WRITE_GUARD_BUF_SIZE];
        let n = match tokio::time::timeout(
            POOLED_FIRST_WRITE_GUARD_TIMEOUT,
            inbound_stream.read(&mut first),
        )
        .await
        {
            Ok(read) => read.map_err(ProxyError::Io)?,
            Err(_) => {
                metrics::counter!(
                    "freedom_pool_first_use_guard_skipped_total",
                    "inbound" => inbound_tag.to_owned(),
                    "outbound" => pool_label.to_owned(),
                    "reason" => "client_first_timeout"
                )
                .increment(1);
                return Ok((inbound_stream, Box::new(outbound), 0));
            }
        };
        if n == 0 {
            return Ok((inbound_stream, Box::new(outbound), 0));
        }

        if outbound.write_all(&first[..n]).await.is_err() {
            metrics::counter!(
                "freedom_pool_first_use_retries_total",
                "inbound" => inbound_tag.to_owned(),
                "outbound" => pool_label.to_owned()
            )
            .increment(1);

            let fresh_result: Result<BoxedStream, ProxyError> = if let Some(addr) = peer_addr {
                match tcp_connect(addr).await {
                    Ok(stream) => Ok(Box::new(stream)),
                    Err(e) => Err(e),
                }
            } else {
                self.connect_outbound(ctx, dest).await
            };

            let mut fresh = match fresh_result {
                Ok(stream) => stream,
                Err(e) => {
                    metrics::counter!(
                        "freedom_pool_fresh_retry_failures_total",
                        "inbound" => inbound_tag.to_owned(),
                        "outbound" => pool_label.to_owned()
                    )
                    .increment(1);
                    return Err(e);
                }
            };

            match fresh.write_all(&first[..n]).await {
                Ok(()) => {
                    metrics::counter!(
                        "freedom_pool_fresh_retry_success_total",
                        "inbound" => inbound_tag.to_owned(),
                        "outbound" => pool_label.to_owned()
                    )
                    .increment(1);
                    return Ok((inbound_stream, fresh, n as u64));
                }
                Err(e) => {
                    metrics::counter!(
                        "freedom_pool_fresh_retry_failures_total",
                        "inbound" => inbound_tag.to_owned(),
                        "outbound" => pool_label.to_owned()
                    )
                    .increment(1);
                    return Err(ProxyError::Io(e));
                }
            };
        }

        metrics::counter!(
            "freedom_pool_hits_total",
            "outbound" => pool_label.to_owned()
        )
        .increment(1);

        Ok((inbound_stream, Box::new(outbound), n as u64))
    }

    /// Route and dial the destination without starting a relay loop.
    pub async fn connect_outbound(
        &self,
        ctx: &Context,
        dest: &Address,
    ) -> Result<BoxedStream, ProxyError> {
        Ok(self
            .connect_outbound_result(ctx, dest, None)
            .await?
            .connect
            .stream)
    }

    async fn connect_outbound_result(
        &self,
        ctx: &Context,
        dest: &Address,
        early_payload: Option<Vec<u8>>,
    ) -> Result<RoutedOutboundConnectResult, ProxyError> {
        let dest = self.restore_fakeip_destination(dest);

        let protocol_label = ctx.sniffed_protocol.as_deref().unwrap_or("tcp");
        record_connection_accepted(&ctx.inbound_tag, protocol_label);

        let t_route = Instant::now();
        let route = self
            .pick_route_xray(
                &ctx.inbound_tag,
                &dest,
                ctx.user.as_deref(),
                ctx.sniffed_protocol.as_deref(),
                ctx.sniffed_domain.as_deref(),
            )
            .await?;
        record_route(&ctx.inbound_tag, t_route.elapsed());

        relay_log!(self.profile, outbound = %route.outbound_tag, "route selected");
        let plan_label = self
            .connection_plans
            .get(&*ctx.inbound_tag)
            .map(|label| label.as_ref())
            .unwrap_or("dynamic");
        record_connection_plan_selected(plan_label);
        let route_tag = Arc::clone(&route.outbound_tag);

        let outbound = self
            .outbounds
            .get(route.outbound_tag.as_ref())
            .ok_or_else(|| {
                ProxyError::Protocol(format!("outbound '{}' not found", route.outbound_tag))
            })?;

        let t_connect = Instant::now();
        let result = tokio::time::timeout(
            OUTBOUND_CONNECT_TIMEOUT,
            outbound.connect_with_early_payload(ctx, &dest, early_payload),
        )
        .await
        .map_err(|_| ProxyError::Timeout)
        .and_then(|result| result)
        .map_err(|e| {
            warn!(
                outbound = %route.outbound_tag,
                dest = %dest,
                error = %e,
                "outbound connect failed"
            );
            e
        });
        if let Ok(result) = &result {
            if result.wrote_early_payload {
                record_early_payload_written(&ctx.inbound_tag, &route.outbound_tag);
                record_first_byte_latency(protocol_label, &route.outbound_tag, t_connect.elapsed());
            }
        }
        record_outbound_connect(&ctx.inbound_tag, &route.outbound_tag, t_connect.elapsed());
        result.map(|connect| RoutedOutboundConnectResult {
            connect,
            outbound_tag: route_tag,
        })
    }

    /// Xray routing: https://xtls.github.io/en/config/routing.html#domainstrategy
    async fn pick_route_xray(
        &self,
        inbound_tag: &str,
        dest: &Address,
        user: Option<&str>,
        sniffed_protocol: Option<&str>,
        sniffed_domain: Option<&str>,
    ) -> Result<crate::router::Route, ProxyError> {
        let strategy = normalize_routing_domain_strategy(self.router.domain_strategy().as_deref());

        if strategy == RoutingDomainStrategy::IpOnDemand
            && matches!(dest, Address::Domain(..))
            && self.router.has_ip_rules()
        {
            if let Some(ips) = self.resolve_domain_ips(dest, inbound_tag).await {
                for ip_dest in &ips {
                    let ctx = Self::routing_ctx(
                        ip_dest,
                        inbound_tag,
                        user,
                        sniffed_protocol,
                        sniffed_domain,
                    );
                    let (route, matched) = self.router.pick_route_match(&ctx);
                    if matched {
                        return Ok(route);
                    }
                }
            }
        }

        // For IpIfNonMatch on a domain destination, start DNS resolution in the
        // background immediately — before we know whether a domain rule will match.
        // The domain rule check is synchronous and fast; by overlapping it with
        // DNS we avoid serialising the two when the domain rule misses and IP rules
        // need to be consulted. A tokio task is spawned only when DNS is configured
        // AND the router has IP-based rules (otherwise DNS would be pointless).
        let dns_prefetch_allowed = !self.first_packet_boost.enabled || self.first_packet_boost.dns;
        let prefetch = if dns_prefetch_allowed
            && strategy == RoutingDomainStrategy::IpIfNonMatch
            && matches!(dest, Address::Domain(..))
            && self.router.has_ip_rules()
        {
            if self.first_packet_boost.enabled {
                record_first_packet_boost("dns_prefetch");
            }
            self.prefetch_dns(dest)
        } else {
            None
        };

        let ctx = Self::routing_ctx(dest, inbound_tag, user, sniffed_protocol, sniffed_domain);
        let (route, matched) = self.router.pick_route_match(&ctx);
        if matched || strategy == RoutingDomainStrategy::AsIs {
            if let Some(h) = prefetch {
                h.abort();
            }
            return Ok(route);
        }

        if strategy == RoutingDomainStrategy::IpIfNonMatch {
            if let Address::Domain(_, _) = dest {
                // Await the pre-fetched DNS result (may already be ready).
                let ips = if let Some(h) = prefetch {
                    h.await.ok().flatten()
                } else {
                    self.resolve_domain_ips(dest, inbound_tag).await
                };
                if let Some(ips) = ips {
                    for ip_dest in &ips {
                        let ctx = Self::routing_ctx(
                            ip_dest,
                            inbound_tag,
                            user,
                            sniffed_protocol,
                            sniffed_domain,
                        );
                        let (route, matched) = self.router.pick_route_match(&ctx);
                        if matched {
                            return Ok(route);
                        }
                    }
                }
            }
        }

        Ok(route)
    }

    /// Spawn a background task that resolves `dest` to IP addresses.
    ///
    /// Returns `None` when no DNS module is configured. The handle can be
    /// awaited for the result or aborted if the resolution is no longer needed.
    fn prefetch_dns(
        &self,
        dest: &Address,
    ) -> Option<tokio::task::JoinHandle<Option<SmallVec<[Address; 4]>>>> {
        let Address::Domain(name, port) = dest else {
            return None;
        };
        let dns = self.dns.clone()?;
        let name = name.clone();
        let port = *port;
        Some(tokio::spawn(async move {
            let domain = name.as_str();
            // Inline version of resolve_domain_ips without borrowing self.
            let mut ips: SmallVec<[Address; 4]> = SmallVec::new();
            let resolved = tokio::time::timeout(ROUTING_DNS_TIMEOUT, dns.resolve(domain)).await;
            match resolved {
                Ok(Ok(addrs)) => {
                    for ip in addrs {
                        ips.push(match ip {
                            std::net::IpAddr::V4(v4) => Address::Ipv4(v4, port),
                            std::net::IpAddr::V6(v6) => Address::Ipv6(v6, port),
                        });
                    }
                }
                Err(_) => {
                    record_dns_prefetch("timeout");
                    return None;
                }
                Ok(Err(_)) => {}
            }
            if ips.is_empty() {
                let lookup = tokio::time::timeout(
                    ROUTING_DNS_TIMEOUT,
                    tokio::net::lookup_host((domain, port)),
                );
                match lookup.await {
                    Ok(Ok(addrs)) => {
                        for addr in addrs {
                            ips.push(match addr {
                                std::net::SocketAddr::V4(v4) => Address::Ipv4(*v4.ip(), port),
                                std::net::SocketAddr::V6(v6) => Address::Ipv6(*v6.ip(), port),
                            });
                        }
                    }
                    Err(_) => {
                        record_dns_prefetch("timeout");
                        return None;
                    }
                    Ok(Err(_)) => {}
                }
            }
            if ips.is_empty() {
                record_dns_prefetch("miss");
                None
            } else {
                record_dns_prefetch("hit");
                Some(ips)
            }
        }))
    }

    fn routing_ctx<'a>(
        dest: &'a Address,
        inbound_tag: &'a str,
        user: Option<&'a str>,
        sniffed_protocol: Option<&'a str>,
        sniffed_domain: Option<&'a str>,
    ) -> crate::router::RoutingContext<'a> {
        crate::router::RoutingContext {
            dest,
            network: blackwire_common::Network::Tcp,
            inbound_tag,
            user,
            sniffed_protocol,
            sniffed_domain,
        }
    }

    async fn resolve_domain_ips(
        &self,
        dest: &Address,
        inbound_tag: &str,
    ) -> Option<SmallVec<[Address; 4]>> {
        let Address::Domain(name, port) = dest else {
            return None;
        };
        let t_dns = Instant::now();
        let mut ips: SmallVec<[Address; 4]> = SmallVec::new();
        if let Some(dns) = &self.dns {
            let resolved = tokio::time::timeout(ROUTING_DNS_TIMEOUT, dns.resolve(name)).await;
            if let Ok(Ok(addrs)) = resolved {
                for ip in addrs {
                    ips.push(match ip {
                        std::net::IpAddr::V4(v4) => Address::Ipv4(v4, *port),
                        std::net::IpAddr::V6(v6) => Address::Ipv6(v6, *port),
                    });
                }
            }
        }
        if ips.is_empty() {
            let lookup = tokio::time::timeout(
                ROUTING_DNS_TIMEOUT,
                tokio::net::lookup_host((name.as_str(), *port)),
            );
            if let Ok(Ok(addrs)) = lookup.await {
                for addr in addrs {
                    ips.push(match addr {
                        std::net::SocketAddr::V4(v4) => Address::Ipv4(*v4.ip(), *port),
                        std::net::SocketAddr::V6(v6) => Address::Ipv6(*v6.ip(), *port),
                    });
                }
            }
        }
        record_dns(inbound_tag, t_dns.elapsed());
        if ips.is_empty() {
            None
        } else {
            Some(ips)
        }
    }

    fn restore_fakeip_destination<'a>(&self, dest: &'a Address) -> Cow<'a, Address> {
        let Some(dns) = &self.dns else {
            return Cow::Borrowed(dest);
        };

        match dest {
            Address::Ipv4(ip, port) if dns.is_fake_ip(std::net::IpAddr::V4(*ip)) => {
                let resolved = dns
                    .reverse_fake(*ip)
                    .map(|domain| Address::Domain(domain, *port))
                    .unwrap_or(Address::Ipv4(*ip, *port));
                Cow::Owned(resolved)
            }
            _ => Cow::Borrowed(dest),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::DnsModuleConfig;
    use crate::router::{Route, RoutingContext};
    use blackwire_config::schema::{
        FastLinuxConfig, FastPoolPolicy, FastRelayConfig, FastRelayEngine, FastRelayFlushPolicy,
        FastSplicePolicy, VisionConfig, VisionDirectCopyPolicy,
    };

    struct StaticRouter;

    impl Router for StaticRouter {
        fn pick_route_match(&self, _ctx: &RoutingContext<'_>) -> (Route, bool) {
            (
                Route {
                    outbound_tag: Arc::from("unused"),
                },
                false,
            )
        }
    }

    #[test]
    fn compat_profile_uses_adaptive_splice_by_default() {
        assert_eq!(
            splice_policy_for_profile(ProfileMode::Compat, None),
            FastSplicePolicy::Adaptive
        );
    }

    #[test]
    fn fast_profile_honors_configured_splice_policy() {
        let fast = FastConfig {
            splice: FastSplicePolicy::Always,
            pool: FastPoolPolicy::Disabled,
            relay: FastRelayConfig::default(),
            linux: FastLinuxConfig::default(),
            strict_production: false,
        };
        assert_eq!(
            splice_policy_for_profile(ProfileMode::Fast, Some(&fast)),
            FastSplicePolicy::Always
        );
    }

    #[test]
    fn fast_profile_honors_configured_relay_policy() {
        let fast = FastConfig {
            splice: FastSplicePolicy::Disabled,
            pool: FastPoolPolicy::Disabled,
            relay: FastRelayConfig {
                engine: FastRelayEngine::V2,
                flush: FastRelayFlushPolicy::Deferred,
                initial_buffer: 4096,
                max_buffer: 65536,
            },
            linux: FastLinuxConfig::default(),
            strict_production: false,
        };
        assert_eq!(
            relay_policy_for_profile(ProfileMode::Fast, Some(&fast)),
            fast.relay
        );
    }

    #[test]
    fn dispatcher_honors_configured_vision_policy() {
        let dispatcher =
            DefaultDispatcher::new(Arc::new(StaticRouter), std::collections::HashMap::new())
                .with_profile_fast_and_vision(
                    ProfileMode::Fast,
                    None,
                    Some(&VisionConfig {
                        direct_copy: VisionDirectCopyPolicy::Disabled,
                        max_packets_to_filter: 4,
                        allow_splice_after_direct: false,
                    }),
                );

        assert_eq!(
            dispatcher.vision_policy,
            VisionConfig {
                direct_copy: VisionDirectCopyPolicy::Disabled,
                max_packets_to_filter: 4,
                allow_splice_after_direct: false,
            }
        );
    }

    #[tokio::test]
    async fn dispatcher_restores_fakeip_destination_before_routing() {
        let dns = Arc::new(
            DnsModule::new(DnsModuleConfig {
                fake_ip_enabled: true,
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let fake = dns.resolve_fake("example.com").unwrap();
        let dispatcher = DefaultDispatcher::new_with_dns(
            Arc::new(StaticRouter),
            std::collections::HashMap::new(),
            dns,
        );

        let fake_addr = Address::Ipv4(fake, 443);
        let restored = dispatcher.restore_fakeip_destination(&fake_addr);
        assert_eq!(
            restored.into_owned(),
            Address::Domain("example.com".into(), 443)
        );
    }

    #[tokio::test]
    async fn dispatcher_keeps_unknown_fakeip_as_ip_destination() {
        let dns = Arc::new(
            DnsModule::new(DnsModuleConfig {
                fake_ip_enabled: true,
                ..Default::default()
            })
            .await
            .unwrap(),
        );
        let dispatcher = DefaultDispatcher::new_with_dns(
            Arc::new(StaticRouter),
            std::collections::HashMap::new(),
            dns,
        );

        let ip = "198.18.0.100".parse().unwrap();
        let addr = Address::Ipv4(ip, 443);
        let restored = dispatcher.restore_fakeip_destination(&addr);
        assert_eq!(*restored, Address::Ipv4(ip, 443));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pooled_first_write_retries_stale_socket_with_early_payload() {
        use std::os::fd::AsRawFd;

        use blackwire_common::{PooledStream, PrependedStream};
        use tokio::io::AsyncReadExt;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let pooled_client = TcpStream::connect(addr).await.unwrap();
        let (stale_server, _) = listener.accept().await.unwrap();
        let stale_server = stale_server.into_std().unwrap();
        let linger = libc::linger {
            l_onoff: 1,
            l_linger: 0,
        };
        let rc = unsafe {
            libc::setsockopt(
                stale_server.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_LINGER,
                (&linger as *const libc::linger).cast(),
                std::mem::size_of::<libc::linger>() as libc::socklen_t,
            )
        };
        assert_eq!(rc, 0, "setsockopt(SO_LINGER) failed");
        drop(stale_server);
        drop(listener);

        let fresh_listener = TcpListener::bind(addr).await.unwrap();
        let expected = b"early payload through stale retry".to_vec();
        let expected_for_task = expected.clone();
        let read_task = tokio::spawn(async move {
            let (mut fresh, _) = fresh_listener.accept().await.unwrap();
            let mut got = vec![0u8; expected_for_task.len()];
            fresh.read_exact(&mut got).await.unwrap();
            got
        });

        let dispatcher =
            DefaultDispatcher::new(Arc::new(StaticRouter), std::collections::HashMap::new())
                .with_profile(ProfileMode::Fast);
        let (_client_side, server_side) = tokio::io::duplex(64);
        let inbound: BoxedStream = Box::new(PrependedStream::new(
            Box::new(server_side),
            expected.clone(),
        ));
        let outbound: BoxedStream = Box::new(PooledStream::with_pool_metadata(
            pooled_client,
            "freedom".to_string(),
            addr,
        ));
        let ctx = Context::new("socks-in", addr);

        let (_inbound, _fresh, prewritten) = dispatcher
            .guard_pooled_first_write(
                &ctx,
                &Address::Ipv4("127.0.0.1".parse().unwrap(), addr.port()),
                inbound,
                outbound,
            )
            .await
            .unwrap();

        assert_eq!(prewritten, expected.len() as u64);
        assert_eq!(read_task.await.unwrap(), expected);
    }
}
