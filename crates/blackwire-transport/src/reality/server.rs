use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::Result;
use hkdf::Hkdf;
use sha2::Sha256;
use tokio::time::timeout;
use tracing::{debug, warn};
use x25519_dalek::{PublicKey, StaticSecret};

use blackwire_common::{
    copy_bidirectional_with_idle_limits, tcp_connect, BoxedStream, PrependedStream, ProxyError,
    RelayRateLimit, CONNECTION_IDLE_TIMEOUT,
};

use super::parser::{parse_client_hello, reality_auth_peer_public_keys, ClientHelloFields};
use super::tls13::{
    read_client_hello_message, read_cover_handshake_profile, CoverHandshakeProfile,
    MAX_CLIENT_HELLO_SIZE,
};
use super::{MAX_TIME_DIFF_SECS, REALITY_HKDF_INFO, SESSION_ID_OFFSET_IN_HANDSHAKE_BODY};

const REALITY_RECORD_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const REALITY_COVER_PROFILE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Stream ready for TLS after successful REALITY authentication.
pub struct RealityAccepted {
    /// Accepted stream positioned for the next protocol stage.
    pub stream: BoxedStream,
    /// Per-connection key used by later REALITY/TLS steps.
    pub auth_key: [u8; 32],
    /// Cached visible TLS characteristics learned from the configured cover.
    pub cover_profile: Option<CoverHandshakeProfile>,
}

/// REALITY server configuration read from the inbound config.
pub struct RealityServerConfig {
    /// The server's long-term X25519 private key. Keep this secret.
    pub private_key: [u8; 32],

    /// Valid short IDs for this server. Clients must present one of them.
    pub short_ids: Vec<Vec<u8>>,

    /// Allowed ClientHello SNI values. Wildcards are intentionally unsupported.
    pub server_names: Vec<String>,

    /// Real HTTPS destination used when authentication fails.
    pub fallback: SocketAddr,

    /// Maximum allowed clock skew in seconds.
    pub max_time_diff: i64,
}

/// REALITY server: authenticates incoming connections or forwards them away.
pub struct RealityServer {
    config: Arc<RealityServerConfig>,
    private_key: StaticSecret,
    cover_profile: Arc<tokio::sync::OnceCell<CoverHandshakeProfile>>,
    cover_profile_probe_started: Arc<AtomicBool>,
    fallback_upload_limit: Option<RelayRateLimit>,
    fallback_download_limit: Option<RelayRateLimit>,
}

impl RealityServer {
    /// Create a REALITY server helper from inbound settings.
    ///
    /// If `max_time_diff` is non-positive, the default safety window is used.
    pub fn new(mut config: RealityServerConfig) -> Self {
        if config.max_time_diff <= 0 {
            config.max_time_diff = MAX_TIME_DIFF_SECS;
        }
        config.server_names = config
            .server_names
            .into_iter()
            .map(|name| name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect();
        config.server_names.sort();
        config.server_names.dedup();
        let private_key = StaticSecret::from(config.private_key);
        Self {
            private_key,
            config: Arc::new(config),
            cover_profile: Arc::new(tokio::sync::OnceCell::new()),
            cover_profile_probe_started: Arc::new(AtomicBool::new(false)),
            fallback_upload_limit: None,
            fallback_download_limit: None,
        }
    }

    /// Apply optional pacing only to unauthenticated fallback relays.
    pub fn with_fallback_limits(
        mut self,
        upload: Option<RelayRateLimit>,
        download: Option<RelayRateLimit>,
    ) -> Self {
        self.fallback_upload_limit = upload;
        self.fallback_download_limit = download;
        self
    }

    /// Accept a connection and replay the ClientHello for post-auth TLS.
    ///
    /// The returned [`PrependedStream`] replays the exact ClientHello bytes for
    /// [`complete_tls13_server_handshake`](crate::reality::complete_tls13_server_handshake).
    pub async fn accept(&self, stream: BoxedStream) -> Result<BoxedStream, ProxyError> {
        Ok(self.accept_with_key(stream).await?.stream)
    }

    /// Like [`accept`](Self::accept) but also returns the per-connection REALITY auth key.
    pub async fn accept_with_key(
        &self,
        stream: BoxedStream,
    ) -> Result<RealityAccepted, ProxyError> {
        let (stream, auth_key, cover_profile) = self
            .accept_inner(stream, ReplayMode::PrependClientHello)
            .await?;
        Ok(RealityAccepted {
            stream,
            auth_key,
            cover_profile,
        })
    }

    /// Accept a connection without replaying the ClientHello.
    ///
    /// Direct mode skips ClientHello replay. After authentication, the next
    /// readable byte is the VLESS header (no TLS 1.3 completion on this path).
    pub async fn accept_direct(&self, stream: BoxedStream) -> Result<BoxedStream, ProxyError> {
        Ok(self
            .accept_inner(stream, ReplayMode::ConsumeClientHello)
            .await?
            .0)
    }

    async fn accept_inner(
        &self,
        mut stream: BoxedStream,
        replay_mode: ReplayMode,
    ) -> Result<(BoxedStream, [u8; 32], Option<CoverHandshakeProfile>), ProxyError> {
        // Establish the cover connection before classifying the client, then
        // mirror the ClientHello as it is consumed. The bounded reader handles
        // TLS record fragmentation without adding unbounded per-connection
        // memory or a classification-dependent cover dial delay.
        let mut fallback = tcp_connect(self.config.fallback)
            .await
            .map_err(|e| ProxyError::Transport(format!("fallback connect: {e}")))?;
        let handshake_body = match timeout(
            REALITY_RECORD_READ_TIMEOUT,
            read_client_hello_message(&mut stream, Some(&mut fallback)),
        )
        .await
        {
            Ok(Ok(body)) => body,
            Ok(Err(e)) => {
                debug!(error = %e, "ClientHello read failed — continuing cover relay");
                return self.do_connected_fallback(stream, fallback).await;
            }
            Err(_) => {
                debug!("ClientHello read timed out — continuing cover relay");
                return self.do_connected_fallback(stream, fallback).await;
            }
        };

        let fields = match parse_client_hello(&handshake_body) {
            Ok(f) => f,
            Err(e) => {
                debug!(error = %e, "ClientHello parse failed — forwarding to fallback");
                return self.do_connected_fallback(stream, fallback).await;
            }
        };
        if !self
            .config
            .server_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&fields.sni))
        {
            debug!("ClientHello SNI not in REALITY allow-list — forwarding to fallback");
            return self.do_connected_fallback(stream, fallback).await;
        }

        let auth_key = match self.derive_auth_key(&fields, &handshake_body) {
            Ok(key) => key,
            Err(e) => {
                debug!(error = %e, "REALITY authentication failed — forwarding to fallback");
                return self.do_connected_fallback(stream, fallback).await;
            }
        };

        debug!("REALITY authentication succeeded");
        let stream = match replay_mode {
            ReplayMode::PrependClientHello => {
                let replay = encode_handshake_records(&handshake_body);
                Box::new(PrependedStream::new(stream, replay)) as BoxedStream
            }
            ReplayMode::ConsumeClientHello => stream,
        };
        let cover_profile = self.cover_profile.get().cloned();
        if cover_profile.is_none()
            && self
                .cover_profile_probe_started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let profile_cache = Arc::clone(&self.cover_profile);
            let probe_started = Arc::clone(&self.cover_profile_probe_started);
            tokio::spawn(async move {
                match timeout(
                    REALITY_COVER_PROFILE_TIMEOUT,
                    read_cover_handshake_profile(&mut fallback),
                )
                .await
                {
                    Ok(Ok(profile)) => {
                        if profile.selected_group != 0x001d {
                            warn!(
                                selected_group = profile.selected_group,
                                "REALITY cover TLS fingerprint is incompatible with Blackwire's X25519 server handshake; cover shaping remains disabled"
                            );
                        }
                        let _ = profile_cache.set(profile);
                    }
                    Ok(Err(error)) => {
                        debug!(error = %error, "unable to learn REALITY cover handshake profile");
                        probe_started.store(false, Ordering::Release);
                    }
                    Err(_) => {
                        debug!("timed out learning REALITY cover handshake profile");
                        probe_started.store(false, Ordering::Release);
                    }
                }
            });
        }
        Ok((stream, auth_key, cover_profile))
    }

    /// Derive the REALITY auth key and validate the encrypted session token.
    fn derive_auth_key(
        &self,
        fields: &ClientHelloFields,
        handshake_body: &[u8],
    ) -> Result<[u8; 32]> {
        let peer_keys = reality_auth_peer_public_keys_or_fallback(fields, handshake_body);
        let zeroed_aad = xray_zeroed_session_id_aad(handshake_body)?;
        let wire_aad = handshake_body.to_vec();

        for (peer_idx, peer_pub) in peer_keys.iter().enumerate() {
            let peer_kind = peer_key_kind(peer_idx);
            let auth_key =
                match derive_reality_auth_key(&self.private_key, peer_pub, &fields.random) {
                    Ok(key) => key,
                    Err(_) => continue,
                };

            // Xray/sing-box: Seal(..., hello.SessionId[:16], hello.Raw) with session_id zeroed in Raw.
            // XTLS REALITY server Open(..., hs.clientHello.original) uses zeroed original.
            for (aad_mode, aad) in [
                (RealityAadMode::Zeroed, zeroed_aad.as_slice()),
                (RealityAadMode::Wire, wire_aad.as_slice()),
            ] {
                if let Some(token) = decrypt_and_validate_reality_token(
                    fields,
                    &auth_key,
                    aad,
                    &self.config.short_ids,
                    self.config.max_time_diff,
                ) {
                    log_reality_auth_ok(peer_kind, aad_mode, fields, &token, &auth_key);
                    return Ok(auth_key);
                }
            }

            // Do not brute-force plaintext-session AAD variants here. This path is
            // unauthenticated and must remain constant-work with respect to the
            // configured short_ids and max_time_diff window.
        }

        Err(anyhow::anyhow!("REALITY authentication failed"))
    }

    /// Forward to the real fallback HTTPS site and finish the connection there.
    async fn do_connected_fallback(
        &self,
        mut stream: BoxedStream,
        mut fallback: tokio::net::TcpStream,
    ) -> Result<(BoxedStream, [u8; 32], Option<CoverHandshakeProfile>), ProxyError> {
        debug!(fallback = %self.config.fallback, "continuing REALITY cover relay");
        copy_bidirectional_with_idle_limits(
            &mut stream,
            &mut fallback,
            CONNECTION_IDLE_TIMEOUT,
            self.fallback_upload_limit,
            self.fallback_download_limit,
        )
        .await;

        Err(ProxyError::FallbackRequired)
    }
}

#[derive(Clone, Copy)]
enum ReplayMode {
    PrependClientHello,
    ConsumeClientHello,
}

fn encode_handshake_records(handshake: &[u8]) -> Vec<u8> {
    const MAX_RECORD: usize = 16 * 1024;
    let record_count = handshake.len().div_ceil(MAX_RECORD);
    let mut wire = Vec::with_capacity(handshake.len() + record_count * 5);
    for chunk in handshake.chunks(MAX_RECORD) {
        wire.extend_from_slice(&[0x16, 0x03, 0x03]);
        wire.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
        wire.extend_from_slice(chunk);
    }
    debug_assert!(handshake.len() <= MAX_CLIENT_HELLO_SIZE);
    wire
}

#[derive(Clone, Copy)]
enum RealityAadMode {
    Zeroed,
    Wire,
}

impl RealityAadMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Zeroed => "zeroed",
            Self::Wire => "wire",
        }
    }
}

#[derive(Clone, Copy)]
enum RealityPeerKeyKind {
    X25519,
    MlkemTail,
}

impl RealityPeerKeyKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::X25519 => "x25519",
            Self::MlkemTail => "mlkem_tail",
        }
    }
}

/// Standalone X25519 first, ML-KEM768 tail second; fallback to parsed key_share.
fn reality_auth_peer_public_keys_or_fallback(
    fields: &ClientHelloFields,
    handshake_body: &[u8],
) -> Vec<[u8; 32]> {
    let mut peer_keys = reality_auth_peer_public_keys(handshake_body);
    if peer_keys.is_empty() {
        peer_keys.push(fields.x25519_key_share);
    }
    peer_keys
}

fn peer_key_kind(peer_idx: usize) -> RealityPeerKeyKind {
    if peer_idx == 0 {
        RealityPeerKeyKind::X25519
    } else {
        RealityPeerKeyKind::MlkemTail
    }
}

fn derive_reality_auth_key(
    private_key: &StaticSecret,
    peer_pub: &[u8; 32],
    client_random: &[u8; 32],
) -> Result<[u8; 32]> {
    let shared_secret = private_key.diffie_hellman(&PublicKey::from(*peer_pub));
    let hk = Hkdf::<Sha256>::new(Some(&client_random[..20]), shared_secret.as_bytes());
    let mut auth_key = [0u8; 32];
    hk.expand(REALITY_HKDF_INFO, &mut auth_key)
        .map_err(|_| anyhow::anyhow!("HKDF expand failed"))?;
    Ok(auth_key)
}

fn decrypt_and_validate_reality_token(
    fields: &ClientHelloFields,
    auth_key: &[u8; 32],
    aad: &[u8],
    short_ids: &[Vec<u8>],
    max_time_diff: i64,
) -> Option<Vec<u8>> {
    let plaintext = decrypt_reality_session_id(fields, auth_key, aad).ok()?;
    validate_reality_token(&plaintext, short_ids, max_time_diff).ok()?;
    if !reality_auth_roundtrip_matches(fields, auth_key, aad, &plaintext) {
        return None;
    }
    Some(plaintext)
}

fn log_reality_auth_ok(
    peer_kind: RealityPeerKeyKind,
    aad_mode: RealityAadMode,
    fields: &ClientHelloFields,
    token: &[u8],
    _auth_key: &[u8; 32],
) {
    let version = if token.len() >= 4 {
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            token[0], token[1], token[2], token[3]
        )
    } else {
        "????".to_string()
    };
    let short_id_hex = hex::encode(token.get(8..16).unwrap_or(&[]));
    debug!(
        peer_key = peer_kind.as_str(),
        aad = aad_mode.as_str(),
        client_version = %version,
        "REALITY auth succeeded"
    );
    if std::env::var_os("REALITY_DEBUG_HELLO").is_some() {
        debug!(
            sni = %fields.sni,
            random_prefix = %hex::encode(&fields.random[..4]),
            short_id = %short_id_hex,
            "REALITY_DEBUG_HELLO"
        );
    }
}

/// AAD with session_id zeroed — matches Xray/sing-box Seal input and REALITY Open original.
fn xray_zeroed_session_id_aad(handshake_body: &[u8]) -> Result<Vec<u8>> {
    let sid_start = SESSION_ID_OFFSET_IN_HANDSHAKE_BODY;
    let sid_end = sid_start + 32;
    if handshake_body.len() < sid_end {
        anyhow::bail!("handshake body too short to contain session_id");
    }

    let mut aad = handshake_body.to_vec();
    aad[sid_start..sid_end].fill(0);
    Ok(aad)
}

fn decrypt_reality_session_id(
    fields: &ClientHelloFields,
    auth_key: &[u8; 32],
    aad: &[u8],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(auth_key));
    let nonce = Nonce::from_slice(&fields.random[20..32]);

    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &fields.session_id,
                aad,
            },
        )
        .map_err(|_| anyhow::anyhow!("AES-256-GCM decryption failed (bad client?)"))
}

fn encrypt_session_id(
    fields: &ClientHelloFields,
    auth_key: &[u8; 32],
    aad: &[u8],
    plaintext16: &[u8],
) -> Result<[u8; 32]> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(auth_key));
    let nonce = Nonce::from_slice(&fields.random[20..32]);
    let output = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext16,
                aad,
            },
        )
        .map_err(|_| anyhow::anyhow!("AES-256-GCM encryption failed"))?;
    output
        .try_into()
        .map_err(|_| anyhow::anyhow!("REALITY token encryption length mismatch"))
}

fn reality_auth_roundtrip_matches(
    fields: &ClientHelloFields,
    auth_key: &[u8; 32],
    aad: &[u8],
    plaintext: &[u8],
) -> bool {
    if plaintext.len() < 16 {
        return false;
    }
    encrypt_session_id(fields, auth_key, aad, &plaintext[..16])
        .map(|enc| enc == fields.session_id)
        .unwrap_or(false)
}

fn validate_reality_token(
    plaintext: &[u8],
    allowed_short_ids: &[Vec<u8>],
    max_time_diff: i64,
) -> Result<()> {
    if plaintext.len() < 16 {
        anyhow::bail!("decrypted token too short: {} bytes", plaintext.len());
    }

    let ts = u32::from_be_bytes([plaintext[4], plaintext[5], plaintext[6], plaintext[7]]) as i64;
    let short_id = &plaintext[8..16];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let diff = (now - ts).abs();
    if diff > max_time_diff {
        anyhow::bail!("timestamp skew too large: {diff}s (max {max_time_diff}s)");
    }

    let effective_short_id = strip_zero_padding(short_id);
    let valid = allowed_short_ids
        .iter()
        .any(|allowed| allowed.as_slice() == effective_short_id);
    if !valid {
        anyhow::bail!("short_id not in allowed list");
    }

    Ok(())
}

fn strip_zero_padding(short_id: &[u8]) -> &[u8] {
    let last_nonzero = short_id
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    &short_id[..last_nonzero]
}

#[cfg(test)]
mod tests {
    use super::*;
    use x25519_dalek::{PublicKey, StaticSecret};

    /// Static sing-box Chrome ClientHello fixture (see `testdata/README.md`).
    /// Does not enforce wall-clock freshness; verifies decrypt, auth key, and cert HMAC.
    #[test]
    fn docker_singbox_chrome_hello_authenticates() {
        let hello = include_bytes!("testdata/singbox-chrome-hello.bin");
        let priv_hex = "6f4850ca51ced64b4acfd90c73fd60392c0c2f92744933b28b1bc0f7b8683d79";
        let priv_bytes: [u8; 32] = hex::decode(priv_hex).unwrap().try_into().unwrap();
        let short_id = hex::decode("aabbccdd00000001").unwrap();

        let server = RealityServer::new(RealityServerConfig {
            private_key: priv_bytes,
            short_ids: vec![short_id],
            server_names: vec!["www.microsoft.com".to_string()],
            fallback: "127.0.0.1:80".parse().unwrap(),
            // This fixture is a static Docker capture. Keep the test focused on
            // Xray/sing-box REALITY decrypt + cert HMAC, not wall-clock freshness.
            max_time_diff: 10 * 365 * 24 * 60 * 60,
        });

        let fields = parse_client_hello(hello).expect("parse captured hello");
        let auth_key = server
            .derive_auth_key(&fields, hello)
            .expect("matrix server must authenticate captured sing-box hello");
        let (cert, _) =
            crate::reality::cert::tls_cert_for_auth_key(&auth_key, "www.microsoft.com", false)
                .unwrap();
        crate::reality::cert::verify_reality_cert_hmac(&auth_key, &cert)
            .expect("cert HMAC must verify with same auth_key");
    }

    #[test]
    fn matrix_lab_reality_keypair_is_valid() {
        let priv_hex = "6f4850ca51ced64b4acfd90c73fd60392c0c2f92744933b28b1bc0f7b8683d79";
        let pub_hex = "968612b14962343a5327f212761e90dc0ddf31ced39da41fb839694be2b8e96a";
        let priv_bytes: [u8; 32] = hex::decode(priv_hex).unwrap().try_into().unwrap();
        let expected_pub: [u8; 32] = hex::decode(pub_hex).unwrap().try_into().unwrap();
        let secret = StaticSecret::from(priv_bytes);
        let derived = *PublicKey::from(&secret).as_bytes();
        assert_eq!(derived, expected_pub);
    }
}
