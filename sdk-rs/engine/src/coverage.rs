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

/// Builds a private list snapshot row by row: accepted rows are pushed, rows
/// that cannot be mapped are rejected, and `commit` returns the accepted rows
/// plus the store's current copies that a rejection protects. Venues call it in
/// place of an all-or-nothing `invalid` flag; structural failures (envelope,
/// paging, duplicate identities) still discard the whole snapshot.
pub struct SnapshotBuilder<K, T> {
    rows: Vec<T>,
    coverage: SnapshotCoverage<K>,
}

impl<K, T> Default for SnapshotBuilder<K, T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            coverage: SnapshotCoverage::default(),
        }
    }
}

impl<K: Eq + Hash, T> SnapshotBuilder<K, T> {
    pub fn push(&mut self, row: T) {
        self.rows.push(row);
    }

    /// Records an unmappable row; `key` is its native identity when recoverable.
    pub fn reject(&mut self, key: Option<K>) {
        self.coverage.reject(key);
    }

    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    pub fn rejected(&self) -> u32 {
        self.coverage.rejected()
    }

    pub fn allows_absence(&self, key: &K) -> bool {
        self.coverage.allows_absence(key)
    }

    /// Accepted rows plus each `current` row whose identity was rejected (or every
    /// current row when a rejection had no identity) and that no accepted row replaces.
    pub fn commit(self, current: impl IntoIterator<Item = T>, key: impl Fn(&T) -> K) -> Vec<T> {
        let Self { mut rows, coverage } = self;
        let accepted: HashSet<K> = rows.iter().map(&key).collect();
        rows.extend(current.into_iter().filter(|row| {
            let id = key(row);
            !coverage.allows_absence(&id) && !accepted.contains(&id)
        }));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::{SnapshotBuilder, SnapshotCoverage};

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

    #[test]
    fn builder_keeps_protected_current_rows_that_no_accepted_row_replaces() {
        let mut builder = SnapshotBuilder::default();
        builder.push(("a", 2));
        builder.reject(Some("b"));
        builder.reject(Some("a"));
        let current = [("a", 1), ("b", 1), ("c", 1)];
        assert_eq!(builder.commit(current, |row| row.0), [("a", 2), ("b", 1)]);

        let mut unidentified = SnapshotBuilder::default();
        unidentified.push(("a", 2));
        unidentified.reject(None);
        assert_eq!(
            unidentified.commit(current, |row| row.0),
            [("a", 2), ("b", 1), ("c", 1)]
        );

        let clean: SnapshotBuilder<&str, (&str, i32)> = SnapshotBuilder::default();
        assert!(clean.commit(current, |row| row.0).is_empty());
    }
}
