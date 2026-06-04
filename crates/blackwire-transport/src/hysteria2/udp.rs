//! UDP proxy over Hysteria2 QUIC datagrams.
//!
//! UDP packets are sent as QUIC datagrams (RFC 9221) rather than streams.
//! Each datagram is self-contained and carries: session ID, packet ID,
//! fragmentation info, destination address, and the UDP payload.
//!
//! # Fragmentation
//!
//! QUIC datagram size is bounded by the path MTU (typically ~1200 bytes for
//! the initial datagram). Large UDP payloads are split into fragments. Each
//! fragment has `frag_num > 1`; the last fragment also marks `frag_id =
//! frag_num - 1`. The receiver reassembles fragments by `session_id + packet_id`.
//!
//! # Datagram wire format
//!
//! ```text
//! [session_id: 4 bytes BE]   — identifies the UDP "flow"
//! [packet_id: 2 bytes BE]    — sequence number within session
//! [frag_id: 1 byte]          — which fragment this is (0-indexed)
//! [frag_num: 1 byte]         — total number of fragments (1 = not fragmented)
//! [addr_type: 1 byte]        — 0x01=IPv4, 0x02=IPv6, 0x03=domain
//! [addr + port]              — destination
//! [data: remaining bytes]    — UDP payload fragment
//! ```

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};

use crate::innerflow::{InnerFlowKey, PacketClass};

/// Destination inside a UDP datagram using Hysteria2's compact binary address layout.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    /// IPv4 destination and port.
    V4(Ipv4Addr, u16),
    /// IPv6 destination and port.
    V6(Ipv6Addr, u16),
    /// Domain destination and port.
    Domain(String, u16),
}

/// A single UDP datagram (or fragment of one).
#[derive(Debug, Clone, PartialEq)]
pub struct UdpDatagram {
    /// Identifies the UDP flow (one per client UDP socket).
    pub session_id: u32,
    /// Sequence number within the session for fragment ordering.
    pub packet_id: u16,
    /// Zero-based index of this fragment.
    pub frag_id: u8,
    /// Total number of fragments for this packet (1 means unfragmented).
    pub frag_num: u8,
    /// Destination address for this UDP packet.
    pub dest: Destination,
    /// UDP payload fragment.
    pub data: Bytes,
}

/// Encode a `UdpDatagram` into a byte buffer suitable for a QUIC datagram.
pub fn encode_udp_datagram(dg: &UdpDatagram) -> Bytes {
    let mut buf = BytesMut::with_capacity(256 + dg.data.len());

    buf.put_u32(dg.session_id);
    buf.put_u16(dg.packet_id);
    buf.put_u8(dg.frag_id);
    buf.put_u8(dg.frag_num);

    match &dg.dest {
        Destination::V4(ip, port) => {
            buf.put_u8(0x01);
            buf.put_slice(&ip.octets());
            buf.put_u16(*port);
        }
        Destination::V6(ip, port) => {
            buf.put_u8(0x02);
            buf.put_slice(&ip.octets());
            buf.put_u16(*port);
        }
        Destination::Domain(name, port) => {
            let name_bytes = name.as_bytes();
            buf.put_u8(0x03);
            buf.put_u8(name_bytes.len() as u8);
            buf.put_slice(name_bytes);
            buf.put_u16(*port);
        }
    }

    buf.put_slice(&dg.data);
    buf.freeze()
}

/// Decode a `UdpDatagram` from a raw byte slice received as a QUIC datagram.
///
/// # Errors
///
/// Returns an error if the slice is too short or contains invalid data.
pub fn decode_udp_datagram(mut data: &[u8]) -> Result<UdpDatagram> {
    // Each field below consumes bytes from `data` via the `Buf` trait.
    anyhow::ensure!(data.len() >= 9, "datagram too short (< 9 bytes)");

    let session_id = data.get_u32();
    let packet_id = data.get_u16();
    let frag_id = data.get_u8();
    let frag_num = data.get_u8();

    let addr_type = data.get_u8();
    let dest = match addr_type {
        0x01 => {
            anyhow::ensure!(data.len() >= 6, "truncated IPv4 address");
            let mut octets = [0u8; 4];
            octets.copy_from_slice(&data[..4]);
            data.advance(4);
            let port = data.get_u16();
            Destination::V4(Ipv4Addr::from(octets), port)
        }
        0x02 => {
            anyhow::ensure!(data.len() >= 18, "truncated IPv6 address");
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&data[..16]);
            data.advance(16);
            let port = data.get_u16();
            Destination::V6(Ipv6Addr::from(octets), port)
        }
        0x03 => {
            anyhow::ensure!(!data.is_empty(), "missing domain name length");
            let name_len = data.get_u8() as usize;
            anyhow::ensure!(data.len() >= name_len + 2, "truncated domain name");
            let name_bytes = &data[..name_len];
            let name = std::str::from_utf8(name_bytes).context("domain name is not valid UTF-8")?;
            let name = name.to_string();
            data.advance(name_len);
            let port = data.get_u16();
            Destination::Domain(name, port)
        }
        t => anyhow::bail!("unknown UDP address type: 0x{t:02X}"),
    };

    let payload = Bytes::copy_from_slice(data);

    Ok(UdpDatagram {
        session_id,
        packet_id,
        frag_id,
        frag_num,
        dest,
        data: payload,
    })
}

const FEC_MARKER_DOMAIN: &str = "__blackwire_fec_v1__";
const FEC_MARKER_V4: Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);
const FEC_MARKER_PORT: u16 = 0;
const FEC_MAGIC: &[u8; 6] = b"BWFEC1";
const FEC_FLAG_COMPACT_PAYLOAD: u8 = 0x80;
const FEC_FLAG_FIXED_LENGTHS: u8 = 0x40;
const FEC_GROUP_SIZE: u8 = 4;
const FEC_GROUP_TTL: Duration = Duration::from_millis(750);
const FEC_RECOVERY_DEADLINE: Duration = Duration::from_millis(100);
const FEC_SEEN_LIMIT: usize = 4096;
const FEC_GENERATION_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatagramLane {
    Reliable,
    Unreliable,
    Priority,
}

impl DatagramLane {
    pub fn class(self) -> &'static str {
        match self {
            Self::Reliable => "reliable",
            Self::Unreliable => "unreliable",
            Self::Priority => "priority",
        }
    }
}

/// Priority mode that governs which datagram lane is used for outbound UDP traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DatagramPriorityMode {
    #[default]
    /// Standard single-lane mode: all datagrams use the unreliable QUIC datagram channel.
    Standard,
    /// H2+ mode: small and DNS-like datagrams use a priority lane; larger ones use unreliable.
    H2Plus,
}

/// Policy controlling how outbound UDP datagrams are scheduled and retried.
#[derive(Debug, Clone, Copy)]
pub struct DatagramPolicy {
    /// Datagram priority mode determining which QUIC lane to use.
    pub mode: DatagramPriorityMode,
    /// Maximum time in milliseconds a datagram may wait in the queue before being dropped.
    pub max_queue_delay_ms: u64,
    /// Whether to immediately retry DNS-like datagrams on a priority lane.
    pub fast_dns_retry: bool,
    /// Delay in milliseconds before the fast DNS retry is dispatched.
    pub fast_dns_retry_delay_ms: u64,
}

impl Default for DatagramPolicy {
    fn default() -> Self {
        Self {
            mode: DatagramPriorityMode::Standard,
            max_queue_delay_ms: 25,
            fast_dns_retry: false,
            fast_dns_retry_delay_ms: 20,
        }
    }
}

impl DatagramPolicy {
    /// Select the appropriate datagram lane for the given destination and payload size.
    pub fn lane_for(&self, dest: &Destination, payload_len: usize) -> DatagramLane {
        match self.mode {
            DatagramPriorityMode::Standard => DatagramLane::Unreliable,
            DatagramPriorityMode::H2Plus => {
                if is_dns_like(dest) || payload_len <= 140 {
                    DatagramLane::Priority
                } else {
                    DatagramLane::Unreliable
                }
            }
        }
    }

    /// Returns `true` if a fast retry on the priority lane should be attempted for this DNS destination.
    pub fn should_fast_retry_dns(&self, dest: &Destination) -> bool {
        self.mode == DatagramPriorityMode::H2Plus && self.fast_dns_retry && is_dns_like(dest)
    }
}

/// Classify a UDP datagram into a `PacketClass` based on destination and payload size.
pub fn packet_class_for(dest: &Destination, payload_len: usize) -> PacketClass {
    if is_dns_like(dest) {
        PacketClass::Dns
    } else if payload_len <= 140 {
        PacketClass::Interactive
    } else {
        PacketClass::Bulk
    }
}

/// Build an `InnerFlowKey` that identifies a hysteria2 UDP session for scheduling purposes.
pub fn flow_key_for(dest: &Destination, session_id: u32) -> InnerFlowKey {
    let (dst_ip, dst_port) = match dest {
        Destination::V4(ip, port) => (Some(IpAddr::V4(*ip)), *port),
        Destination::V6(ip, port) => (Some(IpAddr::V6(*ip)), *port),
        Destination::Domain(_, port) => (None, *port),
    };
    InnerFlowKey {
        src_ip: None,
        dst_ip,
        src_port: (session_id & 0xffff) as u16,
        dst_port,
        protocol: 17,
        user_hash: None,
    }
}

fn is_dns_like(dest: &Destination) -> bool {
    match dest {
        Destination::V4(_, port) | Destination::V6(_, port) => matches!(port, 53 | 5353),
        Destination::Domain(_, port) => matches!(port, 53 | 5353),
    }
}

fn destination_encoded_len(dest: &Destination) -> usize {
    match dest {
        Destination::V4(_, _) => 1 + 4 + 2,
        Destination::V6(_, _) => 1 + 16 + 2,
        Destination::Domain(name, _) => 1 + 1 + name.len() + 2,
    }
}

fn encode_destination(dest: &Destination, buf: &mut BytesMut) {
    match dest {
        Destination::V4(ip, port) => {
            buf.put_u8(0x01);
            buf.put_slice(&ip.octets());
            buf.put_u16(*port);
        }
        Destination::V6(ip, port) => {
            buf.put_u8(0x02);
            buf.put_slice(&ip.octets());
            buf.put_u16(*port);
        }
        Destination::Domain(name, port) => {
            let name_bytes = name.as_bytes();
            buf.put_u8(0x03);
            buf.put_u8(name_bytes.len() as u8);
            buf.put_slice(name_bytes);
            buf.put_u16(*port);
        }
    }
}

fn decode_destination(data: &mut &[u8]) -> Option<Destination> {
    if data.is_empty() {
        return None;
    }
    match data.get_u8() {
        0x01 => {
            if data.len() < 6 {
                return None;
            }
            let mut octets = [0u8; 4];
            octets.copy_from_slice(&data[..4]);
            data.advance(4);
            Some(Destination::V4(Ipv4Addr::from(octets), data.get_u16()))
        }
        0x02 => {
            if data.len() < 18 {
                return None;
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&data[..16]);
            data.advance(16);
            Some(Destination::V6(Ipv6Addr::from(octets), data.get_u16()))
        }
        0x03 => {
            if data.is_empty() {
                return None;
            }
            let name_len = data.get_u8() as usize;
            if data.len() < name_len + 2 {
                return None;
            }
            let name = std::str::from_utf8(&data[..name_len]).ok()?.to_string();
            data.advance(name_len);
            Some(Destination::Domain(name, data.get_u16()))
        }
        _ => None,
    }
}

/// Forward-error-correction algorithm applied to outbound hysteria2 UDP packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FecMode {
    /// Disable forward-error-correction parity generation.
    Off,
    /// Emit one XOR parity packet for each completed block of packets.
    Xor1OfN,
    /// Emit Reed-Solomon parity for stronger recovery at higher cost.
    ReedSolomon,
    /// Reserved adaptive parity mode for future experimentation.
    RaptorLike,
    /// Let the runtime choose the effective mode from the policy.
    Auto,
}

impl FecMode {
    /// Resolve any automatic policy choice into a concrete runtime mode.
    pub fn effective(self, _max_overhead_percent: u8) -> Self {
        match self {
            Self::Auto => Self::Off,
            mode => mode,
        }
    }

    /// Return the metrics/debug label for this FEC mode.
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Xor1OfN => "xor1-of-n",
            Self::ReedSolomon => "reed-solomon",
            Self::RaptorLike => "raptor-like",
            Self::Auto => "auto",
        }
    }
}

/// Runtime policy controlling forward-error-correction behaviour for a hysteria2 session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FecPolicy {
    /// Selected parity algorithm.
    pub mode: FecMode,
    /// Maximum parity overhead the encoder may spend, as a percentage.
    pub max_overhead_percent: u8,
    /// Target packet group size before parity is emitted.
    pub group_size: u8,
    /// Skip block FEC while the flow still looks like sequential DNS traffic.
    pub disable_for_sequential_dns: bool,
    /// Minimum number of packets that must be present before generating parity.
    pub min_concurrency_for_block_fec: usize,
    /// Maximum time to hold a partial block while waiting for parity input.
    pub max_generation_delay: Duration,
    /// Maximum age for a recovery attempt once parity is available.
    pub recovery_deadline: Duration,
    /// Size of the duplicate-detection window, in packet keys.
    pub dedup_window_packets: usize,
}

/// Snapshot counters describing recent FEC encoder/decoder activity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FecSnapshot {
    /// Number of parity packets generated by the encoder.
    pub parity_packets: u64,
    /// Total parity bytes added to the wire.
    pub overhead_bytes: u64,
    /// Number of original packets recovered from parity.
    pub recovered_packets: u64,
    /// Number of stale FEC groups discarded before recovery completed.
    pub stale_drops: u64,
    /// Number of packets intentionally left unprotected for safety heuristics.
    pub duplicate_safe_skips: u64,
}

impl Default for FecPolicy {
    fn default() -> Self {
        Self {
            mode: FecMode::Off,
            max_overhead_percent: 20,
            group_size: FEC_GROUP_SIZE,
            disable_for_sequential_dns: true,
            min_concurrency_for_block_fec: 4,
            max_generation_delay: FEC_GENERATION_DELAY,
            recovery_deadline: FEC_RECOVERY_DEADLINE,
            dedup_window_packets: 1024,
        }
    }
}

impl FecPolicy {
    /// Whether this policy should actively generate and accept parity packets.
    pub fn enabled(self) -> bool {
        matches!(
            self.mode.effective(self.max_overhead_percent),
            FecMode::Xor1OfN | FecMode::ReedSolomon | FecMode::RaptorLike
        ) && self.max_overhead_percent >= 20
    }

    fn effective_group_size(self) -> u8 {
        let overhead = self.max_overhead_percent.max(1) as usize;
        let min_for_cap = 100usize.div_ceil(overhead).max(2);
        self.group_size.max(min_for_cap.min(u8::MAX as usize) as u8)
    }

    fn effective_group_size_for(
        self,
        payload_len: usize,
        encoded_len: usize,
        dest: &Destination,
    ) -> u8 {
        let mut group_size = self.effective_group_size() as usize;
        let overhead = self.max_overhead_percent.max(1) as usize;
        let payload_len = payload_len.max(1);
        while group_size < u8::MAX as usize {
            let parity_len = estimate_parity_datagram_len(
                payload_len,
                group_size,
                Some(destination_encoded_len(dest)),
                true,
            )
            .min(estimate_parity_datagram_len(
                encoded_len,
                group_size,
                None,
                false,
            ));
            if parity_len * 100 <= payload_len * group_size * overhead {
                break;
            }
            group_size += 1;
        }
        group_size as u8
    }
}

/// Stateful encoder for datagram FEC groups.
#[derive(Debug)]
pub struct FecEncoder {
    policy: FecPolicy,
    groups: HashMap<(u32, u16), FecEncodeGroup>,
    active_dns_flows: HashSet<u32>,
    parity_packets: u64,
    overhead_bytes: u64,
    duplicate_safe_skips: u64,
}

#[derive(Debug)]
struct FecEncodeGroup {
    created: Instant,
    slots: Vec<Option<Bytes>>,
}

impl FecEncoder {
    /// Create an encoder using the supplied FEC policy.
    pub fn new(policy: FecPolicy) -> Self {
        Self {
            policy,
            groups: HashMap::new(),
            active_dns_flows: HashSet::new(),
            parity_packets: 0,
            overhead_bytes: 0,
            duplicate_safe_skips: 0,
        }
    }

    /// Optionally emit a parity datagram that protects the provided payload.
    pub fn protect(&mut self, original: &UdpDatagram, encoded: &Bytes) -> Option<Bytes> {
        self.expire_stale();
        if !self.policy.enabled() || original.frag_num != 1 {
            return None;
        }
        if self.should_skip_sequential_dns(original) {
            self.duplicate_safe_skips += 1;
            record_fec_duplicate_safe_skip("sequential-dns");
            return None;
        }
        let group_size = self.policy.effective_group_size_for(
            original.data.len(),
            encoded.len(),
            &original.dest,
        );
        let base = original.packet_id - (original.packet_id % group_size as u16);
        let idx = (original.packet_id - base) as usize;
        let key = (original.session_id, base);
        let group = self.groups.entry(key).or_insert_with(|| FecEncodeGroup {
            created: Instant::now(),
            slots: vec![None; group_size as usize],
        });
        if idx >= group.slots.len() {
            return None;
        }
        group.slots[idx] = Some(encoded.clone());
        if group.slots.iter().filter(|slot| slot.is_some()).count()
            < self
                .policy
                .min_concurrency_for_block_fec
                .max(1)
                .min(group_size as usize)
        {
            return None;
        }
        if group.slots.iter().all(Option::is_some) {
            let mode = self.policy.mode.effective(self.policy.max_overhead_percent);
            let parity = build_parity(mode, original.session_id, base, &group.slots)?;
            self.groups.remove(&key);
            self.parity_packets += 1;
            self.overhead_bytes += parity.len() as u64;
            record_fec_mode(mode);
            record_fec_overhead(parity.len());
            return Some(parity);
        }
        None
    }

    fn should_skip_sequential_dns(&mut self, original: &UdpDatagram) -> bool {
        if !self.policy.disable_for_sequential_dns || !is_dns_like(&original.dest) {
            return false;
        }
        self.active_dns_flows.insert(original.session_id);
        self.active_dns_flows.len() < self.policy.min_concurrency_for_block_fec.max(2)
    }

    fn expire_stale(&mut self) {
        let max_age = self
            .policy
            .max_generation_delay
            .max(Duration::from_millis(1));
        self.groups
            .retain(|_, group| group.created.elapsed() <= max_age);
        if self.groups.is_empty() {
            self.active_dns_flows.clear();
        }
    }

    /// Return cumulative encoder-side FEC counters.
    pub fn snapshot(&self) -> FecSnapshot {
        FecSnapshot {
            parity_packets: self.parity_packets,
            overhead_bytes: self.overhead_bytes,
            duplicate_safe_skips: self.duplicate_safe_skips,
            ..FecSnapshot::default()
        }
    }
}

/// Stateful decoder and recovery engine for datagram FEC groups.
#[derive(Debug)]
pub struct FecDecoder {
    policy: FecPolicy,
    groups: HashMap<(u32, u16), FecDecodeGroup>,
    seen: HashSet<(u32, u16)>,
    seen_order: VecDeque<(u32, u16)>,
    recovered_packets: u64,
    stale_drops: u64,
}

#[derive(Debug)]
struct FecDecodeGroup {
    created: Instant,
    slots: Vec<Option<Bytes>>,
    parity: Option<FecParity>,
}

#[derive(Debug)]
struct FecParity {
    session_id: u32,
    base_packet_id: u16,
    group_size: u8,
    max_len: usize,
    lengths: Vec<usize>,
    parity: Bytes,
    mode: FecMode,
    dest: Option<Destination>,
}

impl FecDecoder {
    /// Create a decoder using the supplied FEC policy.
    pub fn new(policy: FecPolicy) -> Self {
        Self {
            policy,
            groups: HashMap::new(),
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
            recovered_packets: 0,
            stale_drops: 0,
        }
    }

    /// Decode one raw datagram and return any recovered application datagrams.
    pub fn decode(&mut self, raw: Bytes) -> Vec<UdpDatagram> {
        self.expire_stale();
        let Ok(dg) = decode_udp_datagram(&raw) else {
            return Vec::new();
        };
        if let Some(parity) = decode_xor_parity(&dg) {
            if !self.policy.enabled() {
                record_datagram_fallback("fec-disabled");
                return Vec::new();
            }
            return self.accept_parity(parity);
        }

        let key = (dg.session_id, dg.packet_id);
        if !self.mark_seen(key) {
            return Vec::new();
        }
        let mut decoded = vec![dg.clone()];
        decoded.extend(self.accept_original(&dg, raw));
        decoded
    }

    fn accept_original(&mut self, dg: &UdpDatagram, raw: Bytes) -> Vec<UdpDatagram> {
        if !self.policy.enabled() || dg.frag_num != 1 {
            return Vec::new();
        }
        let group_size = self
            .policy
            .effective_group_size_for(dg.data.len(), raw.len(), &dg.dest);
        let base = dg.packet_id - (dg.packet_id % group_size as u16);
        let idx = (dg.packet_id - base) as usize;
        let recovered_raw = {
            let group =
                self.groups
                    .entry((dg.session_id, base))
                    .or_insert_with(|| FecDecodeGroup {
                        created: Instant::now(),
                        slots: vec![None; group_size as usize],
                        parity: None,
                    });
            if idx < group.slots.len() {
                group.slots[idx] = Some(raw);
            }
            if group.parity.is_some() && group.created.elapsed() <= self.policy.recovery_deadline {
                recover_one_missing(group)
            } else {
                None
            }
        };
        let Some(recovered_raw) = recovered_raw else {
            return Vec::new();
        };
        let Ok(recovered) = decode_udp_datagram(&recovered_raw) else {
            return Vec::new();
        };
        let seen_key = (recovered.session_id, recovered.packet_id);
        if !self.mark_seen(seen_key) {
            return Vec::new();
        }
        self.recovered_packets += 1;
        record_fec_recovered();
        vec![recovered]
    }

    fn accept_parity(&mut self, parity: FecParity) -> Vec<UdpDatagram> {
        let key = (parity.session_id, parity.base_packet_id);
        let group = self.groups.entry(key).or_insert_with(|| FecDecodeGroup {
            created: Instant::now(),
            slots: vec![None; parity.group_size as usize],
            parity: None,
        });
        group.parity = Some(parity);
        if group.created.elapsed() > FEC_RECOVERY_DEADLINE {
            self.stale_drops += 1;
            record_fec_stale_drop();
            return Vec::new();
        }
        if group.created.elapsed() > self.policy.recovery_deadline {
            self.stale_drops += 1;
            record_fec_stale_drop();
            return Vec::new();
        }
        let Some(recovered_raw) = recover_one_missing(group) else {
            return Vec::new();
        };
        let Ok(recovered) = decode_udp_datagram(&recovered_raw) else {
            return Vec::new();
        };
        let seen_key = (recovered.session_id, recovered.packet_id);
        if !self.mark_seen(seen_key) {
            return Vec::new();
        }
        self.recovered_packets += 1;
        record_fec_recovered();
        vec![recovered]
    }

    fn mark_seen(&mut self, key: (u32, u16)) -> bool {
        if !self.seen.insert(key) {
            return false;
        }
        self.seen_order.push_back(key);
        let limit = self.policy.dedup_window_packets.clamp(1, FEC_SEEN_LIMIT);
        while self.seen_order.len() > limit {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }

    fn expire_stale(&mut self) {
        let now = Instant::now();
        let before = self.groups.len();
        self.groups
            .retain(|_, group| now.duration_since(group.created) <= FEC_GROUP_TTL);
        let dropped = before.saturating_sub(self.groups.len());
        self.stale_drops += dropped as u64;
        for _ in 0..dropped {
            record_fec_stale_drop();
        }
    }

    /// Return cumulative decoder-side FEC counters.
    pub fn snapshot(&self) -> FecSnapshot {
        FecSnapshot {
            recovered_packets: self.recovered_packets,
            stale_drops: self.stale_drops,
            ..FecSnapshot::default()
        }
    }
}

fn build_parity(
    mode: FecMode,
    session_id: u32,
    base_packet_id: u16,
    slots: &[Option<Bytes>],
) -> Option<Bytes> {
    let mode = mode.effective(100);
    let compact = compact_payload_slots(slots);
    let parity_slots;
    let (source_slots, compact_dest) = if let Some((dest, slots)) = compact {
        parity_slots = slots;
        (&parity_slots[..], Some(dest))
    } else {
        (slots, None)
    };
    let max_len = source_slots
        .iter()
        .filter_map(|s| s.as_ref().map(Bytes::len))
        .max()?;
    let (lengths, parity) = match mode {
        FecMode::Xor1OfN | FecMode::RaptorLike => build_xor_parity(source_slots, max_len)?,
        FecMode::ReedSolomon => build_reed_solomon_parity(source_slots, max_len)?,
        FecMode::Auto | FecMode::Off => return None,
    };
    let dest_len = compact_dest
        .as_ref()
        .map(destination_encoded_len)
        .unwrap_or_default();
    let fixed_lengths = compact_dest.is_some() && lengths.iter().all(|len| *len == max_len);
    let length_bytes = if fixed_lengths { 2 } else { lengths.len() * 2 };
    let mut payload =
        BytesMut::with_capacity(FEC_MAGIC.len() + 9 + length_bytes + dest_len + max_len);
    payload.put_slice(FEC_MAGIC);
    payload.put_u8(
        mode_byte(mode)
            | if compact_dest.is_some() {
                FEC_FLAG_COMPACT_PAYLOAD
            } else {
                0
            }
            | if fixed_lengths {
                FEC_FLAG_FIXED_LENGTHS
            } else {
                0
            },
    );
    payload.put_u16(base_packet_id);
    payload.put_u8(slots.len() as u8);
    payload.put_u16(max_len as u16);
    if fixed_lengths {
        payload.put_u16(max_len as u16);
    } else {
        for len in lengths {
            payload.put_u16(len as u16);
        }
    }
    if let Some(dest) = &compact_dest {
        encode_destination(dest, &mut payload);
    }
    payload.put_slice(&parity);

    let dg = UdpDatagram {
        session_id,
        packet_id: base_packet_id,
        frag_id: 0,
        frag_num: 1,
        dest: Destination::V4(FEC_MARKER_V4, FEC_MARKER_PORT),
        data: payload.freeze(),
    };
    Some(encode_udp_datagram(&dg))
}

fn compact_payload_slots(slots: &[Option<Bytes>]) -> Option<(Destination, Vec<Option<Bytes>>)> {
    let mut dest = None;
    let mut compact = Vec::with_capacity(slots.len());
    for slot in slots {
        let dg = decode_udp_datagram(slot.as_ref()?).ok()?;
        if dg.frag_num != 1 || dg.frag_id != 0 {
            return None;
        }
        match &dest {
            Some(existing) if existing != &dg.dest => return None,
            None => dest = Some(dg.dest.clone()),
            _ => {}
        }
        compact.push(Some(dg.data));
    }
    dest.map(|dest| (dest, compact))
}

fn estimate_parity_datagram_len(
    parity_len: usize,
    group_size: usize,
    compact_dest_len: Option<usize>,
    fixed_lengths: bool,
) -> usize {
    let outer_header_len = 8;
    let marker_dest_len = 1 + 4 + 2;
    let fec_payload_header_len = FEC_MAGIC.len() + 1 + 2 + 1 + 2;
    let compact_dest_len = compact_dest_len.unwrap_or_default();
    let length_bytes = if fixed_lengths { 2 } else { group_size * 2 };
    outer_header_len
        + marker_dest_len
        + fec_payload_header_len
        + length_bytes
        + compact_dest_len
        + parity_len
}

fn build_xor_parity(slots: &[Option<Bytes>], max_len: usize) -> Option<(Vec<usize>, Vec<u8>)> {
    let mut xor = vec![0u8; max_len];
    let mut lengths = Vec::with_capacity(slots.len());
    for slot in slots {
        let bytes = slot.as_ref()?;
        lengths.push(bytes.len());
        for (idx, byte) in bytes.iter().enumerate() {
            xor[idx] ^= byte;
        }
    }
    Some((lengths, xor))
}

fn build_reed_solomon_parity(
    slots: &[Option<Bytes>],
    max_len: usize,
) -> Option<(Vec<usize>, Vec<u8>)> {
    let data_shards = slots.len();
    let rs = reed_solomon_erasure::galois_8::ReedSolomon::new(data_shards, 1).ok()?;
    let mut shards = Vec::with_capacity(data_shards + 1);
    let mut lengths = Vec::with_capacity(data_shards);
    for slot in slots {
        let bytes = slot.as_ref()?;
        lengths.push(bytes.len());
        let mut shard = vec![0u8; max_len];
        shard[..bytes.len()].copy_from_slice(bytes);
        shards.push(shard);
    }
    shards.push(vec![0u8; max_len]);
    rs.encode(&mut shards).ok()?;
    let parity = shards.pop()?;
    Some((lengths, parity))
}

fn decode_xor_parity(dg: &UdpDatagram) -> Option<FecParity> {
    let compact_marker =
        matches!(dg.dest, Destination::V4(ip, FEC_MARKER_PORT) if ip == FEC_MARKER_V4);
    let legacy_marker =
        matches!(&dg.dest, Destination::Domain(name, FEC_MARKER_PORT) if name == FEC_MARKER_DOMAIN);
    if !compact_marker && !legacy_marker {
        return None;
    }
    let mut data = dg.data.as_ref();
    if data.len() < FEC_MAGIC.len() + 6 || &data[..FEC_MAGIC.len()] != FEC_MAGIC {
        return None;
    }
    data.advance(FEC_MAGIC.len());
    let mode_flags = data.get_u8();
    let compact = mode_flags & FEC_FLAG_COMPACT_PAYLOAD != 0;
    let fixed_lengths = mode_flags & FEC_FLAG_FIXED_LENGTHS != 0;
    let mode = mode_from_byte(mode_flags & !(FEC_FLAG_COMPACT_PAYLOAD | FEC_FLAG_FIXED_LENGTHS))?;
    let base_packet_id = data.get_u16();
    let group_size = data.get_u8();
    let max_len = data.get_u16() as usize;
    let length_bytes = if fixed_lengths {
        2
    } else {
        group_size as usize * 2
    };
    if group_size < 2 || data.len() < length_bytes + max_len {
        return None;
    }
    let mut lengths = Vec::with_capacity(group_size as usize);
    if fixed_lengths {
        let len = data.get_u16() as usize;
        lengths.resize(group_size as usize, len);
    } else {
        for _ in 0..group_size {
            lengths.push(data.get_u16() as usize);
        }
    }
    let dest = compact.then(|| decode_destination(&mut data)).flatten();
    if compact && dest.is_none() {
        return None;
    }
    if data.len() < max_len {
        return None;
    }
    let parity = Bytes::copy_from_slice(&data[..max_len]);
    Some(FecParity {
        session_id: dg.session_id,
        base_packet_id,
        group_size,
        max_len,
        lengths,
        parity,
        mode,
        dest,
    })
}

fn recover_one_missing(group: &FecDecodeGroup) -> Option<Bytes> {
    let parity = group.parity.as_ref()?;
    let missing: Vec<usize> = group
        .slots
        .iter()
        .enumerate()
        .filter_map(|(idx, slot)| slot.is_none().then_some(idx))
        .collect();
    if missing.len() != 1 || group.slots.len() != parity.group_size as usize {
        return None;
    }
    let missing_idx = missing[0];
    let mut recovered = match parity.mode {
        FecMode::Xor1OfN | FecMode::RaptorLike => recover_xor(group, parity)?,
        FecMode::ReedSolomon => recover_reed_solomon(group, parity, missing_idx)?,
        FecMode::Auto | FecMode::Off => return None,
    };
    let len = *parity.lengths.get(missing_idx)?;
    recovered.truncate(len);
    if let Some(dest) = &parity.dest {
        return Some(encode_udp_datagram(&UdpDatagram {
            session_id: parity.session_id,
            packet_id: parity.base_packet_id + missing_idx as u16,
            frag_id: 0,
            frag_num: 1,
            dest: dest.clone(),
            data: Bytes::from(recovered),
        }));
    }
    Some(Bytes::from(recovered))
}

fn recover_xor(group: &FecDecodeGroup, parity: &FecParity) -> Option<Vec<u8>> {
    let mut recovered = parity.parity.to_vec();
    for slot in group.slots.iter().flatten() {
        let bytes = parity_source_bytes(slot, parity)?;
        for (idx, byte) in bytes.iter().enumerate().take(parity.max_len) {
            recovered[idx] ^= byte;
        }
    }
    Some(recovered)
}

fn recover_reed_solomon(
    group: &FecDecodeGroup,
    parity: &FecParity,
    missing_idx: usize,
) -> Option<Vec<u8>> {
    let data_shards = group.slots.len();
    let rs = reed_solomon_erasure::galois_8::ReedSolomon::new(data_shards, 1).ok()?;
    let mut shards: Vec<Option<Vec<u8>>> = group
        .slots
        .iter()
        .map(|slot| {
            slot.as_ref().and_then(|bytes| {
                let bytes = parity_source_bytes(bytes, parity)?;
                let mut shard = vec![0u8; parity.max_len];
                shard[..bytes.len()].copy_from_slice(&bytes);
                Some(shard)
            })
        })
        .collect();
    shards.push(Some(parity.parity.to_vec()));
    rs.reconstruct(&mut shards).ok()?;
    shards.get_mut(missing_idx)?.take()
}

fn parity_source_bytes(raw: &Bytes, parity: &FecParity) -> Option<Bytes> {
    if parity.dest.is_some() {
        return decode_udp_datagram(raw).ok().map(|dg| dg.data);
    }
    Some(raw.clone())
}

fn mode_byte(mode: FecMode) -> u8 {
    match mode {
        FecMode::Xor1OfN => 1,
        FecMode::ReedSolomon => 2,
        FecMode::RaptorLike => 3,
        FecMode::Auto | FecMode::Off => 0,
    }
}

fn mode_from_byte(mode: u8) -> Option<FecMode> {
    match mode {
        1 => Some(FecMode::Xor1OfN),
        2 => Some(FecMode::ReedSolomon),
        3 => Some(FecMode::RaptorLike),
        _ => None,
    }
}

/// Record one classified datagram packet for transport metrics.
pub fn record_datagram_packet(class: &'static str, direction: &'static str) {
    metrics::counter!(
        "blackwire_datagram_packets_total",
        "class" => class,
        "direction" => direction
    )
    .increment(1);
}

/// Record one datagram fallback reason for transport metrics.
pub fn record_datagram_fallback(reason: &'static str) {
    metrics::counter!("blackwire_datagram_fallback_total", "reason" => reason).increment(1);
}

fn record_fec_mode(mode: FecMode) {
    metrics::counter!("blackwire_fec_mode_total", "mode" => mode.as_label()).increment(1);
}

fn record_fec_recovered() {
    metrics::counter!("blackwire_fec_recovered_packets_total").increment(1);
}

fn record_fec_overhead(bytes: usize) {
    metrics::counter!("blackwire_fec_overhead_bytes_total").increment(bytes as u64);
}

fn record_fec_stale_drop() {
    metrics::counter!("blackwire_fec_stale_drops_total").increment(1);
}

fn record_fec_duplicate_safe_skip(reason: &'static str) {
    metrics::counter!("blackwire_fec_duplicate_safe_skip_total", "reason" => reason).increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dg(dest: Destination) -> UdpDatagram {
        UdpDatagram {
            session_id: 0x1234_5678,
            packet_id: 42,
            frag_id: 0,
            frag_num: 1,
            dest,
            data: Bytes::from_static(b"hello world"),
        }
    }

    #[test]
    fn udp_datagram_ipv4_roundtrip() {
        let dg = make_dg(Destination::V4("192.168.1.1".parse().unwrap(), 53));
        let encoded = encode_udp_datagram(&dg);
        let decoded = decode_udp_datagram(&encoded).unwrap();
        assert_eq!(dg, decoded);
    }

    #[test]
    fn udp_datagram_ipv6_roundtrip() {
        let dg = make_dg(Destination::V6("::1".parse().unwrap(), 5353));
        let encoded = encode_udp_datagram(&dg);
        let decoded = decode_udp_datagram(&encoded).unwrap();
        assert_eq!(dg, decoded);
    }

    #[test]
    fn udp_datagram_domain_roundtrip() {
        let dg = make_dg(Destination::Domain("dns.google".to_string(), 53));
        let encoded = encode_udp_datagram(&dg);
        let decoded = decode_udp_datagram(&encoded).unwrap();
        assert_eq!(dg, decoded);
    }

    #[test]
    fn truncated_datagram_returns_error() {
        assert!(decode_udp_datagram(&[0u8; 4]).is_err());
    }

    #[test]
    fn xor_fec_recovers_one_missing_datagram() {
        let policy = FecPolicy {
            mode: FecMode::Xor1OfN,
            max_overhead_percent: 255,
            group_size: 4,
            disable_for_sequential_dns: false,
            min_concurrency_for_block_fec: 1,
            ..FecPolicy::default()
        };
        let mut encoder = FecEncoder::new(policy);
        let mut originals = Vec::new();
        let mut parity = None;
        for packet_id in 0..4 {
            let dg = UdpDatagram {
                session_id: 7,
                packet_id,
                frag_id: 0,
                frag_num: 1,
                dest: Destination::V4("127.0.0.1".parse().unwrap(), 53),
                data: Bytes::from(format!("payload-{packet_id}")),
            };
            let encoded = encode_udp_datagram(&dg);
            parity = encoder.protect(&dg, &encoded).or(parity);
            originals.push(encoded);
        }

        let mut decoder = FecDecoder::new(policy);
        for idx in [0usize, 1, 3] {
            let _ = decoder.decode(originals[idx].clone());
        }
        let recovered = decoder.decode(parity.expect("parity datagram"));
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].packet_id, 2);
        assert_eq!(recovered[0].data.as_ref(), b"payload-2");
    }

    #[test]
    fn reed_solomon_fec_recovers_one_missing_datagram() {
        let policy = FecPolicy {
            mode: FecMode::ReedSolomon,
            max_overhead_percent: 255,
            group_size: 4,
            disable_for_sequential_dns: false,
            min_concurrency_for_block_fec: 1,
            ..FecPolicy::default()
        };
        let mut encoder = FecEncoder::new(policy);
        let mut originals = Vec::new();
        let mut parity = None;
        for packet_id in 0..4 {
            let dg = UdpDatagram {
                session_id: 9,
                packet_id,
                frag_id: 0,
                frag_num: 1,
                dest: Destination::V4("127.0.0.1".parse().unwrap(), 53),
                data: Bytes::from(format!("rs-payload-{packet_id}")),
            };
            let encoded = encode_udp_datagram(&dg);
            parity = encoder.protect(&dg, &encoded).or(parity);
            originals.push(encoded);
        }

        let mut decoder = FecDecoder::new(policy);
        for idx in [0usize, 2, 3] {
            let _ = decoder.decode(originals[idx].clone());
        }
        let recovered = decoder.decode(parity.expect("parity datagram"));
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].packet_id, 1);
        assert_eq!(recovered[0].data.as_ref(), b"rs-payload-1");
    }

    #[test]
    fn auto_fec_is_conservative_without_loss_classifier() {
        let policy = FecPolicy {
            mode: FecMode::Auto,
            max_overhead_percent: 20,
            group_size: 4,
            ..FecPolicy::default()
        };
        assert!(!policy.enabled());
        assert_eq!(FecMode::Auto.effective(20), FecMode::Off);
        assert_eq!(policy.effective_group_size(), 5);
        let dns = Destination::V4("127.0.0.1".parse().unwrap(), 53);
        assert!(policy.effective_group_size_for(64, 80, &dns) > 5);
    }

    #[test]
    fn h2_plus_keeps_bulk_udp_unreliable() {
        let policy = DatagramPolicy {
            mode: DatagramPriorityMode::H2Plus,
            ..DatagramPolicy::default()
        };
        let dest = Destination::V4("127.0.0.1".parse().unwrap(), 443);
        assert_eq!(policy.lane_for(&dest, 1200), DatagramLane::Unreliable);
    }

    #[test]
    fn packet_classifier_marks_dns_and_bulk() {
        let dns = Destination::V4("127.0.0.1".parse().unwrap(), 53);
        let bulk = Destination::V4("127.0.0.1".parse().unwrap(), 443);
        assert_eq!(packet_class_for(&dns, 1200), PacketClass::Dns);
        assert_eq!(packet_class_for(&bulk, 64), PacketClass::Interactive);
        assert_eq!(packet_class_for(&bulk, 1200), PacketClass::Bulk);
    }

    #[test]
    fn fec_drops_recovery_after_interactive_deadline() {
        let policy = FecPolicy {
            mode: FecMode::Xor1OfN,
            max_overhead_percent: 255,
            group_size: 4,
            disable_for_sequential_dns: false,
            min_concurrency_for_block_fec: 1,
            ..FecPolicy::default()
        };
        let mut encoder = FecEncoder::new(policy);
        let mut originals = Vec::new();
        let mut parity = None;
        for packet_id in 0..4 {
            let dg = UdpDatagram {
                session_id: 11,
                packet_id,
                frag_id: 0,
                frag_num: 1,
                dest: Destination::V4("127.0.0.1".parse().unwrap(), 53),
                data: Bytes::from(format!("late-payload-{packet_id}")),
            };
            let encoded = encode_udp_datagram(&dg);
            parity = encoder.protect(&dg, &encoded).or(parity);
            originals.push(encoded);
        }

        let mut decoder = FecDecoder::new(policy);
        let _ = decoder.decode(originals[0].clone());
        let key = (11, 0);
        decoder.groups.get_mut(&key).expect("decode group").created =
            Instant::now() - FEC_RECOVERY_DEADLINE - Duration::from_millis(1);
        for idx in [1usize, 3] {
            let _ = decoder.decode(originals[idx].clone());
        }

        let recovered = decoder.decode(parity.expect("parity datagram"));
        assert!(recovered.is_empty());
    }

    #[test]
    fn fec_skips_sequential_dns_until_concurrency_threshold() {
        let policy = FecPolicy {
            mode: FecMode::Xor1OfN,
            max_overhead_percent: 255,
            group_size: 4,
            disable_for_sequential_dns: true,
            min_concurrency_for_block_fec: 4,
            ..FecPolicy::default()
        };
        let mut encoder = FecEncoder::new(policy);
        for packet_id in 0..4 {
            let dg = UdpDatagram {
                session_id: 100,
                packet_id,
                frag_id: 0,
                frag_num: 1,
                dest: Destination::V4("127.0.0.1".parse().unwrap(), 53),
                data: Bytes::from(format!("dns-{packet_id}")),
            };
            let encoded = encode_udp_datagram(&dg);
            assert!(encoder.protect(&dg, &encoded).is_none());
        }
    }

    #[test]
    fn fec_late_original_after_recovery_is_deduped() {
        let policy = FecPolicy {
            mode: FecMode::Xor1OfN,
            max_overhead_percent: 255,
            group_size: 4,
            disable_for_sequential_dns: false,
            min_concurrency_for_block_fec: 1,
            ..FecPolicy::default()
        };
        let mut encoder = FecEncoder::new(policy);
        let mut originals = Vec::new();
        let mut parity = None;
        for packet_id in 0..4 {
            let dg = UdpDatagram {
                session_id: 12,
                packet_id,
                frag_id: 0,
                frag_num: 1,
                dest: Destination::V4("127.0.0.1".parse().unwrap(), 53),
                data: Bytes::from(format!("dedup-payload-{packet_id}")),
            };
            let encoded = encode_udp_datagram(&dg);
            parity = encoder.protect(&dg, &encoded).or(parity);
            originals.push(encoded);
        }

        let mut decoder = FecDecoder::new(policy);
        for idx in [0usize, 1, 3] {
            let _ = decoder.decode(originals[idx].clone());
        }
        let recovered = decoder.decode(parity.expect("parity datagram"));
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].packet_id, 2);
        let late_original = decoder.decode(originals[2].clone());
        assert!(late_original.is_empty());
    }

    #[test]
    fn fec_group_size_accounts_for_tiny_packet_wire_overhead() {
        let policy = FecPolicy {
            mode: FecMode::ReedSolomon,
            max_overhead_percent: 20,
            group_size: 4,
            disable_for_sequential_dns: false,
            min_concurrency_for_block_fec: 1,
            ..FecPolicy::default()
        };
        let mut encoder = FecEncoder::new(policy);
        let mut parity = None;
        let mut emitted_after = 0usize;
        for packet_id in 0..u8::MAX as u16 {
            let dg = UdpDatagram {
                session_id: 13,
                packet_id,
                frag_id: 0,
                frag_num: 1,
                dest: Destination::V4("127.0.0.1".parse().unwrap(), 53),
                data: Bytes::from(vec![packet_id as u8; 64]),
            };
            let encoded = encode_udp_datagram(&dg);
            if let Some(repair) = encoder.protect(&dg, &encoded) {
                emitted_after = packet_id as usize + 1;
                parity = Some(repair);
                break;
            }
        }

        let parity = parity.expect("parity datagram");
        assert!(emitted_after > 5);
        assert!(parity.len() * 100 <= emitted_after * 64 * 20);
    }
}
