//! Bidirectional relay — copy bytes between client and upstream until one side closes.
//!
//! # How relay works
//!
//! After the dispatcher opens an outbound connection, it runs a relay loop:
//!
//!   client ←→ inbound stream ←→ outbound stream ←→ destination
//!
//! Both directions run concurrently until either side closes or errors.
//!
//! # Linux splice(2)
//!
//! On Linux, when **both** sides are raw `TcpStream`s, we try `splice(2)` first.
//! Splice moves data through kernel pipes — bytes never touch userspace buffers,
//! which saves CPU on large transfers. If either stream is wrapped (TLS, WebSocket,
//! REALITY, etc.) or splice fails, we fall back to `tokio::io::copy_bidirectional`.

use std::io;
#[cfg(target_os = "linux")]
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use blackwire_common::relay::{RelayFlushPolicy, RelayV2Options};
use blackwire_common::BoxedStream;
#[cfg(target_os = "linux")]
use blackwire_common::{BufferPool, LowerState};
#[cfg(target_os = "linux")]
use blackwire_config::schema::VisionDirectCopyPolicy;
#[cfg(target_os = "linux")]
use blackwire_config::schema::{FastExperimentalBackendPolicy, FastZerocopyPolicy};
use blackwire_config::schema::{
    FastLinuxConfig, FastRelayConfig, FastRelayEngine, FastRelayFlushPolicy, FastSplicePolicy,
    VisionConfig,
};
#[cfg(target_os = "linux")]
use bytes::BytesMut;
#[cfg(target_os = "linux")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Minimum bytes transferred before the adaptive splice policy kicks in (Linux).
#[cfg(target_os = "linux")]
pub const ADAPTIVE_SPLICE_MIN_BYTES: u64 = 256 * 1024;
/// Elapsed time after which a stream is considered long-lived and splice is preferred (Linux).
#[cfg(target_os = "linux")]
pub const ADAPTIVE_SPLICE_LONG_STREAM_AFTER: Duration = Duration::from_millis(30);
#[cfg(target_os = "linux")]
const ADAPTIVE_COPY_BUFFER_BYTES: usize = 64 * 1024;
#[cfg(target_os = "linux")]
const ADAPTIVE_SPLICE_FULL_READ_STREAK: u8 = 4;
#[cfg(target_os = "linux")]
const ADAPTIVE_SPLICE_FULL_READ_MIN_BYTES: u64 = 64 * 1024;

/// Minimum bytes transferred before the adaptive splice policy kicks in (non-Linux stub).
#[cfg(not(target_os = "linux"))]
pub const ADAPTIVE_SPLICE_MIN_BYTES: u64 = 0;
/// Elapsed time after which a stream is considered long-lived and splice is preferred (non-Linux stub).
#[cfg(not(target_os = "linux"))]
pub const ADAPTIVE_SPLICE_LONG_STREAM_AFTER: Duration = Duration::from_millis(0);

/// Relay bytes between two streams until either side closes.
///
/// Returns `(bytes_client_to_server, bytes_server_to_client)`.
#[allow(dead_code)]
pub async fn relay_bidirectional(
    inbound: BoxedStream,
    outbound: BoxedStream,
) -> io::Result<(u64, u64)> {
    relay_bidirectional_with_splice_policy(inbound, outbound, FastSplicePolicy::Always).await
}

/// Relay bytes with an explicit Fast Profile splice policy.
pub async fn relay_bidirectional_with_splice_policy(
    inbound: BoxedStream,
    outbound: BoxedStream,
    splice_policy: FastSplicePolicy,
) -> io::Result<(u64, u64)> {
    relay_bidirectional_with_policies(
        inbound,
        outbound,
        splice_policy,
        FastRelayConfig::default(),
        FastLinuxConfig::default(),
        VisionConfig::default(),
    )
    .await
}

/// Relay bytes with explicit Fast Profile splice and userspace relay policies.
pub async fn relay_bidirectional_with_policies(
    inbound: BoxedStream,
    outbound: BoxedStream,
    splice_policy: FastSplicePolicy,
    relay_policy: FastRelayConfig,
    linux_policy: FastLinuxConfig,
    vision_policy: VisionConfig,
) -> io::Result<(u64, u64)> {
    #[cfg(target_os = "linux")]
    {
        use blackwire_common::{
            try_into_tcp_stream_with_prefix, try_into_vision_stream, PrependedStream,
        };

        let (mut inbound, inbound_prefix) = match try_into_tcp_stream_with_prefix(inbound) {
            Ok(parts) => parts,
            Err(inbound) => {
                let inbound = match try_into_vision_stream(inbound) {
                    Ok(vision) => {
                        return relay_vision_inbound_with_splice_policy(
                            vision,
                            outbound,
                            splice_policy,
                            relay_policy,
                            linux_policy,
                            vision_policy,
                        )
                        .await;
                    }
                    Err(inbound) => inbound,
                };
                metrics::counter!(
                    "proxy_relay_splice_fallback_total",
                    "reason" => "inbound_wrapped"
                )
                .increment(1);
                return userspace_copy_bidirectional(inbound, outbound, relay_policy).await;
            }
        };

        let (mut outbound, outbound_prefix) = match try_into_tcp_stream_with_prefix(outbound) {
            Ok(parts) => parts,
            Err(outbound) => {
                metrics::counter!(
                    "proxy_relay_splice_fallback_total",
                    "reason" => "outbound_wrapped"
                )
                .increment(1);
                let inbound: BoxedStream = if inbound_prefix.is_empty() {
                    Box::new(inbound)
                } else {
                    Box::new(PrependedStream::new(inbound, inbound_prefix))
                };
                return userspace_copy_bidirectional(inbound, outbound, relay_policy).await;
            }
        };

        let prefix_up = inbound_prefix.len() as u64;
        let prefix_down = outbound_prefix.len() as u64;

        if !inbound_prefix.is_empty() {
            outbound.write_all(&inbound_prefix).await?;
        }
        if !outbound_prefix.is_empty() {
            inbound.write_all(&outbound_prefix).await?;
        }

        if splice_policy == FastSplicePolicy::Disabled {
            metrics::counter!(
                "proxy_relay_splice_fallback_total",
                "reason" => "policy_disabled"
            )
            .increment(1);
            if linux_policy.zerocopy != FastZerocopyPolicy::Disabled {
                return zerocopy_tcp_bidirectional(
                    inbound,
                    outbound,
                    prefix_up,
                    prefix_down,
                    linux_policy,
                )
                .await;
            }

            let (up, down) =
                userspace_copy_bidirectional(Box::new(inbound), Box::new(outbound), relay_policy)
                    .await?;
            record_relay_path_bytes(
                userspace_relay_path(relay_policy, "copy", "copy_v2"),
                up + prefix_up,
                down + prefix_down,
            );
            return Ok((up + prefix_up, down + prefix_down));
        }

        if splice_policy == FastSplicePolicy::Adaptive {
            return adaptive_copy_then_splice(
                inbound,
                outbound,
                prefix_up,
                prefix_down,
                linux_policy,
            )
            .await;
        }

        metrics::counter!("proxy_relay_splice_selected_total", "policy" => "always").increment(1);

        if let Ok((up, down)) = blackwire_common::splice::splice_bidirectional_with_backend(
            &mut inbound,
            &mut outbound,
            splice_backend_policy(linux_policy),
        )
        .await
        {
            record_relay_path_bytes("splice", up + prefix_up, down + prefix_down);
            return Ok((up + prefix_up, down + prefix_down));
        }
        // splice can fail on exotic socket types — fall back safely.
        metrics::counter!(
            "proxy_relay_splice_fallback_total",
            "reason" => "splice_error"
        )
        .increment(1);
        let (up, down) =
            userspace_copy_bidirectional(Box::new(inbound), Box::new(outbound), relay_policy)
                .await?;
        record_relay_path_bytes(
            userspace_relay_path(relay_policy, "copy", "copy_v2"),
            up + prefix_up,
            down + prefix_down,
        );
        Ok((up + prefix_up, down + prefix_down))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = splice_policy;
        let _ = linux_policy;
        let _ = vision_policy;
        userspace_copy_bidirectional(inbound, outbound, relay_policy).await
    }
}

#[cfg(target_os = "linux")]
async fn relay_vision_inbound_with_splice_policy(
    mut inbound: blackwire_common::VisionStream<BoxedStream>,
    outbound: BoxedStream,
    splice_policy: FastSplicePolicy,
    relay_policy: FastRelayConfig,
    linux_policy: FastLinuxConfig,
    vision_policy: VisionConfig,
) -> io::Result<(u64, u64)> {
    use blackwire_common::{try_into_tcp_stream_with_prefix, PrependedStream};

    record_vision_phase(inbound.lower_state());

    if vision_policy.direct_copy == VisionDirectCopyPolicy::Disabled {
        metrics::counter!(
            "blackwire_vision_lower_failed_total",
            "reason" => "policy_disabled"
        )
        .increment(1);
        record_vision_decision(
            userspace_relay_path(relay_policy, "vision_copy", "vision_copy_v2"),
            "balanced",
            false,
            false,
        );
        let (up, down) =
            userspace_copy_bidirectional(Box::new(inbound), outbound, relay_policy).await?;
        record_relay_path_bytes(
            userspace_relay_path(relay_policy, "vision_copy", "vision_copy_v2"),
            up,
            down,
        );
        return Ok((up, down));
    }

    let (mut outbound, outbound_prefix) = match try_into_tcp_stream_with_prefix(outbound) {
        Ok(parts) => parts,
        Err(outbound) => {
            metrics::counter!(
                "proxy_relay_splice_fallback_total",
                "reason" => "vision_outbound_wrapped"
            )
            .increment(1);
            metrics::counter!(
                "blackwire_vision_lower_failed_total",
                "reason" => "outbound_wrapped"
            )
            .increment(1);
            record_vision_decision(
                userspace_relay_path(relay_policy, "vision_copy", "vision_copy_v2"),
                "balanced",
                false,
                false,
            );
            return userspace_copy_bidirectional(Box::new(inbound), outbound, relay_policy).await;
        }
    };

    let mut up = 0u64;
    let mut down = outbound_prefix.len() as u64;
    let mut up_eof = false;
    let mut down_eof = false;
    let mut up_buf = PooledRelayBuffer::new(ADAPTIVE_COPY_BUFFER_BYTES);
    let mut down_buf = PooledRelayBuffer::new(ADAPTIVE_COPY_BUFFER_BYTES);
    let outbound_zerocopy = enable_zerocopy_for_policy(&outbound, linux_policy, "vision_up")?;
    let started_at = tokio::time::Instant::now();
    let mut up_full_read_streak = 0u8;
    let mut down_full_read_streak = 0u8;

    if !outbound_prefix.is_empty() {
        inbound.write_all(&outbound_prefix).await?;
    }

    loop {
        if inbound.is_direct_copy_ready()
            && inbound.inner_is_tcp_like()
            && !up_eof
            && !down_eof
            && vision_policy.allow_splice_after_direct
            && vision_splice_policy_ready(
                splice_policy,
                up,
                down,
                started_at.elapsed(),
                up_full_read_streak,
                down_full_read_streak,
            )
        {
            metrics::counter!("blackwire_vision_direct_copy_ready_total").increment(1);
            record_vision_decision("vision_splice", "bulk", true, false);
            metrics::counter!(
                "proxy_relay_splice_selected_total",
                "policy" => vision_splice_policy_label(splice_policy),
                "flow" => "vision"
            )
            .increment(1);

            let inbound_inner = inbound.into_inner();
            match try_into_tcp_stream_with_prefix(inbound_inner) {
                Ok((mut inbound_tcp, inbound_prefix)) => {
                    if !inbound_prefix.is_empty() {
                        outbound.write_all(&inbound_prefix).await?;
                        up += inbound_prefix.len() as u64;
                        metrics::counter!("blackwire_vision_cached_bytes_total")
                            .increment(inbound_prefix.len() as u64);
                    }
                    match blackwire_common::splice::splice_bidirectional_with_backend(
                        &mut inbound_tcp,
                        &mut outbound,
                        splice_backend_policy(linux_policy),
                    )
                    .await
                    {
                        Ok((more_up, more_down)) => {
                            metrics::counter!("blackwire_vision_direct_copy_active_total")
                                .increment(1);
                            metrics::counter!("blackwire_vision_splice_after_direct_total")
                                .increment(1);
                            up += more_up;
                            down += more_down;
                            let path = if splice_policy == FastSplicePolicy::Adaptive {
                                "vision_adaptive_splice"
                            } else {
                                "vision_splice"
                            };
                            record_relay_path_bytes(path, up, down);
                            return Ok((up, down));
                        }
                        Err(_) => {
                            metrics::counter!(
                                "proxy_relay_splice_fallback_total",
                                "reason" => "vision_splice_error"
                            )
                            .increment(1);
                            metrics::counter!(
                                "blackwire_vision_lower_failed_total",
                                "reason" => "splice_error"
                            )
                            .increment(1);
                            let inbound: BoxedStream = if inbound_prefix.is_empty() {
                                Box::new(inbound_tcp)
                            } else {
                                Box::new(PrependedStream::new(inbound_tcp, inbound_prefix))
                            };
                            let (more_up, more_down) = userspace_copy_bidirectional(
                                inbound,
                                Box::new(outbound),
                                relay_policy,
                            )
                            .await?;
                            record_relay_path_bytes(
                                userspace_relay_path(
                                    relay_policy,
                                    "vision_copy_after_splice_error",
                                    "vision_copy_v2_after_splice_error",
                                ),
                                up + more_up,
                                down + more_down,
                            );
                            return Ok((up + more_up, down + more_down));
                        }
                    }
                }
                Err(inbound_inner) => {
                    metrics::counter!(
                        "proxy_relay_splice_fallback_total",
                        "reason" => "vision_inner_wrapped"
                    )
                    .increment(1);
                    metrics::counter!(
                        "blackwire_vision_lower_failed_total",
                        "reason" => "inner_wrapped"
                    )
                    .increment(1);
                    let (more_up, more_down) = userspace_copy_bidirectional(
                        inbound_inner,
                        Box::new(outbound),
                        relay_policy,
                    )
                    .await?;
                    record_relay_path_bytes(
                        userspace_relay_path(
                            relay_policy,
                            "vision_copy_inner_wrapped",
                            "vision_copy_v2_inner_wrapped",
                        ),
                        up + more_up,
                        down + more_down,
                    );
                    return Ok((up + more_up, down + more_down));
                }
            }
        }

        if up_eof && down_eof {
            let reason = if splice_policy == FastSplicePolicy::Disabled {
                "vision_policy_disabled"
            } else if !vision_policy.allow_splice_after_direct {
                "vision_splice_disabled"
            } else {
                "vision_direct_not_ready"
            };
            metrics::counter!("proxy_relay_splice_fallback_total", "reason" => reason).increment(1);
            if vision_policy.direct_copy == VisionDirectCopyPolicy::Require {
                metrics::counter!(
                    "blackwire_vision_lower_failed_total",
                    "reason" => "required_not_ready"
                )
                .increment(1);
            }
            record_vision_decision("vision_copy", "balanced", false, false);
            record_relay_path_bytes("vision_copy", up, down);
            return Ok((up, down));
        }

        tokio::select! {
            biased;
            read = outbound.read(down_buf.as_mut_slice()), if !down_eof => {
                let n = read?;
                if n == 0 {
                    down_eof = true;
                    inbound.shutdown().await?;
                } else {
                    inbound.write_all(&down_buf.as_slice()[..n]).await?;
                    down += n as u64;
                    down_full_read_streak =
                        update_full_read_streak(down_full_read_streak, n, down_buf.len());
                }
            }
            read = inbound.read(up_buf.as_mut_slice()), if !up_eof => {
                let n = read?;
                if n == 0 {
                    up_eof = true;
                    outbound.shutdown().await?;
                } else {
                    let report = write_tcp_with_zerocopy(
                        &mut outbound,
                        &up_buf.as_slice()[..n],
                        outbound_zerocopy,
                        linux_policy,
                        "vision_up",
                    )
                    .await?;
                    up += report.bytes as u64;
                    up_full_read_streak =
                        update_full_read_streak(up_full_read_streak, n, up_buf.len());
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn vision_splice_policy_ready(
    splice_policy: FastSplicePolicy,
    up: u64,
    down: u64,
    elapsed: Duration,
    up_full_read_streak: u8,
    down_full_read_streak: u8,
) -> bool {
    match splice_policy {
        FastSplicePolicy::Disabled => false,
        FastSplicePolicy::Always => true,
        FastSplicePolicy::Adaptive => adaptive_splice_ready_for_directions(
            up,
            down,
            elapsed,
            up_full_read_streak,
            down_full_read_streak,
        ),
    }
}

#[cfg(target_os = "linux")]
fn vision_splice_policy_label(splice_policy: FastSplicePolicy) -> &'static str {
    match splice_policy {
        FastSplicePolicy::Disabled => "disabled",
        FastSplicePolicy::Always => "always",
        FastSplicePolicy::Adaptive => "adaptive",
    }
}

#[cfg(target_os = "linux")]
fn record_vision_phase(state: LowerState) {
    let phase = match state {
        LowerState::Never => "never",
        LowerState::NotYet => "not_yet",
        LowerState::AfterHandshake => "after_handshake",
        LowerState::Now => "now",
    };
    metrics::counter!("blackwire_vision_phase_total", "phase" => phase).increment(1);
}

#[cfg(target_os = "linux")]
async fn zerocopy_tcp_bidirectional(
    mut inbound: tokio::net::TcpStream,
    mut outbound: tokio::net::TcpStream,
    prefix_up: u64,
    prefix_down: u64,
    linux_policy: FastLinuxConfig,
) -> io::Result<(u64, u64)> {
    let inbound_zerocopy = enable_zerocopy_for_policy(&inbound, linux_policy, "down")?;
    let outbound_zerocopy = enable_zerocopy_for_policy(&outbound, linux_policy, "up")?;
    let mut up = 0u64;
    let mut down = 0u64;
    let mut up_eof = false;
    let mut down_eof = false;
    let mut up_buf = PooledRelayBuffer::new(ADAPTIVE_COPY_BUFFER_BYTES);
    let mut down_buf = PooledRelayBuffer::new(ADAPTIVE_COPY_BUFFER_BYTES);

    loop {
        if up_eof && down_eof {
            record_relay_path_bytes("zerocopy_copy", up + prefix_up, down + prefix_down);
            return Ok((up + prefix_up, down + prefix_down));
        }

        tokio::select! {
            biased;
            read = outbound.read(down_buf.as_mut_slice()), if !down_eof => {
                let n = read?;
                if n == 0 {
                    down_eof = true;
                    inbound.shutdown().await?;
                } else {
                    let report = write_tcp_with_zerocopy(
                        &mut inbound,
                        &down_buf.as_slice()[..n],
                        inbound_zerocopy,
                        linux_policy,
                        "down",
                    )
                    .await?;
                    down += report.bytes as u64;
                }
            }
            read = inbound.read(up_buf.as_mut_slice()), if !up_eof => {
                let n = read?;
                if n == 0 {
                    up_eof = true;
                    outbound.shutdown().await?;
                } else {
                    let report = write_tcp_with_zerocopy(
                        &mut outbound,
                        &up_buf.as_slice()[..n],
                        outbound_zerocopy,
                        linux_policy,
                        "up",
                    )
                    .await?;
                    up += report.bytes as u64;
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn enable_zerocopy_for_policy(
    stream: &tokio::net::TcpStream,
    linux_policy: FastLinuxConfig,
    direction: &'static str,
) -> io::Result<bool> {
    if linux_policy.zerocopy == FastZerocopyPolicy::Disabled {
        return Ok(false);
    }

    match blackwire_common::zerocopy::enable_tcp_zerocopy(stream) {
        Ok(true) => {
            metrics::counter!(
                "proxy_relay_zerocopy_socket_enabled_total",
                "direction" => direction
            )
            .increment(1);
            Ok(true)
        }
        Ok(false) => {
            metrics::counter!(
                "proxy_relay_zerocopy_fallback_total",
                "direction" => direction,
                "reason" => "unsupported"
            )
            .increment(1);
            Ok(false)
        }
        Err(err) => {
            metrics::counter!(
                "proxy_relay_zerocopy_fallback_total",
                "direction" => direction,
                "reason" => "setsockopt_error"
            )
            .increment(1);
            Err(err)
        }
    }
}

#[cfg(target_os = "linux")]
async fn write_tcp_with_zerocopy(
    stream: &mut tokio::net::TcpStream,
    buf: &[u8],
    zerocopy_enabled: bool,
    linux_policy: FastLinuxConfig,
    direction: &'static str,
) -> io::Result<blackwire_common::zerocopy::ZeroCopyWriteReport> {
    let min_bytes = match linux_policy.zerocopy {
        FastZerocopyPolicy::Disabled => usize::MAX,
        FastZerocopyPolicy::Bulk => {
            blackwire_common::zerocopy::normalized_min_bytes(linux_policy.zerocopy_min_bytes)
        }
        FastZerocopyPolicy::Always => 1,
    };
    let report = blackwire_common::zerocopy::write_all_maybe_zerocopy(
        stream,
        buf,
        zerocopy_enabled,
        min_bytes,
    )
    .await?;

    if report.used_zerocopy {
        metrics::counter!(
            "proxy_relay_zerocopy_bytes_total",
            "direction" => direction
        )
        .increment(report.bytes as u64);
    }
    if report.fallback_used {
        metrics::counter!(
            "proxy_relay_zerocopy_fallback_total",
            "direction" => direction,
            "reason" => "send_fallback"
        )
        .increment(1);
    }

    Ok(report)
}

#[cfg(target_os = "linux")]
fn splice_backend_policy(
    linux_policy: FastLinuxConfig,
) -> blackwire_common::splice::SpliceBackendPolicy {
    match linux_policy.io_uring {
        FastExperimentalBackendPolicy::Disabled => {
            blackwire_common::splice::SpliceBackendPolicy::EpollOnly
        }
        FastExperimentalBackendPolicy::Auto => blackwire_common::splice::SpliceBackendPolicy::Auto,
        FastExperimentalBackendPolicy::Require => {
            blackwire_common::splice::SpliceBackendPolicy::RequireIoUring
        }
    }
}

#[cfg(target_os = "linux")]
fn record_vision_decision(
    path: &'static str,
    profile: &'static str,
    splice_eligible: bool,
    zero_copy_eligible: bool,
) {
    metrics::counter!(
        "blackwire_relay_v2_selected_total",
        "path" => path,
        "profile" => profile,
        "splice_eligible" => splice_eligible.to_string(),
        "zero_copy_eligible" => zero_copy_eligible.to_string()
    )
    .increment(1);
}

#[cfg(target_os = "linux")]
async fn adaptive_copy_then_splice(
    mut inbound: tokio::net::TcpStream,
    mut outbound: tokio::net::TcpStream,
    prefix_up: u64,
    prefix_down: u64,
    linux_policy: FastLinuxConfig,
) -> io::Result<(u64, u64)> {
    let mut up = 0u64;
    let mut down = 0u64;
    let mut up_eof = false;
    let mut down_eof = false;
    let mut up_buf = PooledRelayBuffer::new(ADAPTIVE_COPY_BUFFER_BYTES);
    let mut down_buf = PooledRelayBuffer::new(ADAPTIVE_COPY_BUFFER_BYTES);
    let started_at = tokio::time::Instant::now();
    let mut up_full_read_streak = 0u8;
    let mut down_full_read_streak = 0u8;

    loop {
        if !up_eof
            && !down_eof
            && adaptive_splice_ready_for_directions(
                prefix_up + up,
                prefix_down + down,
                started_at.elapsed(),
                up_full_read_streak,
                down_full_read_streak,
            )
        {
            metrics::counter!("proxy_relay_splice_selected_total", "policy" => "adaptive")
                .increment(1);
            match blackwire_common::splice::splice_bidirectional_with_backend(
                &mut inbound,
                &mut outbound,
                splice_backend_policy(linux_policy),
            )
            .await
            {
                Ok((more_up, more_down)) => {
                    up += more_up;
                    down += more_down;
                    record_relay_path_bytes("adaptive_splice", up + prefix_up, down + prefix_down);
                    return Ok((up + prefix_up, down + prefix_down));
                }
                Err(_) => {
                    metrics::counter!(
                        "proxy_relay_splice_fallback_total",
                        "reason" => "adaptive_splice_error"
                    )
                    .increment(1);
                    // Continue on the copy path. The streams are still owned and
                    // usable here; splice failed before consuming user-space data.
                }
            }
        }

        if up_eof && down_eof {
            metrics::counter!(
                "proxy_relay_splice_fallback_total",
                "reason" => "adaptive_below_threshold"
            )
            .increment(1);
            record_relay_path_bytes("adaptive_copy", up + prefix_up, down + prefix_down);
            return Ok((up + prefix_up, down + prefix_down));
        }

        tokio::select! {
            biased;
            read = outbound.read(down_buf.as_mut_slice()), if !down_eof => {
                let n = read?;
                if n == 0 {
                    down_eof = true;
                    inbound.shutdown().await?;
                } else {
                    inbound.write_all(&down_buf.as_slice()[..n]).await?;
                    down += n as u64;
                    down_full_read_streak =
                        update_full_read_streak(down_full_read_streak, n, down_buf.len());
                }
            }
            read = inbound.read(up_buf.as_mut_slice()), if !up_eof => {
                let n = read?;
                if n == 0 {
                    up_eof = true;
                    outbound.shutdown().await?;
                } else {
                    outbound.write_all(&up_buf.as_slice()[..n]).await?;
                    up += n as u64;
                    up_full_read_streak =
                        update_full_read_streak(up_full_read_streak, n, up_buf.len());
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn adaptive_relay_pool() -> &'static BufferPool {
    static POOL: OnceLock<Arc<BufferPool>> = OnceLock::new();
    POOL.get_or_init(BufferPool::new).as_ref()
}

#[cfg(target_os = "linux")]
struct PooledRelayBuffer {
    pool: &'static BufferPool,
    buf: Option<BytesMut>,
}

#[cfg(target_os = "linux")]
impl PooledRelayBuffer {
    fn new(size: usize) -> Self {
        let pool = adaptive_relay_pool();
        let mut buf = pool.acquire(size);
        buf.resize(size, 0);
        Self {
            pool,
            buf: Some(buf),
        }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.buf
            .as_mut()
            .expect("pooled relay buffer must exist while borrowed")
            .as_mut()
    }

    fn as_slice(&self) -> &[u8] {
        self.buf
            .as_ref()
            .expect("pooled relay buffer must exist while borrowed")
            .as_ref()
    }

    fn len(&self) -> usize {
        self.buf
            .as_ref()
            .expect("pooled relay buffer must exist while borrowed")
            .len()
    }
}

#[cfg(target_os = "linux")]
impl Drop for PooledRelayBuffer {
    fn drop(&mut self) {
        if let Some(buf) = self.buf.take() {
            self.pool.release(buf);
        }
    }
}

#[cfg(target_os = "linux")]
fn adaptive_splice_ready(copied_total: u64, elapsed: Duration, full_read_streak: u8) -> bool {
    let bulk_reads = full_read_streak >= ADAPTIVE_SPLICE_FULL_READ_STREAK;
    bulk_reads
        && (copied_total >= ADAPTIVE_SPLICE_MIN_BYTES
            || (copied_total >= ADAPTIVE_SPLICE_FULL_READ_MIN_BYTES
                && elapsed >= ADAPTIVE_SPLICE_LONG_STREAM_AFTER))
}

#[cfg(target_os = "linux")]
fn adaptive_splice_ready_for_directions(
    up_copied: u64,
    down_copied: u64,
    elapsed: Duration,
    up_full_read_streak: u8,
    down_full_read_streak: u8,
) -> bool {
    adaptive_splice_ready(up_copied, elapsed, up_full_read_streak)
        || adaptive_splice_ready(down_copied, elapsed, down_full_read_streak)
}

#[cfg(target_os = "linux")]
fn update_full_read_streak(current: u8, read_len: usize, buf_len: usize) -> u8 {
    if read_len == buf_len {
        current.saturating_add(1)
    } else {
        0
    }
}

#[allow(dead_code)]
fn record_relay_path_bytes(path: &'static str, up: u64, down: u64) {
    metrics::counter!(
        "proxy_relay_bytes_total",
        "direction" => "up",
        "path" => path
    )
    .increment(up);
    metrics::counter!(
        "proxy_relay_bytes_total",
        "direction" => "down",
        "path" => path
    )
    .increment(down);
}

async fn userspace_copy_bidirectional(
    inbound: BoxedStream,
    outbound: BoxedStream,
    relay_policy: FastRelayConfig,
) -> io::Result<(u64, u64)> {
    match relay_policy.engine {
        FastRelayEngine::Legacy => {
            blackwire_common::relay::copy_bidirectional_pooled(inbound, outbound).await
        }
        FastRelayEngine::V2 => {
            let stats = blackwire_common::relay::copy_bidirectional_v2(
                inbound,
                outbound,
                relay_v2_options(relay_policy),
            )
            .await?;
            metrics::counter!("proxy_relay_v2_flushes_total").increment(stats.flush_ops);
            metrics::counter!("proxy_relay_v2_buffer_grows_total")
                .increment(stats.buffer_grow_events);
            Ok(stats.byte_totals())
        }
    }
}

#[cfg(target_os = "linux")]
fn userspace_relay_path(
    relay_policy: FastRelayConfig,
    legacy_path: &'static str,
    v2_path: &'static str,
) -> &'static str {
    match relay_policy.engine {
        FastRelayEngine::Legacy => legacy_path,
        FastRelayEngine::V2 => v2_path,
    }
}

fn relay_v2_options(relay_policy: FastRelayConfig) -> RelayV2Options {
    RelayV2Options {
        initial_buffer: relay_policy.initial_buffer,
        max_buffer: relay_policy.max_buffer,
        flush_policy: match relay_policy.flush {
            FastRelayFlushPolicy::Immediate => RelayFlushPolicy::Immediate,
            FastRelayFlushPolicy::Deferred => RelayFlushPolicy::Deferred,
            FastRelayFlushPolicy::Adaptive => RelayFlushPolicy::Adaptive,
        },
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::*;
    #[cfg(target_os = "linux")]
    use blackwire_common::PrependedStream;
    #[cfg(target_os = "linux")]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[cfg(target_os = "linux")]
    use tokio::net::{TcpListener, TcpStream};

    #[cfg(target_os = "linux")]
    async fn tcp_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        (client, server)
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn relay_drains_prepended_prefix_before_raw_tcp_splice() {
        let (mut client_a, server_a) = tcp_pair().await;
        let (mut client_b, server_b) = tcp_pair().await;

        let relay = tokio::spawn(async move {
            relay_bidirectional(
                Box::new(PrependedStream::new(server_a, b"pre-a-".to_vec())),
                Box::new(PrependedStream::new(server_b, b"pre-b-".to_vec())),
            )
            .await
            .unwrap()
        });

        client_a.write_all(b"from-a").await.unwrap();
        client_b.write_all(b"from-b").await.unwrap();
        client_a.shutdown().await.unwrap();
        client_b.shutdown().await.unwrap();

        let mut got_a = Vec::new();
        let mut got_b = Vec::new();
        client_a.read_to_end(&mut got_a).await.unwrap();
        client_b.read_to_end(&mut got_b).await.unwrap();

        let (up, down) = relay.await.unwrap();

        assert_eq!(got_a, b"pre-b-from-b");
        assert_eq!(got_b, b"pre-a-from-a");
        assert_eq!(up, b"pre-a-from-a".len() as u64);
        assert_eq!(down, b"pre-b-from-b".len() as u64);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn adaptive_splice_waits_for_bulk_evidence() {
        assert!(!adaptive_splice_ready(
            64 * 1024,
            ADAPTIVE_SPLICE_LONG_STREAM_AFTER,
            ADAPTIVE_SPLICE_FULL_READ_STREAK - 1
        ));
        assert!(!adaptive_splice_ready(
            ADAPTIVE_SPLICE_MIN_BYTES - 1,
            Duration::ZERO,
            ADAPTIVE_SPLICE_FULL_READ_STREAK
        ));
        assert!(!adaptive_splice_ready(
            ADAPTIVE_SPLICE_MIN_BYTES,
            Duration::ZERO,
            ADAPTIVE_SPLICE_FULL_READ_STREAK - 1
        ));
        assert!(adaptive_splice_ready(
            ADAPTIVE_SPLICE_MIN_BYTES,
            Duration::ZERO,
            ADAPTIVE_SPLICE_FULL_READ_STREAK
        ));
        assert!(adaptive_splice_ready(
            64 * 1024,
            ADAPTIVE_SPLICE_LONG_STREAM_AFTER,
            ADAPTIVE_SPLICE_FULL_READ_STREAK
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn adaptive_splice_full_read_streak_resets_on_short_read() {
        let streak = update_full_read_streak(0, 16 * 1024, 16 * 1024);
        let streak = update_full_read_streak(streak, 16 * 1024, 16 * 1024);
        assert_eq!(streak, 2);
        assert_eq!(update_full_read_streak(streak, 1024, 16 * 1024), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn adaptive_splice_requires_one_direction_to_look_bulk() {
        let elapsed = ADAPTIVE_SPLICE_LONG_STREAM_AFTER;
        assert!(!adaptive_splice_ready_for_directions(
            ADAPTIVE_SPLICE_FULL_READ_MIN_BYTES - 1,
            ADAPTIVE_SPLICE_FULL_READ_MIN_BYTES - 1,
            elapsed,
            ADAPTIVE_SPLICE_FULL_READ_STREAK,
            ADAPTIVE_SPLICE_FULL_READ_STREAK,
        ));

        assert!(adaptive_splice_ready_for_directions(
            ADAPTIVE_SPLICE_FULL_READ_MIN_BYTES,
            ADAPTIVE_SPLICE_FULL_READ_MIN_BYTES - 1,
            elapsed,
            ADAPTIVE_SPLICE_FULL_READ_STREAK,
            ADAPTIVE_SPLICE_FULL_READ_STREAK,
        ));
    }
}
