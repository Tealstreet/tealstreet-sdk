//! Which store rows a partially mapped private snapshot may remove.
//!
//! A venue skips a row it cannot map instead of failing the whole snapshot. A
//! skipped row with a known identity keeps the store's current copy of that id;
//! a skipped row with no identity means absence proves nothing, so no current
//! row in the snapshot's scope may be removed. A fully valid empty snapshot
//! still removes everything.
use crate::log::code;
use crate::{Cx, Level};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Bound on remembered identities; past it the snapshot counts as unidentified.
const MAX_PROTECTED: usize = 4096;
/// Persistent diagnostic memory has its own limit, independent of coverage.
const MAX_REPORTS: usize = 4096;

#[derive(Clone)]
pub struct RowAdmission<K> {
    protected: HashSet<K>,
    unidentified: bool,
    overflowed: bool,
    rejected: u32,
}

/// Compatibility name for existing snapshot consumers during admission migration.
pub type SnapshotCoverage<K> = RowAdmission<K>;

impl<K> Default for RowAdmission<K> {
    fn default() -> Self {
        Self {
            protected: HashSet::new(),
            unidentified: false,
            overflowed: false,
            rejected: 0,
        }
    }
}

impl<K: Eq + Hash> RowAdmission<K> {
    /// Records coverage only, without reporting a second event when a rejection
    /// or a newer valid update is propagated into an in-flight read.
    pub fn reject(&mut self, key: Option<K>) {
        self.rejected = self.rejected.saturating_add(1);
        self.protect(key);
    }

    fn protect(&mut self, key: Option<K>) {
        match key {
            Some(key) if self.protected.len() < MAX_PROTECTED || self.protected.contains(&key) => {
                self.protected.insert(key);
            }
            Some(_) => {
                self.unidentified = true;
                self.overflowed = true;
            }
            None => self.unidentified = true,
        }
    }

    /// Report one rejected native row and protect its identity. Repeated copies
    /// of the coverage should use `extend`/`reject`, which do not report again.
    pub fn skip<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        key: Option<K>,
        level: Level,
        message: impl Into<String>,
    ) {
        self.reject(key);
        row_skipped(cx, level, message);
    }

    /// Admit a mapped REST or WS row; mapping/identity recovery stays with the
    /// venue. A rejected row does not become a default/zero row or a fill.
    pub fn admit<T, P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        row: Result<T, Option<K>>,
        level: Level,
        message: impl Into<String>,
    ) -> Option<T> {
        match row {
            Ok(row) => Some(row),
            Err(key) => {
                self.skip(cx, key, level, message);
                None
            }
        }
    }

    /// Merge another page/stream's coverage without emitting events or counting
    /// protected-key reinsertion as additional rejected rows.
    pub fn extend(&mut self, other: Self) {
        self.unidentified |= other.unidentified;
        self.overflowed |= other.overflowed;
        self.rejected = self.rejected.saturating_add(other.rejected);
        for key in other.protected {
            self.protect(Some(key));
        }
    }

    /// A missing identity blocks absence, while still allowing healthy peers.
    pub fn holds<Q: Eq + Hash + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.unidentified || self.protected.contains(key)
    }

    /// An exact rejected identity conflicts with a same-key update. After
    /// overflow no key can prove it was not rejected; unknown identity alone
    /// does not veto all healthy peers.
    pub fn conflicts<Q: Eq + Hash + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.overflowed || self.protected.contains(key)
    }

    pub fn any(&self) -> bool {
        self.unidentified || !self.protected.is_empty()
    }

    pub fn unidentified(&self) -> bool {
        self.unidentified
    }

    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Whether this snapshot's omission of `key` may remove the stored row.
    pub fn allows_absence(&self, key: &K) -> bool {
        !self.holds(key)
    }

    pub fn rejected(&self) -> u32 {
        self.rejected
    }
}

/// The shared diagnostic boundary also serves deferred/grouped row decisions
/// whose coverage has already been recorded. Call only at the original report
/// site, never while copying coverage or retaining a cached row.
pub fn row_skipped<P>(cx: &mut Cx<'_, P>, level: Level, message: impl Into<String>) {
    cx.notice(level, code::ROW_SKIPPED, message);
}

/// Duplicate-inclusive row count with an exact first-label distinction.
/// Anonymous and empty labels are different; a later row never replaces either.
#[derive(Default)]
pub struct RowCount {
    count: usize,
    first: Option<Option<String>>,
}

impl RowCount {
    pub fn add(&mut self, id: Option<&str>) {
        self.count += 1;
        self.first.get_or_insert_with(|| id.map(str::to_owned));
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn any(&self) -> bool {
        self.count > 0
    }

    pub fn first(&self) -> Option<Option<&str>> {
        self.first.as_ref().map(|id| id.as_deref())
    }

    pub fn summary(&self, format: impl FnOnce(&Self) -> String) -> Option<String> {
        self.first.as_ref().map(|_| format(self))
    }
}

/// First-seen unique known IDs, plus every anonymous or empty-ID row.
#[derive(Default)]
pub struct RowKeys {
    ids: Vec<String>,
    anonymous: usize,
}

impl RowKeys {
    pub fn add(&mut self, id: Option<&str>) {
        match id.filter(|id| !id.is_empty()) {
            Some(id) if !self.ids.iter().any(|known| known == id) => self.ids.push(id.into()),
            Some(_) => (),
            None => self.anonymous += 1,
        }
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    pub fn into_ids(self) -> Vec<String> {
        self.ids
    }

    pub fn anonymous(&self) -> usize {
        self.anonymous
    }

    pub fn count(&self) -> usize {
        self.ids.len() + self.anonymous
    }

    pub fn any(&self) -> bool {
        !self.ids.is_empty() || self.anonymous > 0
    }

    pub fn summary(&self, format: impl FnOnce(&Self) -> String) -> Option<String> {
        self.any().then(|| format(self))
    }
}

/// Report one native group, independently of its row count and host filtering.
pub fn row_summary<P>(cx: &mut Cx<'_, P>, level: Level, summary: Option<String>) -> bool {
    match summary {
        Some(message) => {
            row_skipped(cx, level, message);
            true
        }
        None => false,
    }
}

/// Repeat policy for persistent row diagnostics; independent of row admission.
#[derive(Clone, Copy)]
pub enum RowRepeat {
    /// Report an identity once until it is forgotten or the memory clears.
    Once,
    /// Also report the first Error after a previous non-Error report.
    ErrorUpgrade,
    /// Report whenever the exact message differs from the last report.
    ChangedMessage,
}

#[derive(Clone)]
enum ReportState {
    Seen,
    Error(bool),
    Message(String),
}

/// Persistent diagnostic memory shared by REST/WS row consumers. This does not
/// decide coverage, retain rows or make a snapshot eligible for publication.
#[derive(Clone)]
pub struct RowReports<K> {
    remembered: HashMap<K, ReportState>,
    repeat: RowRepeat,
    bounded: bool,
}

impl<K> Default for RowReports<K> {
    fn default() -> Self {
        Self::new(RowRepeat::Once)
    }
}

impl<K> RowReports<K> {
    /// Clear the whole memory before the next report at the shared 4096 limit,
    /// even if its key was already remembered. This is not an LRU cache.
    pub fn new(repeat: RowRepeat) -> Self {
        Self {
            remembered: HashMap::new(),
            repeat,
            bounded: true,
        }
    }

    /// Preserve native fixed-kind or otherwise explicitly unbounded memory.
    pub fn unbounded(repeat: RowRepeat) -> Self {
        Self {
            bounded: false,
            ..Self::new(repeat)
        }
    }

    pub fn clear(&mut self) {
        self.remembered.clear();
    }
}

impl<K: Eq + Hash> RowReports<K> {
    /// An empty group re-arms only this key; a present empty string is a report.
    pub fn report_summary<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        key: K,
        level: Level,
        summary: Option<String>,
    ) -> bool {
        match summary {
            Some(message) => self.report(cx, key, level, message),
            None => {
                self.forget(&key);
                false
            }
        }
    }

    /// Return whether repeat suppression admitted a call to `row_skipped`.
    /// Host filtering never changes memory, this result, or health accounting.
    pub fn report<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        key: K,
        level: Level,
        message: impl Into<String>,
    ) -> bool {
        if self.bounded && self.remembered.len() >= MAX_REPORTS {
            self.clear();
        }
        let previous = self.remembered.get(&key);
        match (self.repeat, previous) {
            (RowRepeat::Once, Some(_)) => return false,
            (RowRepeat::ErrorUpgrade, Some(ReportState::Error(errored)))
                if *errored || level != Level::Error =>
            {
                return false;
            }
            _ => (),
        }
        let message = message.into();
        if let (RowRepeat::ChangedMessage, Some(ReportState::Message(text))) =
            (self.repeat, previous)
            && text == &message
        {
            return false;
        }
        let state = match self.repeat {
            RowRepeat::Once => ReportState::Seen,
            RowRepeat::ErrorUpgrade => ReportState::Error(level == Level::Error),
            RowRepeat::ChangedMessage => ReportState::Message(message.clone()),
        };
        self.remembered.insert(key, state);
        row_skipped(cx, level, message);
        true
    }

    pub fn forget<Q: Eq + Hash + ?Sized>(&mut self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.remembered.remove(key).is_some()
    }
}

impl RowReports<String> {
    /// A clean snapshot rearms only its native namespace; otherwise only mapped
    /// identities rearm. The empty identity is a valid namespaced key.
    pub fn rearm_namespace<'a>(
        &mut self,
        prefix: &str,
        clean: bool,
        mapped: impl IntoIterator<Item = &'a str>,
    ) {
        if clean {
            self.remembered.retain(|key, _| !key.starts_with(prefix));
        } else {
            for key in mapped {
                self.forget(&format!("{prefix}{key}"));
            }
        }
    }
}

/// Builds a private list snapshot row by row: accepted rows are pushed, rows
/// that cannot be mapped are rejected, and `commit` returns the accepted rows
/// plus the store's current copies that a rejection protects. Venues call it in
/// place of an all-or-nothing `invalid` flag; structural failures (envelope,
/// paging, duplicate identities) still discard the whole snapshot.
#[derive(Clone)]
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

    pub fn skip<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        key: Option<K>,
        level: Level,
        message: impl Into<String>,
    ) {
        self.coverage.skip(cx, key, level, message);
    }

    /// Returns whether a mapped row was staged; rejected rows are reported once
    /// through the same admission path used by incremental WS consumers.
    pub fn admit<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        row: Result<T, Option<K>>,
        level: Level,
        message: impl Into<String>,
    ) -> bool {
        if let Some(row) = self.coverage.admit(cx, row, level, message) {
            self.rows.push(row);
            true
        } else {
            false
        }
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
    use super::{
        RowAdmission, RowCount, RowKeys, RowRepeat, RowReports, SnapshotBuilder, SnapshotCoverage,
        row_skipped, row_summary,
    };
    use crate::book::Book;
    use crate::{Action, Command, Cx, Level, Notice, Response, SocketId, TimerId, Venue};

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

    #[test]
    fn unknown_identity_holds_absence_without_conflicting_with_healthy_peers() {
        let mut rows = RowAdmission::<String>::default();
        rows.reject(Some("bad".into()));
        rows.reject(None);
        assert!(rows.holds("bad"));
        assert!(rows.conflicts("bad"));
        assert!(rows.holds("healthy"));
        assert!(!rows.conflicts("healthy"));
        assert!(rows.unidentified());
        assert!(!rows.overflowed());
        assert!(!rows.allows_absence(&"healthy".to_owned()));
    }

    #[test]
    fn merging_counts_rows_once_without_counting_protected_key_reinsertion() {
        let mut rest = RowAdmission::default();
        rest.reject(Some("rest"));
        let mut ws = RowAdmission::default();
        ws.reject(Some("ws"));
        ws.reject(Some("ws"));
        ws.reject(None);
        rest.extend(ws);
        assert_eq!(rest.rejected(), 4);
        assert_eq!(rest.protected.len(), 2);
        assert!(rest.conflicts("rest"));
        assert!(rest.conflicts("ws"));
        assert!(!rest.conflicts("healthy"));
        assert!(rest.holds("healthy"));
        assert!(rest.any());
        rest.extend(RowAdmission::default());
        assert_eq!(rest.rejected(), 4);
    }

    #[test]
    fn merge_overflow_is_bounded_and_survives_later_page_merges() {
        let mut first = RowAdmission::default();
        for key in 0..super::MAX_PROTECTED {
            first.reject(Some(key));
        }
        let mut next = RowAdmission::default();
        next.reject(Some(super::MAX_PROTECTED));
        next.reject(Some(super::MAX_PROTECTED));
        first.extend(next);
        assert_eq!(first.protected.len(), super::MAX_PROTECTED);
        assert_eq!(first.rejected(), super::MAX_PROTECTED as u32 + 2);
        assert!(first.overflowed());
        assert!(first.conflicts(&usize::MAX));
        assert!(first.holds(&usize::MAX));
        let mut final_page = RowAdmission::default();
        final_page.extend(first);
        assert!(final_page.overflowed());
        assert!(final_page.unidentified());
        assert!(final_page.conflicts(&usize::MAX));
        assert_eq!(final_page.rejected(), super::MAX_PROTECTED as u32 + 2);
        assert_eq!(final_page.protected.len(), super::MAX_PROTECTED);
    }

    #[test]
    fn rejection_count_saturates_including_during_merge() {
        let mut rows = RowAdmission::<()> {
            rejected: u32::MAX,
            ..RowAdmission::default()
        };
        rows.reject(None);
        let mut other = RowAdmission::default();
        other.reject(None);
        rows.extend(other);
        assert_eq!(rows.rejected(), u32::MAX);
    }

    struct AdmissionProbe(fn(&mut Cx<'_, ()>));
    impl Venue for AdmissionProbe {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            (self.0)(cx);
        }
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn admission_probe(cx: &mut Cx<'_, ()>) {
        let mut ws = RowAdmission::<&str>::default();
        assert_eq!(ws.admit(cx, Ok(7), Level::Warn, "unused"), Some(7));
        assert_eq!(
            ws.admit::<(), _>(cx, Err(Some("bad")), Level::Warn, "known row"),
            None
        );
        assert_eq!(
            ws.admit::<(), _>(cx, Err(None), Level::Error, "unknown row"),
            None
        );
        let mut rest = RowAdmission::default();
        rest.extend(ws);
        assert_eq!(rest.rejected(), 2);
        assert!(rest.holds("healthy"));
        assert!(!rest.conflicts("healthy"));

        let mut builder = SnapshotBuilder::default();
        assert!(builder.admit(cx, Ok(("healthy", 2)), Level::Warn, "unused"));
        assert!(!builder.admit(cx, Err(Some("bad")), Level::Warn, "snapshot row"));
        // Read current held rows at commit, after their newer update. Venue
        // freshness checks remain responsible for admitted-row precedence.
        assert_eq!(
            builder.commit([("healthy", 1), ("bad", 9), ("absent", 1)], |r| r.0),
            [("healthy", 2), ("bad", 9)]
        );
        // Deferred reporting must not mutate the coverage a second time.
        row_skipped(cx, Level::Error, "deferred row");
        assert_eq!(rest.rejected(), 2);
    }

    #[test]
    fn rest_and_ws_admission_report_once_and_propagation_stays_silent() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(admission_probe));
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 4);
        let events: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 4);
        for (event, (level, message)) in events.iter().zip([
            (Level::Warn, "known row"),
            (Level::Error, "unknown row"),
            (Level::Warn, "snapshot row"),
            (Level::Error, "deferred row"),
        ]) {
            assert_eq!(event.code, crate::log::code::ROW_SKIPPED);
            assert_eq!(event.level, level);
            assert_eq!(event.message, message);
        }
    }

    #[test]
    fn rejected_rows_count_even_when_the_host_filters_their_notices() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(admission_probe));
        engine.set_log_level(Level::Error);
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 4);
        assert_eq!(
            engine
                .drain_actions()
                .filter(|a| matches!(a, Action::Notice(Notice::Event(_))))
                .count(),
            2
        );
    }

    fn reporting_probe(cx: &mut Cx<'_, ()>) {
        let mut once = RowReports::<String>::default();
        assert!(once.report(cx, "".into(), Level::Warn, "once"));
        assert!(!once.report(cx, "".into(), Level::Error, "once upgraded"));
        assert!(once.forget(""));
        assert!(!once.forget(""));
        assert!(once.report(cx, "".into(), Level::Error, "once rearmed"));

        let mut upgrades = RowReports::new(RowRepeat::ErrorUpgrade);
        for (level, admitted) in [
            (Level::Warn, true),
            (Level::Error, true),
            (Level::Warn, false),
            (Level::Error, false),
        ] {
            assert_eq!(upgrades.report(cx, "key", level, "upgrade"), admitted);
        }
        // Error first is already final, regardless of subsequent message text.
        assert!(upgrades.report(cx, "error-first", Level::Error, "error first"));
        assert!(!upgrades.report(cx, "error-first", Level::Warn, "different"));

        let mut changed = RowReports::unbounded(RowRepeat::ChangedMessage);
        for (text, level, admitted) in [
            ("A", Level::Warn, true),
            ("A", Level::Error, false),
            ("B", Level::Warn, true),
            ("A", Level::Warn, true),
        ] {
            assert_eq!(changed.report(cx, "kind", level, text), admitted);
        }
        changed.clear();
        assert!(changed.report(cx, "kind", Level::Warn, "A"));
    }

    #[test]
    fn persistent_reports_preserve_repeat_policies_codes_levels_and_health() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(reporting_probe));
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 9);
        let events: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) => Some(event),
                _ => None,
            })
            .collect();
        let expected = [
            (Level::Warn, "once"),
            (Level::Error, "once rearmed"),
            (Level::Warn, "upgrade"),
            (Level::Error, "upgrade"),
            (Level::Error, "error first"),
            (Level::Warn, "A"),
            (Level::Warn, "B"),
            (Level::Warn, "A"),
            (Level::Warn, "A"),
        ];
        assert_eq!(events.len(), expected.len());
        for (event, (level, message)) in events.iter().zip(expected) {
            assert_eq!(event.code, crate::log::code::ROW_SKIPPED);
            assert_eq!((event.level, event.message.as_str()), (level, message));
        }
    }

    #[test]
    fn filtered_reports_still_remember_repeats_and_count_health_once() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(reporting_probe));
        engine.set_log_level(Level::Error);
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 9);
        let events: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 3);
        assert!(events.iter().all(|event| event.level == Level::Error));
    }

    fn namespace_probe(cx: &mut Cx<'_, ()>) {
        let mut reports = RowReports::new(RowRepeat::ErrorUpgrade);
        for key in ["order:", "order:x", "holding:x", "preorder:x"] {
            assert!(reports.report(cx, key.to_owned(), Level::Warn, key));
        }
        reports.rearm_namespace("order:", false, ["x"]);
        assert!(reports.report(cx, "order:x".into(), Level::Warn, "mapped"));
        assert!(!reports.report(cx, "order:".into(), Level::Warn, "unknown"));
        assert!(!reports.report(cx, "holding:x".into(), Level::Warn, "peer"));
        reports.rearm_namespace("order:", true, std::iter::empty());
        assert!(reports.report(cx, "order:".into(), Level::Warn, "clean unknown"));
        assert!(reports.report(cx, "order:x".into(), Level::Warn, "clean known"));
        assert!(!reports.report(cx, "holding:x".into(), Level::Warn, "peer"));
        assert!(!reports.report(cx, "preorder:x".into(), Level::Warn, "not a prefix"));
        // An empty mapped native identity also rearms its namespaced sentinel.
        reports.rearm_namespace("order:", false, [""]);
        assert!(reports.report(cx, "order:".into(), Level::Warn, "mapped unknown"));
    }

    #[test]
    fn namespace_rearm_preserves_peer_memory_and_empty_identity() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(namespace_probe));
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 8);
    }

    fn capacity_probe(cx: &mut Cx<'_, ()>) {
        let mut bounded = RowReports::default();
        for key in 0..4096 {
            assert!(bounded.report(cx, key, Level::Warn, "bounded"));
        }
        // Capacity reset precedes even a duplicate lookup, forgetting all peers.
        assert!(bounded.report(cx, 0, Level::Warn, "boundary duplicate"));
        assert!(!bounded.report(cx, 0, Level::Warn, "repeat"));
        assert!(bounded.report(cx, 1, Level::Warn, "old peer forgotten"));
        let mut unbounded = RowReports::unbounded(RowRepeat::Once);
        for key in 0..4097 {
            assert!(unbounded.report(cx, key, Level::Warn, "unbounded"));
        }
        assert!(!unbounded.report(cx, 0, Level::Warn, "unbounded repeat"));
        assert!(unbounded.forget(&0));
        assert!(unbounded.report(cx, 0, Level::Warn, "unbounded rearm"));
    }

    #[test]
    fn bounded_memory_clears_before_duplicate_lookup_unbounded_memory_does_not() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(capacity_probe));
        // Exercise capacity without accumulating thousands of host notices.
        engine.set_log_level(Level::Error);
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 4098 + 4098);
        assert!(
            !engine
                .drain_actions()
                .any(|a| matches!(a, Action::Notice(Notice::Event(_))))
        );
    }

    #[test]
    fn grouped_collectors_preserve_distinct_count_and_first_label_policies() {
        let mut count = RowCount::default();
        assert!(!count.any());
        assert_eq!(count.first(), None);
        assert_eq!(count.summary(|_| panic!("empty formatter")), None);
        for first in [None, Some(""), Some("z")] {
            let mut count = RowCount::default();
            for label in [first, Some("a"), Some("a"), None] {
                count.add(label);
            }
            assert_eq!(count.count(), 4);
            assert!(count.any());
            assert_eq!(count.first(), Some(first));
            let mut calls = 0;
            assert_eq!(
                count.summary(|_| {
                    calls += 1;
                    String::new()
                }),
                Some(String::new())
            );
            assert_eq!(calls, 1);
        }
        count.add(None);
        assert_eq!(count.first(), Some(None));
        let mut keys = RowKeys::default();
        assert!(!keys.any());
        assert_eq!(keys.summary(|_| panic!("empty formatter")), None);
        for label in [Some("z"), Some("a"), Some("z"), None, Some("")] {
            keys.add(label);
        }
        assert_eq!(keys.ids(), ["z", "a"]);
        assert_eq!(keys.anonymous(), 2);
        assert_eq!(keys.count(), 4);
        assert!(keys.any());
        let mut calls = 0;
        assert_eq!(
            keys.summary(|_| {
                calls += 1;
                "group".into()
            })
            .as_deref(),
            Some("group")
        );
        assert_eq!(calls, 1);
        assert_eq!(keys.into_ids(), ["z", "a"]);
    }

    fn summary_probe(cx: &mut Cx<'_, ()>) {
        assert!(!row_summary(cx, Level::Error, None));
        assert!(row_summary(cx, Level::Error, Some("group".into())));
        assert!(row_summary(cx, Level::Warn, Some(String::new())));
        let mut reports = RowReports::unbounded(RowRepeat::ChangedMessage);
        for (key, summary, expected) in [
            ("a", Some("A"), true),
            ("b", Some("B"), true),
            ("a", Some("A"), false),
            ("a", None, false),
            ("b", Some("B"), false),
            ("a", Some("A"), true),
            ("a", Some(""), true),
        ] {
            assert_eq!(
                reports.report_summary(cx, key, Level::Warn, summary.map(str::to_owned)),
                expected
            );
        }
    }

    #[test]
    fn grouped_summary_rearm_preserves_peers_empty_messages_codes_levels_and_health() {
        for filtered in [false, true] {
            let mut engine = crate::test_support::seeded_engine(AdmissionProbe(summary_probe));
            if filtered {
                engine.set_log_level(Level::Error);
            }
            engine.start(1.0);
            assert_eq!(engine.health().rows_skipped, 6);
            let events: Vec<_> = engine
                .drain_actions()
                .filter_map(|a| match a {
                    Action::Notice(Notice::Event(event)) => Some(event),
                    _ => None,
                })
                .collect();
            let expected: &[&str] = if filtered {
                &["group"]
            } else {
                &["group", "", "A", "B", "A", ""]
            };
            assert_eq!(
                events
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            for (i, event) in events.iter().enumerate() {
                assert_eq!(event.code, crate::log::code::ROW_SKIPPED);
                assert_eq!(event.level, if i == 0 { Level::Error } else { Level::Warn });
            }
        }
    }

    fn empty_capacity_probe(cx: &mut Cx<'_, ()>) {
        let mut reports = RowReports::default();
        for key in 0..4096 {
            assert!(reports.report_summary(cx, key, Level::Warn, Some("seen".into())));
        }
        assert!(!reports.report_summary(cx, usize::MAX, Level::Warn, None));
        assert!(!reports.report_summary(cx, 0, Level::Warn, None));
        assert!(!reports.report_summary(cx, 1, Level::Error, Some("remembered peer".into())));
        assert!(reports.report_summary(cx, 0, Level::Error, Some(String::new())));
    }

    #[test]
    fn empty_summary_at_capacity_forgets_only_its_key_without_clearing_peers() {
        let mut engine = crate::test_support::seeded_engine(AdmissionProbe(empty_capacity_probe));
        engine.set_log_level(Level::Error);
        engine.start(1.0);
        assert_eq!(engine.health().rows_skipped, 4097);
    }
}
