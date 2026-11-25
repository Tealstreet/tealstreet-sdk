//! Order identities this account's own clients placed or amended (AGT-196).
//!
//! Two users can connect the same venue account, and each gets its own
//! account actor, so every actor's private stream carries the other user's
//! fills too. This records what is known to be this actor's: the identities
//! its engine selected at dispatch (`commandProgress`) and the native ids its
//! placement and amend completions returned. Bounded and memory-only, so a
//! restarted actor knows none of the orders placed before it; a miss means
//! "not known here", never "someone else's".

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Identities kept per account; the oldest are forgotten first.
const CAPACITY: usize = 4096;

/// Shared between the account actor, which records, and its handle's readers.
pub type Placements = Arc<Mutex<OwnPlacements>>;

/// Poison-safe: this ledger only labels notifications, so a panic elsewhere
/// while it was held must never take order placement down with it.
pub fn lock(placements: &Placements) -> MutexGuard<'_, OwnPlacements> {
    placements.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Default)]
pub struct OwnPlacements {
    order: VecDeque<String>,
    ids: HashSet<String>,
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

    /// Whether this account placed the order with exactly this id or client
    /// order id. Never by prefix: fills carry native ids, and a caller's
    /// `desk_sl` is not the leg of another user's `desk`.
    pub fn placed(&self, identity: &str) -> bool {
        self.ids.contains(identity)
    }
}

/// The ids in a placement or amend completion's result: a JSON array of
/// strings or one string. Any other shape records nothing, so an unknown
/// order is never claimed.
pub(crate) fn result_ids(result: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(result)
        .or_else(|_| serde_json::from_str::<String>(result).map(|id| vec![id]))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_for_recorded_ids_only() {
        let mut placements = OwnPlacements::default();
        placements.record("o-1");
        assert!(placements.placed("o-1"));
        assert!(!placements.placed("o-2"));
        assert!(!placements.placed("o"));
        placements.record("");
        assert!(!placements.placed(""));
    }

    /// A caller's client id that happens to end in a leg suffix is another
    /// order: `desk` must never claim `desk_sl` (AGT-196 review).
    #[test]
    fn a_leg_suffixed_client_id_is_not_claimed_by_its_prefix() {
        let mut placements = OwnPlacements::default();
        placements.record("desk");
        for other in ["desk_sl", "desk_tp", "desk_tsl", "desk__stop_loss", "desk__take_profit"] {
            assert!(!placements.placed(other), "{other}");
        }
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
    fn a_poisoned_ledger_still_answers() {
        let placements = Placements::default();
        let poisoner = placements.clone();
        let _ = std::thread::spawn(move || {
            let _held = poisoner.lock().unwrap();
            panic!("poison the ledger");
        })
        .join();
        assert!(placements.is_poisoned());
        lock(&placements).record("after-poison");
        assert!(lock(&placements).placed("after-poison"));
    }

    #[test]
    fn result_ids_read_string_arrays_and_single_strings_only() {
        assert_eq!(result_ids(r#""9""#), vec!["9"]);
        assert_eq!(result_ids(r#"["7","8"]"#), vec!["7", "8"]);
        assert!(result_ids(r#"{"orderIds":["7"]}"#).is_empty());
        assert!(result_ids("[7]").is_empty());
        assert!(result_ids("null").is_empty());
    }
}
