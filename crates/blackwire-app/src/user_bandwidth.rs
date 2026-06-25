//! Shared per-user bandwidth shaping.
//!
//! Limits are keyed by the authenticated user label (`ctx.user`) and shared
//! across all of that user's concurrent connections. The shaping is intentionally
//! conservative: it paces writes only, which is enough to bound client upload
//! (writes to upstream) and download (writes back to the client) without
//! disturbing the read side of transports or protocol parsers.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
use std::time::Instant;

use arc_swap::ArcSwap;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{sleep, Duration};

use blackwire_common::BoxedStream;

/// Write direction relative to the authenticated user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserBandwidthDirection {
    /// Client -> upstream traffic.
    Upload,
    /// Upstream -> client traffic.
    Download,
}

/// Static bandwidth limits for a single user.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UserBandwidthLimit {
    /// Upload cap in bytes per second.
    pub upload_bps: Option<u64>,
    /// Download cap in bytes per second.
    pub download_bps: Option<u64>,
}

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    last: Instant,
}

#[derive(Debug)]
struct SharedRateBucket {
    rate_bps: u64,
    burst_bytes: u64,
    state: Mutex<BucketState>,
}

impl SharedRateBucket {
    fn new(rate_bps: u64) -> Arc<Self> {
        let burst_bytes = ((rate_bps as f64) * 0.025).ceil() as u64;
        let burst_bytes = burst_bytes.clamp(1024, 256 * 1024);
        Arc::new(Self {
            rate_bps: rate_bps.max(1),
            burst_bytes: burst_bytes.max(1),
            state: Mutex::new(BucketState {
                tokens: burst_bytes as f64,
                last: Instant::now(),
            }),
        })
    }

    async fn acquire(&self, requested: usize) -> usize {
        if requested == 0 {
            return 0;
        }

        loop {
            let wait = {
                let mut state = self.state.lock().expect("rate bucket lock poisoned");
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(state.last).as_secs_f64();
                state.last = now;
                state.tokens =
                    (state.tokens + elapsed * self.rate_bps as f64).min(self.burst_bytes as f64);
                if state.tokens >= 1.0 {
                    let allowed = requested.min(state.tokens.floor().max(1.0) as usize);
                    state.tokens -= allowed as f64;
                    return allowed;
                }
                let deficit = 1.0 - state.tokens;
                Duration::from_secs_f64((deficit / self.rate_bps as f64).max(0.000_001))
            };
            sleep(wait).await;
        }
    }

    fn refund(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let mut state = self.state.lock().expect("rate bucket lock poisoned");
        state.tokens = (state.tokens + bytes as f64).min(self.burst_bytes as f64);
    }
}

#[derive(Debug)]
struct UserBandwidthPolicy {
    upload: Option<Arc<SharedRateBucket>>,
    download: Option<Arc<SharedRateBucket>>,
}

impl UserBandwidthPolicy {
    fn new(limit: UserBandwidthLimit) -> Self {
        Self {
            upload: limit
                .upload_bps
                .filter(|v| *v > 0)
                .map(SharedRateBucket::new),
            download: limit
                .download_bps
                .filter(|v| *v > 0)
                .map(SharedRateBucket::new),
        }
    }

    fn bucket_for(&self, direction: UserBandwidthDirection) -> Option<Arc<SharedRateBucket>> {
        match direction {
            UserBandwidthDirection::Upload => self.upload.as_ref().map(Arc::clone),
            UserBandwidthDirection::Download => self.download.as_ref().map(Arc::clone),
        }
    }
}

fn registry() -> &'static ArcSwap<HashMap<Arc<str>, Arc<UserBandwidthPolicy>>> {
    static REGISTRY: OnceLock<ArcSwap<HashMap<Arc<str>, Arc<UserBandwidthPolicy>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| ArcSwap::from_pointee(HashMap::new()))
}

/// Replace the active per-user bandwidth policy table.
pub fn set_user_bandwidth_policies(policies: HashMap<Arc<str>, UserBandwidthLimit>) {
    let mapped = policies
        .into_iter()
        .map(|(user, limit)| (user, Arc::new(UserBandwidthPolicy::new(limit))))
        .collect::<HashMap<_, _>>();
    registry().store(Arc::new(mapped));
}

/// Update one user's bandwidth policy in place.
///
/// Passing `None` removes the policy entry, which means unlimited bandwidth for
/// future writes on that user label.
pub fn set_user_bandwidth_policy(user: Arc<str>, limit: Option<UserBandwidthLimit>) {
    let current = registry().load();
    let mut next = (**current).clone();
    match limit {
        Some(limit) => {
            next.insert(user, Arc::new(UserBandwidthPolicy::new(limit)));
        }
        None => {
            next.remove(user.as_ref());
        }
    }
    registry().store(Arc::new(next));
}

fn policy_for_user(user: Option<&str>) -> Option<Arc<UserBandwidthPolicy>> {
    let user = user?;
    registry().load().get(user).map(Arc::clone)
}

fn bucket_for_user(
    user: Option<&str>,
    direction: UserBandwidthDirection,
) -> Option<Arc<SharedRateBucket>> {
    policy_for_user(user).and_then(|policy| policy.bucket_for(direction))
}

type AllowFuture = Pin<Box<dyn Future<Output = usize> + Send>>;

/// A stream wrapper that applies per-user shaping to writes only.
pub struct UserWriteShapedStream<S> {
    inner: S,
    bucket: Option<Arc<SharedRateBucket>>,
    pending: Option<AllowFuture>,
    reserved: usize,
}

impl<S> UserWriteShapedStream<S> {
    fn new(inner: S, bucket: Option<Arc<SharedRateBucket>>) -> Self {
        Self {
            inner,
            bucket,
            pending: None,
            reserved: 0,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for UserWriteShapedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for UserWriteShapedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let Some(bucket) = self.bucket.as_ref().map(Arc::clone) else {
            return Pin::new(&mut self.inner).poll_write(cx, buf);
        };
        if buf.is_empty() {
            return Pin::new(&mut self.inner).poll_write(cx, buf);
        }

        if self.pending.is_none() {
            let requested = buf.len();
            self.pending = Some(Box::pin({
                let bucket = Arc::clone(&bucket);
                async move { bucket.acquire(requested).await }
            }));
        }

        if self.reserved == 0 {
            match self
                .pending
                .as_mut()
                .expect("allow future initialized")
                .as_mut()
                .poll(cx)
            {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(allowed) => {
                    self.pending = None;
                    self.reserved = allowed.max(1).min(buf.len());
                }
            }
        }

        let reserved = self.reserved.min(buf.len());
        match Pin::new(&mut self.inner).poll_write(cx, &buf[..reserved]) {
            Poll::Ready(Ok(written)) => {
                if reserved > written {
                    bucket.refund(reserved - written);
                }
                self.reserved = 0;
                Poll::Ready(Ok(written))
            }
            Poll::Ready(Err(e)) => {
                bucket.refund(reserved);
                self.reserved = 0;
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Wrap a boxed stream so writes are paced for the given user's traffic direction.
pub fn shape_stream_writes_for_user(
    stream: BoxedStream,
    user: Option<&Arc<str>>,
    direction: UserBandwidthDirection,
) -> BoxedStream {
    let bucket = bucket_for_user(user.map(|value| value.as_ref()), direction);
    if bucket.is_none() {
        return stream;
    }
    Box::new(UserWriteShapedStream::new(stream, bucket))
}

/// Wait until the given datagram or frame size is permitted for the user.
pub async fn wait_for_user_write_budget(
    user: Option<&str>,
    direction: UserBandwidthDirection,
    bytes: usize,
) {
    if let Some(bucket) = bucket_for_user(user, direction) {
        let _ = bucket.acquire(bytes).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn policy_table_replaces_contents() {
        let mut policies = HashMap::new();
        policies.insert(
            Arc::<str>::from("alice@example.local"),
            UserBandwidthLimit {
                upload_bps: Some(1024),
                download_bps: None,
            },
        );
        set_user_bandwidth_policies(policies);
        assert!(
            bucket_for_user(Some("alice@example.local"), UserBandwidthDirection::Upload).is_some()
        );
        assert!(bucket_for_user(Some("missing"), UserBandwidthDirection::Upload).is_none());
    }

    #[tokio::test]
    async fn shaped_stream_still_relays_bytes() {
        let mut policies = HashMap::new();
        policies.insert(
            Arc::<str>::from("alice@example.local"),
            UserBandwidthLimit {
                upload_bps: Some(1024 * 1024),
                download_bps: Some(1024 * 1024),
            },
        );
        set_user_bandwidth_policies(policies);

        let (client, mut server) = tokio::io::duplex(1024);
        let user: Arc<str> = "alice@example.local".into();
        let mut shaped = shape_stream_writes_for_user(
            Box::new(client),
            Some(&user),
            UserBandwidthDirection::Download,
        );

        shaped.write_all(b"ping").await.unwrap();
        shaped.flush().await.unwrap();

        let mut buf = [0u8; 4];
        server.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
    }
}
