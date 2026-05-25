//! HTTPUpgrade transport — Xray `transport/internet/httpupgrade` (initial wiring).
//!
//! Full sing-box/Xray interop is gated in `labs/realistic`; this module provides
//! the client dial path used by outbound transport stacking.

use blackwire_common::{BoxedStream, ProxyError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use blackwire_config::schema::StreamSettingsConfig;

/// Dial TCP, send HTTP/1.1 Upgrade request, return stream after `101 Switching Protocols`.
pub async fn dial_httpupgrade(
    server: std::net::SocketAddr,
    dest_domain: &str,
    stream_settings: &StreamSettingsConfig,
) -> Result<BoxedStream, ProxyError> {
    let path = stream_settings
        .ws_settings
        .as_ref()
        .map(|w| w.path.clone())
        .unwrap_or_else(|| "/".to_string());
    let host = stream_settings
        .ws_settings
        .as_ref()
        .and_then(|w| {
            w.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("host"))
                .map(|(_, v)| v.clone())
        })
        .or_else(|| {
            stream_settings
                .tls_settings
                .as_ref()
                .map(|t| t.server_name.clone())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| dest_domain.to_string());

    let mut stream = TcpStream::connect(server).await?;
    stream.set_nodelay(true)?;

    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: Upgrade\r\n\
         Upgrade: websocket\r\n\
         \r\n"
    );
    stream.write_all(request.as_bytes()).await?;
    stream.flush().await?;

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let response = std::str::from_utf8(&buf[..n])
        .map_err(|_| ProxyError::Protocol("HTTPUpgrade response not UTF-8".into()))?;
    if !response.starts_with("HTTP/1.1 101") && !response.starts_with("HTTP/1.0 101") {
        return Err(ProxyError::Protocol(format!(
            "HTTPUpgrade expected 101, got: {}",
            response.lines().next().unwrap_or("")
        )));
    }

    Ok(Box::new(stream))
}
