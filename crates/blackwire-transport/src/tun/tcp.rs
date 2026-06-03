use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use super::nat::TunTx;
use super::packet::{build_tcp_packet, build_tcp_packet_with_options, IpPacket, TransportProtocol};

const TCP_FIN: u8 = 0x01;
const TCP_SYN: u8 = 0x02;
const TCP_RST: u8 = 0x04;
const TCP_PSH: u8 = 0x08;
const TCP_ACK: u8 = 0x10;

const FLOW_CHAN_CAP: usize = 256;
const DEFAULT_MAX_TCP_ENTRIES: usize = 4096;
const SERVER_ISN: u32 = 0x4257_0001;
const TCP_SEND_WINDOW_CAP: u32 = 4 * 1024 * 1024;
const TCP_WINDOW_SCALE: u8 = 7;

struct TcpEntry {
    tx: mpsc::Sender<Vec<u8>>,
    _cancel: oneshot::Sender<()>,
    last_seen: Instant,
    server_seq: Arc<AtomicU32>,
    client_ack_seen: Arc<AtomicU32>,
    client_window_seen: Arc<AtomicU32>,
    client_next_seq: u32,
    client_window_scale: u8,
}

/// Minimal packet-level TCP bridge used by Windows Wintun.
///
/// Linux and macOS use OS redirection (iptables/PF) before packets reach this
/// runtime. Windows does not have a native redirect rule available here, so it
/// terminates TCP from the TUN side and opens a SOCKS5 CONNECT to the local
/// proxy listener configured by `redirect_port`.
pub struct TcpBridgeTable {
    entries: HashMap<(SocketAddr, SocketAddr), TcpEntry>,
    redirect_port: u16,
    idle_timeout: Duration,
    max_entries: usize,
}

impl TcpBridgeTable {
    pub fn with_defaults(redirect_port: u16, idle_timeout: Duration) -> Self {
        Self::new(redirect_port, idle_timeout, DEFAULT_MAX_TCP_ENTRIES)
    }

    pub fn new(redirect_port: u16, idle_timeout: Duration, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            redirect_port,
            idle_timeout,
            max_entries: max_entries.max(1),
        }
    }

    pub async fn forward(&mut self, packet: &IpPacket, raw: &[u8], tun_tx: &TunTx) -> Result<()> {
        if packet.protocol != TransportProtocol::Tcp {
            return Ok(());
        }

        let tcp =
            TcpHeader::from_packet(packet, raw).context("TCP bridge: missing TCP metadata")?;
        let client = SocketAddr::new(packet.src, packet.src_port);
        let remote = SocketAddr::new(packet.dst, packet.dst_port);
        let key = (client, remote);
        let now = Instant::now();

        if tcp.flags & TCP_RST != 0 {
            self.entries.remove(&key);
            return Ok(());
        }

        if tcp.flags & TCP_SYN != 0 && !self.entries.contains_key(&key) {
            self.evict_idle();
            if self.entries.len() >= self.max_entries {
                self.evict_oldest();
            }
            if self.entries.len() >= self.max_entries {
                anyhow::bail!("TCP bridge: flow table full ({})", self.max_entries);
            }

            let client_next_seq = tcp.seq.wrapping_add(1);
            let server_seq = SERVER_ISN ^ u32::from(client.port());
            let server_next_seq = Arc::new(AtomicU32::new(server_seq.wrapping_add(1)));
            let client_ack_seen = Arc::new(AtomicU32::new(server_seq.wrapping_add(1)));
            let client_window_scale = tcp.window_scale.unwrap_or(0);
            let client_window_seen = Arc::new(AtomicU32::new(
                scaled_window(tcp.window, client_window_scale).max(1),
            ));
            let (payload_tx, payload_rx) = mpsc::channel(FLOW_CHAN_CAP);
            let (cancel_tx, cancel_rx) = oneshot::channel();

            tokio::spawn(flow_task(FlowTask {
                client,
                remote,
                server_seq: Arc::clone(&server_next_seq),
                client_ack_seen: Arc::clone(&client_ack_seen),
                client_window_seen: Arc::clone(&client_window_seen),
                client_ack: client_next_seq,
                redirect_port: self.redirect_port,
                tun_tx: tun_tx.clone(),
                payload_rx,
                cancel_rx,
            }));

            self.entries.insert(
                key,
                TcpEntry {
                    tx: payload_tx,
                    _cancel: cancel_tx,
                    last_seen: now,
                    server_seq: server_next_seq,
                    client_ack_seen,
                    client_window_seen,
                    client_next_seq,
                    client_window_scale,
                },
            );

            send_control(
                tun_tx.clone(),
                remote,
                client,
                server_seq,
                client_next_seq,
                TCP_SYN | TCP_ACK,
                65535,
                &[0x01, 0x03, 0x03, TCP_WINDOW_SCALE],
            )
            .await;
            return Ok(());
        }

        let Some(entry) = self.entries.get_mut(&key) else {
            return Ok(());
        };
        entry.last_seen = now;
        if tcp.flags & TCP_ACK != 0 {
            entry.client_ack_seen.store(tcp.ack, Ordering::Relaxed);
            entry.client_window_seen.store(
                scaled_window(tcp.window, entry.client_window_scale).max(1),
                Ordering::Relaxed,
            );
        }

        if tcp.flags & TCP_FIN != 0 {
            entry.client_next_seq = tcp.seq.wrapping_add(1);
            send_control(
                tun_tx.clone(),
                remote,
                client,
                entry.server_seq.load(Ordering::Relaxed),
                entry.client_next_seq,
                TCP_ACK,
                65535,
                &[],
            )
            .await;
            self.entries.remove(&key);
            return Ok(());
        }

        let Some(payload) = packet.payload(raw) else {
            return Ok(());
        };
        if payload.is_empty() {
            return Ok(());
        }

        entry.client_next_seq = tcp.seq.wrapping_add(payload.len() as u32);
        send_control(
            tun_tx.clone(),
            remote,
            client,
            entry.server_seq.load(Ordering::Relaxed),
            entry.client_next_seq,
            TCP_ACK,
            65535,
            &[],
        )
        .await;

        if entry.tx.send(payload.to_vec()).await.is_err() {
            self.entries.remove(&key);
        }

        Ok(())
    }

    pub fn evict_idle(&mut self) -> usize {
        let now = Instant::now();
        let timeout = self.idle_timeout;
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| now.duration_since(entry.last_seen) <= timeout);
        before - self.entries.len()
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_seen)
            .map(|(key, _)| *key)
        {
            self.entries.remove(&oldest_key);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_control(
    tun_tx: TunTx,
    src: SocketAddr,
    dst: SocketAddr,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    options: &[u8],
) {
    if let Some(packet) =
        build_tcp_packet_with_options(src, dst, seq, ack, flags, window, options, &[])
    {
        let _ = tun_tx.send(packet).await;
    }
}

struct TcpHeader {
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    window_scale: Option<u8>,
}

impl TcpHeader {
    fn from_packet(packet: &IpPacket, raw: &[u8]) -> Option<Self> {
        let tcp = packet.transport_offset;
        let header_len = packet.payload_offset.checked_sub(packet.transport_offset)?;
        let window = u16::from_be_bytes([*raw.get(tcp + 14)?, *raw.get(tcp + 15)?]);
        Some(Self {
            seq: packet.tcp_seq?,
            ack: packet.tcp_ack?,
            flags: packet.tcp_flags?,
            window,
            window_scale: if packet.tcp_flags? & TCP_SYN != 0 {
                parse_window_scale(raw.get(tcp + 20..tcp + header_len)?)
            } else {
                None
            },
        })
    }
}

struct FlowTask {
    client: SocketAddr,
    remote: SocketAddr,
    server_seq: Arc<AtomicU32>,
    client_ack_seen: Arc<AtomicU32>,
    client_window_seen: Arc<AtomicU32>,
    client_ack: u32,
    redirect_port: u16,
    tun_tx: TunTx,
    payload_rx: mpsc::Receiver<Vec<u8>>,
    cancel_rx: oneshot::Receiver<()>,
}

async fn flow_task(task: FlowTask) {
    let FlowTask {
        client,
        remote,
        server_seq,
        client_ack_seen,
        client_window_seen,
        mut client_ack,
        redirect_port,
        tun_tx,
        mut payload_rx,
        mut cancel_rx,
    } = task;
    let result = async {
        let mut stream = connect_local_socks(remote, redirect_port).await?;
        let mut buf = vec![0u8; 16 * 1024];

        loop {
            tokio::select! {
                Some(payload) = payload_rx.recv() => {
                    client_ack = client_ack.wrapping_add(payload.len() as u32);
                    stream.write_all(&payload).await.context("TCP bridge: write local SOCKS stream")?;
                }
                read = stream.read(&mut buf) => {
                    let n = read.context("TCP bridge: read local SOCKS stream")?;
                    let seq = server_seq.load(Ordering::Relaxed);
                    if n == 0 {
                        send_tcp(&tun_tx, remote, client, seq, client_ack, TCP_FIN | TCP_ACK, &[]).await;
                        server_seq.store(seq.wrapping_add(1), Ordering::Relaxed);
                        break;
                    }
                    wait_for_client_window(&server_seq, &client_ack_seen, &client_window_seen, n as u32).await;
                    send_tcp(&tun_tx, remote, client, seq, client_ack, TCP_PSH | TCP_ACK, &buf[..n]).await;
                    server_seq.store(seq.wrapping_add(n as u32), Ordering::Relaxed);
                }
                _ = &mut cancel_rx => break,
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(e) = result {
        debug!(%e, %client, %remote, "TCP bridge flow ended with error");
        send_tcp(
            &tun_tx,
            remote,
            client,
            server_seq.load(Ordering::Relaxed),
            client_ack,
            TCP_RST | TCP_ACK,
            &[],
        )
        .await;
    }
}

async fn wait_for_client_window(
    server_seq: &AtomicU32,
    client_ack_seen: &AtomicU32,
    client_window_seen: &AtomicU32,
    payload_len: u32,
) {
    loop {
        let seq = server_seq.load(Ordering::Relaxed);
        let ack = client_ack_seen.load(Ordering::Relaxed);
        let in_flight = seq.wrapping_sub(ack);
        let recv_window = client_window_seen
            .load(Ordering::Relaxed)
            .min(TCP_SEND_WINDOW_CAP);
        if in_flight.saturating_add(payload_len) <= recv_window {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

fn scaled_window(window: u16, scale: u8) -> u32 {
    (window as u32) << scale.min(14)
}

fn parse_window_scale(options: &[u8]) -> Option<u8> {
    let mut i = 0usize;
    while i < options.len() {
        match options[i] {
            0 => break,
            1 => i += 1,
            kind => {
                let len = *options.get(i + 1)? as usize;
                if len < 2 || i + len > options.len() {
                    break;
                }
                if kind == 3 && len == 3 {
                    return options.get(i + 2).copied();
                }
                i += len;
            }
        }
    }
    None
}

async fn connect_local_socks(remote: SocketAddr, redirect_port: u16) -> Result<TcpStream> {
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), redirect_port);
    let mut stream = TcpStream::connect(local)
        .await
        .with_context(|| format!("TCP bridge: connect local SOCKS listener at {local}"))?;

    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .context("TCP bridge: send SOCKS greeting")?;
    let mut greeting = [0u8; 2];
    stream
        .read_exact(&mut greeting)
        .await
        .context("TCP bridge: read SOCKS greeting")?;
    if greeting != [0x05, 0x00] {
        anyhow::bail!("TCP bridge: SOCKS listener rejected no-auth greeting");
    }

    let mut request = Vec::with_capacity(22);
    request.extend_from_slice(&[0x05, 0x01, 0x00]);
    match remote {
        SocketAddr::V4(addr) => {
            request.push(0x01);
            request.extend_from_slice(&addr.ip().octets());
            request.extend_from_slice(&addr.port().to_be_bytes());
        }
        SocketAddr::V6(addr) => {
            request.push(0x04);
            request.extend_from_slice(&addr.ip().octets());
            request.extend_from_slice(&addr.port().to_be_bytes());
        }
    }
    stream
        .write_all(&request)
        .await
        .context("TCP bridge: send SOCKS CONNECT")?;
    read_socks_reply(&mut stream)
        .await
        .context("TCP bridge: read SOCKS CONNECT reply")?;
    Ok(stream)
}

async fn read_socks_reply(stream: &mut TcpStream) -> Result<()> {
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    if head[0] != 0x05 || head[1] != 0x00 {
        anyhow::bail!("SOCKS CONNECT failed with status {}", head[1]);
    }

    let addr_len = match head[3] {
        0x01 => 4,
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            len[0] as usize
        }
        0x04 => 16,
        atyp => anyhow::bail!("SOCKS CONNECT reply used unsupported ATYP {atyp}"),
    };
    let mut discard = vec![0u8; addr_len + 2];
    stream.read_exact(&mut discard).await?;
    Ok(())
}

async fn send_tcp(
    tun_tx: &TunTx,
    src: SocketAddr,
    dst: SocketAddr,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    if let Some(packet) = build_tcp_packet(src, dst, seq, ack, flags, payload) {
        let _ = tun_tx.send(packet).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tun::packet::parse_ip_packet;

    #[test]
    fn tcp_header_reads_sequence_and_flags_from_ip_packet() {
        let src: SocketAddr = "10.0.0.2:50000".parse().unwrap();
        let dst: SocketAddr = "93.184.216.34:443".parse().unwrap();
        let raw = build_tcp_packet(src, dst, 7, 0, TCP_SYN, &[]).unwrap();
        let packet = parse_ip_packet(&raw).unwrap();
        let tcp = TcpHeader::from_packet(&packet, &raw).unwrap();

        assert_eq!(tcp.seq, 7);
        assert_eq!(tcp.flags, TCP_SYN);
    }
}
