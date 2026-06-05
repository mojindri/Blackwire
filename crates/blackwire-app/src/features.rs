//! Core trait definitions: InboundHandler, OutboundHandler, ConnectionHandler.
//!
//! These traits are the architectural backbone of the proxy. Every protocol
//! and transport implements one or more of these traits.
//!
//! # The key rule: traits hide the implementation
//!
//! All code that uses these traits works with `Arc<dyn Trait>`, not with
//! concrete types. This means:
//!   - A SOCKS5 inbound does not know if the outbound is VLESS or freedom.
//!   - A VLESS protocol handler does not know if the transport is TCP or WebSocket.
//!   - The router does not know anything about the actual network connections.
//!
//! This separation makes it safe to add new protocols and transports without
//! touching the existing ones.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use blackwire_common::{Address, BoxedStream, ProxyError};

use crate::context::Context;
use crate::dispatcher::Dispatcher;

/// Result of opening an outbound stream.
pub struct OutboundConnectResult {
    /// Stream ready for normal bidirectional relay.
    pub stream: BoxedStream,
    /// Whether the outbound wrote the caller-provided early payload itself.
    pub wrote_early_payload: bool,
    /// Optional bytes produced during connect that must be delivered to the inbound before relay.
    pub returned_early_response: Option<Vec<u8>>,
}

impl OutboundConnectResult {
    /// Constructs a result wrapping a plain stream with no early-payload handling.
    pub fn stream(stream: BoxedStream) -> Self {
        Self {
            stream,
            wrote_early_payload: false,
            returned_early_response: None,
        }
    }
}

/// An inbound handler: listens for incoming connections and processes them.
///
/// Implement this trait to add a new inbound protocol (e.g. SOCKS5, HTTP CONNECT,
/// VLESS, VMess). The handler receives a raw byte stream from the transport
/// layer and is responsible for:
///   1. Reading and validating the protocol header (authentication).
///   2. Extracting the destination address the client wants to reach.
///   3. Passing the connection to the dispatcher with the destination address.
///
/// # Fallback on failure
///
/// If authentication fails, the handler MUST NOT close the connection.
/// Instead, it should forward all received bytes (including the auth header)
/// to the configured fallback backend. This makes the server indistinguishable
/// from a real HTTPS server to probers and censors.
#[async_trait]
pub trait InboundHandler: Send + Sync + 'static {
    /// The unique tag for this inbound, as configured in config.json.
    /// Used in routing rules and log messages.
    fn tag(&self) -> &str;

    /// Which network types this inbound supports.
    /// Most inbounds support TCP only. Hysteria2 supports both TCP and UDP.
    fn networks(&self) -> &[blackwire_common::Network];

    /// Handle a new incoming connection.
    ///
    /// # Arguments
    /// * `stream` — the raw byte stream, already unwrapped from the transport layer
    /// * `source` — the client's IP address and port (for logging and routing)
    /// * `dispatcher` — used to forward the connection after the protocol header is decoded
    async fn handle(
        &self,
        stream: BoxedStream,
        source: SocketAddr,
        dispatcher: Arc<dyn Dispatcher>,
    ) -> Result<(), ProxyError>;
}

/// An outbound handler: connects to a remote server using a proxy protocol.
///
/// Implement this trait to add a new outbound protocol (e.g. VLESS, freedom,
/// Hysteria2). The handler receives the destination address from the dispatcher
/// and must:
///   1. Connect to the proxy server (or directly to the destination for freedom).
///   2. Perform any required protocol handshake (send UUID, etc.).
///   3. Return a `BoxedStream` that the dispatcher can use to relay data.
#[async_trait]
pub trait OutboundHandler: Send + Sync + 'static {
    /// The unique tag for this outbound, as configured in config.json.
    fn tag(&self) -> &str;

    /// Connect to `dest` and return a stream ready for bidirectional data relay.
    ///
    /// # Arguments
    /// * `ctx` — connection context (for logging and routing decisions)
    /// * `dest` — the destination the client wants to reach
    async fn connect(&self, ctx: &Context, dest: &Address) -> Result<BoxedStream, ProxyError>;

    /// Connect and, when supported, write the first already-buffered payload during handshake.
    ///
    /// The default preserves existing behavior: connect normally, then write the
    /// early payload before returning the stream to the relay loop.
    async fn connect_with_early_payload(
        &self,
        ctx: &Context,
        dest: &Address,
        early_payload: Option<&[u8]>,
    ) -> Result<OutboundConnectResult, ProxyError> {
        let mut stream = self.connect(ctx, dest).await?;
        let wrote_early_payload = if let Some(payload) = early_payload {
            if !payload.is_empty() {
                stream.write_all(payload).await?;
                true
            } else {
                false
            }
        } else {
            false
        };
        Ok(OutboundConnectResult {
            stream,
            wrote_early_payload,
            returned_early_response: None,
        })
    }
}

/// A low-level connection handler, used by transport layers.
///
/// While `InboundHandler` works at the protocol level (reads proxy headers),
/// `ConnectionHandler` works at the transport level (receives a raw stream
/// and decides what to do with it). Used by REALITY and ShadowTLS, which need
/// to intercept the connection before the proxy protocol layer sees it.
#[async_trait]
pub trait ConnectionHandler: Send + Sync + 'static {
    /// Handle a raw connection.
    ///
    /// # Arguments
    /// * `stream` — raw byte stream from the underlying TCP socket
    /// * `source` — the client's IP address and port
    async fn handle_connection(
        &self,
        stream: BoxedStream,
        source: SocketAddr,
    ) -> Result<(), ProxyError>;
}
