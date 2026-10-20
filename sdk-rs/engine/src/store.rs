//! Typed exchange state plus change tracking. Once per flush the engine drains
//! the changes with [`Store::take_pending`] and writes them into the flush
//! frame with [`Store::write`].
use crate::model::{
    Balance, CoinBalance, CurrencyBalance, Loaded, MarginType, Market, Order, OrderStatus,
    Position, Ticker,
};
#[cfg(any(test, feature = "product-semantics"))]
use crate::model::{DerivativeProduct, InstrumentClassification, PositionGreeks, ProductKind};
#[cfg(feature = "prediction")]
use crate::model::{PredictionHolding, PredictionMarket};
use crate::wire::{self, FrameWriter, loaded_flag, market_flag, section};
use std::collections::HashMap;

#[derive(Default)]
pub struct Store {
    markets: Vec<Market>,
    #[cfg(any(test, feature = "product-semantics"))]
    instrument_classifications: Option<Vec<InstrumentClassification>>,
    #[cfg(any(test, feature = "product-semantics"))]
    derivative_products: Option<Vec<DerivativeProduct>>,
    #[cfg(any(test, feature = "product-semantics"))]
    position_greeks: Option<Vec<PositionGreeks>>,
    #[cfg(feature = "prediction")]
    prediction_markets: Option<Vec<PredictionMarket>>,
    #[cfg(feature = "prediction")]
    prediction_holdings: Option<Vec<PredictionHolding>>,
    market_by_symbol: HashMap<Box<str>, u32>,
    // Mixed IDs use sorted row indexes borrowing market identity. When every
    // instrument ID is its symbol, the symbol index serves both.
    market_by_id: Option<Vec<u32>>,
    tickers: Vec<Ticker>,
    ticker_revision: u64,
    // Sorted row indexes borrow ticker identity instead of copying every symbol.
    ticker_by_symbol: Vec<u32>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    position_ids: bool,
    balance: Balance,
    coin_balances: Option<Vec<CoinBalance>>,
    currency_balances: Option<Vec<CurrencyBalance>>,
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
        #[cfg(any(test, feature = "product-semantics"))]
        {
            self.instrument_classifications = None;
            self.derivative_products = None;
        }
        #[cfg(feature = "prediction")]
        {
            let had_prediction_markets = self.prediction_markets.take().is_some();
            if self.prediction_holdings.take().is_some() || had_prediction_markets {
                self.changes.positions = true;
            }
        }
        self.changes.markets = true;
        self.set_loaded(|loaded| loaded.markets = true);
    }

    #[cfg(any(test, feature = "product-semantics"))]
    pub fn instrument_classifications(&self) -> Option<&[InstrumentClassification]> {
        self.instrument_classifications.as_deref()
    }

    /// Replace labels after `set_markets`; indices belong to that snapshot.
    #[cfg(any(test, feature = "product-semantics"))]
    pub fn set_instrument_classifications(
        &mut self,
        classifications: Vec<InstrumentClassification>,
    ) {
        if self.instrument_classifications.as_ref() != Some(&classifications) {
            self.instrument_classifications = Some(classifications);
            self.changes.markets = true;
        }
    }

    #[cfg(any(test, feature = "product-semantics"))]
    pub fn derivative_products(&self) -> Option<&[DerivativeProduct]> {
        self.derivative_products.as_deref()
    }

    #[cfg(any(test, feature = "product-semantics"))]
    pub fn set_derivative_products(&mut self, products: Vec<DerivativeProduct>) {
        if self.derivative_products.as_ref() != Some(&products) {
            self.derivative_products = Some(products);
            self.changes.markets = true;
        }
    }

    #[cfg(any(test, feature = "product-semantics"))]
    pub fn position_greeks(&self) -> Option<&[PositionGreeks]> {
        self.position_greeks.as_deref()
    }

    #[cfg(any(test, feature = "product-semantics"))]
    pub fn set_position_greeks(&mut self, greeks: Vec<PositionGreeks>) {
        if self.position_greeks.as_ref() != Some(&greeks) {
            self.position_greeks = Some(greeks);
            self.prune_position_greeks();
            // Greeks reference exact venue IDs, including positions without a mode.
            if self
                .position_greeks
                .as_ref()
                .is_some_and(|rows| !rows.is_empty())
            {
                self.retain_position_ids();
            }
            self.changes.positions = true;
        }
    }

    #[cfg(any(test, feature = "product-semantics"))]
    fn prune_position_greeks(&mut self) {
        if let Some(rows) = &mut self.position_greeks {
            rows.retain(|row| {
                self.positions
                    .iter()
                    .any(|position| position.id == row.position_id)
            });
        }
    }

    #[cfg(feature = "prediction")]
    pub fn prediction_markets(&self) -> Option<&[PredictionMarket]> {
        self.prediction_markets.as_deref()
    }

    /// Replace metadata after `set_markets`; table indices belong to that snapshot.
    #[cfg(feature = "prediction")]
    pub fn set_prediction_markets(&mut self, markets: Vec<PredictionMarket>) {
        if self.prediction_markets.as_ref() != Some(&markets) {
            self.prediction_markets = Some(markets);
            self.changes.markets = true;
            // Existing holdings derive condition identity from their market metadata.
            if !self.positions.is_empty() {
                self.changes.positions = true;
            }
        }
    }

    #[cfg(feature = "prediction")]
    pub fn prediction_holdings(&self) -> Option<&[PredictionHolding]> {
        self.prediction_holdings.as_deref()
    }

    /// Replace settlement flags after an authoritative position snapshot.
    #[cfg(feature = "prediction")]
    pub fn set_prediction_holdings(&mut self, holdings: Vec<PredictionHolding>) {
        if self.prediction_holdings.as_ref() != Some(&holdings) {
            self.prediction_holdings = Some(holdings);
            self.changes.positions = true;
        }
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
        self.ticker_revision = self.ticker_revision.wrapping_add(1);
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

    pub(crate) fn ticker_revision(&self) -> u64 {
        self.ticker_revision
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
        self.ticker_revision = self.ticker_revision.wrapping_add(1);
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
        #[cfg(any(test, feature = "product-semantics"))]
        self.prune_position_greeks();
        #[cfg(feature = "prediction")]
        self.prune_prediction_holdings();
        self.changes.positions = true;
        self.set_loaded(|loaded| loaded.positions = true);
    }

    #[cfg(feature = "prediction")]
    fn prune_prediction_holdings(&mut self) {
        if let Some(holdings) = &mut self.prediction_holdings {
            holdings.retain(|holding| {
                self.markets
                    .get(holding.market as usize)
                    .is_some_and(|market| {
                        self.positions
                            .iter()
                            .any(|position| position.symbol == market.symbol)
                    })
            });
        }
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
        #[cfg(any(test, feature = "product-semantics"))]
        self.prune_position_greeks();
        #[cfg(feature = "prediction")]
        self.prune_prediction_holdings();
        self.changes.positions = true;
    }

    /// Change positions in place; rows left with zero contracts are removed.
    pub fn update_positions(&mut self, mut update: impl FnMut(&mut Position)) {
        for position in &mut self.positions {
            update(position);
        }
        self.positions.retain(|p| p.contracts != 0.0);
        #[cfg(any(test, feature = "product-semantics"))]
        self.prune_position_greeks();
        #[cfg(feature = "prediction")]
        self.prune_prediction_holdings();
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

    /// `None` means unspecified; an explicitly empty slice clears the consumer.
    pub fn currency_balances(&self) -> Option<&[CurrencyBalance]> {
        self.currency_balances.as_deref()
    }

    pub fn set_currency_balances(&mut self, currencies: Vec<CurrencyBalance>) {
        if self.currency_balances.as_ref() != Some(&currencies) {
            self.currency_balances = Some(currencies);
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

    pub(crate) fn set_loaded(&mut self, update: impl FnOnce(&mut Loaded)) {
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
        let strings = if changes.markets {
            // Identity IDs and symbols share a string-table entry.
            let per_market = if self.market_by_id.is_none() { 3 } else { 4 };
            per_market * self.markets.len()
        } else {
            0
        } + if changes.orders {
            3 * self.orders.len()
        } else {
            0
        };
        #[cfg(feature = "prediction")]
        let strings = strings
            + if changes.markets {
                self.prediction_markets
                    .as_ref()
                    .map_or(0, |rows| 6 * rows.len())
            } else {
                0
            };
        #[cfg(any(test, feature = "product-semantics"))]
        let strings = strings
            + if changes.markets {
                self.instrument_classifications
                    .as_ref()
                    .map_or(0, |rows| 3 * rows.len())
                    + self
                        .derivative_products
                        .as_ref()
                        .map_or(0, |rows| 5 * rows.len())
            } else {
                0
            }
            + if changes.positions {
                self.position_greeks
                    .as_ref()
                    .map_or(0, |rows| 2 * rows.len())
            } else {
                0
            };
        w.reserve_strings(strings);
        if changes.markets {
            self.write_markets(w);
            #[cfg(any(test, feature = "product-semantics"))]
            {
                self.write_instrument_classifications(w);
                self.write_derivative_products(w);
            }
            #[cfg(feature = "prediction")]
            self.write_prediction_markets(w);
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
            #[cfg(any(test, feature = "product-semantics"))]
            self.write_position_greeks(w);
            #[cfg(feature = "prediction")]
            self.write_prediction_holdings(w);
        }
        if changes.balance {
            let b = &self.balance;
            w.begin(section::BALANCE);
            w.f64s([b.used, b.free, b.total, b.upnl]);
            if self.coin_balances.is_some() || self.currency_balances.is_some() {
                let coins = self.coin_balances.as_deref().unwrap_or_default();
                w.u32(coins.len() as u32);
                for coin in coins {
                    let base = w.string(&coin.base);
                    w.u32(base);
                }
                w.pad();
                w.f64s(coins.iter().map(|coin| coin.amount));
                w.f64s(coins.iter().map(|coin| coin.notional));
                if let Some(currencies) = &self.currency_balances {
                    w.u32(currencies.len() as u32);
                    w.u32(u32::from(self.coin_balances.is_none()));
                    for currency in currencies {
                        let name = w.string(&currency.currency);
                        w.u32(name);
                    }
                    w.pad();
                    w.f64s(currencies.iter().map(|row| row.balance.total));
                    w.f64s(currencies.iter().map(|row| row.balance.free));
                    w.f64s(currencies.iter().map(|row| row.balance.used));
                    w.f64s(currencies.iter().map(|row| row.balance.upnl));
                }
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
            #[cfg(any(test, feature = "product-semantics"))]
            {
                if let Some(rows) = &self.instrument_classifications {
                    // Count, market index and three string references per row.
                    size += 8 + (4 + 16 * rows.len()).next_multiple_of(8);
                    for row in rows {
                        for value in [
                            row.symbol_type.as_deref(),
                            row.market_region.as_deref(),
                            row.underlying_ticker.as_deref(),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            strings += 1;
                            string_bytes += value.len();
                        }
                    }
                }
                if let Some(rows) = &self.derivative_products {
                    // Seven u32 columns, then aligned expiry and strike columns.
                    size += 8 + (4 + 28 * rows.len()).next_multiple_of(8) + 16 * rows.len();
                    for row in rows {
                        for value in [
                            Some(row.underlying.as_str()),
                            row.settlement_asset.as_deref(),
                            row.premium_asset.as_deref(),
                            row.style.as_deref(),
                            row.settlement.as_deref(),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            strings += 1;
                            string_bytes += value.len();
                        }
                    }
                }
            }
        }
        #[cfg(feature = "prediction")]
        if changes.markets
            && let Some(markets) = &self.prediction_markets
        {
            size += 16 + 40 * markets.len();
            for market in markets {
                for value in [
                    &market.condition_id,
                    &market.outcome,
                    &market.question,
                    &market.description,
                    &market.end_date,
                    &market.slug,
                ] {
                    strings += 1;
                    string_bytes += value.len();
                }
            }
        }
        if changes.tickers_replaced {
            let count = self.tickers.len();
            size += 8 + (4 + 4 * count).next_multiple_of(8) + 88 * count;
            if self
                .tickers
                .iter()
                .any(|ticker| self.market_index(&ticker.symbol).is_none())
            {
                size += 8 * count;
                strings += 2 * count;
                string_bytes += self
                    .tickers
                    .iter()
                    .map(|ticker| ticker.symbol.len() + ticker.id.len())
                    .sum::<usize>();
            }
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
            let filled = self.orders.iter().any(|order| {
                order
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.filled_in_placement_units)
                    .is_some()
            });
            if filled
                || self.orders.iter().any(|order| {
                    order
                        .extra
                        .as_ref()
                        .and_then(|extra| extra.remaining_native)
                        .is_some()
                })
            {
                size += 8 * count;
            }
            if filled {
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
            #[cfg(any(test, feature = "product-semantics"))]
            if let Some(rows) = &self.position_greeks {
                // Two string references, then four aligned f64 columns.
                size += 8 + (4 + 8 * rows.len()).next_multiple_of(8) + 32 * rows.len();
                for row in rows {
                    for value in [&row.position_id, &row.greeks.units] {
                        strings += 1;
                        string_bytes += value.len();
                    }
                }
            }
        }
        #[cfg(feature = "prediction")]
        if changes.positions
            && let Some(holdings) = &self.prediction_holdings
        {
            size += 16 + 8 * holdings.len();
        }
        if changes.balance {
            size += 40;
            if self.coin_balances.is_some() || self.currency_balances.is_some() {
                let coins = self.coin_balances.as_deref().unwrap_or_default();
                let count = coins.len();
                size += (4 + 4 * count).next_multiple_of(8) + 16 * count;
                strings += count;
                string_bytes += coins.iter().map(|coin| coin.base.len()).sum::<usize>();
            }
            if let Some(currencies) = &self.currency_balances {
                let count = currencies.len();
                size += (8 + 4 * count).next_multiple_of(8) + 32 * count;
                strings += count;
                string_bytes += currencies
                    .iter()
                    .map(|currency| currency.currency.len())
                    .sum::<usize>();
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

    #[cfg(any(test, feature = "product-semantics"))]
    fn write_instrument_classifications<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let Some(rows) = &self.instrument_classifications else {
            return;
        };
        w.begin(section::INSTRUMENT_CLASSIFICATIONS);
        w.u32(rows.len() as u32);
        w.u32s(rows.iter().map(|row| row.market));
        for row in rows {
            let refs = [
                row.symbol_type
                    .as_deref()
                    .map(|s| w.string(s))
                    .unwrap_or(wire::NONE),
                row.market_region
                    .as_deref()
                    .map(|s| w.string(s))
                    .unwrap_or(wire::NONE),
                row.underlying_ticker
                    .as_deref()
                    .map(|s| w.string(s))
                    .unwrap_or(wire::NONE),
            ];
            w.u32s(refs);
        }
        w.end();
    }

    #[cfg(any(test, feature = "product-semantics"))]
    fn write_derivative_products<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let Some(rows) = &self.derivative_products else {
            return;
        };
        w.begin(section::DERIVATIVE_PRODUCTS);
        w.u32(rows.len() as u32);
        w.u32s(rows.iter().map(|row| row.market));
        w.u32s(rows.iter().map(|row| match row.kind {
            ProductKind::Future => 1,
            ProductKind::Call => 2,
            ProductKind::Put => 3,
        }));
        for row in rows {
            let refs = [
                w.string(&row.underlying),
                w.optional_string(row.settlement_asset.as_deref()),
                w.optional_string(row.premium_asset.as_deref()),
                w.optional_string(row.style.as_deref()),
                w.optional_string(row.settlement.as_deref()),
            ];
            w.u32s(refs);
        }
        w.pad();
        w.f64s(rows.iter().map(|row| row.expiry.unwrap_or(f64::NAN)));
        w.f64s(rows.iter().map(|row| row.strike.unwrap_or(f64::NAN)));
        w.end();
    }

    #[cfg(any(test, feature = "product-semantics"))]
    fn write_position_greeks<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let Some(rows) = &self.position_greeks else {
            return;
        };
        w.begin(section::POSITION_GREEKS);
        w.u32(rows.len() as u32);
        for row in rows {
            let refs = [w.string(&row.position_id), w.string(&row.greeks.units)];
            w.u32s(refs);
        }
        w.pad();
        for value in [0, 1, 2, 3] {
            w.f64s(rows.iter().map(|row| match value {
                0 => row.greeks.delta,
                1 => row.greeks.gamma,
                2 => row.greeks.vega,
                _ => row.greeks.theta,
            }));
        }
        w.end();
    }

    #[cfg(feature = "prediction")]
    fn write_prediction_markets<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let Some(markets) = &self.prediction_markets else {
            return;
        };
        w.begin(section::PREDICTION_MARKETS);
        w.u32(markets.len() as u32);
        w.u32s(markets.iter().map(|market| market.market));
        for market in markets {
            let refs = [
                w.string(&market.condition_id),
                w.string(&market.outcome),
                w.string(&market.question),
                w.string(&market.description),
                w.string(&market.end_date),
                w.string(&market.slug),
            ];
            w.u32s(refs);
        }
        w.u32s(markets.iter().map(|market| u32::from(market.neg_risk)));
        w.pad();
        w.f64s(markets.iter().map(|market| market.tick_size));
        w.end();
    }

    #[cfg(feature = "prediction")]
    fn write_prediction_holdings<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let Some(holdings) = &self.prediction_holdings else {
            return;
        };
        w.begin(section::PREDICTION_HOLDINGS);
        w.u32(holdings.len() as u32);
        w.u32s(holdings.iter().map(|holding| holding.market));
        w.u32s(
            holdings
                .iter()
                .map(|holding| u32::from(holding.redeemable) | (u32::from(holding.mergeable) << 1)),
        );
        w.end();
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
        // Delisted symbols can remain in the venue's aggregate ticker stream.
        // Preserve their identities without inventing a tradeable market row.
        if tickers
            .iter()
            .any(|ticker| self.market_index(&ticker.symbol).is_none())
        {
            for ticker in tickers {
                let symbol = w.string(&ticker.symbol);
                w.u32(symbol);
            }
            for ticker in tickers {
                let id = w.string(&ticker.id);
                w.u32(id);
            }
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
        if orders.iter().any(|o| {
            o.extra.as_ref().is_some_and(|e| {
                e.remaining_native.is_some() || e.filled_in_placement_units.is_some()
            })
        }) {
            w.f64s(orders.iter().map(|o| {
                o.extra
                    .as_ref()
                    .and_then(|e| e.remaining_native)
                    .unwrap_or(f64::NAN)
            }));
            if orders.iter().any(|o| {
                o.extra
                    .as_ref()
                    .and_then(|e| e.filled_in_placement_units)
                    .is_some()
            }) {
                w.f64s(orders.iter().map(|o| {
                    o.extra
                        .as_ref()
                        .and_then(|e| e.filled_in_placement_units)
                        .unwrap_or(f64::NAN)
                }));
            }
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
    fn frame_hint_includes_orphan_tickers_and_currency_balance_tails() {
        for count in 0..=3 {
            for coins_present in [false, true] {
                let mut store = Store::default();
                store.set_markets(vec![market("LIVE", "LIVE")]);
                store.set_tickers((0..count).map(|i| ticker(&format!("ORPHAN{i}"))).collect());
                if coins_present {
                    store.set_coin_balances(vec![CoinBalance {
                        base: "BTC".into(),
                        amount: 1.0,
                        notional: 2.0,
                    }]);
                }
                store.set_currency_balances(
                    (0..count)
                        .map(|i| CurrencyBalance {
                            currency: format!("COIN{i}"),
                            balance: Balance {
                                used: 1.0,
                                free: 2.0,
                                total: 3.0,
                                upnl: 4.0,
                            },
                        })
                        .collect(),
                );
                let pending = store.take_pending();
                let hint = 16 + store.frame_size_hint(&pending);
                let buffer = Vec::with_capacity(hint);
                let capacity = buffer.capacity();
                let mut writer = FrameWriter::new(buffer, 0.0);
                store.write(&pending, &mut writer);
                let frame = writer.finish();
                assert!(frame.len() <= hint);
                assert_eq!(
                    frame.capacity(),
                    capacity,
                    "rows={count}, coins={coins_present}"
                );
                let mut writer = FrameWriter::new(Vec::new(), 0.0);
                store.write(&pending, &mut writer);
                assert_eq!(frame, writer.finish(), "reservation preserves wire");
            }
        }
    }

    #[test]
    fn ticker_identity_tail_preserves_orphans_and_leaves_legacy_columns_unchanged() {
        let mut store = Store::default();
        store.set_markets(vec![market("LIVEUSDT", "LIVEUSDT")]);
        store.set_tickers(vec![ticker("LIVEUSDT")]);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write_tickers(&mut writer);
        let legacy = writer.finish();
        let (_, legacy_payload) = crate::wire::sections(&legacy)
            .find(|(kind, _)| *kind == section::TICKERS)
            .unwrap();
        assert_eq!(
            legacy_payload.len(),
            96,
            "known markets retain the old ticker layout"
        );

        let mut orphan = ticker("OLDUSDT");
        orphan.id = "native-retired-id".into();
        orphan.last = 42.0;
        store.set_tickers(vec![ticker("LIVEUSDT"), orphan]);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write_tickers(&mut writer);
        let frame = writer.finish();
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::TICKERS)
            .unwrap();
        let word = |at| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        assert_eq!(payload.len(), 208);
        assert_eq!((word(0), word(4), word(8)), (2, 0, wire::NONE));
        assert_eq!(
            f64::from_le_bytes(payload[56..64].try_into().unwrap()),
            42.0
        );
        assert_eq!((word(192), word(196), word(200), word(204)), (0, 1, 0, 2));
        let (_, strings) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::STRINGS)
            .unwrap();
        assert_eq!(u32::from_le_bytes(strings[..4].try_into().unwrap()), 3);
        let expected = b"LIVEUSDTOLDUSDTnative-retired-id";
        assert_eq!(&strings[20..20 + expected.len()], expected);
        assert_eq!(
            store.markets().len(),
            1,
            "ticker identity does not synthesize markets"
        );
    }

    #[test]
    fn orphan_ticker_deltas_keep_the_existing_row_index_and_packed_shape() {
        let mut store = Store::default();
        store.set_markets(vec![market("LIVEUSDT", "LIVEUSDT")]);
        store.set_tickers(vec![ticker("LIVEUSDT"), ticker("OLDUSDT")]);
        store.take_pending();
        store.update_ticker("OLDUSDT", |ticker| ticker.last = 42.0);
        let pending = store.take_pending();
        assert_eq!(pending.ticker_deltas, [(1, 1 << 2)]);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write(&pending, &mut writer);
        let frame = writer.finish();
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::TICKER_DELTAS)
            .unwrap();
        assert_eq!(payload.len(), 24);
        let values = payload
            .as_chunks::<8>()
            .0
            .iter()
            .map(|bytes| f64::from_le_bytes(*bytes))
            .collect::<Vec<_>>();
        assert_eq!(values, [1.0, 4.0, 42.0]);
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
            for extensions in 0..4 {
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
                            // One row enables each optional column for every row.
                            extra: (index == count - 1).then_some(OrderExtra {
                                remaining_native: (extensions & 1 != 0).then_some(7.0),
                                filled_in_placement_units: (extensions & 2 != 0).then_some(3.0),
                                ..OrderExtra::default()
                            }),
                        })
                        .collect(),
                );
                let mut pending = store.take_pending();
                pending.changes.status = false;
                let size = 16 + store.frame_size_hint(&pending);
                let buffer = Vec::with_capacity(size);
                let capacity = buffer.capacity();
                let pointer = buffer.as_ptr();
                let mut writer = FrameWriter::new(buffer, 1.0);
                store.write(&pending, &mut writer);
                let frame = writer.finish();
                assert_eq!(frame.len(), size, "rows={count}, extensions={extensions}");
                assert_eq!(frame.capacity(), capacity);
                assert_eq!(frame.as_ptr(), pointer);
                let mut plain = FrameWriter::new(Vec::new(), 1.0);
                store.write_orders(&mut plain);
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
    fn spot_currency_tail_preserves_full_and_reserved_holdings() {
        let mut store = Store::default();
        assert_eq!(store.currency_balances(), None);
        let holdings = vec![CurrencyBalance {
            currency: "BTC".into(),
            balance: Balance {
                total: 2.,
                free: 1.25,
                used: 0.75,
                upnl: 0.,
            },
        }];
        store.set_currency_balances(holdings.clone());
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(payload.len(), 88);
        assert_eq!(u32::from_le_bytes(payload[32..36].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(payload[40..44].try_into().unwrap()), 1);
        let value = |offset| f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
        assert_eq!(
            (value(56), value(64), value(72), value(80)),
            (2., 1.25, 0.75, 0.)
        );
        assert_eq!(store.currency_balances(), Some(holdings.as_slice()));
        store.set_currency_balances(holdings);
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
        );
        store.set_currency_balances(vec![]);
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(payload.len(), 48);
        assert_eq!(u32::from_le_bytes(payload[40..44].try_into().unwrap()), 0);
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
    fn market_position_store() -> Store {
        use crate::model::{Limits, MinMax, PositionSide, Precision};
        let mut store = Store::default();
        store.set_markets(vec![Market {
            id: "1234567890123456789012345678901234567890".into(),
            symbol: "1234567890123456789012345678901234567890".into(),
            pretty_symbol: None,
            base: "Yes".into(),
            quote: "pUSD".into(),
            active: true,
            precision: Precision {
                amount: 0.01,
                price: 0.001,
            },
            limits: Limits {
                amount: MinMax {
                    min: 5.0,
                    max: f64::INFINITY,
                },
                leverage: MinMax { min: 1.0, max: 1.0 },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: Some(true),
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        store.set_positions(vec![Position {
            id: "outcome-holding".into(),
            symbol: store.markets[0].symbol.clone(),
            side: PositionSide::Long,
            entry_price: 0.4,
            notional: 6.0,
            leverage: 1.0,
            margin_type: None,
            position_mode: None,
            unrealized_pnl: 2.0,
            realized_pnl: None,
            contracts: 10.0,
            liquidation_price: 0.0,
            margin_size: None,
            last_update: None,
        }]);
        store
    }

    #[test]
    fn product_sections_frame_hint_preserves_unicode_optional_fields_without_growth() {
        let template = market_position_store();
        for count in [0, 1, 2, 3, 257] {
            for sections in [1, 2, 4, 7] {
                for optional in [0, 1, 7, 15] {
                    let mut store = Store::default();
                    store.set_markets(
                        (0..count)
                            .map(|index| {
                                let mut row =
                                    market(&format!("id雪{index}"), &format!("symbol🦀{index}"));
                                row.base = format!("baseλ{index}");
                                row.quote = format!("quote東京{index}");
                                row
                            })
                            .collect(),
                    );
                    store.set_positions(
                        (0..count)
                            .map(|index| {
                                let mut row = template.positions[0].clone();
                                row.id = format!("position🌊{index}");
                                row.symbol = store.markets[index].symbol.clone();
                                row
                            })
                            .collect(),
                    );
                    if sections & 1 != 0 {
                        store.set_instrument_classifications(
                            (0..count)
                                .map(|index| InstrumentClassification {
                                    market: index as u32,
                                    symbol_type: (optional & 1 != 0).then(|| {
                                        if index == 0 {
                                            String::new()
                                        } else {
                                            format!("type雪{index}")
                                        }
                                    }),
                                    market_region: (optional & 2 != 0)
                                        .then(|| format!("regioné{index}")),
                                    underlying_ticker: (optional & 4 != 0)
                                        .then(|| format!("ticker株{index}")),
                                })
                                .collect(),
                        );
                    }
                    if sections & 2 != 0 {
                        store.set_derivative_products(
                            (0..count)
                                .map(|index| DerivativeProduct {
                                    market: index as u32,
                                    kind: ProductKind::Call,
                                    underlying: format!("underlying雪{index}"),
                                    expiry: (optional & 1 != 0).then_some(1234.0),
                                    strike: (optional & 2 != 0).then_some(42.0),
                                    settlement_asset: (optional & 1 != 0)
                                        .then(|| format!("asset🌍{index}")),
                                    premium_asset: (optional & 2 != 0)
                                        .then(|| format!("premiumλ{index}")),
                                    style: (optional & 4 != 0).then(|| format!("style東京{index}")),
                                    settlement: (optional & 8 != 0)
                                        .then(|| format!("settlementé{index}")),
                                })
                                .collect(),
                        );
                    }
                    if sections & 4 != 0 {
                        store.set_position_greeks(
                            (0..count)
                                .map(|index| PositionGreeks {
                                    position_id: store.positions[index].id.clone(),
                                    greeks: crate::model::OptionGreeks {
                                        delta: 0.5,
                                        gamma: 0.01,
                                        vega: 2.0,
                                        theta: -3.0,
                                        units: format!("units🦀{index}"),
                                    },
                                })
                                .collect(),
                        );
                    }
                    let mut pending = store.take_pending();
                    pending.changes.status = false;
                    let size = 16 + store.frame_size_hint(&pending);
                    let buffer = Vec::with_capacity(size);
                    let capacity = buffer.capacity();
                    let pointer = buffer.as_ptr();
                    let mut reserved = FrameWriter::new(buffer, 7.0);
                    store.write(&pending, &mut reserved);
                    let frame = reserved.finish();
                    assert!(
                        frame.len() <= size,
                        "rows={count}, sections={sections}, optional={optional}: {} > {size}",
                        frame.len()
                    );
                    assert_eq!(frame.capacity(), capacity);
                    assert_eq!(frame.as_ptr(), pointer);
                    // Serialize the same sections without any whole-frame reservation.
                    let mut plain = FrameWriter::new(Vec::new(), 7.0);
                    store.write_markets(&mut plain);
                    store.write_instrument_classifications(&mut plain);
                    store.write_derivative_products(&mut plain);
                    if pending.changes.positions {
                        store.write_positions(&mut plain);
                        store.write_position_greeks(&mut plain);
                    }
                    assert_eq!(frame, plain.finish());
                }
            }
        }
    }

    #[test]
    fn semantic_labels_preserve_empty_absent_and_refresh_without_numeric_changes() {
        let mut store = market_position_store();
        let legacy = balance_frame(&mut store);
        let labels = InstrumentClassification {
            market: 0,
            symbol_type: Some(String::new()),
            market_region: None,
            underlying_ticker: Some("株式".into()),
        };
        store.set_instrument_classifications(vec![labels.clone()]);
        let frame = balance_frame(&mut store);
        let payload = |bytes: &[u8], kind| {
            wire::sections(bytes)
                .find(|(k, _)| *k == kind)
                .unwrap()
                .1
                .to_vec()
        };
        assert_eq!(
            payload(&legacy, section::MARKETS),
            payload(&frame, section::MARKETS)
        );
        let words = payload(&frame, section::INSTRUMENT_CLASSIFICATIONS);
        let word = |at| u32::from_le_bytes(words[at..at + 4].try_into().unwrap());
        assert_eq!((word(0), word(4)), (1, 0));
        assert_ne!(word(8), wire::NONE);
        assert_eq!(word(12), wire::NONE);
        assert_ne!(word(16), wire::NONE);
        store.set_instrument_classifications(vec![labels]);
        assert!(!store.take_pending().changes.markets);
        store.set_instrument_classifications(vec![]);
        let cleared = balance_frame(&mut store);
        assert_eq!(
            &payload(&cleared, section::INSTRUMENT_CLASSIFICATIONS)[..4],
            &[0; 4]
        );
        store.set_markets(store.markets.clone());
        assert!(store.instrument_classifications().is_none());
        assert!(
            wire::sections(&balance_frame(&mut store))
                .all(|(k, _)| k != section::INSTRUMENT_CLASSIFICATIONS)
        );
    }

    #[test]
    fn derivative_terms_are_optional_and_position_greeks_never_resurrect_on_reopen() {
        let mut store = market_position_store();
        store.set_derivative_products(vec![DerivativeProduct {
            market: 0,
            kind: ProductKind::Call,
            underlying: "BTC".into(),
            expiry: None,
            strike: Some(100_000.0),
            settlement_asset: None,
            premium_asset: None,
            style: None,
            settlement: None,
        }]);
        let metadata = PositionGreeks {
            position_id: store.positions[0].id.clone(),
            greeks: crate::model::OptionGreeks {
                delta: 0.5,
                gamma: 0.01,
                vega: 2.0,
                theta: -3.0,
                units: "native".into(),
            },
        };
        store.set_position_greeks(vec![metadata.clone()]);
        let frame = balance_frame(&mut store);
        assert!(wire::sections(&frame).any(|(k, _)| k == section::DERIVATIVE_PRODUCTS));
        assert!(wire::sections(&frame).any(|(k, _)| k == section::POSITION_GREEKS));
        let position = store.positions[0].clone();
        store.set_positions(vec![]);
        assert_eq!(store.position_greeks(), Some([].as_slice()));
        store.upsert_position(position);
        assert_eq!(store.position_greeks(), Some([].as_slice()));
        store.set_position_greeks(vec![metadata]);
        store.set_markets(store.markets.clone());
        assert!(store.derivative_products().is_none());
    }

    #[test]
    fn position_greeks_prune_on_every_close_path_and_preserve_ids_without_resurrection() {
        fn assert_position_id_frame(store: &Store) {
            let mut writer = FrameWriter::new(vec![], 0.0);
            store.write_positions(&mut writer);
            let frame = writer.finish();
            let (_, positions) = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::POSITIONS)
                .unwrap();
            // One ordinary position occupies 88 bytes; the optional ID follows it.
            assert_eq!(positions.len(), 96);
            assert_eq!(u32::from_le_bytes(positions[88..92].try_into().unwrap()), 0);
            let (_, strings) = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::STRINGS)
                .unwrap();
            assert_eq!(u32::from_le_bytes(strings[..4].try_into().unwrap()), 1);
            let id = store.positions()[0].id.as_bytes();
            assert_eq!(
                u32::from_le_bytes(strings[8..12].try_into().unwrap()) as usize,
                id.len()
            );
            assert_eq!(&strings[12..12 + id.len()], id);
        }

        for close in ["set_positions", "upsert_position", "update_positions"] {
            let mut store = market_position_store();
            let position = store.positions()[0].clone();
            assert!(position.position_mode.is_none());
            assert!(!store.position_ids);
            store.set_position_greeks(vec![PositionGreeks {
                position_id: position.id.clone(),
                greeks: crate::model::OptionGreeks {
                    delta: 0.5,
                    gamma: 0.01,
                    vega: 2.0,
                    theta: -3.0,
                    units: "native".into(),
                },
            }]);
            assert_position_id_frame(&store);
            store.take_pending();

            match close {
                "set_positions" => store.set_positions(vec![]),
                "upsert_position" => {
                    let mut closed = position.clone();
                    closed.contracts = 0.0;
                    store.upsert_position(closed);
                }
                "update_positions" => store.update_positions(|row| row.contracts = 0.0),
                _ => unreachable!(),
            }
            assert!(store.positions().is_empty(), "{close}");
            assert_eq!(store.position_greeks(), Some([].as_slice()), "{close}");
            let cleared = balance_frame(&mut store);
            let (_, greeks) = wire::sections(&cleared)
                .find(|(kind, _)| *kind == section::POSITION_GREEKS)
                .unwrap();
            assert_eq!(u32::from_le_bytes(greeks[..4].try_into().unwrap()), 0);

            store.upsert_position(position);
            assert_eq!(store.position_greeks(), Some([].as_slice()), "{close}");
            assert_position_id_frame(&store);
            let reopened = balance_frame(&mut store);
            let (_, greeks) = wire::sections(&reopened)
                .find(|(kind, _)| *kind == section::POSITION_GREEKS)
                .unwrap();
            assert_eq!(u32::from_le_bytes(greeks[..4].try_into().unwrap()), 0);
        }
    }

    #[cfg(feature = "prediction")]
    fn prediction_metadata() -> PredictionMarket {
        PredictionMarket {
            market: 0,
            condition_id: "0xcondition".into(),
            outcome: "Yes".into(),
            question: "Will café open?".into(),
            description: "An exact description".into(),
            end_date: "2026-12-31T00:00:00Z".into(),
            slug: "cafe-open".into(),
            tick_size: 0.001,
            neg_risk: true,
        }
    }

    #[test]
    #[cfg(feature = "prediction")]
    fn prediction_and_currency_frame_hint_covers_unicode_columns_without_growth() {
        let mut store = market_position_store();
        let mut metadata = prediction_metadata();
        metadata.condition_id = "condition雪".repeat(31);
        metadata.outcome = "outcome🦀".repeat(17);
        metadata.question = "question café?".repeat(23);
        metadata.description = "description\0東京".repeat(29);
        metadata.end_date = "end_dateλ".repeat(11);
        metadata.slug = "slug🌍".repeat(13);
        store.set_prediction_markets(vec![metadata.clone()]);
        store.set_prediction_holdings(vec![PredictionHolding {
            market: 0,
            redeemable: true,
            mergeable: true,
        }]);
        // Three currency names exercise the extra flag word at odd row counts.
        store.set_currency_balances(
            (0..3)
                .map(|index| CurrencyBalance {
                    currency: format!("currency雪{index}"),
                    balance: Balance {
                        total: 3.0,
                        free: 2.0,
                        used: 1.0,
                        upnl: -4.0,
                    },
                })
                .collect(),
        );
        let pending = store.take_pending();
        let hint = 16 + store.frame_size_hint(&pending);
        let buffer = Vec::with_capacity(hint);
        let capacity = buffer.capacity();
        let pointer = buffer.as_ptr();
        let mut reserved = FrameWriter::new(buffer, 7.0);
        store.write(&pending, &mut reserved);
        let frame = reserved.finish();
        assert_eq!(frame.len(), hint);
        assert_eq!(frame.capacity(), capacity);
        assert_eq!(frame.as_ptr(), pointer);
        let mut plain = FrameWriter::new(Vec::new(), 7.0);
        store.write(&pending, &mut plain);
        assert_eq!(frame, plain.finish());

        let payload = |kind| wire::sections(&frame).find(|(k, _)| *k == kind).unwrap().1;
        let markets = payload(section::PREDICTION_MARKETS);
        let holdings = payload(section::PREDICTION_HOLDINGS);
        assert_eq!(markets.len(), 48);
        assert_eq!(holdings.len(), 16);
        assert_eq!(u32::from_le_bytes(holdings[8..12].try_into().unwrap()), 3);
        let strings = payload(section::STRINGS);
        let count = u32::from_le_bytes(strings[..4].try_into().unwrap()) as usize;
        let text = |index: usize| {
            let offset =
                |i| u32::from_le_bytes(strings[4 + i * 4..8 + i * 4].try_into().unwrap()) as usize;
            std::str::from_utf8(
                &strings[8 + count * 4 + offset(index)..8 + count * 4 + offset(index + 1)],
            )
            .unwrap()
        };
        for (index, expected) in [
            metadata.condition_id,
            metadata.outcome,
            metadata.question,
            metadata.description,
            metadata.end_date,
            metadata.slug,
        ]
        .iter()
        .enumerate()
        {
            let at = 8 + 4 * index;
            let reference = u32::from_le_bytes(markets[at..at + 4].try_into().unwrap());
            assert_eq!(text(reference as usize), expected);
        }
        let balance = payload(section::BALANCE);
        assert_eq!(balance.len(), 160);
        assert_eq!(u32::from_le_bytes(balance[40..44].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(balance[44..48].try_into().unwrap()), 1);
        assert_eq!(
            f64::from_le_bytes(balance[136..144].try_into().unwrap()),
            -4.0
        );
    }

    #[test]
    #[cfg(feature = "prediction")]
    fn prediction_columns_preserve_ordinary_layout_and_full_snapshot_identity() {
        let mut store = market_position_store();
        let legacy = balance_frame(&mut store);
        let metadata = prediction_metadata();
        store.set_prediction_markets(vec![metadata.clone()]);
        store.set_prediction_holdings(vec![PredictionHolding {
            market: 0,
            redeemable: true,
            mergeable: false,
        }]);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write(&store.everything(), &mut writer);
        let frame = writer.finish();
        for kind in [section::MARKETS, section::POSITIONS] {
            let old = wire::sections(&legacy).find(|(k, _)| *k == kind).unwrap().1;
            let new = wire::sections(&frame).find(|(k, _)| *k == kind).unwrap().1;
            assert_eq!(old, new, "prediction metadata cannot shift legacy columns");
        }
        assert_eq!(store.prediction_markets(), Some([metadata].as_slice()));
        let payload = wire::sections(&frame)
            .find(|(k, _)| *k == section::PREDICTION_MARKETS)
            .unwrap()
            .1;
        let word = |at| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        assert_eq!(payload.len(), 48);
        assert_eq!((word(0), word(4), word(32)), (1, 0, 1));
        assert_eq!(
            f64::from_le_bytes(payload[40..48].try_into().unwrap()),
            0.001
        );
        let strings = wire::sections(&frame)
            .find(|(k, _)| *k == section::STRINGS)
            .unwrap()
            .1;
        let count = u32::from_le_bytes(strings[..4].try_into().unwrap()) as usize;
        let text = |index: usize| {
            let offset =
                |i| u32::from_le_bytes(strings[4 + i * 4..8 + i * 4].try_into().unwrap()) as usize;
            std::str::from_utf8(
                &strings[8 + count * 4 + offset(index)..8 + count * 4 + offset(index + 1)],
            )
            .unwrap()
        };
        assert_eq!(text(word(8) as usize), "0xcondition");
        assert_eq!(text(word(16) as usize), "Will café open?");
        let holdings = wire::sections(&frame)
            .find(|(k, _)| *k == section::PREDICTION_HOLDINGS)
            .unwrap()
            .1;
        assert_eq!(u32::from_le_bytes(holdings[8..12].try_into().unwrap()), 1);
    }

    #[test]
    #[cfg(feature = "prediction")]
    fn prediction_flags_survive_position_changes_but_never_resurrect_on_reopen() {
        let mut store = market_position_store();
        store.set_prediction_markets(vec![prediction_metadata()]);
        let holding = PredictionHolding {
            market: 0,
            redeemable: false,
            mergeable: true,
        };
        store.set_prediction_holdings(vec![holding]);
        store.take_pending();
        store.set_prediction_markets(vec![prediction_metadata()]);
        store.set_prediction_holdings(vec![holding]);
        let unchanged = store.take_pending();
        assert!(!unchanged.changes.markets && !unchanged.changes.positions);
        let positions = store.positions.clone();
        store.set_positions(positions.clone());
        assert_eq!(store.prediction_holdings(), Some([holding].as_slice()));
        store.update_positions(|position| position.contracts = 0.0);
        assert_eq!(store.prediction_holdings(), Some([].as_slice()));
        store.set_positions(positions);
        assert_eq!(store.prediction_holdings(), Some([].as_slice()));
        store.set_prediction_holdings(vec![holding]);
        let mut closed = store.positions[0].clone();
        closed.contracts = 0.0;
        store.upsert_position(closed);
        assert_eq!(store.prediction_holdings(), Some([].as_slice()));
        store.set_prediction_markets(vec![]);
        let cleared = balance_frame(&mut store);
        let payload = wire::sections(&cleared)
            .find(|(k, _)| *k == section::PREDICTION_MARKETS)
            .unwrap()
            .1;
        assert_eq!(&payload[..4], &[0, 0, 0, 0]);
        store.set_markets(store.markets.clone());
        assert_eq!(store.prediction_markets(), None);
        assert_eq!(store.prediction_holdings(), None);
        let reset = balance_frame(&mut store);
        assert!(
            wire::sections(&reset).all(|(kind, _)| kind != section::PREDICTION_MARKETS
                && kind != section::PREDICTION_HOLDINGS)
        );
    }

    #[test]
    fn currency_columns_keep_all_balance_fields_and_explicitly_clear() {
        let mut store = Store::default();
        assert_eq!(store.currency_balances(), None);
        let currencies = vec![
            CurrencyBalance {
                currency: "USDT".into(),
                balance: Balance {
                    used: 1.0,
                    free: 2.0,
                    total: 3.0,
                    upnl: -4.0,
                },
            },
            CurrencyBalance {
                currency: "USDC".into(),
                balance: Balance {
                    used: 5.0,
                    free: 6.0,
                    total: 11.0,
                    upnl: 8.0,
                },
            },
        ];
        store.set_currency_balances(currencies.clone());
        assert_eq!(store.currency_balances(), Some(currencies.as_slice()));
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        let word = |at| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        let value = |at| f64::from_le_bytes(payload[at..at + 8].try_into().unwrap());
        assert_eq!(payload.len(), 120);
        assert_eq!(
            word(32),
            0,
            "currency-only frame retains an empty coin tail"
        );
        assert_eq!(word(40), 2);
        assert_eq!((word(44), word(48), word(52)), (1, 0, 1));
        assert_eq!((value(56), value(64)), (3.0, 11.0));
        assert_eq!((value(72), value(80)), (2.0, 6.0));
        assert_eq!((value(88), value(96)), (1.0, 5.0));
        assert_eq!((value(104), value(112)), (-4.0, 8.0));
        assert_eq!(
            store.coin_balances(),
            None,
            "wire padding does not invent store holdings"
        );
        store.set_currency_balances(currencies);
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
        );
        store.set_currency_balances(vec![]);
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(payload.len(), 48);
        assert_eq!(u32::from_le_bytes(payload[40..44].try_into().unwrap()), 0);
        assert_eq!(store.currency_balances(), Some([].as_slice()));
    }

    #[test]
    fn currency_extension_follows_existing_collateral_and_survives_snapshot() {
        let mut store = Store::default();
        store.set_coin_balances(vec![CoinBalance {
            base: "BTC".into(),
            amount: 0.25,
            notional: 12500.0,
        }]);
        store.set_currency_balances(vec![CurrencyBalance {
            currency: "USDT".into(),
            balance: Balance {
                used: 1.0,
                free: 2.0,
                total: 3.0,
                upnl: 4.0,
            },
        }]);
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(payload.len(), 104);
        assert_eq!(u32::from_le_bytes(payload[32..36].try_into().unwrap()), 1);
        assert_eq!(
            f64::from_le_bytes(payload[40..48].try_into().unwrap()),
            0.25
        );
        assert_eq!(
            f64::from_le_bytes(payload[48..56].try_into().unwrap()),
            12500.0
        );
        assert_eq!(u32::from_le_bytes(payload[56..60].try_into().unwrap()), 1);
        let mut writer = FrameWriter::new(vec![], 0.0);
        store.write(&store.everything(), &mut writer);
        let snapshot = writer.finish();
        let (_, snapshot_payload) = crate::wire::sections(&snapshot)
            .find(|(kind, _)| *kind == section::BALANCE)
            .unwrap();
        assert_eq!(snapshot_payload.len(), payload.len());
        assert_eq!(&snapshot_payload[64..], &payload[64..]);
    }
}
