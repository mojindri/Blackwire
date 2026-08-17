//! TLS 1.3 server handshake for REALITY (post-auth camouflage).

use ed25519_dalek::{Signer, SigningKey};
use rand::RngExt;
use x25519_dalek::{PublicKey, StaticSecret};

use blackwire_common::{BoxedStream, ProxyError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::{
    decrypt_app_record, derive_app_keys, derive_handshake_keys, encrypt_app_record,
    read_client_hello_message, read_record_stream, split_handshake_messages,
    write_handshake_record, AppKeys, CipherSuite, HsKeys, HS_CERTIFICATE, HS_CERTIFICATE_VERIFY,
    HS_ENCRYPTED_EXTENSIONS, HS_FINISHED, HS_SERVER_HELLO, RT_ALERT, RT_APPLICATION_DATA,
    RT_CHANGE_CIPHER_SPEC, RT_HANDSHAKE,
};

const SIG_ED25519: u16 = 0x0807;
const GROUP_X25519: u16 = 0x001d;
// Must match Go TLS `serverSignatureContext`; Xray/sing-box verify this with uTLS.
const TLS13_SERVER_CERT_VERIFY_CONTEXT: &[u8] = b"TLS 1.3, server CertificateVerify\x00";

/// Small, reusable description of the cover server's visible TLS 1.3 flight.
/// It intentionally contains no ephemeral keys or certificate material.
#[derive(Clone, Debug)]
pub struct CoverHandshakeProfile {
    pub(crate) cipher_suite: CipherSuite,
    pub(crate) selected_group: u16,
    pub(crate) sends_ccs: bool,
    /// TLS record body lengths for the encrypted server handshake flight.
    pub(crate) encrypted_record_lengths: Vec<usize>,
}

/// Learn the cover's visible handshake shape from a connection that has
/// already received the mirrored ClientHello. The result is bounded and safe
/// to cache across connections because it excludes all ephemeral key data.
pub(crate) async fn read_cover_handshake_profile(
    cover: &mut TcpStream,
) -> Result<CoverHandshakeProfile, ProxyError> {
    const MAX_PROFILE_BYTES: usize = 64 * 1024;
    const MAX_PROFILE_RECORDS: usize = 8;

    let (header, server_hello) = read_tcp_record(cover).await?;
    if header[0] != RT_HANDSHAKE {
        return Err(ProxyError::Protocol(
            "cover did not return a TLS ServerHello".into(),
        ));
    }
    let (cipher_suite, selected_group, _) = super::parse_server_hello(&server_hello)?;
    let mut total = header.len() + server_hello.len();
    let mut sends_ccs = false;
    let mut encrypted_record_lengths = Vec::with_capacity(2);

    while encrypted_record_lengths.len() < MAX_PROFILE_RECORDS && total < MAX_PROFILE_BYTES {
        let (record_header, body) = read_tcp_record(cover).await?;
        total = total.saturating_add(record_header.len() + body.len());
        match record_header[0] {
            RT_CHANGE_CIPHER_SPEC => sends_ccs = true,
            RT_APPLICATION_DATA => {
                encrypted_record_lengths.push(body.len());
                // A normal TLS 1.3 server flight is usually one or two records.
                // Stop after the first full-size-short record; reading further
                // risks waiting for post-handshake tickets.
                if body.len() < 16 * 1024 {
                    break;
                }
            }
            RT_ALERT => break,
            _ => break,
        }
    }

    if encrypted_record_lengths.is_empty() {
        return Err(ProxyError::Protocol(
            "cover returned no encrypted TLS handshake record".into(),
        ));
    }
    Ok(CoverHandshakeProfile {
        cipher_suite,
        selected_group,
        sends_ccs,
        encrypted_record_lengths,
    })
}

async fn read_tcp_record(tcp: &mut TcpStream) -> Result<([u8; 5], Vec<u8>), ProxyError> {
    let mut header = [0u8; 5];
    tcp.read_exact(&mut header).await?;
    let body_len = u16::from_be_bytes([header[3], header[4]]) as usize;
    if body_len > 18 * 1024 {
        return Err(ProxyError::Protocol(format!(
            "cover TLS record too large: {body_len}"
        )));
    }
    let mut body = vec![0u8; body_len];
    tcp.read_exact(&mut body).await?;
    Ok((header, body))
}

/// Complete TLS 1.3 as server after REALITY auth.
pub async fn complete_tls13_server_handshake(
    stream: &mut BoxedStream,
    auth_key: &[u8; 32],
    cover_sni: &str,
    cover_profile: Option<&CoverHandshakeProfile>,
) -> Result<AppKeys, ProxyError> {
    let ch_body = read_client_hello_message(stream, None).await?;
    // Blackwire currently negotiates X25519. A cover selecting another group
    // cannot be mirrored safely, so retain the standard compatible shape.
    let cover_profile = cover_profile.filter(|profile| profile.selected_group == GROUP_X25519);

    let cs = pick_cipher_suite(&ch_body, cover_profile.map(|profile| profile.cipher_suite))?;
    let client_share = crate::reality::parse_client_hello(&ch_body)
        .map_err(|e| ProxyError::Protocol(e.to_string()))?
        .x25519_key_share;
    // External clients (sing-box) strip ML-KEM shares; use the standard cert template.
    let (cert_der, signing_key) =
        crate::reality::cert::tls_cert_for_auth_key(auth_key, cover_sni, false)?;
    crate::reality::cert::verify_reality_cert_hmac(auth_key, &cert_der)
        .map_err(|e| ProxyError::Tls(format!("REALITY cert self-check before send: {e}")))?;

    let server_tls_secret = StaticSecret::random();
    let server_tls_pub = PublicKey::from(&server_tls_secret);
    let server_pub_bytes = *server_tls_pub.as_bytes();

    let mut transcript = ch_body.clone();

    let session_id = parse_client_session_id(&ch_body)?;
    let sh_body = build_server_hello(cs, &server_pub_bytes, session_id);
    stream.write_all(&write_handshake_record(&sh_body)).await?;
    transcript.extend_from_slice(&sh_body);
    let client_tls_pub = PublicKey::from(client_share);
    let tls_dhe = server_tls_secret
        .diffie_hellman(&client_tls_pub)
        .as_bytes()
        .to_vec();
    let tls_dhe: [u8; 32] = tls_dhe
        .try_into()
        .map_err(|_| ProxyError::Protocol("TLS DHE secret length mismatch".into()))?;

    let transcript_hash_after_sh = cs.hash(&transcript);
    let hs_keys = derive_handshake_keys(cs, &tls_dhe, &transcript_hash_after_sh)?;

    // Follow the cover's visible CCS behavior when a profile is available.
    if cover_profile.is_none_or(|profile| profile.sends_ccs) {
        stream
            .write_all(&[RT_CHANGE_CIPHER_SPEC, 0x03, 0x03, 0x00, 0x01, 0x01])
            .await?;
    }

    let ee_msg = build_encrypted_extensions();
    transcript.extend_from_slice(&ee_msg);

    let cert_msg = build_certificate(&cert_der);
    transcript.extend_from_slice(&cert_msg);

    let cv_msg = build_certificate_verify(cs, &signing_key, &transcript)?;
    transcript.extend_from_slice(&cv_msg);

    let finished_hash = cs.hash(&transcript);
    let server_finished_data = cs.hmac(&hs_keys.server_finished_key, &finished_hash)?;
    let finished_msg = build_finished(server_finished_data);
    transcript.extend_from_slice(&finished_msg);

    let mut flight =
        Vec::with_capacity(ee_msg.len() + cert_msg.len() + cv_msg.len() + finished_msg.len());
    flight.extend_from_slice(&ee_msg);
    flight.extend_from_slice(&cert_msg);
    flight.extend_from_slice(&cv_msg);
    flight.extend_from_slice(&finished_msg);
    write_encrypted_flight(stream, cs, &hs_keys, &flight, cover_profile).await?;

    let app_transcript_hash = cs.hash(&transcript);
    let app_keys = derive_app_keys(cs, &hs_keys.master_secret, &app_transcript_hash)?;

    read_client_finished(stream, cs, &hs_keys, &app_transcript_hash).await?;

    Ok(app_keys)
}

async fn write_encrypted_flight(
    stream: &mut BoxedStream,
    cs: CipherSuite,
    hs_keys: &HsKeys,
    flight: &[u8],
    cover_profile: Option<&CoverHandshakeProfile>,
) -> Result<(), ProxyError> {
    let mut offset = 0usize;
    let mut seq = 0u64;
    if let Some(profile) = cover_profile {
        for &ciphertext_len in &profile.encrypted_record_lengths {
            if offset == flight.len() {
                break;
            }
            let content_capacity = ciphertext_len.saturating_sub(17);
            if content_capacity == 0 {
                continue;
            }
            let take = (flight.len() - offset).min(content_capacity);
            let record = super::encrypt_app_record_padded(
                cs,
                &hs_keys.server_key,
                &hs_keys.server_iv,
                seq,
                &flight[offset..offset + take],
                RT_HANDSHAKE,
                ciphertext_len,
            )?;
            stream.write_all(&record).await?;
            offset += take;
            seq += 1;
        }
    }
    while offset < flight.len() {
        let take = (flight.len() - offset).min(16 * 1024 - 1);
        let record = encrypt_app_record(
            cs,
            &hs_keys.server_key,
            &hs_keys.server_iv,
            seq,
            &flight[offset..offset + take],
            RT_HANDSHAKE,
        )?;
        stream.write_all(&record).await?;
        offset += take;
        seq += 1;
    }
    Ok(())
}

async fn read_client_finished(
    stream: &mut BoxedStream,
    cs: CipherSuite,
    hs_keys: &HsKeys,
    app_transcript_hash: &[u8],
) -> Result<(), ProxyError> {
    let mut cli_seq: u64 = 0;
    loop {
        let (rec_header, rec_body) = read_record_stream(stream).await?;
        match rec_header[0] {
            RT_CHANGE_CIPHER_SPEC => continue,
            RT_ALERT => {
                let desc = rec_body.get(1).copied().unwrap_or(0);
                return Err(ProxyError::Protocol(format!(
                    "TLS alert from client during handshake: desc={desc}"
                )));
            }
            RT_APPLICATION_DATA => {
                let (inner, inner_type) = decrypt_app_record(
                    cs,
                    &hs_keys.client_key,
                    &hs_keys.client_iv,
                    cli_seq,
                    &rec_body,
                    rec_header,
                )?;
                cli_seq += 1;
                if inner_type != RT_HANDSHAKE {
                    continue;
                }
                for (hs_type, msg_bytes) in split_handshake_messages(&inner) {
                    if hs_type == HS_FINISHED {
                        let body_start = 4;
                        let verify_data = &msg_bytes[body_start..];
                        let expected =
                            cs.hmac(&hs_keys.client_finished_key, app_transcript_hash)?;
                        if verify_data != expected.as_slice() {
                            return Err(ProxyError::Protocol(
                                "client Finished HMAC mismatch".into(),
                            ));
                        }
                        return Ok(());
                    }
                }
            }
            other => {
                return Err(ProxyError::Protocol(format!(
                    "unexpected TLS record 0x{other:02x} waiting for client Finished"
                )));
            }
        }
    }
}

fn pick_cipher_suite(
    ch_body: &[u8],
    cover_preference: Option<CipherSuite>,
) -> Result<CipherSuite, ProxyError> {
    let list = crate::reality::parser::client_hello_cipher_suites(ch_body)?;
    if let Some(preferred) = cover_preference {
        let preferred = preferred.to_u16();
        if list
            .chunks_exact(2)
            .any(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]) == preferred)
        {
            return CipherSuite::from_u16(preferred);
        }
    }
    for prefer in [0x1301u16, 0x1302] {
        for chunk in list.chunks_exact(2) {
            if u16::from_be_bytes([chunk[0], chunk[1]]) == prefer {
                return CipherSuite::from_u16(prefer);
            }
        }
    }
    Err(ProxyError::Protocol(
        "ClientHello offers no supported TLS 1.3 cipher suite".into(),
    ))
}

fn parse_client_session_id(ch_body: &[u8]) -> Result<&[u8], ProxyError> {
    crate::reality::parser::client_hello_session_id(ch_body)
}

fn build_server_hello(cs: CipherSuite, server_pub: &[u8; 32], session_id: &[u8]) -> Vec<u8> {
    let mut random = [0u8; 32];
    rand::rng().fill(&mut random[..]);

    let mut extensions = Vec::new();
    extensions.extend_from_slice(&[0x00, 0x2b, 0x00, 0x02, 0x03, 0x04]);
    extensions.extend_from_slice(&[0x00, 0x33, 0x00, 0x24]);
    extensions.extend_from_slice(&29u16.to_be_bytes());
    extensions.extend_from_slice(&32u16.to_be_bytes());
    extensions.extend_from_slice(server_pub);

    let body_len = 2 + 32 + 1 + session_id.len() + 2 + 1 + 2 + extensions.len();
    let mut body = Vec::with_capacity(4 + body_len);
    body.push(HS_SERVER_HELLO);
    body.push((body_len >> 16) as u8);
    body.push((body_len >> 8) as u8);
    body.push(body_len as u8);
    body.extend_from_slice(&[0x03, 0x03]);
    body.extend_from_slice(&random);
    body.push(session_id.len() as u8);
    body.extend_from_slice(session_id);
    body.extend_from_slice(&cs.to_u16().to_be_bytes());
    body.push(0);
    body.extend_from_slice(&(extensions.len() as u16).to_be_bytes());
    body.extend_from_slice(&extensions);
    body
}

fn build_encrypted_extensions() -> Vec<u8> {
    let body_len = 2u32;
    vec![
        HS_ENCRYPTED_EXTENSIONS,
        (body_len >> 16) as u8,
        (body_len >> 8) as u8,
        body_len as u8,
        0,
        0,
    ]
}

fn build_certificate(cert_der: &[u8]) -> Vec<u8> {
    let mut entry = Vec::with_capacity(3 + cert_der.len() + 2);
    let elen = cert_der.len();
    entry.push((elen >> 16) as u8);
    entry.push((elen >> 8) as u8);
    entry.push(elen as u8);
    entry.extend_from_slice(cert_der);
    entry.extend_from_slice(&[0x00, 0x00]);

    let mut cert_list = Vec::with_capacity(3 + entry.len());
    let list_len = entry.len();
    cert_list.push((list_len >> 16) as u8);
    cert_list.push((list_len >> 8) as u8);
    cert_list.push(list_len as u8);
    cert_list.extend_from_slice(&entry);

    let payload_len = 1 + cert_list.len();
    let mut msg = Vec::with_capacity(4 + payload_len);
    msg.push(HS_CERTIFICATE);
    msg.push((payload_len >> 16) as u8);
    msg.push((payload_len >> 8) as u8);
    msg.push(payload_len as u8);
    msg.push(0);
    msg.extend_from_slice(&cert_list);
    msg
}

fn build_certificate_verify(
    cs: CipherSuite,
    signing_key: &SigningKey,
    transcript: &[u8],
) -> Result<Vec<u8>, ProxyError> {
    let mut content = vec![0x20u8; 64];
    content.extend_from_slice(TLS13_SERVER_CERT_VERIFY_CONTEXT);
    content.extend_from_slice(&cs.hash(transcript));

    let signature = signing_key.sign(&content);
    let sig_bytes = signature.to_bytes();

    let payload_len = 2 + 2 + sig_bytes.len();
    let mut msg = Vec::with_capacity(4 + payload_len);
    msg.push(HS_CERTIFICATE_VERIFY);
    msg.push((payload_len >> 16) as u8);
    msg.push((payload_len >> 8) as u8);
    msg.push(payload_len as u8);
    msg.extend_from_slice(&SIG_ED25519.to_be_bytes());
    msg.extend_from_slice(&(sig_bytes.len() as u16).to_be_bytes());
    msg.extend_from_slice(&sig_bytes);
    Ok(msg)
}

fn build_finished(verify_data: Vec<u8>) -> Vec<u8> {
    let vd_len = verify_data.len() as u32;
    let mut msg = Vec::with_capacity(4 + verify_data.len());
    msg.push(HS_FINISHED);
    msg.push((vd_len >> 16) as u8);
    msg.push((vd_len >> 8) as u8);
    msg.push(vd_len as u8);
    msg.extend_from_slice(&verify_data);
    msg
}

#[cfg(test)]
mod tests {
    use super::super::parse_server_hello;
    use super::build_server_hello;
    use super::complete_tls13_server_handshake;
    use super::CipherSuite;
    use crate::reality::parse_client_hello;
    use crate::Tls13Stream;

    #[test]
    fn client_hello_key_share_matches_builder() {
        use blackwire_tls::ClientHelloBuilder;
        use x25519_dalek::{PublicKey, StaticSecret};

        let secret = StaticSecret::random();
        let pub_key = *PublicKey::from(&secret).as_bytes();
        let random = [7u8; 32];
        let session_id = [0u8; 32];
        let mut rng = rand::rng();
        let hello = ClientHelloBuilder::chrome_131().build_with_additional_key_share(
            "www.example.com",
            &random,
            &session_id,
            Some(&pub_key),
            None,
            &mut rng,
        );
        let fields = parse_client_hello(&hello[5..]).unwrap();
        assert_eq!(fields.x25519_key_share, pub_key);
    }

    #[test]
    fn encrypted_extensions_record_roundtrips() {
        use super::super::{decrypt_app_record, derive_handshake_keys, encrypt_app_record};

        let dhe = [1u8; 32];
        let th = [2u8; 32];
        let hs = derive_handshake_keys(CipherSuite::Aes128GcmSha256, &dhe, &th).unwrap();
        let ee = build_encrypted_extensions();
        let record = encrypt_app_record(
            CipherSuite::Aes128GcmSha256,
            &hs.server_key,
            &hs.server_iv,
            0,
            &ee,
            RT_HANDSHAKE,
        )
        .unwrap();
        let header: [u8; 5] = record[..5].try_into().unwrap();
        let (plain, ty) = decrypt_app_record(
            CipherSuite::Aes128GcmSha256,
            &hs.server_key,
            &hs.server_iv,
            0,
            &record[5..],
            header,
        )
        .unwrap();
        assert_eq!(plain, ee);
        assert_eq!(ty, RT_HANDSHAKE);
    }

    #[test]
    fn cover_shaped_record_matches_observed_length_and_roundtrips() {
        use super::super::{decrypt_app_record, derive_handshake_keys, encrypt_app_record_padded};

        let hs =
            derive_handshake_keys(CipherSuite::Aes128GcmSha256, &[1u8; 32], &[2u8; 32]).unwrap();
        let content = build_encrypted_extensions();
        let record = encrypt_app_record_padded(
            CipherSuite::Aes128GcmSha256,
            &hs.server_key,
            &hs.server_iv,
            0,
            &content,
            RT_HANDSHAKE,
            4096,
        )
        .unwrap();
        assert_eq!(record.len(), 5 + 4096);
        let header = record[..5].try_into().unwrap();
        let (plain, ty) = decrypt_app_record(
            CipherSuite::Aes128GcmSha256,
            &hs.server_key,
            &hs.server_iv,
            0,
            &record[5..],
            header,
        )
        .unwrap();
        assert_eq!(plain, content);
        assert_eq!(ty, RT_HANDSHAKE);
    }

    #[test]
    fn handshake_traffic_keys_match_manual() {
        use super::super::{derive_handshake_keys, parse_server_hello};
        use blackwire_tls::ClientHelloBuilder;
        use x25519_dalek::{PublicKey, StaticSecret};

        let client_secret = StaticSecret::random();
        let client_pub = *PublicKey::from(&client_secret).as_bytes();
        let server_secret = StaticSecret::random();
        let server_pub = *PublicKey::from(&server_secret).as_bytes();

        let mut rng = rand::rng();
        let hello = ClientHelloBuilder::chrome_131().build_with_additional_key_share(
            "www.example.com",
            &[1u8; 32],
            &[0u8; 32],
            Some(&client_pub),
            None,
            &mut rng,
        );
        let ch_body = hello[5..].to_vec();
        let sh_body = build_server_hello(CipherSuite::Aes128GcmSha256, &server_pub, &[0u8; 32]);

        let parsed_client = parse_client_hello(&ch_body).unwrap();
        let (_cs, _g, parsed_server_pub) = parse_server_hello(&sh_body).unwrap();
        assert_eq!(parsed_server_pub.as_slice(), server_pub);

        let mut server_pub_arr = [0u8; 32];
        server_pub_arr.copy_from_slice(&parsed_server_pub);
        let dhe_c = client_secret
            .diffie_hellman(&PublicKey::from(server_pub_arr))
            .as_bytes()
            .to_vec();
        let dhe_s = server_secret
            .diffie_hellman(&PublicKey::from(parsed_client.x25519_key_share))
            .as_bytes()
            .to_vec();
        assert_eq!(dhe_c, dhe_s);

        let mut transcript = ch_body;
        transcript.extend_from_slice(&sh_body);
        let th = CipherSuite::Aes128GcmSha256.hash(&transcript);
        let dhe: [u8; 32] = dhe_c.try_into().unwrap();
        let client_keys = derive_handshake_keys(CipherSuite::Aes128GcmSha256, &dhe, &th).unwrap();
        let server_keys = derive_handshake_keys(CipherSuite::Aes128GcmSha256, &dhe, &th).unwrap();
        assert_eq!(client_keys.server_key, server_keys.server_key);
        assert_eq!(client_keys.client_key, server_keys.client_key);
    }

    #[test]
    fn server_hello_parses_like_client() {
        let pub_key = [0xABu8; 32];
        let sh = build_server_hello(CipherSuite::Aes128GcmSha256, &pub_key, &[0u8; 32]);
        let (cs, group, key) = parse_server_hello(&sh).unwrap();
        assert_eq!(cs, CipherSuite::Aes128GcmSha256);
        assert_eq!(group, 29);
        assert_eq!(key.as_slice(), pub_key);
    }

    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use super::*;
    use crate::reality::{RealityClient, RealityClientConfig, RealityServer, RealityServerConfig};

    async fn spawn_cover_sink() -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0u8; 4096];
                while stream.read(&mut buffer).await.unwrap_or(0) != 0 {}
            }
        });
        address
    }

    #[tokio::test]
    async fn client_hello_split_across_tls_records_is_reassembled() {
        use blackwire_tls::ClientHelloBuilder;
        use tokio::io::AsyncWriteExt;

        let client_secret = StaticSecret::random();
        let client_pub = *PublicKey::from(&client_secret).as_bytes();
        let mut rng = rand::rng();
        let hello = ClientHelloBuilder::chrome_131().build_with_additional_key_share(
            "www.example.com",
            &[3u8; 32],
            &[0u8; 32],
            Some(&client_pub),
            None,
            &mut rng,
        );
        let handshake = &hello[5..];
        let split_at = 73;
        let mut fragmented = Vec::with_capacity(handshake.len() + 10);
        for fragment in [&handshake[..split_at], &handshake[split_at..]] {
            fragmented.extend_from_slice(&[RT_HANDSHAKE, 0x03, 0x03]);
            fragmented.extend_from_slice(&(fragment.len() as u16).to_be_bytes());
            fragmented.extend_from_slice(fragment);
        }

        let (mut writer, reader) = tokio::io::duplex(fragmented.len() + 32);
        tokio::spawn(async move {
            writer.write_all(&fragmented).await.unwrap();
        });
        let mut stream = Box::new(reader) as BoxedStream;
        let reassembled = read_client_hello_message(&mut stream, None).await.unwrap();
        assert_eq!(reassembled, handshake);
    }

    #[tokio::test]
    async fn self_client_server_tls13_roundtrip() {
        let priv_bytes =
            hex::decode("8cb13706aa547712de8f687dc32e66b0ec2e753ba310e734b72fb52ce5e6a4a8")
                .unwrap()
                .try_into()
                .unwrap();
        let pub_bytes =
            hex::decode("bbf29cec98e1aff519fcd09456d90407804f91ae62be4b8aac48f6d676807865")
                .unwrap()
                .try_into()
                .unwrap();
        let short_id = hex::decode("0123456789abcdef").unwrap();
        let fallback = spawn_cover_sink().await;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = Arc::new(RealityServer::new(RealityServerConfig {
            private_key: priv_bytes,
            short_ids: vec![short_id.clone()],
            server_names: vec!["www.example.com".to_string()],
            fallback,
            max_time_diff: 120,
        }));

        let (tx, rx) = oneshot::channel();
        let srv = server.clone();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let accepted = srv.accept_with_key(Box::new(tcp)).await.unwrap();
            let cover_profile = accepted.cover_profile;
            let mut stream = accepted.stream;
            let keys = complete_tls13_server_handshake(
                &mut stream,
                &accepted.auth_key,
                "www.example.com",
                cover_profile.as_ref(),
            )
            .await
            .unwrap();
            let mut tls = Tls13Stream::new_server(stream, keys);
            let mut buf = [0u8; 4];
            tls.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            tls.write_all(b"pong").await.unwrap();
            let _ = tx.send(());
        });

        let client = RealityClient::new(RealityClientConfig {
            server: addr,
            server_public_key: pub_bytes,
            short_id,
            sni: "www.example.com".to_string(),
            fingerprint: "chrome".to_string(),
        });
        let mut stream = client.dial().await.expect("client dial");
        stream.write_all(b"ping").await.unwrap();
        let mut reply = [0u8; 4];
        stream.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"pong");
        rx.await.unwrap();
    }

    /// sing-box seals with plaintext in `session_id` + `hello.Raw` AAD (not zeroed).
    #[tokio::test]
    async fn singbox_style_seal_auth_and_tls_roundtrip() {
        use aes_gcm::aead::{Aead, KeyInit, Payload};
        use aes_gcm::{Aes256Gcm, Key, Nonce};
        use blackwire_tls::ClientHelloBuilder;
        use hkdf::Hkdf;
        use sha2::Sha256;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        use tokio::sync::oneshot;
        use x25519_dalek::{PublicKey, StaticSecret};

        use super::super::complete_tls13_handshake;
        use crate::reality::{REALITY_HKDF_INFO, SESSION_ID_OFFSET_IN_HANDSHAKE_BODY};
        use crate::{RealityServer, RealityServerConfig, Tls13Stream};

        let server_secret = StaticSecret::random();
        let client_secret = StaticSecret::random();
        let client_pub = *PublicKey::from(&client_secret).as_bytes();

        let shared = server_secret
            .diffie_hellman(&PublicKey::from(client_pub))
            .as_bytes()
            .to_vec();
        let mut auth_key = [0u8; 32];
        auth_key.copy_from_slice(shared.as_slice());

        let mut random = [0u8; 32];
        rand::rng().fill(&mut random[..]);
        let hk = Hkdf::<Sha256>::new(Some(&random[..20]), &auth_key);
        hk.expand(REALITY_HKDF_INFO, &mut auth_key).unwrap();

        let short_id = vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
        let mut session_id = [0u8; 32];
        session_id[0] = 1;
        session_id[1] = 8;
        session_id[2] = 1;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        session_id[4..8].copy_from_slice(&ts.to_be_bytes());
        session_id[8..16].copy_from_slice(&short_id);

        let mut rng = rand::rng();
        let hello_bytes = ClientHelloBuilder::chrome_131().build_with_additional_key_share(
            "www.microsoft.com",
            &random,
            &[0u8; 32],
            Some(&client_pub),
            None,
            &mut rng,
        );
        let hs_body = &hello_bytes[5..];
        // Xray/sing-box: hello.Raw session_id is zero at Seal time; plaintext is SessionId only.
        let aad = hs_body.to_vec();
        let sid = SESSION_ID_OFFSET_IN_HANDSHAKE_BODY;

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&auth_key));
        let nonce = Nonce::from_slice(&random[20..32]);
        let ct = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: &session_id[..16],
                    aad: &aad,
                },
            )
            .unwrap();
        let mut wire_hello = hello_bytes;
        wire_hello[5 + sid..5 + sid + 32].copy_from_slice(&ct);

        let priv_bytes = *server_secret.as_bytes();
        let fallback = spawn_cover_sink().await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = Arc::new(RealityServer::new(RealityServerConfig {
            private_key: priv_bytes,
            short_ids: vec![short_id.clone()],
            server_names: vec!["www.microsoft.com".to_string()],
            fallback,
            max_time_diff: 120,
        }));

        let (tx, rx) = oneshot::channel();
        let srv = server.clone();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let accepted = srv.accept_with_key(Box::new(tcp)).await.unwrap();
            let cover_profile = accepted.cover_profile;
            let mut stream = accepted.stream;
            let keys = complete_tls13_server_handshake(
                &mut stream,
                &accepted.auth_key,
                "www.microsoft.com",
                cover_profile.as_ref(),
            )
            .await
            .unwrap();
            let mut tls = Tls13Stream::new_server(stream, keys);
            let mut buf = [0u8; 4];
            tls.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            tls.write_all(b"pong").await.unwrap();
            let _ = tx.send(());
        });

        let mut tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        tcp.write_all(&wire_hello).await.unwrap();
        let hs_body = &wire_hello[5..];
        let keys = complete_tls13_handshake(&mut tcp, hs_body, &client_secret, None, &auth_key)
            .await
            .expect("client TLS with sing-box-style auth");
        let mut tls = Tls13Stream::new(Box::new(tcp), keys);
        tls.write_all(b"ping").await.unwrap();
        let mut reply = [0u8; 4];
        tls.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"pong");
        rx.await.unwrap();
    }
}
