use std::time::Duration;

use anyhow::Result;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use tracing::info;
#[cfg(target_os = "macos")]
use tun::AbstractDevice;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use super::backend::current_tun_support;

/// Platform TUN device type used by [`crate::tun::TunRuntime`].
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub type TunDevice = tun::AsyncDevice;

/// Placeholder device type for platforms whose TUN backend is not implemented yet.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[derive(Debug)]
pub struct TunDevice;

use super::batch::TunBatchConfig;

/// Settings used when creating the OS TUN interface.
#[derive(Debug, Clone)]
pub struct TunConfig {
    /// Interface name (for example `blackwire-tun`).
    pub name: String,
    /// IPv4 address assigned to the TUN interface.
    pub address: std::net::Ipv4Addr,
    /// IPv4 netmask assigned to the TUN interface.
    pub netmask: std::net::Ipv4Addr,
    /// MTU for the interface.
    pub mtu: u16,
    /// Packet mark used to bypass TUN redirection rules.
    pub bypass_mark: u32,
    /// macOS/Windows physical interface used by protected outbound sockets.
    pub outbound_interface: Option<String>,
    /// Local TCP port where redirected TCP flows are sent.
    pub redirect_port: u16,
    /// Local UDP port where redirected DNS packets are sent.
    pub dns_port: u16,
    /// Windows-only path to `wintun.dll`.
    pub wintun_file: Option<String>,
    /// Packet batching controls for packets written back to TUN.
    pub batch: TunBatchConfig,
    /// Maximum concurrent UDP NAT/session flows.
    pub udp_max_sessions: usize,
    /// UDP idle timeout for NAT/session flows.
    pub udp_idle_timeout: Duration,
    /// Maximum concurrent packet-level TCP bridge flows.
    pub tcp_max_sessions: usize,
    /// Linux-only packet backend experiments.
    pub linux: TunLinuxConfig,
}

/// Linux-only packet backend settings carried alongside the TUN runtime config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TunLinuxConfig {
    pub backend: TunLinuxBackend,
    pub af_xdp: TunAfXdpConfig,
}

/// Linux packet backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TunLinuxBackend {
    #[default]
    Tun,
    AfXdp,
}

/// AF_XDP backend options for Linux experiments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunAfXdpConfig {
    /// Network interface to attach the AF_XDP socket to, or `None` to use the TUN interface.
    pub interface: Option<String>,
    /// Hardware queue index to bind the AF_XDP socket to.
    pub queue_id: u32,
    /// Number of descriptors in each AF_XDP ring (RX/TX/fill/completion).
    pub ring_entries: u32,
    /// Total number of frames in the UMEM region.
    pub frame_count: u32,
    /// Size in bytes of each frame in the UMEM region.
    pub frame_size: u32,
    /// Force copy mode even when zero-copy is available.
    pub force_copy: bool,
    /// Force zero-copy mode; fails if the driver does not support it.
    pub force_zerocopy: bool,
}

impl Default for TunAfXdpConfig {
    fn default() -> Self {
        Self {
            interface: None,
            queue_id: 0,
            ring_entries: 1024,
            frame_count: 4096,
            frame_size: 2048,
            force_copy: true,
            force_zerocopy: false,
        }
    }
}

impl Default for TunConfig {
    fn default() -> Self {
        let address: std::net::Ipv4Addr = "198.18.0.1"
            .parse()
            .expect("valid default TUN address literal");
        let netmask: std::net::Ipv4Addr = "255.255.0.0"
            .parse()
            .expect("valid default TUN netmask literal");
        Self {
            name: "blackwire-tun".into(),
            address,
            netmask,
            mtu: 1500,
            bypass_mark: 0x1234,
            outbound_interface: None,
            redirect_port: 7890,
            dns_port: 5300,
            wintun_file: None,
            batch: TunBatchConfig::default(),
            udp_max_sessions: 4096,
            udp_idle_timeout: Duration::from_secs(60),
            tcp_max_sessions: 4096,
            linux: TunLinuxConfig::default(),
        }
    }
}

/// Create and bring up an async TUN device using `config`.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub fn create_tun(config: &TunConfig) -> Result<TunDevice> {
    let mut cfg = tun::Configuration::default();

    configure_tun_name(&mut cfg, &config.name);

    cfg.address(config.address)
        .netmask(config.netmask)
        .mtu(config.mtu)
        .up();

    #[cfg(target_os = "linux")]
    cfg.platform_config(|p| {
        p.ensure_root_privileges(true);
    });

    #[cfg(target_os = "macos")]
    cfg.platform_config(|p| {
        p.packet_information(true);
        p.enable_routing(false);
    });

    #[cfg(target_os = "windows")]
    if let Some(wintun_file) = &config.wintun_file {
        cfg.platform_config(|p| {
            p.wintun_file(wintun_file);
        });
    }

    let dev = tun::create_as_async(&cfg)?;
    info!(name = %config.name, address = %config.address, mtu = config.mtu, "TUN interface created");
    Ok(dev)
}

/// Return the OS-assigned name for a TUN device.
#[cfg(target_os = "macos")]
pub fn tun_device_name(device: &TunDevice) -> Result<String> {
    device.tun_name().map_err(Into::into)
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn configure_tun_name(cfg: &mut tun::Configuration, name: &str) {
    cfg.tun_name(name);
}

#[cfg(target_os = "macos")]
fn configure_tun_name(cfg: &mut tun::Configuration, name: &str) {
    if is_macos_utun_name(name) {
        cfg.tun_name(name);
    }
}

#[cfg(target_os = "macos")]
fn is_macos_utun_name(name: &str) -> bool {
    name.strip_prefix("utun")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

/// Return a clear unsupported error until a native backend exists.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn create_tun(_config: &TunConfig) -> Result<TunDevice> {
    let support = current_tun_support();
    anyhow::bail!(
        "TUN device backend is not supported on {} yet: {}",
        support.backend,
        support.note
    );
}
