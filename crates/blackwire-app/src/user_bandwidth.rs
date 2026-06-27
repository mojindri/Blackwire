//! Shared per-user bandwidth shaping API.
//!
//! Blackwire currently does not throttle bandwidth. The API remains as a stable
//! no-op surface for protocol paths that already call it, and for configs that
//! may still contain historical `upMbps` / `downMbps` fields.

use std::collections::HashMap;
use std::sync::Arc;

use blackwire_common::BoxedStream;

/// Write direction relative to the authenticated user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserBandwidthDirection {
    /// Client -> upstream traffic.
    Upload,
    /// Upstream -> client traffic.
    Download,
}

/// Historical per-user bandwidth limit shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserBandwidthLimit {
    /// Upload cap in bytes per second.
    pub upload_bps: Option<u64>,
    /// Download cap in bytes per second.
    pub download_bps: Option<u64>,
}

/// Replace the active per-user bandwidth policy table.
pub fn set_user_bandwidth_policies(_policies: HashMap<Arc<str>, UserBandwidthLimit>) {}

/// Update one user's bandwidth policy in place.
pub fn set_user_bandwidth_policy(_user: Arc<str>, _limit: Option<UserBandwidthLimit>) {}

/// Return the stream unchanged. Bandwidth shaping is disabled.
pub fn shape_stream_writes_for_user(
    stream: BoxedStream,
    _user: Option<&Arc<str>>,
    _direction: UserBandwidthDirection,
) -> BoxedStream {
    stream
}

/// Return immediately. Bandwidth shaping is disabled.
pub async fn wait_for_user_write_budget(
    _user: Option<&str>,
    _direction: UserBandwidthDirection,
    _bytes: usize,
) {
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn policy_setters_are_noops() {
        let mut policies = HashMap::new();
        policies.insert(
            Arc::<str>::from("alice@example.local"),
            UserBandwidthLimit {
                upload_bps: Some(1024),
                download_bps: None,
            },
        );
        set_user_bandwidth_policies(policies);
        set_user_bandwidth_policy(
            Arc::<str>::from("alice@example.local"),
            Some(UserBandwidthLimit {
                upload_bps: Some(2048),
                download_bps: Some(2048),
            }),
        );
    }

    #[tokio::test]
    async fn stream_passthrough_relays_bytes() {
        let (client, mut server) = tokio::io::duplex(1024);
        let user: Arc<str> = "alice@example.local".into();
        let mut stream = shape_stream_writes_for_user(
            Box::new(client),
            Some(&user),
            UserBandwidthDirection::Download,
        );

        stream.write_all(b"ping").await.unwrap();
        stream.flush().await.unwrap();

        let mut buf = [0u8; 4];
        server.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
    }
}
