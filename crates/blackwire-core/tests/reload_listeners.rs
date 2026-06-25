use std::net::IpAddr;

use blackwire_config::schema::{
    Config, DatagramConfig, FecConfig, FecMode, InboundConfig, LimitsConfig, LogConfig,
    OutboundConfig, ProfileMode, Protocol, QuicConfig,
};
use blackwire_core::{inbound_listener_changes, requires_instance_restart};

fn minimal_config(port: u16) -> Config {
    Config {
        profile: ProfileMode::default(),
        fast: None,
        budget: None,
        vision: None,
        first_packet_boost: None,
        quic: None,
        datagram: None,
        fec: None,
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

#[test]
fn requires_instance_restart_ignores_vless_client_list_changes() {
    let mut old = minimal_config(1080);
    old.inbounds[0].protocol = Protocol::Vless;
    old.inbounds[0].settings = serde_json::json!({
        "clients": [{"id":"00000000-0000-4000-8000-000000000001"}]
    });

    let mut new = old.clone();
    new.inbounds[0].settings = serde_json::json!({
        "clients": [
            {"id":"00000000-0000-4000-8000-000000000001"},
            {"id":"00000000-0000-4000-8000-000000000002"}
        ]
    });

    assert!(!requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_ignores_non_vless_client_bandwidth_only_changes() {
    let mut old = minimal_config(1080);
    old.inbounds[0].protocol = Protocol::Trojan;
    old.inbounds[0].settings = serde_json::json!({
        "clients": [{"password":"secret","email":"alice@example.local","upMbps":10,"downMbps":20}]
    });

    let mut new = old.clone();
    new.inbounds[0].settings = serde_json::json!({
        "clients": [{"password":"secret","email":"alice@example.local","upMbps":40,"downMbps":80}]
    });

    assert!(!requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_for_non_reloadable_setting_changes() {
    let mut old = minimal_config(1080);
    old.inbounds[0].protocol = Protocol::Trojan;
    old.inbounds[0].settings = serde_json::json!({
        "clients": [{"password":"secret","email":"alice@example.local","upMbps":10,"downMbps":20}],
        "network": "tcp"
    });

    let mut new = old.clone();
    new.inbounds[0].settings = serde_json::json!({
        "clients": [{"password":"changed","email":"alice@example.local","upMbps":10,"downMbps":20}],
        "network": "ws"
    });

    assert!(requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_ignores_vmess_client_auth_changes() {
    let mut old = minimal_config(1080);
    old.inbounds[0].protocol = Protocol::Vmess;
    old.inbounds[0].settings = serde_json::json!({
        "clients": [{"id":"00000000-0000-4000-8000-000000000001","email":"alice@example.local"}]
    });

    let mut new = old.clone();
    new.inbounds[0].settings = serde_json::json!({
        "clients": [{"id":"00000000-0000-4000-8000-000000000002","email":"bob@example.local"}]
    });

    assert!(!requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_ignores_tuic_user_auth_changes() {
    let mut old = minimal_config(1080);
    old.inbounds[0].protocol = Protocol::Tuic;
    old.inbounds[0].settings = serde_json::json!({
        "users": [{"uuid":"00000000-0000-4000-8000-000000000001","password":"secret"}]
    });

    let mut new = old.clone();
    new.inbounds[0].settings = serde_json::json!({
        "users": [{"uuid":"00000000-0000-4000-8000-000000000002","password":"changed"}]
    });

    assert!(!requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_for_outbound_changes() {
    let old = minimal_config(1080);
    let mut new = minimal_config(1080);
    new.outbounds.push(OutboundConfig {
        tag: "backup".into(),
        protocol: Protocol::Freedom,
        settings: serde_json::json!({}),
        stream_settings: None,
    });

    assert!(requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_ignores_per_user_limit_changes() {
    let mut old = minimal_config(1080);
    old.limits.max_connections_per_user = Some(16);

    let mut new = old.clone();
    new.limits.max_connections_per_user = Some(32);

    assert!(!requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_for_non_reloadable_global_limit_changes() {
    let mut old = minimal_config(1080);
    old.limits.max_connections = Some(128);

    let mut new = old.clone();
    new.limits.max_connections = Some(256);

    assert!(requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_for_quic_socket_tuning_changes() {
    let old = minimal_config(1080);
    let mut new = minimal_config(1080);
    new.quic = Some(QuicConfig {
        reuse_port: true,
        ..QuicConfig::default()
    });

    assert!(requires_instance_restart(&old, &new));
}

#[test]
fn requires_instance_restart_for_datagram_fec_changes() {
    let old = minimal_config(1080);
    let mut new = minimal_config(1080);
    new.datagram = Some(DatagramConfig::default());
    new.fec = Some(FecConfig {
        mode: FecMode::Xor1OfN,
        ..FecConfig::default()
    });

    assert!(requires_instance_restart(&old, &new));
}
