//! Per-symbol metadata ownership. Dispatch, leases and retry permission remain
//! with RequestContext/read_scheduler; this module owns waiters and publication.
use crate::store::MarketSetting;
use crate::{CommandId, Cx, ErrorKind, Outcome, VenueError};
use std::collections::HashMap;

const MAX_ATTEMPTS: usize = 256;
const MAX_WAITERS: usize = 64;
const MAX_SYMBOLS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Token(u64);

/// Observation of a background settings read, before any native response rows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Observation(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    IndependentRails,
    CompleteSetting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Start(Token),
    Joined,
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
pub struct Completion {
    pub published: bool,
    commands: Vec<CommandId>,
    result: Result<Value, VenueError>,
}
impl Completion {
    pub fn finish<P>(self, cx: &mut Cx<'_, P>, encode: impl FnOnce(Value) -> String) {
        let result = self.result.map(encode);
        for command in self.commands {
            match &result {
                Ok(value) => cx.done(command, value.clone()),
                Err(error) => cx.fail(command, error.clone()),
            }
        }
    }
    pub fn commands(&self) -> &[CommandId] {
        &self.commands
    }
    pub fn result(&self) -> &Result<Value, VenueError> {
        &self.result
    }
}

struct Attempt {
    symbol: String,
    revision: u64,
    global: u64,
    publish: bool,
    fallback: Fallback,
    commands: Vec<CommandId>,
}

#[derive(Default)]
pub(crate) struct State {
    serial: u64,
    global: u64,
    exhausted: bool,
    account_fence: u64,
    revisions: HashMap<String, u64>,
    observation_floors: HashMap<String, u64>,
    current: HashMap<String, Token>,
    attempts: HashMap<Token, Attempt>,
    writes: HashMap<CommandId, Option<String>>,
}
impl State {
    fn tick(&mut self) -> u64 {
        if let Some(next) = self.serial.checked_add(1) {
            self.serial = next;
        } else {
            self.exhausted = true;
        }
        self.serial
    }
    pub(crate) fn observation(&mut self) -> Observation {
        Observation(self.tick())
    }
    pub(crate) fn observe(&mut self, symbol: &str, stamp: Observation) -> bool {
        if self.exhausted
            || self.global > stamp.0
            || self.observation_floors.get(symbol).copied().unwrap_or(0) >= stamp.0
            || (!self.revisions.contains_key(symbol) && self.revisions.len() >= MAX_SYMBOLS)
        {
            return false;
        }
        // Order background proposals by issue, not completion. A later-issued
        // read may supersede an earlier commit, but never a later write/event.
        let version = self.tick();
        self.revisions.insert(symbol.to_owned(), version);
        self.observation_floors.insert(symbol.to_owned(), stamp.0);
        true
    }
    pub(crate) fn changed(&mut self, symbol: Option<&str>) {
        let next = self.tick();
        if let Some(symbol) = symbol {
            if !self.revisions.contains_key(symbol) && self.revisions.len() >= MAX_SYMBOLS {
                // Never forget an invalidation because the bounded map is full.
                self.global = next;
            } else {
                self.revisions.insert(symbol.to_owned(), next);
                self.observation_floors.insert(symbol.to_owned(), next);
            }
        } else {
            self.global = next;
        }
    }
    pub(crate) fn invalidate(&mut self, symbol: Option<&str>) {
        self.changed(symbol);
        self.account_fence = self.serial;
    }
    pub(crate) fn revision(&self) -> u64 {
        self.account_fence
    }
    pub(crate) fn write_started(
        &mut self,
        command: CommandId,
        symbol: Option<&str>,
    ) -> Result<(), VenueError> {
        if self.writes.len() >= MAX_ATTEMPTS || self.writes.contains_key(&command) {
            return Err(invalid("metadata write capacity or duplicate owner"));
        }
        self.invalidate(symbol);
        self.writes.insert(command, symbol.map(str::to_owned));
        Ok(())
    }
    pub(crate) fn write_finished(&mut self, command: CommandId) {
        if let Some(symbol) = self.writes.remove(&command) {
            self.invalidate(symbol.as_deref());
        }
    }
    pub(crate) fn write_pending(&self, symbol: &str) -> bool {
        self.writes
            .values()
            .any(|scope| scope.as_deref().is_none_or(|s| s == symbol))
    }
    pub(crate) fn begin(
        &mut self,
        symbol: &str,
        command: CommandId,
        publish: bool,
        fallback: Fallback,
    ) -> Result<Admission, VenueError> {
        let publish = publish && !self.write_pending(symbol);
        if self.exhausted {
            return Err(invalid("metadata token space exhausted"));
        }
        if self
            .attempts
            .values()
            .any(|read| read.commands.contains(&command))
        {
            return Err(invalid("duplicate metadata command owner"));
        }
        if let Some(token) = self.current.get(symbol)
            && let Some(read) = self.attempts.get_mut(token)
            && read.revision == self.revisions.get(symbol).copied().unwrap_or(0)
            && read.global == self.global
            && read.publish == publish
            && read.fallback == fallback
        {
            if read.commands.len() >= MAX_WAITERS {
                return Err(invalid("metadata waiter capacity exceeded"));
            }
            read.commands.push(command);
            return Ok(Admission::Joined);
        }
        if self.attempts.len() >= MAX_ATTEMPTS
            || (!self.revisions.contains_key(symbol) && self.revisions.len() >= MAX_SYMBOLS)
        {
            return Err(invalid("metadata read capacity exceeded"));
        }
        // Ownership starts here; publication freshness changes only on an
        // accepted proposal or write/event, never merely because I/O was queued.
        self.tick();
        let revision = *self.revisions.entry(symbol.to_owned()).or_default();
        if self.exhausted {
            return Err(invalid("metadata token space exhausted"));
        }
        let token = Token(self.serial);
        self.current.insert(symbol.to_owned(), token);
        self.attempts.insert(
            token,
            Attempt {
                symbol: symbol.to_owned(),
                revision,
                global: self.global,
                publish,
                fallback,
                commands: vec![command],
            },
        );
        Ok(Admission::Start(token))
    }
    pub(crate) fn eligible(&self, token: Token) -> bool {
        self.attempts.get(&token).is_some_and(|read| {
            !self.exhausted
                && read.publish
                && read.global == self.global
                && self.revisions.get(&read.symbol) == Some(&read.revision)
                && self.current.get(&read.symbol) == Some(&token)
        })
    }
    pub(crate) fn symbol(&self, token: Token) -> Option<&str> {
        self.attempts.get(&token).map(|read| read.symbol.as_str())
    }
    pub(crate) fn complete(
        &mut self,
        token: Token,
        result: Result<Proposal, VenueError>,
        cached: Option<MarketSetting>,
    ) -> Option<(Completion, Option<MarketSetting>)> {
        let eligible = self.eligible(token);
        let read = self.attempts.remove(&token)?;
        if self.current.get(&read.symbol) == Some(&token) {
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
            Completion {
                published: publication.is_some(),
                commands: read.commands,
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
        command: u32,
        publish: bool,
        fallback: Fallback,
    ) -> Token {
        let Admission::Start(token) = store
            .begin_metadata(symbol, command, publish, fallback)
            .unwrap()
        else {
            panic!("new attempt expected")
        };
        token
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
        assert!(two.finish_metadata(a, Ok(proposal(7.0, 9.0))).is_none());
        assert!(two.metadata_eligible(b));
    }

    #[test]
    fn foreign_store_observation_cannot_publish_settings() {
        let mut one = Store::default();
        let mut two = Store::default();
        let foreign = one.metadata_observation();
        assert!(!two.observe_setting(
            "BTC", foreign, ObservedSetting::new(setting(99.0, 99.0)).unwrap()
        ));
        assert!(two.setting("BTC").is_none());
    }

    #[test]
    fn joined_owners_share_one_atomic_publication() {
        let mut store = Store::default();
        let token = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        assert_eq!(
            store
                .begin_metadata("BTC", 2, true, Fallback::IndependentRails)
                .unwrap(),
            Admission::Joined
        );
        let done = store
            .finish_metadata(token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert!(done.published);
        assert_eq!(done.commands(), [1, 2]);
        assert_eq!(store.setting("BTC"), Some(setting(7.0, 9.0)));
        assert!(
            store
                .finish_metadata(token, Ok(proposal(99.0, 99.0)))
                .is_none()
        );
        assert_eq!(store.setting("BTC"), Some(setting(7.0, 9.0)));
    }
    #[test]
    fn failed_attempt_retains_settings_and_releases_all_waiters() {
        let mut store = Store::default();
        store.update_setting("BTC", |s| *s = setting(20.0, 20.0));
        let token = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        store
            .begin_metadata("BTC", 2, true, Fallback::CompleteSetting)
            .unwrap();
        let error = VenueError::new("timeout").kind(ErrorKind::Timeout);
        let done = store.finish_metadata(token, Err(error.clone())).unwrap();
        assert!(!done.published);
        assert_eq!(done.commands(), [1, 2]);
        assert_eq!(done.result().as_ref().unwrap_err(), &error);
        assert_eq!(store.setting("BTC"), Some(setting(20.0, 20.0)));
        let next = start(&mut store, "BTC", 3, true, Fallback::CompleteSetting);
        assert_ne!(next, token);
        assert!(
            store
                .finish_metadata(token, Ok(proposal(99.0, 99.0)))
                .is_none()
        );
        assert!(store.metadata_eligible(next));
    }
    #[test]
    fn writes_fence_old_read_and_new_waiters_do_not_join_it() {
        let mut store = Store::default();
        let old = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        store.invalidate_metadata(Some("BTC"));
        store.update_setting("BTC", |s| *s = setting(30.0, 40.0));
        let next = start(&mut store, "BTC", 2, true, Fallback::IndependentRails);
        let stale = store.finish_metadata(old, Ok(proposal(7.0, 9.0))).unwrap();
        assert!(!stale.published);
        assert_eq!(stale.commands(), [1]);
        assert_eq!(stale.result().as_ref().unwrap().buy, 30.0);
        assert!(store.metadata_eligible(next));
        assert!(
            store
                .finish_metadata(next, Ok(proposal(50.0, 60.0)))
                .unwrap()
                .published
        );
    }
    #[test]
    fn pending_write_publication_restriction_is_sticky() {
        let mut store = Store::default();
        let restricted = start(&mut store, "BTC", 1, false, Fallback::IndependentRails);
        assert!(!store.metadata_eligible(restricted));
        let next = start(&mut store, "BTC", 2, true, Fallback::IndependentRails);
        let done = store
            .finish_metadata(
                restricted,
                Ok(Proposal::rails(Rails::new(7.0, 9.0).unwrap())),
            )
            .unwrap();
        assert!(!done.published);
        assert!(store.setting("BTC").is_none());
        assert!(store.metadata_eligible(next));
    }
    #[test]
    fn symbol_changes_do_not_revoke_an_unrelated_read() {
        let mut store = Store::default();
        let btc = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        let eth = start(&mut store, "ETH", 2, true, Fallback::CompleteSetting);
        store.invalidate_metadata(Some("BTC"));
        assert!(!store.metadata_eligible(btc));
        assert!(store.metadata_eligible(eth));
        store.invalidate_metadata(None);
        assert!(!store.metadata_eligible(eth));
    }
    #[test]
    fn a_partial_proposal_never_publishes() {
        let mut store = Store::default();
        let token = start(&mut store, "BTC", 1, true, Fallback::IndependentRails);
        let done = store
            .finish_metadata(token, Ok(Proposal::rails(Rails::new(7.0, 9.0).unwrap())))
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
                .finish_metadata(token, Ok(proposal(7.0, 9.0)))
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
            .finish_metadata(token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert!(!store.observe_setting(
            "BTC",
            old,
            ObservedSetting::new(setting(2.0, 2.0)).unwrap()
        ));
        let fresh = store.metadata_observation();
        assert!(store.observe_setting(
            "ETH",
            fresh,
            ObservedSetting::new(setting(3.0, 3.0)).unwrap()
        ));
        store.invalidate_metadata(Some("BTC"));
        assert!(!store.observe_setting(
            "BTC",
            fresh,
            ObservedSetting::new(setting(4.0, 4.0)).unwrap()
        ));
        assert_eq!(store.setting("BTC"), Some(setting(7.0, 9.0)));
    }
    #[test]
    fn waiter_capacity_and_duplicate_owners_fail_without_dropping_the_original() {
        let mut store = Store::default();
        let token = start(&mut store, "BTC", 1, true, Fallback::CompleteSetting);
        assert!(
            store
                .begin_metadata("BTC", 1, true, Fallback::CompleteSetting)
                .is_err()
        );
        for id in 2..=MAX_WAITERS as u32 {
            assert_eq!(
                store
                    .begin_metadata("BTC", id, true, Fallback::CompleteSetting)
                    .unwrap(),
                Admission::Joined
            );
        }
        assert!(
            store
                .begin_metadata("BTC", 1000, true, Fallback::CompleteSetting)
                .is_err()
        );
        let done = store
            .finish_metadata(token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert_eq!(done.commands().len(), MAX_WAITERS);
    }
    #[test]
    fn token_exhaustion_fails_closed_without_reusing_identity() {
        let mut state = State {
            serial: u64::MAX - 1,
            ..State::default()
        };
        let Admission::Start(token) = state
            .begin("BTC", 1, true, Fallback::CompleteSetting)
            .unwrap()
        else {
            panic!()
        };
        state.changed(Some("BTC"));
        assert!(!state.eligible(token));
        assert!(
            state
                .begin("BTC", 2, true, Fallback::CompleteSetting)
                .is_err()
        );
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
                    new,
                    ObservedSetting::new(setting(20.0, 20.0)).unwrap()
                ));
                assert!(!store.observe_setting(
                    "BTC",
                    old,
                    ObservedSetting::new(setting(10.0, 10.0)).unwrap()
                ));
            } else {
                assert!(store.observe_setting(
                    "BTC",
                    old,
                    ObservedSetting::new(setting(10.0, 10.0)).unwrap()
                ));
                assert!(store.observe_setting(
                    "BTC",
                    new,
                    ObservedSetting::new(setting(20.0, 20.0)).unwrap()
                ));
            }
            assert_eq!(store.setting("BTC"), Some(setting(20.0, 20.0)));
            assert!(!store.observe_setting(
                "BTC",
                new,
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
            background,
            ObservedSetting::new(setting(20.0, 20.0)).unwrap()
        ));
        let result = store
            .finish_metadata(token, Ok(proposal(7.0, 9.0)))
            .unwrap();
        assert!(!result.published);
        assert_eq!(result.result().as_ref().unwrap().buy, 20.0);
        let old = store.metadata_observation();
        let fresh = start(&mut store, "BTC", 2, true, Fallback::IndependentRails);
        assert!(
            store
                .finish_metadata(fresh, Ok(proposal(30.0, 40.0)))
                .unwrap()
                .published
        );
        assert!(!store.observe_setting(
            "BTC",
            old,
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
        assert!(!store.metadata_eligible(blocked));
        assert!(store.metadata_eligible(eth));
        store.metadata_write_finished(80);
        assert!(!store.metadata_write_pending("BTC"));
        assert!(!store.metadata_eligible(blocked));
        let next = start(&mut store, "BTC", 3, true, Fallback::IndependentRails);
        assert!(store.metadata_eligible(next));
        let old = store
            .finish_metadata(
                blocked,
                Err(VenueError::new("canceled").kind(ErrorKind::Timeout)),
            )
            .unwrap();
        assert_eq!(old.commands(), [1]);
        assert!(store.metadata_eligible(next));
        store.metadata_write_started(81, None).unwrap();
        assert!(store.metadata_write_pending("ETH"));
        assert!(!store.metadata_eligible(eth));
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
                .begin_metadata("overflow", 999, true, Fallback::IndependentRails)
                .is_err()
        );
        let first = tokens[0];
        store
            .finish_metadata(
                first,
                Err(VenueError::new("expired").kind(ErrorKind::Timeout)),
            )
            .unwrap();
        let next = start(&mut store, "S0", 999, true, Fallback::IndependentRails);
        assert_ne!(next, first);
        assert!(
            store
                .finish_metadata(first, Ok(proposal(99.0, 99.0)))
                .is_none()
        );
        assert!(store.metadata_eligible(next));
    }
    #[test]
    fn bounded_symbol_state_never_forgets_an_overflow_invalidation() {
        let mut state = State::default();
        for i in 0..MAX_SYMBOLS {
            state.changed(Some(&format!("S{i}")));
        }
        assert!(
            state
                .begin("overflow", 1, true, Fallback::IndependentRails)
                .is_err()
        );
        let Admission::Start(token) = state
            .begin("S0", 1, true, Fallback::IndependentRails)
            .unwrap()
        else {
            panic!()
        };
        state.changed(Some("overflow"));
        assert!(!state.eligible(token));
        assert_eq!(state.revisions.len(), MAX_SYMBOLS);
    }
}
