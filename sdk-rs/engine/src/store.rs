//! Typed exchange state plus change tracking. Once per flush the engine drains
//! the changes with [`Store::take_pending`] and writes them into the flush
//! frame with [`Store::write`].
use crate::model::{Balance, Loaded, MarginType, Market, Order, OrderStatus, Position, Ticker};
use crate::wire::{self, FrameWriter, loaded_flag, market_flag, section};
use std::collections::HashMap;

#[derive(Default)]
pub struct Store {
    markets: Vec<Market>,
    market_by_symbol: HashMap<Box<str>, u32>,
    market_by_id: HashMap<Box<str>, u32>,
    tickers: Vec<Ticker>,
    ticker_by_market: Vec<u32>,
    tickers_without_market: HashMap<String, u32>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    balance: Balance,
    /// Per-market account settings, keyed by market index.
    settings: HashMap<u32, MarketSetting>,
    hedged: bool,
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

/// The account's leverage and margin type on one market.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarketSetting {
    pub leverage: f64,
    pub margin_type: Option<MarginType>,
    pub buy_leverage: Option<f64>,
    pub sell_leverage: Option<f64>,
}

#[derive(Default)]
struct Changes {
    markets: bool,
    tickers_replaced: bool,
    orders: bool,
    positions: bool,
    balance: bool,
    settings: bool,
    status: bool,
}

/// The changes drained for one frame.
#[derive(Default)]
pub struct Pending {
    changes: Changes,
    /// `(ticker index, changed-field mask)`.
    ticker_deltas: Vec<(u32, u16)>,
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

    /// The market's row in the most recent `MARKETS` frame section.
    pub fn market_index(&self, symbol: &str) -> Option<u32> {
        self.market_by_symbol.get(symbol).copied()
    }

    pub fn market_index_by_id(&self, id: &str) -> Option<u32> {
        self.market_by_id.get(id).copied()
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
            .map(|(index, market)| (market.symbol.clone().into_boxed_str(), index as u32))
            .collect();
        self.market_by_id = markets
            .iter()
            .enumerate()
            .map(|(index, market)| (market.id.clone().into_boxed_str(), index as u32))
            .collect();
        self.markets = markets;
        self.rebuild_ticker_index();
        self.changes.markets = true;
        self.set_loaded(|loaded| loaded.markets = true);
    }

    pub fn tickers(&self) -> &[Ticker] {
        &self.tickers
    }

    pub fn ticker(&self, symbol: &str) -> Option<&Ticker> {
        self.ticker_index(symbol)
            .map(|index| &self.tickers[index as usize])
    }

    pub fn set_tickers(&mut self, tickers: Vec<Ticker>) {
        self.tickers = tickers;
        self.rebuild_ticker_index();
        self.changes.tickers_replaced = true;
        self.ticker_masks = vec![0; self.tickers.len()];
        self.dirty_tickers.clear();
        self.set_loaded(|loaded| loaded.tickers = true);
    }

    fn ticker_index(&self, symbol: &str) -> Option<u32> {
        match self.market_index(symbol) {
            Some(market) => self
                .ticker_by_market
                .get(market as usize)
                .copied()
                .filter(|&index| index != wire::NONE),
            None => self.tickers_without_market.get(symbol).copied(),
        }
    }

    fn rebuild_ticker_index(&mut self) {
        self.ticker_by_market = vec![wire::NONE; self.markets.len()];
        self.tickers_without_market = HashMap::new();
        for (index, ticker) in self.tickers.iter().enumerate() {
            if let Some(&market) = self.market_by_symbol.get(ticker.symbol.as_str()) {
                self.ticker_by_market[market as usize] = index as u32;
            } else {
                self.tickers_without_market
                    .insert(ticker.symbol.clone(), index as u32);
            }
        }
    }

    /// Mutate one ticker in place; changed fields go out with the next deltas.
    pub fn update_ticker(&mut self, symbol: &str, update: impl FnOnce(&mut Ticker)) {
        let Some(index) = self.ticker_index(symbol) else {
            return;
        };
        self.update_ticker_at(index, update);
    }

    /// Mutate a row of the current ticker table; rebuild cached indexes on replacement.
    pub fn update_ticker_at(&mut self, index: u32, update: impl FnOnce(&mut Ticker)) {
        let Some(ticker) = self.tickers.get_mut(index as usize) else {
            return;
        };
        #[cfg(debug_assertions)]
        let symbol = ticker.symbol.clone();
        let before = ticker_values(ticker);
        update(ticker);
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            ticker.symbol, symbol,
            "ticker identity changes require set_tickers"
        );
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

    pub fn orders(&self) -> &[Order] {
        &self.orders
    }

    pub fn order(&self, id: &str) -> Option<&Order> {
        self.orders.iter().find(|order| order.id == id)
    }

    /// Replace every order with a venue snapshot.
    pub fn set_orders(&mut self, orders: Vec<Order>) {
        self.orders = orders;
        self.changes.orders = true;
        self.set_loaded(|loaded| loaded.orders = true);
    }

    /// Insert or replace by id. A closed or canceled row stays, with its
    /// status, until the next snapshot replaces the list.
    pub fn upsert_order(&mut self, order: Order) {
        match self
            .orders
            .iter()
            .position(|existing| existing.id == order.id)
        {
            Some(index) => self.orders[index] = order,
            None => self.orders.push(order),
        }
        self.changes.orders = true;
    }

    /// Mutate one order in place; returns whether it exists.
    pub fn update_order(&mut self, id: &str, update: impl FnOnce(&mut Order)) -> bool {
        let Some(order) = self.orders.iter_mut().find(|order| order.id == id) else {
            return false;
        };
        update(order);
        self.changes.orders = true;
        true
    }

    pub fn set_order_status(&mut self, id: &str, status: OrderStatus) -> bool {
        self.update_order(id, |order| order.status = status)
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

    pub fn position(&self, id: &str) -> Option<&Position> {
        self.positions.iter().find(|p| p.id == id)
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

    /// Change positions in place; rows left with zero contracts are removed.
    pub fn update_positions(&mut self, mut update: impl FnMut(&mut Position)) {
        for position in &mut self.positions {
            update(position);
        }
        self.positions.retain(|p| p.contracts != 0.0);
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

    pub fn setting(&self, symbol: &str) -> Option<MarketSetting> {
        let market = self.market_index(symbol)?;
        self.settings.get(&market).copied()
    }

    /// Replace every market's settings.
    pub fn set_settings(&mut self, settings: impl IntoIterator<Item = (u32, MarketSetting)>) {
        self.settings = settings.into_iter().collect();
        self.changes.settings = true;
    }

    pub fn update_setting(&mut self, symbol: &str, update: impl FnOnce(&mut MarketSetting)) {
        let Some(market) = self.market_index(symbol) else {
            return;
        };
        let setting = self.settings.entry(market).or_insert(MarketSetting {
            leverage: f64::NAN,
            margin_type: None,
            ..MarketSetting::default()
        });
        let before = *setting;
        update(setting);
        if *setting != before {
            self.changes.settings = true;
        }
    }

    pub fn hedged(&self) -> bool {
        self.hedged
    }

    pub fn set_hedged(&mut self, hedged: bool) {
        if hedged != self.hedged {
            self.hedged = hedged;
            self.changes.status = true;
        }
    }

    /// Change market rows in place (indexes and symbols must not change).
    pub fn update_markets(&mut self, mut update: impl FnMut(&mut Market)) {
        for market in &mut self.markets {
            update(market);
        }
        self.changes.markets = true;
    }

    pub fn loaded(&self) -> Loaded {
        self.loaded
    }

    pub fn set_latency(&mut self, latency: f64) {
        if latency != self.latency {
            self.latency = latency;
            self.changes.status = true;
        }
    }

    fn set_loaded(&mut self, update: impl FnOnce(&mut Loaded)) {
        let before = self.loaded;
        update(&mut self.loaded);
        if self.loaded != before {
            self.changes.status = true;
        }
    }

    /// Every section, for a snapshot frame; change tracking is left alone.
    pub fn everything(&self) -> Pending {
        Pending {
            changes: Changes {
                markets: true,
                tickers_replaced: true,
                orders: true,
                positions: true,
                balance: true,
                settings: true,
                status: true,
            },
            ticker_deltas: Vec::new(),
        }
    }

    /// Drain change tracking for one frame.
    pub fn take_pending(&mut self) -> Pending {
        let changes = std::mem::take(&mut self.changes);
        let mut ticker_deltas = Vec::with_capacity(self.dirty_tickers.len());
        for index in self.dirty_tickers.drain(..) {
            let mask = std::mem::take(&mut self.ticker_masks[index as usize]);
            // A replaced list already carries every value.
            if !changes.tickers_replaced {
                ticker_deltas.push((index, mask));
            }
        }
        Pending {
            changes,
            ticker_deltas,
        }
    }

    /// Write the drained changes as frame sections.
    pub fn write<'a>(&'a self, pending: &Pending, w: &mut FrameWriter<'a>) {
        let changes = &pending.changes;
        if changes.markets || changes.tickers_replaced {
            self.reserve_tables(pending, w);
        }
        if changes.markets {
            self.write_markets(w);
        }
        if changes.tickers_replaced {
            self.write_tickers(w);
        } else if !pending.ticker_deltas.is_empty() {
            w.begin(section::TICKER_DELTAS);
            for &(index, mask) in &pending.ticker_deltas {
                let values = ticker_values(&self.tickers[index as usize]);
                w.f64s([index as f64, mask as f64]);
                for (bit, value) in values.iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        w.f64(*value);
                    }
                }
            }
            w.end();
        }
        if changes.orders {
            self.write_orders(w);
        }
        if changes.positions {
            self.write_positions(w);
        }
        if changes.balance {
            let b = &self.balance;
            w.begin(section::BALANCE);
            w.f64s([b.used, b.free, b.total, b.upnl]);
            w.end();
        }
        if changes.settings {
            self.write_settings(w);
        }
        if changes.status {
            let l = self.loaded;
            let loaded = [
                (l.balance, loaded_flag::BALANCE),
                (l.orders, loaded_flag::ORDERS),
                (l.markets, loaded_flag::MARKETS),
                (l.tickers, loaded_flag::TICKERS),
                (l.positions, loaded_flag::POSITIONS),
            ]
            .iter()
            .filter(|(set, _)| *set)
            .fold(0, |bits, (_, flag)| bits | flag);
            w.begin(section::STATUS);
            let flags = if self.hedged {
                wire::status_flag::HEDGED
            } else {
                0
            };
            w.u32s([loaded, flags]);
            w.f64(self.latency);
            w.end();
        }
    }

    fn market_ref(&self, symbol: &str) -> u32 {
        self.market_index(symbol).unwrap_or(wire::NONE)
    }

    fn reserve_tables(&self, pending: &Pending, w: &mut FrameWriter<'_>) {
        fn table(rows: usize, words: usize, floats: usize) -> usize {
            8 + (4 + rows * words * 4).next_multiple_of(8) + rows * floats * 8
        }
        let changes = &pending.changes;
        let mut bytes = 0;
        let mut strings = 0;
        let mut indexed_strings = 0;
        let mut string_refs = 0;
        let mut string_bytes = 0;
        if changes.markets {
            bytes += table(self.markets.len(), 5, 10);
            // Most market tables share one quote. Other quotes can grow normally.
            strings += self.markets.len() * 3 + usize::from(!self.markets.is_empty());
            indexed_strings += usize::from(!self.markets.is_empty());
            string_refs += self.markets.len() * 3;
            string_bytes += self
                .markets
                .iter()
                .map(|m| m.id.len() + m.symbol.len() + m.base.len())
                .sum::<usize>();
            if let Some(first) = self.markets.first() {
                string_refs += 1;
                string_bytes += first.quote.len();
                for market in self
                    .markets
                    .iter()
                    .skip(1)
                    .filter(|m| m.quote != first.quote)
                {
                    string_refs += 1;
                    string_bytes += market.quote.len();
                }
            }
        }
        if changes.tickers_replaced {
            bytes += table(self.tickers.len(), 1, wire::TICKER_COLUMNS);
        } else if !pending.ticker_deltas.is_empty() {
            bytes += 8 + pending
                .ticker_deltas
                .iter()
                .map(|(_, mask)| (2 + mask.count_ones() as usize) * 8)
                .sum::<usize>();
        }
        if changes.orders {
            bytes += table(self.orders.len(), 5, 7);
            for order in &self.orders {
                for value in [
                    Some(order.id.as_str()),
                    order.client_order_id.as_deref(),
                    order.parent_id.as_deref(),
                ]
                .into_iter()
                .flatten()
                {
                    strings += 1;
                    indexed_strings += 1;
                    string_refs += 1;
                    string_bytes += value.len();
                }
            }
        }
        if changes.positions {
            bytes += table(self.positions.len(), 2, 9);
        }
        if changes.settings {
            let floats = if self
                .settings
                .values()
                .any(|s| s.buy_leverage.is_some() || s.sell_leverage.is_some())
            {
                3
            } else {
                1
            };
            bytes += table(self.settings.len(), 2, floats);
        }
        if changes.balance {
            bytes += 40;
        }
        if changes.status {
            bytes += 24;
        }
        if string_refs > 0 {
            bytes += (8 + 4 + (string_refs + 1) * 4 + string_bytes).next_multiple_of(8);
        }
        w.reserve(bytes, strings, indexed_strings);
    }

    fn write_markets<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let markets = &self.markets;
        w.begin(section::MARKETS);
        w.u32(markets.len() as u32);
        for m in markets {
            let refs = [
                w.unshared_string(&m.id),
                w.unshared_string(&m.symbol),
                w.unshared_string(&m.base),
                w.string(&m.quote),
            ];
            w.u32s(refs);
        }
        w.u32s(markets.iter().map(|m| {
            [
                (Some(m.active), market_flag::ACTIVE),
                (m.is_inverse, market_flag::INVERSE),
                (m.is_spot, market_flag::SPOT),
                (m.is_perpetual, market_flag::PERPETUAL),
                (m.is_expiring, market_flag::EXPIRING),
                (m.is_read_only, market_flag::READ_ONLY),
            ]
            .iter()
            .filter(|(set, _)| *set == Some(true))
            .fold(0, |bits, (_, flag)| bits | flag)
        }));
        w.pad();
        let columns: [fn(&Market) -> f64; 10] = [
            |m| m.precision.amount,
            |m| m.precision.price,
            |m| m.limits.amount.min,
            |m| m.limits.amount.max,
            |m| m.limits.leverage.min,
            |m| m.limits.leverage.max,
            |m| m.limits.price.map_or(f64::NAN, |p| p.min),
            |m| m.contract_size.unwrap_or(f64::NAN),
            |m| m.taker_fee.unwrap_or(f64::NAN),
            |m| m.maker_fee.unwrap_or(f64::NAN),
        ];
        for column in columns {
            w.f64s(markets.iter().map(column));
        }
        w.end();
    }

    fn write_tickers<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let tickers = &self.tickers;
        w.begin(section::TICKERS);
        w.u32(tickers.len() as u32);
        w.u32s(tickers.iter().map(|t| self.market_ref(&t.symbol)));
        w.pad();
        for column in 0..wire::TICKER_COLUMNS {
            w.f64s(tickers.iter().map(|t| ticker_values(t)[column]));
        }
        w.end();
    }

    fn write_orders<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let orders = &self.orders;
        w.begin(section::ORDERS);
        w.u32(orders.len() as u32);
        for o in orders {
            let refs = [
                w.string(&o.id),
                w.optional_string(o.client_order_id.as_deref()),
                w.optional_string(o.parent_id.as_deref()),
            ];
            w.u32s(refs);
        }
        w.u32s(orders.iter().map(|o| self.market_ref(&o.symbol)));
        w.u32s(orders.iter().map(|o| {
            wire::order_enums(
                o.status,
                o.kind,
                o.side,
                o.reduce_only,
                o.close,
                o.time_in_force,
                o.trigger_type,
                o.extra.as_ref().and_then(|e| e.reject_reason),
            )
        }));
        w.pad();
        let columns: [fn(&Order) -> f64; 7] = [
            |o| o.price,
            |o| o.amount,
            |o| o.filled,
            |o| o.remaining,
            |o| o.last_update,
            |o| o.take_profit.unwrap_or(f64::NAN),
            |o| o.stop_loss.unwrap_or(f64::NAN),
        ];
        for column in columns {
            w.f64s(orders.iter().map(column));
        }
        w.end();
    }

    fn write_settings<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let mut rows: Vec<(&u32, &MarketSetting)> = self.settings.iter().collect();
        rows.sort_unstable_by_key(|(market, _)| **market);
        w.begin(section::SETTINGS);
        w.u32(rows.len() as u32);
        w.u32s(rows.iter().map(|(market, _)| **market));
        w.u32s(
            rows.iter()
                .map(|(_, s)| s.margin_type.map_or(0, |m| m as u32 + 1)),
        );
        w.pad();
        w.f64s(rows.iter().map(|(_, s)| s.leverage));
        if rows
            .iter()
            .any(|(_, s)| s.buy_leverage.is_some() || s.sell_leverage.is_some())
        {
            w.f64s(
                rows.iter()
                    .map(|(_, s)| s.buy_leverage.unwrap_or(s.leverage)),
            );
            w.f64s(
                rows.iter()
                    .map(|(_, s)| s.sell_leverage.unwrap_or(s.leverage)),
            );
        }
        w.end();
    }

    fn write_positions<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let positions = &self.positions;
        w.begin(section::POSITIONS);
        w.u32(positions.len() as u32);
        w.u32s(positions.iter().map(|p| self.market_ref(&p.symbol)));
        w.u32s(
            positions
                .iter()
                .map(|p| wire::position_enums(p.side, p.margin_type, p.position_mode)),
        );
        w.pad();
        let columns: [fn(&Position) -> f64; 9] = [
            |p| p.entry_price,
            |p| p.notional,
            |p| p.leverage,
            |p| p.unrealized_pnl,
            |p| p.realized_pnl.unwrap_or(f64::NAN),
            |p| p.contracts,
            |p| p.liquidation_price,
            |p| p.margin_size.unwrap_or(f64::NAN),
            |p| p.last_update.unwrap_or(f64::NAN),
        ];
        for column in columns {
            w.f64s(positions.iter().map(column));
        }
        w.end();
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

    fn market(symbol: &str) -> Market {
        use crate::model::{Limits, MinMax, Precision};
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: symbol.into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax { min: 0.0, max: 1.0 },
                leverage: MinMax {
                    min: 1.0,
                    max: 100.0,
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
    fn ticker_lookups_allow_symbols_without_markets() {
        let mut store = Store::default();
        store.set_markets(vec![market("A"), market("B")]);
        store.set_tickers(vec![ticker("UNKNOWN"), ticker("B")]);
        store.take_pending();

        assert!(store.ticker("A").is_none());
        assert!(store.ticker("ABSENT").is_none());
        store.update_ticker("ABSENT", |_| panic!("missing ticker updated"));
        store.update_ticker("UNKNOWN", |t| t.last = 3.0);
        store.update_ticker("B", |t| t.last = 7.0);
        assert_eq!(store.ticker("UNKNOWN").unwrap().last, 3.0);
        assert_eq!(store.ticker("B").unwrap().last, 7.0);
        assert_eq!(
            store.take_pending().ticker_deltas,
            [(0, 1 << 2), (1, 1 << 2)]
        );
    }

    #[test]
    fn ticker_duplicates_use_the_last_row_with_or_without_markets() {
        let mut store = Store::default();
        store.set_markets(vec![market("A")]);
        store.set_tickers(vec![ticker("A"), ticker("A"), ticker("X"), ticker("X")]);
        store.take_pending();
        store.update_ticker("A", |t| t.last = 4.0);
        store.update_ticker("X", |t| t.last = 8.0);

        assert_eq!(store.ticker("A").unwrap().last, 4.0);
        assert_eq!(store.ticker("X").unwrap().last, 8.0);
        assert_eq!(store.tickers()[0].last, 0.0);
        assert_eq!(store.tickers()[2].last, 0.0);
        assert_eq!(
            store.take_pending().ticker_deltas,
            [(1, 1 << 2), (3, 1 << 2)]
        );
        store.set_tickers(vec![ticker("X")]);
        assert!(store.ticker("A").is_none());
        assert!(store.ticker("X").is_some());
        store.set_tickers(vec![]);
        assert!(store.ticker("X").is_none());
    }

    #[test]
    fn market_replacement_preserves_existing_ticker_lookups_and_deltas() {
        let mut store = Store::default();
        store.set_tickers(vec![ticker("A"), ticker("B"), ticker("X")]);
        store.set_markets(vec![market("A"), market("B")]);
        store.take_pending();

        store.update_ticker("A", |t| t.last = 1.0);
        store.set_markets(vec![market("X"), market("B"), market("NEW")]);
        store.update_ticker("B", |t| t.last = 2.0);
        store.update_ticker("X", |t| t.last = 3.0);
        assert_eq!(store.ticker("A").unwrap().last, 1.0);
        assert_eq!(store.ticker("B").unwrap().last, 2.0);
        assert_eq!(store.ticker("X").unwrap().last, 3.0);
        assert!(store.ticker("NEW").is_none());
        assert_eq!(
            store.take_pending().ticker_deltas,
            [(0, 1 << 2), (1, 1 << 2), (2, 1 << 2)]
        );

        store.set_markets(vec![]);
        store.update_ticker("X", |t| t.last = 4.0);
        store.set_markets(vec![market("B"), market("X"), market("A")]);
        assert_eq!(store.ticker("A").unwrap().last, 1.0);
        assert_eq!(store.ticker("B").unwrap().last, 2.0);
        assert_eq!(store.ticker("X").unwrap().last, 4.0);
    }

    #[test]
    fn ticker_lookup_tracks_unsorted_replacements_and_last_duplicate() {
        let mut store = Store::default();
        let mut duplicate = ticker("A");
        duplicate.last = 9.;
        store.set_tickers(vec![ticker("C"), ticker("A"), ticker("B"), duplicate]);
        assert_eq!(store.ticker("A").unwrap().last, 9.);
        store.update_ticker("B", |row| row.last = 5.);
        assert_eq!(store.tickers()[2].last, 5.);
        assert!(store.ticker("missing").is_none());
        store.set_tickers(vec![ticker("B")]);
        assert!(store.ticker("A").is_none());
        assert_eq!(store.ticker("B").unwrap().last, 0.);
    }

    #[test]
    fn indexed_updates_match_symbol_updates_and_ignore_missing_rows() {
        let mut by_symbol = Store::default();
        let mut by_index = Store::default();
        for store in [&mut by_symbol, &mut by_index] {
            store.set_tickers(vec![ticker("C"), ticker("A"), ticker("B")]);
            store.take_pending();
        }
        by_symbol.update_ticker("A", |row| {
            row.last = 7.;
            row.volume = 3.;
        });
        by_index.update_ticker_at(1, |row| {
            row.last = 7.;
            row.volume = 3.;
        });
        by_index.update_ticker_at(u32::MAX, |_| panic!("invalid row reached closure"));
        assert_eq!(
            by_symbol.take_pending().ticker_deltas,
            by_index.take_pending().ticker_deltas
        );
        assert_eq!(by_index.tickers()[1].last, 7.);
    }

    #[test]
    fn drains_only_changed_ticker_fields_after_the_initial_list() {
        let mut store = Store::default();
        store.set_tickers(vec![ticker("A"), ticker("B"), ticker("C")]);
        store.update_ticker("A", |t| t.bid = 5.0);
        let first = store.take_pending();
        assert!(first.changes.tickers_replaced);
        assert!(
            first.ticker_deltas.is_empty(),
            "full list supersedes deltas"
        );

        store.update_ticker("B", |t| t.last = 1.0);
        store.update_ticker("B", |t| {
            t.last = 2.0;
            t.volume = 7.0;
        });
        store.update_ticker("C", |t| t.last = 0.0);
        let second = store.take_pending();
        assert!(!second.changes.tickers_replaced);
        assert_eq!(second.ticker_deltas, [(1, (1 << 2) | (1 << 9))]);
        assert!(store.take_pending().ticker_deltas.is_empty());
    }
}
