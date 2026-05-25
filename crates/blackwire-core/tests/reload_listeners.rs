use std::net::IpAddr;

use blackwire_config::schema::{Config, InboundConfig, LimitsConfig, LogConfig, OutboundConfig, Protocol};
use blackwire_core::inbound_listener_changes;

fn minimal_config(port: u16) -> Config {
    Config {
        log: LogConfig::default(),
        dns: None,
        routing: None,
        tun: None,
        limits: LimitsConfig::default(),
        inbounds: vec![InboundConfig {
            tag: "in".into(),
            listen: "127.0.0.1".parse::<IpAddr>().unwrap(),
            port,
            protocol: Protocol::Socks,
            settings: serde_json::json!({}),
            stream_settings: None,
            limits: None,
            sniffing: None,
        }],
        outbounds: vec![OutboundConfig {
            tag: "direct".into(),
            protocol: Protocol::Freedom,
            settings: serde_json::json!({}),
            stream_settings: None,
        }],
        stats: None,
        api: None,
        metrics_addr: None,
    }
}

#[test]
fn inbound_listener_changes_detects_port_change() {
    let old = minimal_config(1080);
    let new = minimal_config(1081);
    let changes = inbound_listener_changes(&old, &new);
    assert_eq!(changes, vec!["in".to_string()]);
}
