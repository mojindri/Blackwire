//! Hysteria2 UDP datagram relay micro-benchmark.
//!
//! Measures the server-side UDP hot path: a `Hysteria2UdpSession` client sends
//! datagrams through the Hysteria2 server's datagram relay to a loopback UDP
//! echo server and reads the echoes back. The proxy pair is built once, outside
//! the measured loop, so only per-datagram relay cost is timed.
//!
//! This exists to verify the per-session reader / inline-send hot path against
//! the previous per-datagram task-spawn design. Run with:
//!
//! ```bash
//! cargo bench -p blackwire-benches --bench udp_relay
//! ```

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::net::UdpSocket;
use tokio::runtime::Runtime;
use tokio::time::timeout;

const TEST_PASSWORD: &str = "hysteria2-udp-bench-pw";
const RECV_TIMEOUT: Duration = Duration::from_secs(2);

fn bench_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
}

fn unused_local_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("port reserve")
        .local_addr()
        .expect("port addr")
        .port()
}

async fn spawn_udp_echo_server() -> (u16, tokio::task::JoinHandle<()>) {
    let sock = UdpSocket::bind("127.0.0.1:0").await.expect("echo bind");
    let port = sock.local_addr().expect("echo addr").port();
    let handle = tokio::spawn(async move {
        let mut buf = [0u8; 65535];
        loop {
            let Ok((n, peer)) = sock.recv_from(&mut buf).await else {
                break;
            };
            let _ = sock.send_to(&buf[..n], peer).await;
        }
    });
    (port, handle)
}

fn write_dev_cert_files() -> (String, String) {
    let (cert_pem, key_pem) = blackwire_transport::dev_self_signed().expect("dev cert");
    let dir = std::env::temp_dir();
    let unique = format!(
        "blackwire-udp-bench-{}-{}",
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

fn server_config(
    hysteria_port: u16,
    cert_path: &str,
    key_path: &str,
) -> Arc<blackwire_config::schema::Config> {
    let cert_json = serde_json::to_string(cert_path).expect("serialize cert path");
    let key_json = serde_json::to_string(key_path).expect("serialize key path");
    Arc::new(
        serde_json::from_str(&format!(
            r#"{{
                "datagram": {{"enabled": true, "udpOverDatagram": true}},
                "inbounds": [{{
                    "tag": "hysteria2-udp-in",
                    "protocol": "hysteria2",
                    "listen": "127.0.0.1",
                    "port": {hysteria_port},
                    "settings": {{
                        "auth": "{TEST_PASSWORD}",
                        "upMbps": 1000,
                        "downMbps": 1000
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
        .expect("config parse"),
    )
}

fn client_config(hysteria_port: u16) -> blackwire_transport::Hysteria2ClientConfig {
    blackwire_transport::Hysteria2ClientConfig {
        server: format!("127.0.0.1:{hysteria_port}").parse().unwrap(),
        server_name: "localhost".to_string(),
        password: TEST_PASSWORD.to_string(),
        up_mbps: 1000,
        down_mbps: 1000,
        skip_cert_verify: true,
        congestion: blackwire_transport::CongestionConfig {
            up_mbps: 1000,
            down_mbps: 1000,
            ..blackwire_transport::CongestionConfig::default()
        },
        endpoint_shards: 1,
        socket: blackwire_transport::QuicSocketConfig::default(),
        datagram_enabled: true,
        fec: blackwire_transport::FecPolicy::default(),
        datagram_policy: blackwire_transport::DatagramPolicy::default(),
    }
}

/// A running Hysteria2 server, UDP echo server, and an authenticated client
/// UDP session. Built once and reused for the whole bench group.
struct UdpRelayPair {
    session: blackwire_transport::Hysteria2UdpSession,
    echo_port: u16,
    _server: blackwire_core::Instance,
    _echo: tokio::task::JoinHandle<()>,
    cert_path: String,
    key_path: String,
}

impl UdpRelayPair {
    async fn new() -> Self {
        let hysteria_port = unused_local_port();
        let (cert_path, key_path) = write_dev_cert_files();
        let server = blackwire_core::Instance::from_config(server_config(
            hysteria_port,
            &cert_path,
            &key_path,
        ))
        .await
        .expect("Hysteria2 server start");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (echo_port, echo) = spawn_udp_echo_server().await;
        let session = timeout(
            Duration::from_secs(5),
            blackwire_transport::Hysteria2UdpSession::connect(&client_config(hysteria_port)),
        )
        .await
        .expect("connect timed out")
        .expect("connect failed");
        Self {
            session,
            echo_port,
            _server: server,
            _echo: echo,
            cert_path,
            key_path,
        }
    }

    /// Relay `count` datagrams of `payload_len` bytes through the tunnel while
    /// keeping up to `window` datagrams in flight, reading every echo back.
    ///
    /// Pipelining keeps the server's UDP relay continuously busy so the
    /// measurement reflects per-datagram server cost (task spawn vs inline send
    /// + shared reader) rather than a single datagram's QUIC round-trip latency,
    /// which on loopback dwarfs that cost. Returns the number of echoes read.
    async fn relay(&self, count: usize, window: usize, payload_len: usize) -> usize {
        let dest = blackwire_transport::UdpDestination::V4(Ipv4Addr::LOCALHOST, self.echo_port);
        let payload = Bytes::from(vec![0xABu8; payload_len]);

        let mut sent = 0usize;
        let mut received = 0usize;
        let prime = window.min(count);
        for _ in 0..prime {
            if self.session.send(dest.clone(), payload.clone()).is_err() {
                return received;
            }
            sent += 1;
        }
        while received < count {
            match timeout(RECV_TIMEOUT, self.session.recv()).await {
                Ok(Ok(_)) => {
                    received += 1;
                    if sent < count && self.session.send(dest.clone(), payload.clone()).is_ok() {
                        sent += 1;
                    }
                }
                _ => break,
            }
        }
        received
    }
}

impl Drop for UdpRelayPair {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.cert_path);
        let _ = std::fs::remove_file(&self.key_path);
    }
}

fn bench_udp_relay(c: &mut Criterion) {
    let rt = bench_runtime();
    let pair = rt.block_on(UdpRelayPair::new());

    // Up to this many datagrams in flight at once, so the server's per-datagram
    // relay cost — not loopback round-trip latency — is the throughput limiter.
    const WINDOW: usize = 32;

    let mut group = c.benchmark_group("hysteria2_udp_relay");
    // Small, interactive-sized datagrams that fit in a single QUIC datagram:
    // this path is dominated by per-datagram overhead (task spawn, buffer alloc)
    // rather than byte copies, which is exactly what the per-session reader
    // refactor targets.
    for payload_len in [64usize, 512] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("pipelined", payload_len),
            &payload_len,
            |b, &payload_len| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let start = Instant::now();
                        let done = pair.relay(iters as usize, WINDOW, payload_len).await;
                        let elapsed = start.elapsed();
                        assert_eq!(done, iters as usize, "datagram round-trips lost");
                        elapsed
                    })
                });
            },
        );
    }
    group.finish();
}

criterion_group!(udp_relay, bench_udp_relay);
criterion_main!(udp_relay);
