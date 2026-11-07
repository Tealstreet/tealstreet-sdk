//! Per-symbol metadata ownership. Dispatch, leases and retry permission remain
//! with the engine Plan/group owner; this module owns publication only.
use crate::fence::{Epoch, Stamp};
use crate::store::MarketSetting;
use crate::{CommandId, ErrorKind, Outcome, VenueError};
use std::cmp::Ordering;
use std::collections::HashMap;

const MAX_ATTEMPTS: usize = 256;
const MAX_SYMBOLS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Token(Stamp);

/// Observation of a background settings read, before any native response rows.
#[derive(Clone, Debug, Default)]
pub struct Observation(Option<Stamp>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    IndependentRails,
    CompleteSetting,
}

#[derive(Clone, Copy, Debug)]
pub struct Rails {
    buy: f64,
    sell: f64,
}
impl Rails {
    pub fn new(buy: f64, sell: f64) -> Result<Self, VenueError> {
        if valid(buy) && valid(sell) {
            Ok(Self { buy, sell })
        } else {
            Err(invalid("invalid metadata leverage rails"))
        }
    }
}

/// Complete validated settings, required for any Store publication.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot(MarketSetting);
impl Snapshot {
    pub fn new(setting: MarketSetting) -> Result<Self, VenueError> {
        if valid(setting.leverage)
            && setting.buy_leverage.is_some_and(valid)
            && setting.sell_leverage.is_some_and(valid)
            && setting.margin_type.is_some()
        {
            Ok(Self(setting))
        } else {
            Err(invalid("incomplete metadata settings proposal"))
        }
    }
}

/// A native account row may know one hedge rail before its neighbour. It must
/// still validate every supplied value; it cannot prove complete metadata.
#[derive(Clone, Copy, Debug)]
pub struct ObservedSetting(pub(crate) MarketSetting);
impl ObservedSetting {
    pub fn new(setting: MarketSetting) -> Result<Self, VenueError> {
        if valid(setting.leverage)
            && setting.buy_leverage.is_none_or(valid)
            && setting.sell_leverage.is_none_or(valid)
            && (setting.buy_leverage.is_some() || setting.sell_leverage.is_some())
            && setting.margin_type.is_some()
        {
            Ok(Self(setting))
        } else {
            Err(invalid("invalid observed settings proposal"))
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Proposal {
    rails: Rails,
    setting: Option<Snapshot>,
}
impl Proposal {
    /// A superseded multi-leg read may answer validated rails without parsing
    /// a discarded later leg, but can never publish this partial proposal.
    pub fn rails(rails: Rails) -> Self {
        Self {
            rails,
            setting: None,
        }
    }
    pub fn complete(snapshot: Snapshot) -> Self {
        let setting = snapshot.0;
        Self {
            rails: Rails {
                buy: setting.buy_leverage.expect("validated buy rail"),
                sell: setting.sell_leverage.expect("validated sell rail"),
            },
            setting: Some(snapshot),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Value {
    pub buy: f64,
    pub sell: f64,
    pub margin: Option<crate::model::MarginType>,
}

/// Publication already happened atomically inside Store. The flag permits an
/// adapter's infallible native-cache/position-fence update before notification.
pub struct Resolution {
    pub published: bool,
    result: Result<Value, VenueError>,
}
impl Resolution {
    pub fn result(&self) -> &Result<Value, VenueError> {
        &self.result
    }
    pub fn into_result(self) -> Result<Value, VenueError> {
        self.result
    }
}

struct Attempt {
    symbol: String,
    revision: Stamp,
    global: Option<Stamp>,
    publish: bool,
    fallback: Fallback,
}

#[derive(Default)]
pub(crate) struct State {
    epoch: Epoch,
    global: Option<Stamp>,
    account_fence: Option<Stamp>,
    revisions: HashMap<String, Stamp>,
    observation_floors: HashMap<String, Stamp>,
    current: HashMap<String, Token>,
    attempts: HashMap<Token, Attempt>,
    writes: Vec<(CommandId, Option<String>)>,
    #[cfg(feature = "http-plans")]
    work: work::Index<crate::engine::ExecutionHandle>,
}
impl State {
    fn tick(&mut self) -> Option<Stamp> {
        self.epoch.advance();
        self.epoch.stamp()
    }
    fn live(&self, stamp: &Stamp) -> bool {
        self.epoch.compare(stamp, stamp) == Some(Ordering::Equal)
    }
    pub(crate) fn observation(&mut self) -> Observation {
        Observation(self.tick())
    }
    pub(crate) fn observe(&mut self, symbol: &str, observation: &Observation) -> bool {
        let Some(stamp) = observation.0.as_ref() else {
            return false;
        };
        if !self.live(stamp)
            || self
                .global
                .as_ref()
                .is_some_and(|f| self.epoch.compare(f, stamp) != Some(Ordering::Less) && f != stamp)
            || self
                .observation_floors
                .get(symbol)
                .is_some_and(|f| self.epoch.compare(f, stamp) != Some(Ordering::Less))
            || (!self.revisions.contains_key(symbol) && self.revisions.len() >= MAX_SYMBOLS)
        {
            return false;
        }
        // One issuer orders observations by issue, mutations by commit; issuing
        // an unrelated read alone must never revoke a symbol's publication.
        let Some(version) = self.tick() else {
            return false;
        };
        self.revisions.insert(symbol.to_owned(), version);
        self.observation_floors
            .insert(symbol.to_owned(), stamp.clone());
        true
    }
    pub(crate) fn changed(&mut self, symbol: Option<&str>) {
        let Some(next) = self.tick() else {
            return;
        };
        if let Some(symbol) = symbol {
            if !self.revisions.contains_key(symbol) && self.revisions.len() >= MAX_SYMBOLS {
                self.global = Some(next);
            } else {
                self.revisions.insert(symbol.to_owned(), next.clone());
                self.observation_floors.insert(symbol.to_owned(), next);
            }
        } else {
            self.global = Some(next);
        }
    }
    pub(crate) fn invalidate(&mut self, symbol: Option<&str>) {
        self.changed(symbol);
        self.account_fence = self.epoch.stamp();
    }
    pub(crate) fn revision(&mut self) -> Observation {
        Observation(self.account_fence.clone().or_else(|| self.epoch.stamp()))
    }
    pub(crate) fn revision_current(&self, observation: &Observation) -> bool {
        let Some(stamp) = observation.0.as_ref() else {
            return false;
        };
        self.live(stamp)
            && self
                .account_fence
                .as_ref()
                .is_none_or(|fence| fence == stamp)
    }
    pub(crate) fn write_started(
        &mut self,
        command: CommandId,
        symbol: Option<&str>,
    ) -> Result<(), VenueError> {
        if self.writes.len() >= MAX_ATTEMPTS || self.writes.iter().any(|(id, _)| *id == command) {
            return Err(invalid("metadata write capacity or duplicate owner"));
        }
        self.invalidate(symbol);
        self.writes.push((command, symbol.map(str::to_owned)));
        Ok(())
    }
    pub(crate) fn write_finished(&mut self, command: CommandId) {
        if let Some(index) = self.writes.iter().position(|(id, _)| *id == command) {
            let (_, symbol) = self.writes.swap_remove(index);
            self.invalidate(symbol.as_deref());
        }
    }
    pub(crate) fn write_pending(&self, symbol: &str) -> bool {
        self.writes
            .iter()
            .any(|(_, scope)| scope.as_deref().is_none_or(|s| s == symbol))
    }
    pub(crate) fn begin(
        &mut self,
        symbol: &str,
        publish: bool,
        fallback: Fallback,
    ) -> Result<Token, VenueError> {
        let publish = publish && !self.write_pending(symbol);
        if self.epoch.stamp().is_none() {
            return Err(invalid("metadata token space exhausted"));
        }
        if self.attempts.len() >= MAX_ATTEMPTS
            || (!self.revisions.contains_key(symbol) && self.revisions.len() >= MAX_SYMBOLS)
        {
            return Err(invalid("metadata read capacity exceeded"));
        }
        let stamp = self
            .tick()
            .ok_or_else(|| invalid("metadata token space exhausted"))?;
        let revision = self
            .revisions
            .entry(symbol.to_owned())
            .or_insert_with(|| stamp.clone())
            .clone();
        let token = Token(stamp);
        self.current.insert(symbol.to_owned(), token.clone());
        self.attempts.insert(
            token.clone(),
            Attempt {
                symbol: symbol.to_owned(),
                revision,
                global: self.global.clone(),
                publish,
                fallback,
            },
        );
        Ok(token)
    }
    pub(crate) fn eligible(&self, token: &Token) -> bool {
        self.current(token) && self.attempts.get(token).is_some_and(|read| read.publish)
    }
    pub(crate) fn current(&self, token: &Token) -> bool {
        self.live(&token.0)
            && self.attempts.get(token).is_some_and(|read| {
                read.global == self.global
                    && self.revisions.get(&read.symbol) == Some(&read.revision)
                    && self.current.get(&read.symbol) == Some(token)
            })
    }
    pub(crate) fn symbol(&self, token: &Token) -> Option<&str> {
        self.attempts.get(token).map(|read| read.symbol.as_str())
    }
    pub(crate) fn complete(
        &mut self,
        token: &Token,
        result: Result<Proposal, VenueError>,
        cached: Option<MarketSetting>,
    ) -> Option<(Resolution, Option<MarketSetting>)> {
        let eligible = self.eligible(token);
        let read = self.attempts.remove(token)?;
        #[cfg(feature = "http-plans")]
        self.work.remove(token);
        if self.current.get(&read.symbol) == Some(token) {
            self.current.remove(&read.symbol);
        }
        let mut publication = None;
        let result = result.and_then(|proposal| {
            let setting = if eligible {
                let Some(snapshot) = proposal.setting else {
                    return Err(invalid("metadata publication requires complete settings"));
                };
                publication = Some(snapshot.0);
                Some(snapshot.0)
            } else if read.fallback == Fallback::CompleteSetting {
                cached
                    .and_then(|setting| Snapshot::new(setting).ok())
                    .or(proposal.setting)
                    .map(|snapshot| snapshot.0)
            } else {
                proposal.setting.map(|snapshot| snapshot.0)
            };
            if read.fallback == Fallback::CompleteSetting && setting.is_none() {
                return Err(invalid("complete metadata fallback requires settings"));
            }
            let (buy, sell) = if !eligible && read.fallback == Fallback::IndependentRails {
                (
                    cached
                        .and_then(|s| s.buy_leverage)
                        .filter(|v| valid(*v))
                        .unwrap_or(proposal.rails.buy),
                    cached
                        .and_then(|s| s.sell_leverage)
                        .filter(|v| valid(*v))
                        .unwrap_or(proposal.rails.sell),
                )
            } else if let Some(setting) = setting {
                (
                    setting.buy_leverage.expect("validated setting"),
                    setting.sell_leverage.expect("validated setting"),
                )
            } else {
                (proposal.rails.buy, proposal.rails.sell)
            };
            Ok(Value {
                buy,
                sell,
                margin: setting.and_then(|setting| setting.margin_type),
            })
        });
        if publication.is_some() {
            self.changed(Some(&read.symbol));
        }
        Some((
            Resolution {
                published: publication.is_some(),
                result,
            },
            publication,
        ))
    }
}
fn valid(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn invalid(message: &str) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::InvalidInput)
        .outcome(Outcome::NotSent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Store, model::MarginType};

    fn setting(buy: f64, sell: f64) -> MarketSetting {
        MarketSetting {
            leverage: buy,
            buy_leverage: Some(buy),
            sell_leverage: Some(sell),
            margin_type: Some(MarginType::Cross),
        }
    }
    fn start(
        store: &mut Store,
        symbol: &str,
        _command: u32,
        publish: bool,
        fallback: Fallback,
    ) -> Token {
        store.begin_metadata(symbol, publish, fallback).unwrap()
    }

    fn proposal(buy: f64, sell: f64) -> Proposal {
        Proposal::complete(Snapshot::new(setting(buy, sell)).unwrap())
    }

    #[test]
    fn foreign_store_tokens_are_distinct_and_cannot_finish_another_owner() {
        let mut one = Store::default();
        let mut two = Store::default();
        let a = start(&mut one, "BTC", 1, true, Fallback::CompleteSetting);
        let b = start(&mut two, "BTC", 2, true, Fallback::CompleteSetting);
        assert_ne!(a, b, "attempt tokens must bind the Store issuer");
        assert!(two.finish_metadata(&a, Ok(proposal(7.0, 9.0))).is_none());
        assert!(two.metadata_eligible(&b));
    }

    #[test]
    fn foreign_store_observation_cannot_publish_settings() {
        let mut one = Store::default();
        let mut two = Store::default();
        let foreign = one.metadata_observation();
        assert!(!two.observe_setting(
            "BTC",
            &foreign,
            ObservedSetting::new(setting(99.0, 99.0)).unwrap()
        ));
        assert!(two.setting("BTC").is_none());
    }

    #[test]
    fn account_revision_ignores_read_issuance_but_not_settings_invalidation() {
        let mut store = Store::default();
        let revision = store.metadata_revision();
        assert!(store.metadata_revision_current(&revision));
        let _background = store.metadata_observation();
        let _read = start(&mut store, "ETH", 2, true, Fallback::CompleteSetting);
        assert!(store.metadata_revision_current(&revision));
        store.invalidate_metadata(Some("BTC"));
        assert!(!store.metadata_revision_current(&revision));
        let fresh = store.metadata_revision();
        let _another_read = store.metadata_observation();
        assert!(store.metadata_revision_current(&fresh));
    }

    #[test]
    fn default_foreign_and_exhausted_account_revisions_never_authorize() {
        let mut one = Store::default();
        let mut two = Store::default();
        let foreign = one.metadata_revision();
        let own = two.metadata_revision();
        assert!(!two.metadata_revision_current(&foreign));
        assert!(!two.metadata_revision_current(&Observation::default()));
        assert!(!two.observe_setting(
            "BTC",
            &Observation::default(),
            ObservedSetting::new(setting(7.0, 9.0)).unwrap()
        ));
        let mut state = State::default();
        let current = state.revision();
        state.epoch.exhaust_for_test();
        assert!(!state.revision_current(&current));
        let exhausted = state.revision();
        assert!(!state.revision_current(&exhausted));
        assert!(!state.observe("BTC", &current));
        assert!(two.metadata_revision_current(&own));
    }

    #[test]
    fn one_token_publishes_atomically_only_once() {
        let mut store = Store::default();
        let token = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        let done = store
            .finish_metadata(&token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert!(done.published);
        assert_eq!(store.setting("BTC"), Some(setting(7.0, 9.0)));
        assert!(
            store
                .finish_metadata(&token, Ok(proposal(99.0, 99.0)))
                .is_none()
        );
        assert_eq!(store.setting("BTC"), Some(setting(7.0, 9.0)));
    }
    #[test]
    fn failed_attempt_retains_settings_and_retires_its_token() {
        let mut store = Store::default();
        store.update_setting("BTC", |s| *s = setting(20.0, 20.0));
        let token = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        let error = VenueError::new("timeout").kind(ErrorKind::Timeout);
        let done = store.finish_metadata(&token, Err(error.clone())).unwrap();
        assert!(!done.published);
        assert_eq!(done.result().as_ref().unwrap_err(), &error);
        assert_eq!(store.setting("BTC"), Some(setting(20.0, 20.0)));
        let next = start(&mut store, "BTC", 3, true, Fallback::CompleteSetting);
        assert_ne!(next, token);
        assert!(
            store
                .finish_metadata(&token, Ok(proposal(99.0, 99.0)))
                .is_none()
        );
        assert!(store.metadata_eligible(&next));
    }
    #[test]
    fn writes_fence_old_read_and_new_waiters_do_not_join_it() {
        let mut store = Store::default();
        let old = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        store.invalidate_metadata(Some("BTC"));
        store.update_setting("BTC", |s| *s = setting(30.0, 40.0));
        let next = start(&mut store, "BTC", 2, true, Fallback::IndependentRails);
        let stale = store.finish_metadata(&old, Ok(proposal(7.0, 9.0))).unwrap();
        assert!(!stale.published);
        assert_eq!(stale.result().as_ref().unwrap().buy, 30.0);
        assert!(store.metadata_eligible(&next));
        assert!(
            store
                .finish_metadata(&next, Ok(proposal(50.0, 60.0)))
                .unwrap()
                .published
        );
    }
    #[test]
    fn pending_write_publication_restriction_is_sticky() {
        let mut store = Store::default();
        let restricted = start(&mut store, "BTC", 1, false, Fallback::IndependentRails);
        assert!(!store.metadata_eligible(&restricted));
        let next = start(&mut store, "BTC", 2, true, Fallback::IndependentRails);
        let done = store
            .finish_metadata(
                &restricted,
                Ok(Proposal::rails(Rails::new(7.0, 9.0).unwrap())),
            )
            .unwrap();
        assert!(!done.published);
        assert!(store.setting("BTC").is_none());
        assert!(store.metadata_eligible(&next));
    }
    #[test]
    fn symbol_changes_do_not_revoke_an_unrelated_read() {
        let mut store = Store::default();
        let btc = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        let eth = start(&mut store, "ETH", 2, true, Fallback::CompleteSetting);
        store.invalidate_metadata(Some("BTC"));
        assert!(!store.metadata_eligible(&btc));
        assert!(store.metadata_eligible(&eth));
        store.invalidate_metadata(None);
        assert!(!store.metadata_eligible(&eth));
    }
    #[test]
    fn a_partial_proposal_never_publishes() {
        let mut store = Store::default();
        let token = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        let done = store
            .finish_metadata(&token, Ok(Proposal::rails(Rails::new(7.0, 9.0).unwrap())))
            .unwrap();
        assert!(done.result().is_err());
        assert!(!done.published);
        assert!(store.setting("BTC").is_none());
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(Rails::new(bad, 1.0).is_err());
            assert!(Snapshot::new(setting(1.0, bad)).is_err());
        }
        assert!(
            Snapshot::new(MarketSetting {
                margin_type: None,
                ..setting(1.0, 1.0)
            })
            .is_err()
        );
    }
    #[test]
    fn fallback_policies_preserve_independent_rails_and_complete_triplets() {
        for fallback in [Fallback::IndependentRails, Fallback::CompleteSetting] {
            let mut store = Store::default();
            store.update_setting("BTC", |s| {
                *s = MarketSetting {
                    sell_leverage: None,
                    ..setting(30.0, 40.0)
                }
            });
            let token = start(&mut store, "BTC", 1, false, fallback);
            let done = store
                .finish_metadata(&token, Ok(proposal(7.0, 9.0)))
                .unwrap();
            let value = done.result().as_ref().unwrap();
            assert_eq!(
                (value.buy, value.sell),
                if fallback == Fallback::IndependentRails {
                    (30.0, 9.0)
                } else {
                    (7.0, 9.0)
                }
            );
            assert!(!done.published);
        }
    }
    #[test]
    fn old_background_proposals_cannot_overwrite_new_metadata_or_writes() {
        let mut store = Store::default();
        let old = store.metadata_observation();
        let token = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        store
            .finish_metadata(&token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert!(!store.observe_setting(
            "BTC",
            &old,
            ObservedSetting::new(setting(2.0, 2.0)).unwrap()
        ));
        let fresh = store.metadata_observation();
        assert!(store.observe_setting(
            "ETH",
            &fresh,
            ObservedSetting::new(setting(3.0, 3.0)).unwrap()
        ));
        store.invalidate_metadata(Some("BTC"));
        assert!(!store.observe_setting(
            "BTC",
            &fresh,
            ObservedSetting::new(setting(4.0, 4.0)).unwrap()
        ));
        assert_eq!(store.setting("BTC"), Some(setting(7.0, 9.0)));
    }
    #[test]
    fn attempt_capacity_refuses_without_dropping_existing_tokens() {
        let mut store = Store::default();
        let first = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        for id in 1..MAX_ATTEMPTS {
            store
                .begin_metadata(&format!("market-{id}"), true, Fallback::CompleteSetting)
                .unwrap();
        }
        assert!(
            store
                .begin_metadata("overflow", true, Fallback::CompleteSetting)
                .is_err()
        );
        assert!(store.metadata_eligible(&first));
        assert!(
            store
                .finish_metadata(&first, Ok(proposal(7.0, 9.0)))
                .unwrap()
                .published
        );
        assert!(
            store
                .begin_metadata("overflow", true, Fallback::CompleteSetting)
                .is_ok()
        );
    }
    #[test]
    fn token_exhaustion_fails_closed_without_reusing_identity() {
        let mut state = State::default();
        let token = state.begin("BTC", true, Fallback::CompleteSetting).unwrap();
        state.epoch.exhaust_for_test();
        state.changed(Some("BTC"));
        assert!(!state.eligible(&token));
        assert!(state.begin("BTC", true, Fallback::CompleteSetting).is_err());
    }
    #[test]
    fn background_issue_order_is_stable_in_either_completion_order() {
        for newer_first in [false, true] {
            let mut store = Store::default();
            let old = store.metadata_observation();
            let new = store.metadata_observation();
            if newer_first {
                assert!(store.observe_setting(
                    "BTC",
                    &new,
                    ObservedSetting::new(setting(20.0, 20.0)).unwrap()
                ));
                assert!(!store.observe_setting(
                    "BTC",
                    &old,
                    ObservedSetting::new(setting(10.0, 10.0)).unwrap()
                ));
            } else {
                assert!(store.observe_setting(
                    "BTC",
                    &old,
                    ObservedSetting::new(setting(10.0, 10.0)).unwrap()
                ));
                assert!(store.observe_setting(
                    "BTC",
                    &new,
                    ObservedSetting::new(setting(20.0, 20.0)).unwrap()
                ));
            }
            assert_eq!(store.setting("BTC"), Some(setting(20.0, 20.0)));
            assert!(!store.observe_setting(
                "BTC",
                &new,
                ObservedSetting::new(setting(99.0, 99.0)).unwrap()
            ));
        }
    }
    #[test]
    fn a_valid_background_completion_supersedes_pending_metadata_but_not_published_metadata() {
        let mut store = Store::default();
        let background = store.metadata_observation();
        let token = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        assert!(store.observe_setting(
            "BTC",
            &background,
            ObservedSetting::new(setting(20.0, 20.0)).unwrap()
        ));
        let result = store
            .finish_metadata(&token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert!(!result.published);
        assert_eq!(result.result().as_ref().unwrap().buy, 20.0);
        let old = store.metadata_observation();
        let fresh = start(&mut store, "BTC", 2, true, Fallback::IndependentRails);
        assert!(
            store
                .finish_metadata(&fresh, Ok(proposal(30.0, 40.0)))
                .unwrap()
                .published
        );
        assert!(!store.observe_setting(
            "BTC",
            &old,
            ObservedSetting::new(setting(99.0, 99.0)).unwrap()
        ));
    }
    #[test]
    fn observed_partial_settings_validate_each_supplied_field() {
        let partial = MarketSetting {
            sell_leverage: None,
            ..setting(7.0, 9.0)
        };
        assert!(ObservedSetting::new(partial).is_ok());
        assert!(Snapshot::new(partial).is_err());
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                ObservedSetting::new(MarketSetting {
                    sell_leverage: Some(bad),
                    ..partial
                })
                .is_err()
            );
            assert!(
                ObservedSetting::new(MarketSetting {
                    leverage: bad,
                    ..partial
                })
                .is_err()
            );
        }
        assert!(
            ObservedSetting::new(MarketSetting {
                margin_type: None,
                ..partial
            })
            .is_err()
        );
    }
    #[test]
    fn write_reservations_are_scoped_and_read_restrictions_do_not_upgrade_after_refusal() {
        let mut store = Store::default();
        store.metadata_write_started(80, Some("BTC")).unwrap();
        assert!(store.metadata_write_pending("BTC"));
        assert!(!store.metadata_write_pending("ETH"));
        let blocked = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        let eth = start(&mut store, "ETH", 2, true, Fallback::IndependentRails);
        assert!(!store.metadata_eligible(&blocked));
        assert!(store.metadata_eligible(&eth));
        store.metadata_write_finished(80);
        assert!(!store.metadata_write_pending("BTC"));
        assert!(!store.metadata_eligible(&blocked));
        let next = start(&mut store, "BTC", 3, true, Fallback::IndependentRails);
        assert!(store.metadata_eligible(&next));
        let old = store
            .finish_metadata(
                &blocked,
                Err(VenueError::new("canceled").kind(ErrorKind::Timeout)),
            )
            .unwrap();
        assert_eq!(
            old.result().as_ref().unwrap_err().kind,
            Some(ErrorKind::Timeout)
        );
        assert!(store.metadata_eligible(&next));
        store.metadata_write_started(81, None).unwrap();
        assert!(store.metadata_write_pending("ETH"));
        assert!(!store.metadata_eligible(&eth));
        store.metadata_write_finished(81);
    }
    #[test]
    fn attempt_capacity_recovers_after_terminal_cleanup_and_old_tokens_stay_inert() {
        let mut store = Store::default();
        let mut tokens = Vec::new();
        for i in 0..MAX_ATTEMPTS {
            tokens.push(start(
                &mut store,
                &format!("S{i}"),
                i as u32,
                true,
                Fallback::IndependentRails,
            ));
        }
        assert!(
            store
                .begin_metadata("overflow", true, Fallback::IndependentRails)
                .is_err()
        );
        let first = tokens[0].clone();
        store
            .finish_metadata(
                &first,
                Err(VenueError::new("expired").kind(ErrorKind::Timeout)),
            )
            .unwrap();
        let next = start(&mut store, "S0", 999, true, Fallback::IndependentRails);
        assert_ne!(next, first);
        assert!(
            store
                .finish_metadata(&first, Ok(proposal(99.0, 99.0)))
                .is_none()
        );
        assert!(store.metadata_eligible(&next));
    }
    #[test]
    fn write_capacity_and_duplicate_refusals_preserve_existing_fences() {
        let mut store = Store::default();
        store.metadata_write_started(1, Some("BTC")).unwrap();
        let before_duplicate = store.metadata_revision();
        assert!(store.metadata_write_started(1, Some("ETH")).is_err());
        assert!(store.metadata_revision_current(&before_duplicate));
        assert!(!store.metadata_write_pending("ETH"));

        for command in 2..=MAX_ATTEMPTS as u32 {
            store.metadata_write_started(command, Some("BTC")).unwrap();
        }
        let overflow = MAX_ATTEMPTS as u32 + 1;
        let before_overflow = store.metadata_revision();
        assert!(store.metadata_write_started(overflow, Some("ETH")).is_err());
        assert!(store.metadata_revision_current(&before_overflow));
        assert!(store.metadata_write_pending("BTC"));
        assert!(!store.metadata_write_pending("ETH"));

        store.metadata_write_finished(7);
        store.metadata_write_started(overflow, Some("ETH")).unwrap();
        assert!(store.metadata_write_pending("BTC"));
        assert!(store.metadata_write_pending("ETH"));
        store.metadata_write_finished(overflow);
        assert!(store.metadata_write_pending("BTC"));
        assert!(!store.metadata_write_pending("ETH"));
        let after_retirement = store.metadata_revision();
        store.metadata_write_finished(overflow);
        assert!(store.metadata_revision_current(&after_retirement));

        for command in 1..=MAX_ATTEMPTS as u32 {
            store.metadata_write_finished(command);
        }
        assert!(!store.metadata_write_pending("BTC"));
    }

    #[test]
    fn bounded_symbol_state_never_forgets_an_overflow_invalidation() {
        let mut state = State::default();
        for i in 0..MAX_SYMBOLS {
            state.changed(Some(&format!("S{i}")));
        }
        assert!(
            state
                .begin("overflow", true, Fallback::IndependentRails)
                .is_err()
        );
        let token = state.begin("S0", true, Fallback::IndependentRails).unwrap();
        state.changed(Some("overflow"));
        assert!(!state.eligible(&token));
        assert_eq!(state.revisions.len(), MAX_SYMBOLS);
    }
}

#[cfg(test)]
#[path = "metadata_release_tests.rs"]
mod release_tests;

#[cfg(feature = "http-plans")]
#[path = "metadata_work.rs"]
mod work;
#[cfg(feature = "http-plans")]
pub use work::{CredentialSubject, NativeWork, WorkKey};
