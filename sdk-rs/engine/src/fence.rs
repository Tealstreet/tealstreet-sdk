//! Whole REST snapshots fenced by a stream revision.
//!
//! A venue discards a snapshot whose read raced a private stream event. When
//! events keep arriving faster than a read completes, every snapshot is
//! discarded and the section never updates. `StaleGuard` admits the next read
//! after `limit` consecutive discards; the caller must re-read at once so the
//! admitted, possibly stale, snapshot is corrected.

#[derive(Debug, Default)]
pub struct StaleGuard {
    discards: u8,
}

impl StaleGuard {
    /// Whether to commit a snapshot whose fence is `current`.
    pub fn admit(&mut self, current: bool, limit: u8) -> bool {
        if current || self.discards >= limit {
            self.discards = 0;
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
}
