use std::time::{Duration, Instant};

/// Runtime knobs for batching packets written back to the TUN device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunBatchConfig {
    pub enabled: bool,
    /// Maximum number of packets to accumulate before flushing.
    pub max_packets: usize,
    /// Maximum time to hold a batch before flushing regardless of packet count.
    pub max_delay: Duration,
    /// Flush immediately when a single packet exceeds this byte threshold (0 = disabled).
    pub latency_flush_bytes: usize,
}

impl Default for TunBatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_packets: 32,
            max_delay: Duration::from_micros(750),
            latency_flush_bytes: 256,
        }
    }
}

impl TunBatchConfig {
    /// Return a copy of this config with all fields clamped to safe operating ranges.
    pub fn normalized(self) -> Self {
        Self {
            enabled: self.enabled,
            max_packets: self.max_packets.clamp(1, 256),
            max_delay: self.max_delay.min(Duration::from_millis(10)),
            latency_flush_bytes: if self.latency_flush_bytes == 0 {
                0
            } else {
                self.latency_flush_bytes.clamp(64, 4096)
            },
        }
    }
}

/// Small bounded packet batch used by the TUN runtime write side.
#[derive(Debug)]
pub struct TunPacketBatch {
    config: TunBatchConfig,
    packets: Vec<Vec<u8>>,
    first_packet_at: Option<Instant>,
}

impl TunPacketBatch {
    /// Create a new batch using the given config (normalized on construction).
    pub fn new(config: TunBatchConfig) -> Self {
        let config = config.normalized();
        Self {
            packets: Vec::with_capacity(config.max_packets),
            first_packet_at: None,
            config,
        }
    }

    /// Append a packet to the batch, recording the arrival time of the first packet.
    pub fn push(&mut self, packet: Vec<u8>, now: Instant) {
        if self.packets.is_empty() {
            self.first_packet_at = Some(now);
        }
        self.packets.push(packet);
    }

    /// Returns `true` if the batch should be flushed now based on packet count, delay, or size.
    pub fn should_flush(&self, now: Instant) -> bool {
        if self.packets.is_empty() {
            return false;
        }
        if !self.config.enabled || self.packets.len() >= self.config.max_packets {
            return true;
        }
        if self.config.latency_flush_bytes > 0
            && self
                .packets
                .first()
                .is_some_and(|packet| packet.len() <= self.config.latency_flush_bytes)
        {
            return true;
        }
        self.first_packet_at
            .is_some_and(|first| now.duration_since(first) >= self.config.max_delay)
    }

    /// Drain all buffered packets and reset the batch, returning them as an iterator.
    pub fn drain(&mut self) -> impl Iterator<Item = Vec<u8>> + '_ {
        self.first_packet_at = None;
        self.packets.drain(..)
    }

    /// Returns the number of packets currently buffered in the batch.
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    /// Returns `true` if the batch contains no buffered packets.
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_flushes_on_packet_cap() {
        let mut batch = TunPacketBatch::new(TunBatchConfig {
            enabled: true,
            max_packets: 2,
            max_delay: Duration::from_secs(1),
            latency_flush_bytes: 0,
        });
        let now = Instant::now();
        batch.push(vec![1], now);
        assert!(!batch.should_flush(now));
        batch.push(vec![2], now);
        assert!(batch.should_flush(now));
    }

    #[test]
    fn batch_flushes_on_delay() {
        let mut batch = TunPacketBatch::new(TunBatchConfig {
            enabled: true,
            max_packets: 16,
            max_delay: Duration::from_millis(1),
            latency_flush_bytes: 0,
        });
        let now = Instant::now();
        batch.push(vec![1], now);
        assert!(batch.should_flush(now + Duration::from_millis(2)));
    }

    #[test]
    fn batch_flushes_latency_sized_packets_immediately() {
        let mut batch = TunPacketBatch::new(TunBatchConfig {
            enabled: true,
            max_packets: 16,
            max_delay: Duration::from_secs(1),
            latency_flush_bytes: 256,
        });
        let now = Instant::now();
        batch.push(vec![0; 96], now);
        assert!(batch.should_flush(now));
    }
}
