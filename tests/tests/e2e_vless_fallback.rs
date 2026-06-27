//! VLESS rejection fallback behavior.
//!
//! These tests keep active-probe-sensitive rejection cases on the same backend
//! fallback path when a fallback is configured.

use std::sync::Arc;
use std::time::Duration;

use blackwire_common::Address;
use blackwire_core::Instance;
use blackwire_protocol::vless::codec::{encode_request, Command};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const GOOD_UUID: &str = "a0000000-0000-4000-8000-000000000001";
const WRONG_UUID: [u8; 16] = [0xBA; 16];
const FALLBACK_BODY: &[u8] = b"fallback-ok";

struct VlessFallbackFixture {
    port: u16,
    _server: Instance,
}

fn parse_config(json: serde_json::Value) -> Arc<blackwire_config::schema::Config> {
    Arc::new(serde_json::from_value(json).expect("config parse"))
}

fn unused_local_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("port reserve")
        .local_addr()
        .expect("port addr")
        .port()
}

async fn spawn_fallback() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("fallback bind");
    let port = listener.local_addr().expect("fallback addr").port();

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 512];
                let _ = timeout(Duration::from_secs(2), stream.read(&mut buf)).await;
                let response = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nfallback-ok";
                let _ = stream.write_all(response).await;
            });
        }
    });

    port
}

async fn spawn_vless_server(flow: &str) -> VlessFallbackFixture {
    let vless_port = unused_local_port();
    let fallback_port = spawn_fallback().await;

    let server = Instance::from_config(parse_config(serde_json::json!({
        "inbounds": [{
            "tag": "vless-in",
            "protocol": "vless",
            "listen": "127.0.0.1",
            "port": vless_port,
            "settings": {
                "clients": [{
                    "id": GOOD_UUID,
                    "email": "fallback@example.test",
                    "flow": flow
                }],
                "fallback": {
                    "dest": format!("127.0.0.1:{fallback_port}")
                }
            }
        }],
        "outbounds": [{"tag": "direct", "protocol": "freedom"}]
    })))
    .await
    .expect("server start");

    tokio::time::sleep(Duration::from_millis(50)).await;
    VlessFallbackFixture {
        port: vless_port,
        _server: server,
    }
}

async fn send_probe(vless_port: u16, bytes: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", vless_port))
        .await
        .expect("connect vless");
    stream.write_all(bytes).await.expect("write probe");

    let mut response = vec![0u8; 256];
    let n = timeout(Duration::from_secs(2), stream.read(&mut response))
        .await
        .expect("read fallback timed out")
        .expect("read fallback");
    response.truncate(n);
    response
}

#[tokio::test]
async fn wrong_uuid_uses_fallback_backend() {
    let fixture = spawn_vless_server("").await;
    let request = encode_request(
        &WRONG_UUID,
        "",
        Command::Tcp,
        &Address::Domain("example.com".into(), 443),
    )
    .expect("encode wrong uuid request");

    let response = send_probe(fixture.port, &request).await;
    assert!(response
        .windows(FALLBACK_BODY.len())
        .any(|w| w == FALLBACK_BODY));
}

#[tokio::test]
async fn malformed_header_uses_fallback_backend() {
    let fixture = spawn_vless_server("").await;

    let response = send_probe(fixture.port, b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n").await;
    assert!(response
        .windows(FALLBACK_BODY.len())
        .any(|w| w == FALLBACK_BODY));
}

#[tokio::test]
async fn wrong_flow_uses_same_fallback_backend() {
    let fixture = spawn_vless_server("").await;
    let good_uuid = GOOD_UUID.parse::<uuid::Uuid>().expect("uuid").into_bytes();
    let request = encode_request(
        &good_uuid,
        "xtls-rprx-vision",
        Command::Tcp,
        &Address::Domain("example.com".into(), 443),
    )
    .expect("encode wrong flow request");

    let response = send_probe(fixture.port, &request).await;
    assert!(response
        .windows(FALLBACK_BODY.len())
        .any(|w| w == FALLBACK_BODY));
}
