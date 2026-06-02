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
const FEC_MAGIC: &[u8; 6] = b"BWFEC1";
const FEC_GROUP_SIZE: u8 = 4;
const FEC_GROUP_TTL: Duration = Duration::from_millis(750);
const FEC_RECOVERY_DEADLINE: Duration = Duration::from_millis(100);
const FEC_SEEN_LIMIT: usize = 4096;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum DatagramPriorityMode {
    #[default]
    Standard,
    H2Plus,
}


#[derive(Debug, Clone, Copy)]
pub struct DatagramPolicy {
    pub mode: DatagramPriorityMode,
    pub max_queue_delay_ms: u64,
    pub fast_dns_retry: bool,
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

    pub fn should_fast_retry_dns(&self, dest: &Destination) -> bool {
        self.mode == DatagramPriorityMode::H2Plus && self.fast_dns_retry && is_dns_like(dest)
    }
}

pub fn packet_class_for(dest: &Destination, payload_len: usize) -> PacketClass {
    if is_dns_like(dest) {
        PacketClass::Dns
    } else if payload_len <= 140 {
        PacketClass::Interactive
    } else {
        PacketClass::Bulk
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FecMode {
    Off,
    Xor1OfN,
    ReedSolomon,
    RaptorLike,
    Auto,
}

impl FecMode {
    pub fn effective(self, _max_overhead_percent: u8) -> Self {
        match self {
            Self::Auto => Self::Off,
            mode => mode,
        }
    }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FecPolicy {
    pub mode: FecMode,
    pub max_overhead_percent: u8,
    pub group_size: u8,
}

impl Default for FecPolicy {
    fn default() -> Self {
        Self {
            mode: FecMode::Off,
            max_overhead_percent: 20,
            group_size: FEC_GROUP_SIZE,
        }
    }
}

impl FecPolicy {
    pub fn enabled(self) -> bool {
        matches!(
            self.mode.effective(self.max_overhead_percent),
            FecMode::Xor1OfN | FecMode::ReedSolomon | FecMode::RaptorLike
        ) && self.max_overhead_percent >= 20
    }
}

#[derive(Debug)]
pub struct FecEncoder {
    policy: FecPolicy,
    groups: HashMap<(u32, u16), Vec<Option<Bytes>>>,
}

impl FecEncoder {
    pub fn new(policy: FecPolicy) -> Self {
        Self {
            policy,
            groups: HashMap::new(),
        }
    }

    pub fn protect(&mut self, original: &UdpDatagram, encoded: &Bytes) -> Option<Bytes> {
        if !self.policy.enabled() || original.frag_num != 1 {
            return None;
        }
        let group_size = self.policy.group_size.max(2);
        let base = original.packet_id - (original.packet_id % group_size as u16);
        let idx = (original.packet_id - base) as usize;
        let key = (original.session_id, base);
        let slots = self
            .groups
            .entry(key)
            .or_insert_with(|| vec![None; group_size as usize]);
        if idx >= slots.len() {
            return None;
        }
        slots[idx] = Some(encoded.clone());
        if slots.iter().all(Option::is_some) {
            let mode = self.policy.mode.effective(self.policy.max_overhead_percent);
            let parity = build_parity(mode, original.session_id, base, slots)?;
            self.groups.remove(&key);
            record_fec_mode(mode);
            record_fec_overhead(parity.len());
            return Some(parity);
        }
        None
    }
}

#[derive(Debug)]
pub struct FecDecoder {
    policy: FecPolicy,
    groups: HashMap<(u32, u16), FecDecodeGroup>,
    seen: HashSet<(u32, u16)>,
    seen_order: VecDeque<(u32, u16)>,
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
}

impl FecDecoder {
    pub fn new(policy: FecPolicy) -> Self {
        Self {
            policy,
            groups: HashMap::new(),
            seen: HashSet::new(),
            seen_order: VecDeque::new(),
        }
    }

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
        self.accept_original(&dg, raw);
        vec![dg]
    }

    fn accept_original(&mut self, dg: &UdpDatagram, raw: Bytes) {
        if !self.policy.enabled() || dg.frag_num != 1 {
            return;
        }
        let group_size = self.policy.group_size.max(2);
        let base = dg.packet_id - (dg.packet_id % group_size as u16);
        let idx = (dg.packet_id - base) as usize;
        let group = self
            .groups
            .entry((dg.session_id, base))
            .or_insert_with(|| FecDecodeGroup {
                created: Instant::now(),
                slots: vec![None; group_size as usize],
                parity: None,
            });
        if idx < group.slots.len() {
            group.slots[idx] = Some(raw);
        }
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
        record_fec_recovered();
        vec![recovered]
    }

    fn mark_seen(&mut self, key: (u32, u16)) -> bool {
        if !self.seen.insert(key) {
            return false;
        }
        self.seen_order.push_back(key);
        while self.seen_order.len() > FEC_SEEN_LIMIT {
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
        for _ in 0..dropped {
            record_fec_stale_drop();
        }
    }
}

fn build_parity(
    mode: FecMode,
    session_id: u32,
    base_packet_id: u16,
    slots: &[Option<Bytes>],
) -> Option<Bytes> {
    let max_len = slots
        .iter()
        .filter_map(|s| s.as_ref().map(Bytes::len))
        .max()?;
    let mode = mode.effective(100);
    let (lengths, parity) = match mode {
        FecMode::Xor1OfN | FecMode::RaptorLike => build_xor_parity(slots, max_len)?,
        FecMode::ReedSolomon => build_reed_solomon_parity(slots, max_len)?,
        FecMode::Auto | FecMode::Off => return None,
    };
    let mut payload = BytesMut::with_capacity(FEC_MAGIC.len() + 9 + lengths.len() * 2 + max_len);
    payload.put_slice(FEC_MAGIC);
    payload.put_u8(mode_byte(mode));
    payload.put_u16(base_packet_id);
    payload.put_u8(slots.len() as u8);
    payload.put_u16(max_len as u16);
    for len in lengths {
        payload.put_u16(len as u16);
    }
    payload.put_slice(&parity);

    let dg = UdpDatagram {
        session_id,
        packet_id: base_packet_id,
        frag_id: 0,
        frag_num: 1,
        dest: Destination::Domain(FEC_MARKER_DOMAIN.into(), 0),
        data: payload.freeze(),
    };
    Some(encode_udp_datagram(&dg))
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
    if !matches!(&dg.dest, Destination::Domain(name, 0) if name == FEC_MARKER_DOMAIN) {
        return None;
    }
    let mut data = dg.data.as_ref();
    if data.len() < FEC_MAGIC.len() + 6 || &data[..FEC_MAGIC.len()] != FEC_MAGIC {
        return None;
    }
    data.advance(FEC_MAGIC.len());
    let mode = mode_from_byte(data.get_u8())?;
    let base_packet_id = data.get_u16();
    let group_size = data.get_u8();
    let max_len = data.get_u16() as usize;
    if group_size < 2 || data.len() < group_size as usize * 2 + max_len {
        return None;
    }
    let mut lengths = Vec::with_capacity(group_size as usize);
    for _ in 0..group_size {
        lengths.push(data.get_u16() as usize);
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
    Some(Bytes::from(recovered))
}

fn recover_xor(group: &FecDecodeGroup, parity: &FecParity) -> Option<Vec<u8>> {
    let mut recovered = parity.parity.to_vec();
    for bytes in group.slots.iter().flatten() {
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
            slot.as_ref().map(|bytes| {
                let mut shard = vec![0u8; parity.max_len];
                shard[..bytes.len()].copy_from_slice(bytes);
                shard
            })
        })
        .collect();
    shards.push(Some(parity.parity.to_vec()));
    rs.reconstruct(&mut shards).ok()?;
    shards.get_mut(missing_idx)?.take()
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

pub fn record_datagram_packet(class: &'static str, direction: &'static str) {
    metrics::counter!(
        "blackwire_datagram_packets_total",
        "class" => class,
        "direction" => direction
    )
    .increment(1);
}

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
            max_overhead_percent: 25,
            group_size: 4,
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
            max_overhead_percent: 25,
            group_size: 4,
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
        };
        assert!(!policy.enabled());
        assert_eq!(FecMode::Auto.effective(20), FecMode::Off);
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
            max_overhead_percent: 25,
            group_size: 4,
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
}
