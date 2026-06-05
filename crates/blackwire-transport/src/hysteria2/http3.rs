//! HTTP/3 front door for Hysteria2 — authentication then raw QUIC TCP streams and UDP datagrams.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _, Result};
use blackwire_app::context::Context;
use blackwire_app::dispatcher::Dispatcher;
use blackwire_common::{BoxedStream, ReunionStream};
use dashmap::DashMap;
use h3_quinn::Connection as H3QuinnConnection;
use http::{Response, StatusCode};
use quinn::Connection;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::{sleep, timeout};
use tracing::warn;

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
const UDP_REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_UDP_WORKERS_PER_CONN: usize = 256;
/// Cap on per-connection UDP sessions (and thus bound upstream sockets / FDs).
/// `session_id` is client-controlled, so an unbounded map is an FD-exhaustion vector.
const MAX_UDP_SESSIONS_PER_CONN: usize = 512;
/// Idle lifetime for a per-session upstream socket before it is evicted.
const UDP_SESSION_IDLE: Duration = Duration::from_secs(60);
/// Bound on the scheduled-datagram channel; backpressure instead of unbounded growth.
const SCHEDULED_UDP_CHANNEL_CAP: usize = 1024;

struct ScheduledUdpDatagram {
    packet: InnerFlowPacket,
}

/// Per-session upstream socket plus a last-used timestamp for idle eviction.
struct UdpSession {
    sock: Arc<UdpSocket>,
    last_used: Instant,
}

/// Serve one QUIC connection: HTTP/3 auth, then TCP proxy streams on QUIC bidi streams.
pub async fn serve_connection(
    conn: Connection,
    config: Hysteria2ServerConfig,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<()> {
    let server_rx_bps = config.up_mbps.saturating_mul(1_000_000 / 8);

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

    timeout(
        H3_AUTH_HANDLE_TIMEOUT,
        handle_h3_auth_request(resolver, &config.password, server_rx_bps, true),
    )
    .await
    .context("handle HTTP/3 auth timed out")??;
    // Keep the HTTP/3 server driver alive for the QUIC session without calling
    // `accept()` again. Official hysteria uses http3.StreamDispatcher to hijack
    // proxy streams (varint 0x401); the Rust `h3` crate has no equivalent, so we
    // take proxy streams via `conn.accept_bi()` below. A competing `h3_conn.accept()`
    // would treat 0x401 TCPRequest bytes as HTTP/3 and reset the connection.
    tokio::spawn(async move {
        let _h3_conn = h3_conn;
        std::future::pending::<()>().await
    });

    let inbound_tag = config.tag.clone();

    // Spawn the UDP datagram relay concurrently with the TCP stream accept loop.
    let udp_conn = conn.clone();
    let udp_tag = inbound_tag.clone();
    let datagram_enabled = config.datagram_enabled;
    let datagram_policy = config.datagram_policy;
    let fec = config.fec;
    tokio::spawn(async move {
        serve_udp_sessions(udp_conn, udp_tag, datagram_enabled, fec, datagram_policy).await;
    });

    loop {
        let (mut send, mut recv) = conn
            .accept_bi()
            .await
            .context("accept Hysteria2 TCP stream")?;

        let dispatcher = Arc::clone(&dispatcher);
        let tag = inbound_tag.clone();
        let congestion = config.congestion.clone();
        tokio::spawn(async move {
            let dest = match timeout(TCP_REQUEST_TIMEOUT, tcp::server_read_request(&mut recv)).await
            {
                Ok(Ok(d)) => d,
                Ok(Err(e)) => {
                    warn!("Hysteria2 bad TCP request: {e}");
                    let _ = tcp::server_write_response(&mut send, false, &e.to_string()).await;
                    return;
                }
                Err(_) => {
                    warn!("Hysteria2 TCP request read timed out");
                    let _ = tcp::server_write_response(&mut send, false, "request timeout").await;
                    return;
                }
            };

            if let Err(e) = tcp::server_write_response(&mut send, true, "").await {
                warn!("Hysteria2 TCP response write failed: {e}");
                return;
            }

            let stream = ReunionStream::new(recv, send);
            let stream: BoxedStream =
                Box::new(PacedStream::new(stream, server_download_pacer(&congestion)));
            let ctx = Context {
                sniffed_domain: None,
                source: None,
                inbound_tag: tag.into(),
                user: None,
                sniffed_protocol: None,
                vision_flow: false,
            };

            if let Err(e) = dispatcher.dispatch(ctx, dest, stream).await {
                warn!("Hysteria2 dispatch error: {e}");
            }
        });
    }
}

/// Relay UDP datagrams for one QUIC connection.
///
/// Loops on `conn.read_datagram()`. Each datagram is decoded, and the
/// payload is forwarded to the destination via a per-session UDP socket.
/// Responses are encoded and sent back as QUIC datagrams.
async fn serve_udp_sessions(
    conn: Connection,
    inbound_tag: String,
    datagram_enabled: bool,
    fec: super::udp::FecPolicy,
    datagram_policy: super::udp::DatagramPolicy,
) {
    if !datagram_enabled {
        super::udp::record_datagram_fallback("disabled");
        return;
    }
    // session_id → per-session upstream socket bound on 0.0.0.0:0
    let sessions: Arc<DashMap<u32, UdpSession>> = Arc::new(DashMap::new());
    let worker_limiter = Arc::new(Semaphore::new(MAX_UDP_WORKERS_PER_CONN));
    let mut fec_decoder = FecDecoder::new(fec);
    let fec_encoder = Arc::new(std::sync::Mutex::new(FecEncoder::new(fec)));
    let (scheduled_tx, scheduled_rx) = mpsc::channel(SCHEDULED_UDP_CHANNEL_CAP);
    tokio::spawn(send_scheduled_udp_datagrams(conn.clone(), scheduled_rx));

    loop {
        let raw: bytes::Bytes = match conn.read_datagram().await {
            Ok(b) => b,
            Err(_) => break,
        };

        // Evict idle sessions when the table grows past its cap, dropping their
        // upstream sockets (and FDs). Bounds client-driven socket accumulation.
        if sessions.len() > MAX_UDP_SESSIONS_PER_CONN {
            let cutoff = Instant::now() - UDP_SESSION_IDLE;
            sessions.retain(|_, s| s.last_used > cutoff);
        }

        let datagrams = fec_decoder.decode(raw);
        if datagrams.is_empty() {
            continue;
        }
        for dg in datagrams {
            let lane = datagram_policy.lane_for(&dg.dest, dg.data.len());
            record_datagram_packet(lane.class(), "rx");
            handle_udp_datagram(
                conn.clone(),
                inbound_tag.clone(),
                Arc::clone(&sessions),
                Arc::clone(&worker_limiter),
                Arc::clone(&fec_encoder),
                scheduled_tx.clone(),
                dg,
                datagram_policy,
            )
            .await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_udp_datagram(
    _conn: Connection,
    inbound_tag: String,
    sessions: Arc<DashMap<u32, UdpSession>>,
    worker_limiter: Arc<Semaphore>,
    fec_encoder: Arc<std::sync::Mutex<FecEncoder>>,
    scheduled_tx: mpsc::Sender<ScheduledUdpDatagram>,
    dg: UdpDatagram,
    datagram_policy: super::udp::DatagramPolicy,
) {
    let dest_addr: SocketAddr = match &dg.dest {
        Destination::V4(ip, port) => SocketAddr::new((*ip).into(), *port),
        Destination::V6(ip, port) => SocketAddr::new((*ip).into(), *port),
        Destination::Domain(name, port) => {
            match tokio::net::lookup_host((name.as_str(), *port)).await {
                Ok(mut addrs) => match addrs.next() {
                    Some(a) => a,
                    None => {
                        warn!(tag = %inbound_tag, "Hysteria2 UDP: could not resolve '{name}'");
                        return;
                    }
                },
                Err(e) => {
                    warn!(tag = %inbound_tag, "Hysteria2 UDP DNS failed for '{name}': {e}");
                    return;
                }
            }
        }
    };

    let session_id = dg.session_id;
    let packet_id = dg.packet_id;
    let payload = dg.data;
    let dest = dg.dest;
    let tx_lane = datagram_policy.lane_for(&dest, payload.len());
    let use_isolated_priority_socket =
        matches!(tx_lane, DatagramLane::Priority) && datagram_policy.should_fast_retry_dns(&dest);

    // Fast DNS retry can leave duplicate upstream replies. Isolate only that
    // retry path; ordinary priority traffic keeps the per-session socket.
    let sock = if use_isolated_priority_socket {
        match UdpSocket::bind("0.0.0.0:0").await {
            Ok(new_sock) => Arc::new(new_sock),
            Err(e) => {
                warn!(tag = %inbound_tag, "Hysteria2 UDP: priority socket bind failed: {e}");
                return;
            }
        }
    } else if let Some(mut entry) = sessions.get_mut(&session_id) {
        entry.last_used = Instant::now();
        Arc::clone(&entry.sock)
    } else {
        match UdpSocket::bind("0.0.0.0:0").await {
            Ok(new_sock) => {
                let s = Arc::new(new_sock);
                sessions.insert(
                    session_id,
                    UdpSession {
                        sock: Arc::clone(&s),
                        last_used: Instant::now(),
                    },
                );
                s
            }
            Err(e) => {
                warn!(tag = %inbound_tag, "Hysteria2 UDP: socket bind failed: {e}");
                return;
            }
        }
    };

    let permit = match Arc::clone(&worker_limiter).try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            warn!(
                tag = %inbound_tag,
                max = MAX_UDP_WORKERS_PER_CONN,
                "Hysteria2 UDP worker limit reached; dropping datagram"
            );
            return;
        }
    };

    tokio::spawn(async move {
        let _permit = permit;
        if let Err(e) = sock.send_to(payload.as_ref(), dest_addr).await {
            warn!("Hysteria2 UDP send to {dest_addr}: {e}");
            return;
        }

        if matches!(tx_lane, DatagramLane::Priority) && datagram_policy.should_fast_retry_dns(&dest)
        {
            let retry_payload = payload.clone();
            let retry_sock = Arc::clone(&sock);
            let delay =
                std::time::Duration::from_millis(datagram_policy.fast_dns_retry_delay_ms.max(1));
            tokio::spawn(async move {
                sleep(delay).await;
                if let Err(e) = retry_sock.send_to(retry_payload.as_ref(), dest_addr).await {
                    warn!("Hysteria2 UDP fast-retry send failed: {e}");
                }
            });
        }

        let mut buf = vec![0u8; 65535];
        match timeout(UDP_REPLY_TIMEOUT, sock.recv_from(&mut buf)).await {
            Err(_) => {
                warn!("Hysteria2 UDP recv from {dest_addr}: reply timeout");
            }
            Ok(Err(e)) => {
                warn!("Hysteria2 UDP recv from {dest_addr}: {e}");
            }
            Ok(Ok((n, _src))) => {
                let response_dg = UdpDatagram {
                    session_id,
                    packet_id,
                    frag_id: 0,
                    frag_num: 1,
                    dest: dest.clone(),
                    data: bytes::Bytes::copy_from_slice(&buf[..n]),
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
                    warn!("Hysteria2 UDP: scheduled datagram channel closed");
                }
            }
        }
    });
}

async fn send_scheduled_udp_datagrams(
    conn: Connection,
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
                warn!("Hysteria2 UDP: scheduled send_datagram failed: {e}");
            }
            for followup in followups {
                if let Err(e) = conn.send_datagram(followup) {
                    warn!("Hysteria2 UDP: scheduled follow-up datagram failed: {e}");
                }
            }
        }
    }
}

async fn handle_h3_auth_request(
    resolver: h3::server::RequestResolver<H3QuinnConnection, bytes::Bytes>,
    password: &str,
    server_rx_bps: u64,
    udp_enabled: bool,
) -> Result<()> {
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
        return stream.finish().await.context("finish 404 stream");
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
            stream.finish().await.context("finish auth stream")
        }
        Err(AuthError::WrongPassword) => {
            let resp = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(())
                .context("build auth failure response")?;
            stream.send_response(resp).await.context("send auth 404")?;
            stream.finish().await.context("finish auth failure")
        }
        Err(AuthError::Protocol(msg)) => Err(anyhow::anyhow!("auth protocol error: {msg}")),
    }
}
