//! Relay helpers aligned with Xray policy defaults.

use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use std::future::poll_fn;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::pin;

use crate::{BufferPool, ProxyError};

/// Default idle timeout for established connections (Xray `ConnectionIdle`).
pub const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Milliseconds elapsed since the relay module was first used.
/// Provides a lightweight monotonic clock for idle-timeout tracking without
/// allocating a mutex or taking a lock on every packet.
fn now_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Shared buffer pool for the idle relay helper.
/// Reusing 16 KiB buffers avoids per-connection heap allocations.
fn relay_pool() -> &'static BufferPool {
    static POOL: OnceLock<Arc<BufferPool>> = OnceLock::new();
    POOL.get_or_init(BufferPool::new).as_ref()
}

/// Bidirectional relay using pooled 16 KiB buffers.
///
/// Equivalent to `tokio::io::copy_bidirectional` but reuses buffers from the
/// shared pool instead of allocating fresh per call. This matters when
/// connections are short-lived (benchmarks, many small requests).
///
/// Takes ownership of both streams (uses `tokio::io::split` internally).
/// Returns `(bytes_a_to_b, bytes_b_to_a)`.
pub async fn copy_bidirectional_pooled<A, B>(a: A, b: B) -> io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (a_rx, a_tx) = tokio::io::split(a);
    let (b_rx, b_tx) = tokio::io::split(b);
    let pool = relay_pool();
    let (r_up, r_down) = tokio::join!(
        copy_one_pooled(a_rx, b_tx, pool),
        copy_one_pooled(b_rx, a_tx, pool),
    );
    Ok((r_up?, r_down?))
}

/// Flush policy for [`copy_bidirectional_v2`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelayFlushPolicy {
    /// Flush after every successful write, matching the legacy relay behavior.
    #[default]
    Immediate,
    /// Flush on EOF/shutdown only. This lowers syscall pressure for bulk flows.
    Deferred,
    /// Coalesce flushes during an active burst, flushing only when the source
    /// pauses (the reader returns `Pending`) or reaches EOF.
    ///
    /// This keeps bulk throughput's low syscall pressure (one flush per burst
    /// instead of one per write) while preserving interactive latency on
    /// wrapped transports such as TLS/WebSocket, where a buffered partial record
    /// would otherwise sit unflushed until the next write under
    /// [`Deferred`](RelayFlushPolicy::Deferred).
    Adaptive,
}

/// Options for [`copy_bidirectional_v2`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayV2Options {
    /// Initial ring-buffer capacity in bytes.
    pub initial_buffer: usize,
    /// Maximum ring-buffer capacity in bytes; the buffer will not grow beyond this.
    pub max_buffer: usize,
    /// When to flush the write side of the relay.
    pub flush_policy: RelayFlushPolicy,
}

impl Default for RelayV2Options {
    fn default() -> Self {
        Self {
            initial_buffer: 16 * 1024,
            max_buffer: 256 * 1024,
            flush_policy: RelayFlushPolicy::Immediate,
        }
    }
}

/// Runtime counters returned by [`copy_bidirectional_v2`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelayV2Stats {
    /// Bytes relayed from stream A to stream B.
    pub bytes_a_to_b: u64,
    /// Bytes relayed from stream B to stream A.
    pub bytes_b_to_a: u64,
    /// Total number of read syscalls issued.
    pub read_ops: u64,
    /// Total number of write syscalls issued.
    pub write_ops: u64,
    /// Total number of flush calls issued.
    pub flush_ops: u64,
    /// Number of times the ring buffer grew to accommodate more data.
    pub buffer_grow_events: u64,
}

impl RelayV2Stats {
    /// Returns `(bytes_a_to_b, bytes_b_to_a)` as a tuple.
    pub fn byte_totals(&self) -> (u64, u64) {
        (self.bytes_a_to_b, self.bytes_b_to_a)
    }
}

/// Growable FIFO buffer used by the v2 relay.
///
/// Backed by a `BytesMut` plus a read cursor so the reader consumes from the
/// front in O(1) (`consume` just advances the cursor) and always sees a single
/// contiguous `front_slice` — unlike a `VecDeque<u8>` ring, which can wrap and
/// expose only a partial leading segment. Consumed front space is reclaimed by
/// compaction only when the tail runs out of room, keeping memmoves rare.
#[derive(Debug)]
pub struct RelayRingBuffer {
    buf: BytesMut,
    start: usize,
    max_capacity: usize,
}

impl RelayRingBuffer {
    /// Create a new buffer with the given initial and maximum capacities.
    pub fn new(initial_capacity: usize, max_capacity: usize) -> Self {
        let initial_capacity = initial_capacity.max(1);
        let max_capacity = max_capacity.max(initial_capacity);
        Self {
            buf: BytesMut::with_capacity(initial_capacity),
            start: 0,
            max_capacity,
        }
    }

    /// Number of bytes currently held in the buffer.
    pub fn len(&self) -> usize {
        self.buf.len() - self.start
    }

    /// Current allocated capacity of the buffer.
    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }

    /// Number of bytes that can be pushed before the buffer is full.
    pub fn remaining_capacity(&self) -> usize {
        self.capacity().saturating_sub(self.len())
    }

    /// True if the buffer holds no bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append as many bytes from `bytes` as fit; returns the number appended.
    pub fn push_slice(&mut self, bytes: &[u8]) -> usize {
        let n = bytes.len().min(self.remaining_capacity());
        if n == 0 {
            return 0;
        }
        // Reclaim consumed front space if the tail cannot hold the new bytes.
        if self.buf.capacity() - self.buf.len() < n {
            self.compact();
        }
        self.buf.extend_from_slice(&bytes[..n]);
        n
    }

    /// A contiguous view of the leading bytes in the buffer.
    pub fn front_slice(&self) -> &[u8] {
        &self.buf[self.start..]
    }

    /// Remove the first `n` bytes from the front of the buffer.
    pub fn consume(&mut self, n: usize) {
        self.start += n.min(self.len());
        if self.start == self.buf.len() {
            // Fully drained — reset to the front so capacity is fully usable.
            self.buf.clear();
            self.start = 0;
        }
    }

    /// Attempt to double the buffer capacity up to `max_capacity`. Returns `false` if already at max.
    pub fn grow(&mut self) -> bool {
        let capacity = self.capacity();
        if capacity >= self.max_capacity {
            return false;
        }
        let next = capacity.saturating_mul(2).min(self.max_capacity).max(1);
        self.buf.reserve(next.saturating_sub(self.buf.len()));
        true
    }

    /// Move live bytes to the front of the allocation, dropping consumed prefix.
    /// Uses `copy_within` (not `BytesMut::advance`) so `capacity()` stays stable
    /// and equal to the underlying allocation; only `grow` changes capacity.
    fn compact(&mut self) {
        if self.start == 0 {
            return;
        }
        let live = self.buf.len() - self.start;
        self.buf.copy_within(self.start.., 0);
        self.buf.truncate(live);
        self.start = 0;
    }
}

struct RelayDirectionState {
    pending: RelayRingBuffer,
    /// Read scratch, borrowed from the shared pool and returned on drop so each
    /// relay direction reuses a pooled allocation instead of a fresh `Vec`.
    scratch: BytesMut,
    read_eof: bool,
    shutdown_sent: bool,
    flush_pending: bool,
    bytes: u64,
    read_ops: u64,
    write_ops: u64,
    flush_ops: u64,
    grow_events: u64,
}

impl RelayDirectionState {
    fn new(options: RelayV2Options) -> Self {
        let initial = options.initial_buffer.max(1);
        let mut scratch = relay_pool().acquire(initial);
        scratch.resize(initial, 0);
        Self {
            pending: RelayRingBuffer::new(initial, options.max_buffer.max(initial)),
            scratch,
            read_eof: false,
            shutdown_sent: false,
            flush_pending: false,
            bytes: 0,
            read_ops: 0,
            write_ops: 0,
            flush_ops: 0,
            grow_events: 0,
        }
    }

    fn done(&self) -> bool {
        self.read_eof && self.pending.is_empty() && self.shutdown_sent
    }
}

impl Drop for RelayDirectionState {
    fn drop(&mut self) {
        relay_pool().release(std::mem::take(&mut self.scratch));
    }
}

/// One-task bidirectional relay with growable ring buffers and configurable flushing.
///
/// The implementation owns both split halves but drives both directions from a
/// single future. That keeps per-connection scheduling overhead lower than the
/// legacy two-copy-loop implementation while preserving full-duplex polling.
pub async fn copy_bidirectional_v2<A, B>(
    a: A,
    b: B,
    options: RelayV2Options,
) -> io::Result<RelayV2Stats>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (mut a_rx, mut a_tx) = tokio::io::split(a);
    let (mut b_rx, mut b_tx) = tokio::io::split(b);
    let mut up = RelayDirectionState::new(options);
    let mut down = RelayDirectionState::new(options);

    poll_fn(|cx| loop {
        let mut progressed = false;

        match poll_relay_direction(cx, &mut a_rx, &mut b_tx, &mut up, options.flush_policy) {
            Poll::Ready(Ok(moved)) => progressed |= moved,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }
        match poll_relay_direction(cx, &mut b_rx, &mut a_tx, &mut down, options.flush_policy) {
            Poll::Ready(Ok(moved)) => progressed |= moved,
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }

        if up.done() && down.done() {
            return Poll::Ready(Ok(RelayV2Stats {
                bytes_a_to_b: up.bytes,
                bytes_b_to_a: down.bytes,
                read_ops: up.read_ops + down.read_ops,
                write_ops: up.write_ops + down.write_ops,
                flush_ops: up.flush_ops + down.flush_ops,
                buffer_grow_events: up.grow_events + down.grow_events,
            }));
        }

        if !progressed {
            return Poll::Pending;
        }
    })
    .await
}

fn poll_relay_direction<R, W>(
    cx: &mut Context<'_>,
    reader: &mut R,
    writer: &mut W,
    state: &mut RelayDirectionState,
    flush_policy: RelayFlushPolicy,
) -> Poll<io::Result<bool>>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut progressed = false;

    if !state.pending.is_empty() {
        let front = state.pending.front_slice();
        match Pin::new(&mut *writer).poll_write(cx, front) {
            Poll::Ready(Ok(0)) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "relay v2 write returned zero",
                )));
            }
            Poll::Ready(Ok(n)) => {
                state.pending.consume(n);
                state.bytes += n as u64;
                state.write_ops += 1;
                progressed = true;
                // Immediate and Adaptive both mark unflushed data; they differ in
                // *when* the flush fires (see the flush block below).
                if matches!(
                    flush_policy,
                    RelayFlushPolicy::Immediate | RelayFlushPolicy::Adaptive
                ) {
                    state.flush_pending = true;
                }
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }
    }

    // Immediate flush: the write buffer just drained, so push it out now —
    // before the read below can refill the buffer. This preserves the
    // "flush after every write" contract.
    if flush_policy == RelayFlushPolicy::Immediate
        && state.flush_pending
        && state.pending.is_empty()
    {
        match Pin::new(&mut *writer).poll_flush(cx) {
            Poll::Ready(Ok(())) => {
                state.flush_pending = false;
                state.flush_ops += 1;
                progressed = true;
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }
    }

    // Read after the immediate flush so `Adaptive` can observe whether the source
    // still has data ready this round. `read_pending` is true when the reader had
    // nothing to hand us — i.e. the burst has drained and a flush is worthwhile.
    let mut read_pending = false;
    if !state.read_eof && state.pending.remaining_capacity() > 0 {
        let read_len = state.pending.remaining_capacity().min(state.scratch.len());
        let mut read_buf = ReadBuf::new(&mut state.scratch[..read_len]);
        match Pin::new(reader).poll_read(cx, &mut read_buf) {
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled().len();
                if filled == 0 {
                    state.read_eof = true;
                    progressed = true;
                } else {
                    let pushed = state.pending.push_slice(&read_buf.filled()[..filled]);
                    debug_assert_eq!(pushed, filled);
                    state.read_ops += 1;
                    progressed = true;
                    if filled == read_len
                        && state.pending.remaining_capacity() == 0
                        && state.pending.grow()
                    {
                        state.grow_events += 1;
                        let new_len = state.scratch.len().saturating_mul(2);
                        state
                            .scratch
                            .resize(new_len.min(state.pending.capacity()), 0);
                    }
                }
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => read_pending = true,
        }
    }

    // Adaptive flush: coalesce a run of back-to-back writes into a single flush,
    // firing only once the source has paused (reader Pending) or hit EOF.
    if flush_policy == RelayFlushPolicy::Adaptive
        && state.flush_pending
        && state.pending.is_empty()
        && (read_pending || state.read_eof)
    {
        match Pin::new(&mut *writer).poll_flush(cx) {
            Poll::Ready(Ok(())) => {
                state.flush_pending = false;
                state.flush_ops += 1;
                progressed = true;
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {}
        }
    }

    if state.read_eof && state.pending.is_empty() && !state.shutdown_sent {
        // Deferred deferred all flushes to here; Adaptive may still have a
        // pending flush if EOF arrived in the same round as the final write.
        if state.flush_pending || flush_policy == RelayFlushPolicy::Deferred {
            match Pin::new(&mut *writer).poll_flush(cx) {
                Poll::Ready(Ok(())) => {
                    state.flush_pending = false;
                    state.flush_ops += 1;
                    progressed = true;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Ready(Ok(progressed)),
            }
        }
        match Pin::new(writer).poll_shutdown(cx) {
            Poll::Ready(Ok(())) => {
                state.shutdown_sent = true;
                progressed = true;
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Ready(Ok(progressed)),
        }
    }

    Poll::Ready(Ok(progressed))
}

async fn copy_one_pooled<R, W>(mut reader: R, mut writer: W, pool: &BufferPool) -> io::Result<u64>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    const BUF_SIZE: usize = 16 * 1024;
    let mut buf = pool.acquire(BUF_SIZE);
    buf.resize(BUF_SIZE, 0);
    let mut total = 0u64;
    let mut io_err: Option<io::Error> = None;
    loop {
        let n = match reader.read(&mut buf[..]).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                io_err = Some(e);
                break;
            }
        };
        if let Err(e) = writer.write_all(&buf[..n]).await {
            io_err = Some(e);
            break;
        }
        if let Err(e) = writer.flush().await {
            io_err = Some(e);
            break;
        }
        total += n as u64;
    }
    // Always propagate EOF to the peer, even when the reader side hit an error
    // (e.g. ECONNRESET from a remote RST). Without this, the write half is left
    // open and the far end stalls waiting for data that will never arrive.
    let _ = writer.shutdown().await;
    pool.release(buf);
    match io_err {
        Some(e) => Err(e),
        None => Ok(total),
    }
}

/// Run an async handshake step with an optional wall-clock limit.
pub async fn with_handshake_timeout<T, F>(
    timeout: Option<Duration>,
    fut: F,
) -> Result<T, ProxyError>
where
    F: std::future::Future<Output = Result<T, ProxyError>>,
{
    match timeout {
        Some(limit) => match tokio::time::timeout(limit, fut).await {
            Ok(result) => result,
            Err(_) => Err(ProxyError::Timeout),
        },
        None => fut.await,
    }
}

/// Bidirectional relay that closes when neither direction moves data for `idle`.
pub async fn copy_bidirectional_with_idle<A, B>(a: &mut A, b: &mut B, idle: Duration)
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let (a_read, a_write) = tokio::io::split(a);
    let (b_read, b_write) = tokio::io::split(b);

    // AtomicU64 stores the last-activity timestamp (ms since module init).
    // Both relay halves update it lock-free; `sleep_until_idle` reads it.
    let last_activity = Arc::new(AtomicU64::new(now_ms()));

    let up = copy_one_way_with_idle(b_read, a_write, idle, Arc::clone(&last_activity));
    let down = copy_one_way_with_idle(a_read, b_write, idle, last_activity);

    let _ = tokio::join!(up, down);
}

async fn copy_one_way_with_idle<R, W>(
    mut reader: R,
    mut writer: W,
    idle: Duration,
    last_activity: Arc<AtomicU64>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    const BUF_SIZE: usize = 16 * 1024; // medium size class — matches BufferPool
    let pool = relay_pool();
    let mut buf = pool.acquire(BUF_SIZE);
    buf.resize(BUF_SIZE, 0); // make the full capacity addressable for reads

    loop {
        let read_fut = reader.read(&mut buf[..]);
        pin!(read_fut);

        let idle_fut = sleep_until_idle(&last_activity, idle);
        pin!(idle_fut);

        let n = tokio::select! {
            biased;
            res = &mut read_fut => match res {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            },
            _ = &mut idle_fut => break,
        };

        if writer.write_all(&buf[..n]).await.is_err() {
            break;
        }
        if writer.flush().await.is_err() {
            break;
        }
        last_activity.store(now_ms(), Ordering::Relaxed);
    }

    // Propagate EOF to the peer so it does not stall waiting for data that will
    // never arrive (idle timeout fired or reader errored on the other half).
    let _ = writer.shutdown().await;
    pool.release(buf);
}

/// Sleeps until the idle deadline (last_activity + idle) expires without renewal.
async fn sleep_until_idle(last_activity: &Arc<AtomicU64>, idle: Duration) {
    let idle_ms = idle.as_millis() as u64;
    loop {
        let last_ms = last_activity.load(Ordering::Relaxed);
        let deadline_ms = last_ms.saturating_add(idle_ms);
        let now = now_ms();
        if now >= deadline_ms {
            break;
        }
        tokio::time::sleep(Duration::from_millis(deadline_ms - now)).await;
        // If activity didn't change during sleep, the connection is idle.
        if last_activity.load(Ordering::Relaxed) == last_ms {
            break;
        }
        // Activity occurred during sleep — recompute and sleep again.
    }
}

/// Reject domain names longer than the SOCKS5 wire format allows (1-byte length field).
pub fn domain_wire_len(name: &str) -> Result<u8, ProxyError> {
    if name.len() > 255 {
        return Err(ProxyError::Protocol(format!(
            "domain too long: {} bytes",
            name.len()
        )));
    }
    Ok(name.len() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn domain_wire_len_matches_xray_limit() {
        assert!(domain_wire_len(&"a".repeat(255)).is_ok());
        assert!(domain_wire_len(&"a".repeat(256)).is_err());
    }

    #[tokio::test]
    async fn idle_copy_completes_on_eof() {
        let mut a = std::io::Cursor::new(Vec::<u8>::new());
        let mut b = std::io::Cursor::new(Vec::<u8>::new());
        copy_bidirectional_with_idle(&mut a, &mut b, Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn handshake_timeout_returns_error() {
        let slow = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok::<(), ProxyError>(())
        };
        let err = with_handshake_timeout(Some(Duration::from_millis(10)), slow)
            .await
            .unwrap_err();
        assert!(matches!(err, ProxyError::Timeout));
    }

    #[test]
    fn relay_ring_buffer_wraps_and_grows() {
        let mut ring = RelayRingBuffer::new(4, 8);
        assert_eq!(ring.push_slice(b"abcd"), 4);
        assert_eq!(ring.front_slice(), b"abcd");
        ring.consume(3);
        assert_eq!(ring.push_slice(b"efg"), 3);
        assert_eq!(ring.len(), 4);
        assert!(ring.grow());
        assert!(ring.capacity() >= 8);
    }

    #[tokio::test]
    async fn relay_v2_transfers_both_directions() {
        let (mut a_client, a_relay) = tokio::io::duplex(4096);
        let (mut b_client, b_relay) = tokio::io::duplex(4096);

        let relay = tokio::spawn(copy_bidirectional_v2(
            a_relay,
            b_relay,
            RelayV2Options {
                initial_buffer: 8,
                max_buffer: 64,
                flush_policy: RelayFlushPolicy::Deferred,
            },
        ));

        a_client.write_all(b"from-a").await.unwrap();
        b_client.write_all(b"from-b").await.unwrap();
        a_client.shutdown().await.unwrap();
        b_client.shutdown().await.unwrap();

        let mut got_a = Vec::new();
        let mut got_b = Vec::new();
        a_client.read_to_end(&mut got_a).await.unwrap();
        b_client.read_to_end(&mut got_b).await.unwrap();

        let stats = relay.await.unwrap().unwrap();
        assert_eq!(got_a, b"from-b");
        assert_eq!(got_b, b"from-a");
        assert_eq!(stats.byte_totals(), (6, 6));
        assert!(stats.read_ops >= 2);
        assert!(stats.write_ops >= 2);
    }

    #[tokio::test]
    async fn relay_v2_adaptive_coalesces_flushes_for_a_burst() {
        // A burst of back-to-back writes from one side should be covered by far
        // fewer flushes under Adaptive than under Immediate, while delivering the
        // exact same bytes. We drive many small writes before reading them out so
        // the relay sees a run of writes between source pauses.
        // A small ring (1 KiB) forces the 16 KiB payload across many
        // read→write cycles, so Immediate flushes once per cycle while Adaptive
        // coalesces them until the source pauses.
        const RING: usize = 1024;
        const TOTAL: usize = 16 * 1024;
        async fn run(policy: RelayFlushPolicy) -> RelayV2Stats {
            let (mut a_client, a_relay) = tokio::io::duplex(64 * 1024);
            let (mut b_client, b_relay) = tokio::io::duplex(64 * 1024);

            let relay = tokio::spawn(copy_bidirectional_v2(
                a_relay,
                b_relay,
                RelayV2Options {
                    initial_buffer: RING,
                    max_buffer: RING,
                    flush_policy: policy,
                },
            ));

            // Hand the whole payload to the relay up front, then let it drain.
            a_client.write_all(&[0xABu8; TOTAL]).await.unwrap();
            a_client.shutdown().await.unwrap();
            b_client.shutdown().await.unwrap();

            let mut got = Vec::new();
            b_client.read_to_end(&mut got).await.unwrap();
            assert_eq!(got.len(), TOTAL);
            relay.await.unwrap().unwrap()
        }

        let immediate = run(RelayFlushPolicy::Immediate).await;
        let adaptive = run(RelayFlushPolicy::Adaptive).await;

        // Same payload moved either way.
        assert_eq!(immediate.bytes_a_to_b, TOTAL as u64);
        assert_eq!(adaptive.bytes_a_to_b, TOTAL as u64);
        // Adaptive must not flush more than Immediate, and for a coalesced burst
        // it should flush strictly fewer times.
        assert!(
            adaptive.flush_ops <= immediate.flush_ops,
            "adaptive flushes ({}) should not exceed immediate ({})",
            adaptive.flush_ops,
            immediate.flush_ops
        );
        assert!(
            adaptive.flush_ops < immediate.flush_ops,
            "adaptive ({}) should coalesce below immediate ({})",
            adaptive.flush_ops,
            immediate.flush_ops
        );
    }

    #[tokio::test]
    async fn relay_v2_reports_buffer_growth() {
        let (mut a_client, a_relay) = tokio::io::duplex(4096);
        let (mut b_client, b_relay) = tokio::io::duplex(4096);

        let relay = tokio::spawn(copy_bidirectional_v2(
            a_relay,
            b_relay,
            RelayV2Options {
                initial_buffer: 8,
                max_buffer: 64,
                flush_policy: RelayFlushPolicy::Immediate,
            },
        ));

        a_client.write_all(&[7u8; 64]).await.unwrap();
        a_client.shutdown().await.unwrap();
        b_client.shutdown().await.unwrap();

        let mut got = Vec::new();
        b_client.read_to_end(&mut got).await.unwrap();
        let stats = relay.await.unwrap().unwrap();
        assert_eq!(got, vec![7u8; 64]);
        assert!(stats.buffer_grow_events > 0);
    }
}
