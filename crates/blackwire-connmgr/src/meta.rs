use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// Application-level protocol carried by a managed connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// Raw TCP proxy.
    Tcp,
    /// UDP proxy.
    Udp,
    /// HTTP/HTTPS proxy.
    Http,
    /// TLS passthrough.
    Tls,
    /// SOCKS4/SOCKS5 proxy.
    Socks,
    /// VLESS protocol.
    Vless,
    /// VMess protocol.
    Vmess,
    /// Trojan protocol.
    Trojan,
    /// Shadowsocks (including SS2022) protocol.
    Shadowsocks,
    /// Hysteria2 protocol.
    Hysteria2,
    /// Unrecognized or not yet determined protocol.
    Unknown,
}

impl Protocol {
    /// Returns a lowercase ASCII label for use in metrics labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
            Protocol::Http => "http",
            Protocol::Tls => "tls",
            Protocol::Socks => "socks",
            Protocol::Vless => "vless",
            Protocol::Vmess => "vmess",
            Protocol::Trojan => "trojan",
            Protocol::Shadowsocks => "shadowsocks",
            Protocol::Hysteria2 => "hysteria2",
            Protocol::Unknown => "unknown",
        }
    }
}

impl From<&str> for Protocol {
    fn from(value: &str) -> Self {
        match value {
            "http" => Protocol::Http,
            "tls" => Protocol::Tls,
            "socks" => Protocol::Socks,
            "vless" => Protocol::Vless,
            "vmess" => Protocol::Vmess,
            "trojan" => Protocol::Trojan,
            "shadowsocks" | "ss2022" => Protocol::Shadowsocks,
            "hysteria2" => Protocol::Hysteria2,
            "udp" => Protocol::Udp,
            "tcp" => Protocol::Tcp,
            _ => Protocol::Unknown,
        }
    }
}

/// Network transport layer used by a managed connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Plain TCP.
    Tcp,
    /// Plain UDP.
    Udp,
    /// TLS over TCP.
    Tls,
    /// WebSocket framing.
    WebSocket,
    /// gRPC over HTTP/2.
    Grpc,
    /// QUIC.
    Quic,
    /// mKCP (KCP over UDP).
    Kcp,
    /// SplitHTTP (chunked HTTP upload/download).
    SplitHttp,
    /// Unrecognized or not yet determined transport.
    Unknown,
}

impl Transport {
    /// Returns a lowercase ASCII label for use in metrics labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Transport::Tcp => "tcp",
            Transport::Udp => "udp",
            Transport::Tls => "tls",
            Transport::WebSocket => "websocket",
            Transport::Grpc => "grpc",
            Transport::Quic => "quic",
            Transport::Kcp => "kcp",
            Transport::SplitHttp => "splithttp",
            Transport::Unknown => "unknown",
        }
    }
}

/// Relay implementation used to transfer bytes for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayPath {
    /// Standard async copy relay.
    Copy,
    /// Ring-buffered async copy relay (v2).
    CopyV2,
    /// Linux splice(2) zero-copy relay.
    Splice,
    /// Async copy with XTLS Vision unwrapping.
    VisionCopy,
    /// Adaptive relay that switches strategy based on flow characteristics.
    Adaptive,
    /// Unrecognized or not yet determined relay path.
    Unknown,
}

impl RelayPath {
    /// Returns a lowercase ASCII label for use in metrics labels.
    pub fn as_str(self) -> &'static str {
        match self {
            RelayPath::Copy => "copy",
            RelayPath::CopyV2 => "copy_v2",
            RelayPath::Splice => "splice",
            RelayPath::VisionCopy => "vision_copy",
            RelayPath::Adaptive => "adaptive",
            RelayPath::Unknown => "unknown",
        }
    }
}

/// Reason a managed connection was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// Connection is still active (placeholder / sentinel).
    Active,
    /// Both sides shut down cleanly.
    Completed,
    /// Closed due to an I/O or protocol error.
    Error,
    /// Closed by explicit connection ID via the management API.
    ClosedById,
    /// Closed by the authenticated user.
    ClosedByUser,
    /// Inbound side hung up first.
    ClosedByInbound,
    /// Outbound side hung up first.
    ClosedByOutbound,
    /// Dropped before relay started (e.g. rejected by policy).
    Dropped,
}

impl CloseReason {
    /// Returns a lowercase ASCII label for use in metrics labels.
    pub fn as_str(self) -> &'static str {
        match self {
            CloseReason::Active => "active",
            CloseReason::Completed => "completed",
            CloseReason::Error => "error",
            CloseReason::ClosedById => "closed_by_id",
            CloseReason::ClosedByUser => "closed_by_user",
            CloseReason::ClosedByInbound => "closed_by_inbound",
            CloseReason::ClosedByOutbound => "closed_by_outbound",
            CloseReason::Dropped => "dropped",
        }
    }

    pub(crate) fn to_u8(self) -> u8 {
        match self {
            CloseReason::Active => 0,
            CloseReason::Completed => 1,
            CloseReason::Error => 2,
            CloseReason::ClosedById => 3,
            CloseReason::ClosedByUser => 4,
            CloseReason::ClosedByInbound => 5,
            CloseReason::ClosedByOutbound => 6,
            CloseReason::Dropped => 7,
        }
    }

    pub(crate) fn from_u8(value: u8) -> Self {
        match value {
            1 => CloseReason::Completed,
            2 => CloseReason::Error,
            3 => CloseReason::ClosedById,
            4 => CloseReason::ClosedByUser,
            5 => CloseReason::ClosedByInbound,
            6 => CloseReason::ClosedByOutbound,
            7 => CloseReason::Dropped,
            _ => CloseReason::Active,
        }
    }
}

/// Shared live state for a managed connection, held in the connection table.
pub struct ConnectionMeta {
    /// Unique connection identifier.
    pub id: u64,
    /// Name of the inbound that accepted this connection.
    pub inbound: Arc<str>,
    /// Name of the outbound that is serving this connection.
    pub outbound: Arc<str>,
    /// Authenticated user, if known.
    pub user: Option<Arc<str>>,
    /// Application-level protocol.
    pub protocol: Protocol,
    /// Network transport layer.
    pub transport: Transport,
    /// Wall-clock time when the connection was opened.
    pub started_at: Instant,
    /// Bytes sent from client to server (upload).
    pub bytes_up: AtomicU64,
    /// Bytes sent from server to client (download).
    pub bytes_down: AtomicU64,
    /// Relay implementation chosen for this connection.
    pub relay_path: RelayPath,
    /// Packed close reason (see `CloseReason::to_u8`).
    pub close_reason: AtomicU8,
    /// Cancel signal; dropping or cancelling this token tears down the connection.
    pub cancellation: CancellationToken,
}

impl ConnectionMeta {
    /// Reads the current close reason from the atomic field.
    pub fn close_reason(&self) -> CloseReason {
        CloseReason::from_u8(self.close_reason.load(Ordering::Relaxed))
    }

    /// Atomically stores a new close reason.
    pub fn set_close_reason(&self, reason: CloseReason) {
        self.close_reason.store(reason.to_u8(), Ordering::Relaxed);
    }

    /// Returns a point-in-time snapshot of this connection's counters and metadata.
    pub fn snapshot(&self) -> ConnectionSnapshot {
        ConnectionSnapshot {
            id: self.id,
            inbound: self.inbound.to_string(),
            outbound: self.outbound.to_string(),
            user: self.user.as_ref().map(ToString::to_string),
            protocol: self.protocol,
            transport: self.transport,
            age_secs: self.started_at.elapsed().as_secs_f64(),
            bytes_up: self.bytes_up.load(Ordering::Relaxed),
            bytes_down: self.bytes_down.load(Ordering::Relaxed),
            relay_path: self.relay_path,
            close_reason: self.close_reason(),
        }
    }
}

/// Serializable point-in-time snapshot of a connection's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSnapshot {
    /// Unique connection identifier.
    pub id: u64,
    /// Name of the inbound that accepted this connection.
    pub inbound: String,
    /// Name of the outbound serving this connection.
    pub outbound: String,
    /// Authenticated user, if known.
    pub user: Option<String>,
    /// Application-level protocol.
    pub protocol: Protocol,
    /// Network transport layer.
    pub transport: Transport,
    /// Seconds elapsed since the connection was opened.
    pub age_secs: f64,
    /// Bytes uploaded (client → server) at snapshot time.
    pub bytes_up: u64,
    /// Bytes downloaded (server → client) at snapshot time.
    pub bytes_down: u64,
    /// Relay implementation in use.
    pub relay_path: RelayPath,
    /// Reason the connection was closed (or `Active` if still open).
    pub close_reason: CloseReason,
}

impl ConnectionSnapshot {
    /// Returns the sum of uploaded and downloaded bytes.
    pub fn total_bytes(&self) -> u64 {
        self.bytes_up.saturating_add(self.bytes_down)
    }
}
