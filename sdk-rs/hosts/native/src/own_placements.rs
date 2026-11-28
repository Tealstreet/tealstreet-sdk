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
//! A CALLER-chosen client id (`desk`) is never claimed: a venue may report a
//! duplicate as Unknown, and a closed id can be reused by another order. Only
//! the venue's own native order id from a proven placement claims. Where the
//! venue keys orders by client id (Binance spot and portfolio margin, GRVT),
//! its completion carries the client id, so the ack's native id arrives as
//! `Notice::PlacementNativeId`; an ack without one records nothing. Coinbase
//! answers a repeated client id with the EXISTING order's id, so there no
//! caller-id placement claims at all.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Identities kept per account; the oldest are forgotten first.
const CAPACITY: usize = 4096;

/// How long a recorded identity claims fills. Most orders fill or cancel within
/// hours; a resting TP/SL or far limit can outlive this and is then unclaimed
/// (both logins notified, the fail-open side). Native ids are never reused, so
/// this only bounds memory and engine-generated client ids.
pub const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Shared between the account actor, which records, and its handle's readers.
pub type Placements = Arc<Mutex<OwnPlacements>>;

/// Poison-safe: this ledger only labels notifications, so a panic elsewhere
/// while it was held must never take order placement down with it.
pub fn lock(placements: &Placements) -> MutexGuard<'_, OwnPlacements> {
    placements.lock().unwrap_or_else(PoisonError::into_inner)
}

/// (symbol, order id or client order id).
pub type Key = (String, String);

/// One leg of a placement: its symbol and the client id its caller chose.
pub type Leg = (String, Option<String>);

#[derive(Debug, Default)]
pub struct OwnPlacements {
    /// Each identity's expiry.
    recorded: HashMap<Key, Instant>,
    /// The venue completes a repeated client id with the existing order's id
    /// (Coinbase), so a caller-id placement proves nothing about who placed it.
    idempotent_client_ids: bool,
    /// A client-keyed venue's native ids (`Notice::OrderNativeId`), with their
    /// expiry: the latest by (symbol, client id), and each fill's by (symbol,
    /// client id, fill id). Fill keys and claims use them.
    native_ids: HashMap<Key, (String, Instant)>,
    fill_native_ids: HashMap<(String, String, String), (String, Instant)>,
    /// The leg suffixes (`_sl`, `_tp`) last seen on rows of this account's own
    /// native orders (`observe_legs`), by (symbol, native id).
    leg_rows: HashMap<Key, (Vec<String>, Instant)>,
    /// Own orders that gained a leg while this account's own command on that
    /// symbol was in flight: theirs or another login's, settled by the command.
    /// Each keeps the client ids and parents its rows carry, which a child may
    /// name instead of the native id.
    provisional: HashMap<Key, (Vec<String>, Instant)>,
    /// Own orders that gained a leg another client added (AGT-333: another
    /// login's OKX full-position leg joined this one's algo), by native id and
    /// by each of their rows' client ids and parents. Which leg fired is
    /// unknown, so their triggered children link to nobody.
    joint: HashMap<Key, ((), Instant)>,
    /// Own orders an own placement just joined before their new leg row was
    /// seen (the ack came first): that leg, seen within `OWN_JOIN_WINDOW`, is
    /// this account's own.
    own_joins: HashMap<Key, Instant>,
}

/// How long an own join's acknowledgement waits for its new leg row. OKX
/// allows one leg of each kind, so once the join is acknowledged no other
/// client can add a leg until one is deleted.
const OWN_JOIN_WINDOW: Duration = Duration::from_secs(60);

/// A fill's native order id on a client-keyed venue (`OwnPlacements::resolve`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The order id is already native: no `OrderNativeId` named it.
    Native,
    /// Named for this very fill: exact even when two orders share the id.
    Fill(String),
    /// The fill carried no id, so only the latest order under this client id
    /// is known; it may be another order's.
    Latest(String),
}

/// Insert into a bounded map of expiring entries, dropping expired then the
/// soonest-expiring entries when full.
fn insert_bounded<K: Eq + std::hash::Hash + Clone, V>(
    map: &mut HashMap<K, (V, Instant)>,
    key: K,
    value: V,
    now: Instant,
) {
    if !map.contains_key(&key) && map.len() >= CAPACITY {
        map.retain(|_, (_, expiry)| now < *expiry);
        if map.len() >= CAPACITY
            && let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (_, expiry))| *expiry)
                .map(|(key, _)| key.clone())
        {
            map.remove(&oldest);
        }
    }
    map.insert(key, (value, now + RETENTION));
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
        }
        // A joint order stays joint as long as it stays recorded here.
        if self.joint.contains_key(&key) {
            insert_bounded(&mut self.joint, key.clone(), (), now);
        }
        self.recorded.insert(key, now + RETENTION);
        fresh
    }

    /// Drop identities a placement recorded before the venue definitively
    /// refused it.
    pub fn forget(&mut self, keys: &[Key]) {
        for key in keys {
            self.recorded.remove(key);
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

    /// Whether `set_idempotent_client_ids` applies to this account.
    pub fn idempotent_client_ids(&self) -> bool {
        self.idempotent_client_ids
    }

    /// Whether a caller-id placement's acknowledgement may name another
    /// login's order, so it must not be claimed.
    fn unprovable(&self, legs: &[Leg]) -> bool {
        self.idempotent_client_ids && legs.iter().any(|(_, caller)| caller.is_some())
    }

    /// A completed command's ids: every leg was accepted. A result names no
    /// symbol, so only a single-symbol command's ids are recorded, under that
    /// symbol; a multi-symbol batch's cannot be attributed and record nothing.
    /// An id equal to a caller-chosen client id (a client-keyed venue's
    /// completion) never claims: that venue's native id comes from
    /// `record_placement_native_id`.
    ///
    /// `held` lists the rows already open on the symbol when the command was
    /// dispatched that are not this account's (`held_rows`). A completion id
    /// naming one of them is an order the command joined, not one it placed
    /// (OKX completes a full-position join with the held algo's id, AGT-333),
    /// so it records nothing.
    pub fn record_completion(
        &mut self,
        symbols: &[String],
        legs: &[Leg],
        result: &str,
        held: &[String],
    ) {
        let [symbol] = symbols else {
            return;
        };
        if self.unprovable(legs) {
            return;
        }
        for id in result_ids(result) {
            if !caller_chosen(legs, &id) && !names_held(held, &id) {
                self.record(symbol, &id);
            }
        }
    }

    /// The ids and parent ids of `orders` on `symbols` that this account did
    /// not place: what a placement dispatched now may join but never claims.
    pub fn held_rows(
        &self,
        symbols: &[String],
        orders: &[safe_cex_engine::model::Order],
    ) -> Vec<String> {
        let now = Instant::now();
        // A leg row (`{algoId}_sl`) is this account's when its native order is.
        let ours = |order: &safe_cex_engine::model::Order| {
            [
                Some(order.id.as_str()),
                order.id.split_once('_').map(|(native, _)| native),
                order.parent_id.as_deref(),
                order.client_order_id.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|identity| self.placed_at(&order.symbol, identity, now))
        };
        orders
            .iter()
            .filter(|order| symbols.contains(&order.symbol) && !ours(order))
            .flat_map(|order| [Some(order.id.clone()), order.parent_id.clone()])
            .flatten()
            .collect()
    }

    /// A failed batch's legs the venue proved accepted. A leg is matched by
    /// its client id to the (symbol, client id) its receipt named, never by
    /// position: venues number native rows (split TP/SL legs) separately. A
    /// client id sent on two symbols matches neither. Unknown, refused and
    /// unmatched legs, and caller-chosen client ids, record nothing.
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
            // An element names no symbol: a client id the batch sent on two
            // symbols cannot say which leg this is, so it records nothing.
            let mut named = sent.iter().filter(|(_, identity)| identity == client);
            let (Some((symbol, _)), None) = (named.next(), named.next()) else {
                continue;
            };
            let symbol = symbol.clone();
            for id in order_ids.iter().filter(|id| !caller_chosen(legs, id)) {
                self.record(&symbol, id);
            }
        }
    }

    /// The venue acknowledged a leg placed under the caller's `client_order_id`
    /// on `symbol` with its own `native_id` (`Notice::PlacementNativeId`):
    /// that native id, never the reusable client id, is this account's.
    pub fn record_placement_native_id(
        &mut self,
        legs: &[Leg],
        symbol: &str,
        client_order_id: &str,
        native_id: &str,
    ) {
        let named = legs
            .iter()
            .any(|(leg, caller)| leg == symbol && caller.as_deref() == Some(client_order_id));
        if named && !self.unprovable(legs) {
            self.record(symbol, native_id);
        }
    }

    /// A client-keyed venue named `native_id` for its order `order_id`, with
    /// the fill (`fill_id`) that named it. Bounded like the claims.
    pub fn note_native_id(
        &mut self,
        symbol: &str,
        order_id: &str,
        native_id: &str,
        fill_id: Option<&str>,
    ) {
        let now = Instant::now();
        let key = (symbol.to_owned(), order_id.to_owned());
        insert_bounded(&mut self.native_ids, key, native_id.to_owned(), now);
        if let Some(fill) = fill_id.filter(|fill| !fill.is_empty()) {
            let key = (symbol.to_owned(), order_id.to_owned(), fill.to_owned());
            insert_bounded(&mut self.fill_native_ids, key, native_id.to_owned(), now);
        }
    }

    /// The native order id of one fill of `order_id` on `symbol`.
    pub fn resolve(&self, symbol: &str, order_id: &str, fill_id: Option<&str>) -> Resolved {
        let now = Instant::now();
        let live = |entry: Option<&(String, Instant)>| {
            entry
                .filter(|(_, expiry)| now < *expiry)
                .map(|(native, _)| native.clone())
        };
        if let Some(fill) = fill_id
            && let Some(native) = live(self.fill_native_ids.get(&(
                symbol.to_owned(),
                order_id.to_owned(),
                fill.to_owned(),
            )))
        {
            return Resolved::Fill(native);
        }
        live(
            self.native_ids
                .get(&(symbol.to_owned(), order_id.to_owned())),
        )
        .map_or(Resolved::Native, Resolved::Latest)
    }

    /// Whether any native id seen under client id `order_id` on `symbol` is
    /// this account's: a fill without its own fill id cannot say which order
    /// it is, so it is claimed if any could be (a duplicate push, never a
    /// withheld one).
    pub fn claims_any_native_of(&self, symbol: &str, order_id: &str) -> bool {
        let now = Instant::now();
        self.fill_native_ids
            .iter()
            .filter(|((s, o, _), (_, expiry))| s == symbol && o == order_id && now < *expiry)
            .map(|(_, (native, _))| native)
            .chain(
                self.native_ids
                    .get(&(symbol.to_owned(), order_id.to_owned()))
                    .map(|(native, _)| native),
            )
            .any(|native| self.placed_at(symbol, native, now))
    }

    /// A venue order an algo's trigger created is this account's when the algo
    /// is, by its id or its client id. Never the reverse: a child cannot make
    /// its algo ours.
    pub fn link(&mut self, symbol: &str, order_id: &str, triggered_by: &[String]) {
        self.link_at(symbol, order_id, triggered_by, Instant::now());
    }

    pub fn link_at(&mut self, symbol: &str, order_id: &str, triggered_by: &[String], now: Instant) {
        let joint = |algo: &String| {
            self.joint
                .get(&(symbol.to_owned(), algo.clone()))
                .is_some_and(|(_, expiry)| now < *expiry)
        };
        let provisional = |algo: &String| {
            self.provisional
                .iter()
                .any(|((s, native), (aliases, expiry))| {
                    s == symbol && now < *expiry && (native == algo || aliases.contains(algo))
                })
        };
        // A joint (or unsettled) algo's child may be another login's leg.
        if triggered_by
            .iter()
            .filter(|algo| !algo.is_empty())
            .any(|algo| joint(algo) || provisional(algo))
        {
            return;
        }
        if triggered_by
            .iter()
            .any(|algo| self.placed_at(symbol, algo, now))
        {
            self.record_at(symbol, order_id, now);
        }
    }

    /// Note the leg rows (`{native}_{leg}`) of this account's own orders. A
    /// leg new since the last look was added by this account only if its own
    /// command on that symbol was in flight (`in_flight`), which `settle`
    /// decides; otherwise another client added it and the order is joint.
    /// The first look at an order only sets what it carries.
    pub fn observe_legs(
        &mut self,
        orders: &[safe_cex_engine::model::Order],
        in_flight: impl Fn(&str) -> bool,
    ) {
        let now = Instant::now();
        let mut current: HashMap<Key, (Vec<String>, Vec<String>)> = HashMap::new();
        for order in orders {
            // By its native id, or the client id or parent a child also links
            // through (an OKX algo whose ack was lost keeps its receipt's id).
            let Some((native, leg)) = order.id.split_once('_') else {
                continue;
            };
            let aliases = [order.client_order_id.as_deref(), order.parent_id.as_deref()];
            let ours = std::iter::once(Some(native))
                .chain(aliases)
                .flatten()
                .any(|identity| self.placed_at(&order.symbol, identity, now));
            if ours {
                let (legs, known) = current
                    .entry((order.symbol.clone(), native.to_owned()))
                    .or_default();
                legs.push(leg.to_owned());
                for alias in aliases.into_iter().flatten() {
                    if !alias.is_empty() && alias != native && !known.iter().any(|k| k == alias) {
                        known.push(alias.to_owned());
                    }
                }
            }
        }
        for (key, (legs, aliases)) in current {
            let grew = self
                .leg_rows
                .get(&key)
                .filter(|(_, expiry)| now < *expiry)
                .is_some_and(|(seen, _)| legs.iter().any(|leg| !seen.contains(leg)));
            let own_join = grew
                && self
                    .own_joins
                    .remove(&key)
                    .is_some_and(|expiry| now < expiry);
            if grew && !own_join {
                if in_flight(&key.0) {
                    insert_bounded(&mut self.provisional, key.clone(), aliases, now);
                } else {
                    self.mark_joint(&key, &aliases, now);
                }
            }
            insert_bounded(&mut self.leg_rows, key, legs, now);
        }
    }

    fn mark_joint(&mut self, (symbol, native): &Key, aliases: &[String], now: Instant) {
        for identity in std::iter::once(native).chain(aliases) {
            insert_bounded(&mut self.joint, (symbol.clone(), identity.clone()), (), now);
        }
    }

    /// This account's own command on `symbols` ended. An order that gained a
    /// leg while it was in flight is this account's own join only if the
    /// command completed naming it (`result`); a failed or unnamed one is joint.
    /// A placement that completes naming an own order it could join (`joinable`,
    /// its `joinable` at dispatch) joined it; if that new leg has not been seen
    /// yet, it is expected (`OWN_JOIN_WINDOW`).
    pub fn settle(&mut self, symbols: &[String], result: Option<&str>, joinable: &[String]) {
        let named = result.map(result_ids).unwrap_or_default();
        let settled: Vec<Key> = self
            .provisional
            .keys()
            .filter(|(symbol, _)| symbols.contains(symbol))
            .cloned()
            .collect();
        let now = Instant::now();
        for key in settled {
            let Some((aliases, _)) = self.provisional.remove(&key) else {
                continue;
            };
            if !named.contains(&key.1) {
                self.mark_joint(&key, &aliases, now);
            }
        }
        // A placement completes with an existing id only when it joined it.
        let [symbol] = symbols else {
            return;
        };
        for id in named.into_iter().filter(|id| joinable.contains(id)) {
            let key = (symbol.clone(), id);
            if !self.joint.contains_key(&key) {
                if self.own_joins.len() >= CAPACITY {
                    self.own_joins.retain(|_, expiry| now < *expiry);
                }
                if self.own_joins.len() < CAPACITY {
                    self.own_joins.insert(key, now + OWN_JOIN_WINDOW);
                }
            }
        }
    }

    /// The own orders on `symbols` whose leg rows are tracked: what a placement
    /// dispatched now could join.
    pub fn joinable(&self, symbols: &[String]) -> Vec<String> {
        let now = Instant::now();
        self.leg_rows
            .iter()
            .filter(|((symbol, _), (_, expiry))| symbols.contains(symbol) && now < *expiry)
            .map(|((_, native), _)| native.clone())
            .collect()
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

/// Whether `id` is a held row, or the native order a held row is a leg of
/// (OKX stores an algo's legs as `{algoId}_sl`/`_tp`). Matching a leg by its
/// prefix only ever withholds a claim, the fail-open side.
fn names_held(held: &[String], id: &str) -> bool {
    held.iter()
        .any(|row| row == id || row.strip_prefix(id).is_some_and(|leg| leg.starts_with('_')))
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

    /// Review A (5f75821a55): a batch that sent one client id on two symbols
    /// cannot say which leg a proven element is, so it records nothing for it.
    #[test]
    fn a_leg_whose_client_id_names_two_symbols_records_nothing() {
        let legs: Vec<Leg> = vec![(BTC.into(), None), ("ETHUSDT".into(), None)];
        let sent: Vec<Key> = vec![
            (BTC.into(), "same".into()),
            ("ETHUSDT".into(), "same".into()),
        ];
        let mut placements = OwnPlacements::default();
        placements.record_proven_legs(&sent, &legs, &[placed_leg(0, Some("same"), &["x-1"])]);
        assert!(!placements.placed(BTC, "x-1") && !placements.placed("ETHUSDT", "x-1"));
    }

    /// Review B (e6e681d490): a completed batch over several symbols cannot say
    /// which id is whose, so it records none; a single-symbol one records all.
    #[test]
    fn a_completion_records_ids_only_under_its_one_symbol() {
        let mut placements = OwnPlacements::default();
        let legs: Vec<Leg> = vec![(BTC.into(), None), ("ETHUSDT".into(), None)];
        placements.record_completion(&[BTC.into(), "ETHUSDT".into()], &legs, r#"["1","2"]"#, &[]);
        assert!(!placements.placed(BTC, "1") && !placements.placed("ETHUSDT", "2"));
        placements.record_completion(&[BTC.into()], &[(BTC.into(), None)], r#"["3"]"#, &[]);
        assert!(placements.placed(BTC, "3") && !placements.placed("ETHUSDT", "3"));
    }

    /// AGT-333: OKX completes a full-position join with the HELD algo's id.
    /// Another login's held algo (stored as its `_sl` leg) is never claimed by
    /// the login that joined it; this login's own held algo still is, and a
    /// fresh id beside it records as before. Synthetic rows.
    #[test]
    fn a_completion_never_claims_an_order_it_joined() {
        use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
        let row = |id: &str, symbol: &str| Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: symbol.into(),
            kind: OrderType::StopLoss,
            side: OrderSide::Sell,
            price: 36000.0,
            amount: 0.0,
            filled: 0.0,
            remaining: 0.0,
            reduce_only: true,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "88");
        let orders = [
            row("77_sl", BTC),
            row("88_sl", BTC),
            row("99_sl", "ETHUSDT"),
        ];
        let held = placements.held_rows(&[BTC.into()], &orders);
        assert_eq!(held, ["77_sl"], "only another login's rows on this symbol");
        let one = [(BTC.to_owned(), None)];
        placements.record_completion(&[BTC.into()], &one, r#"["77"]"#, &held);
        assert!(
            !placements.placed(BTC, "77"),
            "a joined algo is not placed here"
        );
        placements.record_completion(&[BTC.into()], &one, r#"["7","770"]"#, &held);
        assert!(placements.placed(BTC, "7") && placements.placed(BTC, "770"));
    }

    fn leg(id: &str) -> safe_cex_engine::model::Order {
        use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
        Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: BTC.into(),
            kind: OrderType::StopLoss,
            side: OrderSide::Sell,
            price: 36000.0,
            amount: 0.0,
            filled: 0.0,
            remaining: 0.0,
            reduce_only: true,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }

    /// AGT-333 review: an own algo that gains a leg no own command added is
    /// joint, and its children link to nobody. The first look only sets what
    /// it carries; a leg deleted and later re-added by another client is new
    /// again. Synthetic rows.
    #[test]
    fn an_own_algo_that_gains_a_foreign_leg_links_nobody() {
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "77");
        placements.record(BTC, "88");
        placements.observe_legs(&[leg("77_sl"), leg("88_sl"), leg("88_tp")], |_| false);
        placements.link(BTC, "child-88", &["88".into()]);
        assert!(
            placements.placed(BTC, "child-88"),
            "a first look is no growth"
        );
        placements.observe_legs(&[leg("77_sl"), leg("77_tp"), leg("88_sl")], |_| false);
        placements.observe_legs(
            &[leg("77_sl"), leg("77_tp"), leg("88_sl"), leg("88_tp")],
            |_| false,
        );
        for algo in ["77", "88"] {
            let child = format!("child-{algo}-2");
            placements.link(BTC, &child, &[algo.into(), "its-client-id".into()]);
            assert!(!placements.placed(BTC, &child), "{algo}: joint");
        }
        assert!(
            placements.placed(BTC, "77"),
            "the algo itself stays recorded"
        );
    }

    /// Review A (8e01a745a1): an algo known here only by its client id (the
    /// ack was lost) is still tracked through its rows' client id, under its
    /// native id, so a foreign leg makes it joint for both identities.
    #[test]
    fn an_algo_known_only_by_its_client_id_still_becomes_joint() {
        let with_client = |id: &str| {
            let mut row = leg(id);
            row.client_order_id = Some("algo-client".into());
            row
        };
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "algo-client");
        placements.observe_legs(&[with_client("77_sl")], |_| false);
        placements.observe_legs(&[with_client("77_sl"), with_client("77_tp")], |_| false);
        placements.link(BTC, "child", &["77".into(), "algo-client".into()]);
        assert!(!placements.placed(BTC, "child"));
    }

    /// CodeRabbit (7fe0f26557): a joint algo's child that names only the
    /// algo's client id links nobody, and so does a provisional one's.
    #[test]
    fn a_joint_algos_child_named_only_by_client_id_links_nobody() {
        let with_client = |id: &str, client: &str| {
            let mut row = leg(id);
            row.client_order_id = Some(client.into());
            row
        };
        let mut placements = OwnPlacements::default();
        for (native, client) in [("77", "c77"), ("88", "c88")] {
            placements.record(BTC, native);
            placements.record(BTC, client);
        }
        placements.observe_legs(
            &[with_client("77_sl", "c77"), with_client("88_sl", "c88")],
            |_| false,
        );
        placements.observe_legs(
            &[with_client("77_sl", "c77"), with_client("77_tp", "c77")],
            |_| false,
        );
        placements.observe_legs(
            &[
                with_client("77_sl", "c77"),
                with_client("77_tp", "c77"),
                with_client("88_sl", "c88"),
                with_client("88_tp", "c88"),
            ],
            |_| true,
        );
        placements.link(BTC, "child-77", &[String::new(), "c77".into()]);
        placements.link(BTC, "child-88", &[String::new(), "c88".into()]);
        assert!(!placements.placed(BTC, "child-77"), "joint");
        assert!(!placements.placed(BTC, "child-88"), "provisional");
        placements.settle(&[BTC.into()], None, &[]);
        placements.link(BTC, "child-88-2", &[String::new(), "c88".into()]);
        assert!(
            !placements.placed(BTC, "child-88-2"),
            "settled joint by alias too"
        );
    }

    /// CodeRabbit (7fe0f26557): recording a joint order again keeps it joint
    /// for as long as it stays recorded.
    #[test]
    fn a_joint_order_recorded_again_stays_joint() {
        let start = Instant::now();
        let mut placements = OwnPlacements::default();
        placements.record_at(BTC, "77", start);
        placements.observe_legs(&[leg("77_sl")], |_| false);
        placements.observe_legs(&[leg("77_sl"), leg("77_tp")], |_| false);
        let refreshed = start + RETENTION - Duration::from_secs(1);
        placements.record_at(BTC, "77", refreshed);
        let later = start + RETENTION + Duration::from_secs(60);
        assert!(placements.placed_at(BTC, "77", later));
        placements.link_at(BTC, "child", &["77".into()], later);
        assert!(!placements.placed_at(BTC, "child", later));
    }

    /// CodeRabbit (7fe0f26557): an own join acknowledged before its new leg row
    /// is seen expects that leg; a placement that could not join (not joinable
    /// at dispatch) expects nothing, so a later foreign leg is still joint.
    #[test]
    fn an_own_join_acknowledged_before_its_row_keeps_the_order_claimable() {
        let symbols = [BTC.to_owned()];
        let mut placements = OwnPlacements::default();
        placements.record(BTC, "77");
        placements.record(BTC, "99");
        placements.observe_legs(&[leg("77_sl"), leg("99_sl")], |_| false);
        placements.settle(&symbols, Some(r#"["77"]"#), &["77".into()]);
        placements.settle(&symbols, Some(r#"["99"]"#), &[]);
        placements.observe_legs(
            &[leg("77_sl"), leg("77_tp"), leg("99_sl"), leg("99_tp")],
            |_| false,
        );
        placements.link(BTC, "c77", &["77".into()]);
        placements.link(BTC, "c99", &["99".into()]);
        assert!(placements.placed(BTC, "c77"), "its own join");
        assert!(!placements.placed(BTC, "c99"), "not joinable: joint");
    }

    /// A leg that appears while this account's own command on the symbol is in
    /// flight is its own only if that command completes naming the algo.
    #[test]
    fn a_leg_added_during_an_own_command_is_settled_by_it() {
        let symbols = [BTC.to_owned()];
        let mut placements = OwnPlacements::default();
        for algo in ["1", "2", "3"] {
            placements.record(BTC, algo);
        }
        placements.observe_legs(&[leg("1_sl"), leg("2_sl"), leg("3_sl")], |_| false);
        let joined = [
            leg("1_sl"),
            leg("1_tp"),
            leg("2_sl"),
            leg("2_tp"),
            leg("3_sl"),
            leg("3_tp"),
        ];
        placements.observe_legs(&joined, |symbol| symbol == BTC);
        placements.link(BTC, "early", &["1".into()]);
        assert!(!placements.placed(BTC, "early"), "unsettled: nobody");
        placements.settle(&symbols, Some(r#"["1"]"#), &[]);
        placements.settle(&symbols, None, &[]);
        placements.link(BTC, "c1", &["1".into()]);
        placements.link(BTC, "c2", &["2".into()]);
        assert!(
            placements.placed(BTC, "c1"),
            "named by its completion: own join"
        );
        assert!(!placements.placed(BTC, "c2"), "not named: joint");
        placements.observe_legs(&joined, |_| false);
        placements.link(BTC, "c1-again", &["1".into()]);
        assert!(placements.placed(BTC, "c1-again"), "no growth since");
    }

    /// Review B (e6e681d490): Coinbase completes a repeated client id with the
    /// existing order's id, which may be another login's, so no caller-id
    /// placement claims there. Generated ids still do. Synthetic.
    #[test]
    fn coinbase_caller_id_placements_never_claim() {
        let mut placements = OwnPlacements::default();
        placements.set_idempotent_client_ids();
        let desk: Vec<Leg> = vec![(BTC.into(), Some("desk".into()))];
        placements.record_completion(&[BTC.into()], &desk, r#"["their-order"]"#, &[]);
        placements.record_proven_legs(
            &[(BTC.into(), "desk".into())],
            &desk,
            &[placed_leg(0, Some("desk"), &["their-order"])],
        );
        assert!(!placements.placed(BTC, "their-order"));
        placements.record_completion(&[BTC.into()], &[(BTC.into(), None)], r#"["mine"]"#, &[]);
        assert!(placements.placed(BTC, "mine"));
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

    /// Review A and B (b515532bfa): a caller-chosen client id never claims,
    /// whatever its completion says; only the ack's native id does, for this
    /// command's own leg. An ack without a native id records nothing.
    #[test]
    fn only_an_acked_native_id_claims_a_caller_id_placement() {
        let desk: Vec<Leg> = vec![(BTC.into(), Some("desk".into()))];
        let mut placements = OwnPlacements::default();
        placements.record_completion(&[BTC.into()], &desk, r#"["desk"]"#, &[]);
        assert!(!placements.placed(BTC, "desk"), "a client-keyed completion");
        placements.record_placement_native_id(&desk, BTC, "other", "n-other");
        placements.record_placement_native_id(&desk, "ETHUSDT", "desk", "n-eth");
        assert!(!placements.placed(BTC, "n-other") && !placements.placed("ETHUSDT", "n-eth"));
        placements.record_placement_native_id(&desk, BTC, "desk", "n-1");
        assert!(placements.placed(BTC, "n-1"));
        assert!(!placements.placed(BTC, "desk"), "still never the client id");
    }
}
