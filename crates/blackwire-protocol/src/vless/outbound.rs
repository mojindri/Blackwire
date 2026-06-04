//! VLESS outbound handler — connects to a VLESS server.
//!
//! This is the client-side half of the VLESS protocol. When the dispatcher
//! needs to forward a connection via VLESS, this handler:
//!
//!   1. Dials a TCP connection to the VLESS server.
//!   2. Sends the VLESS request header (UUID + destination address).
//!   3. Reads and validates the VLESS response header from the server.
//!   4. Returns the stream, now positioned at the start of proxied data,
//!      ready for bidirectional relay.
//!
//! Transport layering (TLS, REALITY, WebSocket, etc.) is handled in
//! `blackwire-core` — this handler dials plain TCP and writes the VLESS header.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tracing::debug;

use blackwire_app::context::Context;
use blackwire_app::features::{OutboundConnectResult, OutboundHandler};
use blackwire_common::{Address, BoxedStream, PrependedStream, ProxyError};

use super::codec::{encode_request, Command};
use super::vision::wrap_vision_stream;

/// Send a VLESS request header over an already-established stream.
///
/// Use this when the transport layer (e.g. REALITY or WebSocket) has already
/// set up the connection and you just need to run the VLESS handshake on top.
///
/// # Arguments
/// * `stream` — an already-connected stream (e.g. from `RealityClient::dial()`)
/// * `uuid` — the 16-byte user UUID
/// * `flow` — the VLESS flow string (empty for no special flow)
/// * `dest` — the destination the client wants to reach
///
/// # Returns
/// The same stream, positioned after the VLESS response header, ready for
/// bidirectional data relay.
pub async fn connect_vless_on_stream(
    mut stream: BoxedStream,
    uuid: &[u8; 16],
    flow: &str,
    command: Command,
    dest: &Address,
) -> Result<BoxedStream, ProxyError> {
    let header = encode_request(uuid, flow, command, dest)?;
    stream.write_all(&header).await?;
    // Flush explicitly so that WebSocket and other buffered transports send
    // the VLESS header immediately without waiting for more data.
    stream.flush().await?;

    // Read VLESS response header: VER(1) + ADDONS_LEN(1) + ADDONS(N).
    // Buffer the reads so 2-3 small calls become one recvfrom. Any payload
    // bytes over-read are recovered via PrependedStream.
    let mut buf_reader = BufReader::with_capacity(16, &mut stream);
    let ver = buf_reader.read_u8().await?;
    if ver != 0x00 {
        return Err(ProxyError::Protocol(format!(
            "VLESS server responded with unexpected version {ver:#x}"
        )));
    }
    let addons_len = buf_reader.read_u8().await? as usize;
    if addons_len > 0 {
        let mut addons = vec![0u8; addons_len];
        buf_reader.read_exact(&mut addons).await?;
    }
    let leftover = buf_reader.buffer().to_vec();
    drop(buf_reader);
    if !leftover.is_empty() {
        stream = Box::new(PrependedStream::new(stream, leftover));
    }
    if flow == "xtls-rprx-vision" {
        Ok(wrap_vision_stream(stream, *uuid))
    } else {
        Ok(stream)
    }
}

/// Send a VLESS request header and optional first payload over an established stream.
pub async fn connect_vless_on_stream_with_early_payload(
    mut stream: BoxedStream,
    uuid: &[u8; 16],
    flow: &str,
    command: Command,
    dest: &Address,
    early_payload: Option<Vec<u8>>,
) -> Result<OutboundConnectResult, ProxyError> {
    if flow == "xtls-rprx-vision" {
        let mut stream = connect_vless_on_stream(stream, uuid, flow, command, dest).await?;
        let wrote_early_payload = if let Some(payload) = early_payload.as_deref() {
            if !payload.is_empty() {
                stream.write_all(payload).await?;
                true
            } else {
                false
            }
        } else {
            false
        };
        return Ok(OutboundConnectResult {
            stream,
            wrote_early_payload,
            returned_early_response: None,
        });
    }

    let header = encode_request(uuid, flow, command, dest)?;
    stream.write_all(&header).await?;
    let wrote_early_payload = if let Some(payload) = early_payload.as_deref() {
        if !payload.is_empty() {
            stream.write_all(payload).await?;
            true
        } else {
            false
        }
    } else {
        false
    };
    stream.flush().await?;

    let mut buf_reader = BufReader::with_capacity(16, &mut stream);
    let ver = buf_reader.read_u8().await?;
    if ver != 0x00 {
        return Err(ProxyError::Protocol(format!(
            "VLESS server responded with unexpected version {ver:#x}"
        )));
    }
    let addons_len = buf_reader.read_u8().await? as usize;
    if addons_len > 0 {
        let mut addons = vec![0u8; addons_len];
        buf_reader.read_exact(&mut addons).await?;
    }
    let leftover = buf_reader.buffer().to_vec();
    drop(buf_reader);
    if !leftover.is_empty() {
        stream = Box::new(PrependedStream::new(stream, leftover));
    }

    Ok(OutboundConnectResult {
        stream,
        wrote_early_payload,
        returned_early_response: None,
    })
}

/// VLESS outbound configuration.
#[derive(Debug, Clone)]
pub struct VlessOutboundConfig {
    /// The VLESS server's address and port.
    pub server: SocketAddr,

    /// The 16-byte user UUID to send in the VLESS header.
    pub uuid: [u8; 16],

    /// The optional flow string (e.g. "xtls-rprx-vision").
    /// Leave empty for normal VLESS without XTLS.
    pub flow: String,
}

/// The VLESS outbound handler.
pub struct VlessOutbound {
    /// The unique tag for this outbound (from config.json).
    tag: String,

    /// Connection configuration.
    config: VlessOutboundConfig,
}

impl VlessOutbound {
    /// Create a new VLESS outbound handler.
    pub fn new(tag: impl Into<String>, config: VlessOutboundConfig) -> Arc<Self> {
        Arc::new(Self {
            tag: tag.into(),
            config,
        })
    }
}

#[async_trait]
impl OutboundHandler for VlessOutbound {
    fn tag(&self) -> &str {
        &self.tag
    }

    async fn connect(&self, _ctx: &Context, dest: &Address) -> Result<BoxedStream, ProxyError> {
        // Plain TCP dial; transport-wrapped outbounds use TransportVlessOutbound instead.
        let mut stream = TcpStream::connect(self.config.server).await?;
        stream.set_nodelay(true)?;

        debug!(
            server = %self.config.server,
            dest = %dest,
            "VLESS outbound connecting"
        );

        // Step 2: Send the VLESS request header.
        // This tells the server which user we are and where we want to connect.
        let header = encode_request(&self.config.uuid, &self.config.flow, Command::Tcp, dest)?;
        stream.write_all(&header).await?;
        stream.flush().await?;

        // Step 3: Read the VLESS response header: VER(1) + ADDONS_LEN(1) + ADDONS(N).
        // Buffer to collapse 2-3 recvfrom calls into one; recover leftover via PrependedStream.
        let mut buf_reader = BufReader::with_capacity(16, &mut stream);
        let ver = buf_reader.read_u8().await?;
        if ver != 0x00 {
            return Err(ProxyError::Protocol(format!(
                "VLESS server responded with unexpected version {ver:#x}"
            )));
        }
        let addons_len = buf_reader.read_u8().await? as usize;
        if addons_len > 0 {
            let mut addons = vec![0u8; addons_len];
            buf_reader.read_exact(&mut addons).await?;
        }
        let leftover = buf_reader.buffer().to_vec();
        drop(buf_reader);

        debug!(server = %self.config.server, dest = %dest, "VLESS handshake complete");

        let stream: BoxedStream = if leftover.is_empty() {
            Box::new(stream)
        } else {
            Box::new(PrependedStream::new(stream, leftover))
        };
        if self.config.flow == "xtls-rprx-vision" {
            Ok(wrap_vision_stream(stream, self.config.uuid))
        } else {
            Ok(stream)
        }
    }

    async fn connect_with_early_payload(
        &self,
        _ctx: &Context,
        dest: &Address,
        early_payload: Option<Vec<u8>>,
    ) -> Result<OutboundConnectResult, ProxyError> {
        let stream = TcpStream::connect(self.config.server).await?;
        stream.set_nodelay(true)?;

        debug!(
            server = %self.config.server,
            dest = %dest,
            "VLESS outbound connecting with early payload"
        );

        let stream: BoxedStream = Box::new(stream);
        connect_vless_on_stream_with_early_payload(
            stream,
            &self.config.uuid,
            &self.config.flow,
            Command::Tcp,
            dest,
            early_payload,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::{TcpListener, TcpStream};

    use crate::vless::codec as vless_codec;

    #[tokio::test]
    async fn connect_on_stream_with_early_payload_preserves_first_bytes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let uuid = [7u8; 16];
        let dest = Address::Domain("example.com".into(), 443);
        let early_payload = b"GET / HTTP/1.1\r\n\r\n".to_vec();

        let expected_dest = dest.clone();
        let expected_payload = early_payload.clone();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut stream: BoxedStream = Box::new(tcp);
            let req = vless_codec::decode_request(&mut stream).await.unwrap();
            assert_eq!(req.uuid, uuid);
            assert_eq!(req.command, Command::Tcp);
            assert_eq!(req.dest, expected_dest);

            let mut payload = vec![0u8; expected_payload.len()];
            stream.read_exact(&mut payload).await.unwrap();
            assert_eq!(payload, expected_payload);
            stream.write_all(&[0x00, 0x00]).await.unwrap();
        });

        let tcp = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let result = connect_vless_on_stream_with_early_payload(
            Box::new(tcp),
            &uuid,
            "",
            Command::Tcp,
            &dest,
            Some(early_payload),
        )
        .await
        .unwrap();

        assert!(result.wrote_early_payload);
        assert!(result.returned_early_response.is_none());
    }
}
