//! Experimental Linux AF_XDP backend bring-up.
//!
//! This module exposes a real AF_XDP socket/UMEM/ring backend that can be used
//! by privileged benchmarks and future runtime integration. It deliberately does
//! not replace the existing TUN runtime by default.

#[cfg(target_os = "linux")]
use std::ffi::CString;

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::Result;

use super::device::TunAfXdpConfig;

/// Snapshot of the NIC capabilities observed while opening an AF_XDP backend.
#[derive(Debug, Clone)]
pub struct AfXdpCapabilities {
    pub interface: String,
    pub interface_index: u32,
    pub queue_count: u32,
    pub queue_id: u32,
    pub zero_copy_available: bool,
}

/// Experimental AF_XDP backend handle.
#[cfg(target_os = "linux")]
pub struct AfXdpBackend {
    capabilities: AfXdpCapabilities,
    _socket: xdp::socket::XdpSocket,
    _rings: xdp::Rings,
    _umem: xdp::Umem,
}

#[cfg(target_os = "linux")]
impl std::fmt::Debug for AfXdpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AfXdpBackend")
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

#[cfg(target_os = "linux")]
impl AfXdpBackend {
    /// Open an AF_XDP socket bound to `config.interface` and `config.queue_id`.
    pub fn open(config: &TunAfXdpConfig) -> Result<Self> {
        let interface = config
            .interface
            .as_deref()
            .filter(|value| !value.is_empty())
            .context("AF_XDP requires a non-empty interface name")?;

        if config.force_copy && config.force_zerocopy {
            anyhow::bail!("AF_XDP cannot force both copy and zerocopy mode");
        }

        let iface = CString::new(interface).context("AF_XDP interface contains interior NUL")?;
        let nic = xdp::nic::NicIndex::lookup_by_name(&iface)?
            .with_context(|| format!("AF_XDP interface '{interface}' was not found"))?;
        let capabilities = nic
            .query_capabilities()
            .with_context(|| format!("AF_XDP capability query failed for '{interface}'"))?;

        if config.queue_id >= capabilities.queue_count {
            anyhow::bail!(
                "AF_XDP queue {} is out of range for '{}' (queue_count={})",
                config.queue_id,
                interface,
                capabilities.queue_count
            );
        }

        let frame_size = match config.frame_size {
            2048 => xdp::umem::FrameSize::TwoK,
            4096 => xdp::umem::FrameSize::FourK,
            other => anyhow::bail!("AF_XDP frame_size must be 2048 or 4096 bytes, got {other}"),
        };

        let umem_cfg = xdp::umem::UmemCfgBuilder {
            frame_count: config.frame_count,
            frame_size,
            ..Default::default()
        }
        .build()
        .context("AF_XDP UMEM configuration is invalid")?;
        let mut umem = xdp::Umem::map(umem_cfg).context("AF_XDP UMEM map failed")?;

        let ring_cfg = xdp::RingConfigBuilder {
            rx_count: config.ring_entries,
            tx_count: config.ring_entries,
            fill_count: config.ring_entries,
            completion_count: config.ring_entries,
        }
        .build()
        .context("AF_XDP ring configuration is invalid")?;

        let mut builder =
            xdp::socket::XdpSocketBuilder::new().context("AF_XDP socket creation failed")?;
        let (mut rings, mut bind_flags) = builder
            .build_rings(&umem, ring_cfg)
            .context("AF_XDP ring setup failed")?;
        if config.force_copy {
            bind_flags.force_copy();
        } else if config.force_zerocopy {
            bind_flags.force_zerocopy();
        }
        let socket = builder
            .bind(nic, config.queue_id, bind_flags)
            .context("AF_XDP bind failed")?;

        // Hand the kernel initial receive buffers immediately so a privileged
        // smoke test can verify the queue is live.
        let initial_fill = config.ring_entries.min(config.frame_count) as usize;
        unsafe {
            let queued = rings.fill_ring.enqueue(&mut umem, initial_fill);
            if queued == 0 {
                anyhow::bail!("AF_XDP fill ring could not queue any UMEM frames");
            }
        }

        Ok(Self {
            capabilities: AfXdpCapabilities {
                interface: interface.to_string(),
                interface_index: nic.0,
                queue_count: capabilities.queue_count,
                queue_id: config.queue_id,
                zero_copy_available: capabilities.zero_copy.is_available(),
            },
            _socket: socket,
            _rings: rings,
            _umem: umem,
        })
    }

    pub fn capabilities(&self) -> &AfXdpCapabilities {
        &self.capabilities
    }
}

/// Non-Linux builds expose a stub so config/schema users still compile cleanly.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct AfXdpBackend;

#[cfg(not(target_os = "linux"))]
impl AfXdpBackend {
    pub fn open(_config: &TunAfXdpConfig) -> Result<Self> {
        anyhow::bail!("AF_XDP backend is only available on Linux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn af_xdp_requires_interface_name() {
        let err = AfXdpBackend::open(&TunAfXdpConfig::default()).unwrap_err();
        assert!(err.to_string().contains("interface"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn af_xdp_rejects_conflicting_copy_modes() {
        let err = AfXdpBackend::open(&TunAfXdpConfig {
            interface: Some("eth0".into()),
            force_copy: true,
            force_zerocopy: true,
            ..TunAfXdpConfig::default()
        })
        .unwrap_err();
        assert!(err.to_string().contains("both copy and zerocopy"));
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn af_xdp_is_explicitly_unsupported_off_linux() {
        let err = AfXdpBackend::open(&TunAfXdpConfig::default()).unwrap_err();
        assert!(err.to_string().contains("only available on Linux"));
    }
}
