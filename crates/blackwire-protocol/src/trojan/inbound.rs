//! Trojan inbound handler — accepts Trojan connections from clients.
//!
//! The server-side flow:
//!
//! 1. Read the 56-byte auth token from the stream.
//! 2. Compare it (in constant time) against the expected token derived from
//!    each configured password.
//! 3. If valid: read the SOCKS5 address, then relay to the dispatcher.
//! 4. If invalid: reject the connection without sending a Trojan error response.
//!
//! # TLS requirement
//!
//! In production, Trojan must run over TLS — the stream passed to this handler
//! should already have been upgraded by `tls_accept`. In tests we use plain
//! TCP to avoid the overhead of a TLS round-trip.
//!
//! # Active-probe resistance
//!
//! If the auth token is wrong we do not send a Trojan error response. The current
//! handler returns `AuthFailed`, and the caller closes the connection.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use subtle::ConstantTimeEq;
use tracing::{debug, warn};

use blackwire_app::context::Context;
use blackwire_app::dispatcher::Dispatcher;
use blackwire_app::dns::DnsModule;
use blackwire_app::features::InboundHandler;
use blackwire_app::user_limits::UserConnectionLimiter;
use tokio::io::BufReader;

use blackwire_common::{
    with_handshake_timeout, Address, BoxedStream, Network, PrependedStream, ProxyError,
};

use super::codec::{compute_token, decode_request, CMD_CONNECT, CMD_UDP_ASSOCIATE, TOKEN_LEN};

/// True for Trojan UDP tunnel (Xray `commandUDP` or SOCKS associate via `0.0.0.0:0`).
fn is_trojan_udp_associate(request: &super::codec::TrojanRequest) -> bool {
    if request.command == CMD_UDP_ASSOCIATE {
        return true;
    }
    request.command == CMD_CONNECT && is_unspecified_associate_dest(&request.dest)
}

fn is_unspecified_associate_dest(dest: &Address) -> bool {
    match dest {
        Address::Ipv4(ip, port) => ip.is_unspecified() && *port == 0,
        Address::Ipv6(ip, port) => ip.is_unspecified() && *port == 0,
        _ => false,
    }
}
use super::udp::relay_trojan_udp;

/// Accepted Trojan credential with an optional stats label.
#[derive(Debug, Clone)]
pub struct TrojanUser {
    /// Plaintext Trojan password accepted by this inbound.
    pub password: String,
    /// Optional user label used for traffic accounting.
    pub label: Option<String>,
}

struct TrojanToken {
    token: [u8; TOKEN_LEN],
    user: Option<Arc<str>>,
}

/// Reloadable Trojan token store.
#[derive(Default)]
pub struct TrojanAuthStore {
    tokens: RwLock<Vec<TrojanToken>>,
}

impl TrojanAuthStore {
    /// Creates an empty shared token store.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Atomically replaces all accepted Trojan users.
    pub fn replace_users(&self, users: &[TrojanUser]) {
        let tokens = users
            .iter()
            .map(|user| {
                let hex = compute_token(&user.password);
                let mut arr = [0u8; TOKEN_LEN];
                arr.copy_from_slice(hex.as_bytes());
                TrojanToken {
                    token: arr,
                    user: user.label.as_deref().map(Arc::from),
                }
            })
            .collect();
        *self.tokens.write() = tokens;
    }

    /// Validates a Trojan token and returns its optional user label.
    pub fn validate_token(&self, token: &[u8; TOKEN_LEN]) -> Option<Option<Arc<str>>> {
        self.tokens
            .read()
            .iter()
            .find(|expected| expected.token.ct_eq(token).into())
            .map(|expected| expected.user.clone())
    }

    /// Returns the number of accepted Trojan users.
    pub fn len(&self) -> usize {
        self.tokens.read().len()
    }

    /// Returns true when no Trojan users are configured.
    pub fn is_empty(&self) -> bool {
        self.tokens.read().is_empty()
    }
}

/// A Trojan inbound handler.
pub struct TrojanInbound {
    /// The inbound tag from config.
    tag: Arc<str>,
    /// Shared auth token store refreshed in place on config reload.
    auth: Arc<TrojanAuthStore>,

    /// DNS module for UDP relay domain resolution.
    dns: Option<Arc<DnsModule>>,
    /// Optional limit for reading and authenticating the Trojan request header.
    handshake_timeout: Option<Duration>,
    /// Optional authenticated per-user connection limiter.
    user_limiter: Option<Arc<UserConnectionLimiter>>,
}

impl TrojanInbound {
    /// Create a new Trojan inbound handler.
    ///
    /// # Arguments
    /// * `tag`       — unique inbound tag from config
    /// * `passwords` — list of accepted Trojan passwords
    /// * `dns`       — optional DNS module for UDP relay resolution
    pub fn new(
        tag: impl Into<Arc<str>>,
        passwords: &[String],
        dns: Option<Arc<DnsModule>>,
    ) -> Arc<Self> {
        let users = passwords
            .iter()
            .map(|password| TrojanUser {
                password: password.clone(),
                label: None,
            })
            .collect::<Vec<_>>();
        Self::new_with_users(tag, &users, dns, None)
    }

    /// Create a new Trojan inbound handler with per-client stats labels.
    pub fn new_with_users(
        tag: impl Into<Arc<str>>,
        users: &[TrojanUser],
        dns: Option<Arc<DnsModule>>,
        user_limiter: Option<Arc<UserConnectionLimiter>>,
    ) -> Arc<Self> {
        let auth = TrojanAuthStore::new();
        auth.replace_users(users);
        Self::new_with_auth_store(tag, auth, dns, None, user_limiter)
    }

    /// Create a new Trojan inbound handler backed by a shared auth store.
    pub fn new_with_auth_store(
        tag: impl Into<Arc<str>>,
        auth: Arc<TrojanAuthStore>,
        dns: Option<Arc<DnsModule>>,
        handshake_timeout: Option<Duration>,
        user_limiter: Option<Arc<UserConnectionLimiter>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            tag: tag.into(),
            auth,
            dns,
            handshake_timeout,
            user_limiter,
        })
    }
}

#[async_trait]
impl InboundHandler for TrojanInbound {
    fn tag(&self) -> &str {
        &self.tag
    }

    fn networks(&self) -> &[Network] {
        &[Network::Tcp]
    }

    async fn handle(
        &self,
        mut stream: BoxedStream,
        source: SocketAddr,
        dispatcher: Arc<dyn Dispatcher>,
    ) -> Result<(), ProxyError> {
        let (request, user, leftover) = with_handshake_timeout(self.handshake_timeout, async {
            // Buffer the header reads to collapse ~5-6 small recvfrom syscalls into one.
            // The Trojan header (token 56B + CRLF + cmd + atyp + addr + CRLF) fits in 128 bytes.
            // Any payload read ahead is recovered via PrependedStream.
            let mut buf_reader = BufReader::with_capacity(128, &mut stream);
            let request = decode_request(&mut buf_reader).await.map_err(|e| {
                debug!(source = %source, error = %e, "Trojan header parse failed");
                e
            })?;
            let leftover = buf_reader.buffer().to_vec();

            // Validate the token in constant time.
            let user = match self.auth.validate_token(&request.token) {
                Some(user) => user,
                None => {
                    warn!(source = %source, "Trojan auth failed — dropping connection");
                    return Err(ProxyError::AuthFailed);
                }
            };

            Ok((request, user, leftover))
        })
        .await?;
        if !leftover.is_empty() {
            stream = Box::new(PrependedStream::new(stream, leftover));
        }

        debug!(
            source = %source,
            dest = %request.dest,
            "Trojan authenticated"
        );

        let _user_permit = if let Some(limiter) = &self.user_limiter {
            match user.as_ref() {
                Some(user) => match limiter.try_acquire(Some(user)) {
                    Some(permit) => Some(permit),
                    None => {
                        warn!(
                            source = %source,
                            inbound = %self.tag,
                            user = %user,
                            max = limiter.max_connections_per_user(),
                            "per-user connection limit reached; dropping Trojan connection"
                        );
                        return Ok(());
                    }
                },
                None => None,
            }
        } else {
            None
        };

        if is_trojan_udp_associate(&request) {
            return relay_trojan_udp(stream, self.dns.clone(), self.tag.clone(), user).await;
        }

        let ctx = match user {
            Some(user) => Context::new(self.tag.clone(), source).with_user(user),
            None => Context::new(self.tag.clone(), source),
        };
        dispatcher.dispatch(ctx, request.dest, stream).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validate that a correct token is accepted and a wrong one is rejected.
    #[test]
    fn token_validation() {
        let handler = TrojanInbound::new("test", &["correct-password".to_string()], None);

        let good = compute_token("correct-password");
        let mut good_arr = [0u8; TOKEN_LEN];
        good_arr.copy_from_slice(good.as_bytes());

        let bad = compute_token("wrong-password");
        let mut bad_arr = [0u8; TOKEN_LEN];
        bad_arr.copy_from_slice(bad.as_bytes());

        assert!(handler.auth.validate_token(&good_arr).is_some());
        assert!(handler.auth.validate_token(&bad_arr).is_none());
    }

    /// Multiple passwords: any valid one is accepted.
    #[test]
    fn multi_password_validation() {
        let handler = TrojanInbound::new("test", &["pass1".to_string(), "pass2".to_string()], None);

        for pw in &["pass1", "pass2"] {
            let token_str = compute_token(pw);
            let mut arr = [0u8; TOKEN_LEN];
            arr.copy_from_slice(token_str.as_bytes());
            assert!(
                handler.auth.validate_token(&arr).is_some(),
                "password '{pw}' should be valid"
            );
        }

        let bad_str = compute_token("pass3");
        let mut bad_arr = [0u8; TOKEN_LEN];
        bad_arr.copy_from_slice(bad_str.as_bytes());
        assert!(handler.auth.validate_token(&bad_arr).is_none());
    }

    #[test]
    fn token_validation_returns_user_label() {
        let handler = TrojanInbound::new_with_users(
            "test",
            &[TrojanUser {
                password: "correct-password".into(),
                label: Some("trojan@example.local".into()),
            }],
            None,
            None,
        );

        let good = compute_token("correct-password");
        let mut good_arr = [0u8; TOKEN_LEN];
        good_arr.copy_from_slice(good.as_bytes());

        assert_eq!(
            handler.auth.validate_token(&good_arr).flatten().as_deref(),
            Some("trojan@example.local")
        );
    }
}
