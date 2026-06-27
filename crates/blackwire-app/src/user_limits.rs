//! Per-user connection admission limits.
//!
//! These limits are applied after authentication, so they protect shared
//! deployments from one user opening too many concurrent sessions without
//! affecting unauthenticated probes or the handshake path.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dashmap::DashMap;

#[derive(Debug)]
struct UserCounter {
    active: AtomicUsize,
}

impl UserCounter {
    fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
        }
    }
}

/// Shared admission limiter keyed by authenticated user label.
#[derive(Debug)]
pub struct UserConnectionLimiter {
    max_connections_per_user: AtomicUsize,
    counters: DashMap<Arc<str>, Arc<UserCounter>>,
}

impl UserConnectionLimiter {
    /// Create a per-user connection limiter with a fixed maximum.
    pub fn new(max_connections_per_user: usize) -> Self {
        Self {
            max_connections_per_user: AtomicUsize::new(max_connections_per_user.max(1)),
            counters: DashMap::new(),
        }
    }

    /// Return the configured per-user concurrent-connection cap.
    pub fn max_connections_per_user(&self) -> usize {
        self.max_connections_per_user.load(Ordering::Relaxed)
    }

    /// Update the configured per-user concurrent-connection cap in place.
    pub fn set_max_connections_per_user(&self, max_connections_per_user: usize) {
        self.max_connections_per_user
            .store(max_connections_per_user.max(1), Ordering::Relaxed);
    }

    /// Try to acquire one connection slot for the authenticated user.
    ///
    /// Returns `None` when no user is attached to the context or when the user
    /// has already reached the configured cap.
    pub fn try_acquire(self: &Arc<Self>, user: Option<&Arc<str>>) -> Option<UserConnectionPermit> {
        let user = user?.clone();
        let max = self.max_connections_per_user();
        if max == usize::MAX {
            return Some(UserConnectionPermit {
                limiter: None,
                user,
                counter: None,
            });
        }
        let counter = self
            .counters
            .entry(Arc::clone(&user))
            .or_insert_with(|| Arc::new(UserCounter::new()))
            .clone();
        let active = counter.active.fetch_add(1, Ordering::AcqRel) + 1;
        if active > max {
            counter.active.fetch_sub(1, Ordering::AcqRel);
            return None;
        }

        Some(UserConnectionPermit {
            limiter: Some(Arc::clone(self)),
            user,
            counter: Some(counter),
        })
    }
}

/// Owned permit that keeps one per-user connection slot occupied.
#[derive(Debug)]
pub struct UserConnectionPermit {
    limiter: Option<Arc<UserConnectionLimiter>>,
    user: Arc<str>,
    counter: Option<Arc<UserCounter>>,
}

impl Drop for UserConnectionPermit {
    fn drop(&mut self) {
        let Some(limiter) = &self.limiter else {
            return;
        };
        let Some(counter) = &self.counter else {
            return;
        };
        let previous = counter.active.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "user connection counter underflow");
        if previous <= 1 {
            limiter.counters.remove_if(&self.user, |_, current| {
                Arc::ptr_eq(current, counter) && current.active.load(Ordering::Acquire) == 0
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_acquire_without_user_is_noop() {
        let limiter = Arc::new(UserConnectionLimiter::new(1));
        assert!(limiter.try_acquire(None).is_none());
    }

    #[test]
    fn try_acquire_enforces_cap_per_user() {
        let limiter = Arc::new(UserConnectionLimiter::new(1));
        let user: Arc<str> = "alice@example.local".into();

        let first = limiter.try_acquire(Some(&user));
        assert!(first.is_some());
        assert!(limiter.try_acquire(Some(&user)).is_none());

        drop(first);
        assert!(limiter.try_acquire(Some(&user)).is_some());
    }

    #[test]
    fn users_are_limited_independently() {
        let limiter = Arc::new(UserConnectionLimiter::new(1));
        let alice: Arc<str> = "alice@example.local".into();
        let bob: Arc<str> = "bob@example.local".into();

        let _alice = limiter.try_acquire(Some(&alice)).unwrap();
        assert!(limiter.try_acquire(Some(&bob)).is_some());
    }

    #[test]
    fn cap_updates_apply_without_restart() {
        let limiter = Arc::new(UserConnectionLimiter::new(1));
        let user: Arc<str> = "alice@example.local".into();

        let first = limiter.try_acquire(Some(&user)).unwrap();
        assert!(limiter.try_acquire(Some(&user)).is_none());

        limiter.set_max_connections_per_user(2);
        let second = limiter.try_acquire(Some(&user));
        assert!(second.is_some());

        drop(first);
        drop(second);
    }
}
