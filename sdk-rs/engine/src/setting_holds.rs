//! Frame-fed setting holds. A settings write this engine confirmed (position
//! mode, margin type, leverage, or a venue's own key such as a builder
//! approval) stays in force over its data source's frames until the source
//! reports the same value, or until [`HOLD_MS`] passes and the source wins with
//! a warning. A source learns of a change only on its next read, and until
//! then each frame would put the old value back under the next write.
//!
//! Store-level keys ([`SettingKey::Hedged`], a symbol's margin type and
//! leverage) are kept by the engine across the frame's STATUS and SETTINGS
//! sections. A venue key lives in venue state, so the venue checks
//! `Cx::setting_held` in `apply_venue_state` and calls `Cx::release_setting`
//! once the source agrees.
use crate::store::{MarketSetting, Store};

/// How long a confirmed setting outlasts a data source that disagrees.
pub const HOLD_MS: f64 = 30_000.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingKey {
    /// The account-wide hedge flag (`Store::hedged`).
    Hedged,
    /// A symbol's position mode, where the venue keeps it per symbol.
    PositionMode(String),
    /// A symbol's margin type (`MarketSetting::margin_type`).
    MarginType(String),
    /// A symbol's leverage rails (`MarketSetting` leverages).
    Leverage(String),
    /// A venue's own key, such as Hyperliquid's builder approval.
    Venue(&'static str, String),
}

impl SettingKey {
    pub fn symbol(&self) -> Option<&str> {
        match self {
            Self::Hedged => None,
            Self::PositionMode(symbol) | Self::MarginType(symbol) | Self::Leverage(symbol) => {
                Some(symbol)
            }
            Self::Venue(_, scope) => (!scope.is_empty()).then_some(scope.as_str()),
        }
    }
}

struct Hold {
    key: SettingKey,
    at: f64,
    /// The source reported a different value while held. Only such a hold
    /// warns when it expires; one the source never spoke about lapses quietly,
    /// since the source may already have held the same value.
    disputed: bool,
}

#[derive(Default)]
pub(crate) struct SettingHolds {
    held: Vec<Hold>,
}

/// The store values a frame may overwrite, taken before it is applied.
pub(crate) struct Kept {
    hedged: Option<bool>,
    settings: Vec<(SettingKey, Option<MarketSetting>)>,
}

fn same_leverage(a: &MarketSetting, b: &MarketSetting) -> bool {
    let bits = |v: Option<f64>| v.map(f64::to_bits);
    a.leverage.to_bits() == b.leverage.to_bits()
        && bits(a.buy_leverage) == bits(b.buy_leverage)
        && bits(a.sell_leverage) == bits(b.sell_leverage)
}

impl SettingHolds {
    pub fn hold(&mut self, key: SettingKey, now: f64) {
        self.held.retain(|hold| hold.key != key);
        self.held.push(Hold {
            key,
            at: now,
            disputed: false,
        });
    }

    pub fn held(&self, key: &SettingKey) -> bool {
        self.held.iter().any(|hold| hold.key == *key)
    }

    pub fn release(&mut self, key: &SettingKey) {
        self.held.retain(|hold| hold.key != *key);
    }

    fn dispute(&mut self, key: &SettingKey) {
        for hold in self.held.iter_mut().filter(|hold| hold.key == *key) {
            hold.disputed = true;
        }
    }

    /// A VENUE_STATE was applied and a venue key is still held: the venue kept
    /// its value over the source's (agreement would have released it).
    pub fn dispute_venue_keys(&mut self) {
        for hold in &mut self.held {
            if matches!(
                hold.key,
                SettingKey::Venue(..) | SettingKey::PositionMode(_)
            ) {
                hold.disputed = true;
            }
        }
    }

    /// Drops holds older than [`HOLD_MS`]; the source's value then applies.
    /// Returns those the source had contradicted: only they are worth a warning.
    pub fn expire(&mut self, now: f64) -> Vec<SettingKey> {
        let mut disputed = Vec::new();
        self.held.retain(|hold| {
            let keep = now - hold.at < HOLD_MS;
            if !keep && hold.disputed {
                disputed.push(hold.key.clone());
            }
            keep
        });
        disputed
    }

    pub fn keep(&self, store: &Store) -> Kept {
        let mut kept = Kept {
            hedged: None,
            settings: Vec::new(),
        };
        for Hold { key, .. } in &self.held {
            match key {
                SettingKey::Hedged => kept.hedged = Some(store.hedged()),
                SettingKey::MarginType(symbol) | SettingKey::Leverage(symbol) => {
                    kept.settings.push((key.clone(), store.setting(symbol)));
                }
                _ => {}
            }
        }
        kept
    }

    /// After a frame: a held store value the source now matches is released;
    /// one it contradicts is put back. Only a frame that carried the value's
    /// section says anything about it: the source writes STATUS and SETTINGS
    /// only when they change, so a ticker or orders delta neither agrees nor
    /// disagrees, and leaves the hold as it was.
    pub fn restore(&mut self, store: &mut Store, kept: Kept, status: bool, settings: bool) {
        if let Some(mine) = kept.hedged.filter(|_| status) {
            if store.hedged() == mine {
                self.release(&SettingKey::Hedged);
            } else {
                store.set_hedged(mine);
                self.dispute(&SettingKey::Hedged);
            }
        }
        for (key, mine) in kept.settings.into_iter().filter(|_| settings) {
            let Some(mine) = mine else {
                continue;
            };
            let (SettingKey::MarginType(symbol) | SettingKey::Leverage(symbol)) = &key else {
                continue;
            };
            let source = store.setting(symbol).unwrap_or_default();
            let agrees = match key {
                SettingKey::MarginType(_) => source.margin_type == mine.margin_type,
                _ => same_leverage(&source, &mine),
            };
            if agrees {
                self.release(&key);
                continue;
            }
            self.dispute(&key);
            store.update_setting(symbol, |setting| match key {
                SettingKey::MarginType(_) => setting.margin_type = mine.margin_type,
                _ => {
                    setting.leverage = mine.leverage;
                    setting.buy_leverage = mine.buy_leverage;
                    setting.sell_leverage = mine.sell_leverage;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MarginType;

    #[test]
    fn only_a_hold_the_source_contradicted_warns_when_it_expires() {
        let mut holds = SettingHolds::default();
        let mut store = Store::default();
        store.set_hedged(true);
        holds.hold(SettingKey::Hedged, 0.);
        holds.hold(SettingKey::Venue("builder", String::new()), 0.);
        // The source never speaks about either: both lapse quietly.
        assert!(holds.expire(HOLD_MS).is_empty());
        assert!(!holds.held(&SettingKey::Hedged));
        // A stale STATUS contradicts the hedge flag: that expiry is worth a warning.
        holds.hold(SettingKey::Hedged, 0.);
        let kept = holds.keep(&store);
        store.set_hedged(false);
        holds.restore(&mut store, kept, true, false);
        holds.hold(SettingKey::Venue("builder", String::new()), 0.);
        holds.dispute_venue_keys();
        assert_eq!(
            holds.expire(HOLD_MS),
            vec![
                SettingKey::Hedged,
                SettingKey::Venue("builder", String::new())
            ]
        );
    }

    #[test]
    fn a_frame_without_status_neither_releases_nor_restores_the_hedge_hold() {
        let mut holds = SettingHolds::default();
        let mut store = Store::default();
        store.set_hedged(true);
        holds.hold(SettingKey::Hedged, 0.);
        // A ticker or orders delta: the store still agrees, but that is no evidence.
        let kept = holds.keep(&store);
        holds.restore(&mut store, kept, false, false);
        assert!(holds.held(&SettingKey::Hedged));
        // The source's next, stale STATUS: the confirmed value is kept.
        let kept = holds.keep(&store);
        store.set_hedged(false);
        holds.restore(&mut store, kept, true, false);
        assert!(store.hedged());
        assert!(holds.held(&SettingKey::Hedged));
    }

    #[test]
    fn a_frame_without_settings_leaves_a_margin_hold_in_place() {
        let mut holds = SettingHolds::default();
        let mut store = Store::default();
        store.update_setting("BTCUSDT", |s| s.margin_type = Some(MarginType::Isolated));
        holds.hold(SettingKey::MarginType("BTCUSDT".into()), 0.);
        let kept = holds.keep(&store);
        holds.restore(&mut store, kept, true, false);
        assert!(holds.held(&SettingKey::MarginType("BTCUSDT".into())));
    }

    #[test]
    fn a_held_hedge_flag_survives_a_contradicting_frame_and_releases_when_it_agrees() {
        let mut holds = SettingHolds::default();
        let mut store = Store::default();
        store.set_hedged(true);
        holds.hold(SettingKey::Hedged, 0.);
        let kept = holds.keep(&store);
        store.set_hedged(false);
        holds.restore(&mut store, kept, true, false);
        assert!(store.hedged());
        assert!(holds.held(&SettingKey::Hedged));
        let kept = holds.keep(&store);
        holds.restore(&mut store, kept, true, false);
        assert!(!holds.held(&SettingKey::Hedged));
    }

    #[test]
    fn a_held_margin_type_is_put_back_and_a_hold_expires() {
        let mut holds = SettingHolds::default();
        let mut store = Store::default();
        store.update_setting("BTCUSDT", |s| s.margin_type = Some(MarginType::Isolated));
        holds.hold(SettingKey::MarginType("BTCUSDT".into()), 0.);
        let kept = holds.keep(&store);
        store.update_setting("BTCUSDT", |s| s.margin_type = Some(MarginType::Cross));
        holds.restore(&mut store, kept, false, true);
        assert_eq!(
            store.setting("BTCUSDT").unwrap().margin_type,
            Some(MarginType::Isolated)
        );
        assert!(holds.expire(HOLD_MS - 1.).is_empty());
        assert_eq!(
            holds.expire(HOLD_MS),
            vec![SettingKey::MarginType("BTCUSDT".into())]
        );
    }
}
