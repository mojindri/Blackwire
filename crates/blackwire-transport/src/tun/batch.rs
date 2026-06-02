use std::time::{Duration, Instant};

/// Runtime knobs for batching packets written back to the TUN device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunBatchConfig {
    pub enabled: bool,
    pub max_packets: usize,
    pub max_delay: Duration,
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
    pub fn new(config: TunBatchConfig) -> Self {
        let config = config.normalized();
        Self {
            packets: Vec::with_capacity(config.max_packets),
            first_packet_at: None,
            config,
        }
    }

    pub fn push(&mut self, packet: Vec<u8>, now: Instant) {
        if self.packets.is_empty() {
            self.first_packet_at = Some(now);
        }
        self.packets.push(packet);
    }

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

    pub fn drain(&mut self) -> impl Iterator<Item = Vec<u8>> + '_ {
        self.first_packet_at = None;
        self.packets.drain(..)
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

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
