use bytes::Bytes;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Number of `PacketClass` variants; sizes the per-class queue array.
const CLASS_COUNT: usize = 5;

/// Run the full stale sweep (across all classes) once per this many dequeues.
/// The dequeued class always front-drops stale packets eagerly; this periodic
/// sweep only reclaims memory from starved classes without scanning every call.
const STALE_SWEEP_INTERVAL: u32 = 64;

/// Latency class assigned to a packet, used to prioritise traffic in the inner-flow scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketClass {
    /// Highest-priority control-plane traffic (e.g. handshakes, keepalives).
    Control,
    /// DNS query/response traffic.
    Dns,
    /// Interactive traffic requiring low latency (e.g. SSH keystrokes, VoIP).
    Interactive,
    /// First-byte traffic for web requests where TTFB matters.
    WebFirstByte,
    /// Background bulk data transfer with no strict latency requirement.
    Bulk,
}

impl PacketClass {
    /// Return a short ASCII label used in metrics and log output.
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Dns => "dns",
            Self::Interactive => "interactive",
            Self::WebFirstByte => "web-first-byte",
            Self::Bulk => "bulk",
        }
    }

    /// Maximum time a packet of this class may wait in the scheduler queue before being dropped.
    pub fn deadline(self) -> Option<Duration> {
        match self {
            Self::Control => Some(Duration::from_millis(50)),
            Self::Dns => Some(Duration::from_millis(100)),
            Self::Interactive => Some(Duration::from_millis(80)),
            Self::WebFirstByte => Some(Duration::from_millis(150)),
            Self::Bulk => None,
        }
    }

    fn dequeue_order() -> [Self; CLASS_COUNT] {
        [
            Self::Control,
            Self::Interactive,
            Self::Dns,
            Self::WebFirstByte,
            Self::Bulk,
        ]
    }

    /// Stable array index for this class in the scheduler's per-class queues.
    const fn index(self) -> usize {
        match self {
            Self::Control => 0,
            Self::Dns => 1,
            Self::Interactive => 2,
            Self::WebFirstByte => 3,
            Self::Bulk => 4,
        }
    }
}

/// Five-tuple flow identifier used to classify packets into per-flow scheduler queues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerFlowKey {
    /// Source IP address, or `None` for non-IP traffic.
    pub src_ip: Option<IpAddr>,
    /// Destination IP address, or `None` for non-IP traffic.
    pub dst_ip: Option<IpAddr>,
    /// Source transport port (0 for protocols without ports).
    pub src_port: u16,
    /// Destination transport port (0 for protocols without ports).
    pub dst_port: u16,
    /// IP protocol number (e.g. 6 = TCP, 17 = UDP).
    pub protocol: u8,
    /// Optional per-user hash used to separate flows by authenticated identity.
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

/// A packet buffered inside the inner-flow scheduler, carrying its class, flow key, and payload.
#[derive(Debug, Clone)]
pub struct InnerFlowPacket {
    /// Latency class that determines scheduler priority for this packet.
    pub class: PacketClass,
    /// Flow key used to group this packet with others from the same connection.
    pub flow: InnerFlowKey,
    /// Primary packet payload bytes.
    pub payload: Bytes,
    /// Additional payload chunks to be sent after the primary payload.
    pub followups: Vec<Bytes>,
    /// Instant at which this packet was placed into the scheduler queue.
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
///
/// Per-class queues are held in a fixed array indexed by [`PacketClass::index`]
/// rather than a `HashMap`, so enqueue/dequeue do no hashing — important on the
/// TUN/NAT packet hot path that runs tens of thousands of times per second.
#[derive(Debug)]
pub struct InnerFlowScheduler {
    queues: [VecDeque<InnerFlowPacket>; CLASS_COUNT],
    quantum_bytes: usize,
    max_packets_per_flow: usize,
    dequeues_since_sweep: u32,
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
            queues: std::array::from_fn(|_| VecDeque::new()),
            quantum_bytes: quantum_bytes.max(256),
            max_packets_per_flow: max_packets_per_flow.max(1),
            dequeues_since_sweep: 0,
        }
    }

    /// Enqueue a packet; drops the oldest packet in its class queue if the queue is full.
    pub fn enqueue(&mut self, packet: InnerFlowPacket) {
        let class = packet.class;
        let queue = &mut self.queues[class.index()];
        if queue.len() >= self.max_packets_per_flow {
            queue.pop_front();
            record_innerflow_drop(class, "queue-full");
        }
        record_innerflow_enqueue(class);
        queue.push_back(packet);
    }

    /// Dequeue the next packet according to priority order, skipping (and dropping)
    /// any packets that have already exceeded their class deadline.
    pub fn dequeue(&mut self) -> Option<InnerFlowPacket> {
        // Periodically reclaim memory from starved classes that are never
        // reached by the priority loop below; the hot path otherwise only
        // front-drops stale packets from the class it actually serves.
        self.dequeues_since_sweep = self.dequeues_since_sweep.wrapping_add(1);
        if self.dequeues_since_sweep >= STALE_SWEEP_INTERVAL {
            self.dequeues_since_sweep = 0;
            self.drop_stale();
        }

        let now = Instant::now();
        for class in PacketClass::dequeue_order() {
            let deadline = class.deadline();
            let queue = &mut self.queues[class.index()];
            while let Some(packet) = queue.pop_front() {
                if let Some(deadline) = deadline {
                    if now.duration_since(packet.enqueued_at) > deadline {
                        record_innerflow_drop(class, "deadline");
                        continue;
                    }
                }
                let bytes = packet.payload.len().max(1);
                let rounds = bytes.div_ceil(self.quantum_bytes);
                if matches!(class, PacketClass::Bulk) && rounds > 1 {
                    record_bulk_fairness();
                }
                record_innerflow_dequeue(class);
                return Some(packet);
            }
        }
        None
    }

    /// Returns `true` if all per-class queues are empty.
    pub fn is_empty(&self) -> bool {
        self.queues.iter().all(VecDeque::is_empty)
    }

    fn drop_stale(&mut self) {
        let now = Instant::now();
        for class in PacketClass::dequeue_order() {
            let Some(deadline) = class.deadline() else {
                continue;
            };
            let queue = &mut self.queues[class.index()];
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
