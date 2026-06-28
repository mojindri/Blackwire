//! HTTP/3 front door for Hysteria2 — authentication then raw QUIC TCP streams and UDP datagrams.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _, Result};
use blackwire_app::context::Context;
use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::runtime_stats;
use blackwire_app::{wait_for_user_write_budget, UserBandwidthDirection};
use blackwire_common::{BoxedStream, ReunionStream};
use h3_quinn::Connection as H3QuinnConnection;
use http::{Response, StatusCode};
use quinn::{Connection, ConnectionError};
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore, TryAcquireError};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::innerflow::{record_queue_delay, InnerFlowPacket, InnerFlowScheduler};

use super::auth::AuthError;
use super::proto::{auth_response_to_headers, is_auth_request, AuthResponse, STATUS_AUTH_OK};
use super::tcp;
use super::udp::{
    encode_udp_datagram, record_datagram_packet, DatagramLane, Destination, FecDecoder, FecEncoder,
    UdpDatagram,
};
use super::{server_download_pacer, Hysteria2ServerConfig, PacedStream};

const H3_AUTH_ACCEPT_TIMEOUT: Duration = Duration::from_secs(5);
const H3_AUTH_HANDLE_TIMEOUT: Duration = Duration::from_secs(5);
const TCP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
// Match the reference Hysteria2/quic-go default for incoming streams. This is
// a guardrail only; normal flow control is handled by QUIC itself.
const MAX_TCP_STREAMS_PER_CONN: usize = 1024;
const MAX_UDP_WORKERS_PER_CONN: usize = 256;
/// Bound on the scheduled-datagram channel; backpressure instead of unbounded growth.
const SCHEDULED_UDP_CHANNEL_CAP: usize = 1024;

static NEXT_HYSTERIA2_CONN_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_HYSTERIA2_STREAM_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
struct Hysteria2ConnectionStats {
    tcp_streams_opened: AtomicU64,
    tcp_streams_completed: AtomicU64,
    tcp_streams_canceled: AtomicU64,
    tcp_streams_bad_request: AtomicU64,
    tcp_streams_request_timeout: AtomicU64,
    tcp_streams_response_write_failed: AtomicU64,
    tcp_streams_dispatch_error: AtomicU64,
    tcp_streams_active: AtomicU64,
    tcp_streams_active_peak: AtomicU64,
    tcp_streams_backpressure_waits: AtomicU64,
    udp_datagrams_rx: AtomicU64,
    udp_datagrams_decoded_empty: AtomicU64,
    udp_datagrams_worker_drops: AtomicU64,
    udp_datagrams_tx: AtomicU64,
    udp_datagrams_schedule_closed: AtomicU64,
    udp_datagrams_send_failed: AtomicU64,
}

impl Hysteria2ConnectionStats {
    fn inc_tcp_opened(&self) {
        self.tcp_streams_opened.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_completed(&self) {
        self.tcp_streams_completed.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_canceled(&self) {
        self.tcp_streams_canceled.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_bad_request(&self) {
        self.tcp_streams_bad_request.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_request_timeout(&self) {
        self.tcp_streams_request_timeout
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_response_write_failed(&self) {
        self.tcp_streams_response_write_failed
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_dispatch_error(&self) {
        self.tcp_streams_dispatch_error
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_tcp_active(&self) {
        let active = self.tcp_streams_active.fetch_add(1, Ordering::Relaxed) + 1;
        let mut peak = self.tcp_streams_active_peak.load(Ordering::Relaxed);
        while active > peak {
            match self.tcp_streams_active_peak.compare_exchange_weak(
                peak,
                active,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(current) => peak = current,
            }
        }
    }

    fn dec_tcp_active(&self) {
        self.tcp_streams_active.fetch_sub(1, Ordering::Relaxed);
    }

    fn inc_tcp_backpressure_wait(&self) {
        self.tcp_streams_backpressure_waits
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_udp_rx(&self) {
        self.udp_datagrams_rx.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_udp_decoded_empty(&self) {
        self.udp_datagrams_decoded_empty
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_udp_worker_drop(&self) {
        self.udp_datagrams_worker_drops
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_udp_tx(&self) {
        self.udp_datagrams_tx.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_udp_schedule_closed(&self) {
        self.udp_datagrams_schedule_closed
            .fetch_add(1, Ordering::Relaxed);
    }

    fn inc_udp_send_failed(&self) {
        self.udp_datagrams_send_failed
            .fetch_add(1, Ordering::Relaxed);
    }
}

struct Hysteria2ConnectionSummary {
    conn_id: u64,
    inbound_tag: String,
    peer: SocketAddr,
    user: Option<String>,
    started_at: Instant,
    stats: Arc<Hysteria2ConnectionStats>,
}

impl Drop for Hysteria2ConnectionSummary {
    fn drop(&mut self) {
        let elapsed_ms = self.started_at.elapsed().as_millis();
        debug!(
            conn_id = self.conn_id,
            tag = %self.inbound_tag,
            peer = %self.peer,
            user = self.user.as_deref().unwrap_or("<none>"),
            elapsed_ms,
            tcp_opened = self.stats.tcp_streams_opened.load(Ordering::Relaxed),
            tcp_completed = self.stats.tcp_streams_completed.load(Ordering::Relaxed),
            tcp_canceled = self.stats.tcp_streams_canceled.load(Ordering::Relaxed),
            tcp_bad_request = self.stats.tcp_streams_bad_request.load(Ordering::Relaxed),
            tcp_request_timeout = self.stats.tcp_streams_request_timeout.load(Ordering::Relaxed),
            tcp_response_write_failed = self.stats.tcp_streams_response_write_failed.load(Ordering::Relaxed),
            tcp_dispatch_error = self.stats.tcp_streams_dispatch_error.load(Ordering::Relaxed),
            tcp_active = self.stats.tcp_streams_active.load(Ordering::Relaxed),
            tcp_active_peak = self.stats.tcp_streams_active_peak.load(Ordering::Relaxed),
            tcp_backpressure_waits = self.stats.tcp_streams_backpressure_waits.load(Ordering::Relaxed),
            udp_rx = self.stats.udp_datagrams_rx.load(Ordering::Relaxed),
            udp_decoded_empty = self.stats.udp_datagrams_decoded_empty.load(Ordering::Relaxed),
            udp_worker_drops = self.stats.udp_datagrams_worker_drops.load(Ordering::Relaxed),
            udp_tx = self.stats.udp_datagrams_tx.load(Ordering::Relaxed),
            udp_schedule_closed = self.stats.udp_datagrams_schedule_closed.load(Ordering::Relaxed),
            udp_send_failed = self.stats.udp_datagrams_send_failed.load(Ordering::Relaxed),
            "Hysteria2 connection summary"
        );
    }
}

struct ActiveTcpStreamGuard {
    stats: Arc<Hysteria2ConnectionStats>,
}

impl Drop for ActiveTcpStreamGuard {
    fn drop(&mut self) {
        self.stats.dec_tcp_active();
    }
}

#[derive(Clone)]
struct Hysteria2Diagnostics {
    conn_id: u64,
    stats: Arc<Hysteria2ConnectionStats>,
}

#[derive(Clone)]
struct Hysteria2UdpServeState {
    inbound_tag: String,
    user: Option<String>,
    dispatcher: Arc<dyn Dispatcher>,
    diagnostics: Hysteria2Diagnostics,
}

struct ScheduledUdpDatagram {
    packet: InnerFlowPacket,
}

fn destination_to_address(dest: &Destination) -> blackwire_common::Address {
    match dest {
        Destination::V4(ip, port) => blackwire_common::Address::Ipv4(*ip, *port),
        Destination::V6(ip, port) => blackwire_common::Address::Ipv6(*ip, *port),
        Destination::Domain(name, port) => blackwire_common::Address::Domain(name.clone(), *port),
    }
}

fn address_to_destination(addr: blackwire_common::Address) -> Destination {
    match addr {
        blackwire_common::Address::Ipv4(ip, port) => Destination::V4(ip, port),
        blackwire_common::Address::Ipv6(ip, port) => Destination::V6(ip, port),
        blackwire_common::Address::Domain(name, port) => Destination::Domain(name, port),
    }
}

/// Serve one QUIC connection: HTTP/3 auth, then TCP proxy streams on QUIC bidi streams.
pub async fn serve_connection(
    conn: Connection,
    config: Hysteria2ServerConfig,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<()> {
    let conn_id = NEXT_HYSTERIA2_CONN_ID.fetch_add(1, Ordering::Relaxed);
    let peer = conn.remote_address();
    let conn_stats = Arc::new(Hysteria2ConnectionStats::default());
    record_hysteria2_connection(&config.tag, "accepted");
    debug!(
        conn_id,
        tag = %config.tag,
        peer = %peer,
        "Hysteria2 QUIC connection accepted"
    );

    let server_rx_bps = config.up_mbps.saturating_mul(1_000_000 / 8);
    let auth = config
        .auth
        .load()
        .ok_or_else(|| anyhow::anyhow!("Hysteria2 auth store is empty"))?;

    let mut h3_conn = h3::server::Connection::new(H3QuinnConnection::new(conn.clone()))
        .await
        .context("start HTTP/3 server")?;

    let resolver = match timeout(H3_AUTH_ACCEPT_TIMEOUT, h3_conn.accept())
        .await
        .context("accept HTTP/3 auth timed out")??
    {
        Some(resolver) => resolver,
        None => bail!("connection closed before Hysteria2 auth"),
    };

    let authenticated = timeout(
        H3_AUTH_HANDLE_TIMEOUT,
        handle_h3_auth_request(resolver, &auth.password, server_rx_bps, true),
    )
    .await
    .context("handle HTTP/3 auth timed out")??;
    if !authenticated {
        record_hysteria2_connection(&config.tag, "auth_rejected");
        debug!(conn_id, tag = %config.tag, peer = %peer, "Hysteria2 auth rejected");
        bail!("Hysteria2 HTTP/3 authentication rejected");
    }
    // Keep the HTTP/3 server state alive for the QUIC session without calling
    // `accept()` again. Official hysteria uses http3.StreamDispatcher to hijack
    // proxy streams (varint 0x401); the Rust `h3` crate has no equivalent, so we
    // take proxy streams via `conn.accept_bi()` below. A competing `h3_conn.accept()`
    // would treat 0x401 TCPRequest bytes as HTTP/3 and reset the connection.
    //
    // Tie this guard task to QUIC connection closure so `h3_conn` is dropped and
    // the task exits when the client disconnects.
    let h3_guard_conn = conn.clone();
    let close_log_conn = conn.clone();
    let close_log_tag = config.tag.clone();
    tokio::spawn(async move {
        let reason = h3_guard_conn.closed().await;
        log_quic_close(conn_id, &close_log_tag, peer, "h3_guard_closed", &reason);
        drop(h3_conn);
    });

    let inbound_tag = config.tag.clone();
    let user = auth.user.clone();
    let _conn_summary = Hysteria2ConnectionSummary {
        conn_id,
        inbound_tag: inbound_tag.clone(),
        peer,
        user: user.clone(),
        started_at: Instant::now(),
        stats: Arc::clone(&conn_stats),
    };
    record_hysteria2_connection(&inbound_tag, "authenticated");
    debug!(
        conn_id,
        tag = %inbound_tag,
        peer = %peer,
        user = user.as_deref().unwrap_or("<none>"),
        udp_enabled = config.datagram_enabled,
        "Hysteria2 auth accepted"
    );
    let _user_permit = if let (Some(limiter), Some(user)) = (&config.user_limiter, &user) {
        let user: Arc<str> = user.clone().into();
        match limiter.try_acquire(Some(&user)) {
            Some(permit) => Some(permit),
            None => {
                record_hysteria2_connection(&inbound_tag, "user_limit");
                debug!(
                    conn_id,
                    tag = %inbound_tag,
                    peer = %peer,
                    user = %user,
                    max = limiter.max_connections_per_user(),
                    "Hysteria2 per-user connection limit reached"
                );
                bail!(
                    "per-user Hysteria2 connection limit reached for '{user}' (max {})",
                    limiter.max_connections_per_user()
                );
            }
        }
    } else {
        None
    };

    // Spawn the UDP datagram relay concurrently with the TCP stream accept loop.
    let udp_conn = conn.clone();
    let datagram_enabled = config.datagram_enabled;
    let datagram_policy = config.datagram_policy;
    let fec = config.fec;
    let udp_state = Hysteria2UdpServeState {
        inbound_tag: inbound_tag.clone(),
        user: user.clone(),
        dispatcher: Arc::clone(&dispatcher),
        diagnostics: Hysteria2Diagnostics {
            conn_id,
            stats: Arc::clone(&conn_stats),
        },
    };
    tokio::spawn(async move {
        serve_udp_sessions(udp_conn, udp_state, datagram_enabled, fec, datagram_policy).await;
    });

    let stream_shutdown = CancellationToken::new();
    let tcp_stream_limiter = Arc::new(Semaphore::new(MAX_TCP_STREAMS_PER_CONN));
    loop {
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(stream) => stream,
            Err(e) => {
                stream_shutdown.cancel();
                record_hysteria2_connection(&inbound_tag, "tcp_accept_closed");
                let close_reason = close_log_conn.close_reason();
                if let Some(reason) = close_reason.as_ref() {
                    log_quic_close(conn_id, &inbound_tag, peer, "tcp_accept_closed", reason);
                }
                debug!(
                    conn_id,
                    tag = %inbound_tag,
                    peer = %peer,
                    error = %e,
                    close_kind = close_reason.as_ref().map(quic_close_kind).unwrap_or("unknown"),
                    "Hysteria2 TCP accept loop ended"
                );
                return Err(e).context("accept Hysteria2 TCP stream");
            }
        };

        let active_permit = match acquire_tcp_stream_permit(
            &tcp_stream_limiter,
            &conn,
            &conn_stats,
            conn_id,
            &inbound_tag,
        )
        .await
        {
            Ok(permit) => permit,
            Err(e) => {
                stream_shutdown.cancel();
                return Err(e).context("acquire Hysteria2 TCP stream permit");
            }
        };
        let stream_id = NEXT_HYSTERIA2_STREAM_ID.fetch_add(1, Ordering::Relaxed);
        conn_stats.inc_tcp_opened();
        conn_stats.inc_tcp_active();
        record_hysteria2_tcp_stream(&inbound_tag, "opened");
        let dispatcher = Arc::clone(&dispatcher);
        let tag = inbound_tag.clone();
        let user = user.clone();
        let congestion = config.congestion.clone();
        let stream_shutdown = stream_shutdown.child_token();
        let stats = Arc::clone(&conn_stats);
        let cancel_tag = tag.clone();
        let cancel_stats = Arc::clone(&stats);
        tokio::spawn(async move {
            let _active_permit = active_permit;
            let _active_guard = ActiveTcpStreamGuard {
                stats: Arc::clone(&stats),
            };
            tokio::select! {
                _ = stream_shutdown.cancelled() => {
                    cancel_stats.inc_tcp_canceled();
                    record_hysteria2_tcp_stream(&cancel_tag, "canceled");
                    debug!(
                        conn_id,
                        stream_id,
                        tag = %cancel_tag,
                        "Hysteria2 TCP stream canceled after QUIC close"
                    );
                },
                _ = async move {
                    let dest = match timeout(TCP_REQUEST_TIMEOUT, tcp::server_read_request(&mut recv)).await
                    {
                        Ok(Ok(d)) => d,
                        Ok(Err(e)) => {
                            stats.inc_tcp_bad_request();
                            record_hysteria2_tcp_stream(&tag, "bad_request");
                            warn!("Hysteria2 bad TCP request: {e}");
                            let _ = tcp::server_write_response(&mut send, false, &e.to_string()).await;
                            return;
                        }
                        Err(_) => {
                            stats.inc_tcp_request_timeout();
                            record_hysteria2_tcp_stream(&tag, "request_timeout");
                            debug!("Hysteria2 TCP request read timed out");
                            let _ = tcp::server_write_response(&mut send, false, "request timeout").await;
                            return;
                        }
                    };
                    debug!(
                        conn_id,
                        stream_id,
                        tag = %tag,
                        dest = ?dest,
                        "Hysteria2 TCP stream request accepted"
                    );

                    if let Err(e) = tcp::server_write_response(&mut send, true, "").await {
                        stats.inc_tcp_response_write_failed();
                        record_hysteria2_tcp_stream(&tag, "response_write_failed");
                        debug!("Hysteria2 TCP response write failed: {e}");
                        return;
                    }

                    let stream = ReunionStream::new(recv, send);
                    let stream: BoxedStream =
                        Box::new(PacedStream::new(stream, server_download_pacer(&congestion)));
                    let ctx = Context {
                        sniffed_domain: None,
                        source: None,
                        inbound_tag: tag.clone().into(),
                        user: user.map(Into::into),
                        sniffed_protocol: None,
                        vision_flow: false,
                    };

                    if let Err(e) = dispatcher.dispatch(ctx, dest, stream).await {
                        stats.inc_tcp_dispatch_error();
                        record_hysteria2_tcp_stream(&tag, "dispatch_error");
                        warn!("Hysteria2 dispatch error: {e}");
                    } else {
                        stats.inc_tcp_completed();
                        record_hysteria2_tcp_stream(&tag, "completed");
                        debug!(
                            conn_id,
                            stream_id,
                            tag = %tag,
                            "Hysteria2 TCP stream completed"
                        );
                    }
                } => {},
            }
        });
    }
}

async fn acquire_tcp_stream_permit(
    limiter: &Arc<Semaphore>,
    conn: &Connection,
    stats: &Arc<Hysteria2ConnectionStats>,
    conn_id: u64,
    inbound_tag: &str,
) -> Result<OwnedSemaphorePermit> {
    match Arc::clone(limiter).try_acquire_owned() {
        Ok(permit) => Ok(permit),
        Err(TryAcquireError::NoPermits) => {
            stats.inc_tcp_backpressure_wait();
            record_hysteria2_tcp_stream(inbound_tag, "backpressure_wait");
            debug!(
                conn_id,
                tag = %inbound_tag,
                max_active_streams = MAX_TCP_STREAMS_PER_CONN,
                "Hysteria2 TCP stream backpressure engaged"
            );
            tokio::select! {
                permit = Arc::clone(limiter).acquire_owned() => {
                    permit.map_err(|_| anyhow::anyhow!("Hysteria2 TCP stream limiter closed"))
                }
                _ = conn.closed() => {
                    anyhow::bail!("Hysteria2 QUIC connection closed while waiting for stream capacity");
                }
            }
        }
        Err(TryAcquireError::Closed) => anyhow::bail!("Hysteria2 TCP stream limiter closed"),
    }
}

fn quic_close_kind(reason: &ConnectionError) -> &'static str {
    match reason {
        ConnectionError::VersionMismatch => "version_mismatch",
        ConnectionError::TransportError(_) => "transport_error",
        ConnectionError::ConnectionClosed(_) => "peer_transport_close",
        ConnectionError::ApplicationClosed(_) => "peer_application_close",
        ConnectionError::Reset => "peer_reset",
        ConnectionError::TimedOut => "idle_timeout",
        ConnectionError::LocallyClosed => "locally_closed",
        ConnectionError::CidsExhausted => "cids_exhausted",
    }
}

fn log_quic_close(
    conn_id: u64,
    inbound_tag: &str,
    peer: SocketAddr,
    observed_at: &'static str,
    reason: &ConnectionError,
) {
    match reason {
        ConnectionError::TransportError(error) => {
            debug!(
                conn_id,
                tag = %inbound_tag,
                peer = %peer,
                observed_at,
                close_kind = quic_close_kind(reason),
                error_code = u64::from(error.code),
                error_code_debug = ?error.code,
                frame_type = ?error.frame,
                close_reason = %error.reason,
                error = %reason,
                "Hysteria2 QUIC connection closed"
            );
        }
        ConnectionError::ConnectionClosed(close) => {
            debug!(
                conn_id,
                tag = %inbound_tag,
                peer = %peer,
                observed_at,
                close_kind = quic_close_kind(reason),
                error_code = u64::from(close.error_code),
                error_code_debug = ?close.error_code,
                frame_type = ?close.frame_type,
                close_reason = %String::from_utf8_lossy(&close.reason),
                error = %reason,
                "Hysteria2 QUIC connection closed"
            );
        }
        ConnectionError::ApplicationClosed(close) => {
            debug!(
                conn_id,
                tag = %inbound_tag,
                peer = %peer,
                observed_at,
                close_kind = quic_close_kind(reason),
                error_code = close.error_code.into_inner(),
                close_reason = %String::from_utf8_lossy(&close.reason),
                error = %reason,
                "Hysteria2 QUIC connection closed"
            );
        }
        _ => {
            debug!(
                conn_id,
                tag = %inbound_tag,
                peer = %peer,
                observed_at,
                close_kind = quic_close_kind(reason),
                error = %reason,
                "Hysteria2 QUIC connection closed"
            );
        }
    }
}

/// Relay UDP datagrams for one QUIC connection.
///
/// Loops on `conn.read_datagram()`. Each datagram is decoded, and the
/// payload is forwarded to the destination via a per-session UDP socket.
/// Responses are encoded and sent back as QUIC datagrams.
async fn serve_udp_sessions(
    conn: Connection,
    state: Hysteria2UdpServeState,
    datagram_enabled: bool,
    fec: super::udp::FecPolicy,
    datagram_policy: super::udp::DatagramPolicy,
) {
    if !datagram_enabled {
        super::udp::record_datagram_fallback("disabled");
        record_hysteria2_udp_event(&state.inbound_tag, "disabled");
        debug!(
            conn_id = state.diagnostics.conn_id,
            tag = %state.inbound_tag,
            "Hysteria2 UDP datagrams disabled for connection"
        );
        return;
    }
    let worker_limiter = Arc::new(Semaphore::new(MAX_UDP_WORKERS_PER_CONN));
    let mut fec_decoder = FecDecoder::new(fec);
    let fec_encoder = Arc::new(std::sync::Mutex::new(FecEncoder::new(fec)));
    let (scheduled_tx, scheduled_rx) = mpsc::channel(SCHEDULED_UDP_CHANNEL_CAP);
    tokio::spawn(send_scheduled_udp_datagrams(
        conn.clone(),
        state.inbound_tag.clone(),
        state.diagnostics.clone(),
        scheduled_rx,
    ));

    loop {
        let raw: bytes::Bytes = match conn.read_datagram().await {
            Ok(b) => b,
            Err(e) => {
                record_hysteria2_udp_event(&state.inbound_tag, "read_closed");
                debug!(
                    conn_id = state.diagnostics.conn_id,
                    tag = %state.inbound_tag,
                    error = %e,
                    "Hysteria2 UDP datagram read loop ended"
                );
                break;
            }
        };

        state.diagnostics.stats.inc_udp_rx();
        let datagrams = fec_decoder.decode(raw);
        if datagrams.is_empty() {
            state.diagnostics.stats.inc_udp_decoded_empty();
            record_hysteria2_udp_event(&state.inbound_tag, "decoded_empty");
            continue;
        }
        for dg in datagrams {
            let lane = datagram_policy.lane_for(&dg.dest, dg.data.len());
            record_datagram_packet(lane.class(), "rx");
            handle_udp_datagram(
                Arc::clone(&worker_limiter),
                Arc::clone(&fec_encoder),
                scheduled_tx.clone(),
                dg,
                datagram_policy,
                state.clone(),
            )
            .await;
        }
    }
}

async fn handle_udp_datagram(
    worker_limiter: Arc<Semaphore>,
    fec_encoder: Arc<std::sync::Mutex<FecEncoder>>,
    scheduled_tx: mpsc::Sender<ScheduledUdpDatagram>,
    dg: UdpDatagram,
    datagram_policy: super::udp::DatagramPolicy,
    state: Hysteria2UdpServeState,
) {
    let session_id = dg.session_id;
    let packet_id = dg.packet_id;
    let payload = dg.data;
    let dest = dg.dest;
    let tx_lane = datagram_policy.lane_for(&dest, payload.len());
    let permit = match Arc::clone(&worker_limiter).try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            state.diagnostics.stats.inc_udp_worker_drop();
            record_hysteria2_udp_event(&state.inbound_tag, "worker_limit_drop");
            warn!(
                tag = %state.inbound_tag,
                max = MAX_UDP_WORKERS_PER_CONN,
                "Hysteria2 UDP worker limit reached; dropping datagram"
            );
            return;
        }
    };

    tokio::spawn(async move {
        let _permit = permit;
        let address = destination_to_address(&dest);
        let user_arc = state.user.as_deref().map(Arc::<str>::from);
        let ctx = Context {
            sniffed_domain: None,
            source: None,
            inbound_tag: Arc::from(state.inbound_tag.clone()),
            user: user_arc.clone(),
            sniffed_protocol: None,
            vision_flow: false,
        };

        let upload_len = payload.len();
        wait_for_user_write_budget(
            user_arc.as_deref(),
            UserBandwidthDirection::Upload,
            upload_len,
        )
        .await;
        let response = match state
            .dispatcher
            .dispatch_udp_datagram(ctx, address, payload)
            .await
        {
            Ok(response) => response,
            Err(e) => {
                warn!(tag = %state.inbound_tag, dest = ?dest, error = %e, "Hysteria2 UDP dispatch failed");
                record_hysteria2_udp_event(&state.inbound_tag, "dispatch_error");
                return;
            }
        };
        runtime_stats::record_relay_traffic(
            &state.inbound_tag,
            user_arc.as_deref(),
            upload_len as u64,
            0,
        );

        let Some(response) = response else {
            return;
        };
        wait_for_user_write_budget(
            user_arc.as_deref(),
            UserBandwidthDirection::Download,
            response.data.len(),
        )
        .await;
        runtime_stats::record_relay_traffic(
            &state.inbound_tag,
            user_arc.as_deref(),
            0,
            response.data.len() as u64,
        );

        let response_dg = UdpDatagram {
            session_id,
            packet_id,
            frag_id: 0,
            frag_num: 1,
            dest: address_to_destination(response.source),
            data: response.data,
        };
        let encoded = encode_udp_datagram(&response_dg);
        let parity = fec_encoder.lock().ok().and_then(|mut encoder| {
            if matches!(tx_lane, DatagramLane::Priority) {
                encoder.protect(&response_dg, &encoded)
            } else {
                None
            }
        });
        record_datagram_packet(tx_lane.class(), "tx");
        state.diagnostics.stats.inc_udp_tx();
        let class = super::udp::packet_class_for(&dest, response_dg.data.len());
        let flow = super::udp::flow_key_for(&dest, session_id);
        let mut packet = InnerFlowPacket::new(class, flow, encoded);
        if let Some(parity) = parity {
            packet.followups.push(parity);
        }
        if scheduled_tx
            .send(ScheduledUdpDatagram { packet })
            .await
            .is_err()
        {
            state.diagnostics.stats.inc_udp_schedule_closed();
            record_hysteria2_udp_event(&state.inbound_tag, "schedule_closed");
            warn!(
                conn_id = state.diagnostics.conn_id,
                tag = %state.inbound_tag,
                "Hysteria2 UDP: scheduled datagram channel closed"
            );
        }
    });
}

async fn send_scheduled_udp_datagrams(
    conn: Connection,
    inbound_tag: String,
    diagnostics: Hysteria2Diagnostics,
    mut rx: mpsc::Receiver<ScheduledUdpDatagram>,
) {
    let mut scheduler = InnerFlowScheduler::default();
    while let Some(item) = rx.recv().await {
        scheduler.enqueue(item.packet);
        while let Ok(item) = rx.try_recv() {
            scheduler.enqueue(item.packet);
        }
        while let Some(packet) = scheduler.dequeue() {
            record_queue_delay(packet.class, packet.enqueued_at);
            let followups = packet.followups;
            if let Err(e) = conn.send_datagram(packet.payload) {
                diagnostics.stats.inc_udp_send_failed();
                record_hysteria2_udp_event(&inbound_tag, "send_failed");
                warn!(
                    conn_id = diagnostics.conn_id,
                    tag = %inbound_tag,
                    error = %e,
                    "Hysteria2 UDP: scheduled send_datagram failed"
                );
            }
            for followup in followups {
                if let Err(e) = conn.send_datagram(followup) {
                    diagnostics.stats.inc_udp_send_failed();
                    record_hysteria2_udp_event(&inbound_tag, "followup_send_failed");
                    warn!(
                        conn_id = diagnostics.conn_id,
                        tag = %inbound_tag,
                        error = %e,
                        "Hysteria2 UDP: scheduled follow-up datagram failed"
                    );
                }
            }
        }
    }
}

fn record_hysteria2_connection(inbound_tag: &str, result: &'static str) {
    metrics::counter!(
        "blackwire_hysteria2_connections_total",
        "inbound" => inbound_tag.to_owned(),
        "result" => result
    )
    .increment(1);
}

fn record_hysteria2_tcp_stream(inbound_tag: &str, result: &'static str) {
    metrics::counter!(
        "blackwire_hysteria2_tcp_streams_total",
        "inbound" => inbound_tag.to_owned(),
        "result" => result
    )
    .increment(1);
}

fn record_hysteria2_udp_event(inbound_tag: &str, event: &'static str) {
    metrics::counter!(
        "blackwire_hysteria2_udp_events_total",
        "inbound" => inbound_tag.to_owned(),
        "event" => event
    )
    .increment(1);
}

async fn handle_h3_auth_request(
    resolver: h3::server::RequestResolver<H3QuinnConnection, bytes::Bytes>,
    password: &str,
    server_rx_bps: u64,
    udp_enabled: bool,
) -> Result<bool> {
    let (req, mut stream) = resolver
        .resolve_request()
        .await
        .context("resolve HTTP/3 request")?;

    let method = req.method().as_str();
    let path = req.uri().path();
    let authority = req.uri().host().or_else(|| {
        req.headers()
            .get(http::header::HOST)
            .and_then(|v| v.to_str().ok())
    });

    if !is_auth_request(method, path, authority) {
        let resp = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(())
            .context("build 404 response")?;
        stream.send_response(resp).await.context("send 404")?;
        stream.finish().await.context("finish 404 stream")?;
        return Ok(false);
    }

    match super::auth::verify_auth_request(req.headers(), password) {
        Ok(_) => {
            let mut headers = http::HeaderMap::new();
            auth_response_to_headers(
                &mut headers,
                &AuthResponse {
                    ok: true,
                    udp_enabled,
                    rx_bps: server_rx_bps,
                    rx_auto: server_rx_bps == 0,
                },
            );
            let mut resp_builder = Response::builder().status(STATUS_AUTH_OK);
            for (name, value) in headers.iter() {
                resp_builder = resp_builder.header(name, value);
            }
            let resp = resp_builder.body(()).context("build 233 response")?;
            stream
                .send_response(resp)
                .await
                .context("send auth success")?;
            stream.finish().await.context("finish auth stream")?;
            Ok(true)
        }
        Err(AuthError::WrongPassword) => {
            let resp = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(())
                .context("build auth failure response")?;
            stream.send_response(resp).await.context("send auth 404")?;
            stream.finish().await.context("finish auth failure")?;
            Ok(false)
        }
        Err(AuthError::Protocol(msg)) => Err(anyhow::anyhow!("auth protocol error: {msg}")),
    }
}
