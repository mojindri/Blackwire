//! Prometheus metrics + HTTP health/readiness endpoint.
//!
//! Starts a lightweight HTTP server (using axum 0.8) that exposes:
//!
//! - `GET /healthz` → 200 OK with body `"ok"`
//! - `GET /readyz`  → 200 OK when the instance is ready
//! - `GET /metrics` → Prometheus text format
//! - `GET /version` → JSON `{"version":"0.1.0"}`
//!
//! # Selected metrics
//!
//! | Metric | Type | Labels |
//! |--------|------|--------|
//! | `proxy_connections_total` | Counter | `inbound`, `protocol` |
//! | `proxy_bytes_total` | Counter | `direction` (rx/tx), `inbound` |
//! | `proxy_active_connections` | Gauge | `inbound` |
//! | `proxy_connection_duration_seconds` | Histogram | `inbound` |
//! | `proxy_inbound_parse_seconds` | Histogram | `inbound` |
//! | `proxy_route_seconds` | Histogram | `inbound` |
//! | `proxy_dns_seconds` | Histogram | `inbound` |
//! | `proxy_outbound_connect_seconds` | Histogram | `inbound`, `outbound` |
//! | `proxy_relay_errors_total` | Counter | `inbound` |
//! | `proxy_relay_first_byte_failures_total` | Counter | `inbound` |
//! | `proxy_relay_splice_selected_total` | Counter | `policy` |
//! | `proxy_relay_splice_fallback_total` | Counter | `reason` |
//! | `proxy_relay_bytes_total` | Counter | `direction`, `path` |
//! | `proxy_relay_v2_flushes_total` | Counter | none |
//! | `proxy_relay_v2_buffer_grows_total` | Counter | none |
//! | `blackwire_relay_v2_selected_total` | Counter | `path`, `profile` |
//! | `blackwire_vision_phase_total` | Counter | `phase` |
//! | `blackwire_vision_direct_copy_ready_total` | Counter | none |
//! | `blackwire_vision_direct_copy_active_total` | Counter | none |
//! | `blackwire_vision_lower_failed_total` | Counter | `reason` |
//! | `blackwire_vision_cached_bytes_total` | Counter | none |
//! | `blackwire_vision_splice_after_direct_total` | Counter | none |
//! | `blackwire_early_payload_bytes_total` | Counter | `protocol` |
//! | `blackwire_early_payload_written_total` | Counter | `protocol`, `outbound` |
//! | `blackwire_handshake_kick_total` | Counter | `protocol`, `direction`, `result` |
//! | `blackwire_first_byte_latency_seconds` | Histogram | `protocol`, `transport` |
//! | `blackwire_route_match_seconds` | Histogram | none |
//! | `blackwire_route_cache_hits_total` | Counter | none |
//! | `blackwire_route_cache_misses_total` | Counter | none |
//! | `blackwire_route_compiled_rules_total` | Gauge | `kind` |
//! | `blackwire_dns_prefetch_total` | Counter | `result` |
//! | `freedom_pool_leases_total` | Counter | `outbound` |
//! | `balancer_adaptive_profile_score` | Gauge | `balancer`, `profile` |
//! | `balancer_adaptive_profile_selected` | Gauge | `balancer`, `profile` |
//! | `balancer_adaptive_selections_total` | Counter | `balancer`, `profile` |
//! | `balancer_adaptive_cooldowns_total` | Counter | `balancer`, `profile` |
//! | `balancer_adaptive_connect_success_total` | Counter | `balancer`, `profile` |
//! | `balancer_adaptive_connect_failures_total` | Counter | `balancer`, `profile` |
//!
//! # Usage
//!
//! Call [`start_metrics_server`] once during startup to bind the HTTP server.
//! Recording metrics is done via the `metrics` crate macros anywhere in the
//! codebase after the recorder has been installed.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::runtime_stats;

static METRICS_ENABLED: AtomicBool = AtomicBool::new(false);

#[inline]
fn metrics_enabled() -> bool {
    METRICS_ENABLED.load(Ordering::Relaxed)
}

/// Shared state for the metrics HTTP server.
#[derive(Clone)]
struct MetricsState {
    prometheus_handle: Arc<PrometheusHandle>,
    ready: bool,
}

/// Start the metrics HTTP server.
///
/// Installs the Prometheus recorder globally and starts listening on `addr`.
/// Call this once at proxy startup.
///
/// # Arguments
/// * `addr` — bind address, e.g. `"127.0.0.1:8080"`
///
/// # Returns
/// A `JoinHandle` for the background server task. Keep alive as long as
/// the proxy is running.
///
/// # Errors
/// Returns an error if the address is invalid or the server fails to bind.
pub fn start_metrics_server(addr: &str) -> anyhow::Result<JoinHandle<()>> {
    let addr: SocketAddr = addr
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid metrics addr '{addr}': {e}"))?;

    // Install the Prometheus recorder.
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("failed to install Prometheus recorder: {e}"))?;
    METRICS_ENABLED.store(true, Ordering::Relaxed);

    // Describe metrics so Prometheus scrape shows help text.
    describe_metrics();

    let state = MetricsState {
        prometheus_handle: Arc::new(handle),
        ready: true,
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics_handler))
        .route("/version", get(version_handler))
        .with_state(state);

    let std_listener = std::net::TcpListener::bind(addr)
        .map_err(|e| anyhow::anyhow!("metrics server failed to bind {addr}: {e}"))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| anyhow::anyhow!("metrics server failed to set nonblocking {addr}: {e}"))?;
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| anyhow::anyhow!("metrics server failed to adopt listener {addr}: {e}"))?;

    let task = tokio::spawn(async move {
        info!(addr = %addr, "metrics server starting");
        if let Err(e) = axum::serve(listener, app).await {
            error!(error = %e, "metrics server error");
        }
    });

    Ok(task)
}

/// Describe all metrics so the Prometheus scrape output includes help/type annotations.
fn describe_metrics() {
    metrics::describe_counter!(
        "proxy_connections_total",
        metrics::Unit::Count,
        "Total number of proxy connections accepted"
    );
    metrics::describe_counter!(
        "proxy_bytes_total",
        metrics::Unit::Bytes,
        "Total bytes relayed through the proxy"
    );
    metrics::describe_gauge!(
        "proxy_active_connections",
        metrics::Unit::Count,
        "Currently open proxy connections"
    );
    metrics::describe_histogram!(
        "proxy_connection_duration_seconds",
        metrics::Unit::Seconds,
        "Connection lifetime in seconds"
    );
    metrics::describe_histogram!(
        "proxy_inbound_parse_seconds",
        metrics::Unit::Seconds,
        "Time to decode the inbound protocol header (VLESS/Trojan/etc.)"
    );
    metrics::describe_histogram!(
        "proxy_route_seconds",
        metrics::Unit::Seconds,
        "Time to select an outbound via the routing engine"
    );
    metrics::describe_histogram!(
        "proxy_dns_seconds",
        metrics::Unit::Seconds,
        "Time spent in DNS resolution during routing (IpOnDemand / IpIfNonMatch)"
    );
    metrics::describe_histogram!(
        "proxy_outbound_connect_seconds",
        metrics::Unit::Seconds,
        "Time to establish the outbound connection (TCP dial + TLS/REALITY handshake)"
    );
    metrics::describe_counter!(
        "proxy_relay_errors_total",
        metrics::Unit::Count,
        "Total relay errors by inbound"
    );
    metrics::describe_counter!(
        "proxy_relay_first_byte_failures_total",
        metrics::Unit::Count,
        "Relay errors before the dispatcher could record transferred bytes"
    );
    metrics::describe_counter!(
        "proxy_relay_splice_selected_total",
        metrics::Unit::Count,
        "Raw TCP relays selected for splice by policy"
    );
    metrics::describe_counter!(
        "proxy_relay_splice_fallback_total",
        metrics::Unit::Count,
        "Raw TCP relays that fell back from splice"
    );
    metrics::describe_counter!(
        "proxy_relay_bytes_total",
        metrics::Unit::Bytes,
        "Bytes relayed by path-specific relay implementation"
    );
    metrics::describe_counter!(
        "proxy_relay_v2_flushes_total",
        metrics::Unit::Count,
        "Flush operations performed by Relay Engine v2"
    );
    metrics::describe_counter!(
        "proxy_relay_v2_buffer_grows_total",
        metrics::Unit::Count,
        "Dynamic buffer growth events performed by Relay Engine v2"
    );
    metrics::describe_counter!(
        "blackwire_relay_v2_selected_total",
        metrics::Unit::Count,
        "Relay v2 or lowering-aware path selections by path and profile"
    );
    metrics::describe_counter!(
        "blackwire_vision_phase_total",
        metrics::Unit::Count,
        "XTLS Vision lower-state observations by phase"
    );
    metrics::describe_counter!(
        "blackwire_vision_direct_copy_ready_total",
        metrics::Unit::Count,
        "XTLS Vision streams that became ready for direct copy"
    );
    metrics::describe_counter!(
        "blackwire_vision_direct_copy_active_total",
        metrics::Unit::Count,
        "XTLS Vision streams that activated direct copy"
    );
    metrics::describe_counter!(
        "blackwire_vision_lower_failed_total",
        metrics::Unit::Count,
        "XTLS Vision lowering failures by reason"
    );
    metrics::describe_counter!(
        "blackwire_vision_cached_bytes_total",
        metrics::Unit::Bytes,
        "Cached XTLS Vision bytes drained while lowering"
    );
    metrics::describe_counter!(
        "blackwire_vision_splice_after_direct_total",
        metrics::Unit::Count,
        "XTLS Vision streams that entered splice after direct copy"
    );
    metrics::describe_counter!(
        "blackwire_early_payload_bytes_total",
        metrics::Unit::Bytes,
        "Inbound bytes captured after protocol handshake and forwarded as early payload"
    );
    metrics::describe_counter!(
        "blackwire_early_payload_written_total",
        metrics::Unit::Count,
        "Early payload write attempts by protocol and outbound path"
    );
    metrics::describe_counter!(
        "blackwire_handshake_kick_total",
        metrics::Unit::Count,
        "Handshake kick events by protocol, direction, and result"
    );
    metrics::describe_histogram!(
        "blackwire_first_byte_latency_seconds",
        metrics::Unit::Seconds,
        "Latency until the first upstream byte can be written"
    );
    metrics::describe_counter!(
        "blackwire_first_packet_boost_total",
        metrics::Unit::Count,
        "First-packet boost decisions by kind"
    );
    metrics::describe_histogram!(
        "blackwire_ttfb_seconds",
        metrics::Unit::Seconds,
        "Time-to-first-byte by protocol and transport"
    );
    metrics::describe_counter!(
        "blackwire_connection_plan_selected_total",
        metrics::Unit::Count,
        "Compiled connection plan selections"
    );
    metrics::describe_histogram!(
        "blackwire_route_match_seconds",
        metrics::Unit::Seconds,
        "Compiled router match latency in seconds"
    );
    metrics::describe_counter!(
        "blackwire_route_cache_hits_total",
        metrics::Unit::Count,
        "Compiled router cache hits"
    );
    metrics::describe_counter!(
        "blackwire_route_cache_misses_total",
        metrics::Unit::Count,
        "Compiled router cache misses"
    );
    metrics::describe_gauge!(
        "blackwire_route_compiled_rules_total",
        metrics::Unit::Count,
        "Compiled routing rules by rule kind"
    );
    metrics::describe_counter!(
        "blackwire_dns_prefetch_total",
        metrics::Unit::Count,
        "Background routing DNS prefetch outcomes"
    );
    metrics::describe_counter!(
        "freedom_pool_hits_total",
        metrics::Unit::Count,
        "Freedom outbound preconnect pool hits after first client write succeeds"
    );
    metrics::describe_counter!(
        "freedom_pool_leases_total",
        metrics::Unit::Count,
        "Freedom outbound preconnect pool sockets leased before first-use validation"
    );
    metrics::describe_counter!(
        "freedom_pool_misses_total",
        metrics::Unit::Count,
        "Freedom outbound preconnect pool misses"
    );
    metrics::describe_counter!(
        "freedom_pool_stales_total",
        metrics::Unit::Count,
        "Freedom outbound stale pooled sockets discarded"
    );
    metrics::describe_counter!(
        "freedom_pool_errors_total",
        metrics::Unit::Count,
        "Freedom outbound background pool dial errors"
    );
    metrics::describe_counter!(
        "freedom_pool_refill_success_total",
        metrics::Unit::Count,
        "Freedom outbound background refill sockets added to the idle pool"
    );
    metrics::describe_counter!(
        "freedom_pool_refill_dropped_total",
        metrics::Unit::Count,
        "Freedom outbound background refill sockets dropped before entering the idle pool"
    );
    metrics::describe_counter!(
        "freedom_pool_first_use_retries_total",
        metrics::Unit::Count,
        "Pooled Freedom sockets discarded after failing the first client write"
    );
    metrics::describe_counter!(
        "freedom_pool_first_use_guard_skipped_total",
        metrics::Unit::Count,
        "Pooled Freedom first-use guard skipped because client bytes were not immediately available"
    );
    metrics::describe_counter!(
        "freedom_pool_fresh_retry_success_total",
        metrics::Unit::Count,
        "Fresh Freedom retries that succeeded after a pooled socket failed first use"
    );
    metrics::describe_counter!(
        "freedom_pool_fresh_retry_failures_total",
        metrics::Unit::Count,
        "Fresh Freedom retries that failed after a pooled socket failed first use"
    );
    metrics::describe_histogram!(
        "freedom_pool_idle_age_seconds",
        metrics::Unit::Seconds,
        "Age of a pooled Freedom socket when reused"
    );
    metrics::describe_gauge!(
        "freedom_pool_capacity",
        metrics::Unit::Count,
        "Current adaptive per-destination Freedom pool capacity tier"
    );
    metrics::describe_gauge!(
        "freedom_pool_hotness",
        metrics::Unit::Count,
        "Current adaptive per-destination Freedom pool hotness estimate"
    );
    metrics::describe_gauge!(
        "balancer_adaptive_profile_score",
        metrics::Unit::Count,
        "Adaptive balancer score by profile"
    );
    metrics::describe_gauge!(
        "balancer_adaptive_profile_selected",
        metrics::Unit::Count,
        "Adaptive balancer selected profile marker"
    );
    metrics::describe_counter!(
        "balancer_adaptive_selections_total",
        metrics::Unit::Count,
        "Adaptive balancer profile selections"
    );
    metrics::describe_counter!(
        "balancer_adaptive_cooldowns_total",
        metrics::Unit::Count,
        "Adaptive balancer profile cooldown entries"
    );
    metrics::describe_counter!(
        "balancer_adaptive_connect_success_total",
        metrics::Unit::Count,
        "Adaptive balancer outbound connect successes"
    );
    metrics::describe_counter!(
        "balancer_adaptive_connect_failures_total",
        metrics::Unit::Count,
        "Adaptive balancer outbound connect failures"
    );
    metrics::describe_counter!(
        "blackwire_quic_congestion_mode_total",
        metrics::Unit::Count,
        "QUIC congestion mode selections"
    );
    metrics::describe_gauge!(
        "blackwire_quic_ack_rate",
        metrics::Unit::Count,
        "Estimated QUIC ACK rate for bad-network congestion control"
    );
    metrics::describe_gauge!(
        "blackwire_quic_loss_rate",
        metrics::Unit::Count,
        "Estimated QUIC loss rate for bad-network congestion control"
    );
    metrics::describe_gauge!(
        "blackwire_quic_queue_delay_ms",
        metrics::Unit::Milliseconds,
        "Estimated QUIC queue delay"
    );
    metrics::describe_gauge!(
        "blackwire_quic_pacing_rate_bps",
        metrics::Unit::Bytes,
        "Selected QUIC pacing rate in bytes per second"
    );
    metrics::describe_gauge!(
        "blackwire_quic_cwnd_bytes",
        metrics::Unit::Bytes,
        "Selected QUIC congestion window"
    );
    metrics::describe_gauge!(
        "blackwire_quic_delivery_rate_bps",
        metrics::Unit::Bytes,
        "Estimated QUIC delivery rate in bytes per second"
    );
    metrics::describe_gauge!(
        "blackwire_quic_endpoint_shards",
        metrics::Unit::Count,
        "Configured QUIC endpoint shard count"
    );
    metrics::describe_counter!(
        "blackwire_hysteria2_pacer_sleep_total",
        metrics::Unit::Count,
        "Hysteria2 application write pacer sleep events by lane"
    );
    metrics::describe_counter!(
        "blackwire_hysteria2_pacer_sleep_ms_total",
        metrics::Unit::Milliseconds,
        "Total Hysteria2 application write pacer sleep time by lane"
    );
    metrics::describe_gauge!(
        "blackwire_hysteria2_pacer_burst_bytes",
        metrics::Unit::Bytes,
        "Hysteria2 application write pacer burst allowance by lane"
    );
    metrics::describe_gauge!(
        "blackwire_hysteria2_pacer_rate_bps",
        metrics::Unit::Bytes,
        "Hysteria2 application write pacer rate in bytes per second by lane"
    );
    metrics::describe_counter!(
        "blackwire_hysteria2_pacer_limited_writes_total",
        metrics::Unit::Count,
        "Hysteria2 writes shortened by application pacing by lane"
    );
    metrics::describe_counter!(
        "blackwire_quic_endpoint_active_total",
        metrics::Unit::Count,
        "QUIC endpoints successfully opened"
    );
    metrics::describe_counter!(
        "blackwire_quic_endpoint_packets_total",
        metrics::Unit::Count,
        "QUIC endpoint packets by endpoint and direction"
    );
    metrics::describe_counter!(
        "blackwire_quic_endpoint_bytes_total",
        metrics::Unit::Bytes,
        "QUIC endpoint bytes by endpoint and direction"
    );
    metrics::describe_counter!(
        "blackwire_quic_socket_drops_total",
        metrics::Unit::Count,
        "QUIC UDP socket drops reported by platform counters"
    );
    metrics::describe_gauge!(
        "blackwire_quic_recv_buffer_bytes",
        metrics::Unit::Bytes,
        "Actual QUIC UDP receive socket buffer size"
    );
    metrics::describe_gauge!(
        "blackwire_quic_send_buffer_bytes",
        metrics::Unit::Bytes,
        "Actual QUIC UDP send socket buffer size"
    );
    metrics::describe_counter!(
        "blackwire_quic_loss_fingerprint_total",
        metrics::Unit::Count,
        "Classified QUIC path loss fingerprints"
    );
    metrics::describe_counter!(
        "blackwire_fec_mode_total",
        metrics::Unit::Count,
        "Selected FEC modes for protected datagram groups"
    );
    metrics::describe_counter!(
        "blackwire_fec_recovered_packets_total",
        metrics::Unit::Count,
        "Datagram packets recovered by FEC"
    );
    metrics::describe_counter!(
        "blackwire_fec_overhead_bytes_total",
        metrics::Unit::Bytes,
        "FEC parity overhead bytes sent"
    );
    metrics::describe_counter!(
        "blackwire_fec_stale_drops_total",
        metrics::Unit::Count,
        "Stale FEC decode groups dropped before recovery"
    );
    metrics::describe_counter!(
        "blackwire_fec_duplicate_safe_skip_total",
        metrics::Unit::Count,
        "FEC protection skipped by duplicate-safe policy"
    );
    metrics::describe_counter!(
        "blackwire_datagram_packets_total",
        metrics::Unit::Count,
        "QUIC DATAGRAM packets by traffic class and direction"
    );
    metrics::describe_counter!(
        "blackwire_datagram_fallback_total",
        metrics::Unit::Count,
        "QUIC DATAGRAM fallback events by reason"
    );
    metrics::describe_histogram!(
        "blackwire_innerflow_queue_delay_ms",
        metrics::Unit::Milliseconds,
        "InnerFlow scheduler queue delay by packet class"
    );
    metrics::describe_counter!(
        "blackwire_innerflow_drops_total",
        metrics::Unit::Count,
        "InnerFlow scheduler drops by packet class and reason"
    );
    metrics::describe_counter!(
        "blackwire_innerflow_dequeued_total",
        metrics::Unit::Count,
        "InnerFlow scheduler enqueue/dequeue events by packet class"
    );
    metrics::describe_counter!(
        "blackwire_innerflow_bulk_starvation_prevented_total",
        metrics::Unit::Count,
        "InnerFlow bulk dequeue events that yielded fairness accounting"
    );
    metrics::describe_counter!(
        "blackwire_pool_acquire_total",
        metrics::Unit::Count,
        "Shared buffer pool acquisitions by size class"
    );
    metrics::describe_counter!(
        "blackwire_pool_release_total",
        metrics::Unit::Count,
        "Shared buffer pool releases by size class"
    );
    metrics::describe_counter!(
        "blackwire_pool_miss_total",
        metrics::Unit::Count,
        "Shared buffer pool misses that allocated a fresh buffer"
    );
    metrics::describe_gauge!(
        "blackwire_pool_bytes_active",
        metrics::Unit::Bytes,
        "Bytes currently checked out from shared buffer pools"
    );
    blackwire_connmgr::metrics::describe_metrics();
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn readyz(State(state): State<MetricsState>) -> impl IntoResponse {
    if state.ready {
        (axum::http::StatusCode::OK, "ready")
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}

async fn metrics_handler(State(state): State<MetricsState>) -> impl IntoResponse {
    let body = state.prometheus_handle.render();
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        body,
    )
}

async fn version_handler() -> impl IntoResponse {
    Json(serde_json::json!({"version": "0.1.0"}))
}

// ── Metrics helpers ───────────────────────────────────────────────────────────

/// Record that a new connection was accepted on `inbound` using `protocol`.
pub fn record_connection_accepted(inbound: &str, protocol: &str) {
    runtime_stats::record_connection_accepted(inbound, protocol);
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "proxy_connections_total",
        "inbound" => inbound.to_owned(),
        "protocol" => protocol.to_owned()
    )
    .increment(1);

    metrics::gauge!(
        "proxy_active_connections",
        "inbound" => inbound.to_owned()
    )
    .increment(1.0);
}

/// Record how long the inbound protocol header parse took.
pub fn record_inbound_parse(inbound: &str, elapsed: std::time::Duration) {
    if !metrics_enabled() {
        return;
    }
    metrics::histogram!(
        "proxy_inbound_parse_seconds",
        "inbound" => inbound.to_owned()
    )
    .record(elapsed.as_secs_f64());
}

/// Record how long the routing decision took.
pub fn record_route(inbound: &str, elapsed: std::time::Duration) {
    if !metrics_enabled() {
        return;
    }
    metrics::histogram!(
        "proxy_route_seconds",
        "inbound" => inbound.to_owned()
    )
    .record(elapsed.as_secs_f64());
}

/// Record how long a DNS resolution took during routing.
pub fn record_dns(inbound: &str, elapsed: std::time::Duration) {
    if !metrics_enabled() {
        return;
    }
    metrics::histogram!(
        "proxy_dns_seconds",
        "inbound" => inbound.to_owned()
    )
    .record(elapsed.as_secs_f64());
}

/// Record how long the outbound connect (dial + handshake) took.
pub fn record_outbound_connect(inbound: &str, outbound: &str, elapsed: std::time::Duration) {
    if !metrics_enabled() {
        return;
    }
    metrics::histogram!(
        "proxy_outbound_connect_seconds",
        "inbound" => inbound.to_owned(),
        "outbound" => outbound.to_owned()
    )
    .record(elapsed.as_secs_f64());
}

/// Record bytes captured by an inbound parser after its handshake.
pub fn record_early_payload(protocol: &str, bytes: u64) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "blackwire_early_payload_bytes_total",
        "protocol" => protocol.to_owned()
    )
    .increment(bytes);
}

/// Record an outbound writing already-buffered first payload bytes.
pub fn record_early_payload_written(protocol: &str, outbound: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "blackwire_early_payload_written_total",
        "protocol" => protocol.to_owned(),
        "outbound" => outbound.to_owned()
    )
    .increment(1);
}

/// Record a handshake kick event.
pub fn record_handshake_kick(protocol: &str, direction: &str, result: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "blackwire_handshake_kick_total",
        "protocol" => protocol.to_owned(),
        "direction" => direction.to_owned(),
        "result" => result.to_owned()
    )
    .increment(1);
}

/// Record latency until the first upstream byte can be written.
pub fn record_first_byte_latency(protocol: &str, transport: &str, elapsed: Duration) {
    if !metrics_enabled() {
        return;
    }
    metrics::histogram!(
        "blackwire_first_byte_latency_seconds",
        "protocol" => protocol.to_owned(),
        "transport" => transport.to_owned()
    )
    .record(elapsed.as_secs_f64());
    metrics::histogram!(
        "blackwire_ttfb_seconds",
        "protocol" => protocol.to_owned(),
        "transport" => transport.to_owned()
    )
    .record(elapsed.as_secs_f64());
}

/// Record a first-packet boost decision.
pub fn record_first_packet_boost(kind: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "blackwire_first_packet_boost_total",
        "kind" => kind.to_owned()
    )
    .increment(1);
}

/// Record a compiled connection plan selection.
pub fn record_connection_plan_selected(plan: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "blackwire_connection_plan_selected_total",
        "plan" => plan.to_owned()
    )
    .increment(1);
}

/// Record compiled router match latency.
pub fn record_route_match(elapsed: Duration) {
    if !metrics_enabled() {
        return;
    }
    metrics::histogram!("blackwire_route_match_seconds").record(elapsed.as_secs_f64());
}

/// Increment a compiled route cache hit.
pub fn record_route_cache_hit() {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!("blackwire_route_cache_hits_total").increment(1);
}

/// Increment a compiled route cache miss.
pub fn record_route_cache_miss() {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!("blackwire_route_cache_misses_total").increment(1);
}

/// Publish the current compiled routing rule count for a rule kind.
pub fn record_route_compiled_rules(kind: &str, count: usize) {
    if !metrics_enabled() {
        return;
    }
    metrics::gauge!(
        "blackwire_route_compiled_rules_total",
        "kind" => kind.to_owned()
    )
    .set(count as f64);
}

/// Record a background routing DNS prefetch outcome.
pub fn record_dns_prefetch(result: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "blackwire_dns_prefetch_total",
        "result" => result.to_owned()
    )
    .increment(1);
}

/// Increment the relay error counter for an inbound.
pub fn record_relay_error(inbound: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "proxy_relay_errors_total",
        "inbound" => inbound.to_owned()
    )
    .increment(1);
}

/// Record that a connection on `inbound` has closed.
///
/// Call this after the relay finishes to decrement the active gauge and
/// record bytes / duration.
pub fn record_connection_closed(inbound: &str, rx_bytes: u64, tx_bytes: u64, duration: Duration) {
    runtime_stats::record_relay_traffic(inbound, None, rx_bytes, tx_bytes);
    if !metrics_enabled() {
        return;
    }
    metrics::gauge!(
        "proxy_active_connections",
        "inbound" => inbound.to_owned()
    )
    .decrement(1.0);

    metrics::counter!(
        "proxy_bytes_total",
        "direction" => "rx",
        "inbound" => inbound.to_owned()
    )
    .increment(rx_bytes);

    metrics::counter!(
        "proxy_bytes_total",
        "direction" => "tx",
        "inbound" => inbound.to_owned()
    )
    .increment(tx_bytes);

    metrics::histogram!(
        "proxy_connection_duration_seconds",
        "inbound" => inbound.to_owned()
    )
    .record(duration.as_secs_f64());
}

/// Record the current adaptive score for one balancer profile.
pub fn record_adaptive_balancer_score(balancer: &str, profile: &str, score: f64) {
    if !metrics_enabled() {
        return;
    }
    metrics::gauge!(
        "balancer_adaptive_profile_score",
        "balancer" => balancer.to_owned(),
        "profile" => profile.to_owned()
    )
    .set(score);
}

/// Record an adaptive profile selection.
pub fn record_adaptive_balancer_selection(balancer: &str, profile: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "balancer_adaptive_selections_total",
        "balancer" => balancer.to_owned(),
        "profile" => profile.to_owned()
    )
    .increment(1);
    metrics::gauge!(
        "balancer_adaptive_profile_selected",
        "balancer" => balancer.to_owned(),
        "profile" => profile.to_owned()
    )
    .set(1.0);
}

/// Record that an adaptive profile entered cooldown.
pub fn record_adaptive_balancer_cooldown(balancer: &str, profile: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "balancer_adaptive_cooldowns_total",
        "balancer" => balancer.to_owned(),
        "profile" => profile.to_owned()
    )
    .increment(1);
}

/// Record an adaptive outbound connect success.
pub fn record_adaptive_balancer_connect_success(balancer: &str, profile: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "balancer_adaptive_connect_success_total",
        "balancer" => balancer.to_owned(),
        "profile" => profile.to_owned()
    )
    .increment(1);
}

/// Record an adaptive outbound connect failure.
pub fn record_adaptive_balancer_connect_failure(balancer: &str, profile: &str) {
    if !metrics_enabled() {
        return;
    }
    metrics::counter!(
        "balancer_adaptive_connect_failures_total",
        "balancer" => balancer.to_owned(),
        "profile" => profile.to_owned()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `record_connection_accepted` and `record_connection_closed` should not panic.
    #[test]
    fn metrics_helpers_do_not_panic() {
        // Without a recorder installed, these are no-ops.
        record_connection_accepted("test-inbound", "ss2022");
        record_connection_closed("test-inbound", 1024, 2048, Duration::from_secs(1));
    }
}
