//! Whole REST snapshots fenced by a stream revision.
//!
//! A venue discards a snapshot whose read raced a private stream event. When
//! events keep arriving faster than a read completes, every snapshot is
//! discarded and the section never updates. `StaleGuard` admits a read after
//! `limit` consecutive discards; the caller must re-read at once so the
//! admitted, possibly stale, snapshot is corrected.

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
    use super::StaleGuard;

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
