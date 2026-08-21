use blackwire_config::schema::Config;
use validator::Validate;

#[test]
fn unsupported_transport_network_option_rejected_at_parse_time() {
    let json = r#"{
        "inbounds": [{
            "tag": "socks",
            "protocol": "socks",
            "listen": "127.0.0.1",
            "port": 1080,
            "streamSettings": { "network": "unknown-net" }
        }],
        "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
    }"#;
    assert!(
        serde_json::from_str::<Config>(json).is_err(),
        "unsupported network value must be rejected by schema parser"
    );
}

#[test]
fn unsupported_security_option_rejected_at_parse_time() {
    let json = r#"{
        "inbounds": [{
            "tag": "socks",
            "protocol": "socks",
            "listen": "127.0.0.1",
            "port": 1080,
            "streamSettings": { "security": "bogus-security" }
        }],
        "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
    }"#;
    assert!(
        serde_json::from_str::<Config>(json).is_err(),
        "unsupported security value must be rejected by schema parser"
    );
}

#[test]
fn invalid_dns_servers_type_rejected() {
    let json = r#"{
        "dns": { "servers": "not-an-array" },
        "inbounds": [{
            "tag": "socks",
            "protocol": "socks",
            "listen": "127.0.0.1",
            "port": 1080
        }],
        "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
    }"#;
    assert!(serde_json::from_str::<Config>(json).is_err());
}

#[test]
fn invalid_outbound_port_zero_fails_validation() {
    let json = r#"{
        "inbounds": [{
            "tag": "socks",
            "protocol": "socks",
            "listen": "127.0.0.1",
            "port": 1080
        }],
        "outbounds": [{
            "tag": "vless",
            "protocol": "vless",
            "settings": {
                "address": "127.0.0.1",
                "port": 0,
                "users": [{"id":"00000000-0000-4000-8000-000000000001"}]
            }
        }]
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("parse config");
    assert!(
        cfg.validate().is_err(),
        "schema validation should fail closed for invalid outbound port"
    );
}

#[test]
fn generic_v2ray_quic_endpoints_fail_validation() {
    for protocol in ["vless", "vmess", "trojan"] {
        let inbound_json = serde_json::json!({
            "inbounds": [{
                "tag": "legacy-quic",
                "protocol": protocol,
                "listen": "127.0.0.1",
                "port": 443,
                "streamSettings": { "network": "quic" }
            }],
            "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
        });
        let inbound: Config =
            serde_json::from_value(inbound_json).expect("parse legacy inbound config");
        assert!(
            inbound.validate().is_err(),
            "generic QUIC inbound must fail for {protocol}"
        );

        let outbound_json = serde_json::json!({
            "inbounds": [{
                "tag": "socks",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": 1080
            }],
            "outbounds": [{
                "tag": "legacy-quic",
                "protocol": protocol,
                "streamSettings": { "network": "quic" }
            }]
        });
        let outbound: Config =
            serde_json::from_value(outbound_json).expect("parse legacy outbound config");
        assert!(
            outbound.validate().is_err(),
            "generic QUIC outbound must fail for {protocol}"
        );
    }
}

#[test]
fn quic_native_protocol_remains_valid() {
    let json = r#"{
        "inbounds": [{
            "tag": "hy2",
            "protocol": "hysteria2",
            "listen": "127.0.0.1",
            "port": 443,
            "settings": { "auth": "secret" },
            "streamSettings": { "network": "quic" }
        }],
        "outbounds": [{ "tag": "direct", "protocol": "freedom" }]
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("parse native QUIC config");
    assert!(cfg.validate().is_ok());
}
