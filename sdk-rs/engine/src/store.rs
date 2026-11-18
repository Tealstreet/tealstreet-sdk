//! Typed exchange state plus change tracking. Once per flush the engine drains
//! the changes with [`Store::take_pending`] and writes them into the flush
//! frame with [`Store::write`].
use crate::fence::{Epoch, Stamp};
use crate::model::{
    Balance, CoinBalance, CurrencyBalance, Loaded, MarginType, Market, Order, OrderStatus,
    Position, StopTriggerType, Ticker,
};
#[cfg(any(test, feature = "product-semantics"))]
use crate::model::{DerivativeProduct, InstrumentClassification, PositionGreeks, ProductKind};
#[cfg(feature = "prediction")]
use crate::model::{PredictionHolding, PredictionMarket};
use crate::order_read_cover::{Covers, OrderReadMark, OrderReadScope};
#[cfg(test)]
use crate::wire::market_flag;
use crate::wire::{self, FrameWriter, loaded_flag, section};
use std::collections::{HashMap, HashSet};

/// Each imported column owns its value (including absence) until its inputs
/// change. A size update must not discard independent native PnL evidence.
#[derive(Clone, Copy, Default)]
struct PositionNativeExtras {
    values: [Option<f64>; 2],
    imported: [bool; 2],
}

impl PositionNativeExtras {
    fn update(&mut self, position: &mut Position, update: impl FnOnce(&mut Position)) {
        // Venue-owned rows have no imported evidence to invalidate.
        let before = self
            .imported
            .iter()
            .any(|&imported| imported)
            .then(|| position.clone());
        update(position);
        if let Some(before) = before {
            let same_row = before.id == position.id && before.symbol == position.symbol;
            // Both sizing inputs matter: which one supplied the native quantity
            // depends on the producer's units/catalogue, not the receiver's.
            self.imported[0] &= same_row
                && before.contracts.to_bits() == position.contracts.to_bits()
                && before.notional.to_bits() == position.notional.to_bits();
            self.imported[1] &=
                same_row && before.unrealized_pnl.to_bits() == position.unrealized_pnl.to_bits();
        }
    }
}

#[derive(Default)]
pub struct Store {
    markets: Vec<Market>,
    quantity_units: u32,
    #[cfg(any(test, feature = "product-semantics"))]
    instrument_classifications: Option<Vec<InstrumentClassification>>,
    #[cfg(any(test, feature = "underlying-tokens"))]
    underlying_tokens: Option<Vec<crate::model::UnderlyingToken>>,
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
    ticker_by_symbol: HashMap<Box<str>, u32>,
    orders: Vec<Order>,
    /// Broad late-ACK fence: every row change advances it. Kept separate from
    /// the snapshot fence by docs/plans/RUST_ORDER_FENCE_DOMAINS.md.
    orders_revision: u64,
    /// Selective read fence: only `invalidate_order_snapshots` advances it.
    order_snapshot_epoch: Epoch,
    /// Which read the rows are as fresh as, and which reads may replace them.
    read_covers: Covers,
    #[cfg(feature = "repair-mutations")]
    snapshot_begins: u64,
    positions: Vec<Position>,
    /// Parallel to positions. Frame-fed metadata is authoritative, including
    /// absent values; venue-owned rows compute their metadata at output time.
    position_native_extras: Vec<PositionNativeExtras>,
    position_ids: bool,
    balance: Balance,
    coin_balances: Option<Vec<CoinBalance>>,
    currency_balances: Option<Vec<CurrencyBalance>>,
    /// Stable symbols retain account settings across market replacement/delisting.
    settings: HashMap<Box<str>, MarketSetting>,
    #[cfg(feature = "metadata-freshness")]
    metadata: Option<Box<crate::metadata::State>>,
    hedged: bool,
    loaded: Loaded,
    latency: Option<f64>,
    changes: Changes,
    rejected_empty_catalogue: bool,
    /// Changed-field bits per ticker, in [`TICKER_FIELDS`] order.
    ticker_masks: Vec<u16>,
    /// Host time each row's last, mark and index price was last observed: changed,
    /// or explicitly reported unchanged by its venue. -inf before any data.
    ticker_seen_at: Vec<[f64; 3]>,
    /// Source-domain provenance, never the local protective-veto receipt clock.
    ticker_observed_at: Vec<[f64; 3]>,
    pub(crate) export_ticker_observations: bool,
    /// The engine's current host time, set at each entry, so rows can be dated.
    clock: f64,
    dirty_tickers: Vec<u32>,
    ticker_deltas_spare: Vec<(u32, u16)>,
    /// Funded base assets awaiting valuation; live data may add their rows after the seed.
    wanted_ticker_bases: HashSet<String>,
}

/// Linear admission authority for one logical order read, including all pages.
/// It cannot be cloned or serialized; only its issuing Store can consume it.
#[derive(Debug)]
pub struct OrderSnapshotToken(
    Stamp,
    /// Order of `begin_orders_snapshot` calls, for repair retirement proofs.
    #[cfg(feature = "repair-mutations")]
    pub(crate) u64,
);

/// Packed ticker delta field order; matches `PACKED_TICKER_FIELDS` in
/// `packages/safe-cex/src/store/store.base.ts`.
pub const TICKER_FIELDS: [&str; 12] = [
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
    "midPrice",
];

fn ticker_values(t: &Ticker) -> [f64; 12] {
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
        t.mid_price.unwrap_or(f64::NAN),
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
    /// Every market's quantity convention, written into its MARKETS flags.
    pub fn set_quantity_units(&mut self, units: crate::QuantityUnits) {
        self.quantity_units = units as u32;
    }

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

    /// An empty catalogue is never authoritative: a venue outage can answer an
    /// empty list, which would otherwise mark markets loaded and wipe the
    /// previous catalogue. The engine reports each rejection as a warning.
    pub fn set_markets(&mut self, mut markets: Vec<Market>) {
        for market in &mut markets {
            crate::market_output::prepare(market, self.quantity_units);
        }
        if markets.is_empty() {
            self.rejected_empty_catalogue = true;
            return;
        }
        let reindexed = self.markets.len() != markets.len()
            || self
                .markets
                .iter()
                .zip(&markets)
                .any(|(old, new)| old.symbol != new.symbol || old.id != new.id);
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
        #[cfg(any(test, feature = "product-semantics"))]
        {
            self.instrument_classifications = None;
            self.derivative_products = None;
        }
        #[cfg(any(test, feature = "underlying-tokens"))]
        {
            self.underlying_tokens = None;
        }
        #[cfg(feature = "prediction")]
        {
            let had_prediction_markets = self.prediction_markets.take().is_some();
            if self.prediction_holdings.take().is_some() || had_prediction_markets {
                self.changes.positions = true;
            }
        }
        self.changes.markets = true;
        if reindexed {
            self.changes.tickers_replaced |= !self.tickers.is_empty();
            self.changes.orders |= !self.orders.is_empty();
            self.changes.positions |= !self.positions.is_empty();
            self.changes.settings |= !self.settings.is_empty();
        }
        self.set_loaded(|loaded| loaded.markets = true);
    }

    pub(crate) fn take_rejected_empty_catalogue(&mut self) -> bool {
        std::mem::take(&mut self.rejected_empty_catalogue)
    }

    pub(crate) fn rejected_empty_catalogue(&self) -> bool {
        self.rejected_empty_catalogue
    }

    pub(crate) fn markets_pending(&self) -> bool {
        self.changes.markets
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

    #[cfg(any(test, feature = "underlying-tokens"))]
    pub fn underlying_tokens(&self) -> Option<&[crate::model::UnderlyingToken]> {
        self.underlying_tokens.as_deref()
    }

    /// Replace underlying tokens after `set_markets`; indices belong to that snapshot.
    #[cfg(any(test, feature = "underlying-tokens"))]
    pub fn set_underlying_tokens(&mut self, tokens: Vec<crate::model::UnderlyingToken>) {
        if self.underlying_tokens.as_ref() != Some(&tokens) {
            self.underlying_tokens = Some(tokens);
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
        // Input-order insertion keeps the last duplicate as the lookup winner.
        // Rebuilding releases obsolete keys and capacity after replacement.
        self.ticker_by_symbol = self
            .tickers
            .iter()
            .enumerate()
            .map(|(index, ticker)| (ticker.symbol.as_str().into(), index as u32))
            .collect();
        self.changes.tickers_replaced = true;
        self.ticker_masks = vec![0; self.tickers.len()];
        self.ticker_seen_at = vec![[self.clock; 3]; self.tickers.len()];
        self.ticker_observed_at = vec![[self.clock; 3]; self.tickers.len()];
        self.dirty_tickers.clear();
        self.set_loaded(|loaded| loaded.tickers = true);
    }

    /// The engine's host time, for dating ticker changes.
    pub(crate) fn set_clock(&mut self, now: f64) {
        if now.is_finite() {
            self.clock = now;
        }
    }

    /// When this symbol's `reference` price was last observed, on the engine's host clock.
    pub fn reference_seen_at(&self, symbol: &str, reference: StopTriggerType) -> Option<f64> {
        let slot = match reference {
            StopTriggerType::LastPrice => 0,
            StopTriggerType::MarkPrice => 1,
            StopTriggerType::IndexPrice => 2,
        };
        self.ticker_index(symbol)
            .and_then(|index| self.ticker_seen_at.get(index as usize))
            .map(|seen| seen[slot])
    }

    /// A venue message reported these fields (`TICKER_FIELDS` bits) for `symbol`,
    /// changed or not; delta omissions are not observations.
    pub fn observe_ticker_fields(&mut self, symbol: &str, mask: u16) {
        if let Some(index) = self.ticker_index(symbol) {
            self.mark_references_seen(index, mask);
            self.mark_observations_seen(index, mask);
        }
    }

    // Bits 2, 3 and 4 of `TICKER_FIELDS` are last, mark and index.
    fn mark_references_seen(&mut self, index: u32, mask: u16) {
        if let Some(seen) = self.ticker_seen_at.get_mut(index as usize) {
            for (slot, bit) in [2, 3, 4].into_iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    seen[slot] = self.clock;
                }
            }
        }
    }

    // High mask bits track observation changes independently of value deltas.
    fn set_observation_times(&mut self, index: u32, times: [f64; 3]) {
        if let Some(seen) = self.ticker_observed_at.get_mut(index as usize) {
            let mut mask = 0;
            for slot in 0..3 {
                if seen[slot].to_bits() != times[slot].to_bits() {
                    seen[slot] = times[slot];
                    mask |= 1 << (12 + slot);
                }
            }
            if mask != 0 && self.export_ticker_observations {
                let dirty = &mut self.ticker_masks[index as usize];
                if *dirty == 0 {
                    self.dirty_tickers.push(index);
                }
                *dirty |= mask;
            }
        }
    }

    // Bits 2, 3 and 4 of `TICKER_FIELDS` are last, mark and index.
    fn mark_observations_seen(&mut self, index: u32, mask: u16) {
        if let Some(mut times) = self.ticker_observed_at.get(index as usize).copied() {
            for (slot, bit) in [2, 3, 4].into_iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    times[slot] = self.clock;
                }
            }
            self.set_observation_times(index, times);
        }
    }

    pub(crate) fn apply_shared_ticker(&mut self, ticker: &crate::public_feed::PublicTicker) {
        let before = self
            .ticker_index(&ticker.symbol)
            .and_then(|index| self.ticker_observed_at.get(index as usize))
            .copied();
        self.update_ticker(&ticker.symbol, |target| ticker.apply(target));
        if let (Some(source), Some(index)) = (ticker.observed_at, self.ticker_index(&ticker.symbol))
        {
            let mut times = before.unwrap_or([f64::NEG_INFINITY; 3]);
            for (slot, value) in [ticker.last, ticker.mark, ticker.index]
                .into_iter()
                .enumerate()
            {
                if value.is_some() {
                    times[slot] = source[slot].unwrap_or(f64::NEG_INFINITY);
                }
            }
            self.set_observation_times(index, times);
        }
    }

    pub(crate) fn import_observation_times(&mut self, index: u32, times: [f64; 3]) {
        self.set_observation_times(index, times);
    }

    pub(crate) fn ticker_revision(&self) -> u64 {
        self.ticker_revision
    }

    fn ticker_index(&self, symbol: &str) -> Option<u32> {
        self.ticker_by_symbol.get(symbol).copied()
    }

    /// Recover a catalogue ticker omitted by a malformed seed when live data arrives.
    /// This does not claim that the REST ticker seed succeeded.
    pub fn ensure_ticker(&mut self, symbol: &str) -> Option<u32> {
        if let Some(index) = self.ticker_index(symbol) {
            return Some(index);
        }
        let market = self.market(symbol)?;
        let ticker = Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            ..Ticker::default()
        };
        let index = self.tickers.len() as u32;
        self.tickers.push(ticker);
        self.ticker_masks.push(0);
        self.ticker_seen_at.push([f64::NEG_INFINITY; 3]);
        self.ticker_observed_at.push([f64::NEG_INFINITY; 3]);
        self.ticker_by_symbol.insert(symbol.into(), index);
        self.changes.tickers_replaced = true;
        Some(index)
    }

    /// Let live data add tickers for markets on this base after the seed, so a
    /// funded asset the seed omitted can still be valued.
    pub fn want_ticker_for_base(&mut self, base: &str) {
        if !self.wanted_ticker_bases.contains(base) {
            self.wanted_ticker_bases.insert(base.to_owned());
        }
    }

    /// Mutate ticker values, keeping its symbol identity; changed fields go out as deltas.
    pub fn update_ticker(&mut self, symbol: &str, update: impl FnOnce(&mut Ticker)) {
        // An existing row is the hot path: live feeds update every tick.
        let index = match self.ticker_index(symbol) {
            Some(index) => index,
            None => {
                // Before the seed, live data may recover a catalogue row;
                // afterwards the seed decides which markets have tickers (as TS
                // does), except for a funded asset a venue still needs to value.
                let recover = !self.loaded.tickers
                    || (!self.wanted_ticker_bases.is_empty()
                        && self
                            .market(symbol)
                            .is_some_and(|market| self.wanted_ticker_bases.contains(&market.base)));
                match recover.then(|| self.ensure_ticker(symbol)).flatten() {
                    Some(index) => index,
                    None => return,
                }
            }
        };
        self.update_ticker_at(index, update);
    }

    /// Mutate a row of the current ticker table; rebuild cached indexes on replacement.
    pub fn update_ticker_at(&mut self, index: u32, update: impl FnOnce(&mut Ticker)) {
        self.update_ticker_at_observed(index, 0, update);
    }

    /// Update a current ticker row and record the fields a venue actually observed,
    /// including unchanged values. Cached indexes must be rebuilt on replacement.
    pub fn update_ticker_at_observed(
        &mut self,
        index: u32,
        observed: u16,
        update: impl FnOnce(&mut Ticker),
    ) {
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
        let reported = mask | observed;
        if reported == 0 {
            return;
        }
        self.mark_references_seen(index, reported);
        self.mark_observations_seen(index, reported);
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

    /// Late-ACK reconciliation only; logical reads use `begin_orders_snapshot`.
    pub(crate) fn orders_revision(&self) -> u64 {
        self.orders_revision
    }

    /// Capture once before issuing a logical order read. Exhaustion is permanent.
    pub fn begin_orders_snapshot(&mut self) -> Option<OrderSnapshotToken> {
        #[cfg(feature = "repair-mutations")]
        {
            let stamp = self.order_snapshot_epoch.stamp()?;
            self.snapshot_begins += 1;
            Some(OrderSnapshotToken(stamp, self.snapshot_begins))
        }
        #[cfg(not(feature = "repair-mutations"))]
        self.order_snapshot_epoch.stamp().map(OrderSnapshotToken)
    }

    /// How many order snapshots have begun; a later token's begin is larger.
    #[cfg(feature = "repair-mutations")]
    pub(crate) fn snapshot_begins(&self) -> u64 {
        self.snapshot_begins
    }

    /// Read-only checks do not consume authority (for page joins and auth evidence).
    pub fn orders_snapshot_current(&self, token: &OrderSnapshotToken) -> bool {
        self.order_snapshot_epoch.current(&token.0)
    }

    /// Accepted native writes, including absent-row ACKs, invalidate older reads.
    /// Call explicitly at the venue's write boundaries; WS row admission and
    /// sibling snapshot publication need not advance this selective fence.
    pub fn invalidate_order_snapshots(&mut self) {
        self.order_snapshot_epoch.advance();
    }

    /// Validate all authority before native-cache application AND row publication.
    /// The callback receives current rows read-only and returns owned final rows.
    /// It must be infallible and synchronous: no I/O, command callbacks
    /// or reborrowing this Store. This is not a panic-recovery transaction.
    pub fn commit_orders_snapshot<I, F>(&mut self, tokens: I, publish: F) -> bool
    where
        I: IntoIterator<Item = OrderSnapshotToken>,
        F: FnOnce(&[Order]) -> Vec<Order>,
    {
        let mut any = false;
        for token in tokens {
            any = true;
            if !self.orders_snapshot_current(&token) {
                return false;
            }
        }
        if !any {
            return false;
        }
        let orders = publish(&self.orders);
        self.set_orders(orders);
        true
    }

    #[cfg(all(test, feature = "order-locks"))]
    pub(crate) fn account_read_cover(&self) -> Option<f64> {
        self.read_covers.account()
    }

    /// Every required order read has answered: orders are loaded, rows as they are.
    pub fn set_orders_loaded(&mut self) {
        self.set_loaded(|loaded| loaded.orders = true);
    }

    /// The row sequence a read released now records (`OrderReadMark::seq`).
    pub fn order_read_sequence(&self) -> u64 {
        self.read_covers.sequence()
    }

    /// What `commit_orders_read` with these rows would refuse, decided now and
    /// changing nothing: `None` when it would refuse the whole read. A venue
    /// keeping its own per-order state applies it only for symbols this admits,
    /// then commits the same rows in the same handler.
    pub fn orders_read_refusals(
        &self,
        read: OrderReadMark,
        scope: OrderReadScope<'_>,
        in_scope: impl Fn(&str) -> bool,
        rows: &[Order],
    ) -> Option<Vec<String>> {
        let selects = |symbol: &str| match scope {
            OrderReadScope::Account { .. } => in_scope(symbol),
            OrderReadScope::Symbols(listed) => listed.contains(&symbol),
        };
        let (_, admission) = self.admit_orders_read(read, scope, &selects, rows);
        (!admission.whole_scope_refused).then_some(admission.refused)
    }

    /// The symbols a read speaks for, and which of them it may replace.
    fn admit_orders_read(
        &self,
        read: OrderReadMark,
        scope: OrderReadScope<'_>,
        selects: &dyn Fn(&str) -> bool,
        rows: &[Order],
    ) -> (Vec<String>, crate::order_read_cover::Admission) {
        let mut candidates: Vec<String> = Vec::new();
        let listed: &[&str] = match scope {
            OrderReadScope::Symbols(listed) => listed,
            OrderReadScope::Account { .. } => &[],
        };
        // A symbol changed after the release is a candidate even with no row
        // left: provenance outlives removal.
        for symbol in self
            .orders
            .iter()
            .chain(rows.iter())
            .map(|order| order.symbol.as_str())
            .chain(listed.iter().copied())
            .chain(self.read_covers.changed_after(read))
        {
            if selects(symbol) && !candidates.iter().any(|known| known == symbol) {
                candidates.push(symbol.to_owned());
            }
        }
        let admission = self
            .read_covers
            .admit(read, scope, candidates.iter().map(String::as_str));
        (candidates, admission)
    }

    /// Commit one read's rows for `scope`, atomically with the cover it proves.
    /// A symbol whose rows changed after the read was released keeps them, and
    /// the read proves nothing for it; an account scope committed after the
    /// read refuses all of it. `in_scope` selects an account scope's symbols.
    /// `unproven` names symbols whose rows the read could not map (rejected or
    /// unsupported rows); its rows still commit, but it proves no cover for
    /// them, and an empty symbol (a row naming no market) proves none at all.
    /// Returns the refused symbols, or `None` when the whole read was refused.
    pub fn commit_orders_read(
        &mut self,
        read: OrderReadMark,
        scope: OrderReadScope<'_>,
        in_scope: impl Fn(&str) -> bool,
        rows: Vec<Order>,
        unproven: &[String],
    ) -> Option<Vec<String>> {
        let selects = |symbol: &str| match scope {
            OrderReadScope::Account { .. } => in_scope(symbol),
            OrderReadScope::Symbols(listed) => listed.contains(&symbol),
        };
        let (candidates, admission) = self.admit_orders_read(read, scope, &selects, &rows);
        if admission.whole_scope_refused {
            return None;
        }
        let refused = admission.refused;
        let kept = |order: &Order| !selects(&order.symbol) || refused.contains(&order.symbol);
        let mut next: Vec<Order> = self
            .orders
            .iter()
            .filter(|order| kept(order))
            .cloned()
            .collect();
        next.extend(rows.into_iter().filter(|order| !kept(order)));
        let written: Vec<String> = candidates
            .into_iter()
            .filter(|symbol| !refused.contains(symbol))
            .collect();
        let complete = self.read_covers.committed(
            read,
            scope,
            &written,
            refused.is_empty() && unproven.is_empty(),
            unproven,
        );
        if next != self.orders {
            self.orders_revision = self.orders_revision.saturating_add(1);
            self.orders = next;
            self.changes.orders = true;
        } else if self.read_covers.any() {
            // The cover moved: send it, beside the unchanged rows.
            self.changes.orders = true;
        }
        if complete {
            self.set_loaded(|loaded| loaded.orders = true);
        }
        Some(refused)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn exhaust_order_snapshots_for_test(&mut self) {
        self.order_snapshot_epoch.exhaust_for_test();
    }

    /// Replace every order with a venue snapshot.
    /// Frame ingest: a data source's snapshot replaces every account section,
    /// so they start empty and unloaded (markets and tickers stay).
    pub fn reset_account(&mut self) {
        self.orders.clear();
        self.orders_revision = self.orders_revision.saturating_add(1);
        self.read_covers.reset();
        self.positions.clear();
        self.position_native_extras.clear();
        self.balance = Balance::default();
        self.coin_balances = None;
        self.currency_balances = None;
        self.settings.clear();
        self.hedged = false;
        self.loaded.balance = false;
        self.loaded.orders = false;
        self.loaded.positions = false;
        self.changes.orders = true;
        self.changes.positions = true;
        self.changes.balance = true;
        self.changes.settings = true;
        self.changes.status = true;
    }

    pub fn set_orders(&mut self, orders: Vec<Order>) {
        self.orders_revision = self.orders_revision.saturating_add(1);
        self.read_covers.mutated_all();
        self.orders = orders;
        self.changes.orders = true;
        self.set_loaded(|loaded| loaded.orders = true);
    }

    /// Insert or replace by id. A closed or canceled row stays, with its
    /// status, until the next snapshot replaces the list.
    pub fn upsert_order(&mut self, order: Order) {
        self.orders_revision = self.orders_revision.saturating_add(1);
        self.read_covers.mutated(&order.symbol);
        match self
            .orders
            .iter()
            .position(|existing| existing.id == order.id)
        {
            Some(index) => {
                if self.orders[index].symbol != order.symbol {
                    self.read_covers.mutated(&self.orders[index].symbol);
                }
                self.orders[index] = order;
            }
            None => self.orders.push(order),
        }
        self.changes.orders = true;
    }

    /// Mutate one order in place; returns whether it exists.
    pub fn update_order(&mut self, id: &str, update: impl FnOnce(&mut Order)) -> bool {
        let Some(order) = self.orders.iter_mut().find(|order| order.id == id) else {
            return false;
        };
        self.orders_revision = self.orders_revision.saturating_add(1);
        let before = order.symbol.clone();
        update(order);
        let after = (order.symbol != before).then(|| order.symbol.clone());
        self.read_covers.mutated(&before);
        if let Some(after) = after {
            self.read_covers.mutated(&after);
        }
        self.changes.orders = true;
        true
    }

    pub fn set_order_status(&mut self, id: &str, status: OrderStatus) -> bool {
        self.update_order(id, |order| order.status = status)
    }

    pub fn remove_orders(&mut self, ids: &[&str]) {
        let before = self.orders.len();
        let covers = &mut self.read_covers;
        self.orders.retain(|order| {
            let removed = ids.contains(&order.id.as_str());
            if removed {
                covers.mutated(&order.symbol);
            }
            !removed
        });
        if self.orders.len() != before {
            self.orders_revision = self.orders_revision.saturating_add(1);
            self.changes.orders = true;
        }
    }

    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    pub fn set_positions(&mut self, positions: Vec<Position>) {
        self.position_native_extras = vec![PositionNativeExtras::default(); positions.len()];
        self.positions = positions;
        #[cfg(any(test, feature = "product-semantics"))]
        self.prune_position_greeks();
        #[cfg(feature = "prediction")]
        self.prune_prediction_holdings();
        self.changes.positions = true;
        self.set_loaded(|loaded| loaded.positions = true);
    }

    /// Keep the producer's native values and presence bits instead of deriving
    /// them again from the receiving store's possibly missing/stale catalogue.
    pub(crate) fn set_frame_positions(
        &mut self,
        positions: Vec<Position>,
        native_extras: Vec<[Option<f64>; 2]>,
    ) {
        assert_eq!(positions.len(), native_extras.len());
        self.set_positions(positions);
        self.position_native_extras = native_extras
            .into_iter()
            .map(|values| PositionNativeExtras {
                values,
                imported: [true; 2],
            })
            .collect();
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
                self.position_native_extras.remove(index);
            }
            (Some(index), false) => {
                self.position_native_extras[index].update(&mut self.positions[index], |stored| {
                    *stored = position;
                });
            }
            (None, true) => return,
            (None, false) => {
                self.positions.push(position);
                self.position_native_extras
                    .push(PositionNativeExtras::default());
            }
        }
        #[cfg(any(test, feature = "product-semantics"))]
        self.prune_position_greeks();
        #[cfg(feature = "prediction")]
        self.prune_prediction_holdings();
        self.changes.positions = true;
    }

    /// Change positions in place; rows left with zero contracts are removed.
    pub fn update_positions(&mut self, mut update: impl FnMut(&mut Position)) {
        for (position, native) in self
            .positions
            .iter_mut()
            .zip(&mut self.position_native_extras)
        {
            native.update(position, &mut update);
        }
        // Filter the sidecar using the same rows before compacting positions.
        let mut rows = self.positions.iter();
        self.position_native_extras
            .retain(|_| rows.next().is_some_and(|p| p.contracts != 0.0));
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

    /// Back to never supplied: a frame-fed source that no longer reports collateral.
    pub fn clear_coin_balances(&mut self) {
        if self.coin_balances.take().is_some() {
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
        if !self.loaded.balance || balance != self.balance {
            self.balance = balance;
            self.changes.balance = true;
        }
        self.set_loaded(|loaded| loaded.balance = true);
    }

    pub fn setting(&self, symbol: &str) -> Option<MarketSetting> {
        self.settings.get(symbol).copied()
    }

    /// Replace every market's settings.
    pub fn set_settings(&mut self, settings: impl IntoIterator<Item = (u32, MarketSetting)>) {
        self.settings = settings
            .into_iter()
            .filter_map(|(index, setting)| {
                self.markets
                    .get(index as usize)
                    .map(|market| (market.symbol.clone().into_boxed_str(), setting))
            })
            .collect();
        self.changes.settings = true;
        #[cfg(feature = "metadata-freshness")]
        if let Some(metadata) = &mut self.metadata {
            metadata.changed(None);
        }
    }

    /// Replace every market's settings, keyed by symbol.
    pub fn replace_settings<'a>(
        &mut self,
        settings: impl IntoIterator<Item = (&'a str, MarketSetting)>,
    ) {
        let settings: HashMap<Box<str>, MarketSetting> = settings
            .into_iter()
            .map(|(symbol, setting)| (symbol.into(), setting))
            .collect();
        if settings != self.settings {
            self.settings = settings;
            self.changes.settings = true;
            #[cfg(feature = "metadata-freshness")]
            if let Some(metadata) = &mut self.metadata {
                metadata.changed(None);
            }
        }
    }

    pub fn update_setting(&mut self, symbol: &str, update: impl FnOnce(&mut MarketSetting)) {
        let setting = self.settings.entry(symbol.into()).or_default();
        let before = *setting;
        update(setting);
        if *setting != before {
            self.changes.settings = true;
            #[cfg(feature = "metadata-freshness")]
            if let Some(metadata) = &mut self.metadata {
                metadata.changed(Some(symbol));
            }
        }
    }
}

#[cfg(feature = "metadata-freshness")]
impl Store {
    fn metadata_state(&mut self) -> &mut crate::metadata::State {
        self.metadata.get_or_insert_with(Default::default)
    }

    pub fn begin_metadata(
        &mut self,
        symbol: &str,
        publish: bool,
        fallback: crate::metadata::Fallback,
    ) -> Result<crate::metadata::Token, crate::VenueError> {
        self.metadata_state().begin(symbol, publish, fallback)
    }

    pub fn metadata_eligible(&self, token: &crate::metadata::Token) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|metadata| metadata.eligible(token))
    }

    #[cfg(feature = "http-plans")]
    pub fn metadata_work(
        &self,
        key: &crate::metadata::WorkKey,
    ) -> Option<crate::engine::ExecutionHandle> {
        self.metadata.as_ref()?.lookup_work(key)
    }
    #[cfg(feature = "http-plans")]
    pub fn metadata_work_space(&self, symbol: &str) -> bool {
        self.metadata
            .as_ref()
            .is_none_or(|state| state.work_space(symbol))
    }
    #[cfg(feature = "http-plans")]
    pub fn metadata_executions(
        &self,
    ) -> Vec<(crate::metadata::Token, crate::engine::ExecutionHandle)> {
        self.metadata
            .as_ref()
            .map_or_else(Vec::new, |state| state.work_handles())
    }
    #[cfg(feature = "http-plans")]
    pub fn bind_metadata_work(
        &mut self,
        key: crate::metadata::WorkKey,
        handle: crate::engine::ExecutionHandle,
    ) -> Result<crate::metadata::Token, crate::VenueError> {
        self.metadata_state().bind_work(key, handle)
    }
    #[cfg(feature = "http-plans")]
    pub fn retire_metadata_work(&mut self, token: &crate::metadata::Token) {
        if let Some(state) = self.metadata.as_mut() {
            state.retire_work(token);
        }
    }

    pub fn metadata_observation(&mut self) -> crate::metadata::Observation {
        self.metadata_state().observation()
    }

    pub fn invalidate_metadata(&mut self, symbol: Option<&str>) {
        self.metadata_state().invalidate(symbol);
    }
    pub fn metadata_revision(&mut self) -> crate::metadata::Observation {
        self.metadata_state().revision()
    }
    pub fn metadata_revision_current(&self, stamp: &crate::metadata::Observation) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|state| state.revision_current(stamp))
    }

    /// A background read cannot overwrite a newer read or settings write.
    pub fn observe_setting(
        &mut self,
        symbol: &str,
        stamp: &crate::metadata::Observation,
        setting: crate::metadata::ObservedSetting,
    ) -> bool {
        if !self.metadata_state().observe(symbol, stamp) {
            return false;
        }
        self.settings.insert(symbol.into(), setting.0);
        self.changes.settings = true;
        true
    }

    pub fn metadata_write_started(
        &mut self,
        command: crate::CommandId,
        symbol: Option<&str>,
    ) -> Result<(), crate::VenueError> {
        self.metadata_state().write_started(command, symbol)
    }
    pub fn metadata_write_finished(&mut self, command: crate::CommandId) {
        if let Some(metadata) = self.metadata.as_mut() {
            metadata.write_finished(command);
        }
    }
    pub fn metadata_write_pending(&self, symbol: &str) -> bool {
        self.metadata
            .as_ref()
            .is_some_and(|metadata| metadata.write_pending(symbol))
    }

    /// Completes ownership and publishes a full validated proposal atomically.
    /// Native caches update before the separate engine-owned Plan terminal.
    pub fn finish_metadata(
        &mut self,
        token: &crate::metadata::Token,
        result: Result<crate::metadata::Proposal, crate::VenueError>,
    ) -> Option<crate::metadata::Resolution> {
        let symbol = self.metadata.as_ref()?.symbol(token)?.to_owned();
        let cached = self.setting(&symbol);
        let (completion, publication) = self.metadata.as_mut()?.complete(token, result, cached)?;
        if let Some(setting) = publication {
            self.settings.insert(symbol.into_boxed_str(), setting);
            self.changes.settings = true;
        }
        Some(completion)
    }
}

impl Store {
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

    pub fn latency(&self) -> Option<f64> {
        self.latency
    }

    pub fn set_latency(&mut self, latency: f64) {
        self.set_latency_sample(Some(latency));
    }

    pub(crate) fn set_latency_sample(&mut self, latency: Option<f64>) {
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

    /// Full private state, including an unflushed catalog needed for its indexes.
    /// Change tracking and public events are left alone.
    pub fn account_snapshot(&self) -> Pending {
        Pending {
            changes: Changes {
                markets: self.changes.markets,
                orders: true,
                positions: true,
                balance: true,
                settings: true,
                status: true,
                ..Changes::default()
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
            let market_bytes = if changes.markets {
                self.markets
                    .iter()
                    .map(|m| 116 + m.id.len() + m.symbol.len() + m.base.len() + m.quote.len())
                    .sum()
            } else {
                0
            };
            let ticker_bytes = if changes.tickers_replaced {
                108 * self.tickers.len()
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
            #[cfg(any(test, feature = "product-semantics"))]
            {
                self.write_instrument_classifications(w);
                self.write_derivative_products(w);
            }
            #[cfg(any(test, feature = "underlying-tokens"))]
            self.write_underlying_tokens(w);
            #[cfg(feature = "prediction")]
            self.write_prediction_markets(w);
        }
        if changes.tickers_replaced {
            self.write_tickers(w);
        } else if pending
            .ticker_deltas
            .iter()
            .any(|(_, mask)| mask & 0xfff != 0)
        {
            let revision = u16::from(
                pending
                    .ticker_deltas
                    .iter()
                    .any(|(_, mask)| mask & (1 << 11) != 0),
            );
            w.begin_revision(section::TICKER_DELTAS, revision);
            for &(index, mask) in &pending.ticker_deltas {
                let mask = mask & 0xfff;
                if mask == 0 {
                    continue;
                }
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
        if self.export_ticker_observations
            && (changes.tickers_replaced
                || pending
                    .ticker_deltas
                    .iter()
                    .any(|(_, mask)| mask & 0x7000 != 0))
        {
            w.begin(section::TICKER_OBSERVATIONS);
            if changes.tickers_replaced {
                for (index, times) in self.ticker_observed_at.iter().enumerate() {
                    w.f64(index as f64);
                    w.f64s(times.iter().copied());
                }
            } else {
                for &(index, mask) in &pending.ticker_deltas {
                    if mask & 0x7000 != 0 {
                        w.f64(index as f64);
                        w.f64s(self.ticker_observed_at[index as usize]);
                    }
                }
            }
            w.end();
        }
        if changes.orders {
            self.write_orders(w);
            // Directly after ORDERS: a segmenting host keeps the two together.
            // Nothing is written until a read has proven a cover.
            if self.read_covers.any() {
                self.write_orders_read(w);
            }
        }
        if changes.positions {
            self.write_positions(w);
            #[cfg(any(test, feature = "product-semantics"))]
            self.write_position_greeks(w);
            #[cfg(feature = "prediction")]
            self.write_prediction_holdings(w);
        }
        if changes.balance {
            crate::balance_output::write(
                w,
                &self.balance,
                self.coin_balances.as_deref(),
                self.currency_balances.as_deref(),
            );
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
            let flags = flags
                | if self.latency.is_some() {
                    wire::status_flag::LATENCY_MEASURED
                } else {
                    0
                };
            w.u32s([loaded, flags]);
            w.f64(self.latency.unwrap_or(0.0));
            w.end();
        }
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

    #[cfg(any(test, feature = "underlying-tokens"))]
    fn write_underlying_tokens<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let Some(rows) = &self.underlying_tokens else {
            return;
        };
        w.begin(section::UNDERLYING_TOKENS);
        w.u32(rows.len() as u32);
        w.u32s(rows.iter().map(|row| row.market));
        for row in rows {
            let refs = [
                w.string(&row.name),
                w.optional_string(row.full_name.as_deref()),
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
        w.f64s(
            rows.iter()
                .map(|row| crate::market_output::optional(row.expiry)),
        );
        w.f64s(
            rows.iter()
                .map(|row| crate::market_output::optional(row.strike)),
        );
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
        crate::market_output::write(&self.markets, self.quantity_units, w);
    }

    fn write_tickers<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let tickers = &self.tickers;
        let mid = tickers.iter().any(|ticker| ticker.mid_price.is_some());
        w.begin_revision(section::TICKERS, u16::from(mid));
        w.u32(tickers.len() as u32);
        w.u32s(tickers.iter().map(|t| self.market_ref(&t.symbol)));
        w.pad();
        for column in 0..if mid { wire::TICKER_COLUMNS } else { 11 } {
            w.f64s(tickers.iter().map(|t| ticker_values(t)[column]));
        }
        // Delisted symbols can remain in the venue's aggregate ticker stream.
        // Preserve their identities without inventing a tradeable market row.
        if mid
            || tickers
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

    /// `ORDERS_READ` revision 1: the account cover (NaN when none), then each
    /// symbol's newer cover, on this writer's clock.
    fn write_orders_read<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let symbols: Vec<(&str, f64)> = self.read_covers.symbol_covers().collect();
        w.begin_revision(section::ORDERS_READ, 1);
        w.u32(symbols.len() as u32);
        w.pad();
        w.f64(self.read_covers.account().unwrap_or(f64::NAN));
        let refs: Vec<u32> = symbols.iter().map(|(symbol, _)| w.string(symbol)).collect();
        w.u32s(refs);
        w.pad();
        w.f64s(symbols.iter().map(|(_, sent)| *sent));
        w.end();
    }

    fn write_orders<'a>(&'a self, w: &mut FrameWriter<'a>) {
        crate::order_output::write(
            &self.orders,
            |symbol| self.market(symbol),
            |symbol| self.market_ref(symbol),
            self.quantity_units,
            w,
        );
    }

    fn write_settings<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let mut rows: Vec<_> = self
            .settings
            .iter()
            .filter_map(|(symbol, setting)| {
                crate::store_output::rails(setting)
                    .map(|rails| (self.market_ref(symbol), symbol, setting, rails))
            })
            .collect();
        rows.sort_unstable_by(|(a_index, a_symbol, _, _), (b_index, b_symbol, _, _)| {
            a_index.cmp(b_index).then(a_symbol.cmp(b_symbol))
        });
        w.begin_revision(section::SETTINGS, 1);
        w.u32(rows.len() as u32);
        w.u32s(rows.iter().map(|(market, _, _, _)| *market));
        w.u32s(
            rows.iter()
                .map(|(_, _, s, _)| s.margin_type.map_or(0, |m| m as u32 + 1)),
        );
        w.pad();
        w.f64s(rows.iter().map(|(_, _, s, _)| s.leverage));
        w.f64s(rows.iter().map(|(_, _, _, rails)| rails[0]));
        w.f64s(rows.iter().map(|(_, _, _, rails)| rails[1]));
        for (_, symbol, _, _) in &rows {
            let symbol = w.string(symbol);
            w.u32(symbol);
        }
        w.end();
    }

    fn write_positions<'a>(&'a self, w: &mut FrameWriter<'a>) {
        let positions = &self.positions;
        let extras: Vec<_> = positions
            .iter()
            .zip(&self.position_native_extras)
            .map(|(position, native)| {
                if native.imported == [true; 2] {
                    return native.values;
                }
                let derived = {
                    let market = self
                        .market_by_symbol
                        .get(position.symbol.as_str())
                        .map(|index| &self.markets[*index as usize]);
                    crate::position_output::native_extras(position, market, self.quantity_units)
                };
                std::array::from_fn(|i| {
                    if native.imported[i] {
                        native.values[i]
                    } else {
                        derived[i]
                    }
                })
            })
            .collect();
        w.begin_revision(section::POSITIONS, 2);
        w.u32(positions.len() as u32);
        w.u32s(positions.iter().map(|p| self.market_ref(&p.symbol)));
        w.u32s(positions.iter().zip(&extras).map(|(p, extra)| {
            wire::position_enums(p.side, p.margin_type, p.position_mode)
                | (u32::from(extra[0].is_some()) << 24)
                | (u32::from(extra[1].is_some()) << 25)
        }));
        w.pad();
        let columns: [fn(&Position) -> f64; 9] = [
            |p| p.entry_price,
            |p| p.notional,
            |p| crate::position_output::display(p.leverage),
            |p| crate::position_output::display(p.unrealized_pnl),
            |p| p.realized_pnl.filter(|v| v.is_finite()).unwrap_or(f64::NAN),
            |p| p.contracts,
            |p| crate::position_output::liquidation(p.liquidation_price).unwrap_or(f64::NAN),
            |p| p.margin_size.filter(|v| v.is_finite()).unwrap_or(f64::NAN),
            |p| p.last_update.filter(|v| v.is_finite()).unwrap_or(f64::NAN),
        ];
        for column in columns {
            w.f64s(positions.iter().map(column));
        }
        w.f64s(extras.iter().map(|extra| extra[0].unwrap_or(f64::NAN)));
        w.f64s(extras.iter().map(|extra| extra[1].unwrap_or(f64::NAN)));
        for position in positions {
            let id = w.string(&position.id);
            w.u32(id);
        }
        for position in positions {
            let symbol = w.string(&position.symbol);
            w.u32(symbol);
        }
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "metadata-freshness")]
    #[test]
    fn an_unrelated_read_terminal_does_not_allocate_metadata_state() {
        use crate::request_context::{Operation, Policy, RetryPolicy};
        use crate::{Action, Command, Cx, Engine, Event, Notice, Response, SocketId, Venue};
        struct Read;
        impl Venue for Read {
            type Pending = ();
            fn start(&mut self, _: &mut Cx<'_, ()>) {}
            fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
                let root = cx
                    .admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .unwrap();
                cx.done_context(&root, "[]".into()).unwrap();
            }
            fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: crate::TimerId) {}
            fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
            fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
            fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
            fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
            fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
            fn book(&self, _: &str) -> Option<&crate::book::Book> {
                None
            }
            fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
                Box::new(std::iter::empty())
            }
        }
        let mut engine: Engine<Read> = crate::test_support::seeded_engine(Read);
        assert!(engine.store().metadata.is_none());
        engine.handle(
            0.,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "BTCUSD".into(),
                interval: crate::model::Interval::M1,
                limit: Some(2),
                from_ms: None,
                to_ms: None,
            }),
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a,
                    Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]"
                ))
                .count(),
            1
        );
        assert!(engine.store().metadata.is_none());
    }

    fn snapshot_order(id: &str) -> Order {
        Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTC/USDT".into(),
            kind: crate::model::OrderType::Limit,
            side: crate::model::OrderSide::Buy,
            price: 10.0,
            amount: 2.0,
            filled: 0.0,
            remaining: 2.0,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }
    }

    /// Read covers (docs/plans/RUST_FRAME_FED_READ_COVER.md §2, H5 H8 H9 H10).
    mod read_covers {
        use super::*;
        use crate::order_read_cover::{OrderReadMark, OrderReadScope};

        const ACCOUNT: OrderReadScope<'static> = OrderReadScope::Account {
            key: "all",
            account: &["all"],
        };

        fn row(id: &str, symbol: &str) -> Order {
            Order {
                symbol: symbol.into(),
                ..snapshot_order(id)
            }
        }

        fn release(store: &Store, sent: f64) -> OrderReadMark {
            OrderReadMark {
                seq: store.order_read_sequence(),
                sent,
                epoch: 0,
            }
        }

        fn ids(store: &Store) -> Vec<&str> {
            store
                .orders()
                .iter()
                .map(|order| order.id.as_str())
                .collect()
        }

        fn sections(store: &mut Store) -> Vec<u16> {
            let pending = store.account_snapshot();
            let mut w = FrameWriter::new(Vec::new(), 0.);
            store.write(&pending, &mut w);
            crate::wire::sections(&w.finish())
                .map(|(kind, _)| kind)
                .filter(|kind| *kind != section::STRINGS)
                .collect()
        }

        // H8: an old read (sent 0) misses the write; a fresh read (sent 5) finds
        // the order; the old one lands last and must neither remove it nor move
        // the cover. Every output after it still carries the order and cover 5.
        #[test]
        fn an_older_read_landing_after_a_fresh_one_neither_removes_the_found_order_nor_lowers_the_cover()
         {
            let mut store = Store::default();
            let old = release(&store, 0.);
            let fresh = release(&store, 5.);
            let found = vec![row("lost", "BTC")];
            assert_eq!(
                store.commit_orders_read(fresh, ACCOUNT, |_| true, found, &[]),
                Some(vec![])
            );
            assert_eq!(store.read_covers.account(), Some(5.));
            assert_eq!(
                store.commit_orders_read(old, ACCOUNT, |_| true, vec![], &[]),
                None
            );
            assert_eq!(ids(&store), ["lost"]);
            assert_eq!(store.read_covers.account(), Some(5.));
            assert_eq!(
                sections(&mut store)[..2],
                [section::ORDERS, section::ORDERS_READ],
                "a snapshot carries the order and its cover, together"
            );
        }

        // H10: a read released at 5000 and a stream row at the same millisecond,
        // after it. Ordered by sequence, the read is refused for that symbol.
        #[test]
        fn a_stream_row_after_a_reads_release_at_the_same_millisecond_refuses_that_read() {
            let mut store = Store::default();
            let read = release(&store, 5_000.);
            store.upsert_order(row("streamed", "BTC"));
            assert_eq!(
                store.commit_orders_read(read, ACCOUNT, |_| true, vec![], &[]),
                Some(vec!["BTC".to_owned()])
            );
            assert_eq!(ids(&store), ["streamed"], "the stream row stays");
            assert_eq!(
                store.read_covers.account(),
                None,
                "a refused read proves nothing"
            );
            // The same read, released after that row: it speaks for it.
            let after = release(&store, 5_000.);
            assert_eq!(
                store.commit_orders_read(after, ACCOUNT, |_| true, vec![], &[]),
                Some(vec![])
            );
            assert!(store.orders().is_empty());
            assert_eq!(store.read_covers.account(), Some(5_000.));
        }

        // A missing row never erases the proof: a stream row added then removed
        // after a read's release still refuses that read's copy of it, and an
        // empty full replacement refuses the whole read.
        #[test]
        fn provenance_outlives_removed_rows_and_empty_replacements() {
            let mut store = Store::default();
            let read = release(&store, 1.);
            store.upsert_order(row("gone", "BTC"));
            store.remove_orders(&["gone"]);
            assert_eq!(
                store.commit_orders_read(read, ACCOUNT, |_| true, vec![row("gone", "BTC")], &[]),
                Some(vec!["BTC".to_owned()])
            );
            assert!(store.orders().is_empty(), "the removed row is not re-added");
            let read = release(&store, 2.);
            store.set_orders(Vec::new());
            assert_eq!(
                store.commit_orders_read(read, ACCOUNT, |_| true, vec![row("stale", "ETH")], &[]),
                None
            );
            assert!(store.orders().is_empty());
        }

        // H9: an account cover is the oldest send across its scopes.
        #[test]
        fn an_account_cover_is_the_oldest_send_across_its_scopes() {
            let scopes: &[&str] = &["core", "dex"];
            let core = OrderReadScope::Account {
                key: "core",
                account: scopes,
            };
            let dex = OrderReadScope::Account {
                key: "dex",
                account: scopes,
            };
            let mut store = Store::default();
            let old_dex = release(&store, 0.);
            let fresh_core = release(&store, 5.);
            let is_dex = |symbol: &str| symbol.starts_with("xyz:");
            store.commit_orders_read(fresh_core, core, |s| !is_dex(s), vec![], &[]);
            assert_eq!(
                store.read_covers.account(),
                None,
                "one scope is not the account"
            );
            store.commit_orders_read(old_dex, dex, is_dex, vec![], &[]);
            assert_eq!(
                store.read_covers.account(),
                Some(0.),
                "never the last scope's send"
            );
            assert_eq!(store.read_covers.cover("xyz:TSLA"), Some(0.));
        }

        // H5: per-symbol reads move only their own symbols' covers, across
        // flushes, and an older one landing late is refused.
        #[test]
        fn per_symbol_reads_never_become_an_account_cover() {
            let mut store = Store::default();
            let eth = OrderReadScope::Symbols(&["ETH"]);
            let early = release(&store, 1.);
            store.commit_orders_read(early, eth, |_| false, vec![], &[]);
            store.take_pending();
            let late_sent_early = release(&store, 2.);
            let next = release(&store, 500.);
            store.commit_orders_read(next, eth, |_| false, vec![], &[]);
            assert_eq!(store.read_covers.account(), None);
            assert_eq!(store.read_covers.cover("ETH"), Some(500.));
            assert_eq!(store.read_covers.cover("BTC"), None);
            assert_eq!(
                store.commit_orders_read(
                    late_sent_early,
                    eth,
                    |_| false,
                    vec![row("stale", "ETH")],
                    &[]
                ),
                Some(vec!["ETH".to_owned()])
            );
            assert!(store.orders().is_empty());
            assert_eq!(store.read_covers.cover("ETH"), Some(500.));
        }

        // codex-hcmq3f (#3004): rows wiped by an account reset take their cover
        // with them, and nothing goes out claiming it.
        #[test]
        fn an_account_reset_clears_every_cover() {
            let mut store = Store::default();
            let read = release(&store, 3.);
            store.commit_orders_read(read, ACCOUNT, |_| true, vec![row("a", "BTC")], &[]);
            assert_eq!(store.read_covers.account(), Some(3.));
            store.reset_account();
            assert_eq!(store.read_covers.account(), None);
            assert_eq!(store.read_covers.cover("BTC"), None);
            assert!(!sections(&mut store).contains(&section::ORDERS_READ));
            // A read released before the reset cannot restore it.
            assert_eq!(
                store.commit_orders_read(read, ACCOUNT, |_| true, vec![], &[]),
                None
            );
        }

        // codex-hcmq3f (#3004): a row added and removed after the release
        // leaves no row to name it, yet the empty read is still refused for it.
        #[test]
        fn an_empty_read_is_refused_for_a_symbol_added_and_removed_after_its_release() {
            let mut store = Store::default();
            let read = release(&store, 1.);
            store.upsert_order(row("brief", "BTC"));
            store.remove_orders(&["brief"]);
            assert_eq!(
                store.commit_orders_read(read, ACCOUNT, |_| true, vec![], &[]),
                Some(vec!["BTC".to_owned()])
            );
            assert_eq!(store.read_covers.account(), None, "not a clean read");
        }

        // codex-hcmq3f (#3005): rows a read could not map commit as they are,
        // but prove nothing: an unmapped row with no market blocks the account
        // cover, and a known symbol's blocks only that symbol's.
        #[test]
        fn unproven_rows_commit_but_prove_no_cover() {
            let mut store = Store::default();
            let read = release(&store, 1_000.);
            assert_eq!(
                store.commit_orders_read(
                    read,
                    ACCOUNT,
                    |_| true,
                    vec![row("ok", "ETH")],
                    &[String::new()]
                ),
                Some(vec![])
            );
            assert_eq!(ids(&store), ["ok"], "healthy rows still publish");
            assert_eq!(store.read_covers.account(), None);
            let symbols = OrderReadScope::Symbols(&["BTC", "ETH"]);
            let read = release(&store, 2_000.);
            store.commit_orders_read(read, symbols, |_| false, vec![], &["BTC".to_owned()]);
            assert_eq!(store.read_covers.cover("BTC"), None);
            assert_eq!(store.read_covers.cover("ETH"), Some(2_000.));
        }

        // A preview refuses exactly what the commit then refuses, and changes
        // nothing on the way.
        #[test]
        fn a_refusal_preview_matches_its_commit_and_changes_nothing() {
            let mut store = Store::default();
            let read = release(&store, 1.);
            store.upsert_order(row("streamed", "BTC"));
            let rows = vec![row("eth", "ETH")];
            let before = store.order_read_sequence();
            let preview = store.orders_read_refusals(read, ACCOUNT, |_| true, &rows);
            assert_eq!(preview, Some(vec!["BTC".to_owned()]));
            assert_eq!(store.order_read_sequence(), before);
            assert_eq!(
                store.commit_orders_read(read, ACCOUNT, |_| true, rows, &[]),
                preview
            );
        }

        // Until a read proves a cover, no output changes.
        #[test]
        fn no_cover_writes_no_section() {
            let mut store = Store::default();
            store.upsert_order(row("a", "BTC"));
            assert_eq!(
                sections(&mut store),
                [
                    section::ORDERS,
                    section::POSITIONS,
                    section::BALANCE,
                    section::SETTINGS,
                    section::STATUS
                ]
            );
        }
    }

    #[test]
    fn order_snapshot_admission_rejects_before_native_rows_readiness_and_dirty_state() {
        use std::cell::Cell;
        let mut store = Store::default();
        let stale = store.begin_orders_snapshot().unwrap();
        store.invalidate_order_snapshots();
        let native = Cell::new(7);
        assert!(!store.commit_orders_snapshot([stale], |_| {
            native.set(0);
            vec![]
        }));
        assert_eq!(native.get(), 7);
        assert!(!store.loaded().orders);
        assert!(!store.changes.orders && !store.changes.status);
        let token = store.begin_orders_snapshot().unwrap();
        let mut moved = Box::new(store);
        assert!(moved.orders_snapshot_current(&token));
        let mut foreign = Store::default();
        let other = foreign.begin_orders_snapshot().unwrap();
        assert!(!moved.commit_orders_snapshot([token, other], |_| {
            native.set(0);
            vec![]
        }));
        assert_eq!(native.get(), 7);
        assert!(!moved.loaded().orders);
        assert!(!moved.changes.orders && !moved.changes.status);
        assert!(!moved.commit_orders_snapshot([], |_| {
            native.set(0);
            vec![]
        }));
        let fresh = moved.begin_orders_snapshot().unwrap();
        moved.exhaust_order_snapshots_for_test();
        assert!(!moved.commit_orders_snapshot([fresh], |_| {
            native.set(0);
            vec![]
        }));
        moved.invalidate_order_snapshots();
        assert!(moved.begin_orders_snapshot().is_none());
        assert_eq!(native.get(), 7);
        // Accepted writes remain possible after exhaustion.
        moved.set_orders(vec![snapshot_order("accepted")]);
        assert!(moved.order("accepted").is_some());
    }

    #[test]
    fn sibling_snapshots_remain_current_and_fresh_empty_snapshots_clear_rows() {
        let mut store = Store::default();
        let native_commits = std::cell::Cell::new(0);
        let first = store.begin_orders_snapshot().unwrap();
        let sibling = store.begin_orders_snapshot().unwrap();
        let joint_a = store.begin_orders_snapshot().unwrap();
        let joint_b = store.begin_orders_snapshot().unwrap();
        assert!(store.commit_orders_snapshot([first], |current| {
            assert!(current.is_empty());
            native_commits.set(native_commits.get() + 1);
            vec![snapshot_order("seed")]
        }));
        assert_eq!(native_commits.get(), 1);
        assert!(store.order("seed").is_some());
        assert!(store.orders_snapshot_current(&sibling));
        assert!(store.commit_orders_snapshot([sibling], |current| {
            assert_eq!(current[0].id, "seed");
            vec![]
        }));
        assert!(store.orders().is_empty());
        assert!(store.loaded().orders);
        store.take_pending();
        assert!(store.commit_orders_snapshot([joint_a, joint_b], |_| vec![]));
        assert!(store.changes.orders);
        let stale = store.begin_orders_snapshot().unwrap();
        store.invalidate_order_snapshots();
        store.set_orders(vec![snapshot_order("newer")]);
        store.take_pending();
        assert!(!store.commit_orders_snapshot([stale], |_| panic!("stale native effect")));
        assert!(store.order("newer").is_some());
        assert!(!store.changes.orders && !store.changes.status);
    }

    fn word(payload: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(payload[at..at + 4].try_into().unwrap())
    }

    fn frame_string(frame: &[u8], reference: u32) -> &str {
        let strings = wire::sections(frame)
            .find(|(kind, _)| *kind == section::STRINGS)
            .unwrap()
            .1;
        let count = word(strings, 0) as usize;
        let start = word(strings, 4 + reference as usize * 4) as usize;
        let end = word(strings, 8 + reference as usize * 4) as usize;
        std::str::from_utf8(&strings[4 + (count + 1) * 4 + start..4 + (count + 1) * 4 + end])
            .unwrap()
    }

    #[test]
    fn settings_follow_symbols_after_reorder_and_retain_delisted_and_missing_markets() {
        let mut store = Store::default();
        store.set_markets(vec![market("A"), market("B")]);
        store.set_settings([
            (
                0,
                MarketSetting {
                    leverage: 5.,
                    ..MarketSetting::default()
                },
            ),
            (
                1,
                MarketSetting {
                    leverage: 9.,
                    ..MarketSetting::default()
                },
            ),
            (
                100,
                MarketSetting {
                    leverage: 100.,
                    ..MarketSetting::default()
                },
            ),
        ]);
        store.update_setting("ORPHAN", |setting| setting.leverage = 11.);
        store.take_pending();
        store.set_markets(vec![market("B"), market("A")]);
        assert_eq!(store.setting("A").unwrap().leverage, 5.);
        assert_eq!(store.setting("B").unwrap().leverage, 9.);
        let pending = store.take_pending();
        assert!(pending.changes.settings);
        let mut writer = FrameWriter::new(Vec::new(), 1.);
        store.write(&pending, &mut writer);
        let frame = writer.finish();
        let settings = wire::sections(&frame)
            .find(|(kind, _)| *kind == section::SETTINGS)
            .unwrap()
            .1;
        assert_eq!(word(settings, 0), 3, "invalid index cannot invent a symbol");
        assert_eq!(
            (word(settings, 4), word(settings, 8), word(settings, 12)),
            (0, 1, wire::NONE)
        );
        // Three rows: 32 bytes of indices/enums, then three f64 columns.
        assert_eq!(frame_string(&frame, word(settings, 104)), "B");
        assert_eq!(frame_string(&frame, word(settings, 108)), "A");
        assert_eq!(frame_string(&frame, word(settings, 112)), "ORPHAN");
        store.set_markets(vec![market("B")]);
        assert_eq!(store.setting("A").unwrap().leverage, 5.);
        assert_eq!(store.setting("ORPHAN").unwrap().leverage, 11.);
        store.set_markets(vec![market("A"), market("B"), market("ORPHAN")]);
        assert_eq!(store.setting("A").unwrap().leverage, 5.);
        assert_eq!(store.setting("ORPHAN").unwrap().leverage, 11.);
    }

    #[test]
    fn pretty_symbols_preserve_native_empty_and_absent_values() {
        let mut store = Store::default();
        let mut native = market("A");
        native.pretty_symbol = Some("Native / 株式".into());
        let mut empty = market("B");
        empty.pretty_symbol = Some(String::new());
        store.set_markets(vec![native, empty, market("C")]);
        let mut writer = FrameWriter::new(Vec::new(), 1.);
        store.write_markets(&mut writer);
        let frame = writer.finish();
        assert_eq!(u16::from_le_bytes(frame[18..20].try_into().unwrap()), 1);
        let rows = wire::sections(&frame).next().unwrap().1;
        // Three rows: 64 aligned bytes of identity/flags and ten f64 columns.
        assert_eq!(frame_string(&frame, word(rows, 304)), "Native / 株式");
        assert_eq!(frame_string(&frame, word(rows, 308)), "");
        assert_eq!(word(rows, 312), wire::NONE);
    }

    #[test]
    fn orphan_positions_retain_both_exact_id_and_symbol() {
        let mut store = market_position_store();
        store.positions[0].symbol = "DELISTED".into();
        store.positions[0].id = "venue-position-7".into();
        let mut writer = FrameWriter::new(Vec::new(), 1.);
        store.write_positions(&mut writer);
        let frame = writer.finish();
        assert_eq!(u16::from_le_bytes(frame[18..20].try_into().unwrap()), 2);
        let rows = wire::sections(&frame).next().unwrap().1;
        assert_eq!(word(rows, 4), wire::NONE);
        assert_eq!(frame_string(&frame, word(rows, 104)), "venue-position-7");
        assert_eq!(frame_string(&frame, word(rows, 108)), "DELISTED");
    }

    #[test]
    fn mid_price_replacement_and_sparse_changes_preserve_absence_and_clear_values() {
        let mut store = Store::default();
        store.set_tickers(vec![Ticker {
            mid_price: Some(100.5),
            ..ticker("ORPHAN")
        }]);
        let mut writer = FrameWriter::new(Vec::new(), 1.);
        store.write_tickers(&mut writer);
        let frame = writer.finish();
        assert_eq!(u16::from_le_bytes(frame[18..20].try_into().unwrap()), 1);
        let rows = wire::sections(&frame).next().unwrap().1;
        assert_eq!(f64::from_le_bytes(rows[96..104].try_into().unwrap()), 100.5);
        assert_eq!(frame_string(&frame, word(rows, 104)), "ORPHAN");
        store.take_pending();
        for value in [Some(101.5), None] {
            store.update_ticker("ORPHAN", |ticker| ticker.mid_price = value);
            let pending = store.take_pending();
            assert_eq!(pending.ticker_deltas, [(0, 1 << 11)]);
            let mut writer = FrameWriter::new(Vec::new(), 2.);
            store.write(&pending, &mut writer);
            let frame = writer.finish();
            assert_eq!(u16::from_le_bytes(frame[18..20].try_into().unwrap()), 1);
            let rows = wire::sections(&frame).next().unwrap().1;
            let encoded = f64::from_le_bytes(rows[16..24].try_into().unwrap());
            assert!(value.map_or(encoded.is_nan(), |value| encoded == value));
        }
        store.update_ticker("ORPHAN", |ticker| ticker.mid_price = None);
        assert!(store.take_pending().ticker_deltas.is_empty());
    }

    fn ticker(symbol: &str) -> Ticker {
        Ticker {
            id: symbol.into(),
            symbol: symbol.into(),
            ..Ticker::default()
        }
    }

    #[test]
    fn live_ticker_after_the_seed_never_creates_a_row_the_seed_omitted() {
        let mut store = Store::default();
        store.set_markets(vec![market("AAA"), market("DELISTED")]);
        store.set_tickers(vec![ticker("AAA")]);
        store.update_ticker("DELISTED", |ticker| ticker.mark = 0.15);
        assert!(store.ticker("DELISTED").is_none());
        store.update_ticker("AAA", |ticker| ticker.mark = 2.0);
        assert_eq!(store.ticker("AAA").unwrap().mark, 2.0);
        // Test markets share the base "LIVE".
        store.want_ticker_for_base("LIVE");
        store.update_ticker("DELISTED", |ticker| ticker.mark = 0.15);
        assert_eq!(store.ticker("DELISTED").unwrap().mark, 0.15);
    }

    #[test]
    fn live_ticker_recovers_a_skipped_seed_row_without_claiming_seed_success() {
        let mut store = Store::default();
        store.set_markets(vec![market("BBB"), market("AAA")]);
        assert!(!store.loaded().tickers);
        store.update_ticker("BBB", |ticker| ticker.last = 12.0);
        assert!(!store.loaded().tickers);
        assert_eq!(store.ticker("BBB").unwrap().last, 12.0);
        assert!(store.changes.tickers_replaced);
        let index = store.ensure_ticker("BBB").unwrap();
        assert_eq!(index, store.ensure_ticker("BBB").unwrap());
        store.update_ticker("AAA", |ticker| ticker.bid = 11.0);
        assert_eq!(store.ticker("AAA").unwrap().bid, 11.0);
        assert_eq!(store.ticker("BBB").unwrap().last, 12.0);
        store.update_ticker("UNKNOWN", |ticker| ticker.last = 100.0);
        assert!(store.ticker("UNKNOWN").is_none());
        assert_eq!(store.tickers().len(), 2);
    }

    #[test]
    fn an_empty_catalogue_is_never_applied() {
        let mut store = Store::default();
        store.set_markets(Vec::new());
        assert!(
            !store.loaded().markets,
            "an empty first answer leaves markets unloaded"
        );
        assert!(store.take_rejected_empty_catalogue());
        assert!(
            !store.take_rejected_empty_catalogue(),
            "each rejection is reported once"
        );
        store.set_markets(vec![market("AAA")]);
        store.set_markets(Vec::new());
        assert_eq!(
            store.markets().len(),
            1,
            "an empty refresh keeps the previous catalogue"
        );
        assert!(store.market("AAA").is_some());
        assert!(store.loaded().markets);
        assert!(store.take_rejected_empty_catalogue());
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
    fn ticker_lookup_replacement_recovery_and_duplicate_deltas_stay_coherent() {
        let mut store = Store::default();
        store.set_markets(vec![market("A"), market("B"), market("C")]);
        for _ in 0..3 {
            let mut winner = ticker("A");
            winner.last = 9.;
            store.set_tickers(vec![ticker("C"), ticker("A"), ticker("B"), winner]);
            store.take_pending();
            store.update_ticker("A", |row| row.last = -0.);
            assert_eq!(store.tickers()[1].last.to_bits(), 0f64.to_bits());
            assert_eq!(store.tickers()[3].last.to_bits(), (-0f64).to_bits());
            assert_eq!(store.take_pending().ticker_deltas, [(3, 1 << 2)]);
            assert_eq!(store.ensure_ticker("A"), Some(3));
            assert_eq!(store.tickers().len(), 4);
            store.set_tickers(vec![ticker("B")]);
            assert!(store.ticker("A").is_none());
            assert!(store.ticker("C").is_none());
            assert_eq!(store.ensure_ticker("A"), Some(1));
            assert_eq!(store.ensure_ticker("A"), Some(1));
            assert_eq!(
                store
                    .tickers()
                    .iter()
                    .map(|r| r.symbol.as_str())
                    .collect::<Vec<_>>(),
                ["B", "A"]
            );
            assert_eq!(store.ensure_ticker("unknown"), None);
            store.set_tickers(Vec::new());
            assert!(store.ticker("B").is_none());
            assert!(store.ticker("A").is_none());
            assert_eq!(store.ensure_ticker("C"), Some(0));
            assert_eq!(store.ticker("C").unwrap().symbol, "C");
        }
    }

    #[test]
    fn replacing_a_large_ticker_table_releases_obsolete_index_capacity() {
        let mut store = Store::default();
        for _ in 0..3 {
            store.set_tickers((0..4096).map(|i| ticker(&format!("ROW{i}"))).collect());
            let large = store.ticker_by_symbol.capacity();
            assert!(store.ticker("ROW4095").is_some());
            store.set_tickers(vec![ticker("small")]);
            assert!(store.ticker("ROW4095").is_none());
            assert!(store.ticker_by_symbol.capacity() < large);
            store.set_tickers(Vec::new());
            assert!(store.ticker("small").is_none());
            assert_eq!(store.ticker_by_symbol.capacity(), 0);
        }
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
    fn indexed_observed_updates_preserve_reference_times_and_exact_delta_frames() {
        for export_observations in [false, true] {
            let mut separate = Store::default();
            let mut combined = Store::default();
            for store in [&mut separate, &mut combined] {
                store.export_ticker_observations = export_observations;
                store.set_tickers(vec![ticker("B"), ticker("A"), ticker("A")]);
                store.take_pending();
            }
            // Includes an unchanged observation, a changed value, signed zero,
            // omitted references, and a message with no reference observation.
            for (now, observed, last, mark) in [
                (1., 1 << 2, 0., 0.),
                (2., 1 << 3, 0., 5.),
                (3., (1 << 2) | (1 << 4), -0., 5.),
                (4., 0, -0., 5.),
                (5., 1, -0., 5.),
            ] {
                separate.set_clock(now);
                combined.set_clock(now);
                let update = |row: &mut Ticker| {
                    row.last = last;
                    row.mark = mark;
                };
                separate.update_ticker("A", update);
                separate.observe_ticker_fields("A", observed);
                combined.update_ticker_at_observed(2, observed, update);
                assert_eq!(combined.ticker_seen_at, separate.ticker_seen_at);
                assert_eq!(combined.ticker_observed_at, separate.ticker_observed_at);
                assert_eq!(combined.ticker_revision, separate.ticker_revision);
                assert_eq!(combined.tickers()[1].last.to_bits(), 0f64.to_bits());
                let separate_pending = separate.take_pending();
                let combined_pending = combined.take_pending();
                assert_eq!(
                    combined_pending.ticker_deltas,
                    separate_pending.ticker_deltas
                );
                let mut separate_frame = FrameWriter::new(vec![], now);
                let mut combined_frame = FrameWriter::new(vec![], now);
                separate.write(&separate_pending, &mut separate_frame);
                combined.write(&combined_pending, &mut combined_frame);
                assert_eq!(combined_frame.finish(), separate_frame.finish());
            }
            let times = combined.ticker_observed_at.clone();
            combined.update_ticker_at_observed(u32::MAX, u16::MAX, |_| {
                panic!("invalid ticker index reached closure")
            });
            assert_eq!(combined.ticker_observed_at, times);
            assert!(combined.take_pending().ticker_deltas.is_empty());
        }
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
    fn scalar_settings_retain_the_original_scalar_offset_with_fixed_revision_one_tails() {
        let mut store = Store::default();
        store.set_markets(vec![market("A")]);
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
        assert_eq!(u16::from_le_bytes(frame[18..20].try_into().unwrap()), 1);
        assert_eq!(payload.len(), 48);
        assert_eq!(
            f64::from_le_bytes(payload[16..24].try_into().unwrap()),
            15.0
        );
        for offset in [24, 32] {
            assert_eq!(
                f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap()),
                15.0
            );
        }
        assert_eq!(frame_string(&frame, word(payload, 40)), "A");
    }

    #[test]
    fn settings_append_independent_leverages_after_the_original_scalar_column() {
        let mut store = Store::default();
        store.set_markets(vec![market("A"), market("B")]);
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
    fn market_flags_carry_the_venue_quantity_units() {
        let mut store = Store::default();
        store.set_quantity_units(crate::QuantityUnits::ContractCount);
        store.set_markets(vec![market("A"), market("B")]);
        let frame = balance_frame(&mut store);
        let (_, markets) = wire::sections(&frame)
            .find(|(kind, _)| *kind == section::MARKETS)
            .unwrap();
        let flags = &markets[4 + 2 * 16..4 + 2 * 16 + 8];
        for flag in flags.chunks(4) {
            let flag = u32::from_le_bytes(flag.try_into().unwrap());
            assert_eq!(flag >> market_flag::UNITS_SHIFT & 0xf, 3);
            assert_eq!(flag & market_flag::ACTIVE, market_flag::ACTIVE);
        }
    }

    #[test]
    fn first_validated_zero_balance_is_published_once_with_loaded_status() {
        let mut store = Store::default();
        assert!(!store.loaded().balance);
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
        );
        store.set_balance(Balance::default());
        assert!(store.loaded().balance);
        let frame = balance_frame(&mut store);
        let (_, payload) = crate::wire::sections(&frame)
            .find(|(kind, _)| *kind == section::BALANCE)
            .expect("the first validated balance needs numeric evidence even when zero");
        assert_eq!(&payload[..32], &[0_u8; 32]);
        assert!(crate::wire::sections(&frame).any(|(kind, _)| kind == section::STATUS));
        store.set_balance(Balance::default());
        assert!(
            crate::wire::sections(&balance_frame(&mut store))
                .next()
                .is_none()
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
    fn underlying_tokens_ride_their_own_section_and_reset_with_the_market_table() {
        let mut store = market_position_store();
        let legacy = balance_frame(&mut store);
        let token = crate::model::UnderlyingToken {
            market: 0,
            name: "UBTC".into(),
            full_name: None,
        };
        store.set_underlying_tokens(vec![token.clone()]);
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
        let words = payload(&frame, section::UNDERLYING_TOKENS);
        let word = |at| u32::from_le_bytes(words[at..at + 4].try_into().unwrap());
        assert_eq!((word(0), word(4)), (1, 0));
        assert_ne!(word(8), wire::NONE);
        assert_eq!(word(12), wire::NONE, "an absent full name stays absent");
        store.set_underlying_tokens(vec![token]);
        assert!(!store.take_pending().changes.markets);
        store.set_markets(store.markets.clone());
        assert!(store.underlying_tokens().is_none());
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
            // Revision two appends exact ID and symbol after numeric columns.
            assert_eq!(positions.len(), 112);
            assert_eq!(
                frame_string(&frame, word(positions, 104)),
                store.positions()[0].id,
            );
            assert_eq!(
                frame_string(&frame, word(positions, 108)),
                store.positions()[0].symbol,
            );
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
            let numeric_end = if kind == section::POSITIONS {
                88
            } else {
                old.len()
            };
            assert_eq!(
                &old[..numeric_end],
                &new[..numeric_end],
                "prediction metadata cannot shift numeric columns"
            );
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
