//! End-to-end: Hysteria2 UDP datagram policy.
//!
//! Native Hysteria2 UDP datagrams are disabled server-side until they can be
//! routed through the dispatcher/outbound policy stack. These tests verify that
//! the server advertises UDP as disabled and that the client refuses to send
//! datagrams on that disabled lane.

use std::net::Ipv4Addr;
use std::time::Duration;

use tokio::time::timeout;

const TEST_PASSWORD: &str = "hysteria2-udp-test-pw";

fn unused_local_port() -> u16 {
    std::net::UdpSocket::bind(("127.0.0.1", 0))
        .expect("port reserve")
        .local_addr()
        .unwrap()
        .port()
}

fn write_dev_cert_files() -> (String, String) {
    let (cert_pem, key_pem) = blackwire_transport::dev_self_signed().unwrap();
    let dir = std::env::temp_dir();
    let unique = format!(
        "blackwire-hysteria2-udp-{}-{}",
        std::process::id(),
        unused_local_port()
    );
    let cert_path = dir.join(format!("{unique}.cert.pem"));
    let key_path = dir.join(format!("{unique}.key.pem"));
    std::fs::write(&cert_path, cert_pem).expect("write cert");
    std::fs::write(&key_path, key_pem).expect("write key");
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn parse_config(json: String) -> std::sync::Arc<blackwire_config::schema::Config> {
    std::sync::Arc::new(serde_json::from_str(&json).expect("config parse"))
}

fn server_config(
    hysteria_port: u16,
    cert_path: &str,
    key_path: &str,
) -> std::sync::Arc<blackwire_config::schema::Config> {
    let cert_json = serde_json::to_string(cert_path).expect("serialize cert path");
    let key_json = serde_json::to_string(key_path).expect("serialize key path");
    parse_config(format!(
        r#"{{
            "datagram": {{"enabled": true, "udpOverDatagram": true}},
            "fec": {{"mode": "xor1-of-n", "maxOverheadPercent": 25}},
            "inbounds": [{{
                "tag": "hysteria2-udp-in",
                "protocol": "hysteria2",
                "listen": "127.0.0.1",
                "port": {hysteria_port},
                "settings": {{
                    "auth": "{TEST_PASSWORD}",
                    "upMbps": 100,
                    "downMbps": 100
                }},
                "streamSettings": {{
                    "network": "quic",
                    "security": "tls",
                    "tlsSettings": {{
                        "certificateFile": {cert_json},
                        "keyFile": {key_json}
                    }}
                }}
            }}],
            "outbounds": [{{"tag": "freedom", "protocol": "freedom"}}]
        }}"#
    ))
}

fn client_config(hysteria_port: u16) -> blackwire_transport::Hysteria2ClientConfig {
    blackwire_transport::Hysteria2ClientConfig {
        server: format!("127.0.0.1:{hysteria_port}").parse().unwrap(),
        server_name: "localhost".to_string(),
        password: TEST_PASSWORD.to_string(),
        up_mbps: 50,
        down_mbps: 50,
        skip_cert_verify: true,
        congestion: blackwire_transport::CongestionConfig {
            up_mbps: 50,
            down_mbps: 50,
            ..blackwire_transport::CongestionConfig::default()
        },
        endpoint_shards: 1,
        socket: blackwire_transport::QuicSocketConfig::default(),
        datagram_enabled: true,
        fec: blackwire_transport::FecPolicy::default(),
        datagram_policy: blackwire_transport::DatagramPolicy::default(),
    }
}

/// Verify Hysteria2 UDP datagrams are disabled when no dispatcher-routed UDP
/// implementation is available.
#[tokio::test]
async fn hysteria2_udp_datagram_lane_is_disabled() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("error")
        .try_init();

    let hysteria_port = unused_local_port();
    let (cert_path, key_path) = write_dev_cert_files();

    let _server =
        blackwire_core::Instance::from_config(server_config(hysteria_port, &cert_path, &key_path))
            .await
            .expect("Hysteria2 server start");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let config = client_config(hysteria_port);

    let session = timeout(
        Duration::from_secs(5),
        blackwire_transport::Hysteria2UdpSession::connect(&config),
    )
    .await
    .expect("Hysteria2 UDP connect timed out")
    .expect("Hysteria2 UDP connect failed");

    let dest = blackwire_transport::UdpDestination::V4(Ipv4Addr::LOCALHOST, 9);

    let err = session
        .send(dest, bytes::Bytes::from_static(b"hysteria2-udp-disabled"))
        .expect_err("disabled UDP datagram lane must reject sends");
    assert!(
        err.to_string().contains("DATAGRAM lane disabled"),
        "unexpected disabled-lane error: {err}"
    );

    let _ = std::fs::remove_file(&cert_path);
    let _ = std::fs::remove_file(&key_path);
}

#[tokio::test]
async fn hysteria2_udp_datagram_lane_is_disabled_with_xor_fec_enabled() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("error")
        .try_init();

    let hysteria_port = unused_local_port();
    let (cert_path, key_path) = write_dev_cert_files();

    let _server =
        blackwire_core::Instance::from_config(server_config(hysteria_port, &cert_path, &key_path))
            .await
            .expect("Hysteria2 server start");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut config = client_config(hysteria_port);
    config.fec = blackwire_transport::FecPolicy {
        mode: blackwire_transport::FecMode::Xor1OfN,
        max_overhead_percent: 25,
        group_size: 4,
        ..blackwire_transport::FecPolicy::default()
    };

    let session = timeout(
        Duration::from_secs(5),
        blackwire_transport::Hysteria2UdpSession::connect(&config),
    )
    .await
    .expect("Hysteria2 UDP connect timed out")
    .expect("Hysteria2 UDP connect failed");

    let dest = blackwire_transport::UdpDestination::V4(Ipv4Addr::LOCALHOST, 9);
    let err = session
        .send(
            dest,
            bytes::Bytes::from_static(b"hysteria2-udp-fec-disabled"),
        )
        .expect_err("disabled UDP datagram lane must reject sends");
    assert!(
        err.to_string().contains("DATAGRAM lane disabled"),
        "unexpected disabled-lane error: {err}"
    );

    let _ = std::fs::remove_file(&cert_path);
    let _ = std::fs::remove_file(&key_path);
}
