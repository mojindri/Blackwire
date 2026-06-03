//! Bad-network QUIC policy primitives for Hysteria2.
//!
//! These helpers keep the Hysteria2 badnet behavior testable without binding
//! every decision directly to Quinn internals.

use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;

const MIN_WINDOW: u64 = 32 * 1024;
const DEFAULT_INITIAL_RTT: Duration = Duration::from_millis(100);
const LOSS_WINDOW_SECS: usize = 5;
const IDLE_RESET_AFTER: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CongestionDirection {
    ClientUpload,
    ServerDownload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CongestionMode {
    StandardQuic,
    #[default]
    BrutalCompatible,
    NovaCc,
    BadNetLowLatency,
    BadNetThroughput,
    AutoProbe,
}

impl CongestionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StandardQuic => "standard-quic",
            Self::BrutalCompatible => "brutal-compatible",
            Self::NovaCc => "nova-cc",
            Self::BadNetLowLatency => "badnet-low-latency",
            Self::BadNetThroughput => "badnet-throughput",
            Self::AutoProbe => "auto-probe",
        }
    }
}

impl std::str::FromStr for CongestionMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "standard" | "standard-quic" | "quic" => Ok(Self::StandardQuic),
            "brutal" | "brutal-compatible" => Ok(Self::BrutalCompatible),
            "nova" | "nova-cc" => Ok(Self::NovaCc),
            "badnet-low-latency" | "low-latency" => Ok(Self::BadNetLowLatency),
            "badnet-throughput" | "throughput" => Ok(Self::BadNetThroughput),
            "auto" | "auto-probe" => Ok(Self::AutoProbe),
            other => Err(format!("unsupported Hysteria2 congestion mode '{other}'")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CongestionConfig {
    pub mode: CongestionMode,
    pub up_mbps: u64,
    pub down_mbps: u64,
    pub min_ack_rate: f64,
    pub max_queue_delay: Duration,
    pub pacing_gain: f64,
    pub loss_compensation: bool,
}

impl CongestionConfig {
    pub fn target_bps(&self) -> u64 {
        self.target_bps_for(CongestionDirection::ClientUpload)
    }

    pub fn target_bps_for(&self, direction: CongestionDirection) -> u64 {
        let mbps = match direction {
            CongestionDirection::ClientUpload => self.up_mbps,
            CongestionDirection::ServerDownload => self.down_mbps,
        };
        mbps.saturating_mul(1_000_000 / 8)
    }

    pub fn window_profile(&self) -> WindowProfile {
        match self.mode {
            CongestionMode::BadNetLowLatency => WindowProfile {
                bdp_rtt: Duration::from_millis(150),
                min_window_bytes: 1024 * 1024,
                max_window_bytes: 32 * 1024 * 1024,
                conn_window_multiplier: 2,
            },
            _ => WindowProfile {
                bdp_rtt: Duration::from_millis(500),
                min_window_bytes: 4 * 1024 * 1024,
                max_window_bytes: 128 * 1024 * 1024,
                conn_window_multiplier: 3,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WindowProfile {
    pub bdp_rtt: Duration,
    pub min_window_bytes: u64,
    pub max_window_bytes: u64,
    pub conn_window_multiplier: u64,
}

impl Default for CongestionConfig {
    fn default() -> Self {
        Self {
            mode: CongestionMode::BrutalCompatible,
            up_mbps: 100,
            down_mbps: 100,
            min_ack_rate: 0.8,
            max_queue_delay: Duration::from_millis(80),
            pacing_gain: 1.25,
            loss_compensation: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossFingerprint {
    Clean,
    WirelessRandomLoss,
    Bufferbloat,
    SevereLoss,
    Unknown,
}

impl LossFingerprint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::WirelessRandomLoss => "wireless-random-loss",
            Self::Bufferbloat => "bufferbloat",
            Self::SevereLoss => "severe-loss",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for LossFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PathSample {
    pub acked_bytes: u64,
    pub lost_bytes: u64,
    pub acked_packets: u64,
    pub lost_packets: u64,
    pub min_rtt: Duration,
    pub srtt: Duration,
    pub delivery_rate_bps: u64,
}

impl PathSample {
    pub fn loss_rate(self) -> f64 {
        let total = self.acked_packets.saturating_add(self.lost_packets);
        if total == 0 {
            let total = self.acked_bytes.saturating_add(self.lost_bytes);
            if total == 0 {
                0.0
            } else {
                self.lost_bytes as f64 / total as f64
            }
        } else {
            self.lost_packets as f64 / total as f64
        }
    }

    pub fn ack_rate(self) -> f64 {
        1.0 - self.loss_rate()
    }

    pub fn queue_delay(self) -> Duration {
        self.srtt.saturating_sub(self.min_rtt)
    }
}

pub fn classify_loss(sample: PathSample, queue_budget: Duration) -> LossFingerprint {
    let loss_rate = sample.loss_rate();
    let queue_delay = sample.queue_delay();
    if loss_rate < 0.005 && queue_delay <= queue_budget / 2 {
        LossFingerprint::Clean
    } else if loss_rate >= 0.08 {
        LossFingerprint::SevereLoss
    } else if queue_delay > queue_budget {
        LossFingerprint::Bufferbloat
    } else if loss_rate >= 0.01 {
        LossFingerprint::WirelessRandomLoss
    } else {
        LossFingerprint::Unknown
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ControlDecision {
    pub ack_rate: f64,
    pub loss_rate: f64,
    pub queue_delay: Duration,
    pub pacing_rate_bps: u64,
    pub cwnd_bytes: u64,
    pub fingerprint: LossFingerprint,
}

pub fn brutal_compatible_decision(
    cfg: &CongestionConfig,
    direction: CongestionDirection,
    sample: PathSample,
) -> ControlDecision {
    let ack_rate = sample.ack_rate().max(cfg.min_ack_rate).clamp(0.05, 1.0);
    let rtt = sample.srtt.as_secs_f64().max(0.001);
    let target = cfg.target_bps_for(direction) as f64;
    let pacing_rate_bps = (target / ack_rate) as u64;
    let cwnd_bytes = ((target * rtt * cfg.pacing_gain) / ack_rate) as u64;
    ControlDecision {
        ack_rate,
        loss_rate: sample.loss_rate(),
        queue_delay: sample.queue_delay(),
        pacing_rate_bps,
        cwnd_bytes: cwnd_bytes.max(MIN_WINDOW),
        fingerprint: classify_loss(sample, cfg.max_queue_delay),
    }
}

pub fn nova_decision(
    cfg: &CongestionConfig,
    direction: CongestionDirection,
    sample: PathSample,
) -> ControlDecision {
    let fingerprint = classify_loss(sample, cfg.max_queue_delay);
    let mut gain = cfg.pacing_gain;
    match fingerprint {
        LossFingerprint::Clean => gain = gain.max(1.10),
        LossFingerprint::WirelessRandomLoss if cfg.loss_compensation => gain *= 1.10,
        LossFingerprint::SevereLoss if cfg.loss_compensation => gain *= 1.18,
        LossFingerprint::Bufferbloat => gain *= 0.72,
        LossFingerprint::Unknown => {}
        _ => {}
    }
    if sample.delivery_rate_bps > cfg.target_bps_for(direction) {
        gain *= 1.03;
    }

    let mut tuned = cfg.clone();
    tuned.pacing_gain = gain.clamp(0.50, 1.80);
    let mut decision = brutal_compatible_decision(&tuned, direction, sample);
    decision.fingerprint = fingerprint;
    decision
}

pub struct BadNetControllerFactory {
    cfg: CongestionConfig,
    direction: CongestionDirection,
}

impl BadNetControllerFactory {
    pub fn new(cfg: CongestionConfig) -> Self {
        Self {
            cfg,
            direction: CongestionDirection::ClientUpload,
        }
    }

    pub fn new_for_direction(cfg: CongestionConfig, direction: CongestionDirection) -> Self {
        Self { cfg, direction }
    }
}

impl ControllerFactory for BadNetControllerFactory {
    fn build(self: Arc<Self>, _now: Instant, current_mtu: u16) -> Box<dyn Controller> {
        Box::new(BadNetController {
            cfg: self.cfg.clone(),
            direction: self.direction,
            mtu: current_mtu,
            loss_window: LossWindow::default(),
            min_rtt: DEFAULT_INITIAL_RTT,
            srtt: DEFAULT_INITIAL_RTT,
            delivery_rate_bps: 0,
            start: _now,
            last_activity: _now,
            last_decision: brutal_compatible_decision(
                &self.cfg,
                self.direction,
                PathSample {
                    acked_bytes: 0,
                    lost_bytes: 0,
                    acked_packets: 0,
                    lost_packets: 0,
                    min_rtt: DEFAULT_INITIAL_RTT,
                    srtt: DEFAULT_INITIAL_RTT,
                    delivery_rate_bps: 0,
                },
            ),
        })
    }
}

#[derive(Clone)]
pub struct BadNetController {
    cfg: CongestionConfig,
    direction: CongestionDirection,
    mtu: u16,
    loss_window: LossWindow,
    min_rtt: Duration,
    srtt: Duration,
    delivery_rate_bps: u64,
    start: Instant,
    last_activity: Instant,
    last_decision: ControlDecision,
}

impl BadNetController {
    fn sample(&self) -> PathSample {
        let totals = self.loss_window.totals();
        PathSample {
            acked_bytes: totals.acked_bytes,
            lost_bytes: totals.lost_bytes,
            acked_packets: totals.acked_packets,
            lost_packets: totals.lost_packets,
            min_rtt: self.min_rtt,
            srtt: self.srtt,
            delivery_rate_bps: self.delivery_rate_bps,
        }
    }

    fn recompute(&mut self, now: Instant) {
        if now.saturating_duration_since(self.last_activity) > IDLE_RESET_AFTER {
            self.loss_window.reset();
        }
        self.last_activity = now;
        self.last_decision = match self.cfg.mode {
            CongestionMode::NovaCc
            | CongestionMode::BadNetLowLatency
            | CongestionMode::AutoProbe => nova_decision(&self.cfg, self.direction, self.sample()),
            CongestionMode::BadNetThroughput | CongestionMode::BrutalCompatible => {
                brutal_compatible_decision(&self.cfg, self.direction, self.sample())
            }
            CongestionMode::StandardQuic => {
                brutal_compatible_decision(&self.cfg, self.direction, self.sample())
            }
        };
        record_metrics(self.cfg.mode, self.last_decision);
    }

    fn slot_second(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.start).as_secs()
    }
}

impl Controller for BadNetController {
    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        lost_bytes: u64,
    ) {
        let packets = packets_from_bytes(lost_bytes, self.mtu);
        self.loss_window
            .record_loss(self.slot_second(_now), lost_bytes, packets);
        self.recompute(_now);
    }

    fn on_mtu_update(&mut self, new_mtu: u16) {
        self.mtu = new_mtu;
    }

    fn window(&self) -> u64 {
        self.last_decision.cwnd_bytes.max(self.mtu as u64 * 4)
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn initial_window(&self) -> u64 {
        self.window()
    }

    fn on_ack(
        &mut self,
        now: Instant,
        sent: Instant,
        bytes: u64,
        _app_limited: bool,
        rtt: &RttEstimator,
    ) {
        if now.saturating_duration_since(self.last_activity) > IDLE_RESET_AFTER {
            self.loss_window.reset();
        }
        let packets = packets_from_bytes(bytes, self.mtu);
        self.loss_window
            .record_ack(self.slot_second(now), bytes, packets);
        self.srtt = rtt.get().max(Duration::from_millis(1));
        self.min_rtt = self.min_rtt.min(self.srtt);
        let elapsed = now.saturating_duration_since(sent).as_secs_f64().max(0.001);
        self.delivery_rate_bps = (bytes as f64 / elapsed) as u64;
        self.recompute(now);
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

fn packets_from_bytes(bytes: u64, mtu: u16) -> u64 {
    let mtu = u64::from(mtu).max(1);
    (bytes / mtu).max(1)
}

#[derive(Debug, Clone, Copy, Default)]
struct LossSlot {
    second: u64,
    acked_packets: u64,
    lost_packets: u64,
    acked_bytes: u64,
    lost_bytes: u64,
}

#[derive(Debug, Clone, Default)]
struct LossWindow {
    slots: [LossSlot; LOSS_WINDOW_SECS],
}

impl LossWindow {
    fn reset(&mut self) {
        self.slots = [LossSlot::default(); LOSS_WINDOW_SECS];
    }

    fn record_ack(&mut self, second: u64, bytes: u64, packets: u64) {
        let slot = self.slot(second);
        slot.acked_bytes = slot.acked_bytes.saturating_add(bytes);
        slot.acked_packets = slot.acked_packets.saturating_add(packets);
    }

    fn record_loss(&mut self, second: u64, bytes: u64, packets: u64) {
        let slot = self.slot(second);
        slot.lost_bytes = slot.lost_bytes.saturating_add(bytes);
        slot.lost_packets = slot.lost_packets.saturating_add(packets);
    }

    fn totals(&self) -> LossSlot {
        self.slots
            .iter()
            .fold(LossSlot::default(), |mut acc, slot| {
                acc.acked_bytes = acc.acked_bytes.saturating_add(slot.acked_bytes);
                acc.lost_bytes = acc.lost_bytes.saturating_add(slot.lost_bytes);
                acc.acked_packets = acc.acked_packets.saturating_add(slot.acked_packets);
                acc.lost_packets = acc.lost_packets.saturating_add(slot.lost_packets);
                acc
            })
    }

    fn slot(&mut self, second: u64) -> &mut LossSlot {
        let idx = second as usize % LOSS_WINDOW_SECS;
        let slot = &mut self.slots[idx];
        if slot.second != second {
            *slot = LossSlot {
                second,
                ..LossSlot::default()
            };
        }
        slot
    }
}

pub fn record_mode(mode: CongestionMode) {
    metrics::counter!("blackwire_quic_congestion_mode_total", "mode" => mode.as_str()).increment(1);
}

pub fn record_endpoint_shards(shards: usize) {
    metrics::gauge!("blackwire_quic_endpoint_shards").set(shards as f64);
}

fn record_metrics(mode: CongestionMode, decision: ControlDecision) {
    metrics::gauge!("blackwire_quic_ack_rate", "mode" => mode.as_str()).set(decision.ack_rate);
    metrics::gauge!("blackwire_quic_loss_rate", "mode" => mode.as_str()).set(decision.loss_rate);
    metrics::gauge!("blackwire_quic_queue_delay_ms", "mode" => mode.as_str())
        .set(decision.queue_delay.as_secs_f64() * 1000.0);
    metrics::gauge!("blackwire_quic_pacing_rate_bps", "mode" => mode.as_str())
        .set(decision.pacing_rate_bps as f64);
    metrics::gauge!("blackwire_quic_cwnd_bytes", "mode" => mode.as_str())
        .set(decision.cwnd_bytes as f64);
    metrics::gauge!("blackwire_quic_delivery_rate_bps", "mode" => mode.as_str())
        .set(decision.pacing_rate_bps as f64);
    metrics::counter!(
        "blackwire_quic_loss_fingerprint_total",
        "fingerprint" => decision.fingerprint.as_str()
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> CongestionConfig {
        CongestionConfig {
            up_mbps: 100,
            down_mbps: 100,
            ..CongestionConfig::default()
        }
    }

    #[test]
    fn brutal_compatible_compensates_for_loss_with_ack_rate() {
        let sample = PathSample {
            acked_bytes: 80,
            lost_bytes: 20,
            acked_packets: 80,
            lost_packets: 20,
            min_rtt: Duration::from_millis(40),
            srtt: Duration::from_millis(50),
            delivery_rate_bps: 9_000_000,
        };
        let decision =
            brutal_compatible_decision(&cfg(), CongestionDirection::ClientUpload, sample);
        assert_eq!(decision.ack_rate, 0.8);
        assert!(decision.pacing_rate_bps > cfg().target_bps());
        assert!(decision.cwnd_bytes >= MIN_WINDOW);
    }

    #[test]
    fn loss_classifier_distinguishes_bufferbloat_from_wireless_loss() {
        let wireless = PathSample {
            acked_bytes: 98,
            lost_bytes: 2,
            acked_packets: 98,
            lost_packets: 2,
            min_rtt: Duration::from_millis(40),
            srtt: Duration::from_millis(45),
            delivery_rate_bps: 1,
        };
        let bloated = PathSample {
            srtt: Duration::from_millis(180),
            ..wireless
        };
        assert_eq!(
            classify_loss(wireless, Duration::from_millis(80)),
            LossFingerprint::WirelessRandomLoss
        );
        assert_eq!(
            classify_loss(bloated, Duration::from_millis(80)),
            LossFingerprint::Bufferbloat
        );
    }

    #[test]
    fn nova_reduces_cwnd_when_queue_delay_exceeds_budget() {
        let bloated = PathSample {
            acked_bytes: 100,
            lost_bytes: 0,
            acked_packets: 100,
            lost_packets: 0,
            min_rtt: Duration::from_millis(40),
            srtt: Duration::from_millis(180),
            delivery_rate_bps: 12_000_000,
        };
        let cfg = CongestionConfig {
            mode: CongestionMode::NovaCc,
            ..cfg()
        };
        assert!(
            nova_decision(&cfg, CongestionDirection::ClientUpload, bloated).cwnd_bytes
                < brutal_compatible_decision(&cfg, CongestionDirection::ClientUpload, bloated)
                    .cwnd_bytes
        );
    }

    #[test]
    fn target_rate_is_direction_aware() {
        let cfg = CongestionConfig {
            up_mbps: 10,
            down_mbps: 100,
            ..cfg()
        };
        assert_eq!(
            cfg.target_bps_for(CongestionDirection::ClientUpload),
            1_250_000
        );
        assert_eq!(
            cfg.target_bps_for(CongestionDirection::ServerDownload),
            12_500_000
        );
    }
}
