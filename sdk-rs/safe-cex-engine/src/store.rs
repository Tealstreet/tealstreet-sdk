//! Typed exchange state plus change tracking. Hosts drain changes with
//! [`Store::take_update`] and [`Store::take_ticker_deltas`] once per flush
//! instead of per websocket message.
use crate::model::{Balance, Loaded, Market, Order, Position, Ticker};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Default)]
pub struct Store {
    markets: Vec<Market>,
    market_by_symbol: HashMap<String, u32>,
    market_by_id: HashMap<String, u32>,
    tickers: Vec<Ticker>,
    ticker_by_symbol: HashMap<String, u32>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    balance: Balance,
    loaded: Loaded,
    latency: f64,
    changes: Changes,
    /// Changed-field bits per ticker, in [`TICKER_FIELDS`] order.
    ticker_masks: Vec<u16>,
    dirty_tickers: Vec<u32>,
}

/// Packed ticker delta field order; matches `PACKED_TICKER_FIELDS` in
/// `packages/safe-cex/src/store/store.base.ts`.
pub const TICKER_FIELDS: [&str; 11] = [
    "bid",
    "ask",
    "last",
    "mark",
    "index",
    "percentage",
    "openInterest",
    "fundingRate",
    "fundingTime",
    "volume",
    "quoteVolume",
];

fn ticker_values(t: &Ticker) -> [f64; 11] {
    [
        t.bid,
        t.ask,
        t.last,
        t.mark,
        t.index,
        t.percentage,
        t.open_interest,
        t.funding_rate,
        t.funding_time,
        t.volume,
        t.quote_volume,
    ]
}

#[derive(Default)]
struct Changes {
    markets: bool,
    tickers_replaced: bool,
    orders: bool,
    positions: bool,
    balance: bool,
    loaded: bool,
    latency: bool,
}

/// Everything that changed since the previous flush. Absent fields are unchanged.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreUpdate<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markets: Option<&'a [Market]>,
    /// The complete ticker list, sent when the set of tickers was replaced.
    /// Value changes otherwise travel as packed deltas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tickers: Option<&'a [Ticker]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<&'a [Order]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub positions: Option<&'a [Position]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<&'a Balance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<Loaded>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency: Option<f64>,
}

impl StoreUpdate<'_> {
    pub fn is_empty(&self) -> bool {
        self.markets.is_none()
            && self.tickers.is_none()
            && self.orders.is_none()
            && self.positions.is_none()
            && self.balance.is_none()
            && self.loaded.is_none()
            && self.latency.is_none()
    }
}

impl Store {
    pub fn markets(&self) -> &[Market] {
        &self.markets
    }

    pub fn market(&self, symbol: &str) -> Option<&Market> {
        self.market_by_symbol
            .get(symbol)
            .map(|&index| &self.markets[index as usize])
    }

    pub fn market_by_id(&self, id: &str) -> Option<&Market> {
        self.market_by_id
            .get(id)
            .map(|&index| &self.markets[index as usize])
    }

    pub fn set_markets(&mut self, markets: Vec<Market>) {
        self.market_by_symbol = markets
            .iter()
            .enumerate()
            .map(|(index, market)| (market.symbol.clone(), index as u32))
            .collect();
        self.market_by_id = markets
            .iter()
            .enumerate()
            .map(|(index, market)| (market.id.clone(), index as u32))
            .collect();
        self.markets = markets;
        self.changes.markets = true;
        self.set_loaded(|loaded| loaded.markets = true);
    }

    pub fn tickers(&self) -> &[Ticker] {
        &self.tickers
    }

    pub fn ticker(&self, symbol: &str) -> Option<&Ticker> {
        self.ticker_by_symbol
            .get(symbol)
            .map(|&index| &self.tickers[index as usize])
    }

    pub fn set_tickers(&mut self, tickers: Vec<Ticker>) {
        self.ticker_by_symbol = tickers
            .iter()
            .enumerate()
            .map(|(index, ticker)| (ticker.symbol.clone(), index as u32))
            .collect();
        self.tickers = tickers;
        self.changes.tickers_replaced = true;
        self.ticker_masks = vec![0; self.tickers.len()];
        self.dirty_tickers.clear();
        self.set_loaded(|loaded| loaded.tickers = true);
    }

    /// Mutate one ticker in place; changed fields go out with the next deltas.
    pub fn update_ticker(&mut self, symbol: &str, update: impl FnOnce(&mut Ticker)) {
        let Some(&index) = self.ticker_by_symbol.get(symbol) else {
            return;
        };
        let ticker = &mut self.tickers[index as usize];
        let before = ticker_values(ticker);
        update(ticker);
        let after = ticker_values(ticker);
        let mut mask = 0u16;
        for (bit, (a, b)) in before.iter().zip(after.iter()).enumerate() {
            if a.to_bits() != b.to_bits() {
                mask |= 1 << bit;
            }
        }
        if mask == 0 {
            return;
        }
        let slot = &mut self.ticker_masks[index as usize];
        if *slot == 0 {
            self.dirty_tickers.push(index);
        }
        *slot |= mask;
    }

    /// Append changed tickers as packed deltas, `[index, mask, ...values]`
    /// each, and reset tracking.
    pub fn take_ticker_deltas(&mut self, out: &mut Vec<f64>) {
        for index in self.dirty_tickers.drain(..) {
            let mask = std::mem::take(&mut self.ticker_masks[index as usize]);
            let values = ticker_values(&self.tickers[index as usize]);
            out.push(index as f64);
            out.push(mask as f64);
            for (bit, value) in values.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    out.push(*value);
                }
            }
        }
    }

    pub fn orders(&self) -> &[Order] {
        &self.orders
    }

    pub fn set_orders(&mut self, orders: Vec<Order>) {
        self.orders = orders;
        self.changes.orders = true;
        self.set_loaded(|loaded| loaded.orders = true);
    }

    /// Insert or replace by id; an order that is no longer open is removed.
    pub fn upsert_order(&mut self, order: Order) {
        let open = matches!(order.status, crate::model::OrderStatus::Open);
        match self
            .orders
            .iter()
            .position(|existing| existing.id == order.id)
        {
            Some(index) if open => self.orders[index] = order,
            Some(index) => {
                self.orders.remove(index);
            }
            None if open => self.orders.push(order),
            None => return,
        }
        self.changes.orders = true;
    }

    pub fn remove_orders(&mut self, ids: &[&str]) {
        let before = self.orders.len();
        self.orders
            .retain(|order| !ids.contains(&order.id.as_str()));
        if self.orders.len() != before {
            self.changes.orders = true;
        }
    }

    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    pub fn set_positions(&mut self, positions: Vec<Position>) {
        self.positions = positions;
        self.changes.positions = true;
        self.set_loaded(|loaded| loaded.positions = true);
    }

    /// Insert or replace by id; a position with zero contracts is removed.
    pub fn upsert_position(&mut self, position: Position) {
        let index = self.positions.iter().position(|p| p.id == position.id);
        match (index, position.contracts == 0.0) {
            (Some(index), true) => {
                self.positions.remove(index);
            }
            (Some(index), false) => self.positions[index] = position,
            (None, true) => return,
            (None, false) => self.positions.push(position),
        }
        self.changes.positions = true;
    }

    pub fn balance(&self) -> &Balance {
        &self.balance
    }

    pub fn set_balance(&mut self, balance: Balance) {
        if balance != self.balance {
            self.balance = balance;
            self.changes.balance = true;
        }
        self.set_loaded(|loaded| loaded.balance = true);
    }

    pub fn loaded(&self) -> Loaded {
        self.loaded
    }

    pub fn set_latency(&mut self, latency: f64) {
        if latency != self.latency {
            self.latency = latency;
            self.changes.latency = true;
        }
    }

    fn set_loaded(&mut self, update: impl FnOnce(&mut Loaded)) {
        let before = self.loaded;
        update(&mut self.loaded);
        if self.loaded != before {
            self.changes.loaded = true;
        }
    }

    /// Build the delta since the previous call and reset change tracking.
    /// Call it once per flush; serialize the result before mutating again.
    pub fn take_update(&mut self) -> StoreUpdate<'_> {
        let changes = std::mem::take(&mut self.changes);
        if changes.tickers_replaced {
            // The full list supersedes any pending value deltas.
            for index in self.dirty_tickers.drain(..) {
                self.ticker_masks[index as usize] = 0;
            }
        }
        StoreUpdate {
            markets: changes.markets.then_some(self.markets.as_slice()),
            tickers: changes.tickers_replaced.then_some(self.tickers.as_slice()),
            orders: changes.orders.then_some(self.orders.as_slice()),
            positions: changes.positions.then_some(self.positions.as_slice()),
            balance: changes.balance.then_some(&self.balance),
            loaded: changes.loaded.then_some(self.loaded),
            latency: changes.latency.then_some(self.latency),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticker(symbol: &str) -> Ticker {
        Ticker {
            id: symbol.into(),
            symbol: symbol.into(),
            ..Ticker::default()
        }
    }

    #[test]
    fn packs_only_changed_ticker_fields_after_the_initial_list() {
        let mut store = Store::default();
        store.set_tickers(vec![ticker("A"), ticker("B"), ticker("C")]);
        store.update_ticker("A", |t| t.bid = 5.0);
        let first = store.take_update();
        assert_eq!(first.tickers.map(<[_]>::len), Some(3));
        let mut deltas = Vec::new();
        store.take_ticker_deltas(&mut deltas);
        assert!(deltas.is_empty(), "full list supersedes deltas");

        store.update_ticker("B", |t| t.last = 1.0);
        store.update_ticker("B", |t| {
            t.last = 2.0;
            t.volume = 7.0;
        });
        store.update_ticker("C", |t| t.last = 0.0);
        assert!(store.take_update().is_empty());
        store.take_ticker_deltas(&mut deltas);
        let last_and_volume = (1 << 2) | (1 << 9);
        assert_eq!(deltas, [1.0, last_and_volume as f64, 2.0, 7.0]);
        deltas.clear();
        store.take_ticker_deltas(&mut deltas);
        assert!(deltas.is_empty());
    }
}
