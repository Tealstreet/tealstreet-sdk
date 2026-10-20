//! Typed exchange state plus change tracking. Once per flush the engine drains
//! the changes with [`Store::take_pending`] and writes them into the flush
//! frame with [`Store::write`].
use crate::model::{
    Balance, CoinBalance, Loaded, MarginType, Market, Order, OrderStatus, Position, Ticker,
};
use crate::wire::{self, FrameWriter, loaded_flag, market_flag, section};
use std::collections::HashMap;

#[derive(Default)]
pub struct Store {
    markets: Vec<Market>,
    market_by_symbol: HashMap<Box<str>, u32>,
    // Mixed IDs use sorted row indexes borrowing market identity. When every
    // instrument ID is its symbol, the symbol index serves both.
    market_by_id: Option<Vec<u32>>,
    tickers: Vec<Ticker>,
    // Sorted row indexes borrow ticker identity instead of copying every symbol.
    ticker_by_symbol: Vec<u32>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    position_ids: bool,
    balance: Balance,
    coin_balances: Option<Vec<CoinBalance>>,
    /// Per-market account settings, keyed by market index.
    settings: HashMap<u32, MarketSetting>,
    hedged: bool,
    loaded: Loaded,
    latency: f64,
    changes: Changes,
    /// Changed-field bits per ticker, in [`TICKER_FIELDS`] order.
    ticker_masks: Vec<u16>,
    dirty_tickers: Vec<u32>,
    ticker_deltas_spare: Vec<(u32, u16)>,
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarketSetting {
    pub leverage: f64,
    pub margin_type: Option<MarginType>,
    pub buy_leverage: Option<f64>,
    pub sell_leverage: Option<f64>,
}

impl Default for MarketSetting {
    fn default() -> Self {
        Self {
            leverage: f64::NAN,
            margin_type: None,
            buy_leverage: None,
            sell_leverage: None,
        }
    }
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
        match &self.market_by_id {
            None => self.market_by_symbol.get(id).copied(),
            Some(indices) => indices
                .binary_search_by(|&index| self.markets[index as usize].id.as_str().cmp(id))
                .ok()
                .map(|slot| indices[slot]),
        }
    }

    pub fn market_by_id(&self, id: &str) -> Option<&Market> {
        self.market_index_by_id(id)
            .map(|index| &self.markets[index as usize])
    }

    pub fn set_markets(&mut self, mut markets: Vec<Market>) {
        // Rows are replaced wholesale, so collection growth capacity is unused.
        markets.shrink_to_fit();
        self.market_by_symbol = markets
            .iter()
            .enumerate()
            .map(|(index, market)| (market.symbol.clone().into_boxed_str(), index as u32))
            .collect();
        self.market_by_id = markets
            .iter()
            .any(|market| market.id != market.symbol)
            .then(|| {
                let mut indices: Vec<u32> = (0..markets.len() as u32).collect();
                indices.sort_unstable_by(|&a, &b| {
                    markets[a as usize]
                        .id
                        .cmp(&markets[b as usize].id)
                        .then_with(|| b.cmp(&a))
                });
                indices.dedup_by(|a, b| markets[*a as usize].id == markets[*b as usize].id);
                indices
            });
        self.markets = markets;
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

    pub fn set_tickers(&mut self, mut tickers: Vec<Ticker>) {
        // Updates mutate existing rows; only replacement changes the row count.
        tickers.shrink_to_fit();
        self.tickers = tickers;
        self.ticker_by_symbol = (0..self.tickers.len() as u32).collect();
        self.ticker_by_symbol.sort_unstable_by(|&a, &b| {
            self.tickers[a as usize]
                .symbol
                .cmp(&self.tickers[b as usize].symbol)
                .then_with(|| b.cmp(&a))
        });
        self.ticker_by_symbol
            .dedup_by(|a, b| self.tickers[*a as usize].symbol == self.tickers[*b as usize].symbol);
        self.changes.tickers_replaced = true;
        self.ticker_masks = vec![0; self.tickers.len()];
        self.dirty_tickers.clear();
        self.set_loaded(|loaded| loaded.tickers = true);
    }

    fn ticker_index(&self, symbol: &str) -> Option<u32> {
        self.ticker_by_symbol
            .binary_search_by(|&index| self.tickers[index as usize].symbol.as_str().cmp(symbol))
            .ok()
            .map(|slot| self.ticker_by_symbol[slot])
    }

    /// Mutate ticker values, keeping its symbol identity; changed fields go out as deltas.
    pub fn update_ticker(&mut self, symbol: &str, update: impl FnOnce(&mut Ticker)) {
        let Some(index) = self.ticker_index(symbol) else {
            return;
        };
        self.update_ticker_at(index, update);
    }

    /// Update by venue instrument ID without allocating a copy of its symbol.
    pub fn update_ticker_by_market_id(&mut self, id: &str, update: impl FnOnce(&mut Ticker)) {
        let Some(market) = self.market_index_by_id(id) else {
            return;
        };
        let Some(index) = self.ticker_index(&self.markets[market as usize].symbol) else {
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

    /// `None` means this venue has never supplied collateral holdings. An empty
    /// slice explicitly clears previously supplied holdings in the consumer.
    pub fn coin_balances(&self) -> Option<&[CoinBalance]> {
        self.coin_balances.as_deref()
    }

    pub fn set_coin_balances(&mut self, coins: Vec<CoinBalance>) {
        if self.coin_balances.as_ref() != Some(&coins) {
            self.coin_balances = Some(coins);
            self.changes.balance = true;
        }
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
        let setting = self.settings.entry(market).or_default();
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

    /// Change market values in place; IDs and symbols must remain unchanged.
    pub fn update_markets(&mut self, mut update: impl FnMut(&mut Market)) {
        for market in &mut self.markets {
            #[cfg(debug_assertions)]
            let identity = (market.id.clone(), market.symbol.clone());
            update(market);
            #[cfg(debug_assertions)]
            debug_assert_eq!(
                (&market.id, &market.symbol),
                (&identity.0, &identity.1),
                "market identity changes require set_markets"
            );
        }
        self.changes.markets = true;
    }

    /// Preserve venue-defined position identity when symbol/mode cannot derive it.
    pub fn retain_position_ids(&mut self) {
        if !self.position_ids {
            self.position_ids = true;
            self.changes.positions = true;
        }
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
        let mut ticker_deltas = std::mem::take(&mut self.ticker_deltas_spare);
        if !changes.tickers_replaced {
            ticker_deltas.reserve(self.dirty_tickers.len());
        }
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

    /// Return drained row storage after its frame has finished serialization.
    pub(crate) fn recycle_pending(&mut self, mut pending: Pending) {
        pending.ticker_deltas.clear();
        self.ticker_deltas_spare = pending.ticker_deltas;
    }

    /// Write the drained changes as frame sections.
    pub fn write<'a>(&'a self, pending: &Pending, w: &mut FrameWriter<'a>) {
        let changes = &pending.changes;
        w.reserve(self.frame_size_hint(pending));
        w.reserve_strings(
            if changes.markets {
                // Identity IDs and symbols share a string-table entry.
                let per_market = if self.market_by_id.is_none() { 3 } else { 4 };
                per_market * self.markets.len()
            } else {
                0
            } + if changes.orders {
                3 * self.orders.len()
            } else {
                0
            },
        );
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
            if let Some(coins) = &self.coin_balances {
                w.u32(coins.len() as u32);
                for coin in coins {
                    let base = w.string(&coin.base);
                    w.u32(base);
                }
                w.pad();
                w.f64s(coins.iter().map(|coin| coin.amount));
                w.f64s(coins.iter().map(|coin| coin.notional));
            }
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

    pub(crate) fn frame_size_hint(&self, pending: &Pending) -> usize {
        let changes = &pending.changes;
        let mut size = 0;
        let mut strings = 0;
        let mut string_bytes = 0;
        if changes.markets {
            let count = self.markets.len();
            size += 8 + (4 + 20 * count).next_multiple_of(8) + 80 * count;
            for market in &self.markets {
                for value in [&market.id, &market.base, &market.quote] {
                    strings += 1;
                    string_bytes += value.len();
                }
                if market.id != market.symbol {
                    strings += 1;
                    string_bytes += market.symbol.len();
                }
            }
        }
        if changes.tickers_replaced {
            let count = self.tickers.len();
            size += 8 + (4 + 4 * count).next_multiple_of(8) + 88 * count;
        } else if !pending.ticker_deltas.is_empty() {
            size += 8 + pending
                .ticker_deltas
                .iter()
                .map(|(_, mask)| 8 * (2 + mask.count_ones() as usize))
                .sum::<usize>();
        }
        if changes.orders {
            let count = self.orders.len();
            size += 8 + (4 + 20 * count).next_multiple_of(8) + 56 * count;
            if self.orders.iter().any(|order| {
                order
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.remaining_native)
                    .is_some()
            }) {
                size += 8 * count;
            }
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
                    string_bytes += value.len();
                }
            }
        }
        if changes.positions {
            let count = self.positions.len();
            size += 8 + (4 + 8 * count).next_multiple_of(8) + 72 * count;
            if self.position_ids {
                size += (4 * count).next_multiple_of(8);
                strings += count;
                string_bytes += self
                    .positions
                    .iter()
                    .map(|position| position.id.len())
                    .sum::<usize>();
            }
        }
        if changes.balance {
            size += 40;
            if let Some(coins) = &self.coin_balances {
                let count = coins.len();
                size += (4 + 4 * count).next_multiple_of(8) + 16 * count;
                strings += count;
                string_bytes += coins.iter().map(|coin| coin.base.len()).sum::<usize>();
            }
        }
        if changes.settings {
            let count = self.settings.len();
            size += 8 + (4 + 8 * count).next_multiple_of(8) + 8 * count;
            if self
                .settings
                .values()
                .any(|setting| setting.buy_leverage.is_some() || setting.sell_leverage.is_some())
            {
                size += 16 * count;
            }
        }
        if changes.status {
            size += 24;
        }
        if strings > 0 {
            // Duplicate references make this a small upper bound; registration
            // still happens in section order and retains the existing wire IDs.
            size += 8 + (8 + 4 * strings + string_bytes).next_multiple_of(8);
        }
        size
    }

    fn market_ref(&self, symbol: &str) -> u32 {
        self.market_index(symbol).unwrap_or(wire::NONE)
    }

    fn write_markets<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let markets = &self.markets;
        w.begin(section::MARKETS);
        w.u32(markets.len() as u32);
        for m in markets {
            let refs = [
                w.string(&m.id),
                w.string(&m.symbol),
                w.string(&m.base),
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
        // Native leavesQty cannot be recovered from rounded base quantities or
        // amount=0 full-position stops. Older readers ignore this optional tail.
        if orders
            .iter()
            .any(|o| o.extra.as_ref().and_then(|e| e.remaining_native).is_some())
        {
            w.f64s(orders.iter().map(|o| {
                o.extra
                    .as_ref()
                    .and_then(|e| e.remaining_native)
                    .unwrap_or(f64::NAN)
            }));
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
        if self.position_ids {
            for p in positions {
                let id = w.string(&p.id);
                w.u32(id);
            }
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

    fn market(id: &str, symbol: &str) -> Market {
        use crate::model::{Limits, MinMax, Precision};
        Market {
            id: id.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: symbol.into(),
            quote: "USD".into(),
            active: true,
            precision: Precision {
                amount: 1.0,
                price: 1.0,
            },
            limits: Limits {
                amount: MinMax {
                    min: 1.0,
                    max: 10.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 10.0,
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
    fn market_string_bounds_preserve_identity_and_mixed_id_frames_without_growth() {
        const COUNT: usize = 912;
        for (mixed_ids, shared_base_quote) in
            [(false, true), (true, true), (false, false), (true, false)]
        {
            let mut store = Store::default();
            store.set_markets(
                (0..COUNT)
                    .map(|index| {
                        let symbol = format!("S{index:04}USDT");
                        let id = if mixed_ids && index % 2 == 0 {
                            format!("contract{index}")
                        } else {
                            symbol.clone()
                        };
                        let mut row = market(&id, &symbol);
                        row.base = if shared_base_quote {
                            "BASE".into()
                        } else {
                            format!("base{index}")
                        };
                        row.quote = if shared_base_quote {
                            "USDT".into()
                        } else {
                            format!("quote{index}")
                        };
                        row
                    })
                    .collect(),
            );
            let mut pending = store.take_pending();
            pending.changes.status = false;
            let size = 16 + store.frame_size_hint(&pending);
            let buffer = Vec::with_capacity(size);
            let capacity = buffer.capacity();
            let mut writer = FrameWriter::new(buffer, 1.0);
            store.write(&pending, &mut writer);
            let frame = writer.finish();
            assert!(frame.len() <= size);
            if !shared_base_quote {
                assert_eq!(frame.len(), size, "unique strings make the bound exact");
            }
            assert_eq!(frame.capacity(), capacity, "hint must avoid buffer growth");

            let mut unreserved = FrameWriter::new(Vec::new(), 1.0);
            store.write_markets(&mut unreserved);
            assert_eq!(frame, unreserved.finish(), "reservation must preserve wire");
            let payload = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::MARKETS)
                .unwrap()
                .1;
            for index in 0..COUNT {
                let refs = &payload[4 + 16 * index..12 + 16 * index];
                assert_eq!(refs[..4] == refs[4..], !mixed_ids || index % 2 != 0);
            }
            let strings = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::STRINGS)
                .unwrap()
                .1;
            let count = u32::from_le_bytes(strings[..4].try_into().unwrap()) as usize;
            assert_eq!(
                count,
                COUNT
                    + if shared_base_quote { 2 } else { 2 * COUNT }
                    + if mixed_ids { COUNT / 2 } else { 0 }
            );
        }
    }

    #[test]
    fn settings_frame_hint_reserves_both_optional_leverage_columns() {
        for count in 0usize..=3 {
            for legs in 0..4 {
                let mut store = Store::default();
                store.set_settings((0..count).rev().map(|index| {
                    (
                        index as u32,
                        MarketSetting {
                            leverage: 20.0 + index as f64,
                            buy_leverage: (index == 0 && legs & 1 != 0).then_some(10.0),
                            sell_leverage: (index == 0 && legs & 2 != 0).then_some(15.0),
                            ..MarketSetting::default()
                        },
                    )
                }));
                let pending = store.take_pending();
                let size = 16 + store.frame_size_hint(&pending);
                let buffer = Vec::with_capacity(size);
                let capacity = buffer.capacity();
                let mut writer = FrameWriter::new(buffer, 1.0);
                store.write(&pending, &mut writer);
                let frame = writer.finish();
                assert_eq!(frame.len(), size, "rows={count}, legs={legs}");
                assert_eq!(frame.capacity(), capacity, "reservation must avoid growth");

                let (kind, payload) = wire::sections(&frame).next().unwrap();
                assert_eq!(kind, section::SETTINGS);
                let values: Vec<f64> = payload[(4 + 8 * count).next_multiple_of(8)..]
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|bytes| f64::from_le_bytes(*bytes))
                    .collect();
                let mut expected: Vec<f64> = (0..count).map(|i| 20.0 + i as f64).collect();
                if count > 0 && legs != 0 {
                    expected.extend((0..count).map(|i| {
                        if i == 0 && legs & 1 != 0 {
                            10.0
                        } else {
                            20.0 + i as f64
                        }
                    }));
                    expected.extend((0..count).map(|i| {
                        if i == 0 && legs & 2 != 0 {
                            15.0
                        } else {
                            20.0 + i as f64
                        }
                    }));
                }
                assert_eq!(values, expected);
            }
        }
    }

    #[test]
    fn replacement_tables_release_spare_capacity_without_changing_row_identity() {
        let mut store = Store::default();
        let mut markets = Vec::with_capacity(64);
        markets.extend([market("X", "B"), market("Y", "A"), market("X", "A")]);
        let mut tickers = Vec::with_capacity(64);
        let mut last_a = ticker("A");
        last_a.last = 9.0;
        tickers.extend([ticker("B"), ticker("A"), last_a]);
        store.set_markets(markets);
        store.set_tickers(tickers);

        assert!(store.markets.capacity() < 64);
        assert!(store.tickers.capacity() < 64);
        assert_eq!(store.markets()[0].symbol, "B");
        assert_eq!(store.markets()[1].id, "Y");
        assert_eq!(store.market_index("A"), Some(2));
        assert_eq!(store.market_index_by_id("X"), Some(2));
        assert_eq!(store.tickers()[1].last, 0.0);
        assert_eq!(store.ticker("A").unwrap().last, 9.0);
        store.take_pending();
        store.update_ticker_by_market_id("X", |row| row.last = 10.0);
        assert_eq!(store.take_pending().ticker_deltas, [(2, 1 << 2)]);

        store.update_ticker("B", |row| row.bid = 5.0);
        store.set_markets(Vec::with_capacity(64));
        store.set_tickers(Vec::with_capacity(64));
        assert_eq!(store.markets.capacity(), 0);
        assert_eq!(store.tickers.capacity(), 0);
        assert!(store.market_by_id("X").is_none());
        assert!(store.ticker("A").is_none());
        assert!(store.ticker_masks.is_empty());
        assert!(store.dirty_tickers.is_empty());
        assert!(store.take_pending().ticker_deltas.is_empty());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "market identity changes require set_markets")]
    fn market_updates_cannot_change_ids_without_rebuilding_indexes() {
        let mut store = Store::default();
        store.set_markets(vec![market("X", "A"), market("Y", "B")]);
        store.update_markets(|market| market.id = "Z".into());
    }

    #[test]
    fn instrument_updates_follow_symbol_mapping_after_replacement_and_reordering() {
        let mut store = Store::default();
        store.set_markets(vec![market("A-SWAP", "A"), market("B-SWAP", "B")]);
        store.set_tickers(vec![ticker("B"), ticker("A")]);
        store.take_pending();
        store.update_ticker_by_market_id("A-SWAP", |ticker| ticker.last = 7.0);
        store.update_ticker_by_market_id("missing", |_| panic!("missing ID must not update"));
        assert_eq!(store.ticker("A").unwrap().last, 7.0);
        assert_eq!(store.ticker("B").unwrap().last, 0.0);
        assert_eq!(store.take_pending().ticker_deltas, [(1, 1 << 2)]);

        store.set_markets(vec![market("C-SWAP", "A"), market("D-SWAP", "B")]);
        store.set_tickers(vec![ticker("A"), ticker("B")]);
        store.take_pending();
        store.update_ticker_by_market_id("A-SWAP", |_| panic!("old ID must not update"));
        store.update_ticker_by_market_id("C-SWAP", |ticker| ticker.bid = 9.0);
        assert_eq!(store.ticker("A").unwrap().bid, 9.0);
        assert_eq!(store.take_pending().ticker_deltas, [(0, 1)]);

        store.set_tickers(vec![ticker("B")]);
        store.take_pending();
        store.update_ticker_by_market_id("C-SWAP", |_| panic!("missing ticker must not update"));
        assert!(store.take_pending().ticker_deltas.is_empty());
    }

    #[test]
    fn instrument_lookup_preserves_identity_mixed_and_duplicate_id_semantics() {
        let mut store = Store::default();
        let mut last_a = market("A", "A");
        last_a.precision.price = 2.0;
        store.set_markets(vec![market("A", "A"), market("B", "B"), last_a]);
        assert!(store.market_by_id.is_none());
        assert_eq!(store.market_by_id("A").unwrap().precision.price, 2.0);
        assert_eq!(store.market("A").unwrap().precision.price, 2.0);
        store.set_tickers(vec![ticker("A"), ticker("B")]);
        store.take_pending();
        store.update_ticker_by_market_id("A", |ticker| ticker.bid = 3.0);
        assert_eq!(store.take_pending().ticker_deltas, [(0, 1)]);

        store.set_markets(vec![
            market("Z", "C"),
            market("X", "A"),
            market("Y", "D"),
            market("X", "B"),
        ]);
        assert!(store.market_by_id.is_some());
        assert_eq!(store.market_index_by_id("Z"), Some(0));
        assert_eq!(store.market_index_by_id("Y"), Some(2));
        assert_eq!(store.market_index_by_id("X"), Some(3));
        assert_eq!(store.market_by_id("X").unwrap().symbol, "B");
        assert!(
            store.market_by_id("A").is_none(),
            "mixed IDs do not fall back per entry"
        );
        store.take_pending();
        store.update_ticker_by_market_id("X", |ticker| ticker.last = 4.0);
        assert_eq!(store.ticker("B").unwrap().last, 4.0);
        assert_eq!(store.take_pending().ticker_deltas, [(1, 1 << 2)]);

        store.set_markets(vec![market("C", "C")]);
        assert!(store.market_by_id.is_none());
        assert!(store.market_by_id("X").is_none());
        assert_eq!(store.market_by_id("C").unwrap().symbol, "C");
        store.set_markets(Vec::new());
        assert!(store.market_by_id.is_none());
        assert!(store.market_by_id("C").is_none());
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

    #[test]
    fn scalar_settings_keep_the_original_payload_without_side_columns() {
        let mut store = Store::default();
        store.set_settings([(
            0,
            MarketSetting {
                leverage: 15.0,
                margin_type: Some(MarginType::Cross),
                ..MarketSetting::default()
            },
        )]);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write_settings(&mut writer);
        let frame = writer.finish();
        let (_, payload) = crate::wire::sections(&frame).next().unwrap();
        assert_eq!(payload.len(), 24);
        assert_eq!(
            f64::from_le_bytes(payload[16..24].try_into().unwrap()),
            15.0
        );
    }

    #[test]
    fn settings_append_independent_leverages_after_the_original_scalar_column() {
        let mut store = Store::default();
        store.set_settings([
            (
                0,
                MarketSetting {
                    leverage: 20.0,
                    margin_type: Some(MarginType::Cross),
                    buy_leverage: Some(20.0),
                    sell_leverage: Some(30.0),
                },
            ),
            (
                1,
                MarketSetting {
                    leverage: 15.0,
                    ..MarketSetting::default()
                },
            ),
        ]);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write_settings(&mut writer);
        let frame = writer.finish();
        let (_, payload) = crate::wire::sections(&frame).next().unwrap();
        let value = |offset| f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 2);
        assert_eq!((value(24), value(32)), (20.0, 15.0));
        assert_eq!((value(40), value(56)), (20.0, 30.0));
        assert_eq!((value(48), value(64)), (15.0, 15.0));
    }

    fn balance_frame(store: &mut Store) -> Vec<u8> {
        let pending = store.take_pending();
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write(&pending, &mut writer);
        writer.finish()
    }

    #[test]
    fn native_order_remainder_frame_hint_preserves_wire_without_buffer_growth() {
        use crate::model::{OrderExtra, OrderSide, OrderStatus, OrderType};

        for count in [0, 1, 2, 257] {
            for native in [false, true] {
                let mut store = Store::default();
                store.set_orders(
                    (0..count)
                        .map(|index| Order {
                            id: format!("order雪{index}"),
                            parent_id: None,
                            status: OrderStatus::Open,
                            symbol: "BTCUSD".into(),
                            kind: OrderType::StopLoss,
                            side: OrderSide::Sell,
                            price: 54321.0,
                            amount: 0.0,
                            filled: 0.0,
                            remaining: 0.00013,
                            reduce_only: true,
                            close: Some(true),
                            client_order_id: None,
                            time_in_force: None,
                            last_update: 1000.0,
                            trigger_type: None,
                            take_profit: None,
                            stop_loss: None,
                            // A single native value enables the tail for every row.
                            extra: (native && index == count - 1).then_some(OrderExtra {
                                remaining_native: Some(7.0),
                                ..OrderExtra::default()
                            }),
                        })
                        .collect(),
                );
                let pending = store.take_pending();
                let size = 16 + store.frame_size_hint(&pending);
                let buffer = Vec::with_capacity(size);
                let capacity = buffer.capacity();
                let pointer = buffer.as_ptr();
                let mut writer = FrameWriter::new(buffer, 1.0);
                store.write(&pending, &mut writer);
                let frame = writer.finish();
                assert_eq!(frame.len(), size, "rows={count}, native={native}");
                assert_eq!(frame.capacity(), capacity);
                assert_eq!(frame.as_ptr(), pointer);
                let mut plain = FrameWriter::new(Vec::new(), 1.0);
                store.write(&pending, &mut plain);
                assert_eq!(frame, plain.finish());
            }
        }
    }

    #[test]
    fn position_identity_frame_hint_preserves_wire_without_buffer_growth() {
        for count in [0, 1, 2, 257] {
            for ids in [false, true] {
                let mut store = Store::default();
                if ids {
                    store.retain_position_ids();
                }
                store.set_positions(
                    (0..count)
                        .map(|index| Position {
                            id: format!("position雪{index}"),
                            symbol: "BTCUSDC".into(),
                            side: crate::model::PositionSide::Long,
                            entry_price: 100.0,
                            notional: 100.0,
                            leverage: 10.0,
                            margin_type: None,
                            position_mode: None,
                            unrealized_pnl: 0.0,
                            realized_pnl: None,
                            contracts: 1.0,
                            liquidation_price: 50.0,
                            margin_size: None,
                            last_update: None,
                        })
                        .collect(),
                );
                let pending = store.take_pending();
                let size = 16 + store.frame_size_hint(&pending);
                let buffer = Vec::with_capacity(size);
                let capacity = buffer.capacity();
                let mut writer = FrameWriter::new(buffer, 1.0);
                store.write(&pending, &mut writer);
                let frame = writer.finish();
                assert!(frame.len() <= size);
                assert_eq!(frame.capacity(), capacity);
                let mut plain = FrameWriter::new(Vec::new(), 1.0);
                store.write(&pending, &mut plain);
                assert_eq!(frame, plain.finish());
            }
        }
    }

    #[test]
    fn collateral_frame_hint_preserves_wire_without_buffer_growth() {
        for count in [0, 1, 2, 257] {
            let mut store = Store::default();
            store.set_coin_balances(
                (0..count)
                    .map(|index| CoinBalance {
                        base: format!("coin雪{index}"),
                        amount: index as f64,
                        notional: index as f64 * 10.0,
                    })
                    .collect(),
            );
            let pending = store.take_pending();
            let size = 16 + store.frame_size_hint(&pending);
            let buffer = Vec::with_capacity(size);
            let capacity = buffer.capacity();
            let mut writer = FrameWriter::new(buffer, 1.0);
            store.write(&pending, &mut writer);
            let frame = writer.finish();
            assert!(frame.len() <= size);
            assert_eq!(frame.capacity(), capacity);
            let mut plain = FrameWriter::new(Vec::new(), 1.0);
            store.write(&pending, &mut plain);
            assert_eq!(frame, plain.finish());
        }
    }

    #[test]
    fn collateral_tail_preserves_legacy_balance_and_empty_means_clear() {
        let mut store = Store::default();
        let balance = Balance {
            used: 1.0,
            free: 2.0,
            total: 3.0,
            upnl: 4.0,
        };
        assert_eq!(store.coin_balances(), None);
        store.set_balance(balance.clone());
        let legacy = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&legacy)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(payload.len(), 32);
        let scalars = payload.to_vec();

        store.set_coin_balances(vec![]);
        assert_eq!(store.coin_balances(), Some([].as_slice()));
        let cleared = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&cleared)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(payload.len(), 40);
        assert_eq!(&payload[..32], scalars);
        assert_eq!(u32::from_le_bytes(payload[32..36].try_into().unwrap()), 0);
        store.set_coin_balances(vec![]);
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
        );
        store.set_balance(balance);
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
        );
    }

    #[test]
    fn collateral_changes_flush_columns_without_rewriting_aggregate_balance() {
        let mut store = Store::default();
        let coins = vec![
            CoinBalance {
                base: "BTC".into(),
                amount: 0.25,
                notional: 12_500.0,
            },
            CoinBalance {
                base: "ETH".into(),
                amount: 2.0,
                notional: 5_000.0,
            },
        ];
        store.set_coin_balances(coins.clone());
        assert_eq!(store.coin_balances(), Some(coins.as_slice()));
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        let word = |at| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        let value = |at| f64::from_le_bytes(payload[at..at + 8].try_into().unwrap());
        assert_eq!(payload.len(), 80);
        assert_eq!(word(32), 2);
        assert_eq!((word(36), word(40)), (0, 1));
        assert_eq!((value(48), value(56)), (0.25, 2.0));
        assert_eq!((value(64), value(72)), (12_500.0, 5_000.0));
        let (_, strings) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::STRINGS)
            .unwrap();
        assert_eq!(u32::from_le_bytes(strings[..4].try_into().unwrap()), 2);
        assert_eq!(&strings[16..22], b"BTCETH");

        store.set_coin_balances(coins.clone());
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
        );
        let mut changed = coins;
        changed[0].notional = 12_501.0;
        store.set_coin_balances(changed);
        assert!(store.take_pending().changes.balance);
        // A full snapshot must also retain collateral even after dirty state drains.
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write(&store.everything(), &mut writer);
        let snapshot = writer.finish();
        let (_, payload) = crate::wire::sections(&snapshot)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(
            f64::from_le_bytes(payload[64..72].try_into().unwrap()),
            12_501.0
        );
    }
}
