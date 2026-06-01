use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
    Http,
    Tls,
    Socks,
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Unknown,
}

impl Protocol {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Tcp,
    Udp,
    Tls,
    WebSocket,
    Grpc,
    Quic,
    Kcp,
    SplitHttp,
    Unknown,
}

impl Transport {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayPath {
    Copy,
    CopyV2,
    Splice,
    VisionCopy,
    Adaptive,
    Unknown,
}

impl RelayPath {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    Active,
    Completed,
    Error,
    ClosedById,
    ClosedByUser,
    ClosedByInbound,
    ClosedByOutbound,
    Dropped,
}

impl CloseReason {
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

pub struct ConnectionMeta {
    pub id: u64,
    pub inbound: Arc<str>,
    pub outbound: Arc<str>,
    pub user: Option<Arc<str>>,
    pub protocol: Protocol,
    pub transport: Transport,
    pub started_at: Instant,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
    pub relay_path: RelayPath,
    pub close_reason: AtomicU8,
    pub cancellation: CancellationToken,
}

impl ConnectionMeta {
    pub fn close_reason(&self) -> CloseReason {
        CloseReason::from_u8(self.close_reason.load(Ordering::Relaxed))
    }

    pub fn set_close_reason(&self, reason: CloseReason) {
        self.close_reason.store(reason.to_u8(), Ordering::Relaxed);
    }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSnapshot {
    pub id: u64,
    pub inbound: String,
    pub outbound: String,
    pub user: Option<String>,
    pub protocol: Protocol,
    pub transport: Transport,
    pub age_secs: f64,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub relay_path: RelayPath,
    pub close_reason: CloseReason,
}

impl ConnectionSnapshot {
    pub fn total_bytes(&self) -> u64 {
        self.bytes_up.saturating_add(self.bytes_down)
    }
}
