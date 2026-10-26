//! Whole REST snapshots fenced by a stream revision.
//!
//! A venue discards a snapshot whose read raced a private stream event. When
//! events keep arriving faster than a read completes, every snapshot is
//! discarded and the section never updates. `StaleGuard` admits a read after
//! `limit` consecutive discards; the caller must re-read at once so the
//! admitted, possibly stale, snapshot is corrected.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// One checked invalidation domain. Issuer identity is allocated only on use.
/// Unlike StaleGuard, exhaustion never permits progress through a stale fence.
#[derive(Debug)]
pub(crate) struct Epoch {
    revision: Option<u64>,
    issuer: Option<Arc<()>>,
}

impl Default for Epoch {
    fn default() -> Self {
        Self {
            revision: Some(0),
            issuer: None,
        }
    }
}

/// Keeping the allocation alive prevents issuer-address reuse after an Epoch dies.
#[derive(Clone, Debug)]
pub(crate) struct Stamp {
    issuer: Arc<()>,
    revision: u64,
}

impl PartialEq for Stamp {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.issuer, &other.issuer) && self.revision == other.revision
    }
}
impl Eq for Stamp {}
impl Hash for Stamp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.issuer).hash(state);
        self.revision.hash(state);
    }
}

impl Epoch {
    pub(crate) fn stamp(&mut self) -> Option<Stamp> {
        let revision = self.revision?;
        let issuer = self.issuer.get_or_insert_with(|| Arc::new(()));
        Some(Stamp {
            issuer: Arc::clone(issuer),
            revision,
        })
    }

    pub(crate) fn advance(&mut self) {
        self.revision = self.revision.and_then(|revision| revision.checked_add(1));
    }

    pub(crate) fn current(&self, stamp: &Stamp) -> bool {
        self.revision == Some(stamp.revision) && self.owns(stamp)
    }

    fn owns(&self, stamp: &Stamp) -> bool {
        self.issuer
            .as_ref()
            .is_some_and(|issuer| Arc::ptr_eq(issuer, &stamp.issuer))
    }

    /// Ordering within this live domain, also used by metadata observation policy.
    #[allow(dead_code)] // Metadata adoption follows the two financial references.
    pub(crate) fn compare(&self, left: &Stamp, right: &Stamp) -> Option<Ordering> {
        let current = self.revision?;
        (self.owns(left)
            && self.owns(right)
            && left.revision <= current
            && right.revision <= current)
            .then(|| left.revision.cmp(&right.revision))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn exhaust_for_test(&mut self) {
        self.revision = None;
    }
}

#[derive(Debug, Default)]
pub struct StaleGuard {
    discards: u8,
    committed: u64,
}

impl StaleGuard {
    /// Whether to commit a snapshot whose fence is `current`, for venues that
    /// keep one read in flight.
    pub fn admit(&mut self, current: bool, limit: u8) -> bool {
        self.admit_issued(current, u64::MAX, limit)
    }

    /// As `admit`, for overlapping reads numbered in issue order: a stale read
    /// older than one already committed is never admitted.
    pub fn admit_issued(&mut self, current: bool, issued: u64, limit: u8) -> bool {
        if current || (self.discards >= limit && issued > self.committed) {
            self.discards = 0;
            if issued != u64::MAX {
                self.committed = self.committed.max(issued);
            }
            true
        } else {
            self.discards = self.discards.saturating_add(1);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Epoch, StaleGuard};
    use std::cmp::Ordering;
    use std::collections::HashSet;

    #[test]
    fn issuer_identity_survives_moves_and_outlives_a_dropped_epoch() {
        let mut epoch = Epoch::default();
        assert!(epoch.issuer.is_none());
        epoch.advance();
        assert!(epoch.issuer.is_none());
        let old = epoch.stamp().unwrap();
        let mut moved = Box::new(epoch);
        assert!(moved.current(&old));
        let same = moved.stamp().unwrap();
        assert_eq!(old, same);
        moved.advance();
        let newer = moved.stamp().unwrap();
        assert!(!moved.current(&old));
        assert_eq!(moved.compare(&old, &newer), Some(Ordering::Less));
        let mut other = Epoch::default();
        other.advance();
        let foreign = other.stamp().unwrap();
        assert_ne!(foreign, old);
        assert!(!other.current(&old));
        assert_eq!(moved.compare(&old, &foreign), None);
        assert_eq!(HashSet::from([old.clone(), same, foreign]).len(), 2);
        drop(moved);
        let mut replacement = Epoch::default();
        replacement.advance();
        assert!(!replacement.current(&old));
        assert_ne!(replacement.stamp().unwrap(), old);
    }

    #[test]
    fn overflow_permanently_rejects_even_the_final_equal_stamp() {
        let mut epoch = Epoch {
            revision: Some(u64::MAX - 1),
            issuer: None,
        };
        let before = epoch.stamp().unwrap();
        epoch.advance();
        let last = epoch.stamp().unwrap();
        assert!(epoch.current(&last));
        assert_eq!(epoch.compare(&before, &last), Some(Ordering::Less));
        let mut future = last.clone();
        future.revision = u64::MAX;
        epoch.revision = Some(u64::MAX - 1);
        assert!(!epoch.current(&future));
        assert_eq!(epoch.compare(&before, &future), None);
        epoch.revision = Some(u64::MAX);
        epoch.advance();
        for _ in 0..3 {
            assert!(epoch.stamp().is_none());
            assert!(!epoch.current(&last));
            assert_eq!(epoch.compare(&last, &last), None);
            epoch.advance();
        }
    }

    #[test]
    fn admits_current_reads_and_every_read_after_the_limit() {
        let mut guard = StaleGuard::default();
        assert!(guard.admit(true, 2));
        assert!(!guard.admit(false, 2));
        assert!(!guard.admit(false, 2));
        assert!(guard.admit(false, 2));
        assert!(!guard.admit(false, 2));
        assert!(guard.admit(true, 2));
        assert!(!guard.admit(false, 2));
    }

    #[test]
    fn an_overlapping_read_older_than_a_commit_is_never_forced_in() {
        let mut guard = StaleGuard::default();
        assert!(!guard.admit_issued(false, 1, 2));
        assert!(!guard.admit_issued(false, 2, 2));
        assert!(guard.admit_issued(false, 4, 2));
        for _ in 0..4 {
            assert!(!guard.admit_issued(false, 3, 2));
        }
        assert!(guard.admit_issued(false, 5, 2));
    }
}
