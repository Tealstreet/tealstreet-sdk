//! Public discovery retry policy. Venues rebuild requests to refresh signatures.
use crate::{Cx, LogLevel, TimerId};

struct Read<K> {
    key: K,
    attempts: u32,
    timer: Option<TimerId>,
}

pub struct Backoff<K> {
    reads: Vec<Read<K>>,
}
impl<K> Default for Backoff<K> {
    fn default() -> Self {
        Self { reads: Vec::new() }
    }
}
impl<K: PartialEq> Backoff<K> {
    pub fn waiting(&self, key: &K) -> bool {
        self.reads
            .iter()
            .any(|read| &read.key == key && read.timer.is_some())
    }

    /// At most one retry per independent read; no write request enters this API.
    pub fn failed<P>(&mut self, cx: &mut Cx<'_, P>, key: K) {
        let index = match self.reads.iter().position(|read| read.key == key) {
            Some(index) => index,
            None => {
                self.reads.push(Read {
                    key,
                    attempts: 0,
                    timer: None,
                });
                self.reads.len() - 1
            }
        };
        let read = &mut self.reads[index];
        if read.timer.is_some() {
            return;
        }
        read.attempts = read.attempts.saturating_add(1);
        let delay = (1_000u32 << read.attempts.saturating_sub(1).min(5)).min(30_000);
        read.timer = Some(cx.set_timer(f64::from(delay)));
        cx.log(
            LogLevel::Warning,
            "Public discovery read failed; retry scheduled",
        );
    }

    pub fn succeeded<P>(&mut self, cx: &mut Cx<'_, P>, key: &K) {
        if let Some(index) = self.reads.iter().position(|read| &read.key == key)
            && let Some(timer) = self.reads.swap_remove(index).timer
        {
            cx.clear_timer(timer);
        }
    }
}
impl<K: Clone> Backoff<K> {
    pub fn on_timer(&mut self, timer: TimerId) -> Option<K> {
        let read = self
            .reads
            .iter_mut()
            .find(|read| read.timer == Some(timer))?;
        read.timer = None;
        Some(read.key.clone())
    }
}

/// Do not expose raw catalogue rows, which can arrive through authenticated APIs.
pub fn skipped_row<P>(cx: &mut Cx<'_, P>) {
    cx.log(LogLevel::Warning, "Skipped malformed public discovery row");
}

/// TS numeric coercion for public fields that allow blank strings.
pub fn number_or_zero(text: &str) -> Option<f64> {
    if text.trim().is_empty() {
        Some(0.0)
    } else {
        text.parse::<f64>().ok().filter(|value| value.is_finite())
    }
}

/// Retains positional identities when one row in a nested public list is invalid.
pub struct Row<T>(pub Option<T>);
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Row<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: &'de serde_json::value::RawValue = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self(serde_json::from_str(raw.get()).ok()))
    }
}

/// Protects supplied live ticker fields while a slower discovery read is in flight.
/// Masks use `store::TICKER_FIELDS` order, including `MID_PRICE` for midpoint.
/// Inactive recording does no lookup or allocation.
#[derive(Default)]
pub struct LiveTickers {
    active: bool,
    rows: std::collections::HashMap<String, (u16, crate::model::Ticker)>,
}
impl LiveTickers {
    pub const MID_PRICE: u16 = 1 << 11;

    /// Repeated failures keep the same window until a usable seed commits.
    pub fn begin(&mut self) {
        self.active = true;
    }
    pub fn record(&mut self, store: &crate::store::Store, symbol: &str, mask: u16) {
        if !self.active || mask == 0 || store.market(symbol).is_none() {
            return;
        }
        let Some(row) = store.ticker(symbol) else {
            return;
        };
        if let Some((previous, saved)) = self.rows.get_mut(symbol) {
            Self::copy_fields(saved, row, mask);
            *previous |= mask;
        } else {
            self.rows.insert(symbol.into(), (mask, row.clone()));
        }
    }
    fn copy_fields(row: &mut crate::model::Ticker, live: &crate::model::Ticker, mask: u16) {
        let fields = [
            (&mut row.bid, live.bid),
            (&mut row.ask, live.ask),
            (&mut row.last, live.last),
            (&mut row.mark, live.mark),
            (&mut row.index, live.index),
            (&mut row.percentage, live.percentage),
            (&mut row.open_interest, live.open_interest),
            (&mut row.funding_rate, live.funding_rate),
            (&mut row.funding_time, live.funding_time),
            (&mut row.volume, live.volume),
            (&mut row.quote_volume, live.quote_volume),
        ];
        if mask & Self::MID_PRICE != 0 {
            row.mid_price = live.mid_price;
        }
        for (bit, (target, value)) in fields.into_iter().enumerate() {
            if mask & (1 << bit) != 0 {
                *target = value;
            }
        }
    }
    /// Preserve only actual live fields; REST still fills fields absent from live input.
    pub fn overlay(&self, store: &crate::store::Store, rows: &mut Vec<crate::model::Ticker>) {
        for row in rows.iter_mut() {
            if let Some((mask, live)) = self.rows.get(&row.symbol) {
                Self::copy_fields(row, live, *mask);
            }
        }
        // Follow the existing native table order rather than HashMap iteration.
        for previous in store.tickers() {
            if let Some((_, live)) = self.rows.get(&previous.symbol)
                && store.market(&previous.symbol).is_some()
                && !rows.iter().any(|row| row.symbol == previous.symbol)
            {
                rows.push(live.clone());
            }
        }
    }

    pub fn finish(&mut self, store: &crate::store::Store, rows: &mut Vec<crate::model::Ticker>) {
        self.overlay(store, rows);
        self.rows.clear();
        self.active = false;
    }
}

#[cfg(test)]
mod live_ticker_tests {
    use super::LiveTickers;
    use crate::model::{Limits, Market, MinMax, Precision, Ticker};
    use crate::store::Store;
    fn market(symbol: &str) -> Market {
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: symbol.into(),
            quote: "USDC".into(),
            active: true,
            precision: Precision {
                amount: 0.01,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.01,
                    max: 1000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 20.0,
                },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }
    #[test]
    fn late_seed_overlays_only_supplied_live_fields_and_preserves_missing_known_peers() {
        let mut store = Store::default();
        store.set_markets(vec![market("BTC"), market("ETH")]);
        store.set_tickers(vec![
            Ticker {
                id: "BTC".into(),
                symbol: "BTC".into(),
                bid: 110.0,
                ask: 112.0,
                ..Default::default()
            },
            Ticker {
                id: "ETH".into(),
                symbol: "ETH".into(),
                last: 200.0,
                ..Default::default()
            },
        ]);
        let mut live = LiveTickers::default();
        live.begin();
        live.record(&store, "BTC", 3);
        live.record(&store, "ETH", 1 << 2);
        live.record(&store, "UNKNOWN", 1 << 2);
        // A failed source retries within the same window, without erasing live BBO.
        live.begin();
        store.update_ticker("BTC", |ticker| {
            ticker.last = 111.0;
            ticker.bid = 999.0;
        });
        live.record(&store, "BTC", 1 << 2);
        let mut rows = vec![Ticker {
            id: "BTC".into(),
            symbol: "BTC".into(),
            bid: 90.0,
            ask: 92.0,
            last: 91.0,
            mark: 93.0,
            funding_time: 123.0,
            ..Default::default()
        }];
        live.finish(&store, &mut rows);
        assert_eq!(
            (
                rows[0].bid,
                rows[0].ask,
                rows[0].last,
                rows[0].mark,
                rows[0].funding_time
            ),
            (110.0, 112.0, 111.0, 93.0, 123.0)
        );
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].symbol.as_str(), rows[1].last), ("ETH", 200.0));
        assert!(!live.active && live.rows.is_empty());
        live.record(&store, "BTC", u16::MAX);
        assert!(
            live.rows.is_empty(),
            "steady updates do not allocate startup records"
        );
    }
    #[test]
    fn bbo_only_and_explicit_zero_updates_do_not_erase_unreported_seed_values() {
        let mut store = Store::default();
        store.set_markets(vec![market("BTC")]);
        store.set_tickers(vec![Ticker {
            id: "BTC".into(),
            symbol: "BTC".into(),
            bid: 0.0,
            ask: 102.0,
            ..Default::default()
        }]);
        let mut live = LiveTickers::default();
        live.begin();
        live.record(&store, "BTC", 3);
        store.update_ticker("BTC", |ticker| ticker.mid_price = Some(101.0));
        live.record(&store, "BTC", LiveTickers::MID_PRICE);
        let mut rows = vec![Ticker {
            id: "BTC".into(),
            symbol: "BTC".into(),
            bid: 99.0,
            ask: 101.0,
            last: 100.0,
            mark: 103.0,
            ..Default::default()
        }];
        live.overlay(&store, &mut rows);
        assert!(live.active && !live.rows.is_empty());
        assert_eq!(rows[0].mid_price, Some(101.0));
        live.finish(&store, &mut rows);
        assert_eq!(
            (rows[0].bid, rows[0].ask, rows[0].last, rows[0].mark),
            (0.0, 102.0, 100.0, 103.0)
        );
        live.begin();
        store.update_ticker("BTC", |ticker| ticker.last = 200.0);
        live.record(&store, "BTC", 1 << 2);
        live.finish(&store, &mut rows);
        assert_eq!((rows[0].last, rows[0].mark), (200.0, 103.0));
        assert_eq!(crate::store::TICKER_FIELDS[11], "midPrice");
        live.begin();
        store.update_ticker("BTC", |ticker| ticker.mid_price = None);
        live.record(&store, "BTC", LiveTickers::MID_PRICE);
        rows[0].mid_price = Some(999.0);
        live.finish(&store, &mut rows);
        assert_eq!(
            rows[0].mid_price, None,
            "a delayed seed cannot restore a cleared midpoint"
        );
        assert_eq!((rows[0].last, rows[0].mark), (200.0, 103.0));
    }
}
