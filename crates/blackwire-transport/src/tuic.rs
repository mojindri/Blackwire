//! TUIC v5 transport implementation.
//!
//! Implements the TUIC v5 command stream over QUIC for TCP proxying and native
//! QUIC DATAGRAM UDP relay. The wire format follows the TUIC Protocol SPEC.md
//! version `0x05`.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        atomic::{AtomicU16, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use blackwire_app::{context::Context, dispatcher::Dispatcher, features::OutboundHandler};
use blackwire_common::{Address, BoxedStream, ProxyError, ReunionStream};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use dashmap::DashMap;
use quinn::{Connection, Endpoint, RecvStream, SendStream};
use tokio::{
    net::UdpSocket,
    sync::{watch, Mutex, Semaphore},
    time::timeout,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::quic::{
    build_client_endpoint_with_alpn_and_socket, build_server_endpoint_with_alpn_and_socket,
    QuicSocketConfig,
};

const TUIC_VERSION: u8 = 0x05;
const CMD_AUTHENTICATE: u8 = 0x00;
const CMD_CONNECT: u8 = 0x01;
const CMD_PACKET: u8 = 0x02;
const CMD_DISSOCIATE: u8 = 0x03;
const CMD_HEARTBEAT: u8 = 0x04;

const ADDR_DOMAIN: u8 = 0x00;
const ADDR_IPV4: u8 = 0x01;
const ADDR_IPV6: u8 = 0x02;
const ADDR_NONE: u8 = 0xff;

const OPEN_STREAM_TIMEOUT: Duration = Duration::from_secs(5);
const UDP_REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const UDP_SESSION_IDLE: Duration = Duration::from_secs(60);
const MAX_CONNECTIONS: usize = 1024;
const MAX_CLIENT_SESSIONS: usize = 8;
const MAX_UDP_SESSIONS_PER_CONN: usize = 512;
const MAX_UDP_WORKERS_PER_CONN: usize = 256;
const MAX_PACKET_SIZE: usize = 64 * 1024;

/// TUIC v5 user credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuicUser {
    pub uuid: Uuid,
    pub password: String,
}

/// Configuration for a TUIC v5 inbound server.
#[derive(Debug, Clone)]
pub struct TuicServerConfig {
    pub tag: String,
    pub addr: SocketAddr,
    pub users: Vec<TuicUser>,
    pub cert_pem: String,
    pub key_pem: String,
    pub server_name: Option<String>,
    pub max_connections: Option<usize>,
    pub auth_timeout: Duration,
    pub socket: QuicSocketConfig,
    pub enable_udp: bool,
}

/// Configuration for a TUIC v5 outbound client.
#[derive(Debug, Clone)]
pub struct TuicClientConfig {
    pub server: SocketAddr,
    pub server_name: String,
    pub uuid: Uuid,
    pub password: String,
    pub skip_cert_verify: bool,
    pub endpoint_shards: usize,
    pub socket: QuicSocketConfig,
    pub enable_udp: bool,
}

/// A TUIC v5 inbound server.
pub struct TuicServer {
    config: TuicServerConfig,
}

impl TuicServer {
    pub fn new(config: TuicServerConfig) -> Self {
        Self { config }
    }

    pub async fn serve(&self, dispatcher: Arc<dyn Dispatcher>) -> Result<()> {
        let endpoints = self.server_endpoints()?;
        info!(addr = %self.config.addr, endpoints = endpoints.len(), "TUIC v5 server listening");

        let cap = self.config.max_connections.unwrap_or(MAX_CONNECTIONS);
        let limiter = Arc::new(Semaphore::new(cap));
        let mut tasks = Vec::with_capacity(endpoints.len());

        for endpoint in endpoints {
            let config = self.config.clone();
            let dispatcher = Arc::clone(&dispatcher);
            let limiter = Arc::clone(&limiter);
            tasks.push(tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    let permit = match Arc::clone(&limiter).try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            warn!("TUIC v5 connection limit reached; dropping incoming QUIC connection");
                            continue;
                        }
                    };

                    let conn = match incoming.await {
                        Ok(conn) => conn,
                        Err(e) => {
                            warn!("TUIC v5 QUIC handshake failed: {e}");
                            continue;
                        }
                    };
                    let config = config.clone();
                    let dispatcher = Arc::clone(&dispatcher);
                    tokio::spawn(async move {
                        let _permit = permit;
                        if let Err(e) = serve_connection(conn, config, dispatcher).await {
                            warn!("TUIC v5 connection closed: {e}");
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

    fn server_endpoints(&self) -> Result<Vec<Endpoint>> {
        let requested = self.config.socket.endpoint_count.max(1);
        let count = if self.config.socket.reuse_port {
            requested
        } else {
            1
        };
        let alpn = self.alpn();
        let mut endpoints = Vec::with_capacity(count);
        for idx in 0..count {
            let mut socket = self.config.socket;
            socket.endpoint_count = count;
            match build_server_endpoint_with_alpn_and_socket(
                self.config.addr,
                &self.config.cert_pem,
                &self.config.key_pem,
                &alpn,
                socket,
            ) {
                Ok(endpoint) => endpoints.push(endpoint),
                Err(e) if idx > 0 => {
                    warn!(endpoint = idx, error = %e, "TUIC v5 extra endpoint bind failed; continuing with fewer shards");
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(endpoints)
    }

    fn alpn(&self) -> Vec<Vec<u8>> {
        // TUIC v5 commonly runs with h3 ALPN in sing-box/mihomo deployments.
        vec![b"h3".to_vec()]
    }
}

async fn serve_connection(
    conn: Connection,
    config: TuicServerConfig,
    dispatcher: Arc<dyn Dispatcher>,
) -> Result<()> {
    let users = Arc::new(
        config
            .users
            .iter()
            .map(|u| (u.uuid, u.password.clone()))
            .collect::<HashMap<_, _>>(),
    );
    let (auth_tx, auth_rx) = watch::channel::<Option<Uuid>>(None);

    let auth_conn = conn.clone();
    let auth_users = Arc::clone(&users);
    let auth_timeout = config.auth_timeout;
    tokio::spawn(async move {
        if let Err(e) = accept_authentication(auth_conn, auth_users, auth_tx, auth_timeout).await {
            warn!("TUIC v5 authentication failed: {e}");
        }
    });

    if config.enable_udp {
        let udp_conn = conn.clone();
        let udp_auth = auth_rx.clone();
        tokio::spawn(async move { serve_udp_datagrams(udp_conn, udp_auth).await });
    }

    loop {
        let (mut send, mut recv) = conn.accept_bi().await.context("accept TUIC stream")?;
        let dispatcher = Arc::clone(&dispatcher);
        let tag = config.tag.clone();
        let mut auth_rx = auth_rx.clone();
        tokio::spawn(async move {
            if wait_authenticated(&mut auth_rx).await.is_none() {
                let _ = send.finish();
                return;
            }
            let dest = match read_connect_command(&mut recv).await {
                Ok(dest) => dest,
                Err(e) => {
                    warn!("TUIC v5 bad connect command: {e}");
                    let _ = send.finish();
                    return;
                }
            };
            let stream: BoxedStream = Box::new(ReunionStream::new(recv, send));
            let ctx = Context {
                sniffed_domain: None,
                source: None,
                inbound_tag: tag.into(),
                user: None,
                sniffed_protocol: None,
                vision_flow: false,
            };
            if let Err(e) = dispatcher.dispatch(ctx, dest, stream).await {
                warn!("TUIC v5 dispatch error: {e}");
            }
        });
    }
}

async fn accept_authentication(
    conn: Connection,
    users: Arc<HashMap<Uuid, String>>,
    auth_tx: watch::Sender<Option<Uuid>>,
    auth_timeout: Duration,
) -> Result<()> {
    let started = Instant::now();
    loop {
        let remaining = auth_timeout
            .checked_sub(started.elapsed())
            .unwrap_or(Duration::ZERO);
        if remaining.is_zero() {
            anyhow::bail!("authentication timed out");
        }
        let mut stream = timeout(remaining, conn.accept_uni())
            .await
            .context("accept TUIC authentication stream timed out")?
            .context("accept TUIC authentication stream")?;
        match read_authenticate_command(&conn, &users, &mut stream).await {
            Ok(uuid) => {
                let _ = auth_tx.send(Some(uuid));
                return Ok(());
            }
            Err(e) => {
                warn!("ignoring non-auth TUIC v5 unidirectional stream: {e}");
            }
        }
    }
}

async fn wait_authenticated(auth_rx: &mut watch::Receiver<Option<Uuid>>) -> Option<Uuid> {
    loop {
        if let Some(uuid) = *auth_rx.borrow() {
            return Some(uuid);
        }
        if auth_rx.changed().await.is_err() {
            return None;
        }
    }
}

async fn read_authenticate_command(
    conn: &Connection,
    users: &HashMap<Uuid, String>,
    recv: &mut RecvStream,
) -> Result<Uuid> {
    let mut header = [0u8; 2];
    recv.read_exact(&mut header).await?;
    if header != [TUIC_VERSION, CMD_AUTHENTICATE] {
        anyhow::bail!("expected authenticate command");
    }
    let mut body = [0u8; 48];
    recv.read_exact(&mut body).await?;
    let uuid = Uuid::from_slice(&body[..16]).context("invalid TUIC UUID")?;
    let Some(password) = users.get(&uuid) else {
        anyhow::bail!("unknown TUIC UUID {uuid}");
    };
    let expected = export_token(conn, uuid, password)
        .map_err(|e| anyhow::anyhow!("export TUIC token: {e:?}"))?;
    if body[16..] != expected {
        anyhow::bail!("bad TUIC token");
    }
    Ok(uuid)
}

/// A TUIC v5 outbound client.
pub struct TuicClient {
    config: TuicClientConfig,
    sessions: Vec<Mutex<Option<Arc<TuicClientSession>>>>,
    next_session: AtomicUsize,
}

struct TuicClientSession {
    conn: Connection,
    _endpoint: Endpoint,
}

impl TuicClient {
    pub fn new(config: TuicClientConfig) -> Self {
        let shard_count = config.endpoint_shards.clamp(1, MAX_CLIENT_SESSIONS);
        let sessions = (0..shard_count).map(|_| Mutex::new(None)).collect();
        Self {
            config,
            sessions,
            next_session: AtomicUsize::new(0),
        }
    }

    pub async fn connect_and_dial(&self, dest: &Address) -> Result<BoxedStream, ProxyError> {
        let shard = self.next_session.fetch_add(1, Ordering::Relaxed) % self.sessions.len();
        let session = self.session(shard).await?;
        let (mut send, recv) = timeout(OPEN_STREAM_TIMEOUT, session.conn.open_bi())
            .await
            .map_err(|_| ProxyError::Timeout)?
            .map_err(|e| ProxyError::Transport(format!("open TUIC stream: {e}")))?;
        write_connect_command(&mut send, dest).await?;
        Ok(Box::new(ReunionStream::new(recv, send)))
    }

    async fn session(&self, shard: usize) -> Result<Arc<TuicClientSession>, ProxyError> {
        let mut guard = self.sessions[shard].lock().await;
        if let Some(session) = guard.as_ref() {
            if session.conn.close_reason().is_none() {
                return Ok(Arc::clone(session));
            }
        }
        let endpoint = build_client_endpoint_with_alpn_and_socket(
            self.config.skip_cert_verify,
            &[b"h3".to_vec()],
            self.config.socket,
        )
        .map_err(|e| ProxyError::Transport(format!("TUIC client endpoint: {e}")))?;
        let conn = endpoint
            .connect(self.config.server, &self.config.server_name)
            .map_err(|e| ProxyError::Transport(format!("TUIC QUIC connect: {e}")))?
            .await
            .map_err(|e| ProxyError::Transport(format!("TUIC QUIC handshake: {e}")))?;
        write_authenticate_command(&conn, self.config.uuid, &self.config.password).await?;
        let session = Arc::new(TuicClientSession {
            conn,
            _endpoint: endpoint,
        });
        *guard = Some(Arc::clone(&session));
        Ok(session)
    }
}

/// Outbound handler that dials destinations through a TUIC v5 client.
pub struct TuicOutboundHandler {
    client: Arc<TuicClient>,
    tag: String,
}

impl TuicOutboundHandler {
    pub fn new(config: TuicClientConfig, tag: String) -> Arc<Self> {
        Arc::new(Self {
            client: Arc::new(TuicClient::new(config)),
            tag,
        })
    }
}

#[async_trait::async_trait]
impl OutboundHandler for TuicOutboundHandler {
    fn tag(&self) -> &str {
        &self.tag
    }

    async fn connect(&self, _ctx: &Context, dest: &Address) -> Result<BoxedStream, ProxyError> {
        self.client.connect_and_dial(dest).await
    }
}

async fn write_authenticate_command(
    conn: &Connection,
    uuid: Uuid,
    password: &str,
) -> Result<(), ProxyError> {
    let mut send = conn
        .open_uni()
        .await
        .map_err(|e| ProxyError::Transport(format!("open TUIC auth stream: {e}")))?;
    let token = export_token(conn, uuid, password)
        .map_err(|e| ProxyError::Transport(format!("export TUIC token: {e:?}")))?;
    send.write_all(&[TUIC_VERSION, CMD_AUTHENTICATE])
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))?;
    send.write_all(uuid.as_bytes())
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))?;
    send.write_all(&token)
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))?;
    send.finish()
        .map_err(|e| ProxyError::Transport(format!("finish TUIC auth stream: {e}")))?;
    Ok(())
}

fn export_token(
    conn: &Connection,
    uuid: Uuid,
    password: &str,
) -> Result<[u8; 32], quinn::crypto::ExportKeyingMaterialError> {
    let mut token = [0u8; 32];
    conn.export_keying_material(&mut token, uuid.as_bytes(), password.as_bytes())?;
    Ok(token)
}

async fn write_connect_command(send: &mut SendStream, dest: &Address) -> Result<(), ProxyError> {
    let mut buf = BytesMut::new();
    buf.put_u8(TUIC_VERSION);
    buf.put_u8(CMD_CONNECT);
    encode_address(&mut buf, Some(dest))?;
    send.write_all(&buf)
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))
}

async fn read_connect_command(recv: &mut RecvStream) -> Result<Address, ProxyError> {
    let mut header = [0u8; 2];
    recv.read_exact(&mut header)
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))?;
    validate_header(header, CMD_CONNECT)?;
    decode_address_from_stream(recv, false).await
}

fn validate_header(header: [u8; 2], expected_type: u8) -> Result<(), ProxyError> {
    if header[0] != TUIC_VERSION {
        return Err(ProxyError::Protocol(format!(
            "unsupported TUIC version 0x{:02x}",
            header[0]
        )));
    }
    if header[1] != expected_type {
        return Err(ProxyError::Protocol(format!(
            "unexpected TUIC command type 0x{:02x}",
            header[1]
        )));
    }
    Ok(())
}

fn encode_address(buf: &mut BytesMut, addr: Option<&Address>) -> Result<(), ProxyError> {
    match addr {
        None => buf.put_u8(ADDR_NONE),
        Some(Address::Domain(host, port)) => {
            let bytes = host.as_bytes();
            if bytes.len() > u8::MAX as usize {
                return Err(ProxyError::Protocol("TUIC domain too long".into()));
            }
            buf.put_u8(ADDR_DOMAIN);
            buf.put_u8(bytes.len() as u8);
            buf.extend_from_slice(bytes);
            buf.put_u16(*port);
        }
        Some(Address::Ipv4(ip, port)) => {
            buf.put_u8(ADDR_IPV4);
            buf.extend_from_slice(&ip.octets());
            buf.put_u16(*port);
        }
        Some(Address::Ipv6(ip, port)) => {
            buf.put_u8(ADDR_IPV6);
            buf.extend_from_slice(&ip.octets());
            buf.put_u16(*port);
        }
    }
    Ok(())
}

async fn decode_address_from_stream(
    recv: &mut RecvStream,
    allow_none: bool,
) -> Result<Address, ProxyError> {
    let mut typ = [0u8; 1];
    recv.read_exact(&mut typ)
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))?;
    match typ[0] {
        ADDR_NONE if allow_none => Err(ProxyError::Protocol("TUIC address none".into())),
        ADDR_DOMAIN => {
            let mut len = [0u8; 1];
            recv.read_exact(&mut len)
                .await
                .map_err(|e| ProxyError::Transport(e.to_string()))?;
            let mut host = vec![0u8; len[0] as usize];
            recv.read_exact(&mut host)
                .await
                .map_err(|e| ProxyError::Transport(e.to_string()))?;
            let port = read_u16(recv).await?;
            let host = String::from_utf8(host)
                .map_err(|e| ProxyError::Protocol(format!("bad TUIC domain: {e}")))?;
            Ok(Address::Domain(host, port))
        }
        ADDR_IPV4 => {
            let mut octets = [0u8; 4];
            recv.read_exact(&mut octets)
                .await
                .map_err(|e| ProxyError::Transport(e.to_string()))?;
            let port = read_u16(recv).await?;
            Ok(Address::Ipv4(Ipv4Addr::from(octets), port))
        }
        ADDR_IPV6 => {
            let mut octets = [0u8; 16];
            recv.read_exact(&mut octets)
                .await
                .map_err(|e| ProxyError::Transport(e.to_string()))?;
            let port = read_u16(recv).await?;
            Ok(Address::Ipv6(Ipv6Addr::from(octets), port))
        }
        other => Err(ProxyError::Protocol(format!(
            "unsupported TUIC address type 0x{other:02x}"
        ))),
    }
}

async fn read_u16(recv: &mut RecvStream) -> Result<u16, ProxyError> {
    let mut raw = [0u8; 2];
    recv.read_exact(&mut raw)
        .await
        .map_err(|e| ProxyError::Transport(e.to_string()))?;
    Ok(u16::from_be_bytes(raw))
}

#[derive(Debug, Clone)]
pub struct TuicUdpPacket {
    pub assoc_id: u16,
    pub pkt_id: u16,
    pub addr: Address,
    pub data: Bytes,
}

pub fn encode_udp_packet(packet: &TuicUdpPacket) -> Result<Bytes, ProxyError> {
    let mut buf = BytesMut::new();
    buf.put_u8(TUIC_VERSION);
    buf.put_u8(CMD_PACKET);
    buf.put_u16(packet.assoc_id);
    buf.put_u16(packet.pkt_id);
    buf.put_u8(1); // one fragment
    buf.put_u8(0); // first fragment
    if packet.data.len() > u16::MAX as usize {
        return Err(ProxyError::Protocol("TUIC UDP packet too large".into()));
    }
    buf.put_u16(packet.data.len() as u16);
    encode_address(&mut buf, Some(&packet.addr))?;
    buf.extend_from_slice(&packet.data);
    Ok(buf.freeze())
}

pub fn decode_udp_packet(mut raw: Bytes) -> Result<TuicUdpPacket, ProxyError> {
    if raw.len() < 10 {
        return Err(ProxyError::Protocol("TUIC UDP packet too short".into()));
    }
    let version = raw.get_u8();
    let command = raw.get_u8();
    if version != TUIC_VERSION || command != CMD_PACKET {
        return Err(ProxyError::Protocol("not a TUIC packet command".into()));
    }
    let assoc_id = raw.get_u16();
    let pkt_id = raw.get_u16();
    let frag_total = raw.get_u8();
    let frag_id = raw.get_u8();
    let size = raw.get_u16() as usize;
    if frag_total != 1 || frag_id != 0 {
        return Err(ProxyError::Protocol(
            "fragmented TUIC UDP packets are not supported yet".into(),
        ));
    }
    let addr = decode_address_from_buf(&mut raw, false)?;
    if raw.len() < size {
        return Err(ProxyError::Protocol("TUIC UDP payload truncated".into()));
    }
    let data = raw.copy_to_bytes(size);
    Ok(TuicUdpPacket {
        assoc_id,
        pkt_id,
        addr,
        data,
    })
}

fn decode_address_from_buf(raw: &mut Bytes, allow_none: bool) -> Result<Address, ProxyError> {
    if !raw.has_remaining() {
        return Err(ProxyError::Protocol("TUIC address missing".into()));
    }
    match raw.get_u8() {
        ADDR_NONE if allow_none => Err(ProxyError::Protocol("TUIC address none".into())),
        ADDR_DOMAIN => {
            if raw.remaining() < 1 {
                return Err(ProxyError::Protocol("TUIC domain length missing".into()));
            }
            let len = raw.get_u8() as usize;
            if raw.remaining() < len + 2 {
                return Err(ProxyError::Protocol("TUIC domain address truncated".into()));
            }
            let host = raw.copy_to_bytes(len);
            let port = raw.get_u16();
            let host = String::from_utf8(host.to_vec())
                .map_err(|e| ProxyError::Protocol(format!("bad TUIC domain: {e}")))?;
            Ok(Address::Domain(host, port))
        }
        ADDR_IPV4 => {
            if raw.remaining() < 6 {
                return Err(ProxyError::Protocol("TUIC IPv4 address truncated".into()));
            }
            let mut octets = [0u8; 4];
            raw.copy_to_slice(&mut octets);
            let port = raw.get_u16();
            Ok(Address::Ipv4(Ipv4Addr::from(octets), port))
        }
        ADDR_IPV6 => {
            if raw.remaining() < 18 {
                return Err(ProxyError::Protocol("TUIC IPv6 address truncated".into()));
            }
            let mut octets = [0u8; 16];
            raw.copy_to_slice(&mut octets);
            let port = raw.get_u16();
            Ok(Address::Ipv6(Ipv6Addr::from(octets), port))
        }
        other => Err(ProxyError::Protocol(format!(
            "unsupported TUIC address type 0x{other:02x}"
        ))),
    }
}

struct UdpSession {
    socket: Arc<UdpSocket>,
    last_used: Instant,
}

async fn serve_udp_datagrams(conn: Connection, mut auth_rx: watch::Receiver<Option<Uuid>>) {
    if wait_authenticated(&mut auth_rx).await.is_none() {
        return;
    }
    let sessions: Arc<DashMap<u16, UdpSession>> = Arc::new(DashMap::new());
    let workers = Arc::new(Semaphore::new(MAX_UDP_WORKERS_PER_CONN));
    loop {
        let raw = match conn.read_datagram().await {
            Ok(raw) => raw,
            Err(_) => break,
        };
        let packet = match decode_udp_packet(raw) {
            Ok(packet) => packet,
            Err(e) => {
                warn!("TUIC UDP decode failed: {e}");
                continue;
            }
        };
        if sessions.len() > MAX_UDP_SESSIONS_PER_CONN {
            let cutoff = Instant::now() - UDP_SESSION_IDLE;
            sessions.retain(|_, session| session.last_used > cutoff);
        }
        let Ok(permit) = Arc::clone(&workers).try_acquire_owned() else {
            warn!("TUIC UDP worker limit reached; dropping datagram");
            continue;
        };
        let conn = conn.clone();
        let sessions = Arc::clone(&sessions);
        tokio::spawn(async move {
            let _permit = permit;
            handle_udp_packet(conn, sessions, packet).await;
        });
    }
}

async fn handle_udp_packet(
    conn: Connection,
    sessions: Arc<DashMap<u16, UdpSession>>,
    packet: TuicUdpPacket,
) {
    let dest = match resolve_address(&packet.addr).await {
        Ok(dest) => dest,
        Err(e) => {
            warn!("TUIC UDP destination resolve failed: {e}");
            return;
        }
    };
    let socket = if let Some(mut session) = sessions.get_mut(&packet.assoc_id) {
        session.last_used = Instant::now();
        Arc::clone(&session.socket)
    } else {
        match UdpSocket::bind("0.0.0.0:0").await {
            Ok(socket) => {
                let socket = Arc::new(socket);
                sessions.insert(
                    packet.assoc_id,
                    UdpSession {
                        socket: Arc::clone(&socket),
                        last_used: Instant::now(),
                    },
                );
                socket
            }
            Err(e) => {
                warn!("TUIC UDP socket bind failed: {e}");
                return;
            }
        }
    };
    if let Err(e) = socket.send_to(&packet.data, dest).await {
        warn!("TUIC UDP send failed: {e}");
        return;
    }
    let mut buf = vec![0u8; MAX_PACKET_SIZE];
    match timeout(UDP_REPLY_TIMEOUT, socket.recv_from(&mut buf)).await {
        Ok(Ok((n, src))) => {
            let reply = TuicUdpPacket {
                assoc_id: packet.assoc_id,
                pkt_id: packet.pkt_id,
                addr: Address::from(src),
                data: Bytes::copy_from_slice(&buf[..n]),
            };
            match encode_udp_packet(&reply) {
                Ok(raw) => {
                    if let Err(e) = conn.send_datagram(raw) {
                        warn!("TUIC UDP reply send_datagram failed: {e}");
                    }
                }
                Err(e) => warn!("TUIC UDP reply encode failed: {e}"),
            }
        }
        Ok(Err(e)) => warn!("TUIC UDP recv failed: {e}"),
        Err(_) => warn!("TUIC UDP recv timed out"),
    }
}

async fn resolve_address(addr: &Address) -> Result<SocketAddr, ProxyError> {
    match addr {
        Address::Ipv4(ip, port) => Ok(SocketAddr::new(IpAddr::V4(*ip), *port)),
        Address::Ipv6(ip, port) => Ok(SocketAddr::new(IpAddr::V6(*ip), *port)),
        Address::Domain(host, port) => tokio::net::lookup_host((host.as_str(), *port))
            .await
            .map_err(|e| ProxyError::Transport(e.to_string()))?
            .next()
            .ok_or_else(|| ProxyError::DnsResolutionFailed(host.clone())),
    }
}

/// A client-side TUIC v5 native UDP session.
pub struct TuicUdpSession {
    conn: Connection,
    _endpoint: Endpoint,
    assoc_id: u16,
    next_packet_id: AtomicU16,
}

impl TuicUdpSession {
    pub async fn connect(config: &TuicClientConfig) -> Result<Self, ProxyError> {
        let endpoint = build_client_endpoint_with_alpn_and_socket(
            config.skip_cert_verify,
            &[b"h3".to_vec()],
            config.socket,
        )
        .map_err(|e| ProxyError::Transport(format!("TUIC client endpoint: {e}")))?;
        let conn = endpoint
            .connect(config.server, &config.server_name)
            .map_err(|e| ProxyError::Transport(format!("TUIC QUIC connect: {e}")))?
            .await
            .map_err(|e| ProxyError::Transport(format!("TUIC QUIC handshake: {e}")))?;
        write_authenticate_command(&conn, config.uuid, &config.password).await?;
        Ok(Self {
            conn,
            _endpoint: endpoint,
            assoc_id: rand::random(),
            next_packet_id: AtomicU16::new(1),
        })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub async fn send(&self, dest: Address, data: &[u8]) -> Result<(), ProxyError> {
        let packet = TuicUdpPacket {
            assoc_id: self.assoc_id,
            pkt_id: self.next_packet_id.fetch_add(1, Ordering::Relaxed),
            addr: dest,
            data: Bytes::copy_from_slice(data),
        };
        let raw = encode_udp_packet(&packet)?;
        self.conn
            .send_datagram(raw)
            .map_err(|e| ProxyError::Transport(format!("TUIC send_datagram: {e}")))
    }

    pub async fn recv(&self) -> Result<TuicUdpPacket, ProxyError> {
        loop {
            let raw = self
                .conn
                .read_datagram()
                .await
                .map_err(|e| ProxyError::Transport(format!("TUIC read_datagram: {e}")))?;
            let packet = decode_udp_packet(raw)?;
            if packet.assoc_id == self.assoc_id {
                return Ok(packet);
            }
        }
    }
}

pub fn encode_dissociate(assoc_id: u16) -> Bytes {
    let mut buf = BytesMut::new();
    buf.put_u8(TUIC_VERSION);
    buf.put_u8(CMD_DISSOCIATE);
    buf.put_u16(assoc_id);
    buf.freeze()
}

pub fn encode_heartbeat() -> Bytes {
    Bytes::from_static(&[TUIC_VERSION, CMD_HEARTBEAT])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_packet_roundtrips() {
        let packet = TuicUdpPacket {
            assoc_id: 7,
            pkt_id: 9,
            addr: Address::Domain("example.com".into(), 443),
            data: Bytes::from_static(b"hello"),
        };
        let encoded = encode_udp_packet(&packet).unwrap();
        let decoded = decode_udp_packet(encoded).unwrap();
        assert_eq!(decoded.assoc_id, 7);
        assert_eq!(decoded.pkt_id, 9);
        assert_eq!(decoded.addr, Address::Domain("example.com".into(), 443));
        assert_eq!(&decoded.data[..], b"hello");
    }
}
