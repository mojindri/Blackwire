//! HTTP/3 front door for Hysteria2 — authentication then raw QUIC TCP streams and UDP datagrams.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context as _, Result};
use blackwire_app::context::Context;
use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::runtime_stats;
use blackwire_app::{wait_for_user_write_budget, UserBandwidthDirection};
use blackwire_common::{BoxedStream, ReunionStream};
use h3_quinn::Connection as H3QuinnConnection;
use http::{Response, StatusCode};
use quinn::Connection;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;
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
const MAX_UDP_WORKERS_PER_CONN: usize = 256;
/// Bound on the scheduled-datagram channel; backpressure instead of unbounded growth.
const SCHEDULED_UDP_CHANNEL_CAP: usize = 1024;

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
    tokio::spawn(async move {
        h3_guard_conn.closed().await;
        drop(h3_conn);
    });

    let inbound_tag = config.tag.clone();
    let user = auth.user.clone();
    let _user_permit = if let (Some(limiter), Some(user)) = (&config.user_limiter, &user) {
        let user: Arc<str> = user.clone().into();
        match limiter.try_acquire(Some(&user)) {
            Some(permit) => Some(permit),
            None => {
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
    let udp_tag = inbound_tag.clone();
    let datagram_enabled = config.datagram_enabled;
    let datagram_policy = config.datagram_policy;
    let fec = config.fec;
    let udp_user = user.clone();
    let udp_dispatcher = Arc::clone(&dispatcher);
    tokio::spawn(async move {
        serve_udp_sessions(
            udp_conn,
            udp_tag,
            udp_user,
            datagram_enabled,
            fec,
            datagram_policy,
            udp_dispatcher,
        )
        .await;
    });

    loop {
        let (mut send, mut recv) = conn
            .accept_bi()
            .await
            .context("accept Hysteria2 TCP stream")?;

        let dispatcher = Arc::clone(&dispatcher);
        let tag = inbound_tag.clone();
        let user = user.clone();
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
                user: user.map(Into::into),
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
    user: Option<String>,
    datagram_enabled: bool,
    fec: super::udp::FecPolicy,
    datagram_policy: super::udp::DatagramPolicy,
    dispatcher: Arc<dyn Dispatcher>,
) {
    if !datagram_enabled {
        super::udp::record_datagram_fallback("disabled");
        return;
    }
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

        let datagrams = fec_decoder.decode(raw);
        if datagrams.is_empty() {
            continue;
        }
        for dg in datagrams {
            let lane = datagram_policy.lane_for(&dg.dest, dg.data.len());
            record_datagram_packet(lane.class(), "rx");
            handle_udp_datagram(
                inbound_tag.clone(),
                user.clone(),
                Arc::clone(&worker_limiter),
                Arc::clone(&fec_encoder),
                scheduled_tx.clone(),
                dg,
                datagram_policy,
                Arc::clone(&dispatcher),
            )
            .await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_udp_datagram(
    inbound_tag: String,
    user: Option<String>,
    worker_limiter: Arc<Semaphore>,
    fec_encoder: Arc<std::sync::Mutex<FecEncoder>>,
    scheduled_tx: mpsc::Sender<ScheduledUdpDatagram>,
    dg: UdpDatagram,
    datagram_policy: super::udp::DatagramPolicy,
    dispatcher: Arc<dyn Dispatcher>,
) {
    let session_id = dg.session_id;
    let packet_id = dg.packet_id;
    let payload = dg.data;
    let dest = dg.dest;
    let tx_lane = datagram_policy.lane_for(&dest, payload.len());
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
        let address = destination_to_address(&dest);
        let user_arc = user.as_deref().map(Arc::<str>::from);
        let ctx = Context {
            sniffed_domain: None,
            source: None,
            inbound_tag: Arc::from(inbound_tag.clone()),
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
        let response = match dispatcher
            .dispatch_udp_datagram(ctx, address, payload)
            .await
        {
            Ok(response) => response,
            Err(e) => {
                warn!(tag = %inbound_tag, dest = ?dest, error = %e, "Hysteria2 UDP dispatch failed");
                return;
            }
        };
        runtime_stats::record_relay_traffic(
            &inbound_tag,
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
            &inbound_tag,
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
