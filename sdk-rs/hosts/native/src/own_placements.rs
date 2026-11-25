//! Order identities this account's own clients placed (AGT-196).
//!
//! Two users can connect the same venue account, and each gets its own
//! account actor, so every actor's private stream carries the other user's
//! fills too. Only this actor's placements are known to belong to its user:
//! the identities its engine selected at dispatch (`commandProgress`) and the
//! native ids its placement completions returned. Bounded and memory-only, so
//! a restarted actor knows none of the orders placed before it.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};

/// Identities kept per account; the oldest are forgotten first.
const CAPACITY: usize = 4096;

/// Virtual protective legs answer for their parent order's identity.
const LEG_SUFFIXES: [&str; 6] = [
    "__stop_loss",
    "__take_profit",
    "__trailing_stop_loss",
    "_tsl",
    "_sl",
    "_tp",
];

/// Shared between the account actor, which records, and its handle's readers.
pub type Placements = Arc<Mutex<OwnPlacements>>;

#[derive(Debug, Default)]
pub struct OwnPlacements {
    order: VecDeque<String>,
    ids: HashSet<String>,
    in_flight: usize,
}

impl OwnPlacements {
    /// Remember one identity this account placed. Host-owned: only the account
    /// actor's placement path (and tests) may call it.
    pub fn record(&mut self, identity: &str) {
        if identity.is_empty() || self.ids.contains(identity) {
            return;
        }
        if self.order.len() == CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.ids.remove(&oldest);
        }
        self.order.push_back(identity.to_owned());
        self.ids.insert(identity.to_owned());
    }

    /// Whether this account placed the order with this id or client order id,
    /// or the order a virtual leg id belongs to.
    pub fn placed(&self, identity: &str) -> bool {
        self.ids.contains(identity)
            || LEG_SUFFIXES
                .iter()
                .filter_map(|suffix| identity.strip_suffix(suffix))
                .any(|parent| self.ids.contains(parent))
    }

    /// A placement is dispatched but not yet completed: its ids may still come.
    pub fn in_flight(&self) -> bool {
        self.in_flight > 0
    }

    /// A placement was dispatched. Host-owned, like `record`.
    pub fn begin(&mut self) {
        self.in_flight += 1;
    }

    /// A dispatched placement completed or failed. Host-owned, like `record`.
    pub fn end(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }
}

/// The ids in a placement completion's result: a plain JSON array of strings.
/// Any other shape records nothing, so an unknown order is never claimed.
pub(crate) fn result_ids(result: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(result).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_for_recorded_ids_and_their_virtual_legs_only() {
        let mut placements = OwnPlacements::default();
        placements.record("o-1");
        assert!(placements.placed("o-1"));
        assert!(placements.placed("o-1_sl"));
        assert!(placements.placed("o-1__take_profit"));
        assert!(!placements.placed("o-2"));
        assert!(!placements.placed("o"));
        placements.record("");
        assert!(!placements.placed(""));
    }

    #[test]
    fn forgets_the_oldest_past_capacity() {
        let mut placements = OwnPlacements::default();
        for index in 0..=CAPACITY {
            placements.record(&index.to_string());
        }
        assert!(!placements.placed("0"));
        assert!(placements.placed("1"));
        assert!(placements.placed(&CAPACITY.to_string()));
    }

    #[test]
    fn in_flight_counts_dispatched_placements() {
        let mut placements = OwnPlacements::default();
        assert!(!placements.in_flight());
        placements.begin();
        placements.begin();
        placements.end();
        assert!(placements.in_flight());
        placements.end();
        placements.end();
        assert!(!placements.in_flight());
    }

    #[test]
    fn result_ids_read_plain_string_arrays_only() {
        assert_eq!(result_ids(r#"["7","8"]"#), vec!["7", "8"]);
        assert!(result_ids(r#"{"orderIds":["7"]}"#).is_empty());
        assert!(result_ids("[7]").is_empty());
        assert!(result_ids("null").is_empty());
    }
}
