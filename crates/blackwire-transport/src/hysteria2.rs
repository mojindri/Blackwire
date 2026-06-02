//! Hysteria2 transport — QUIC-based proxy protocol for high-latency links.
//!
//! External clients (sing-box, Xray, Hiddify) speak HTTP/3 for authentication and
//! QUIC-varint TCP framing on subsequent streams. See the [Hysteria2 protocol spec](https://v2.hysteria.network/docs/developers/Protocol/).

pub mod auth;
pub mod http3;
pub mod proto;
pub mod tcp;
pub mod udp;
mod varint;

pub use auth::AuthError;
pub use proto::{AuthRequest, AuthResponse, TcpRequest, TcpResponse};
pub use udp::Destination as UdpDestination;

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use anyhow::Result;
use blackwire_app::context::Context;
use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::features::OutboundHandler;
use blackwire_common::{Address, BoxedStream, ProxyError, ReunionStream};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{timeout, Instant, Sleep};
use tracing::{info, warn};

/// Maximum concurrent QUIC connections on a single Hysteria2 server.
///
/// The official hysteria2 server defaults to `maxIncomingConnections: 1024`.
/// sing-quic has no cap, but we follow the reference implementation.
const MAX_HYSTERIA2_CONNECTIONS: usize = 1024;
const MAX_HYSTERIA2_CLIENT_SHARDS: usize = 16;
const CLIENT_AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const CLIENT_OPEN_STREAM_TIMEOUT: Duration = Duration::from_secs(5);
const LOW_LATENCY_PACER_BURST: u64 = 64 * 1024;
const THROUGHPUT_PACER_BURST: u64 = 256 * 1024;

use crate::quic::{
    build_hysteria2_server_endpoint_with_congestion_and_socket, ensure_crypto_provider,
    BadNetControllerFactory, BrutalCCFactory, CongestionConfig, CongestionDirection,
    CongestionMode, QuicSocketConfig,
};

pub use udp::{DatagramLane, DatagramPolicy, DatagramPriorityMode, FecMode, FecPolicy};

/// Configuration for a Hysteria2 inbound server.
#[derive(Debug, Clone)]
pub struct Hysteria2ServerConfig {
    /// Inbound tag used for routing rules.
    pub tag: String,
    /// Socket address to listen on (for example `0.0.0.0:443`).
    pub addr: SocketAddr,
    /// Shared password that clients must send during HTTP/3 auth.
    pub password: String,
    /// Max client → server rate in Mbps (server receive / `Hysteria-CC-RX` in auth response).
    pub up_mbps: u64,
    /// Max server → client rate in Mbps (used for Brutal on server→client path when enabled).
    pub down_mbps: u64,
    /// Server certificate in PEM format.
    pub cert_pem: String,
    /// Private key for `cert_pem`, in PEM format.
    pub key_pem: String,
    /// Maximum concurrent QUIC connections. Falls back to `MAX_HYSTERIA2_CONNECTIONS`.
    pub max_connections: Option<usize>,
    /// QUIC congestion policy for server-to-client response traffic.
    pub congestion: CongestionConfig,
    /// UDP socket tuning and server endpoint sharding policy.
    pub socket: QuicSocketConfig,
    /// Whether UDP traffic should use the QUIC DATAGRAM lane.
    pub datagram_enabled: bool,
    /// Forward-error-correction policy for UDP datagrams.
    pub fec: FecPolicy,
    /// UDP datagram scheduling policy and DNS fast-retry settings.
    pub datagram_policy: DatagramPolicy,
}

/// Configuration for a Hysteria2 outbound client.
#[derive(Debug, Clone)]
pub struct Hysteria2ClientConfig {
    /// Remote Hysteria2 server socket address.
    pub server: SocketAddr,
    /// TLS server name (SNI) to present during QUIC handshake.
    pub server_name: String,
    /// Shared password used for HTTP/3 auth.
    pub password: String,
    /// Max client upload rate in Mbps.
    pub up_mbps: u64,
    /// Max client download rate in Mbps.
    pub down_mbps: u64,
    /// If `true`, skip TLS certificate verification (unsafe, for testing only).
    pub skip_cert_verify: bool,
    /// QUIC congestion policy for lossy/mobile links.
    pub congestion: CongestionConfig,
    /// Number of local QUIC endpoint shards requested by config.
    pub endpoint_shards: usize,
    /// UDP socket tuning for client endpoint shards.
    pub socket: QuicSocketConfig,
    /// Whether UDP traffic should use the QUIC DATAGRAM lane.
    pub datagram_enabled: bool,
    /// Forward-error-correction policy for UDP datagrams.
    pub fec: FecPolicy,
    /// UDP datagram scheduling policy and DNS fast-retry settings.
    pub datagram_policy: DatagramPolicy,
}

/// A Hysteria2 proxy server.
pub struct Hysteria2Server {
    config: Hysteria2ServerConfig,
}

impl Hysteria2Server {
    /// Build a Hysteria2 server from static config.
    pub fn new(config: Hysteria2ServerConfig) -> Self {
        Self { config }
    }

    /// Start accepting QUIC connections and proxying TCP streams.
    ///
    /// This runs until the endpoint is closed or the task is cancelled.
    pub async fn serve(&self, dispatcher: Arc<dyn Dispatcher>) -> Result<()> {
        let endpoints = self.server_endpoints()?;

        info!(
            addr = %self.config.addr,
            endpoints = endpoints.len(),
            reuse_port = self.config.socket.reuse_port,
            "Hysteria2 server listening (HTTP/3)"
        );

        let cap = self
            .config
            .max_connections
            .unwrap_or(MAX_HYSTERIA2_CONNECTIONS);
        let conn_limiter = Arc::new(Semaphore::new(cap));
        let mut tasks = Vec::with_capacity(endpoints.len());

        for endpoint in endpoints {
            let config = self.config.clone();
            let dispatcher = Arc::clone(&dispatcher);
            let conn_limiter = Arc::clone(&conn_limiter);
            tasks.push(tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let permit = match Arc::clone(&conn_limiter).try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => {
                            warn!(
                                max = MAX_HYSTERIA2_CONNECTIONS,
                                "Hysteria2 connection limit reached; dropping incoming QUIC connection"
                            );
                            continue;
                        }
                    };

                    let conn = match incoming.await {
                        Ok(c) => c,
                        Err(e) => {
                            warn!("QUIC connection failed during handshake: {e}");
                            continue;
                        }
                    };

                    let config = config.clone();
                    let dispatcher = Arc::clone(&dispatcher);
                    tokio::spawn(async move {
                        let _permit = permit;
                        if let Err(e) = http3::serve_connection(conn, config, dispatcher).await {
                            warn!("Hysteria2 connection closed: {e}");
                        }
                    });
                }
        }));
        }

        for task in tasks {
            let _ = task.await;
        }
        Ok(())
    }

    fn server_endpoints(&self) -> Result<Vec<quinn::Endpoint>> {
        let requested = self.config.socket.endpoint_count.max(1);
        let count = if self.config.socket.reuse_port {
            requested
        } else {
            1
        };
        let mut endpoints = Vec::with_capacity(count);
        for idx in 0..count {
            let mut socket = self.config.socket;
            socket.endpoint_count = count;
            match build_hysteria2_server_endpoint_with_congestion_and_socket(
                self.config.addr,
                &self.config.cert_pem,
                &self.config.key_pem,
                self.config.up_mbps,
                self.config.down_mbps,
                Some(self.config.congestion.clone()),
                socket,
            ) {
                Ok(endpoint) => endpoints.push(endpoint),
                Err(e) if idx > 0 => {
                    warn!(
                        endpoint = idx,
                        error = %e,
                        "Hysteria2 extra endpoint bind failed; continuing with fewer shards"
                    );
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(endpoints)
    }
}

/// A Hysteria2 proxy client.
pub struct Hysteria2Client {
    config: Hysteria2ClientConfig,
    sessions: Vec<Mutex<Option<Arc<Hysteria2ClientSession>>>>,
    next_session: AtomicUsize,
}

struct Hysteria2ClientSession {
    conn: quinn::Connection,
    _endpoint: quinn::Endpoint,
    _h3_driver: tokio::task::JoinHandle<()>,
}

impl Drop for Hysteria2ClientSession {
    fn drop(&mut self) {
        self._h3_driver.abort();
    }
}

impl Hysteria2Client {
    /// Build a Hysteria2 client from static config.
    pub fn new(config: Hysteria2ClientConfig) -> Self {
        let shard_count = config
            .endpoint_shards
            .max(1)
            .min(MAX_HYSTERIA2_CLIENT_SHARDS);
        let sessions = (0..shard_count).map(|_| Mutex::new(None)).collect();
        Self {
            config,
            sessions,
            next_session: AtomicUsize::new(0),
        }
    }

    /// Connect to the server, authenticate, and open one proxied TCP stream.
    ///
    /// The returned stream is ready to carry bytes for `dest`.
    pub async fn connect_and_dial(&self, dest: &Address) -> Result<BoxedStream, ProxyError> {
        let shard = self.next_shard();
        self.open_with_reconnect(shard, dest).await
    }

    async fn prewarm(&self) {
        if self.sessions.len() <= 1 {
            return;
        }
        for shard in 0..self.sessions.len() {
            if let Err(e) = self.session(shard).await {
                tracing::debug!(shard, error = %e, "Hysteria2 shard prewarm failed");
            }
        }
    }

    fn next_shard(&self) -> usize {
        self.next_session.fetch_add(1, Ordering::Relaxed) % self.sessions.len()
    }

    async fn session(&self, shard: usize) -> Result<Arc<Hysteria2ClientSession>, ProxyError> {
        let mut guard = self.sessions[shard].lock().await;
        if let Some(session) = guard.as_ref() {
            if session.conn.close_reason().is_none() {
                return Ok(Arc::clone(session));
            }
        }

        let rx_bps = self.config.down_mbps.saturating_mul(1_000_000 / 8);
        let transport_arc = Arc::new(self.transport_config());

        let client_config =
            build_hysteria2_client_config(self.config.skip_cert_verify, transport_arc)
                .map_err(|e| ProxyError::Transport(e.to_string()))?;

        let endpoint = crate::quic::build_client_endpoint_with_alpn_and_socket(
            self.config.skip_cert_verify,
            &[b"h3".to_vec()],
            self.config.socket,
        )
        .map_err(|e| ProxyError::Transport(format!("client endpoint: {e}")))?;

        let server_name = &self.config.server_name;
        let conn = endpoint
            .connect_with(client_config, self.config.server, server_name)
            .map_err(|e| ProxyError::Transport(format!("QUIC connect: {e}")))?
            .await
            .map_err(|e| ProxyError::Transport(format!("QUIC handshake: {e}")))?;

        let h3_driver = timeout(
            CLIENT_AUTH_TIMEOUT,
            client_h3_auth(&conn, &self.config.password, rx_bps),
        )
        .await
        .map_err(|_| ProxyError::Timeout)??;

        let session = Arc::new(Hysteria2ClientSession {
            conn,
            _endpoint: endpoint,
            _h3_driver: h3_driver,
        });
        *guard = Some(Arc::clone(&session));
        Ok(session)
    }

    async fn clear_session(&self, shard: usize, session: &Arc<Hysteria2ClientSession>) {
        let mut guard = self.sessions[shard].lock().await;
        if guard
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            *guard = None;
        }
    }

    async fn open_with_reconnect(
        &self,
        shard: usize,
        dest: &Address,
    ) -> Result<BoxedStream, ProxyError> {
        let session = self.session(shard).await?;
        match self.open_proxy_stream(Arc::clone(&session), dest).await {
            Ok(stream) => Ok(stream),
            Err(first) => {
                self.clear_session(shard, &session).await;
                let session = self.session(shard).await?;
                self.open_proxy_stream(session, dest).await.or(Err(first))
            }
        }
    }

    async fn open_proxy_stream(
        &self,
        session: Arc<Hysteria2ClientSession>,
        dest: &Address,
    ) -> Result<BoxedStream, ProxyError> {
        let (mut send, mut recv) = timeout(CLIENT_OPEN_STREAM_TIMEOUT, session.conn.open_bi())
            .await
            .map_err(|_| ProxyError::Timeout)?
            .map_err(|e| ProxyError::Transport(format!("open proxy stream: {e}")))?;

        tcp::client_write_request(&mut send, dest).await?;
        tcp::client_read_response(&mut recv).await?;

        Ok(Box::new(Hysteria2Stream {
            inner: ReunionStream::new(recv, send),
            pacer: pacer_for_config(
                &self.config.congestion,
                CongestionDirection::ClientUpload,
                "client-upload",
            ),
            _session: session,
        }))
    }

    fn transport_config(&self) -> quinn::TransportConfig {
        let mut transport_config = quinn::TransportConfig::default();
        configure_congestion(&mut transport_config, &self.config.congestion);
        crate::quic::badnet::record_mode(self.config.congestion.mode);
        crate::quic::badnet::record_endpoint_shards(self.config.endpoint_shards.max(1));

        // Size QUIC flow-control windows to the configured bandwidth × 500 ms RTT.
        // Without this, congestion control can be stalled waiting for
        // STREAM_DATA_BLOCKED acknowledgement before the CC window fills.
        let (stream_rx, conn_rx, conn_tx) = crate::quic::bdp_windows_with_profile(
            self.config.down_mbps,
            self.config.up_mbps,
            self.config.congestion.window_profile(),
        );
        transport_config.stream_receive_window(stream_rx);
        transport_config.receive_window(conn_rx);
        transport_config.send_window(conn_tx);
        transport_config
    }
}

fn configure_congestion(transport_config: &mut quinn::TransportConfig, cfg: &CongestionConfig) {
    match cfg.mode {
        CongestionMode::StandardQuic => {}
        CongestionMode::BrutalCompatible => {
            transport_config
                .congestion_controller_factory(Arc::new(BrutalCCFactory::new(cfg.target_bps())));
        }
        CongestionMode::NovaCc
        | CongestionMode::BadNetLowLatency
        | CongestionMode::BadNetThroughput
        | CongestionMode::AutoProbe => {
            transport_config.congestion_controller_factory(Arc::new(
                BadNetControllerFactory::new_for_direction(
                    cfg.clone(),
                    CongestionDirection::ClientUpload,
                ),
            ));
        }
    }
}

/// Perform the HTTP/3 authentication handshake.
async fn client_h3_auth(
    conn: &quinn::Connection,
    password: &str,
    rx_bps: u64,
) -> Result<tokio::task::JoinHandle<()>, ProxyError> {
    use http::header::{HeaderName, HeaderValue};
    use http::{Method, Request};

    let (driver, mut send_request) = h3::client::new(h3_quinn::Connection::new(conn.clone()))
        .await
        .map_err(|e| ProxyError::Transport(format!("h3 client: {e}")))?;

    let driver_task = tokio::spawn(async move {
        // Hysteria2 uses HTTP/3 only for the auth request. After auth, TCP
        // proxy streams use raw QUIC stream type 0x401 and UDP uses QUIC
        // DATAGRAM. Polling the h3 driver to idle after auth can close the
        // underlying QUIC connection, so retain the h3 connection object
        // without driving it once the auth exchange has completed.
        let _driver = driver;
        std::future::pending::<()>().await;
    });

    let mut auth_uri = String::with_capacity(proto::AUTH_HOST.len() + proto::AUTH_PATH.len() + 8);
    auth_uri.push_str("https://");
    auth_uri.push_str(proto::AUTH_HOST);
    auth_uri.push_str(proto::AUTH_PATH);
    let mut req_builder = Request::builder().method(Method::POST).uri(auth_uri);
    req_builder = req_builder.header(http::header::HOST, proto::AUTH_HOST);
    req_builder = req_builder.header(
        HeaderName::from_static("hysteria-auth"),
        HeaderValue::from_str(password).map_err(|e| ProxyError::Protocol(e.to_string()))?,
    );
    req_builder = req_builder.header(
        HeaderName::from_static("hysteria-cc-rx"),
        HeaderValue::from_str(&rx_bps.to_string())
            .map_err(|e| ProxyError::Protocol(e.to_string()))?,
    );
    req_builder = req_builder.header(
        HeaderName::from_static("hysteria-padding"),
        HeaderValue::from_static(""),
    );
    let req = req_builder
        .body(())
        .map_err(|e| ProxyError::Protocol(e.to_string()))?;

    let mut stream = send_request
        .send_request(req)
        .await
        .map_err(|e| ProxyError::Transport(format!("send auth request: {e}")))?;
    stream
        .finish()
        .await
        .map_err(|e| ProxyError::Transport(format!("finish auth request: {e}")))?;

    let resp = stream
        .recv_response()
        .await
        .map_err(|e| ProxyError::Transport(format!("recv auth response: {e}")))?;

    if resp.status().as_u16() != proto::STATUS_AUTH_OK {
        driver_task.abort();
        return Err(ProxyError::AuthFailed);
    }

    let _auth = proto::auth_response_from_headers(resp.headers(), resp.status().as_u16());
    Ok(driver_task)
}

struct Hysteria2Stream {
    inner: ReunionStream<quinn::RecvStream, quinn::SendStream>,
    pacer: Option<Pacer>,
    _session: Arc<Hysteria2ClientSession>,
}

impl AsyncRead for Hysteria2Stream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for Hysteria2Stream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let allowed = match self.pacer.as_mut() {
            Some(pacer) => match pacer.poll_allow(cx, buf.len()) {
                Poll::Ready(allowed) => allowed,
                Poll::Pending => return Poll::Pending,
            },
            None => buf.len(),
        };
        Pin::new(&mut self.inner).poll_write(cx, &buf[..allowed])
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub(crate) struct PacedStream<S> {
    inner: S,
    pacer: Option<Pacer>,
}

impl<S> PacedStream<S> {
    pub(crate) fn new(inner: S, pacer: Option<Pacer>) -> Self {
        Self { inner, pacer }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PacedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PacedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let allowed = match self.pacer.as_mut() {
            Some(pacer) => match pacer.poll_allow(cx, buf.len()) {
                Poll::Ready(allowed) => allowed,
                Poll::Pending => return Poll::Pending,
            },
            None => buf.len(),
        };
        Pin::new(&mut self.inner).poll_write(cx, &buf[..allowed])
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub(crate) fn server_download_pacer(cfg: &CongestionConfig) -> Option<Pacer> {
    pacer_for_config(cfg, CongestionDirection::ServerDownload, "server-download")
}

fn pacer_for_config(
    cfg: &CongestionConfig,
    direction: CongestionDirection,
    lane: &'static str,
) -> Option<Pacer> {
    if matches!(cfg.mode, CongestionMode::StandardQuic) {
        return None;
    }

    let target = cfg.target_bps_for(direction);
    if target == 0 {
        return None;
    }

    let ack_rate = cfg.min_ack_rate.clamp(0.05, 1.0);
    let rate_bps = ((target as f64) / ack_rate).ceil() as u64;
    let burst_window = match cfg.mode {
        CongestionMode::BadNetLowLatency => Duration::from_millis(10),
        _ => Duration::from_millis(25),
    };
    let burst_cap = match cfg.mode {
        CongestionMode::BadNetLowLatency => LOW_LATENCY_PACER_BURST,
        _ => THROUGHPUT_PACER_BURST,
    };
    let burst_bytes = ((rate_bps as f64) * burst_window.as_secs_f64())
        .ceil()
        .max(1.0) as u64;
    Some(Pacer::new(rate_bps, burst_bytes.min(burst_cap), lane))
}

pub(crate) struct Pacer {
    rate_bps: u64,
    burst_bytes: u64,
    tokens: f64,
    last: Instant,
    sleep: Option<Pin<Box<Sleep>>>,
    lane: &'static str,
}

impl Pacer {
    fn new(rate_bps: u64, burst_bytes: u64, lane: &'static str) -> Self {
        metrics::gauge!("blackwire_hysteria2_pacer_rate_bps", "lane" => lane).set(rate_bps as f64);
        metrics::gauge!("blackwire_hysteria2_pacer_burst_bytes", "lane" => lane)
            .set(burst_bytes as f64);
        Self {
            rate_bps,
            burst_bytes,
            tokens: burst_bytes as f64,
            last: Instant::now(),
            sleep: None,
            lane,
        }
    }

    fn poll_allow(&mut self, cx: &mut TaskContext<'_>, requested: usize) -> Poll<usize> {
        if requested == 0 {
            return Poll::Ready(0);
        }

        self.refill();
        if self.tokens >= 1.0 {
            return Poll::Ready(self.consume(requested));
        }

        if self.sleep.is_none() {
            let deficit = 1.0 - self.tokens;
            let wait = Duration::from_secs_f64((deficit / self.rate_bps as f64).max(0.000_001));
            metrics::counter!("blackwire_hysteria2_pacer_sleep_total", "lane" => self.lane)
                .increment(1);
            metrics::counter!("blackwire_hysteria2_pacer_sleep_ms_total", "lane" => self.lane)
                .increment(wait.as_millis().max(1) as u64);
            self.sleep = Some(Box::pin(tokio::time::sleep(wait)));
        }

        match self
            .sleep
            .as_mut()
            .expect("sleep initialized")
            .as_mut()
            .poll(cx)
        {
            Poll::Pending => Poll::Pending,
            Poll::Ready(()) => {
                self.sleep = None;
                self.refill();
                if self.tokens < 1.0 {
                    self.tokens = 1.0;
                }
                Poll::Ready(self.consume(requested))
            }
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.rate_bps as f64).min(self.burst_bytes as f64);
    }

    fn consume(&mut self, requested: usize) -> usize {
        let allowed = requested.min(self.tokens.floor().max(1.0) as usize);
        if allowed < requested {
            metrics::counter!("blackwire_hysteria2_pacer_limited_writes_total", "lane" => self.lane)
                .increment(1);
        }
        self.tokens -= allowed as f64;
        allowed
    }
}

/// Outbound handler that dials destinations through a Hysteria2 client.
pub struct Hysteria2OutboundHandler {
    client: Arc<Hysteria2Client>,
    tag: String,
}

impl Hysteria2OutboundHandler {
    /// Create a shared outbound handler with a fixed tag.
    pub fn new(config: Hysteria2ClientConfig, tag: String) -> Arc<Self> {
        let client = Arc::new(Hysteria2Client::new(config));
        let warm_client = Arc::clone(&client);
        tokio::spawn(async move {
            warm_client.prewarm().await;
        });
        Arc::new(Self { client, tag })
    }
}

#[async_trait::async_trait]
impl OutboundHandler for Hysteria2OutboundHandler {
    fn tag(&self) -> &str {
        &self.tag
    }

    async fn connect(&self, _ctx: &Context, dest: &Address) -> Result<BoxedStream, ProxyError> {
        self.client.connect_and_dial(dest).await
    }
}

fn build_hysteria2_client_config(
    skip_verify: bool,
    transport: Arc<quinn::TransportConfig>,
) -> anyhow::Result<quinn::ClientConfig> {
    use anyhow::Context as _;
    use quinn::crypto::rustls::QuicClientConfig;

    ensure_crypto_provider();

    let mut tls_config = if skip_verify {
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipVerifier))
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        let result = rustls_native_certs::load_native_certs();
        for cert in result.certs {
            let _ = roots.add(cert);
        }
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };
    tls_config.alpn_protocols = vec![b"h3".to_vec()];

    let quic_config = QuicClientConfig::try_from(tls_config).context("build QuicClientConfig")?;
    let mut config = quinn::ClientConfig::new(Arc::new(quic_config));
    config.transport_config(transport);
    Ok(config)
}

#[derive(Debug)]
struct SkipVerifier;

impl rustls::client::danger::ServerCertVerifier for SkipVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn congestion(mode: CongestionMode) -> CongestionConfig {
        CongestionConfig {
            mode,
            up_mbps: 10,
            down_mbps: 100,
            min_ack_rate: 0.8,
            ..CongestionConfig::default()
        }
    }

    #[test]
    fn standard_quic_keeps_pacing_disabled_for_control_rows() {
        let cfg = congestion(CongestionMode::StandardQuic);
        assert!(pacer_for_config(&cfg, CongestionDirection::ClientUpload, "test").is_none());
    }

    #[test]
    fn badnet_low_latency_uses_small_burst_cap() {
        let mut cfg = congestion(CongestionMode::BadNetLowLatency);
        cfg.up_mbps = 100;
        let pacer = pacer_for_config(&cfg, CongestionDirection::ClientUpload, "test")
            .expect("badnet low latency enables pacing");

        assert_eq!(pacer.burst_bytes, LOW_LATENCY_PACER_BURST);
        assert_eq!(pacer.rate_bps, 15_625_000);
    }

    #[test]
    fn badnet_throughput_uses_larger_burst_cap() {
        let cfg = congestion(CongestionMode::BadNetThroughput);
        let pacer = pacer_for_config(&cfg, CongestionDirection::ServerDownload, "test")
            .expect("badnet throughput enables pacing");

        assert_eq!(pacer.burst_bytes, THROUGHPUT_PACER_BURST);
        assert_eq!(pacer.rate_bps, 15_625_000);
    }

    #[test]
    fn server_download_pacer_uses_downlink_rate() {
        let cfg = congestion(CongestionMode::NovaCc);
        let pacer = server_download_pacer(&cfg).expect("nova enables pacing");

        assert_eq!(pacer.rate_bps, 15_625_000);
    }

    #[test]
    fn badnet_low_latency_window_profile_is_smaller_than_throughput() {
        let low = congestion(CongestionMode::BadNetLowLatency).window_profile();
        let throughput = congestion(CongestionMode::BadNetThroughput).window_profile();

        assert_eq!(low.bdp_rtt, Duration::from_millis(150));
        assert_eq!(low.min_window_bytes, 1024 * 1024);
        assert_eq!(low.max_window_bytes, 32 * 1024 * 1024);
        assert_eq!(low.conn_window_multiplier, 2);
        assert!(low.max_window_bytes < throughput.max_window_bytes);
        assert!(low.bdp_rtt < throughput.bdp_rtt);
    }
}

/// A client-side Hysteria2 UDP session.
///
/// Wraps the QUIC connection for sending and receiving UDP datagrams.
/// Create one per client UDP flow (SOCKS5 session or test harness).
pub struct Hysteria2UdpSession {
    conn: quinn::Connection,
    _endpoint: quinn::Endpoint,
    session_id: u32,
    packet_id: std::sync::atomic::AtomicU16,
    datagram_enabled: bool,
    datagram_policy: DatagramPolicy,
    fec_encoder: StdMutex<udp::FecEncoder>,
    fec_decoder: StdMutex<udp::FecDecoder>,
}

impl Hysteria2UdpSession {
    /// Connect to a Hysteria2 server and authenticate.
    ///
    /// Returns a UDP session ready for `send` / `recv`.
    pub async fn connect(config: &Hysteria2ClientConfig) -> Result<Self, ProxyError> {
        let rx_bps = config.down_mbps.saturating_mul(1_000_000 / 8);
        let mut transport_config = quinn::TransportConfig::default();
        configure_congestion(&mut transport_config, &config.congestion);
        crate::quic::badnet::record_mode(config.congestion.mode);
        crate::quic::badnet::record_endpoint_shards(config.endpoint_shards.max(1));
        let (stream_rx, conn_rx, conn_tx) = crate::quic::bdp_windows_with_profile(
            config.down_mbps,
            config.up_mbps,
            config.congestion.window_profile(),
        );
        transport_config.stream_receive_window(stream_rx);
        transport_config.receive_window(conn_rx);
        transport_config.send_window(conn_tx);
        // Enable QUIC datagrams for UDP relay.
        transport_config.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
        transport_config.datagram_send_buffer_size(2 * 1024 * 1024);

        let transport_arc = Arc::new(transport_config);
        let client_config = build_hysteria2_client_config(config.skip_cert_verify, transport_arc)
            .map_err(|e| ProxyError::Transport(e.to_string()))?;

        let endpoint = crate::quic::build_client_endpoint_with_alpn_and_socket(
            config.skip_cert_verify,
            &[b"h3".to_vec()],
            config.socket,
        )
        .map_err(|e| ProxyError::Transport(format!("client endpoint: {e}")))?;

        let conn = endpoint
            .connect_with(client_config, config.server, &config.server_name)
            .map_err(|e| ProxyError::Transport(format!("QUIC connect: {e}")))?
            .await
            .map_err(|e| ProxyError::Transport(format!("QUIC handshake: {e}")))?;

        client_h3_auth(&conn, &config.password, rx_bps).await?;

        Ok(Self {
            conn,
            _endpoint: endpoint,
            session_id: rand::random(),
            packet_id: std::sync::atomic::AtomicU16::new(0),
            datagram_enabled: config.datagram_enabled,
            datagram_policy: config.datagram_policy,
            fec_encoder: StdMutex::new(udp::FecEncoder::new(config.fec)),
            fec_decoder: StdMutex::new(udp::FecDecoder::new(config.fec)),
        })
    }

    /// Send a UDP payload to `dest` through the Hysteria2 tunnel.
    pub fn send(&self, dest: udp::Destination, data: bytes::Bytes) -> Result<(), ProxyError> {
        if !self.datagram_enabled {
            udp::record_datagram_fallback("disabled");
            return Err(ProxyError::Protocol(
                "Hysteria2 UDP DATAGRAM lane disabled".into(),
            ));
        }
        let packet_id = self
            .packet_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dg = udp::UdpDatagram {
            session_id: self.session_id,
            packet_id,
            frag_id: 0,
            frag_num: 1,
            dest,
            data,
        };
        let lane = self.datagram_policy.lane_for(&dg.dest, dg.data.len());
        let encoded = udp::encode_udp_datagram(&dg);
        let parity = if matches!(lane, DatagramLane::Priority) {
            self.fec_encoder
                .lock()
                .map_err(|_| ProxyError::Transport("FEC encoder lock poisoned".into()))?
                .protect(&dg, &encoded)
        } else {
            None
        };
        udp::record_datagram_packet(lane.class(), "tx");
        crate::quic::record_endpoint_io("client", "tx", encoded.len());
        self.conn
            .send_datagram(encoded)
            .map_err(|e| ProxyError::Transport(format!("send_datagram: {e}")))?;
        if let Some(parity) = parity {
            self.conn
                .send_datagram(parity)
                .map_err(|e| ProxyError::Transport(format!("send FEC datagram: {e}")))?;
        }
        Ok(())
    }

    /// Receive a UDP response datagram from the server.
    pub async fn recv(&self) -> Result<udp::UdpDatagram, ProxyError> {
        let raw = self
            .conn
            .read_datagram()
            .await
            .map_err(|e| ProxyError::Transport(format!("read_datagram: {e}")))?;
        crate::quic::record_endpoint_io("client", "rx", raw.len());
        let decoded = self
            .fec_decoder
            .lock()
            .map_err(|_| ProxyError::Transport("FEC decoder lock poisoned".into()))?
            .decode(raw);
        if let Some(dg) = decoded.into_iter().next() {
            let lane = self.datagram_policy.lane_for(&dg.dest, dg.data.len());
            udp::record_datagram_packet(lane.class(), "rx");
            Ok(dg)
        } else {
            Err(ProxyError::Transport("received only FEC metadata".into()))
        }
    }
}
