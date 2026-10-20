//! Typed exchange state plus change tracking. Once per flush the engine drains
//! the changes with [`Store::take_pending`] and writes them into the flush
//! frame with [`Store::write`].
use crate::model::{
    Balance, CoinBalance, CurrencyBalance, Loaded, MarginType, Market, Order, OrderStatus,
    Position, Ticker,
};
use crate::wire::{self, FrameWriter, loaded_flag, market_flag, section};
use std::collections::HashMap;

#[derive(Default)]
pub struct Store {
    markets: Vec<Market>,
    market_by_symbol: HashMap<Box<str>, u32>,
    market_by_id: HashMap<Box<str>, u32>,
    tickers: Vec<Ticker>,
    // Sorted row indexes borrow ticker identity instead of copying every symbol.
    ticker_by_symbol: Vec<u32>,
    orders: Vec<Order>,
    positions: Vec<Position>,
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
            let market_bytes = if changes.markets {
                self.markets
                    .iter()
                    .map(|m| 116 + m.id.len() + m.symbol.len() + m.base.len() + m.quote.len())
                    .sum()
            } else {
                0
            };
            let ticker_bytes = if changes.tickers_replaced {
                92 * self.tickers.len()
            } else {
                0
            };
            w.reserve_tables(
                market_bytes + ticker_bytes + 128,
                if changes.markets {
                    4 * self.markets.len()
                } else {
                    0
                },
            );
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
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: "LIVE".into(),
            quote: "USDT".into(),
            active: true,
            precision: crate::model::Precision {
                amount: 0.001,
                price: 0.01,
            },
            limits: crate::model::Limits {
                amount: crate::model::MinMax {
                    min: 0.001,
                    max: 1000.0,
                },
                leverage: crate::model::MinMax {
                    min: 1.0,
                    max: 100.0,
                },
                price: None,
            },
            contract_size: Some(1.0),
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
}
