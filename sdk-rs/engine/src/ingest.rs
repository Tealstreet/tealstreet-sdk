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
use crate::store::Store;
use crate::wire::{FORMAT, MAGIC, NONE, section};

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
    for section in &frame.sections {
        match section.kind {
            section::MARKETS => {}
            section::TICKERS => tickers(store, state, frame, section)?,
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
            section::SETTINGS => settings(store, state, frame, section)?,
            section::STATUS => status(store, state, frame, section)?,
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
    cursor.bytes(8 * 10 * count)?;
    if section.revision >= 1 {
        cursor.bytes(4 * count)?;
    }
    state.source_markets = symbols;
    state.source_market_ids = ids;
    Ok(())
}

fn tickers(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
    Ok(())
}

fn ticker_deltas(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
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

fn balance(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
    Ok(())
}

fn settings(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
    Ok(())
}

fn status(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
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
    fn an_empty_snapshot_round_trips() {
        let (_, applied) = round_trip(&Store::default());
        assert!(applied.venue_state.is_none());
    }
}
