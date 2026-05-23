use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use super::packet::{IpPacket, TransportProtocol};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowKey {
    pub client: SocketAddr,
    pub remote: SocketAddr,
    pub protocol: TransportProtocol,
}

impl FlowKey {
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

    pub fn matches_response(&self, packet: &IpPacket) -> bool {
        packet.protocol == self.protocol
            && packet.src == self.remote.ip()
            && packet.src_port == self.remote.port()
            && packet.dst == self.client.ip()
            && packet.dst_port == self.client.port()
    }
}

#[derive(Debug, Clone)]
pub struct TunSession {
    pub flow: FlowKey,
    pub last_seen: Instant,
}

#[derive(Debug, Default)]
pub struct TunSessionTable {
    sessions: HashMap<FlowKey, TunSession>,
}

impl TunSessionTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_packet(&mut self, packet: &IpPacket, now: Instant) -> Option<&TunSession> {
        let flow = FlowKey::from_packet(packet)?;
        let session = self.sessions.entry(flow.clone()).or_insert(TunSession {
            flow,
            last_seen: now,
        });
        session.last_seen = now;
        Some(session)
    }

    pub fn find_response_flow(&self, packet: &IpPacket) -> Option<&FlowKey> {
        self.sessions
            .keys()
            .find(|flow| flow.matches_response(packet))
    }

    pub fn remove_expired(&mut self, now: Instant, idle_timeout: Duration) -> usize {
        let before = self.sessions.len();
        self.sessions
            .retain(|_, session| now.duration_since(session.last_seen) <= idle_timeout);
        before - self.sessions.len()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

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
            payload_len: 0,
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
}
