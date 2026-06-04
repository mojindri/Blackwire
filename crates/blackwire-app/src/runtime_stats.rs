//! In-process counters exposed through Xray `StatsService` gRPC.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use once_cell::sync::Lazy;

static STARTED_AT: Lazy<Instant> = Lazy::new(Instant::now);
static COUNTERS: Lazy<DashMap<String, Arc<AtomicI64>>> = Lazy::new(DashMap::new);

fn counter(name: &str) -> Arc<AtomicI64> {
    // Fast path: the counter already exists. `DashMap::get` borrows the key as
    // `&str`, so this avoids allocating an owned `String` on every increment —
    // the common case once a counter has been created. Only the very first
    // increment for a given name pays the `to_string()` insert cost.
    if let Some(existing) = COUNTERS.get(name) {
        return Arc::clone(&existing);
    }
    COUNTERS
        .entry(name.to_string())
        .or_insert_with(|| Arc::new(AtomicI64::new(0)))
        .clone()
}

/// Add `delta` to a named counter (creating it if needed).
pub fn increment(name: &str, delta: i64) {
    counter(name).fetch_add(delta, Ordering::Relaxed);
}

// ── Cached per-connection counter handles ───────────────────────────────────
//
// The connection-accept and connection-close paths run on every proxied
// connection. Resolving their counters by formatting a `"inbound>>>…"` key and
// doing a `DashMap` lookup on each call allocates several `String`s per
// connection — pure overhead on short-lived (churned) connections.
//
// Instead we resolve each inbound's counter handles once and cache the
// `Arc<AtomicI64>` directly. The atomics still live in `COUNTERS`, so the Xray
// `StatsService` query path is unchanged — these are just cached clones.

/// Global connection counter, resolved once.
static CONN_TOTAL: Lazy<Arc<AtomicI64>> = Lazy::new(|| counter("connections>>>total"));

/// Per-inbound counter handles, resolved once per inbound tag.
struct InboundHandles {
    connections_total: Arc<AtomicI64>,
    traffic_uplink: Arc<AtomicI64>,
    traffic_downlink: Arc<AtomicI64>,
    /// `protocol -> connections>>>total` counter, resolved lazily per protocol.
    protocol_conns: DashMap<Box<str>, Arc<AtomicI64>>,
}

static INBOUND_HANDLES: Lazy<DashMap<Box<str>, Arc<InboundHandles>>> = Lazy::new(DashMap::new);

fn inbound_handles(inbound: &str) -> Arc<InboundHandles> {
    if let Some(existing) = INBOUND_HANDLES.get(inbound) {
        return Arc::clone(&existing);
    }
    let handles = Arc::new(InboundHandles {
        connections_total: counter(&format!("inbound>>>{inbound}>>>connections>>>total")),
        traffic_uplink: counter(&format!("inbound>>>{inbound}>>>traffic>>>uplink")),
        traffic_downlink: counter(&format!("inbound>>>{inbound}>>>traffic>>>downlink")),
        protocol_conns: DashMap::new(),
    });
    INBOUND_HANDLES
        .entry(inbound.into())
        .or_insert_with(|| Arc::clone(&handles))
        .clone()
}

/// Read a counter; optionally reset it after read.
pub fn get(name: &str, reset: bool) -> Option<i64> {
    let counter = COUNTERS.get(name)?.clone();
    let value = if reset {
        counter.swap(0, Ordering::Relaxed)
    } else {
        counter.load(Ordering::Relaxed)
    };
    Some(value)
}

/// Query counters whose names contain the pattern (wildcards stripped).
pub fn query(pattern: &str, reset: bool) -> Vec<(String, i64)> {
    let needle = pattern.trim_matches('*');
    COUNTERS
        .iter()
        .filter_map(|entry| {
            if !needle.is_empty() && !entry.key().contains(needle) {
                return None;
            }
            let value = if reset {
                entry.value().swap(0, Ordering::Relaxed)
            } else {
                entry.value().load(Ordering::Relaxed)
            };
            Some((entry.key().clone(), value))
        })
        .collect()
}

/// Process uptime in seconds (for SysStats).
pub fn uptime_secs() -> u32 {
    STARTED_AT.elapsed().as_secs().min(u32::MAX as u64) as u32
}

/// Resident set size (RSS) in bytes. Returns 0 if unavailable.
///
/// Reads `VmRSS` from `/proc/self/status` on Linux; uses `getrusage(RUSAGE_SELF)`
/// on other Unix systems (ru_maxrss is in bytes on Linux, kilobytes on macOS/BSD).
pub fn rss_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    // Format: "VmRSS:\t 12345 kB"
                    if let Some(kb_str) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Number of live Tokio tasks (analogous to goroutine count). Returns 0 if
/// called outside a Tokio runtime context or if metrics are unavailable.
pub fn num_tasks() -> u64 {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.metrics().num_alive_tasks() as u64
    } else {
        0
    }
}

/// Number of OS threads in this process. Returns 0 if unavailable.
pub fn num_threads() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("Threads:") {
                    if let Some(n_str) = rest.split_whitespace().next() {
                        if let Ok(n) = n_str.parse::<u64>() {
                            return n;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Increment connection counters for an accepted inbound session.
pub fn record_connection_accepted(inbound: &str, protocol: &str) {
    CONN_TOTAL.fetch_add(1, Ordering::Relaxed);
    let handles = inbound_handles(inbound);
    handles.connections_total.fetch_add(1, Ordering::Relaxed);
    // Resolve the per-protocol counter once per (inbound, protocol) pair; the
    // formatted key + insert only happens the first time a protocol is seen.
    // Clone the handle out of the map guard before any insert so we never hold
    // a `DashMap` read guard across a write (which can deadlock).
    let proto_counter = match handles.protocol_conns.get(protocol) {
        Some(existing) => Arc::clone(&existing),
        None => {
            let c = counter(&format!(
                "inbound>>>{inbound}>>>protocol>>>{protocol}>>>connections>>>total"
            ));
            handles.protocol_conns.insert(protocol.into(), Arc::clone(&c));
            c
        }
    };
    proto_counter.fetch_add(1, Ordering::Relaxed);
}

/// Record relay byte counts on inbound and optional user counters.
pub fn record_relay_traffic(inbound: &str, user: Option<&str>, rx_bytes: u64, tx_bytes: u64) {
    let handles = inbound_handles(inbound);
    handles
        .traffic_uplink
        .fetch_add(rx_bytes.min(i64::MAX as u64) as i64, Ordering::Relaxed);
    handles
        .traffic_downlink
        .fetch_add(tx_bytes.min(i64::MAX as u64) as i64, Ordering::Relaxed);
    if let Some(user) = user {
        increment(
            &format!("user>>>{user}>>>traffic>>>uplink"),
            rx_bytes.min(i64::MAX as u64) as i64,
        );
        increment(
            &format!("user>>>{user}>>>traffic>>>downlink"),
            tx_bytes.min(i64::MAX as u64) as i64,
        );
    }
}

/// Record per-user uplink/downlink byte counters.
pub fn record_user_traffic(user: &str, rx_bytes: u64, tx_bytes: u64) {
    increment(
        &format!("user>>>{user}>>>traffic>>>uplink"),
        rx_bytes.min(i64::MAX as u64) as i64,
    );
    increment(
        &format!("user>>>{user}>>>traffic>>>downlink"),
        tx_bytes.min(i64::MAX as u64) as i64,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_share_storage_with_query() {
        // The cached-handle path must update the same atomics the StatsService
        // query() reads — i.e. counter names are unchanged by the caching.
        record_connection_accepted("bench-in", "tcp");
        record_relay_traffic("bench-in", None, 100, 200);
        let total = get("inbound>>>bench-in>>>connections>>>total", false).unwrap();
        assert!(total >= 1);
        let up = get("inbound>>>bench-in>>>traffic>>>uplink", false).unwrap();
        assert!(up >= 100);
        // The per-protocol counter is still created under the original name.
        let proto = get(
            "inbound>>>bench-in>>>protocol>>>tcp>>>connections>>>total",
            false,
        )
        .unwrap();
        assert!(proto >= 1);
        // query() substring match still finds the cached counters.
        let rows = query("inbound>>>bench-in>>>", false);
        assert!(rows.iter().any(|(k, _)| k.ends_with("connections>>>total")));
    }
}
