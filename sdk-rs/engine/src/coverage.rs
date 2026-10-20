//! Which store rows a partially mapped private snapshot may remove.
//!
//! A venue skips a row it cannot map instead of failing the whole snapshot. A
//! skipped row with a known identity keeps the store's current copy of that id;
//! a skipped row with no identity means absence proves nothing, so no current
//! row in the snapshot's scope may be removed. A fully valid empty snapshot
//! still removes everything.
use std::collections::HashSet;
use std::hash::Hash;

/// Bound on remembered identities; past it the snapshot counts as unidentified.
const MAX_PROTECTED: usize = 4096;

pub struct SnapshotCoverage<K> {
    protected: HashSet<K>,
    unidentified: bool,
    rejected: u32,
}

impl<K> Default for SnapshotCoverage<K> {
    fn default() -> Self {
        Self {
            protected: HashSet::new(),
            unidentified: false,
            rejected: 0,
        }
    }
}

impl<K: Eq + Hash> SnapshotCoverage<K> {
    /// Records a row that could not be mapped; `key` is its identity when known.
    pub fn reject(&mut self, key: Option<K>) {
        self.rejected = self.rejected.saturating_add(1);
        match key {
            Some(key) if self.protected.len() < MAX_PROTECTED || self.protected.contains(&key) => {
                self.protected.insert(key);
            }
            _ => self.unidentified = true,
        }
    }

    /// Whether this snapshot's omission of `key` may remove the stored row.
    pub fn allows_absence(&self, key: &K) -> bool {
        !self.unidentified && !self.protected.contains(key)
    }

    pub fn rejected(&self) -> u32 {
        self.rejected
    }
}

#[cfg(test)]
mod tests {
    use super::SnapshotCoverage;

    #[test]
    fn identified_rejection_protects_only_its_identity() {
        let mut coverage = SnapshotCoverage::default();
        assert!(coverage.allows_absence(&"a"));
        coverage.reject(Some("a"));
        coverage.reject(Some("a"));
        assert!(!coverage.allows_absence(&"a"));
        assert!(coverage.allows_absence(&"b"));
        assert_eq!(coverage.rejected(), 2);
    }

    #[test]
    fn unidentified_rejection_protects_every_identity() {
        let mut coverage = SnapshotCoverage::default();
        coverage.reject(None);
        assert!(!coverage.allows_absence(&"a"));
        assert!(!coverage.allows_absence(&"b"));
        assert!(SnapshotCoverage::<&str>::default().allows_absence(&"a"));
    }

    #[test]
    fn overflow_turns_unidentified_and_never_forgets_a_protected_key() {
        let mut coverage = SnapshotCoverage::default();
        for id in 0..super::MAX_PROTECTED {
            coverage.reject(Some(id));
        }
        assert!(coverage.allows_absence(&usize::MAX));
        coverage.reject(Some(0));
        assert!(coverage.allows_absence(&usize::MAX));
        coverage.reject(Some(usize::MAX));
        assert!(!coverage.allows_absence(&(usize::MAX - 1)));
        assert!(!coverage.allows_absence(&0));
    }
}
