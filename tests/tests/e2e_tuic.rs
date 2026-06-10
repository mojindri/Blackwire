//! End-to-end TUIC v5 coverage for TCP proxy streams and UDP datagrams.

use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time::timeout,
};
use uuid::Uuid;

const TEST_UUID: &str = "8b9a2f4a-5e51-47a6-b012-75c9dfe8bc30";
const TEST_PASSWORD: &str = "tuic-v5-test-password";

fn unused_local_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

async fn spawn_tcp_echo_server() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        loop {
            let n = stream.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            stream.write_all(&buf[..n]).await.unwrap();
        }
    });
    (port, task)
}

async fn spawn_udp_echo_server() -> (u16, tokio::task::JoinHandle<()>) {
    let socket = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
    let port = socket.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, peer)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let _ = socket.send_to(&buf[..n], peer).await;
        }
    });
    (port, task)
}

async fn socks5_connect(socks_port: u16, dest_host: &str, dest_port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", socks_port)).await.unwrap();
    stream.write_all(&[5, 1, 0]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [5, 0]);

    let host_bytes = dest_host.as_bytes();
    let mut req = vec![5, 1, 0, 3, host_bytes.len() as u8];
    req.extend_from_slice(host_bytes);
    req.extend_from_slice(&dest_port.to_be_bytes());
    stream.write_all(&req).await.unwrap();

    let mut reply = [0u8; 10];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0, "SOCKS5 CONNECT failed: REP={:#x}", reply[1]);
    stream
}

fn write_dev_cert_files() -> (String, String) {
    let (cert_pem, key_pem) = blackwire_transport::dev_self_signed().unwrap();
    let dir = std::env::temp_dir();
    let unique = format!(
        "blackwire-tuic-{}-{}",
        std::process::id(),
        unused_local_port()
    );
    let cert_path = dir.join(format!("{unique}.cert.pem"));
    let key_path = dir.join(format!("{unique}.key.pem"));
    std::fs::write(&cert_path, cert_pem).unwrap();
    std::fs::write(&key_path, key_pem).unwrap();
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn parse_config(json: String) -> Arc<blackwire_config::schema::Config> {
    Arc::new(serde_json::from_str(&json).unwrap())
}

fn server_config(
    port: u16,
    cert_path: &str,
    key_path: &str,
) -> Arc<blackwire_config::schema::Config> {
    let cert_json = serde_json::to_string(cert_path).unwrap();
    let key_json = serde_json::to_string(key_path).unwrap();
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "tuic-in",
                "protocol": "tuic",
                "listen": "127.0.0.1",
                "port": {port},
                "settings": {{
                    "users": [{{"uuid": "{TEST_UUID}", "password": "{TEST_PASSWORD}"}}],
                    "network": "tcp,udp"
                }},
                "streamSettings": {{
                    "network": "quic",
                    "security": "tls",
                    "tlsSettings": {{
                        "serverName": "localhost",
                        "certificateFile": {cert_json},
                        "keyFile": {key_json}
                    }}
                }}
            }}],
            "outbounds": [{{ "tag": "freedom", "protocol": "freedom" }}],
            "routing": {{ "rules": [{{ "outboundTag": "freedom" }}] }}
        }}"#
    ))
}

fn client_config(socks_port: u16, tuic_port: u16) -> Arc<blackwire_config::schema::Config> {
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "socks-in",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": {socks_port}
            }}],
            "outbounds": [{{
                "tag": "tuic-out",
                "protocol": "tuic",
                "settings": {{
                    "server": "127.0.0.1:{tuic_port}",
                    "serverName": "localhost",
                    "uuid": "{TEST_UUID}",
                    "password": "{TEST_PASSWORD}",
                    "skipCertVerify": true
                }}
            }}],
            "routing": {{ "rules": [{{ "outboundTag": "tuic-out" }}] }}
        }}"#
    ))
}

fn tuic_udp_client_config(tuic_port: u16) -> blackwire_transport::TuicClientConfig {
    blackwire_transport::TuicClientConfig {
        server: format!("127.0.0.1:{tuic_port}").parse().unwrap(),
        server_name: "localhost".into(),
        uuid: Uuid::parse_str(TEST_UUID).unwrap(),
        password: TEST_PASSWORD.into(),
        skip_cert_verify: true,
        endpoint_shards: 1,
        socket: blackwire_transport::QuicSocketConfig::default(),
        enable_udp: true,
    }
}

#[tokio::test]
async fn tuic_v5_tcp_proxy_roundtrip() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("error")
        .try_init();
    let (echo_port, _echo_task) = spawn_tcp_echo_server().await;
    let socks_port = unused_local_port();
    let tuic_port = unused_local_port();
    let (cert_path, key_path) = write_dev_cert_files();

    let _server =
        blackwire_core::Instance::from_config(server_config(tuic_port, &cert_path, &key_path))
            .await
            .expect("TUIC server instance");
    let _client = blackwire_core::Instance::from_config(client_config(socks_port, tuic_port))
        .await
        .expect("TUIC client instance");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut stream = socks5_connect(socks_port, "127.0.0.1", echo_port).await;
    stream.write_all(b"tuic-v5-tcp").await.unwrap();
    let mut buf = vec![0u8; "tuic-v5-tcp".len()];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, b"tuic-v5-tcp");

    let _ = std::fs::remove_file(cert_path);
    let _ = std::fs::remove_file(key_path);
}

#[tokio::test]
async fn tuic_v5_udp_datagram_roundtrip() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("error")
        .try_init();
    let tuic_port = unused_local_port();
    let (cert_path, key_path) = write_dev_cert_files();
    let _server =
        blackwire_core::Instance::from_config(server_config(tuic_port, &cert_path, &key_path))
            .await
            .expect("TUIC server instance");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let (echo_port, _echo_task) = spawn_udp_echo_server().await;
    let session = timeout(
        Duration::from_secs(5),
        blackwire_transport::TuicUdpSession::connect(&tuic_udp_client_config(tuic_port)),
    )
    .await
    .expect("TUIC UDP connect timeout")
    .expect("TUIC UDP connect");

    session
        .send(
            blackwire_common::Address::Ipv4(Ipv4Addr::LOCALHOST, echo_port),
            b"tuic-v5-udp",
        )
        .await
        .expect("TUIC UDP send");
    let response = timeout(Duration::from_secs(5), session.recv())
        .await
        .expect("TUIC UDP response timeout")
        .expect("TUIC UDP recv");
    assert_eq!(response.data.as_ref(), b"tuic-v5-udp");

    let _ = std::fs::remove_file(cert_path);
    let _ = std::fs::remove_file(key_path);
}
