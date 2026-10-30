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

#[derive(Default)]
pub(crate) struct SettingHolds {
    held: Vec<(SettingKey, f64)>,
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
        self.held.retain(|(held, _)| *held != key);
        self.held.push((key, now));
    }

    pub fn held(&self, key: &SettingKey) -> bool {
        self.held.iter().any(|(held, _)| held == key)
    }

    pub fn release(&mut self, key: &SettingKey) {
        self.held.retain(|(held, _)| held != key);
    }

    /// Drops holds older than [`HOLD_MS`]; the source's value then applies.
    pub fn expire(&mut self, now: f64) -> Vec<SettingKey> {
        let mut expired = Vec::new();
        self.held.retain(|(key, at)| {
            let keep = now - at < HOLD_MS;
            if !keep {
                expired.push(key.clone());
            }
            keep
        });
        expired
    }

    pub fn keep(&self, store: &Store) -> Kept {
        let mut kept = Kept {
            hedged: None,
            settings: Vec::new(),
        };
        for (key, _) in &self.held {
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
    /// one it contradicts is put back.
    pub fn restore(&mut self, store: &mut Store, kept: Kept) {
        if let Some(mine) = kept.hedged {
            if store.hedged() == mine {
                self.release(&SettingKey::Hedged);
            } else {
                store.set_hedged(mine);
            }
        }
        for (key, mine) in kept.settings {
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
    fn a_held_hedge_flag_survives_a_contradicting_frame_and_releases_when_it_agrees() {
        let mut holds = SettingHolds::default();
        let mut store = Store::default();
        store.set_hedged(true);
        holds.hold(SettingKey::Hedged, 0.);
        let kept = holds.keep(&store);
        store.set_hedged(false);
        holds.restore(&mut store, kept);
        assert!(store.hedged());
        assert!(holds.held(&SettingKey::Hedged));
        let kept = holds.keep(&store);
        holds.restore(&mut store, kept);
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
        holds.restore(&mut store, kept);
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
