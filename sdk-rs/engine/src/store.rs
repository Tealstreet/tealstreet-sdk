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
    market_by_id: HashMap<Box<str>, u32>,
    tickers: Vec<Ticker>,
    ticker_revision: u64,
    ticker_by_market: Vec<u32>,
    tickers_without_market: HashMap<String, u32>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    position_ids: bool,
    balance: Balance,
    coin_balances: Option<Vec<CoinBalance>>,
    currency_balances: Option<Vec<CurrencyBalance>>,
    preserve_unspecified_collateral: bool,
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

    pub fn set_tickers(&mut self, tickers: Vec<Ticker>) {
        self.ticker_revision = self.ticker_revision.wrapping_add(1);
        self.tickers = tickers;
        self.rebuild_ticker_index();
        self.changes.tickers_replaced = true;
        self.ticker_masks = vec![0; self.tickers.len()];
        self.dirty_tickers.clear();
        self.set_loaded(|loaded| loaded.tickers = true);
    }

    pub(crate) fn ticker_revision(&self) -> u64 {
        self.ticker_revision
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

    /// Opt in to preserving unspecified collateral when only currencies are
    /// supplied. Existing writers retain their legacy explicit-empty coin tail.
    pub fn preserve_unspecified_collateral(&mut self) {
        if !self.preserve_unspecified_collateral {
            self.preserve_unspecified_collateral = true;
            if self.currency_balances.is_some() && self.coin_balances.is_none() {
                self.changes.balance = true;
            }
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

    /// Change market rows in place (indexes and symbols must not change).
    pub fn update_markets(&mut self, mut update: impl FnMut(&mut Market)) {
        for market in &mut self.markets {
            update(market);
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
        if changes.markets || changes.tickers_replaced {
            self.reserve_tables(pending, w);
        }
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
            }
            if let Some(currencies) = &self.currency_balances {
                w.u32(currencies.len() as u32);
                for currency in currencies {
                    let name = w.string(&currency.currency);
                    w.u32(name);
                }
                w.pad();
                w.f64s(currencies.iter().map(|currency| currency.balance.used));
                w.f64s(currencies.iter().map(|currency| currency.balance.free));
                w.f64s(currencies.iter().map(|currency| currency.balance.total));
                w.f64s(currencies.iter().map(|currency| currency.balance.upnl));
                if self.preserve_unspecified_collateral && self.coin_balances.is_none() {
                    w.u32s([
                        wire::balance_currency_mode::MAGIC,
                        wire::balance_currency_mode::COLLATERAL_UNSPECIFIED,
                    ]);
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

    fn market_ref(&self, symbol: &str) -> u32 {
        self.market_index(symbol).unwrap_or(wire::NONE)
    }

    #[cfg(test)]
    pub(crate) fn reserve_test_tables(&self, pending: &Pending, w: &mut FrameWriter<'_>) {
        self.reserve_tables(pending, w);
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
            #[cfg(feature = "prediction")]
            if let Some(markets) = &self.prediction_markets {
                bytes += table(markets.len(), 8, 1);
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
                        indexed_strings += 1;
                        string_refs += 1;
                        string_bytes += value.len();
                    }
                }
            }
            #[cfg(any(test, feature = "product-semantics"))]
            {
                if let Some(rows) = &self.instrument_classifications {
                    bytes += table(rows.len(), 4, 0);
                    for row in rows {
                        for value in [&row.symbol_type, &row.market_region, &row.underlying_ticker]
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
                if let Some(rows) = &self.derivative_products {
                    bytes += table(rows.len(), 7, 2);
                    for row in rows {
                        for value in [
                            Some(&row.underlying),
                            row.settlement_asset.as_ref(),
                            row.premium_asset.as_ref(),
                            row.style.as_ref(),
                            row.settlement.as_ref(),
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
            }
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
            if self
                .tickers
                .iter()
                .any(|ticker| self.market_index(&ticker.symbol).is_none())
            {
                bytes += self.tickers.len() * 8;
                for ticker in &self.tickers {
                    for value in [&ticker.symbol, &ticker.id] {
                        strings += 1;
                        indexed_strings += 1;
                        string_refs += 1;
                        string_bytes += value.len();
                    }
                }
            }
        } else if !pending.ticker_deltas.is_empty() {
            bytes += 8 + pending
                .ticker_deltas
                .iter()
                .map(|(_, mask)| (2 + mask.count_ones() as usize) * 8)
                .sum::<usize>();
        }
        if changes.orders {
            let floats = if self.orders.iter().any(|o| {
                o.extra
                    .as_ref()
                    .and_then(|e| e.filled_in_placement_units)
                    .is_some()
            }) {
                9
            } else if self
                .orders
                .iter()
                .any(|o| o.extra.as_ref().and_then(|e| e.remaining_native).is_some())
            {
                8
            } else {
                7
            };
            bytes += table(self.orders.len(), 5, floats);
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
            #[cfg(any(test, feature = "product-semantics"))]
            if let Some(rows) = &self.position_greeks {
                bytes += table(rows.len(), 2, 4);
                for row in rows {
                    for value in [&row.position_id, &row.greeks.units] {
                        strings += 1;
                        indexed_strings += 1;
                        string_refs += 1;
                        string_bytes += value.len();
                    }
                }
            }
            #[cfg(feature = "prediction")]
            if let Some(holdings) = &self.prediction_holdings {
                bytes += table(holdings.len(), 2, 0);
            }
            if self.position_ids {
                bytes += (self.positions.len() * 4).next_multiple_of(8);
                for position in &self.positions {
                    strings += 1;
                    indexed_strings += 1;
                    string_refs += 1;
                    string_bytes += position.id.len();
                }
            }
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
            if self.coin_balances.is_none() && self.currency_balances.is_some() {
                bytes += 8;
            }
            if let Some(coins) = &self.coin_balances {
                bytes += (4 + coins.len() * 4).next_multiple_of(8) + coins.len() * 16;
                for coin in coins {
                    strings += 1;
                    indexed_strings += 1;
                    string_refs += 1;
                    string_bytes += coin.base.len();
                }
            }
            if let Some(currencies) = &self.currency_balances {
                bytes += (4 + currencies.len() * 4).next_multiple_of(8) + currencies.len() * 32;
                if self.preserve_unspecified_collateral && self.coin_balances.is_none() {
                    bytes += 8;
                }
                for currency in currencies {
                    strings += 1;
                    indexed_strings += 1;
                    string_refs += 1;
                    string_bytes += currency.currency.len();
                }
            }
        }
        if changes.status {
            bytes += 24;
        }
        if string_refs > 0 {
            bytes += (8 + 4 + (string_refs + 1) * 4 + string_bytes).next_multiple_of(8);
        }
        w.reserve(bytes, strings, indexed_strings);
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
    fn currency_footer_is_opt_in_and_preserves_every_legacy_byte() {
        for count in [0, 1, 2, 63, 64] {
            for coins in [None, Some(0), Some(1)] {
                let mut store = Store {
                    balance: Balance {
                        used: 7.,
                        free: 11.,
                        total: 18.,
                        upnl: -2.,
                    },
                    ..Store::default()
                };
                if let Some(coins) = coins {
                    store.set_coin_balances(
                        (0..coins)
                            .map(|_| CoinBalance {
                                base: "BTC".into(),
                                amount: 0.25,
                                notional: 12500.,
                            })
                            .collect(),
                    );
                }
                store.set_currency_balances(
                    (0..count)
                        .map(|index| CurrencyBalance {
                            currency: format!("CURRENCY{index}"),
                            balance: Balance {
                                used: 1. + index as f64,
                                free: 2.,
                                total: 3. + index as f64,
                                upnl: -4.,
                            },
                        })
                        .collect(),
                );
                let frame = balance_frame(&mut store);
                let payload = wire::sections(&frame)
                    .find(|(k, _)| *k == section::BALANCE)
                    .unwrap()
                    .1;
                let mut expected = Vec::new();
                for value in [7f64, 11., 18., -2.] {
                    expected.extend(value.to_le_bytes());
                }
                let n = coins.unwrap_or(0);
                expected.extend((n as u32).to_le_bytes());
                if n != 0 {
                    expected.extend(0u32.to_le_bytes());
                }
                expected.resize(expected.len().next_multiple_of(8), 0);
                if n != 0 {
                    expected.extend(0.25f64.to_le_bytes());
                    expected.extend(12500f64.to_le_bytes());
                }
                expected.extend((count as u32).to_le_bytes());
                for index in 0..count {
                    expected.extend(((n + index) as u32).to_le_bytes());
                }
                expected.resize(expected.len().next_multiple_of(8), 0);
                for column in 0..4 {
                    for index in 0..count {
                        let value = [1. + index as f64, 2., 3. + index as f64, -4.][column];
                        expected.extend(value.to_le_bytes());
                    }
                }
                assert_eq!(payload, expected, "legacy count={count} coins={coins:?}");
                store.preserve_unspecified_collateral();
                let mut writer = FrameWriter::new(vec![], 0.);
                store.write(&store.everything(), &mut writer);
                let frame = writer.finish();
                let payload = wire::sections(&frame)
                    .find(|(k, _)| *k == section::BALANCE)
                    .unwrap()
                    .1;
                if coins.is_none() {
                    expected.extend(*b"BCM1");
                    expected.extend(1u32.to_le_bytes());
                }
                assert_eq!(payload, expected, "opt-in changes only its footer");
            }
        }
        let mut store = Store::default();
        store.preserve_unspecified_collateral();
        assert!(
            !store.take_pending().changes.balance,
            "opt-in alone cannot publish a fabricated balance"
        );
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
        store.preserve_unspecified_collateral();
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
            (value(48), value(56), value(64), value(72)),
            (0.75, 1.25, 2., 0.)
        );
        assert_eq!(&payload[80..84], b"BCM1");
        assert_eq!(u32::from_le_bytes(payload[84..88].try_into().unwrap()), 1);
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
        assert_eq!(payload.len(), 56);
        assert_eq!(u32::from_le_bytes(payload[40..44].try_into().unwrap()), 0);
        assert_eq!(&payload[48..52], b"BCM1");
        assert_eq!(u32::from_le_bytes(payload[52..56].try_into().unwrap()), 1);
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
    fn semantic_tables_reserve_with_legacy_tails_and_refresh_ticker_indices() {
        use crate::model::{OptionGreeks, OrderExtra, OrderSide, OrderType};
        for count in [1usize, 2, 63, 64] {
            let mut store = market_position_store();
            let template = store.positions[0].clone();
            store.set_tickers(vec![ticker("M0")]);
            store.set_markets(
                (0..count)
                    .map(|index| market(&format!("M{index}")))
                    .collect(),
            );
            store.update_ticker("M0", |row| row.last = 123.);
            assert_eq!(store.ticker("M0").unwrap().last, 123.);
            store.set_positions(
                (0..count)
                    .map(|index| Position {
                        id: format!("position-{index}"),
                        symbol: format!("M{index}"),
                        realized_pnl: Some(7.5),
                        ..template.clone()
                    })
                    .collect(),
            );
            store.set_instrument_classifications(
                (0..count)
                    .map(|index| InstrumentClassification {
                        market: index as u32,
                        symbol_type: Some(format!("type-{index}")),
                        market_region: Some(format!("region-{index}")),
                        underlying_ticker: Some(format!("株式-{index}")),
                    })
                    .collect(),
            );
            store.set_derivative_products(
                (0..count)
                    .map(|index| DerivativeProduct {
                        market: index as u32,
                        kind: ProductKind::Call,
                        underlying: format!("underlying-{index}"),
                        expiry: Some(1000.),
                        strike: Some(100.),
                        settlement_asset: Some(format!("settle-{index}")),
                        premium_asset: Some(format!("premium-{index}")),
                        style: Some(format!("style-{index}")),
                        settlement: Some(format!("delivery-{index}")),
                    })
                    .collect(),
            );
            store.set_position_greeks(
                (0..count)
                    .map(|index| PositionGreeks {
                        position_id: format!("position-{index}"),
                        greeks: OptionGreeks {
                            delta: 0.5,
                            gamma: 0.01,
                            vega: 2.,
                            theta: -3.,
                            units: format!("units-{index}"),
                        },
                    })
                    .collect(),
            );
            store.set_orders(vec![Order {
                id: "native-order".into(),
                parent_id: None,
                status: OrderStatus::Open,
                symbol: "M0".into(),
                kind: OrderType::Limit,
                side: OrderSide::Buy,
                price: 100.,
                amount: 10.,
                filled: 3.,
                remaining: 7.,
                reduce_only: false,
                close: None,
                client_order_id: None,
                time_in_force: None,
                last_update: 1000.,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: Some(OrderExtra {
                    remaining_native: Some(7.),
                    filled_in_placement_units: Some(3.),
                    ..OrderExtra::default()
                }),
            }]);
            store.set_currency_balances(vec![CurrencyBalance {
                currency: "USD-collateral".into(),
                balance: Balance {
                    used: 1.,
                    free: 2.,
                    total: 3.,
                    upnl: 4.,
                },
            }]);
            store.preserve_unspecified_collateral();
            let pending = store.take_pending();
            let mut reservation = FrameWriter::new(Vec::new(), 0.);
            store.reserve_test_tables(&pending, &mut reservation);
            let capacity = reservation.finish().capacity();
            let mut writer = FrameWriter::new(Vec::with_capacity(capacity), 0.);
            store.write(&pending, &mut writer);
            let frame = writer.finish();
            assert_eq!(
                frame.capacity(),
                capacity,
                "all semantic sections and optional tails must fit their reservation"
            );
            for kind in [
                section::INSTRUMENT_CLASSIFICATIONS,
                section::DERIVATIVE_PRODUCTS,
                section::POSITION_GREEKS,
            ] {
                let payload = wire::sections(&frame)
                    .find(|(section, _)| *section == kind)
                    .unwrap()
                    .1;
                assert_eq!(
                    u32::from_le_bytes(payload[..4].try_into().unwrap()),
                    count as u32
                );
            }
            let orders = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::ORDERS)
                .unwrap()
                .1;
            assert_eq!(
                orders.len(),
                24 + 9 * 8,
                "both native order columns survive"
            );
            let positions = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::POSITIONS)
                .unwrap()
                .1;
            let realized = (4 + count * 8).next_multiple_of(8) + count * 4 * 8;
            assert_eq!(
                f64::from_le_bytes(positions[realized..realized + 8].try_into().unwrap()),
                7.5
            );
            let balance = wire::sections(&frame)
                .find(|(kind, _)| *kind == section::BALANCE)
                .unwrap()
                .1;
            assert!(
                balance
                    .windows(4)
                    .any(|bytes| bytes == wire::balance_currency_mode::MAGIC.to_le_bytes())
            );
            store.set_markets(vec![market("new"), market("M0")]);
            assert_eq!(store.ticker("M0").unwrap().last, 123.);
            assert!(store.instrument_classifications().is_none());
            assert!(store.derivative_products().is_none());
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
        assert_eq!((word(44), word(48)), (0, 1));
        assert_eq!((value(56), value(64)), (1.0, 5.0));
        assert_eq!((value(72), value(80)), (2.0, 6.0));
        assert_eq!((value(88), value(96)), (3.0, 11.0));
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
        assert_eq!(payload.len(), 96);
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
    #[test]
    fn ticker_identity_tail_preserves_orphans_and_leaves_legacy_columns_unchanged() {
        let mut store = Store::default();
        store.set_markets(vec![market("LIVEUSDT")]);
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
        store.set_markets(vec![market("LIVEUSDT")]);
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
}
