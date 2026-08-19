//! Client-only runtime assembly for full-device Blackwire operation.
//!
//! The proxy protocols and TUN implementation remain shared crates. This crate
//! owns the client product boundary: validating a local SOCKS interception
//! endpoint, creating the OS TUN device, and applying protected egress policy.

use std::time::Duration;

use anyhow::{Context, Result};
use blackwire_common::{
    clear_outbound_bypass_mark, clear_outbound_interface_index, set_outbound_bypass_mark,
    set_outbound_interface_name,
};
use blackwire_config::schema::{Config, Protocol};
use blackwire_transport::{TunBatchConfig, TunConfig};
use validator::Validate;

/// A validated client configuration split into proxy-core and OS-capture parts.
pub struct ClientConfig {
    /// Shared proxy runtime configuration, with TUN ownership removed.
    pub proxy: Config,
    /// Client-owned OS TUN configuration.
    pub tun: TunConfig,
}

impl ClientConfig {
    /// Validate and split a complete client configuration.
    pub fn from_config(mut config: Config) -> Result<Self> {
        config
            .validate()
            .context("client configuration validation failed")?;
        let source = config
            .tun
            .take()
            .context("client configuration requires a top-level 'tun' section")?;

        let has_redirect_inbound = config.inbounds.iter().any(|inbound| {
            inbound.protocol == Protocol::Socks
                && inbound.port == source.redirect_port
                && inbound.listen.is_loopback()
        });
        anyhow::ensure!(
            has_redirect_inbound,
            "TUN redirectPort {} requires a loopback SOCKS inbound on the same port",
            source.redirect_port
        );

        let tun = TunConfig {
            name: source.name,
            address: source
                .address
                .parse()
                .with_context(|| format!("invalid TUN address '{}'", source.address))?,
            netmask: source
                .netmask
                .parse()
                .with_context(|| format!("invalid TUN netmask '{}'", source.netmask))?,
            mtu: source.mtu,
            bypass_mark: source.bypass_mark,
            outbound_interface: source.outbound_interface,
            redirect_port: source.redirect_port,
            dns_port: source.dns_port,
            wintun_file: source.wintun_file,
            batch: TunBatchConfig {
                enabled: source.batch.enabled,
                max_packets: source.batch.max_packets,
                max_delay: Duration::from_micros(source.batch.max_delay_us),
                latency_flush_bytes: source.batch.latency_flush_bytes,
            },
            udp_max_sessions: source.sessions.udp_max,
            udp_idle_timeout: Duration::from_secs(source.sessions.udp_idle_timeout_sec),
            tcp_max_sessions: source.sessions.tcp_max,
        };

        Ok(Self { proxy: config, tun })
    }
}

/// Process-wide protected-egress settings owned by the client lifetime.
pub struct ProtectedEgressGuard {
    has_mark: bool,
    has_interface: bool,
}

impl ProtectedEgressGuard {
    /// Apply the bypass mark and optional physical interface before TUN routes
    /// begin capturing traffic.
    pub fn apply(config: &TunConfig) -> Result<Self> {
        set_outbound_bypass_mark(config.bypass_mark);
        let has_interface = if let Some(interface) = &config.outbound_interface {
            if let Err(error) = set_outbound_interface_name(interface) {
                clear_outbound_bypass_mark();
                return Err(error)
                    .with_context(|| format!("invalid TUN outbound interface '{interface}'"));
            }
            true
        } else {
            false
        };
        Ok(Self {
            has_mark: true,
            has_interface,
        })
    }
}

impl Drop for ProtectedEgressGuard {
    fn drop(&mut self) {
        if self.has_mark {
            clear_outbound_bypass_mark();
        }
        if self.has_interface {
            clear_outbound_interface_index();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClientConfig;
    use blackwire_config::schema::Config;

    fn parse(value: serde_json::Value) -> Config {
        serde_json::from_value(value).expect("valid fixture")
    }

    #[test]
    fn client_config_requires_tun() {
        let config = parse(serde_json::json!({
            "inbounds": [],
            "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
        }));
        assert!(ClientConfig::from_config(config).is_err());
    }

    #[test]
    fn client_config_requires_matching_loopback_socks_inbound() {
        let config = parse(serde_json::json!({
            "tun": { "redirectPort": 12345 },
            "inbounds": [{
                "tag": "local-http", "protocol": "http",
                "listen": "127.0.0.1", "port": 12345
            }],
            "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
        }));
        assert!(ClientConfig::from_config(config).is_err());
    }

    #[test]
    fn client_config_extracts_tun_from_proxy_core() {
        let config = parse(serde_json::json!({
            "tun": { "redirectPort": 12345, "dnsPort": 5300 },
            "inbounds": [{
                "tag": "tun-socks", "protocol": "socks",
                "listen": "127.0.0.1", "port": 12345
            }],
            "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
        }));
        let client = ClientConfig::from_config(config).expect("client config");
        assert!(client.proxy.tun.is_none());
        assert_eq!(client.tun.redirect_port, 12345);
        assert_eq!(client.tun.dns_port, 5300);
    }
}
