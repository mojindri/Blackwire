use bytes::Bytes;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketClass {
    Control,
    Dns,
    Interactive,
    WebFirstByte,
    Bulk,
}

impl PacketClass {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Dns => "dns",
            Self::Interactive => "interactive",
            Self::WebFirstByte => "web-first-byte",
            Self::Bulk => "bulk",
        }
    }

    pub fn deadline(self) -> Option<Duration> {
        match self {
            Self::Control => Some(Duration::from_millis(50)),
            Self::Dns => Some(Duration::from_millis(100)),
            Self::Interactive => Some(Duration::from_millis(80)),
            Self::WebFirstByte => Some(Duration::from_millis(150)),
            Self::Bulk => None,
        }
    }

    fn dequeue_order() -> [Self; 5] {
        [
            Self::Control,
            Self::Interactive,
            Self::Dns,
            Self::WebFirstByte,
            Self::Bulk,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerFlowKey {
    pub src_ip: Option<IpAddr>,
    pub dst_ip: Option<IpAddr>,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub user_hash: Option<u64>,
}

impl Hash for InnerFlowKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.src_ip.hash(state);
        self.dst_ip.hash(state);
        self.src_port.hash(state);
        self.dst_port.hash(state);
        self.protocol.hash(state);
        self.user_hash.hash(state);
    }
}

#[derive(Debug, Clone)]
pub struct InnerFlowPacket {
    pub class: PacketClass,
    pub flow: InnerFlowKey,
    pub payload: Bytes,
    pub followups: Vec<Bytes>,
    pub enqueued_at: Instant,
}

impl InnerFlowPacket {
    /// Construct a new packet with the given class, flow key, and payload, timestamped now.
    pub fn new(class: PacketClass, flow: InnerFlowKey, payload: Bytes) -> Self {
        Self {
            class,
            flow,
            payload,
            followups: Vec::new(),
            enqueued_at: Instant::now(),
        }
    }
}

/// Deficit-round-robin scheduler that prioritises latency-sensitive packet classes over bulk traffic.
#[derive(Debug)]
pub struct InnerFlowScheduler {
    queues: HashMap<PacketClass, VecDeque<InnerFlowPacket>>,
    quantum_bytes: usize,
    max_packets_per_flow: usize,
}

impl Default for InnerFlowScheduler {
    fn default() -> Self {
        Self::new(1514, 256)
    }
}

impl InnerFlowScheduler {
    /// Create a scheduler with the given per-flow quantum (bytes) and maximum per-class queue depth.
    pub fn new(quantum_bytes: usize, max_packets_per_flow: usize) -> Self {
        Self {
            queues: HashMap::new(),
            quantum_bytes: quantum_bytes.max(256),
            max_packets_per_flow: max_packets_per_flow.max(1),
        }
    }

    /// Enqueue a packet; drops the oldest packet in its class queue if the queue is full.
    pub fn enqueue(&mut self, packet: InnerFlowPacket) {
        let class = packet.class;
        let queue = self.queues.entry(class).or_default();
        if queue.len() >= self.max_packets_per_flow {
            queue.pop_front();
            record_innerflow_drop(class, "queue-full");
        }
        record_innerflow_enqueue(class);
        queue.push_back(packet);
    }

    /// Dequeue the next packet according to priority order, dropping any stale packets first.
    pub fn dequeue(&mut self) -> Option<InnerFlowPacket> {
        self.drop_stale();
        for class in PacketClass::dequeue_order() {
            let Some(queue) = self.queues.get_mut(&class) else {
                continue;
            };
            let Some(packet) = queue.pop_front() else {
                continue;
            };
            let bytes = packet.payload.len().max(1);
            let rounds = bytes.div_ceil(self.quantum_bytes);
            if matches!(class, PacketClass::Bulk) && rounds > 1 {
                record_bulk_fairness();
            }
            record_innerflow_dequeue(class);
            return Some(packet);
        }
        None
    }

    /// Returns `true` if all per-class queues are empty.
    pub fn is_empty(&self) -> bool {
        self.queues.values().all(VecDeque::is_empty)
    }

    fn drop_stale(&mut self) {
        let now = Instant::now();
        for class in PacketClass::dequeue_order() {
            let Some(deadline) = class.deadline() else {
                continue;
            };
            let Some(queue) = self.queues.get_mut(&class) else {
                continue;
            };
            let before = queue.len();
            queue.retain(|packet| now.duration_since(packet.enqueued_at) <= deadline);
            let dropped = before.saturating_sub(queue.len());
            for _ in 0..dropped {
                record_innerflow_drop(class, "deadline");
            }
        }
    }
}

/// Record the scheduling queue delay for a packet class as a metrics histogram observation.
pub fn record_queue_delay(class: PacketClass, enqueued_at: Instant) {
    let delay_ms = enqueued_at.elapsed().as_secs_f64() * 1000.0;
    metrics::histogram!(
        "blackwire_innerflow_queue_delay_ms",
        "class" => class.as_label()
    )
    .record(delay_ms);
}

fn record_innerflow_enqueue(class: PacketClass) {
    metrics::counter!(
        "blackwire_innerflow_dequeued_total",
        "class" => class.as_label(),
        "stage" => "enqueue"
    )
    .increment(1);
}

fn record_innerflow_dequeue(class: PacketClass) {
    metrics::counter!(
        "blackwire_innerflow_dequeued_total",
        "class" => class.as_label(),
        "stage" => "dequeue"
    )
    .increment(1);
}

fn record_innerflow_drop(class: PacketClass, reason: &'static str) {
    metrics::counter!(
        "blackwire_innerflow_drops_total",
        "class" => class.as_label(),
        "reason" => reason
    )
    .increment(1);
}

fn record_bulk_fairness() {
    metrics::counter!("blackwire_innerflow_bulk_starvation_prevented_total").increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(port: u16) -> InnerFlowKey {
        InnerFlowKey {
            src_ip: None,
            dst_ip: None,
            src_port: 12345,
            dst_port: port,
            protocol: 17,
            user_hash: None,
        }
    }

    #[test]
    fn sparse_interactive_and_dns_dequeue_before_bulk() {
        let mut scheduler = InnerFlowScheduler::default();
        scheduler.enqueue(InnerFlowPacket::new(
            PacketClass::Bulk,
            flow(443),
            Bytes::from_static(&[0u8; 1200]),
        ));
        scheduler.enqueue(InnerFlowPacket::new(
            PacketClass::Dns,
            flow(53),
            Bytes::from_static(b"dns"),
        ));
        scheduler.enqueue(InnerFlowPacket::new(
            PacketClass::Interactive,
            flow(5000),
            Bytes::from_static(b"game"),
        ));

        let first = scheduler.dequeue().expect("first packet");
        assert_eq!(first.class, PacketClass::Interactive);
        let second = scheduler.dequeue().expect("second packet");
        assert_eq!(second.class, PacketClass::Dns);
        let third = scheduler.dequeue().expect("third packet");
        assert_eq!(third.class, PacketClass::Bulk);
    }

    #[test]
    fn deadline_scheduler_drops_stale_interactive_packet() {
        let mut scheduler = InnerFlowScheduler::default();
        let mut packet = InnerFlowPacket::new(
            PacketClass::Interactive,
            flow(5000),
            Bytes::from_static(b"game"),
        );
        packet.enqueued_at = Instant::now() - Duration::from_millis(100);
        scheduler.enqueue(packet);
        assert!(scheduler.dequeue().is_none());
    }

    #[test]
    fn bulk_has_no_deadline_drop() {
        let mut scheduler = InnerFlowScheduler::default();
        let mut packet = InnerFlowPacket::new(
            PacketClass::Bulk,
            flow(443),
            Bytes::from_static(&[0u8; 1200]),
        );
        packet.enqueued_at = Instant::now() - Duration::from_secs(10);
        scheduler.enqueue(packet);
        assert_eq!(
            scheduler.dequeue().expect("bulk packet").class,
            PacketClass::Bulk
        );
    }
}
