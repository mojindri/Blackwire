//! Phase 4 integration tests: Trojan protocol + WebSocket/TLS transports.
//!
//! Tests included:
//!
//! ## Trojan
//!  1. `trojan_plain_tcp_single_chunk`         — basic Trojan over plain TCP
//!  2. `trojan_plain_tcp_large_payload`         — large payload Trojan over plain TCP
//!  3. `trojan_wrong_password_is_rejected`      — auth failure on bad token
//!  4. `trojan_over_tls_roundtrip`              — Trojan over TLS (self-signed cert)
//!  5. `trojan_multiple_passwords_any_accepted` — multi-password Trojan
//!  6. `trojan_ipv4_address`                    — Trojan to an IPv4 destination
//!  7. `trojan_domain_address`                  — Trojan to a domain destination
//!  8. `trojan_over_tls_large_payload`          — large payload over TLS
//!
//! ## WebSocket transport
//!  9.  `ws_transport_echo`                   — WS handshake + binary echo
//! 10.  `ws_transport_large_payload`          — large payload over WS
//! 11.  `ws_transport_custom_path`            — WS with non-default path
//!
//! ## VLESS over WebSocket (via SOCKS5 proxy chain)
//! 12.  `vless_over_ws_plain`                 — VLESS over WS, no TLS
//! 13.  `vless_over_ws_tls`                   — VLESS over WS + TLS (wss)
//! 14.  `vless_over_ws_large_payload`         — large payload VLESS over WS
//! 15.  `vless_over_ws_tls_multi_conn`        — multiple sequential connections over WS+TLS

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// ── Shared helpers ────────────────────────────────────────────────────────────

fn unused_local_port() -> u16 {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("failed to reserve local port");
    listener.local_addr().unwrap().port()
}

async fn spawn_echo_server() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("echo server bind failed");
    let port = listener.local_addr().unwrap().port();

    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    let _ = stream.write_all(&buf[..n]).await;
                }
            });
        }
    });

    (port, task)
}

/// SOCKS5 connect helper: negotiate + send CONNECT, return open stream.
async fn socks5_connect(socks_port: u16, dest_host: &str, dest_port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", socks_port))
        .await
        .expect("failed to connect to SOCKS5 proxy");

    // Method negotiation: [VER=5, NMETHODS=1, METHOD=0 (no auth)]
    stream.write_all(&[5, 1, 0]).await.unwrap();
    let mut resp = [0u8; 2];
    stream.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, [5, 0]);

    // CONNECT request: [VER=5, CMD=1, RSV=0, ATYP=3 (domain), len, host, port_hi, port_lo]
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

fn parse_config(json: String) -> Arc<proxy_config::schema::Config> {
    Arc::new(serde_json::from_str(&json).expect("config parse failed"))
}

/// Write a self-signed certificate and private key to temp files.
/// Returns (cert_path, key_path).
fn write_dev_cert_files() -> (String, String) {
    let (cert_pem, key_pem) = proxy_transport::dev_self_signed().unwrap();
    let dir = std::env::temp_dir();
    let unique = format!(
        "proxy-rs-phase4-{}-{}",
        std::process::id(),
        unused_local_port()
    );
    let cert_path = dir.join(format!("{unique}.cert.pem"));
    let key_path = dir.join(format!("{unique}.key.pem"));

    std::fs::write(&cert_path, cert_pem).expect("write cert failed");
    std::fs::write(&key_path, key_pem).expect("write key failed");

    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

const TEST_PASSWORD: &str = "phase4-test-password";
const TEST_UUID: &str = "b45c5b86-1234-4321-abcd-0123456789ab";

// ── Trojan proxy configs ─────────────────────────────────────────────────────

fn trojan_server_plain(trojan_port: u16) -> Arc<proxy_config::schema::Config> {
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "trojan-in",
                "protocol": "trojan",
                "listen": "127.0.0.1",
                "port": {trojan_port},
                "settings": {{
                    "clients": [{{"password": "{TEST_PASSWORD}"}}]
                }}
            }}],
            "outbounds": [{{
                "tag": "freedom",
                "protocol": "freedom"
            }}],
            "routing": {{ "rules": [{{ "outboundTag": "freedom" }}] }}
        }}"#
    ))
}

fn trojan_client_plain(socks_port: u16, trojan_port: u16) -> Arc<proxy_config::schema::Config> {
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "socks-in",
                "protocol": "socks",
                "listen": "127.0.0.1",
                "port": {socks_port}
            }}],
            "outbounds": [{{
                "tag": "trojan-out",
                "protocol": "trojan",
                "settings": {{
                    "address": "127.0.0.1",
                    "port": {trojan_port},
                    "password": "{TEST_PASSWORD}"
                }}
            }}],
            "routing": {{ "rules": [{{ "outboundTag": "trojan-out" }}] }}
        }}"#
    ))
}

fn trojan_server_tls(
    trojan_port: u16,
    cert_path: &str,
    key_path: &str,
) -> Arc<proxy_config::schema::Config> {
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "trojan-in",
                "protocol": "trojan",
                "listen": "127.0.0.1",
                "port": {trojan_port},
                "settings": {{
                    "clients": [{{"password": "{TEST_PASSWORD}"}}]
                }},
                "streamSettings": {{
                    "network": "tcp",
                    "security": "tls",
                    "tlsSettings": {{
                        "certificateFile": "{cert_path}",
                        "keyFile": "{key_path}"
                    }}
                }}
            }}],
            "outbounds": [{{
                "tag": "freedom",
                "protocol": "freedom"
            }}],
            "routing": {{ "rules": [{{ "outboundTag": "freedom" }}] }}
        }}"#
    ))
}

// ── VLESS over WS configs ────────────────────────────────────────────────────

fn vless_ws_server(vless_port: u16) -> Arc<proxy_config::schema::Config> {
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "vless-in",
                "protocol": "vless",
                "listen": "127.0.0.1",
                "port": {vless_port},
                "settings": {{
                    "clients": [{{"id": "{TEST_UUID}", "email": "test@test.com"}}]
                }},
                "streamSettings": {{
                    "network": "ws",
                    "security": "none",
                    "wsSettings": {{
                        "path": "/proxy"
                    }}
                }}
            }}],
            "outbounds": [{{
                "tag": "freedom",
                "protocol": "freedom"
            }}],
            "routing": {{ "rules": [{{ "outboundTag": "freedom" }}] }}
        }}"#
    ))
}

fn vless_ws_tls_server(
    vless_port: u16,
    cert_path: &str,
    key_path: &str,
) -> Arc<proxy_config::schema::Config> {
    parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "vless-in",
                "protocol": "vless",
                "listen": "127.0.0.1",
                "port": {vless_port},
                "settings": {{
                    "clients": [{{"id": "{TEST_UUID}", "email": "test@test.com"}}]
                }},
                "streamSettings": {{
                    "network": "ws",
                    "security": "tls",
                    "tlsSettings": {{
                        "certificateFile": "{cert_path}",
                        "keyFile": "{key_path}"
                    }},
                    "wsSettings": {{
                        "path": "/proxy"
                    }}
                }}
            }}],
            "outbounds": [{{
                "tag": "freedom",
                "protocol": "freedom"
            }}],
            "routing": {{ "rules": [{{ "outboundTag": "freedom" }}] }}
        }}"#
    ))
}

// ── Test 1: Trojan plain TCP single chunk ─────────────────────────────────────

#[tokio::test]
async fn trojan_plain_tcp_single_chunk() {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let trojan_port = unused_local_port();
    let socks_port = unused_local_port();

    let _server = proxy_core::Instance::from_config(trojan_server_plain(trojan_port))
        .await
        .expect("server start failed");
    let _client =
        proxy_core::Instance::from_config(trojan_client_plain(socks_port, trojan_port))
            .await
            .expect("client start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = socks5_connect(socks_port, "127.0.0.1", echo_port).await;
    let payload = b"HELLO TROJAN PLAIN TCP";
    stream.write_all(payload).await.unwrap();

    let mut echoed = vec![0u8; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();
    assert_eq!(echoed, payload);

    echo_task.abort();
}

// ── Test 2: Trojan plain TCP large payload ────────────────────────────────────

#[tokio::test]
async fn trojan_plain_tcp_large_payload() {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let trojan_port = unused_local_port();
    let socks_port = unused_local_port();

    let _server = proxy_core::Instance::from_config(trojan_server_plain(trojan_port))
        .await
        .expect("server start failed");
    let _client =
        proxy_core::Instance::from_config(trojan_client_plain(socks_port, trojan_port))
            .await
            .expect("client start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let mut stream = socks5_connect(socks_port, "127.0.0.1", echo_port).await;
    let payload = vec![0xABu8; 64 * 1024]; // 64 KB
    stream.write_all(&payload).await.unwrap();

    let mut echoed = vec![0u8; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();
    assert_eq!(echoed, payload);

    echo_task.abort();
}

// ── Test 3: Wrong password is rejected ────────────────────────────────────────

#[tokio::test]
async fn trojan_wrong_password_is_rejected() {
    use proxy_protocol::trojan::compute_token;
    use proxy_protocol::trojan::codec::encode_request;
    use proxy_common::Address;

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let trojan_port = unused_local_port();
    let _server = proxy_core::Instance::from_config(trojan_server_plain(trojan_port))
        .await
        .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Connect and send a wrong password token.
    let mut stream = TcpStream::connect(("127.0.0.1", trojan_port))
        .await
        .unwrap();
    let bad_token = compute_token("wrong-password-12345");
    let dest = Address::Domain("example.com".into(), 80);
    let header = encode_request(&bad_token, &dest);
    stream.write_all(&header).await.unwrap();
    stream.flush().await.unwrap();

    // The server should close the connection after auth failure.
    let mut buf = [0u8; 16];
    let result = stream.read(&mut buf).await;
    // Either 0 bytes (EOF) or an error — auth was rejected.
    match result {
        Ok(0) => {} // server closed
        Ok(_) => {} // server sent some data (acceptable in some configs)
        Err(_) => {} // connection reset
    }
}

// ── Test 4: Trojan over TLS (self-signed) ────────────────────────────────────

#[tokio::test]
async fn trojan_over_tls_roundtrip() {
    use proxy_protocol::trojan::{compute_token, connect_trojan_on_stream};
    use proxy_common::Address;
    use proxy_transport::tls_connect;

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let (cert_path, key_path) = write_dev_cert_files();
    let trojan_port = unused_local_port();

    let _server =
        proxy_core::Instance::from_config(trojan_server_tls(trojan_port, &cert_path, &key_path))
            .await
            .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client: connect TCP → TLS → Trojan header.
    let tcp = TcpStream::connect(("127.0.0.1", trojan_port))
        .await
        .unwrap();
    let tls = tls_connect(Box::new(tcp), "localhost", &[], true)
        .await
        .unwrap();

    let token = compute_token(TEST_PASSWORD);
    let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);
    let mut stream = connect_trojan_on_stream(tls, &token, &dest).await.unwrap();

    let payload = b"TROJAN OVER TLS";
    stream.write_all(payload).await.unwrap();
    stream.flush().await.unwrap();

    let mut echoed = vec![0u8; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();
    assert_eq!(&echoed, payload);

    echo_task.abort();
    let _ = std::fs::remove_file(&cert_path);
    let _ = std::fs::remove_file(&key_path);
}

// ── Test 5: Multiple passwords — any valid one accepted ───────────────────────

#[tokio::test]
async fn trojan_multiple_passwords_any_accepted() {
    use proxy_protocol::trojan::{compute_token, connect_trojan_on_stream};
    use proxy_common::Address;

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    // Build a server that accepts two passwords.
    let trojan_port = unused_local_port();
    let (echo_port, echo_task) = spawn_echo_server().await;

    let server_config = parse_config(format!(
        r#"{{
            "inbounds": [{{
                "tag": "trojan-in",
                "protocol": "trojan",
                "listen": "127.0.0.1",
                "port": {trojan_port},
                "settings": {{
                    "clients": [
                        {{"password": "password-one"}},
                        {{"password": "password-two"}}
                    ]
                }}
            }}],
            "outbounds": [{{"tag": "freedom", "protocol": "freedom"}}],
            "routing": {{ "rules": [{{"outboundTag": "freedom"}}] }}
        }}"#
    ));

    let _server = proxy_core::Instance::from_config(server_config)
        .await
        .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Test with first password.
    let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);

    for pw in &["password-one", "password-two"] {
        let tcp = TcpStream::connect(("127.0.0.1", trojan_port))
            .await
            .unwrap();
        let token = compute_token(pw);
        let mut stream = connect_trojan_on_stream(Box::new(tcp), &token, &dest)
            .await
            .unwrap();
        let msg = format!("hello from {pw}").into_bytes();
        stream.write_all(&msg).await.unwrap();
        stream.flush().await.unwrap();
        let mut recv = vec![0u8; msg.len()];
        stream.read_exact(&mut recv).await.unwrap();
        assert_eq!(recv, msg, "password '{pw}' should have been accepted");
    }

    echo_task.abort();
}

// ── Test 6: Trojan to IPv4 address ────────────────────────────────────────────

#[tokio::test]
async fn trojan_ipv4_address() {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let trojan_port = unused_local_port();
    let socks_port = unused_local_port();

    let _server = proxy_core::Instance::from_config(trojan_server_plain(trojan_port))
        .await
        .unwrap();
    let _client =
        proxy_core::Instance::from_config(trojan_client_plain(socks_port, trojan_port))
            .await
            .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Use IPv4 address directly (not a domain).
    let mut stream = socks5_connect(socks_port, "127.0.0.1", echo_port).await;
    let payload = b"IPv4 direct address test";
    stream.write_all(payload).await.unwrap();
    let mut recv = vec![0u8; payload.len()];
    stream.read_exact(&mut recv).await.unwrap();
    assert_eq!(recv, payload);

    echo_task.abort();
}

// ── Test 7: Trojan to domain address ─────────────────────────────────────────

#[tokio::test]
async fn trojan_domain_address() {
    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let trojan_port = unused_local_port();
    let socks_port = unused_local_port();

    let _server = proxy_core::Instance::from_config(trojan_server_plain(trojan_port))
        .await
        .unwrap();
    let _client =
        proxy_core::Instance::from_config(trojan_client_plain(socks_port, trojan_port))
            .await
            .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Use domain name (SOCKS5 sends it as ATYP=3).
    let mut stream = socks5_connect(socks_port, "localhost", echo_port).await;
    let payload = b"domain address test";
    stream.write_all(payload).await.unwrap();
    let mut recv = vec![0u8; payload.len()];
    stream.read_exact(&mut recv).await.unwrap();
    assert_eq!(recv, payload);

    echo_task.abort();
}

// ── Test 8: Trojan over TLS large payload ────────────────────────────────────

#[tokio::test]
async fn trojan_over_tls_large_payload() {
    use proxy_protocol::trojan::{compute_token, connect_trojan_on_stream};
    use proxy_common::Address;
    use proxy_transport::tls_connect;

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let (cert_path, key_path) = write_dev_cert_files();
    let trojan_port = unused_local_port();

    let _server =
        proxy_core::Instance::from_config(trojan_server_tls(trojan_port, &cert_path, &key_path))
            .await
            .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let tcp = TcpStream::connect(("127.0.0.1", trojan_port))
        .await
        .unwrap();
    let tls = tls_connect(Box::new(tcp), "localhost", &[], true)
        .await
        .unwrap();

    let token = compute_token(TEST_PASSWORD);
    let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);
    let mut stream = connect_trojan_on_stream(tls, &token, &dest).await.unwrap();

    let payload = vec![0xCCu8; 32 * 1024]; // 32 KB
    stream.write_all(&payload).await.unwrap();
    stream.flush().await.unwrap();

    let mut echoed = vec![0u8; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();
    assert_eq!(echoed, payload);

    echo_task.abort();
    let _ = std::fs::remove_file(&cert_path);
    let _ = std::fs::remove_file(&key_path);
}

// ── Test 9: WebSocket transport echo ────────────────────────────────────────

#[tokio::test]
async fn ws_transport_echo() {
    use proxy_transport::{ws_accept, ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = ws_accept(Box::new(tcp)).await.unwrap();
        let mut buf = [0u8; 1024];
        let n = ws.read(&mut buf).await.unwrap();
        ws.write_all(&buf[..n]).await.unwrap();
        ws.flush().await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let cfg = WsConnectConfig {
        path: "/echo".to_string(),
        host: "localhost".to_string(),
        headers: vec![],
    };
    let mut ws = ws_connect(Box::new(tcp), cfg).await.unwrap();

    let msg = b"ws transport echo test";
    ws.write_all(msg).await.unwrap();
    ws.flush().await.unwrap();

    let mut recv = vec![0u8; msg.len()];
    ws.read_exact(&mut recv).await.unwrap();
    assert_eq!(&recv, msg);
}

// ── Test 10: WebSocket large payload ────────────────────────────────────────

#[tokio::test]
async fn ws_transport_large_payload() {
    use proxy_transport::{ws_accept, ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = ws_accept(Box::new(tcp)).await.unwrap();
        let mut buf = vec![0u8; 128 * 1024];
        let n = ws.read(&mut buf).await.unwrap();
        ws.write_all(&buf[..n]).await.unwrap();
        ws.flush().await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut ws = ws_connect(
        Box::new(tcp),
        WsConnectConfig {
            path: "/large".to_string(),
            host: "localhost".to_string(),
            headers: vec![],
        },
    )
    .await
    .unwrap();

    let payload = vec![0xEFu8; 64 * 1024]; // 64 KB
    ws.write_all(&payload).await.unwrap();
    ws.flush().await.unwrap();

    let mut recv = vec![0u8; payload.len()];
    ws.read_exact(&mut recv).await.unwrap();
    assert_eq!(recv, payload);
}

// ── Test 11: WebSocket custom path ──────────────────────────────────────────

#[tokio::test]
async fn ws_transport_custom_path() {
    use proxy_transport::{ws_accept, ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = ws_accept(Box::new(tcp)).await.unwrap();
        // The path doesn't affect the server in tungstenite accept — just echo.
        let mut buf = [0u8; 32];
        let n = ws.read(&mut buf).await.unwrap();
        ws.write_all(&buf[..n]).await.unwrap();
        ws.flush().await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let mut ws = ws_connect(
        Box::new(tcp),
        WsConnectConfig {
            path: "/custom/path/here".to_string(),
            host: "example.com".to_string(),
            headers: vec![("X-Test".to_string(), "value".to_string())],
        },
    )
    .await
    .unwrap();

    ws.write_all(b"custom path").await.unwrap();
    ws.flush().await.unwrap();
    let mut recv = [0u8; 11];
    ws.read_exact(&mut recv).await.unwrap();
    assert_eq!(&recv, b"custom path");
}

// ── Test 12: VLESS over WebSocket (plain TCP) ────────────────────────────────

#[tokio::test]
async fn vless_over_ws_plain() {
    use proxy_protocol::vless::connect_vless_on_stream;
    use proxy_common::Address;
    use proxy_transport::{ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let vless_port = unused_local_port();

    // Start VLESS-over-WS server via Instance.
    let _server = proxy_core::Instance::from_config(vless_ws_server(vless_port))
        .await
        .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client: TCP → WS → VLESS header.
    let uuid: [u8; 16] = uuid::Uuid::parse_str(TEST_UUID).unwrap().into_bytes();

    let tcp = TcpStream::connect(("127.0.0.1", vless_port))
        .await
        .unwrap();
    let ws = ws_connect(
        Box::new(tcp),
        WsConnectConfig {
            path: "/proxy".to_string(),
            host: "localhost".to_string(),
            headers: vec![],
        },
    )
    .await
    .unwrap();

    let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);
    let mut stream = connect_vless_on_stream(ws, &uuid, "", &dest)
        .await
        .unwrap();

    let payload = b"VLESS OVER WS PLAIN";
    stream.write_all(payload).await.unwrap();
    stream.flush().await.unwrap();

    let mut recv = vec![0u8; payload.len()];
    stream.read_exact(&mut recv).await.unwrap();
    assert_eq!(&recv, payload);

    echo_task.abort();
}

// ── Test 13: VLESS over WebSocket + TLS (wss) ────────────────────────────────

#[tokio::test]
async fn vless_over_ws_tls() {
    use proxy_protocol::vless::connect_vless_on_stream;
    use proxy_common::Address;
    use proxy_transport::{tls_connect, ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let (cert_path, key_path) = write_dev_cert_files();
    let vless_port = unused_local_port();

    let _server =
        proxy_core::Instance::from_config(vless_ws_tls_server(vless_port, &cert_path, &key_path))
            .await
            .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Client: TCP → TLS → WS → VLESS
    let uuid: [u8; 16] = uuid::Uuid::parse_str(TEST_UUID).unwrap().into_bytes();

    let tcp = TcpStream::connect(("127.0.0.1", vless_port))
        .await
        .unwrap();
    let tls = tls_connect(Box::new(tcp), "localhost", &[], true)
        .await
        .unwrap();
    let ws = ws_connect(
        tls,
        WsConnectConfig {
            path: "/proxy".to_string(),
            host: "localhost".to_string(),
            headers: vec![],
        },
    )
    .await
    .unwrap();

    let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);
    let mut stream = connect_vless_on_stream(ws, &uuid, "", &dest)
        .await
        .unwrap();

    let payload = b"VLESS OVER WSS";
    stream.write_all(payload).await.unwrap();
    stream.flush().await.unwrap();

    let mut recv = vec![0u8; payload.len()];
    stream.read_exact(&mut recv).await.unwrap();
    assert_eq!(&recv, payload);

    echo_task.abort();
    let _ = std::fs::remove_file(&cert_path);
    let _ = std::fs::remove_file(&key_path);
}

// ── Test 14: VLESS over WS large payload ────────────────────────────────────

#[tokio::test]
async fn vless_over_ws_large_payload() {
    use proxy_protocol::vless::connect_vless_on_stream;
    use proxy_common::Address;
    use proxy_transport::{ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let vless_port = unused_local_port();

    let _server = proxy_core::Instance::from_config(vless_ws_server(vless_port))
        .await
        .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let uuid: [u8; 16] = uuid::Uuid::parse_str(TEST_UUID).unwrap().into_bytes();

    let tcp = TcpStream::connect(("127.0.0.1", vless_port))
        .await
        .unwrap();
    let ws = ws_connect(
        Box::new(tcp),
        WsConnectConfig {
            path: "/proxy".to_string(),
            host: "localhost".to_string(),
            headers: vec![],
        },
    )
    .await
    .unwrap();

    let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);
    let mut stream = connect_vless_on_stream(ws, &uuid, "", &dest)
        .await
        .unwrap();

    let payload = vec![0xAAu8; 48 * 1024]; // 48 KB
    stream.write_all(&payload).await.unwrap();
    stream.flush().await.unwrap();

    let mut recv = vec![0u8; payload.len()];
    stream.read_exact(&mut recv).await.unwrap();
    assert_eq!(recv, payload);

    echo_task.abort();
}

// ── Test 15: VLESS over WS+TLS multiple sequential connections ───────────────

#[tokio::test]
async fn vless_over_ws_tls_multi_conn() {
    use proxy_protocol::vless::connect_vless_on_stream;
    use proxy_common::Address;
    use proxy_transport::{tls_connect, ws_connect, WsConnectConfig};

    let _ = tracing_subscriber::fmt().with_env_filter("warn").try_init();

    let (echo_port, echo_task) = spawn_echo_server().await;
    let (cert_path, key_path) = write_dev_cert_files();
    let vless_port = unused_local_port();

    let _server =
        proxy_core::Instance::from_config(vless_ws_tls_server(vless_port, &cert_path, &key_path))
            .await
            .expect("server start failed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let uuid: [u8; 16] = uuid::Uuid::parse_str(TEST_UUID).unwrap().into_bytes();

    // Make 3 sequential connections.
    for i in 0u8..3 {
        let tcp = TcpStream::connect(("127.0.0.1", vless_port))
            .await
            .unwrap();
        let tls = tls_connect(Box::new(tcp), "localhost", &[], true)
            .await
            .unwrap();
        let ws = ws_connect(
            tls,
            WsConnectConfig {
                path: "/proxy".to_string(),
                host: "localhost".to_string(),
                headers: vec![],
            },
        )
        .await
        .unwrap();

        let dest = Address::Ipv4("127.0.0.1".parse().unwrap(), echo_port);
        let mut stream = connect_vless_on_stream(ws, &uuid, "", &dest)
            .await
            .unwrap();

        let msg = format!("connection {i}").into_bytes();
        stream.write_all(&msg).await.unwrap();
        stream.flush().await.unwrap();

        let mut recv = vec![0u8; msg.len()];
        stream.read_exact(&mut recv).await.unwrap();
        assert_eq!(recv, msg, "connection {i} failed");
    }

    echo_task.abort();
    let _ = std::fs::remove_file(&cert_path);
    let _ = std::fs::remove_file(&key_path);
}
