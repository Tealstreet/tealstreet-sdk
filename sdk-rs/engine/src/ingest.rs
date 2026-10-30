//! Frame ingest for frame-fed engines (`crate::frame_fed`): a data source's
//! SCXF frames (`crate::wire`) applied to this engine's Store, so its writes
//! read the same orders, positions and settings the source holds.
//!
//! A snapshot frame resets the account sections first; deltas apply in order.
//! The host owns sequencing: on a gap it asks the source for a re-snapshot and
//! applies nothing until it arrives. Market indexes in a frame refer to the
//! SOURCE's market table, kept here from its MARKETS sections; rows map to
//! symbols through it, never to this engine's own indexes.
//!
//! Each section has its own decoder below; one not yet implemented is skipped.
#[cfg(any(test, feature = "source-leverage"))]
use crate::model::MinMax;
use crate::model::{Balance, CoinBalance, CurrencyBalance, MarginType, Ticker};
use crate::store::{MarketSetting, Store};
use crate::wire::{FORMAT, MAGIC, NONE, TICKER_COLUMNS, section};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestError(pub &'static str);

/// One `(kind, revision, payload)` section of a frame.
#[derive(Clone, Copy, Debug)]
pub struct Section<'a> {
    pub kind: u16,
    pub revision: u16,
    pub payload: &'a [u8],
}

/// The frame's string table (`section::STRINGS`).
#[derive(Clone, Copy, Default)]
pub struct Strings<'a> {
    payload: &'a [u8],
    count: usize,
}

impl<'a> Strings<'a> {
    fn new(payload: &'a [u8]) -> Result<Self, IngestError> {
        let count = Cursor::new(payload).u32()? as usize;
        if 4 + 4 * (count + 1) > payload.len() {
            return Err(IngestError("string table is truncated"));
        }
        Ok(Self { payload, count })
    }

    /// A string reference: `None` for `wire::NONE`.
    pub fn get(&self, index: u32) -> Result<Option<&'a str>, IngestError> {
        if index == NONE {
            return Ok(None);
        }
        let index = index as usize;
        if index >= self.count {
            return Err(IngestError("string reference out of range"));
        }
        let mut offsets = Cursor::new(self.payload);
        offsets.at = 4 + 4 * index;
        let (start, end) = (offsets.u32()? as usize, offsets.u32()? as usize);
        let bytes = 4 + 4 * (self.count + 1);
        let text = self
            .payload
            .get(bytes + start..bytes + end)
            .ok_or(IngestError("string bytes out of range"))?;
        std::str::from_utf8(text)
            .map(Some)
            .map_err(|_| IngestError("string is not UTF-8"))
    }

    /// A string reference that must be present.
    pub fn required(&self, index: u32) -> Result<&'a str, IngestError> {
        self.get(index)?
            .ok_or(IngestError("a required string is absent"))
    }
}

/// A bounds-checked little-endian reader over one section's payload.
pub struct Cursor<'a> {
    payload: &'a [u8],
    pub at: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(payload: &'a [u8]) -> Self {
        Self { payload, at: 0 }
    }

    pub fn u32(&mut self) -> Result<u32, IngestError> {
        let bytes = self
            .payload
            .get(self.at..self.at + 4)
            .ok_or(IngestError("section is truncated"))?;
        self.at += 4;
        Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
    }

    pub fn f64(&mut self) -> Result<f64, IngestError> {
        let bytes = self
            .payload
            .get(self.at..self.at + 8)
            .ok_or(IngestError("section is truncated"))?;
        self.at += 8;
        Ok(f64::from_le_bytes(bytes.try_into().expect("eight bytes")))
    }

    /// Skip to the next 8-byte boundary, as `FrameWriter::pad` wrote it
    /// (payloads start 8-aligned in the frame).
    pub fn align8(&mut self) {
        self.at = self.at.div_ceil(8) * 8;
    }

    pub fn bytes(&mut self, length: usize) -> Result<&'a [u8], IngestError> {
        let bytes = self
            .payload
            .get(self.at..self.at + length)
            .ok_or(IngestError("section is truncated"))?;
        self.at += length;
        Ok(bytes)
    }
}

/// A parsed frame: its sections in order and its string table.
pub struct Frame<'a> {
    pub sections: Vec<Section<'a>>,
    pub strings: Strings<'a>,
}

pub fn parse(frame: &[u8]) -> Result<Frame<'_>, IngestError> {
    if frame.len() < 16 || frame[..4] != MAGIC || u16::from_le_bytes([frame[4], frame[5]]) != FORMAT
    {
        return Err(IngestError("not an SCXF frame of this format"));
    }
    let count = u16::from_le_bytes([frame[6], frame[7]]);
    let mut sections = Vec::with_capacity(count as usize);
    let mut strings = Strings::default();
    let mut at = 16;
    for _ in 0..count {
        let header = frame
            .get(at..at + 8)
            .ok_or(IngestError("section header is truncated"))?;
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let revision = u16::from_le_bytes([header[2], header[3]]);
        let length = u32::from_le_bytes(header[4..8].try_into().expect("four bytes")) as usize;
        let payload = frame
            .get(at + 8..at + 8 + length)
            .ok_or(IngestError("section payload is truncated"))?;
        if kind == section::STRINGS {
            strings = Strings::new(payload)?;
        } else {
            sections.push(Section {
                kind,
                revision,
                payload,
            });
        }
        at += 8 + length;
    }
    Ok(Frame { sections, strings })
}

/// What persists between ingested frames: the source's market table.
#[derive(Default)]
pub struct IngestState {
    /// The source's market symbols, by its market index.
    pub source_markets: Vec<String>,
    /// The source's native market ids, by its market index.
    pub source_market_ids: Vec<String>,
    /// The source's ticker symbols, by its ticker row (`TICKER_DELTAS` indexes these).
    pub source_tickers: Vec<String>,
    /// The source's leverage limits, by its market index: private metadata
    /// (Binance leverage brackets) this engine's public instrument read lacks.
    #[cfg(any(test, feature = "source-leverage"))]
    pub source_leverage: Vec<MinMax>,
    /// Source leverage not yet applied to this engine's own markets.
    #[cfg(any(test, feature = "source-leverage"))]
    leverage_pending: bool,
}

impl IngestState {
    pub fn symbol(&self, market: u32) -> Result<&str, IngestError> {
        self.source_markets
            .get(market as usize)
            .map(String::as_str)
            .ok_or(IngestError("market index outside the source's table"))
    }
}

/// What an applied frame changed, for the engine's follow-up work.
#[derive(Default, Debug, PartialEq)]
pub struct Applied {
    /// An ORDERS section was applied: the source's orders stand as reconciled.
    pub orders: bool,
    pub positions: bool,
    /// A STATUS section (loaded bits and the hedge flag) was applied.
    pub status: bool,
    /// A SETTINGS section, the source's whole settings table, was applied.
    pub settings: bool,
    /// The source's VENUE_STATE payload, for `Venue::apply_venue_state`.
    pub venue_state: Option<Vec<u8>>,
}

/// Apply one parsed frame. `snapshot` resets the account sections first.
pub fn apply(
    store: &mut Store,
    state: &mut IngestState,
    frame: &Frame<'_>,
    snapshot: bool,
) -> Result<Applied, IngestError> {
    if snapshot {
        store.reset_account();
    }
    let mut applied = Applied::default();
    // MARKETS first: every other section indexes the source's table.
    for section in frame.sections.iter().filter(|s| s.kind == section::MARKETS) {
        markets(state, frame, section)?;
    }
    #[cfg(any(test, feature = "source-leverage"))]
    if state.leverage_pending {
        apply_leverage(store, state);
    }
    for section in &frame.sections {
        match section.kind {
            section::MARKETS => {}
            section::TICKERS => tickers(store, &mut *state, frame, section)?,
            section::TICKER_DELTAS => ticker_deltas(store, state, frame, section)?,
            section::ORDERS => {
                orders(store, state, frame, section)?;
                applied.orders = true;
            }
            section::POSITIONS => {
                positions(store, state, frame, section)?;
                applied.positions = true;
            }
            section::BALANCE => balance(store, state, frame, section)?,
            section::SETTINGS => {
                settings(store, state, frame, section)?;
                applied.settings = true;
            }
            section::STATUS => {
                status(store, state, frame, section)?;
                applied.status = true;
            }
            section::VENUE_STATE => {
                applied.venue_state = Some(venue_state(section)?.to_vec());
            }
            // Books, candles, fills and the rest are the data source's display
            // data: the UI takes them from the source, a writer never reads them.
            _ => {}
        }
    }
    Ok(applied)
}

/// `VENUE_STATE`: a u32 length, then the venue's own encoding.
fn venue_state<'a>(section: &Section<'a>) -> Result<&'a [u8], IngestError> {
    let mut cursor = Cursor::new(section.payload);
    let length = cursor.u32()? as usize;
    cursor.bytes(length)
}

// Section decoders. Owners: MARKETS, TICKERS (+deltas), STATUS m26ps4;
// BALANCE (+coins), SETTINGS tbz5hr; ORDERS, POSITIONS, VENUE_STATE fe7lgm.
// Each mirrors its writer in `Store::write` and is round-trip tested there.

/// `MARKETS` (`Store::write_markets`): the source's table, which later sections
/// index. This engine keeps its own market rows from its public instrument read.
fn markets(
    state: &mut IngestState,
    frame: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    if section.revision > 1 {
        return Err(IngestError("unknown MARKETS revision"));
    }
    let mut cursor = Cursor::new(section.payload);
    let count = cursor.u32()? as usize;
    let (mut symbols, mut ids) = (Vec::with_capacity(count), Vec::with_capacity(count));
    for _ in 0..count {
        let [id, symbol, _base, _quote] =
            [cursor.u32()?, cursor.u32()?, cursor.u32()?, cursor.u32()?];
        ids.push(frame.strings.required(id)?.to_owned());
        symbols.push(frame.strings.required(symbol)?.to_owned());
    }
    // Flags, then ten f64 columns (and revision 1's pretty-symbol refs).
    cursor.bytes(4 * count)?;
    cursor.align8();
    let columns = cursor.at;
    cursor.bytes(8 * 10 * count)?;
    if section.revision >= 1 {
        cursor.bytes(4 * count)?;
    }
    state.source_markets = symbols;
    state.source_market_ids = ids;
    #[cfg(any(test, feature = "source-leverage"))]
    source_leverage(state, section.payload, columns, count)?;
    #[cfg(not(any(test, feature = "source-leverage")))]
    let _ = columns;
    Ok(())
}

/// Keep the source's leverage limits (MARKETS columns four and five).
#[cfg(any(test, feature = "source-leverage"))]
fn source_leverage(
    state: &mut IngestState,
    payload: &[u8],
    columns: usize,
    count: usize,
) -> Result<(), IngestError> {
    let mut cursor = Cursor::new(payload);
    cursor.at = columns + 8 * 4 * count;
    let mut column = || {
        (0..count)
            .map(|_| cursor.f64())
            .collect::<Result<Vec<_>, _>>()
    };
    let (min, max) = (column()?, column()?);
    state.source_leverage = min
        .into_iter()
        .zip(max)
        .map(|(min, max)| MinMax { min, max })
        .collect();
    state.leverage_pending = true;
    Ok(())
}

/// Give this engine's markets the source's leverage limits, once its own
/// instrument read has loaded them; until then each frame tries again.
#[cfg(any(test, feature = "source-leverage"))]
fn apply_leverage(store: &mut Store, state: &mut IngestState) {
    if !store.loaded().markets {
        return;
    }
    let limits: std::collections::HashMap<&str, MinMax> = state
        .source_markets
        .iter()
        .map(String::as_str)
        .zip(state.source_leverage.iter().copied())
        .collect();
    let changed = store.markets().iter().any(|market| {
        limits
            .get(market.symbol.as_str())
            .is_some_and(|limit| *limit != market.limits.leverage)
    });
    if changed {
        store.update_markets(|market| {
            if let Some(limit) = limits.get(market.symbol.as_str()) {
                market.limits.leverage = *limit;
            }
        });
    }
    state.leverage_pending = false;
}

/// A ticker's numeric fields in `store::TICKER_FIELDS` order; midpoint `NaN` is absent.
fn set_ticker_field(ticker: &mut Ticker, field: usize, value: f64) {
    match field {
        0 => ticker.bid = value,
        1 => ticker.ask = value,
        2 => ticker.last = value,
        3 => ticker.mark = value,
        4 => ticker.index = value,
        5 => ticker.percentage = value,
        6 => ticker.open_interest = value,
        7 => ticker.funding_rate = value,
        8 => ticker.funding_time = value,
        9 => ticker.volume = value,
        10 => ticker.quote_volume = value,
        _ => ticker.mid_price = (!value.is_nan()).then_some(value),
    }
}

/// `TICKERS` (`Store::write_tickers`): the source's full ticker table. Rows name
/// a source market, or carry their own symbol and id in the identity tail.
fn tickers(
    store: &mut Store,
    state: &mut IngestState,
    frame: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    if section.revision > 1 {
        return Err(IngestError("unknown TICKERS revision"));
    }
    let mut cursor = Cursor::new(section.payload);
    let count = cursor.u32()? as usize;
    let markets = (0..count)
        .map(|_| cursor.u32())
        .collect::<Result<Vec<_>, _>>()?;
    cursor.align8();
    let columns = if section.revision >= 1 {
        TICKER_COLUMNS
    } else {
        11
    };
    let mut rows = vec![Ticker::default(); count];
    for column in 0..columns {
        for row in &mut rows {
            set_ticker_field(row, column, cursor.f64()?);
        }
    }
    // The identity tail is present exactly when its 8 bytes per row follow.
    let tail = count > 0 && section.payload.len() - cursor.at >= 8 * count;
    if tail {
        for row in &mut rows {
            row.symbol = frame.strings.required(cursor.u32()?)?.to_owned();
        }
        for row in &mut rows {
            row.id = frame.strings.required(cursor.u32()?)?.to_owned();
        }
    } else {
        for (row, &market) in rows.iter_mut().zip(&markets) {
            row.symbol = state.symbol(market)?.to_owned();
            row.id = state
                .source_market_ids
                .get(market as usize)
                .ok_or(IngestError("market index outside the source's table"))?
                .clone();
        }
    }
    state.source_tickers = rows.iter().map(|row| row.symbol.clone()).collect();
    store.set_tickers(rows);
    Ok(())
}

/// `TICKER_DELTAS`: per changed ticker, its source row, a field mask and the
/// masked values in field order.
fn ticker_deltas(
    store: &mut Store,
    state: &IngestState,
    _: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    let mut cursor = Cursor::new(section.payload);
    while cursor.at < section.payload.len() {
        let (row, mask) = (cursor.f64()?, cursor.f64()?);
        let whole = |value: f64, limit: f64| value.fract() == 0.0 && (0.0..limit).contains(&value);
        if !whole(row, u32::MAX as f64) || !whole(mask, (1u32 << TICKER_COLUMNS) as f64) {
            return Err(IngestError("ticker delta row or mask is malformed"));
        }
        let (row, mask) = (row as usize, mask as u16);
        let mut values = [None; TICKER_COLUMNS];
        for (field, value) in values.iter_mut().enumerate() {
            if mask & (1 << field) != 0 {
                *value = Some(cursor.f64()?);
            }
        }
        // By row, not symbol: a duplicate symbol must not redirect an earlier row.
        let symbol = state
            .source_tickers
            .get(row)
            .ok_or(IngestError("ticker row outside the source's table"))?;
        if store.tickers().get(row).map(|ticker| &ticker.symbol) != Some(symbol) {
            return Err(IngestError(
                "ticker row no longer matches the source's table",
            ));
        }
        store.update_ticker_at(row as u32, |ticker| {
            for (field, value) in values.iter().enumerate() {
                if let Some(value) = value {
                    set_ticker_field(ticker, field, *value);
                }
            }
        });
    }
    Ok(())
}

/// The `index`th variant of a fieldless enum in declaration order, as
/// `variant as u32` wrote it; `None` for an out-of-range wire value.
fn nth<T: Copy>(variants: &[T], index: u32) -> Result<T, IngestError> {
    variants
        .get(index as usize)
        .copied()
        .ok_or(IngestError("enum value out of range"))
}

/// `nth` for an optional enum written as `variant + 1`, zero when absent.
fn optional<T: Copy>(variants: &[T], value: u32) -> Result<Option<T>, IngestError> {
    match value {
        0 => Ok(None),
        value => nth(variants, value - 1).map(Some),
    }
}

/// A column of `count` f64s, `None` where the writer stored NaN for absent.
fn present(value: f64) -> Option<f64> {
    (!value.is_nan()).then_some(value)
}

/// `ORDERS` (revision 1, `Store::write_orders`): replaces the order list.
fn orders(
    store: &mut Store,
    _: &IngestState,
    frame: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    use crate::model::{
        Order, OrderExtra, OrderSide, OrderStatus, OrderType, RejectReason, StopTriggerType,
        TimeInForce,
    };
    use OrderStatus as S;
    use OrderType as K;
    const STATUS: [S; 6] = [
        S::Open,
        S::Closed,
        S::Canceled,
        S::Creating,
        S::Updating,
        S::Canceling,
    ];
    const KIND: [K; 6] = [
        K::Market,
        K::Limit,
        K::Conditional,
        K::StopLoss,
        K::TakeProfit,
        K::TrailingStopLoss,
    ];
    const SIDE: [OrderSide; 2] = [OrderSide::Buy, OrderSide::Sell];
    const TIF: [TimeInForce; 4] = [
        TimeInForce::GoodTillCancel,
        TimeInForce::ImmediateOrCancel,
        TimeInForce::FillOrKill,
        TimeInForce::PostOnly,
    ];
    const TRIGGER: [StopTriggerType; 3] = [
        StopTriggerType::MarkPrice,
        StopTriggerType::LastPrice,
        StopTriggerType::IndexPrice,
    ];
    const REJECT: [RejectReason; 3] = [
        RejectReason::PostOnly,
        RejectReason::InsufficientBalance,
        RejectReason::ReduceOnly,
    ];
    if section.revision != 1 {
        return Err(IngestError("unsupported ORDERS revision"));
    }
    let strings = &frame.strings;
    let mut c = Cursor::new(section.payload);
    let count = c.u32()? as usize;
    let mut refs = Vec::with_capacity(count);
    for _ in 0..count {
        refs.push((c.u32()?, c.u32()?, c.u32()?));
    }
    for _ in 0..count {
        c.u32()?; // market index: rows carry their own symbol string
    }
    let mut enums = Vec::with_capacity(count);
    for _ in 0..count {
        enums.push(c.u32()?);
    }
    c.align8();
    let mut columns = [(); 11].map(|_| Vec::with_capacity(count));
    for column in &mut columns {
        for _ in 0..count {
            column.push(c.f64()?);
        }
    }
    let mut extra_triggers = Vec::with_capacity(count);
    for _ in 0..count {
        extra_triggers.push(c.u32()?);
    }
    let mut orders = Vec::with_capacity(count);
    for i in 0..count {
        let symbol = strings.required(c.u32()?)?.to_owned();
        let (id, client_order_id, parent_id) = refs[i];
        let packed = enums[i];
        let flags = (packed >> 16) & 0xff;
        let tif_trigger = packed >> 24;
        let column = |n: usize| columns[n][i];
        let tp_sl = extra_triggers[i];
        let extra = OrderExtra {
            take_profit_known: false,
            stop_loss_known: false,
            tp_trigger_type: optional(&TRIGGER, tp_sl & 0xff)?,
            sl_trigger_type: optional(&TRIGGER, (tp_sl >> 8) & 0xff)?,
            remaining_native: present(column(7)),
            filled_in_placement_units: present(column(8)),
            remaining_notional: present(column(9)),
            conditional_order_price: present(column(10)),
            reject_reason: optional(&REJECT, flags >> 4)?,
        };
        orders.push(Order {
            id: strings.required(id)?.to_owned(),
            parent_id: strings.get(parent_id)?.map(str::to_owned),
            status: nth(&STATUS, packed & 0xff)?,
            symbol,
            kind: nth(&KIND, (packed >> 8) & 0xff)?,
            side: nth(&SIDE, flags & 1)?,
            price: column(0),
            amount: column(1),
            filled: column(2),
            remaining: column(3),
            reduce_only: flags & 2 != 0,
            close: (flags & 8 != 0).then_some(flags & 4 != 0),
            client_order_id: strings.get(client_order_id)?.map(str::to_owned),
            time_in_force: optional(&TIF, tif_trigger & 0xf)?,
            last_update: column(4),
            trigger_type: optional(&TRIGGER, tif_trigger >> 4)?,
            take_profit: present(column(5)),
            stop_loss: present(column(6)),
            extra: (extra != OrderExtra::default()).then_some(extra),
        });
    }
    store.set_orders(orders);
    Ok(())
}

/// `POSITIONS` (revision 1, `Store::write_positions`): replaces the positions.
fn positions(
    store: &mut Store,
    _: &IngestState,
    frame: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    use crate::model::{MarginType, Position, PositionMode, PositionSide};
    const SIDE: [PositionSide; 2] = [PositionSide::Long, PositionSide::Short];
    const MARGIN: [MarginType; 2] = [MarginType::Isolated, MarginType::Cross];
    const MODE: [PositionMode; 2] = [PositionMode::Hedged, PositionMode::Oneway];
    if section.revision != 1 {
        return Err(IngestError("unsupported POSITIONS revision"));
    }
    let strings = &frame.strings;
    let mut c = Cursor::new(section.payload);
    let count = c.u32()? as usize;
    for _ in 0..count {
        c.u32()?; // market index: rows carry their own symbol string
    }
    let mut enums = Vec::with_capacity(count);
    for _ in 0..count {
        enums.push(c.u32()?);
    }
    c.align8();
    let mut columns = [(); 9].map(|_| Vec::with_capacity(count));
    for column in &mut columns {
        for _ in 0..count {
            column.push(c.f64()?);
        }
    }
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        ids.push(strings.required(c.u32()?)?.to_owned());
    }
    let mut positions = Vec::with_capacity(count);
    for (i, id) in ids.into_iter().enumerate() {
        let packed = enums[i];
        let column = |n: usize| columns[n][i];
        positions.push(Position {
            id,
            symbol: strings.required(c.u32()?)?.to_owned(),
            side: nth(&SIDE, packed & 0xff)?,
            entry_price: column(0),
            notional: column(1),
            leverage: column(2),
            margin_type: optional(&MARGIN, (packed >> 8) & 0xff)?,
            position_mode: optional(&MODE, (packed >> 16) & 0xff)?,
            unrealized_pnl: column(3),
            realized_pnl: present(column(4)),
            contracts: column(5),
            liquidation_price: column(6),
            margin_size: present(column(7)),
            last_update: present(column(8)),
        });
    }
    store.set_positions(positions);
    Ok(())
}

/// The aggregate, then the collateral tail, then the per-currency tail (whose
/// flag bit 0 says the source has no collateral). An absent tail keeps the
/// store's own, as the writer omits a tail the source never supplied.
fn balance(
    store: &mut Store,
    _: &IngestState,
    frame: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    let mut cursor = Cursor::new(section.payload);
    let aggregate = |cursor: &mut Cursor<'_>| -> Result<Balance, IngestError> {
        Ok(Balance {
            used: cursor.f64()?,
            free: cursor.f64()?,
            total: cursor.f64()?,
            upnl: cursor.f64()?,
        })
    };
    let balance = aggregate(&mut cursor)?;
    store.set_balance(balance);
    if cursor.at >= section.payload.len() {
        return Ok(());
    }
    let count = cursor.u32()? as usize;
    let bases = (0..count)
        .map(|_| frame.strings.required(cursor.u32()?))
        .collect::<Result<Vec<_>, _>>()?;
    cursor.align8();
    let amounts = (0..count)
        .map(|_| cursor.f64())
        .collect::<Result<Vec<_>, _>>()?;
    let notionals = (0..count)
        .map(|_| cursor.f64())
        .collect::<Result<Vec<_>, _>>()?;
    let coins = bases
        .into_iter()
        .zip(amounts.into_iter().zip(notionals))
        .map(|(base, (amount, notional))| CoinBalance {
            base: base.to_owned(),
            amount,
            notional,
        })
        .collect();
    if cursor.at >= section.payload.len() {
        store.set_coin_balances(coins);
        return Ok(());
    }
    let count = cursor.u32()? as usize;
    let flags = cursor.u32()?;
    let names = (0..count)
        .map(|_| frame.strings.required(cursor.u32()?))
        .collect::<Result<Vec<_>, _>>()?;
    cursor.align8();
    let mut columns = [const { Vec::new() }; 4];
    for column in &mut columns {
        *column = (0..count)
            .map(|_| cursor.f64())
            .collect::<Result<Vec<_>, _>>()?;
    }
    let [totals, free, used, upnl] = columns;
    let currencies = (0..count)
        .map(|i| CurrencyBalance {
            currency: names[i].to_owned(),
            balance: Balance {
                total: totals[i],
                free: free[i],
                used: used[i],
                upnl: upnl[i],
            },
        })
        .collect();
    if flags & 1 == 0 {
        store.set_coin_balances(coins);
    } else {
        store.clear_coin_balances();
    }
    store.set_currency_balances(currencies);
    Ok(())
}

/// Rows of market ref and margin type, then scalar, buy and sell leverage
/// columns, then (revision one) symbol refs. Every row keys by symbol, so a
/// setting for a market the source has not listed survives. A side leverage
/// equal to the scalar was written for an unset side and decodes as unset.
fn settings(
    store: &mut Store,
    state: &IngestState,
    frame: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    if section.revision > 1 {
        return Err(IngestError("unknown SETTINGS revision"));
    }
    let mut cursor = Cursor::new(section.payload);
    let count = cursor.u32()? as usize;
    let markets = (0..count)
        .map(|_| cursor.u32())
        .collect::<Result<Vec<_>, _>>()?;
    let margins = (0..count)
        .map(|_| cursor.u32())
        .collect::<Result<Vec<_>, _>>()?;
    cursor.align8();
    let mut column = || {
        (0..count)
            .map(|_| cursor.f64())
            .collect::<Result<Vec<_>, _>>()
    };
    let (leverage, buy, sell) = (column()?, column()?, column()?);
    let symbols = if section.revision >= 1 {
        (0..count)
            .map(|_| frame.strings.required(cursor.u32()?))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        markets
            .iter()
            .map(|market| state.symbol(*market))
            .collect::<Result<Vec<_>, _>>()?
    };
    let side = |value: f64, scalar: f64| (value.to_bits() != scalar.to_bits()).then_some(value);
    store.replace_settings((0..count).map(|i| {
        (
            symbols[i],
            MarketSetting {
                leverage: leverage[i],
                margin_type: match margins[i] {
                    1 => Some(MarginType::Isolated),
                    2 => Some(MarginType::Cross),
                    _ => None,
                },
                buy_leverage: side(buy[i], leverage[i]),
                sell_leverage: side(sell[i], leverage[i]),
            },
        )
    }));
    Ok(())
}

/// `STATUS`: the source's loaded bits, hedged flag and latency. This engine's
/// own instrument read owns its markets bit; the source's account bits rule.
fn status(
    store: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    section: &Section<'_>,
) -> Result<(), IngestError> {
    use crate::wire::{loaded_flag, status_flag};
    let mut cursor = Cursor::new(section.payload);
    let (loaded, flags, latency) = (cursor.u32()?, cursor.u32()?, cursor.f64()?);
    let has = |flag: u32| loaded & flag != 0;
    store.set_loaded(|bits| {
        bits.balance = has(loaded_flag::BALANCE);
        bits.orders = has(loaded_flag::ORDERS);
        bits.tickers = has(loaded_flag::TICKERS);
        bits.positions = has(loaded_flag::POSITIONS);
    });
    store.set_hedged(flags & status_flag::HEDGED != 0);
    store.set_latency(latency);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::wire::FrameWriter;

    /// Encode `source`'s full account snapshot with the real writer and ingest
    /// it into a fresh store: the round trip every section decoder is tested by.
    pub(crate) fn round_trip(source: &Store) -> (Store, Applied) {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        source.write(&source.account_snapshot(), &mut writer);
        let frame = writer.finish();
        let mut store = Store::default();
        let mut state = IngestState::default();
        let applied = apply(&mut store, &mut state, &parse(&frame).unwrap(), true).unwrap();
        (store, applied)
    }

    #[test]
    fn a_frame_parses_into_sections_and_its_string_table() {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::VENUE_STATE);
        writer.u32(3);
        let mut payload = b"abc".to_vec();
        payload.resize(4, 0);
        writer.u32(u32::from_le_bytes(payload.try_into().unwrap()));
        writer.end();
        let reference = writer.string("BTCUSDT");
        let frame = writer.finish();
        let parsed = parse(&frame).unwrap();
        assert_eq!(parsed.strings.required(reference).unwrap(), "BTCUSDT");
        assert_eq!(parsed.strings.get(NONE).unwrap(), None);
        let mut store = Store::default();
        let applied = apply(&mut store, &mut IngestState::default(), &parsed, false).unwrap();
        assert_eq!(applied.venue_state.as_deref(), Some(&b"abc"[..]));
        assert!(parse(&frame[..12]).is_err());
    }

    use crate::model::{Balance, CoinBalance, CurrencyBalance, MarginType};
    use crate::store::MarketSetting;

    fn sections_of(store: &Store, kind: u16) -> Vec<Vec<u8>> {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        store.write(&store.account_snapshot(), &mut writer);
        let frame = writer.finish();
        crate::wire::sections(&frame)
            .filter(|(k, _)| *k == kind)
            .map(|(_, payload)| payload.to_vec())
            .collect()
    }

    fn funded() -> Store {
        let mut store = Store::default();
        store.set_balance(Balance {
            used: 1.5,
            free: 8.5,
            total: 10.,
            upnl: -0.25,
        });
        store
    }

    #[test]
    fn balance_alone_round_trips() {
        let source = funded();
        let (store, _) = round_trip(&source);
        assert_eq!(store.balance(), source.balance());
        assert!(store.loaded().balance);
        assert_eq!(store.coin_balances(), None);
        assert_eq!(store.currency_balances(), None);
        assert_eq!(
            sections_of(&store, section::BALANCE),
            sections_of(&source, section::BALANCE)
        );
    }

    #[test]
    fn balance_round_trips_with_collateral_and_currency_tails() {
        let mut source = funded();
        source.set_coin_balances(vec![
            CoinBalance {
                base: "BTC".into(),
                amount: 0.5,
                notional: 50_000.,
            },
            CoinBalance {
                base: "USDT".into(),
                amount: 120.,
                notional: 120.,
            },
        ]);
        let (store, _) = round_trip(&source);
        assert_eq!(store.coin_balances(), source.coin_balances());
        assert_eq!(store.currency_balances(), None);
        source.set_currency_balances(vec![CurrencyBalance {
            currency: "ETH".into(),
            balance: Balance {
                total: 2.,
                free: 1.25,
                used: 0.75,
                upnl: 0.,
            },
        }]);
        let (store, _) = round_trip(&source);
        assert_eq!(store.coin_balances(), source.coin_balances());
        assert_eq!(store.currency_balances(), source.currency_balances());
        assert_eq!(
            sections_of(&store, section::BALANCE),
            sections_of(&source, section::BALANCE)
        );
    }

    #[test]
    fn currencies_without_collateral_leave_coins_unset() {
        let mut source = funded();
        source.set_currency_balances(vec![]);
        let (store, _) = round_trip(&source);
        assert_eq!(store.coin_balances(), None);
        assert_eq!(store.currency_balances(), Some(&[][..]));
        assert_eq!(
            sections_of(&store, section::BALANCE),
            sections_of(&source, section::BALANCE)
        );
    }

    #[test]
    fn a_balance_delta_without_tails_keeps_the_ingested_holdings() {
        let mut source = funded();
        source.set_coin_balances(vec![CoinBalance {
            base: "BTC".into(),
            amount: 1.,
            notional: 100.,
        }]);
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        source.write(&source.account_snapshot(), &mut writer);
        let snapshot = writer.finish();
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::BALANCE);
        writer.f64s([2., 3., 5., 0.]);
        writer.end();
        let delta = writer.finish();
        let (mut store, mut state) = (Store::default(), IngestState::default());
        apply(&mut store, &mut state, &parse(&snapshot).unwrap(), true).unwrap();
        apply(&mut store, &mut state, &parse(&delta).unwrap(), false).unwrap();
        assert_eq!(store.balance().total, 5.);
        assert_eq!(store.coin_balances(), source.coin_balances());
    }

    #[test]
    fn settings_round_trip_by_symbol_including_unlisted_markets() {
        let mut source = Store::default();
        source.set_markets(vec![market("A"), market("B")]);
        source.update_setting("B", |s| {
            s.leverage = 20.;
            s.margin_type = Some(MarginType::Isolated);
        });
        source.update_setting("A", |s| {
            s.leverage = 5.;
            s.margin_type = Some(MarginType::Cross);
            s.buy_leverage = Some(3.);
            s.sell_leverage = Some(7.);
        });
        // A setting reported before its market was listed keeps its symbol.
        source.update_setting("Z", |s| s.leverage = 2.);
        let (store, _) = round_trip(&source);
        for symbol in ["A", "B", "Z"] {
            assert_eq!(store.setting(symbol), source.setting(symbol), "{symbol}");
        }
        let mut relisted = Store::default();
        relisted.set_markets(source.markets().to_vec());
        relisted.replace_settings(["A", "B", "Z"].map(|s| (s, store.setting(s).unwrap())));
        assert_eq!(
            sections_of(&relisted, section::SETTINGS),
            sections_of(&source, section::SETTINGS)
        );
    }

    #[test]
    fn revision_zero_settings_map_through_the_source_market_table() {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::SETTINGS);
        writer.u32(1);
        writer.u32(1);
        writer.u32(MarginType::Cross as u32 + 1);
        writer.pad();
        writer.f64s([10., 10., 4.]);
        writer.end();
        let frame = writer.finish();
        let mut state = IngestState {
            source_markets: vec!["A".into(), "B".into()],
            ..IngestState::default()
        };
        let mut store = Store::default();
        apply(&mut store, &mut state, &parse(&frame).unwrap(), false).unwrap();
        assert_eq!(
            store.setting("B"),
            Some(MarketSetting {
                leverage: 10.,
                margin_type: Some(MarginType::Cross),
                buy_leverage: None,
                sell_leverage: Some(4.),
            })
        );
        state.source_markets.clear();
        assert!(apply(&mut store, &mut state, &parse(&frame).unwrap(), false).is_err());
    }

    #[test]
    fn a_truncated_balance_is_refused() {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::BALANCE);
        writer.f64s([1., 2.]);
        writer.end();
        let frame = writer.finish();
        let mut store = Store::default();
        assert!(
            apply(
                &mut store,
                &mut IngestState::default(),
                &parse(&frame).unwrap(),
                false
            )
            .is_err()
        );
    }

    pub(crate) fn market(symbol: &str) -> crate::model::Market {
        use crate::model::{Limits, Market, MinMax, Precision};
        Market {
            id: format!("native-{symbol}"),
            symbol: symbol.into(),
            pretty_symbol: (symbol == "ETHUSDT").then(|| "ETH-PERP".into()),
            base: symbol.trim_end_matches("USDT").into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 100.,
                },
                leverage: MinMax { min: 1., max: 100. },
                price: None,
            },
            contract_size: Some(1.),
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }

    /// The source's next frame (everything changed since its last), ingested.
    pub(crate) fn ingest(
        source: &mut Store,
        state: &mut IngestState,
        store: &mut Store,
        snapshot: bool,
    ) -> Applied {
        let pending = source.take_pending();
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        source.write(&pending, &mut writer);
        let frame = writer.finish();
        apply(store, state, &parse(&frame).unwrap(), snapshot).unwrap()
    }

    #[test]
    fn markets_fill_the_source_table_in_its_order_with_pretty_symbols() {
        let mut source = Store::default();
        source.set_markets(vec![market("BTCUSDT"), market("ETHUSDT")]);
        let mut state = IngestState::default();
        ingest(&mut source, &mut state, &mut Store::default(), true);
        assert_eq!(state.source_markets, ["BTCUSDT", "ETHUSDT"]);
        assert_eq!(
            state.source_market_ids,
            ["native-BTCUSDT", "native-ETHUSDT"]
        );
        assert_eq!(state.symbol(1).unwrap(), "ETHUSDT");
        assert!(state.symbol(2).is_err());

        // A truncated section refuses rather than half-filling the table.
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::MARKETS);
        writer.u32(1);
        writer.end();
        let frame = writer.finish();
        let refused = apply(
            &mut Store::default(),
            &mut state,
            &parse(&frame).unwrap(),
            false,
        );
        assert!(refused.is_err());
        assert_eq!(state.source_markets, ["BTCUSDT", "ETHUSDT"]);
    }

    fn ticker(symbol: &str, last: f64, mid: Option<f64>) -> Ticker {
        Ticker {
            id: format!("native-{symbol}"),
            symbol: symbol.into(),
            bid: last - 1.,
            ask: last + 1.,
            last,
            mark: last + 0.5,
            index: last - 0.5,
            mid_price: mid,
            percentage: 1.5,
            open_interest: 7.,
            funding_rate: 0.0001,
            funding_time: 1_700_000_000_000.,
            volume: 10.,
            quote_volume: last * 10.,
        }
    }

    #[test]
    fn tickers_round_trip_with_and_without_midpoints_and_identity_tails() {
        for mid in [None, Some(100.25)] {
            let mut source = Store::default();
            source.set_markets(vec![market("BTCUSDT"), market("ETHUSDT")]);
            let rows = vec![ticker("ETHUSDT", 10., None), ticker("BTCUSDT", 100., mid)];
            source.set_tickers(rows.clone());
            let (mut state, mut store) = (IngestState::default(), Store::default());
            ingest(&mut source, &mut state, &mut store, true);
            assert_eq!(store.tickers(), rows.as_slice());
            assert!(store.loaded().tickers);
            assert_eq!(state.source_tickers, ["ETHUSDT", "BTCUSDT"]);
        }
        // A delisted symbol (no market row) keeps its identity through the tail.
        let mut source = Store::default();
        source.set_markets(vec![market("BTCUSDT")]);
        let rows = vec![ticker("OLDUSDT", 1., None)];
        source.set_tickers(rows.clone());
        let mut store = Store::default();
        ingest(&mut source, &mut IngestState::default(), &mut store, true);
        assert_eq!(store.tickers(), rows.as_slice());
    }

    #[test]
    fn ticker_deltas_apply_only_the_masked_fields_by_source_row() {
        let mut source = Store::default();
        source.set_markets(vec![market("BTCUSDT"), market("ETHUSDT")]);
        source.set_tickers(vec![
            ticker("BTCUSDT", 100., None),
            ticker("ETHUSDT", 10., Some(10.)),
        ]);
        let (mut state, mut store) = (IngestState::default(), Store::default());
        ingest(&mut source, &mut state, &mut store, true);

        source.update_ticker("ETHUSDT", |t| {
            t.last = 11.;
            t.mid_price = None;
        });
        source.update_ticker("BTCUSDT", |t| t.funding_rate = -0.0002);
        ingest(&mut source, &mut state, &mut store, false);
        assert_eq!(store.tickers(), source.tickers());
        assert_eq!(store.ticker("ETHUSDT").unwrap().mid_price, None);
        assert_eq!(store.ticker("ETHUSDT").unwrap().bid, 9.);

        // A row the source never listed, a fractional or NaN row, and a mask bit
        // past the columns all refuse rather than desynchronizing the section.
        for entry in [
            [5., 4., 1.],
            [0.5, 4., 1.],
            [f64::NAN, 4., 1.],
            [0., 4096., 1.],
        ] {
            let mut writer = FrameWriter::new(Vec::new(), 0.0);
            writer.begin(section::TICKER_DELTAS);
            writer.f64s(entry);
            writer.end();
            let frame = writer.finish();
            assert!(apply(&mut store, &mut state, &parse(&frame).unwrap(), false).is_err());
        }
        // An unknown TICKERS or MARKETS revision refuses.
        for kind in [section::TICKERS, section::MARKETS] {
            let mut writer = FrameWriter::new(Vec::new(), 0.0);
            writer.begin_revision(kind, 2);
            writer.u32(0);
            writer.end();
            let frame = writer.finish();
            assert!(apply(&mut store, &mut state, &parse(&frame).unwrap(), false).is_err());
        }
    }

    #[test]
    fn orders_and_positions_round_trip_every_field() {
        use crate::model::*;
        let order = |id: &str, status, kind, side, close, tif, trigger, extra| Order {
            id: id.into(),
            parent_id: Some("parent".into()),
            status,
            symbol: "BTCUSDT".into(),
            kind,
            side,
            price: 100.5,
            amount: 2.0,
            filled: 0.5,
            remaining: 1.5,
            reduce_only: true,
            close,
            client_order_id: Some(format!("c-{id}")),
            time_in_force: tif,
            last_update: 1_700_000_000_000.0,
            trigger_type: trigger,
            take_profit: Some(110.0),
            stop_loss: None,
            extra,
        };
        let mut source = Store::default();
        source.set_orders(vec![
            order(
                "a",
                OrderStatus::Canceling,
                OrderType::TrailingStopLoss,
                OrderSide::Sell,
                Some(false),
                Some(TimeInForce::PostOnly),
                Some(StopTriggerType::IndexPrice),
                Some(OrderExtra {
                    tp_trigger_type: Some(StopTriggerType::LastPrice),
                    sl_trigger_type: Some(StopTriggerType::MarkPrice),
                    remaining_native: Some(3.0),
                    filled_in_placement_units: None,
                    remaining_notional: Some(150.0),
                    conditional_order_price: Some(99.0),
                    reject_reason: Some(RejectReason::ReduceOnly),
                    ..OrderExtra::default()
                }),
            ),
            Order {
                parent_id: None,
                client_order_id: None,
                take_profit: None,
                ..order(
                    "b",
                    OrderStatus::Open,
                    OrderType::Limit,
                    OrderSide::Buy,
                    None,
                    None,
                    None,
                    None,
                )
            },
        ]);
        source.set_positions(vec![Position {
            id: "BTCUSDT-long".into(),
            symbol: "BTCUSDT".into(),
            side: PositionSide::Short,
            entry_price: 101.0,
            notional: 202.0,
            leverage: 5.0,
            margin_type: Some(MarginType::Cross),
            position_mode: Some(PositionMode::Hedged),
            unrealized_pnl: -1.25,
            realized_pnl: None,
            contracts: 2.0,
            liquidation_price: 80.0,
            margin_size: Some(40.0),
            last_update: None,
        }]);
        let (store, applied) = round_trip(&source);
        assert!(applied.orders && applied.positions);
        assert_eq!(store.orders(), source.orders());
        assert_eq!(store.positions(), source.positions());
    }

    #[test]
    fn deltas_follow_the_source_row_even_when_a_symbol_repeats() {
        let mut source = Store::default();
        source.set_markets(vec![market("BTCUSDT")]);
        source.set_tickers(vec![
            ticker("BTCUSDT", 100., None),
            ticker("BTCUSDT", 200., None),
        ]);
        let (mut state, mut store) = (IngestState::default(), Store::default());
        ingest(&mut source, &mut state, &mut store, true);
        source.update_ticker_at(0, |t| t.last = 101.);
        ingest(&mut source, &mut state, &mut store, false);
        assert_eq!(store.tickers()[0].last, 101.);
        assert_eq!(store.tickers()[1].last, 200.);
    }

    #[test]
    fn status_carries_account_loaded_bits_hedged_and_latency_but_not_markets() {
        let mut source = Store::default();
        source.set_hedged(true);
        source.set_latency(42.5);
        source.set_loaded(|bits| {
            bits.balance = true;
            bits.positions = true;
            bits.markets = true;
        });
        let mut store = Store::default();
        ingest(&mut source, &mut IngestState::default(), &mut store, true);
        assert!(store.hedged());
        assert_eq!(store.latency(), 42.5);
        let loaded = store.loaded();
        assert!(loaded.balance && loaded.positions);
        assert!(!loaded.orders && !loaded.tickers && !loaded.markets);

        // A later status turns bits off again, and a truncated one refuses.
        source.set_hedged(false);
        source.set_loaded(|bits| bits.balance = false);
        ingest(&mut source, &mut IngestState::default(), &mut store, false);
        assert!(!store.hedged() && !store.loaded().balance && store.loaded().positions);
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::STATUS);
        writer.u32(1);
        writer.end();
        let frame = writer.finish();
        assert!(
            apply(
                &mut store,
                &mut IngestState::default(),
                &parse(&frame).unwrap(),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn source_leverage_limits_reach_this_engines_markets_once_they_load() {
        let mut source = Store::default();
        let mut bracketed = market("BTCUSDT");
        bracketed.limits.leverage = MinMax { min: 1., max: 125. };
        source.set_markets(vec![bracketed, market("ETHUSDT")]);
        // This engine's own public read knows no brackets.
        let public = |symbol| {
            let mut market = market(symbol);
            market.limits.leverage = MinMax { min: 1., max: 1. };
            market
        };
        let (mut store, mut state) = (Store::default(), IngestState::default());
        ingest(&mut source, &mut state, &mut store, true);
        assert!(store.markets().is_empty());

        store.set_markets(vec![
            public("ETHUSDT"),
            public("BTCUSDT"),
            public("SOLUSDT"),
        ]);
        source.set_balance(Balance::default());
        ingest(&mut source, &mut state, &mut store, false);
        let leverage = |symbol| store.market(symbol).unwrap().limits.leverage;
        assert_eq!(leverage("BTCUSDT"), MinMax { min: 1., max: 125. });
        assert_eq!(leverage("ETHUSDT"), MinMax { min: 1., max: 100. });
        // A market the source does not list keeps its own limits.
        assert_eq!(leverage("SOLUSDT"), MinMax { min: 1., max: 1. });
    }

    #[test]
    fn a_source_without_collateral_clears_the_ingested_coins() {
        let mut source = funded();
        source.set_coin_balances(vec![CoinBalance {
            base: "BTC".into(),
            amount: 1.,
            notional: 100.,
        }]);
        source.set_currency_balances(vec![]);
        let (mut store, mut state) = (Store::default(), IngestState::default());
        ingest(&mut source, &mut state, &mut store, true);
        assert!(store.coin_balances().is_some());
        // A source without collateral writes the currency tail with flag bit 0.
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin(section::BALANCE);
        writer.f64s([2., 3., 5., 0.]);
        writer.u32(0);
        writer.pad();
        writer.u32(0);
        writer.u32(1);
        writer.end();
        let frame = writer.finish();
        apply(&mut store, &mut state, &parse(&frame).unwrap(), false).unwrap();
        assert_eq!(store.coin_balances(), None);
        assert_eq!(store.currency_balances(), Some(&[][..]));
    }

    #[test]
    fn an_unknown_settings_revision_is_refused() {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.begin_revision(section::SETTINGS, 2);
        writer.u32(0);
        writer.end();
        let frame = writer.finish();
        let mut store = Store::default();
        assert!(
            apply(
                &mut store,
                &mut IngestState::default(),
                &parse(&frame).unwrap(),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn an_empty_snapshot_round_trips() {
        let (_, applied) = round_trip(&Store::default());
        assert!(applied.venue_state.is_none());
    }
}
