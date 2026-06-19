//! QUIC endpoint construction helpers.
//!
//! QUIC is a UDP-based transport protocol built on TLS 1.3. It supports
//! multiple simultaneous bidirectional streams over a single connection,
//! built-in loss recovery, and 0-RTT connection establishment.
//!
//! This module provides helpers to build Quinn QUIC server and client endpoints
//! with the TLS configuration required for Hysteria2.

pub mod badnet;
mod brutal_cc;

pub use badnet::{
    BadNetControllerFactory, CongestionConfig, CongestionDirection, CongestionMode,
    LossFingerprint, PathSample,
};
pub use brutal_cc::{BrutalCC, BrutalCCFactory};

use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{
    default_runtime, ClientConfig, Endpoint, EndpointConfig, ServerConfig, TransportConfig,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::RootCertStore;
use socket2::{Domain, Protocol as SocketProtocol, Socket, Type};

/// Tuning knobs applied when opening a QUIC UDP socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuicSocketConfig {
    /// Whether to set `SO_REUSEPORT` so multiple endpoints can share one address.
    pub reuse_port: bool,
    /// Number of parallel Quinn endpoint shards (unused by the socket itself; passed through for metrics).
    pub endpoint_count: usize,
    /// Requested kernel UDP receive-buffer size in bytes.
    pub recv_buffer_bytes: usize,
    /// Requested kernel UDP send-buffer size in bytes.
    pub send_buffer_bytes: usize,
}

impl Default for QuicSocketConfig {
    fn default() -> Self {
        Self {
            reuse_port: false,
            endpoint_count: 1,
            recv_buffer_bytes: 8 * 1024 * 1024,
            send_buffer_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Install the rustls crypto provider used by this workspace.
///
/// Several dependencies may enable different rustls provider features. Calling
/// this before building TLS configs makes QUIC startup deterministic.
/// Install the workspace rustls crypto provider (idempotent).
///
/// Required before any `ClientConfig::builder()` / `ServerConfig::builder()` use when
/// tests or callers have not already gone through `tls_connect` / `tls_accept`.
pub fn ensure_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Build a QUIC server endpoint.
///
/// Parses the certificate and private key from PEM strings, sets up TLS with
/// ALPN `["h3"]`, and opens a UDP socket at `addr`.
///
/// # Arguments
/// * `addr`     — UDP socket address to bind
/// * `cert_pem` — PEM-encoded certificate chain
/// * `key_pem`  — PEM-encoded private key (PKCS#8 or PKCS#1)
pub fn build_server_endpoint(addr: SocketAddr, cert_pem: &str, key_pem: &str) -> Result<Endpoint> {
    build_server_endpoint_with_alpn(addr, cert_pem, key_pem, &[b"h3".to_vec()])
}

/// Build a QUIC server endpoint with explicit ALPN values.
pub fn build_server_endpoint_with_alpn(
    addr: SocketAddr,
    cert_pem: &str,
    key_pem: &str,
    alpn_protocols: &[Vec<u8>],
) -> Result<Endpoint> {
    build_server_endpoint_with_alpn_and_socket(
        addr,
        cert_pem,
        key_pem,
        alpn_protocols,
        QuicSocketConfig::default(),
    )
}

/// Build a QUIC server endpoint with explicit ALPN values and socket configuration.
pub fn build_server_endpoint_with_alpn_and_socket(
    addr: SocketAddr,
    cert_pem: &str,
    key_pem: &str,
    alpn_protocols: &[Vec<u8>],
    socket_config: QuicSocketConfig,
) -> Result<Endpoint> {
    ensure_crypto_provider();

    let (certs, key) = parse_cert_and_key(cert_pem, key_pem)?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("invalid TLS certificate or key")?;

    tls_config.alpn_protocols = alpn_protocols.to_vec();

    let quic_server_config = QuicServerConfig::try_from(tls_config)
        .context("failed to build QUIC server config from TLS config")?;

    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));

    // Set a 30-second idle timeout so stale connections are cleaned up even
    // when the client disappears without sending a proper close.
    let mut transport = TransportConfig::default();
    let idle_timeout = Duration::from_secs(30)
        .try_into()
        .expect("constant 30s idle timeout fits in quinn IdleTimeout");
    transport.max_idle_timeout(Some(idle_timeout));
    transport.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
    transport.datagram_send_buffer_size(2 * 1024 * 1024);
    server_config.transport_config(Arc::new(transport));

    endpoint_server_with_socket(server_config, addr, socket_config)
        .context("failed to open QUIC server endpoint")
}

/// Build a QUIC server endpoint for Hysteria2 inbounds.
///
/// Same as [`build_server_endpoint`] but enables QUIC datagrams and tunes
/// flow-control windows to match the configured bandwidth.
///
/// # Arguments
/// * `up_mbps`   — max client→server throughput in Mbit/s (used to size receive window)
/// * `down_mbps` — max server→client throughput in Mbit/s (used to size send window)
pub fn build_hysteria2_server_endpoint(
    addr: SocketAddr,
    cert_pem: &str,
    key_pem: &str,
    up_mbps: u64,
    down_mbps: u64,
) -> Result<Endpoint> {
    build_hysteria2_server_endpoint_with_congestion(
        addr, cert_pem, key_pem, up_mbps, down_mbps, None,
    )
}

/// Build a Hysteria2 server endpoint with a specific congestion configuration.
pub fn build_hysteria2_server_endpoint_with_congestion(
    addr: SocketAddr,
    cert_pem: &str,
    key_pem: &str,
    up_mbps: u64,
    down_mbps: u64,
    congestion: Option<CongestionConfig>,
) -> Result<Endpoint> {
    build_hysteria2_server_endpoint_with_congestion_and_socket(
        addr,
        cert_pem,
        key_pem,
        up_mbps,
        down_mbps,
        congestion,
        QuicSocketConfig::default(),
    )
}

/// Build a Hysteria2 server endpoint with congestion configuration and socket tuning.
pub fn build_hysteria2_server_endpoint_with_congestion_and_socket(
    addr: SocketAddr,
    cert_pem: &str,
    key_pem: &str,
    up_mbps: u64,
    down_mbps: u64,
    congestion: Option<CongestionConfig>,
    socket_config: QuicSocketConfig,
) -> Result<Endpoint> {
    ensure_crypto_provider();

    let (certs, key) = parse_cert_and_key(cert_pem, key_pem)?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("invalid TLS certificate or key")?;

    tls_config.alpn_protocols = vec![b"h3".to_vec()];

    let quic_server_config = QuicServerConfig::try_from(tls_config)
        .context("failed to build QUIC server config from TLS config")?;

    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));

    let mut transport = TransportConfig::default();
    let idle_timeout = Duration::from_secs(30)
        .try_into()
        .expect("constant 30s idle timeout fits in quinn IdleTimeout");
    transport.max_idle_timeout(Some(idle_timeout));
    transport.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
    transport.datagram_send_buffer_size(2 * 1024 * 1024);
    if let Some(cfg) = congestion.as_ref() {
        match cfg.mode {
            CongestionMode::StandardQuic => {}
            CongestionMode::BrutalCompatible => {
                transport.congestion_controller_factory(Arc::new(BrutalCCFactory::new(
                    cfg.target_bps_for(CongestionDirection::ServerDownload),
                )));
            }
            CongestionMode::NovaCc
            | CongestionMode::BadNetLowLatency
            | CongestionMode::BadNetThroughput
            | CongestionMode::AutoProbe => {
                transport.congestion_controller_factory(Arc::new(
                    BadNetControllerFactory::new_for_direction(
                        cfg.clone(),
                        CongestionDirection::ServerDownload,
                    ),
                ));
            }
        }
    }

    // Size QUIC flow-control windows to the configured bandwidth × 500 ms RTT
    // (BDP for a satellite/high-latency link). This prevents BrutalCC from being
    // stalled by the flow-control window before the congestion window fills.
    let (stream_rx, conn_rx, conn_tx) = congestion
        .as_ref()
        .map(|cfg| bdp_windows_with_profile(up_mbps, down_mbps, cfg.window_profile()))
        .unwrap_or_else(|| bdp_windows(up_mbps, down_mbps));
    transport.stream_receive_window(stream_rx);
    transport.receive_window(conn_rx);
    transport.send_window(conn_tx);

    server_config.transport_config(Arc::new(transport));

    endpoint_server_with_socket(server_config, addr, socket_config)
        .context("failed to open Hysteria2 QUIC endpoint")
}

pub(crate) fn bdp_windows(rx_mbps: u64, tx_mbps: u64) -> (quinn::VarInt, quinn::VarInt, u64) {
    bdp_windows_with_profile(
        rx_mbps,
        tx_mbps,
        badnet::WindowProfile {
            bdp_rtt: Duration::from_millis(500),
            min_window_bytes: 8 * 1024 * 1024,
            max_window_bytes: 128 * 1024 * 1024,
            conn_window_multiplier: 3,
        },
    )
}

/// Compute (stream_receive_window, connection_receive_window, connection_send_window)
/// from configured bandwidth limits and a congestion-mode window profile.
pub(crate) fn bdp_windows_with_profile(
    rx_mbps: u64,
    tx_mbps: u64,
    profile: badnet::WindowProfile,
) -> (quinn::VarInt, quinn::VarInt, u64) {
    let rx_bps = rx_mbps.saturating_mul(1_000_000 / 8);
    let tx_bps = tx_mbps.saturating_mul(1_000_000 / 8);
    let rtt_ms = profile.bdp_rtt.as_millis().try_into().unwrap_or(u64::MAX);

    let stream_rx = (rx_bps.saturating_mul(rtt_ms) / 1000)
        .clamp(profile.min_window_bytes, profile.max_window_bytes);
    // Connection receive window covers multiple concurrent streams.
    let conn_rx = stream_rx
        .saturating_mul(profile.conn_window_multiplier)
        .min(profile.max_window_bytes);
    let conn_tx = (tx_bps.saturating_mul(rtt_ms) / 1000)
        .clamp(profile.min_window_bytes, profile.max_window_bytes);

    (
        quinn::VarInt::from_u64(stream_rx).unwrap_or(quinn::VarInt::MAX),
        quinn::VarInt::from_u64(conn_rx).unwrap_or(quinn::VarInt::MAX),
        conn_tx,
    )
}

/// Build a QUIC client endpoint.
///
/// When `skip_verify` is `true`, TLS certificate validation is disabled.
/// This is useful for development with self-signed certificates but MUST NOT
/// be used in production.
pub fn build_client_endpoint(skip_verify: bool) -> Result<Endpoint> {
    build_client_endpoint_with_alpn(skip_verify, &[b"h3".to_vec()])
}

/// Build a QUIC client endpoint with explicit ALPN values.
pub fn build_client_endpoint_with_alpn(
    skip_verify: bool,
    alpn_protocols: &[Vec<u8>],
) -> Result<Endpoint> {
    build_client_endpoint_with_alpn_and_socket(
        skip_verify,
        alpn_protocols,
        QuicSocketConfig::default(),
    )
}

/// Build a QUIC client endpoint with explicit ALPN values and socket configuration.
pub fn build_client_endpoint_with_alpn_and_socket(
    skip_verify: bool,
    alpn_protocols: &[Vec<u8>],
    socket_config: QuicSocketConfig,
) -> Result<Endpoint> {
    ensure_crypto_provider();

    let mut tls_config = if skip_verify {
        build_no_verify_client_tls()
    } else {
        build_default_client_tls()?
    };
    tls_config.alpn_protocols = alpn_protocols.to_vec();

    let quic_client_config = QuicClientConfig::try_from(tls_config)
        .context("failed to build QUIC client config from TLS config")?;

    let mut client_config = ClientConfig::new(Arc::new(quic_client_config));
    let mut transport = TransportConfig::default();
    transport.datagram_receive_buffer_size(Some(2 * 1024 * 1024));
    transport.datagram_send_buffer_size(2 * 1024 * 1024);
    client_config.transport_config(Arc::new(transport));

    // Bind to any available local port.
    let bind_addr = "0.0.0.0:0"
        .parse()
        .context("invalid client bind address literal")?;
    let mut endpoint = endpoint_client_with_socket(bind_addr, socket_config)
        .context("failed to open client socket")?;
    endpoint.set_default_client_config(client_config);

    Ok(endpoint)
}

/// Generate a throwaway self-signed certificate and key for testing.
///
/// Returns `(cert_pem, key_pem)`. The certificate is valid for `localhost`.
/// Do not use in production — these certs are generated fresh every run
/// and are not persisted anywhere.
pub fn dev_self_signed() -> Result<(String, String)> {
    dev_self_signed_for_names(&["localhost".to_string()])
}

/// Self-signed cert for dev/test with arbitrary DNS SAN entries (e.g. REALITY cover SNI).
pub fn dev_self_signed_for_names(names: &[String]) -> Result<(String, String)> {
    let subjects = if names.is_empty() {
        vec!["localhost".to_string()]
    } else {
        names.to_vec()
    };
    let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(subjects)
        .context("failed to generate self-signed certificate")?;
    Ok((cert.pem(), signing_key.serialize_pem()))
}

// ── Private helpers ────────────────────────────────────────────────────────────

fn endpoint_server_with_socket(
    server_config: ServerConfig,
    addr: SocketAddr,
    socket_config: QuicSocketConfig,
) -> Result<Endpoint> {
    let socket = tuned_udp_socket(addr, socket_config)?;
    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    Endpoint::new(
        EndpointConfig::default(),
        Some(server_config),
        socket,
        runtime,
    )
    .context("constructing Quinn endpoint from tuned UDP socket")
}

fn endpoint_client_with_socket(
    addr: SocketAddr,
    socket_config: QuicSocketConfig,
) -> Result<Endpoint> {
    let socket = tuned_udp_socket(addr, socket_config)?;
    let runtime = default_runtime().ok_or_else(|| anyhow::anyhow!("no async runtime found"))?;
    Endpoint::new(EndpointConfig::default(), None, socket, runtime)
        .context("constructing Quinn client endpoint from tuned UDP socket")
}

fn tuned_udp_socket(addr: SocketAddr, cfg: QuicSocketConfig) -> Result<UdpSocket> {
    let socket = Socket::new(
        Domain::for_address(addr),
        Type::DGRAM,
        Some(SocketProtocol::UDP),
    )
    .context("creating UDP socket")?;
    if addr.is_ipv6() {
        let _ = socket.set_only_v6(false);
    }
    socket
        .set_reuse_address(true)
        .context("setting SO_REUSEADDR")?;
    if cfg.reuse_port {
        #[cfg(unix)]
        socket
            .set_reuse_port(true)
            .context("setting SO_REUSEPORT")?;
        #[cfg(not(unix))]
        tracing::debug!("SO_REUSEPORT requested but unsupported on this target");
    }
    let _ = socket.set_recv_buffer_size(cfg.recv_buffer_bytes);
    let _ = socket.set_send_buffer_size(cfg.send_buffer_bytes);
    socket
        .bind(&addr.into())
        .with_context(|| format!("binding UDP socket {addr}"))?;

    let actual_recv = socket.recv_buffer_size().unwrap_or(0);
    let actual_send = socket.send_buffer_size().unwrap_or(0);
    record_socket_metrics(&cfg, actual_recv, actual_send);

    Ok(socket.into())
}

fn record_socket_metrics(cfg: &QuicSocketConfig, actual_recv: usize, actual_send: usize) {
    metrics::counter!("blackwire_quic_endpoint_active_total").increment(1);
    metrics::counter!("blackwire_quic_socket_drops_total").increment(0);
    metrics::gauge!("blackwire_quic_recv_buffer_bytes").set(actual_recv as f64);
    metrics::gauge!("blackwire_quic_send_buffer_bytes").set(actual_send as f64);
    metrics::gauge!("blackwire_quic_endpoint_shards").set(cfg.endpoint_count as f64);
}

/// Record bytes transferred through a named QUIC endpoint for metrics.
pub fn record_endpoint_io(endpoint: &'static str, direction: &'static str, bytes: usize) {
    metrics::counter!(
        "blackwire_quic_endpoint_packets_total",
        "endpoint" => endpoint,
        "direction" => direction
    )
    .increment(1);
    metrics::counter!(
        "blackwire_quic_endpoint_bytes_total",
        "endpoint" => endpoint,
        "direction" => direction
    )
    .increment(bytes as u64);
}

/// Parse PEM cert chain + PEM private key into rustls types.
fn parse_cert_and_key(
    cert_pem: &str,
    key_pem: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    crate::pem::parse_cert_and_key(cert_pem, key_pem).map_err(|e| anyhow::Error::msg(e.to_string()))
}

/// Build a client TLS config that accepts any server certificate.
///
/// For use in tests and development only. Skips all certificate chain and
/// hostname validation.
fn build_no_verify_client_tls() -> rustls::ClientConfig {
    rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth()
}

/// Build a client TLS config that uses the platform's native root certificates.
///
/// Falls back to an empty trust store if native roots fail to load.
fn build_default_client_tls() -> Result<rustls::ClientConfig> {
    let mut roots = RootCertStore::empty();
    // load_native_certs() returns a CertificateResult with .certs and .errors.
    let result = rustls_native_certs::load_native_certs();
    if !result.errors.is_empty() {
        tracing::warn!(
            "some native root certificates failed to load: {} errors",
            result.errors.len()
        );
    }
    for cert in result.certs {
        // Ignore individual parse errors — one bad root cert in the OS
        // store should not prevent the proxy from connecting.
        let _ = roots.add(cert);
    }
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

/// A TLS certificate verifier that accepts any certificate without validation.
///
/// This is intentionally insecure — only for use in tests and development.
#[derive(Debug)]
struct NoVerifier;

impl rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        // Accept any signature scheme to not block connections.
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_self_signed_returns_valid_pem() {
        let (cert_pem, key_pem) = dev_self_signed().unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("PRIVATE KEY"));
    }

    #[test]
    fn parse_cert_and_key_roundtrip() {
        let (cert_pem, key_pem) = dev_self_signed().unwrap();
        let (certs, _key) = parse_cert_and_key(&cert_pem, &key_pem).unwrap();
        assert!(!certs.is_empty());
    }

    #[test]
    fn brutal_cc_factory_builds_controller() {
        use quinn::congestion::ControllerFactory;
        use std::sync::Arc;
        let factory = Arc::new(BrutalCCFactory::new(12_500_000));
        // ControllerFactory::build takes self: Arc<Self>, so clone to preserve the factory.
        let ctrl = Arc::clone(&factory).build(std::time::Instant::now(), 1200);
        // Window must be at least MIN_WINDOW (32 KiB).
        assert!(ctrl.window() >= 32 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn tuned_udp_socket_allows_reuse_port_shards() {
        let cfg = QuicSocketConfig {
            reuse_port: true,
            endpoint_count: 2,
            recv_buffer_bytes: 1024 * 1024,
            send_buffer_bytes: 1024 * 1024,
        };
        let first = tuned_udp_socket("127.0.0.1:0".parse().unwrap(), cfg).unwrap();
        let addr = first.local_addr().unwrap();
        let second = tuned_udp_socket(addr, cfg).unwrap();
        assert_eq!(second.local_addr().unwrap(), addr);
    }
}
