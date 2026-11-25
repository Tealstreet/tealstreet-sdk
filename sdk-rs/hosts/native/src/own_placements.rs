//! Order identities this account's own clients placed or amended (AGT-196).
//!
//! Two users can connect the same venue account, and each gets its own
//! account actor, so every actor's private stream carries the other user's
//! fills too. This records what is known to be this actor's: the identities
//! its engine generated at dispatch (`commandProgress`) and the native ids its
//! placement and amend completions, or a batch's proven legs, returned.
//! Bounded and memory-only, so a restarted actor knows none of the orders
//! placed before it; a miss means "not known here", never "someone else's".
//!
//! Every identity is scoped to its symbol: Binance USDⓈ-M order ids repeat
//! across symbols, so `7` on BTCUSDT never claims `7` on ETHUSDT. Each expires
//! `RETENTION` after it was last recorded.
//!
//! A CALLER-chosen client id (`desk`) can name another login's order: a venue
//! may report a duplicate as Unknown, and a closed id can be reused. So it is
//! never claimed from a receipt; only a proven placement's native ids are,
//! and an id equal to a caller id (Binance portfolio margin and spot key
//! orders by client id) retires `TERMINAL_GRACE` after its order closes or
//! leaves the open orders. Coinbase answers a repeated client id with the
//! EXISTING order's id, so there no caller-id placement claims at all.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Identities kept per account; the oldest are forgotten first.
const CAPACITY: usize = 4096;

/// How long a recorded identity claims fills. Most orders fill or cancel within
/// hours; a resting TP/SL or far limit can outlive this and is then unclaimed
/// (both logins notified, the fail-open side). Longer would let a reused client
/// id, or Binance portfolio margin's client-keyed ids, claim another login's
/// later order with the same id, withholding it from its placer.
pub const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Shared between the account actor, which records, and its handle's readers.
pub type Placements = Arc<Mutex<OwnPlacements>>;

/// Poison-safe: this ledger only labels notifications, so a panic elsewhere
/// while it was held must never take order placement down with it.
pub fn lock(placements: &Placements) -> MutexGuard<'_, OwnPlacements> {
    placements.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How long a caller-chosen id still claims after its order closed: long
/// enough for that order's last fills to be reported, short enough that a
/// reuse of the id elsewhere is not claimed.
pub const TERMINAL_GRACE: Duration = Duration::from_secs(60);

/// (symbol, order id or client order id).
pub type Key = (String, String);

/// One leg of a placement: its symbol and the client id its caller chose.
pub type Leg = (String, Option<String>);

/// One store row, as `retire_caller_ids` reads it: (symbol, id or client id,
/// closed or canceled).
pub type Row<'a> = (&'a str, &'a str, bool);

#[derive(Debug, Default)]
pub struct OwnPlacements {
    /// Each identity's expiry.
    recorded: HashMap<Key, Instant>,
    /// Recorded identities equal to a caller-chosen client id, and whether a
    /// store row for them has been seen.
    caller: HashMap<Key, bool>,
    /// The venue completes a repeated client id with the existing order's id
    /// (Coinbase), so a caller-id placement proves nothing about who placed it.
    idempotent_client_ids: bool,
}

impl OwnPlacements {
    /// Remember one identity this account placed on `symbol`. Host-owned: only
    /// the account actor's placement path (and tests) may call it. True when it
    /// was not already claimed here, so a refusal may `forget` it again.
    pub fn record(&mut self, symbol: &str, identity: &str) -> bool {
        self.record_at(symbol, identity, Instant::now())
    }

    pub fn record_at(&mut self, symbol: &str, identity: &str, now: Instant) -> bool {
        if symbol.is_empty() || identity.is_empty() {
            return false;
        }
        let fresh = !self.placed_at(symbol, identity, now);
        let key = (symbol.to_owned(), identity.to_owned());
        if !self.recorded.contains_key(&key) && self.recorded.len() >= CAPACITY {
            self.recorded.retain(|_, expiry| now < *expiry);
            if self.recorded.len() >= CAPACITY
                && let Some(oldest) = self
                    .recorded
                    .iter()
                    .min_by_key(|(_, expiry)| **expiry)
                    .map(|(key, _)| key.clone())
            {
                self.recorded.remove(&oldest);
            }
            let recorded = &self.recorded;
            self.caller.retain(|key, _| recorded.contains_key(key));
        }
        self.recorded.insert(key, now + RETENTION);
        fresh
    }

    /// Drop identities a placement recorded before the venue definitively
    /// refused it.
    pub fn forget(&mut self, keys: &[Key]) {
        for key in keys {
            self.recorded.remove(key);
            self.caller.remove(key);
        }
    }

    /// A command's engine-generated dispatch receipts on the symbols it was
    /// checked against. A receipt naming another symbol, or a caller-chosen
    /// client id, records nothing. Returns the keys this added, for `forget`.
    pub fn record_receipts(
        &mut self,
        symbols: &[String],
        legs: &[Leg],
        receipts: &[safe_cex_engine::OrderIdentityReceipt],
    ) -> Vec<Key> {
        receipts
            .iter()
            .filter(|receipt| symbols.contains(&receipt.symbol))
            .filter(|receipt| !caller_chosen(legs, &receipt.identity))
            .filter(|receipt| self.record(&receipt.symbol, &receipt.identity))
            .map(|receipt| (receipt.symbol.clone(), receipt.identity.clone()))
            .collect()
    }

    /// Coinbase: a repeated client id completes with the existing order's id.
    pub fn set_idempotent_client_ids(&mut self) {
        self.idempotent_client_ids = true;
    }

    /// Whether a caller-id placement's acknowledgement may name another
    /// login's order, so it must not be claimed.
    fn unprovable(&self, legs: &[Leg]) -> bool {
        self.idempotent_client_ids && legs.iter().any(|(_, caller)| caller.is_some())
    }

    /// A completed command's ids: every leg was accepted. A result names no
    /// symbol, so only a single-symbol command's ids are recorded, under that
    /// symbol; a multi-symbol batch's cannot be attributed and record nothing.
    pub fn record_completion(&mut self, symbols: &[String], legs: &[Leg], result: &str) {
        let [symbol] = symbols else {
            return;
        };
        if self.unprovable(legs) {
            return;
        }
        for id in result_ids(result) {
            self.record_placed(symbol, &id, caller_chosen(legs, &id));
        }
    }

    /// A failed batch's legs the venue proved accepted. A leg is matched by
    /// its client id to the (symbol, client id) its receipt named, never by
    /// position: venues number native rows (split TP/SL legs) separately.
    /// Unknown, refused and unmatched legs record nothing.
    pub fn record_proven_legs(
        &mut self,
        sent: &[Key],
        legs: &[Leg],
        elements: &[safe_cex_engine::write_plan::ElementOutcome],
    ) {
        use safe_cex_engine::write_plan::ElementResult;
        if self.unprovable(legs) {
            return;
        }
        for element in elements {
            let ElementResult::Placed { order_ids } = &element.result else {
                continue;
            };
            let Some(client) = element.element.client_order_id.as_deref() else {
                continue;
            };
            let Some((symbol, _)) = sent.iter().find(|(_, identity)| identity == client) else {
                continue;
            };
            let caller = caller_chosen(legs, client);
            for id in order_ids {
                self.record_placed(symbol, id, caller && id == client);
            }
        }
    }

    fn record_placed(&mut self, symbol: &str, id: &str, caller: bool) {
        self.record(symbol, id);
        if caller {
            self.caller
                .insert((symbol.to_owned(), id.to_owned()), false);
        }
    }

    /// Whether any caller-chosen id needs `retire_caller_ids`.
    pub fn has_caller_ids(&self) -> bool {
        !self.caller.is_empty()
    }

    /// The store's order rows: a caller-chosen id whose order is closed or
    /// canceled, or whose row was seen and is now gone (an open-orders
    /// snapshot after a stream gap dropped it), stops claiming after
    /// `TERMINAL_GRACE`, so its reuse by another login is not claimed.
    pub fn retire_caller_ids<'a>(&mut self, rows: impl IntoIterator<Item = Row<'a>>, now: Instant) {
        let mut present: HashMap<Key, bool> = HashMap::new();
        for (symbol, identity, terminal) in rows {
            let key = (symbol.to_owned(), identity.to_owned());
            if self.caller.contains_key(&key) {
                *present.entry(key).or_default() |= terminal;
            }
        }
        let mut retired = Vec::new();
        for (key, seen) in &mut self.caller {
            match present.get(key) {
                Some(true) => retired.push(key.clone()),
                Some(false) => *seen = true,
                None if *seen => retired.push(key.clone()),
                None => {}
            }
        }
        for key in retired {
            self.caller.remove(&key);
            if let Some(expiry) = self.recorded.get_mut(&key) {
                *expiry = (*expiry).min(now + TERMINAL_GRACE);
            }
        }
    }

    /// A venue order an algo's trigger created is this account's when the algo
    /// is, by its id or its client id. Never the reverse: a child cannot make
    /// its algo ours.
    pub fn link(&mut self, symbol: &str, order_id: &str, triggered_by: &[String]) {
        self.link_at(symbol, order_id, triggered_by, Instant::now());
    }

    pub fn link_at(&mut self, symbol: &str, order_id: &str, triggered_by: &[String], now: Instant) {
        if triggered_by
            .iter()
            .any(|algo| self.placed_at(symbol, algo, now))
        {
            self.record_at(symbol, order_id, now);
        }
    }

    /// Whether this account placed the order on `symbol` with exactly this id
    /// or client order id, and it has not expired. Never by prefix: fills carry
    /// native ids, and a caller's `desk_sl` is not the leg of another's `desk`.
    pub fn placed(&self, symbol: &str, identity: &str) -> bool {
        self.placed_at(symbol, identity, Instant::now())
    }

    pub fn placed_at(&self, symbol: &str, identity: &str, now: Instant) -> bool {
        self.recorded
            .get(&(symbol.to_owned(), identity.to_owned()))
            .is_some_and(|expiry| now < *expiry)
    }
}

fn caller_chosen(legs: &[Leg], identity: &str) -> bool {
    legs.iter()
        .any(|(_, caller)| caller.as_deref() == Some(identity))
}

/// Hyperliquid's amend completion: the replacement order's ids and cloid.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Replacement {
    order_ids: Vec<String>,
    client_order_id: String,
}

/// The ids in a placement or amend completion's result: a JSON array of
/// strings, one string, or an amend's `Replacement`. Any other shape records
/// nothing, so an unknown order is never claimed.
pub(crate) fn result_ids(result: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(result)
        .or_else(|_| serde_json::from_str::<String>(result).map(|id| vec![id]))
        .or_else(|_| {
            serde_json::from_str::<Replacement>(result).map(|replacement| {
                let mut ids = replacement.order_ids;
                ids.push(replacement.client_order_id);
                ids
            })
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BTC: &str = "BTCUSDT";

    #[test]
    fn answers_for_recorded_ids_only() {
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "o-1");
        assert!(placements.placed(BTC, "o-1"));
        assert!(!placements.placed(BTC, "o-2"));
        assert!(!placements.placed(BTC, "o"));
        placements.record(BTC, "");
        assert!(!placements.placed(BTC, ""));
        placements.record("", "o-3");
        assert!(!placements.placed("", "o-3"));
    }

    /// Binance USDⓈ-M order ids are per symbol: this account's BTCUSDT order 7
    /// is not another login's ETHUSDT order 7 (AGT-196 review B).
    #[test]
    fn an_id_claims_only_on_its_own_symbol() {
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "7");
        placements.record("ETHUSDT", "algo-1");
        placements.link(BTC, "child-1", &["algo-1".into()]);
        assert!(placements.placed(BTC, "7"));
        assert!(!placements.placed("ETHUSDT", "7"));
        assert!(
            !placements.placed(BTC, "child-1"),
            "an algo on another symbol links nothing here"
        );
    }

    /// Review B (e49c33ac90): receipts record only on the symbols the command
    /// was checked against, and report only what they newly added.
    #[test]
    fn receipts_record_only_on_checked_symbols() {
        let receipt = |symbol: &str, identity: &str| safe_cex_engine::OrderIdentityReceipt {
            symbol: symbol.into(),
            kind: safe_cex_engine::OrderIdentityKind::ClientOrderId,
            identity: identity.into(),
        };
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "held");
        let added = placements.record_receipts(
            &[BTC.into()],
            &[(BTC.into(), Some("desk".into()))],
            &[
                receipt(BTC, "desk"),
                receipt(BTC, "new"),
                receipt(BTC, "held"),
                receipt("ETHUSDT", "other"),
            ],
        );
        assert_eq!(added, [(BTC.to_owned(), "new".to_owned())]);
        assert!(!placements.placed("ETHUSDT", "other"));
        assert!(
            !placements.placed(BTC, "desk"),
            "a caller's id is never claimed from a receipt"
        );
        placements.forget(&added);
        assert!(!placements.placed(BTC, "new") && placements.placed(BTC, "held"));
    }

    fn placed_leg(
        index: usize,
        client: Option<&str>,
        ids: &[&str],
    ) -> safe_cex_engine::write_plan::ElementOutcome {
        use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementResult};
        ElementOutcome {
            element: Element {
                index,
                client_order_id: client.map(Into::into),
                role: None,
            },
            result: ElementResult::Placed {
                order_ids: ids.iter().map(|id| (*id).into()).collect(),
            },
        }
    }

    /// Review B (942d22190f, e6e681d490): a failed batch records only the legs
    /// the venue proved placed, each matched by its client id to the symbol its
    /// receipt named, never by position (Binance numbers split TP/SL rows
    /// separately). Synthetic elements.
    #[test]
    fn a_failed_batch_records_only_its_proven_legs_by_client_id() {
        use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementResult};
        let legs: Vec<Leg> = vec![(BTC.into(), None), ("ETHUSDT".into(), None)];
        let sent: Vec<Key> = vec![
            (BTC.into(), "c-btc".into()),
            ("ETHUSDT".into(), "c-eth".into()),
        ];
        let mut placements = OwnPlacements::default();
        placements.record_proven_legs(
            &sent,
            &legs,
            &[
                // Index 0 is the ETH leg's row: the index is not the batch position.
                placed_leg(0, Some("c-eth"), &["eth-1"]),
                ElementOutcome {
                    element: Element {
                        index: 1,
                        client_order_id: Some("c-btc".into()),
                        role: None,
                    },
                    result: ElementResult::Unknown {
                        error: Box::new(safe_cex_engine::VenueError::new("lost")),
                    },
                },
                placed_leg(2, None, &["no-client"]),
                placed_leg(3, Some("unsent"), &["no-receipt"]),
            ],
        );
        assert!(placements.placed("ETHUSDT", "eth-1"));
        assert!(!placements.placed(BTC, "eth-1"));
        for id in ["no-client", "no-receipt"] {
            assert!(
                !placements.placed(BTC, id) && !placements.placed("ETHUSDT", id),
                "{id}"
            );
        }
    }

    /// Review B (e6e681d490): a completed batch over several symbols cannot say
    /// which id is whose, so it records none; a single-symbol one records all.
    #[test]
    fn a_completion_records_ids_only_under_its_one_symbol() {
        let mut placements = OwnPlacements::default();
        let legs: Vec<Leg> = vec![(BTC.into(), None), ("ETHUSDT".into(), None)];
        placements.record_completion(&[BTC.into(), "ETHUSDT".into()], &legs, r#"["1","2"]"#);
        assert!(!placements.placed(BTC, "1") && !placements.placed("ETHUSDT", "2"));
        placements.record_completion(&[BTC.into()], &[(BTC.into(), None)], r#"["3"]"#);
        assert!(placements.placed(BTC, "3") && !placements.placed("ETHUSDT", "3"));
    }

    /// Review B (e6e681d490): Coinbase completes a repeated client id with the
    /// existing order's id, which may be another login's, so no caller-id
    /// placement claims there. Generated ids still do. Synthetic.
    #[test]
    fn coinbase_caller_id_placements_never_claim() {
        let mut placements = OwnPlacements::default();
        placements.set_idempotent_client_ids();
        let desk: Vec<Leg> = vec![(BTC.into(), Some("desk".into()))];
        placements.record_completion(&[BTC.into()], &desk, r#"["their-order"]"#);
        placements.record_proven_legs(
            &[(BTC.into(), "desk".into())],
            &desk,
            &[placed_leg(0, Some("desk"), &["their-order"])],
        );
        assert!(!placements.placed(BTC, "their-order"));
        placements.record_completion(&[BTC.into()], &[(BTC.into(), None)], r#"["mine"]"#);
        assert!(placements.placed(BTC, "mine"));
    }

    /// Review B (942d22190f, e6e681d490): an id equal to a caller-chosen client
    /// id stops claiming shortly after its order closes, or after its row, once
    /// seen, is gone from the store (a post-gap open-orders snapshot dropped
    /// it). A row never seen yet (completion before stream) does not retire.
    /// A generated id keeps its retention. Synthetic rows.
    #[test]
    fn a_caller_id_retires_when_its_order_closes_or_leaves_the_open_orders() {
        let start = Instant::now();
        let later = start + TERMINAL_GRACE;
        let mut placements = OwnPlacements::default();
        for (id, caller) in [
            ("closed", true),
            ("gapped", true),
            ("pending", true),
            ("9001", false),
        ] {
            let caller = caller.then(|| id.to_owned());
            placements.record_completion(
                &[BTC.into()],
                &[(BTC.into(), caller)],
                &format!(r#"["{id}"]"#),
            );
        }
        placements.retire_caller_ids([(BTC, "closed", false), (BTC, "gapped", false)], start);
        placements.retire_caller_ids([(BTC, "closed", true), (BTC, "9001", true)], start);
        assert!(placements.has_caller_ids(), "pending is still unseen");
        let soon = later - Duration::from_secs(1);
        for id in ["closed", "gapped"] {
            assert!(
                placements.placed_at(BTC, id, soon),
                "{id}: its own last fills"
            );
            assert!(!placements.placed_at(BTC, id, later), "{id}: a later reuse");
        }
        assert!(placements.placed_at(BTC, "pending", later));
        assert!(placements.placed_at(BTC, "9001", later));
    }

    /// A caller-reused client id, or a portfolio-margin client-keyed id, ages
    /// out: an old `desk` must not claim a much later `desk` placed elsewhere.
    #[test]
    fn an_identity_claims_only_within_its_retention() {
        let start = Instant::now();
        let mut placements = OwnPlacements::default();
        placements.record_at(BTC, "desk", start);
        let fresh = start + RETENTION - Duration::from_secs(1);
        assert!(placements.placed_at(BTC, "desk", fresh));
        let later = start + RETENTION;
        assert!(!placements.placed_at(BTC, "desk", later));
        placements.link_at(BTC, "child-1", &["desk".into()], later);
        assert!(
            !placements.placed_at(BTC, "child-1", later),
            "an expired algo links nothing"
        );
        placements.record_at(BTC, "desk", later);
        assert!(
            placements.placed_at(BTC, "desk", later + RETENTION / 2),
            "recording it again restarts its retention"
        );
    }

    /// A caller's client id that happens to end in a leg suffix is another
    /// order: `desk` must never claim `desk_sl` (AGT-196 review).
    #[test]
    fn a_leg_suffixed_client_id_is_not_claimed_by_its_prefix() {
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "desk");
        for other in [
            "desk_sl",
            "desk_tp",
            "desk_tsl",
            "desk__stop_loss",
            "desk__take_profit",
        ] {
            assert!(!placements.placed(BTC, other), "{other}");
        }
    }

    #[test]
    fn a_triggered_order_is_ours_when_its_algo_is_by_id_or_client_id() {
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "algo-1");
        placements.record(BTC, "algo-client-2");
        placements.link(BTC, "child-1", &["algo-1".into(), String::new()]);
        placements.link(
            BTC,
            "child-2",
            &["unknown-algo".into(), "algo-client-2".into()],
        );
        placements.link(BTC, "child-3", &["someone-elses-algo".into()]);
        assert!(placements.placed(BTC, "child-1"), "by algoId");
        assert!(placements.placed(BTC, "child-2"), "by algoClOrdId alone");
        assert!(!placements.placed(BTC, "child-3"));
        assert!(
            !placements.placed(BTC, "someone-elses-algo"),
            "never the reverse"
        );
    }

    #[test]
    fn forgets_the_oldest_past_capacity() {
        let start = Instant::now();
        let mut placements = OwnPlacements::default();
        for index in 0..=CAPACITY {
            let at = start + Duration::from_millis(index as u64);
            placements.record_at(BTC, &index.to_string(), at);
        }
        let now = start + Duration::from_secs(60);
        assert!(!placements.placed_at(BTC, "0", now));
        assert!(placements.placed_at(BTC, "1", now));
        assert!(placements.placed_at(BTC, &CAPACITY.to_string(), now));
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
        lock(&placements).record(BTC, "after-poison");
        assert!(lock(&placements).placed(BTC, "after-poison"));
    }

    /// Hyperliquid completes an amend with its replacement's object
    /// (`venues/hyperliquid/src/trading.rs`), not an array (AGT-196 review).
    #[test]
    fn result_ids_read_arrays_strings_and_amend_replacements() {
        assert_eq!(result_ids(r#""9""#), vec!["9"]);
        assert_eq!(result_ids(r#"["7","8"]"#), vec!["7", "8"]);
        assert_eq!(
            result_ids(r#"{"orderIds":["7"],"clientOrderId":"0xabc"}"#),
            vec!["7", "0xabc"]
        );
        assert!(result_ids(r#"{"orderIds":[7]}"#).is_empty());
        assert!(result_ids(r#"{"partial":{"orderIds":["7"]}}"#).is_empty());
        assert!(result_ids("[7]").is_empty());
        assert!(result_ids("null").is_empty());
    }
}
