use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use super::packet::{IpPacket, TransportProtocol};

/// Unique key for one client-to-remote transport flow.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowKey {
    /// Client endpoint (source side of the forward packet).
    pub client: SocketAddr,
    /// Remote endpoint (destination side of the forward packet).
    pub remote: SocketAddr,
    /// Flow protocol (TCP or UDP).
    pub protocol: TransportProtocol,
}

impl FlowKey {
    /// Build a flow key from a parsed packet.
    ///
    /// Returns `None` for protocols we do not track.
    pub fn from_packet(packet: &IpPacket) -> Option<Self> {
        match packet.protocol {
            TransportProtocol::Tcp | TransportProtocol::Udp => Some(Self {
                client: SocketAddr::new(packet.src, packet.src_port),
                remote: SocketAddr::new(packet.dst, packet.dst_port),
                protocol: packet.protocol,
            }),
            TransportProtocol::Other(_) => None,
        }
    }

    /// Returns `true` if `packet` matches this flow in reverse direction.
    pub fn matches_response(&self, packet: &IpPacket) -> bool {
        packet.protocol == self.protocol
            && packet.src == self.remote.ip()
            && packet.src_port == self.remote.port()
            && packet.dst == self.client.ip()
            && packet.dst_port == self.client.port()
    }
}

/// Runtime data for one observed flow.
#[derive(Debug, Clone)]
pub struct TunSession {
    /// 5-tuple-like key for this flow.
    pub flow: FlowKey,
    /// Last time we saw traffic for this flow.
    pub last_seen: Instant,
}

/// In-memory table of active TUN sessions.
#[derive(Debug, Default)]
pub struct TunSessionTable {
    sessions: HashMap<FlowKey, TunSession>,
    max_sessions: usize,
}

impl TunSessionTable {
    /// Create an empty session table.
    pub fn new() -> Self {
        Self::with_max_sessions(4096)
    }

    /// Create a session table with a hard cap on tracked flows.
    pub fn with_max_sessions(max_sessions: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            max_sessions: max_sessions.max(1),
        }
    }

    /// Track one outbound packet and refresh the flow timestamp.
    ///
    /// Returns the stored session, or `None` when protocol is unsupported.
    pub fn observe_packet(&mut self, packet: &IpPacket, now: Instant) -> Option<&TunSession> {
        let flow = FlowKey::from_packet(packet)?;
        // Hot path: an already-tracked flow refreshes its timestamp with a single
        // lookup and no key clone.
        if self.sessions.contains_key(&flow) {
            let session = self.sessions.get_mut(&flow).expect("entry present");
            session.last_seen = now;
            return Some(session);
        }
        // Cold path: a new flow may need to evict before inserting.
        if self.sessions.len() >= self.max_sessions {
            self.evict_oldest();
        }
        let session = self.sessions.entry(flow.clone()).or_insert(TunSession {
            flow,
            last_seen: now,
        });
        session.last_seen = now;
        Some(session)
    }

    /// Find a tracked forward flow that matches this reverse packet.
    pub fn find_response_flow(&self, packet: &IpPacket) -> Option<&FlowKey> {
        self.sessions
            .keys()
            .find(|flow| flow.matches_response(packet))
    }

    /// Remove sessions idle longer than `idle_timeout`.
    ///
    /// Returns number of removed sessions.
    pub fn remove_expired(&mut self, now: Instant, idle_timeout: Duration) -> usize {
        let before = self.sessions.len();
        self.sessions
            .retain(|_, session| now.duration_since(session.last_seen) <= idle_timeout);
        before - self.sessions.len()
    }

    /// Clear all sessions after a network change invalidates socket/interface state.
    pub fn clear_for_network_change(&mut self) -> usize {
        let removed = self.sessions.len();
        self.sessions.clear();
        removed
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .sessions
            .iter()
            .min_by_key(|(_, session)| session.last_seen)
            .map(|(key, _)| key.clone())
        {
            self.sessions.remove(&oldest_key);
        }
    }

    /// Returns number of tracked sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Returns `true` when no sessions are tracked.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

impl From<(IpAddr, u16, IpAddr, u16, TransportProtocol)> for FlowKey {
    fn from(value: (IpAddr, u16, IpAddr, u16, TransportProtocol)) -> Self {
        Self {
            client: SocketAddr::new(value.0, value.1),
            remote: SocketAddr::new(value.2, value.3),
            protocol: value.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn packet(
        src: [u8; 4],
        src_port: u16,
        dst: [u8; 4],
        dst_port: u16,
        protocol: TransportProtocol,
    ) -> IpPacket {
        IpPacket {
            src: IpAddr::V4(Ipv4Addr::from(src)),
            dst: IpAddr::V4(Ipv4Addr::from(dst)),
            src_port,
            dst_port,
            protocol,
            header_len: 20,
            payload_offset: 28,
            transport_offset: 20,
            payload_len: 0,
            tcp_seq: None,
            tcp_ack: None,
            tcp_flags: None,
        }
    }

    #[test]
    fn session_table_tracks_forward_and_reverse_flow() {
        let now = Instant::now();
        let outbound = packet(
            [10, 0, 0, 2],
            53000,
            [8, 8, 8, 8],
            53,
            TransportProtocol::Udp,
        );
        let response = packet(
            [8, 8, 8, 8],
            53,
            [10, 0, 0, 2],
            53000,
            TransportProtocol::Udp,
        );

        let mut table = TunSessionTable::new();
        table.observe_packet(&outbound, now).unwrap();
        let flow = table.find_response_flow(&response).unwrap();

        assert_eq!(flow.client.port(), 53000);
        assert_eq!(flow.remote.port(), 53);
    }

    #[test]
    fn session_table_expires_idle_flows() {
        let now = Instant::now();
        let mut table = TunSessionTable::new();
        let outbound = packet(
            [10, 0, 0, 2],
            53000,
            [8, 8, 8, 8],
            53,
            TransportProtocol::Udp,
        );

        table.observe_packet(&outbound, now).unwrap();
        assert_eq!(table.len(), 1);

        let removed = table.remove_expired(now + Duration::from_secs(61), Duration::from_secs(60));
        assert_eq!(removed, 1);
        assert!(table.is_empty());
    }

    #[test]
    fn session_table_caps_active_flows_and_clears_on_network_change() {
        let now = Instant::now();
        let mut table = TunSessionTable::with_max_sessions(1);
        let first = packet(
            [10, 0, 0, 2],
            53000,
            [8, 8, 8, 8],
            53,
            TransportProtocol::Udp,
        );
        let second = packet(
            [10, 0, 0, 3],
            53001,
            [1, 1, 1, 1],
            53,
            TransportProtocol::Udp,
        );

        table.observe_packet(&first, now).unwrap();
        table
            .observe_packet(&second, now + Duration::from_secs(1))
            .unwrap();

        assert_eq!(table.len(), 1);
        assert_eq!(table.clear_for_network_change(), 1);
        assert!(table.is_empty());
    }
}
