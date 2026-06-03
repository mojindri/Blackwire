use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

use crate::commands::{CloseSelector, ConnectionCommandResult};
use crate::meta::{
    CloseReason, ConnectionMeta, ConnectionSnapshot, Protocol, RelayPath, Transport,
};
use crate::metrics;

static GLOBAL_MANAGER: Lazy<ConnectionManager> = Lazy::new(ConnectionManager::default);

/// Returns a reference to the process-wide singleton connection manager.
pub fn global_manager() -> &'static ConnectionManager {
    &GLOBAL_MANAGER
}

/// Registry of all currently active managed connections.
pub struct ConnectionManager {
    next_id: AtomicU64,
    active: DashMap<u64, Arc<ConnectionMeta>>,
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            active: DashMap::new(),
        }
    }
}

impl ConnectionManager {
    /// Begin tracking a new connection, returning a guard that records its lifetime.
    pub fn track(
        &self,
        inbound: impl Into<Arc<str>>,
        outbound: impl Into<Arc<str>>,
        user: Option<Arc<str>>,
        protocol: Protocol,
        transport: Transport,
        relay_path: RelayPath,
    ) -> ConnectionGuard<'_> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let meta = Arc::new(ConnectionMeta {
            id,
            inbound: inbound.into(),
            outbound: outbound.into(),
            user,
            protocol,
            transport,
            started_at: Instant::now(),
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
            relay_path,
            close_reason: AtomicU8::new(CloseReason::Active.to_u8()),
            cancellation: CancellationToken::new(),
        });
        self.active.insert(id, Arc::clone(&meta));
        metrics::record_open();
        ConnectionGuard {
            manager: self,
            id,
            meta,
            finished: false,
        }
    }

    /// Returns a sorted snapshot of all currently tracked connections.
    pub fn list(&self) -> Vec<ConnectionSnapshot> {
        let mut snapshots: Vec<_> = self.active.iter().map(|entry| entry.snapshot()).collect();
        snapshots.sort_by_key(|snapshot| snapshot.id);
        snapshots
    }

    /// Returns the number of currently active connections.
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Returns `true` if there are no active connections.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Cancel all connections matching `selector` and return how many were matched.
    pub fn close(&self, selector: CloseSelector) -> ConnectionCommandResult {
        let reason = selector.reason();
        let mut matched = 0;
        for entry in self.active.iter() {
            let meta = entry.value();
            let is_match = match &selector {
                CloseSelector::Id(id) => meta.id == *id,
                CloseSelector::User(user) => meta.user.as_deref() == Some(user.as_str()),
                CloseSelector::Inbound(inbound) => meta.inbound.as_ref() == inbound.as_str(),
                CloseSelector::Outbound(outbound) => meta.outbound.as_ref() == outbound.as_str(),
            };
            if is_match {
                meta.set_close_reason(reason);
                meta.cancellation.cancel();
                matched += 1;
            }
        }
        ConnectionCommandResult { matched }
    }

    fn finish(&self, meta: &ConnectionMeta, up: u64, down: u64, fallback_reason: CloseReason) {
        let reason = match meta.close_reason() {
            CloseReason::Active => fallback_reason,
            existing => existing,
        };
        meta.bytes_up.store(up, Ordering::Relaxed);
        meta.bytes_down.store(down, Ordering::Relaxed);
        meta.set_close_reason(reason);
        self.active.remove(&meta.id);
        metrics::record_bytes(meta.protocol, meta.transport, up, down);
        metrics::record_close(reason, meta.started_at.elapsed());
    }
}

/// RAII guard for a tracked connection; finalises metrics on drop.
pub struct ConnectionGuard<'a> {
    manager: &'a ConnectionManager,
    id: u64,
    meta: Arc<ConnectionMeta>,
    finished: bool,
}

impl<'a> ConnectionGuard<'a> {
    /// Returns the unique identifier assigned to this connection.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns a clone of the cancellation token; cancelling it tears down the connection.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.meta.cancellation.clone()
    }

    /// Atomically accumulates bytes transferred in both directions.
    pub fn add_bytes(&self, up: u64, down: u64) {
        self.meta.bytes_up.fetch_add(up, Ordering::Relaxed);
        self.meta.bytes_down.fetch_add(down, Ordering::Relaxed);
    }

    /// Finalise the connection with cumulative byte counts and an explicit close reason.
    pub fn finish(mut self, up: u64, down: u64, reason: CloseReason) {
        self.finished = true;
        self.manager.finish(&self.meta, up, down, reason);
    }
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let up = self.meta.bytes_up.load(Ordering::Relaxed);
            let down = self.meta.bytes_down.load(Ordering::Relaxed);
            self.manager
                .finish(&self.meta, up, down, CloseReason::Dropped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> ConnectionManager {
        ConnectionManager::default()
    }

    #[test]
    fn track_connection() {
        let manager = manager();
        let guard = manager.track(
            "in",
            "out",
            None,
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );

        assert_eq!(manager.len(), 1);
        assert_eq!(manager.list()[0].id, guard.id());
    }

    #[test]
    fn remove_on_close() {
        let manager = manager();
        let guard = manager.track(
            "in",
            "out",
            None,
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );

        guard.finish(10, 20, CloseReason::Completed);

        assert!(manager.is_empty());
    }

    #[test]
    fn close_by_id() {
        let manager = manager();
        let guard = manager.track(
            "in",
            "out",
            None,
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );
        let token = guard.cancellation_token();

        let result = manager.close(CloseSelector::Id(guard.id()));

        assert_eq!(result.matched, 1);
        assert!(token.is_cancelled());
    }

    #[test]
    fn close_by_user() {
        let manager = manager();
        let user: Arc<str> = Arc::from("alice");
        let guard = manager.track(
            "in",
            "out",
            Some(user),
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );
        let token = guard.cancellation_token();

        let result = manager.close(CloseSelector::User("alice".into()));

        assert_eq!(result.matched, 1);
        assert!(token.is_cancelled());
    }

    #[test]
    fn close_all_by_inbound() {
        let manager = manager();
        let guard_a = manager.track(
            "in-a",
            "out",
            None,
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );
        let guard_b = manager.track(
            "in-a",
            "out",
            None,
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );
        let guard_c = manager.track(
            "in-b",
            "out",
            None,
            Protocol::Tcp,
            Transport::Tcp,
            RelayPath::Copy,
        );

        let result = manager.close(CloseSelector::Inbound("in-a".into()));

        assert_eq!(result.matched, 2);
        assert!(guard_a.cancellation_token().is_cancelled());
        assert!(guard_b.cancellation_token().is_cancelled());
        assert!(!guard_c.cancellation_token().is_cancelled());
    }

    #[test]
    fn no_leak_under_churn() {
        let manager = manager();
        for _ in 0..10_000 {
            let guard = manager.track(
                "in",
                "out",
                None,
                Protocol::Tcp,
                Transport::Tcp,
                RelayPath::Copy,
            );
            guard.finish(1, 1, CloseReason::Completed);
        }
        assert!(manager.is_empty());
    }

    #[test]
    fn tracks_10k_idle_connections() {
        let manager = manager();
        let guards: Vec<_> = (0..10_000)
            .map(|_| {
                manager.track(
                    "in",
                    "out",
                    None,
                    Protocol::Tcp,
                    Transport::Tcp,
                    RelayPath::Copy,
                )
            })
            .collect();

        assert_eq!(manager.len(), 10_000);
        drop(guards);
        assert!(manager.is_empty());
    }
}
