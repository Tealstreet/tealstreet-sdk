//! The flush frame: everything a consumer needs from one flush, in one
//! transferable buffer of typed columns. Layout: docs/plans/RUST_REWRITE.md,
//! "Data shapes: the flush frame". The decoder is hosts/wasm/js/wire.mjs.
use crate::model::{
    Candle, MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide,
    RejectReason, StopTriggerType, TimeInForce,
};
use crate::store::TICKER_FIELDS;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

pub const MAGIC: [u8; 4] = *b"SCXF";
pub const FORMAT: u16 = 1;
/// An absent string reference.
pub const NONE: u32 = u32::MAX;

pub mod section {
    pub const STRINGS: u16 = 1;
    pub const MARKETS: u16 = 2;
    pub const TICKERS: u16 = 3;
    pub const TICKER_DELTAS: u16 = 4;
    pub const BOOK: u16 = 5;
    pub const CANDLES: u16 = 6;
    pub const ORDERS: u16 = 7;
    pub const POSITIONS: u16 = 8;
    pub const BALANCE: u16 = 9;
    pub const STATUS: u16 = 10;
    /// Revision 2 adds optional execution identity refs after revision-1 symbol
    /// refs, then aligned realized-PnL and fee columns (NaN when unreported).
    pub const FILLS: u16 = 11;
    /// Scalar leverage retains its offset; revision one appends buy/sell
    /// leverage columns and stable symbol references.
    pub const SETTINGS: u16 = 12;
    /// Optional typed prediction outcome identity and signing metadata.
    pub const PREDICTION_MARKETS: u16 = 13;
    /// Optional typed prediction holding settlement flags.
    pub const PREDICTION_HOLDINGS: u16 = 14;
    /// Absolute changed book levels: market, bid count, ask count, reserved,
    /// then f64 [price, amount] rows. Zero amount deletes that level.
    pub const BOOK_DELTA: u16 = 15;
    /// Native semantic instrument labels, indexed by the current market table.
    pub const INSTRUMENT_CLASSIFICATIONS: u16 = 16;
    pub const DERIVATIVE_PRODUCTS: u16 = 17;
    pub const POSITION_GREEKS: u16 = 18;
    /// The venue's capability record; snapshot frames only.
    pub const CAPABILITIES: u16 = 19;
    /// Underlying venue tokens, indexed by the current market table.
    pub const UNDERLYING_TOKENS: u16 = 20;
    /// Current account read, not a new streaming observation.
    pub const ACCOUNT_READ: u16 = 21;
}

pub mod status_flag {
    pub const HEDGED: u32 = 1;
}

pub mod market_flag {
    pub const ACTIVE: u32 = 1;
    pub const INVERSE: u32 = 1 << 1;
    pub const SPOT: u32 = 1 << 2;
    pub const PERPETUAL: u32 = 1 << 3;
    pub const EXPIRING: u32 = 1 << 4;
    pub const READ_ONLY: u32 = 1 << 5;
    /// Bits 8..12 hold the market's `QuantityUnits`.
    pub const UNITS_SHIFT: u32 = 8;
}

pub mod loaded_flag {
    pub const BALANCE: u32 = 1;
    pub const ORDERS: u32 = 1 << 1;
    pub const MARKETS: u32 = 1 << 2;
    pub const TICKERS: u32 = 1 << 3;
    pub const POSITIONS: u32 = 1 << 4;
}

/// Order enums packed into one u32, one field per byte:
/// status | type << 8 | (side | reduceOnly << 1 | close << 2 | closeKnown << 3
/// | rejectReason << 4) << 16 | (timeInForce | triggerType << 4) << 24.
/// Zero means absent where optional.
#[allow(clippy::too_many_arguments)]
pub fn order_enums(
    status: OrderStatus,
    kind: OrderType,
    side: OrderSide,
    reduce_only: bool,
    close: Option<bool>,
    time_in_force: Option<TimeInForce>,
    trigger: Option<StopTriggerType>,
    reject: Option<RejectReason>,
) -> u32 {
    let status = status as u32;
    let kind = kind as u32;
    let flags = (side as u32)
        | ((reduce_only as u32) << 1)
        | (((close == Some(true)) as u32) << 2)
        | ((close.is_some() as u32) << 3)
        | (reject.map_or(0, |r| r as u32 + 1) << 4);
    let tif = time_in_force.map_or(0, |t| t as u32 + 1);
    let trigger = trigger.map_or(0, |t| t as u32 + 1);
    status | (kind << 8) | (flags << 16) | ((tif | (trigger << 4)) << 24)
}

/// side | marginType << 8 | positionMode << 16; optional fields are 0 when absent.
pub fn position_enums(
    side: PositionSide,
    margin: Option<MarginType>,
    mode: Option<PositionMode>,
) -> u32 {
    (side as u32)
        | (margin.map_or(0, |m| m as u32 + 1) << 8)
        | (mode.map_or(0, |m| m as u32 + 1) << 16)
}

pub struct FrameWriter<'a> {
    buf: Vec<u8>,
    sections: u16,
    open: Option<usize>,
    strings: Vec<&'a str>,
    string_index: HashMap<u32, u32>,
}

impl<'a> FrameWriter<'a> {
    pub fn new(mut buf: Vec<u8>, now: f64) -> Self {
        buf.clear();
        buf.extend_from_slice(&MAGIC);
        buf.extend_from_slice(&FORMAT.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&now.to_le_bytes());
        Self {
            buf,
            sections: 0,
            open: None,
            strings: Vec::new(),
            string_index: HashMap::new(),
        }
    }

    /// Reserve known table sizes once, avoiding geometric growth during startup frames.
    pub fn reserve_tables(&mut self, bytes: usize, strings: usize) {
        self.buf.reserve_exact(bytes);
        self.strings.reserve_exact(strings);
        self.string_index.reserve(strings);
    }

    pub fn is_empty(&self) -> bool {
        self.sections == 0
    }

    pub fn string(&mut self, value: &'a str) -> u32 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        self.intern(value, hasher.finish() as u32)
    }

    fn intern(&mut self, value: &'a str, mut hash: u32) -> u32 {
        loop {
            match self.string_index.get(&hash) {
                Some(&index) if self.strings[index as usize] == value => return index,
                Some(_) => hash = hash.wrapping_add(1),
                None => {
                    let index = self.strings.len() as u32;
                    self.strings.push(value);
                    self.string_index.insert(hash, index);
                    return index;
                }
            }
        }
    }

    pub fn optional_string(&mut self, value: Option<&'a str>) -> u32 {
        value.map_or(NONE, |v| self.string(v))
    }

    pub fn begin(&mut self, kind: u16) {
        self.begin_revision(kind, 0);
    }

    pub fn begin_revision(&mut self, kind: u16, revision: u16) {
        debug_assert!(self.open.is_none(), "sections do not nest");
        self.buf.extend_from_slice(&kind.to_le_bytes());
        self.buf.extend_from_slice(&revision.to_le_bytes());
        self.open = Some(self.buf.len());
        self.buf.extend_from_slice(&0u32.to_le_bytes());
    }

    pub fn end(&mut self) {
        let length_at = self.open.take().expect("a section is open");
        self.pad();
        let length = (self.buf.len() - length_at - 4) as u32;
        self.buf[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
        self.sections += 1;
    }

    pub fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    pub fn u32s(&mut self, values: impl IntoIterator<Item = u32>) {
        for value in values {
            self.u32(value);
        }
    }

    /// Pad to 8 bytes so the following f64 data can be viewed in place.
    pub fn pad(&mut self) {
        while !self.buf.len().is_multiple_of(8) {
            self.buf.push(0);
        }
    }

    pub fn f64(&mut self, value: f64) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    pub fn f64s(&mut self, values: impl IntoIterator<Item = f64>) {
        for value in values {
            self.f64(value);
        }
    }

    pub fn candles(&mut self, rows: &[(u32, &'a str, Candle)]) {
        if rows.is_empty() {
            return;
        }
        self.begin(section::CANDLES);
        self.u32(rows.len() as u32);
        self.u32s(rows.iter().map(|(market, ..)| *market));
        let intervals: Vec<u32> = rows.iter().map(|(_, i, _)| self.string(i)).collect();
        self.u32s(intervals);
        self.pad();
        for (_, _, c) in rows {
            self.f64s([c.timestamp, c.open, c.high, c.low, c.close, c.volume]);
        }
        self.end();
    }

    /// Append the strings section, patch the section count and return the frame.
    pub fn finish(mut self) -> Vec<u8> {
        if !self.strings.is_empty() {
            let strings = std::mem::take(&mut self.strings);
            self.begin(section::STRINGS);
            self.u32(strings.len() as u32);
            let mut offset = 0u32;
            self.u32(0);
            for s in &strings {
                offset += s.len() as u32;
                self.u32(offset);
            }
            for s in &strings {
                self.buf.extend_from_slice(s.as_bytes());
            }
            self.end();
        }
        self.buf[6..8].copy_from_slice(&self.sections.to_le_bytes());
        self.buf
    }
}

/// The `(kind, payload)` sections of a frame, for native consumers; the
/// browser reads frames with `hosts/wasm/js/wire.mjs`.
pub fn sections(frame: &[u8]) -> impl Iterator<Item = (u16, &[u8])> {
    let count = if frame.len() >= 16
        && frame[..4] == MAGIC
        && u16::from_le_bytes([frame[4], frame[5]]) == FORMAT
    {
        u16::from_le_bytes([frame[6], frame[7]])
    } else {
        0
    };
    let mut at = 16;
    (0..count).map_while(move |_| {
        let header = frame.get(at..at + 8)?;
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let payload = frame.get(at + 8..at + 8 + length)?;
        at += 8 + length;
        Some((kind, payload))
    })
}

/// The executions a frame's `FILLS` section carries (revisions 1 and 2), as
/// `FrameApplier.fills` reads them. `None` for a malformed frame or section.
pub fn fills(frame: &[u8]) -> Option<Vec<crate::engine::Fill>> {
    let u32_at = |at: usize| {
        frame
            .get(at..at.checked_add(4)?)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
    };
    let f64_at = |at: usize| {
        frame
            .get(at..at.checked_add(8)?)
            .map(|bytes| f64::from_le_bytes(bytes.try_into().unwrap()))
    };
    if frame.len() < 16 || frame[..4] != MAGIC || u16::from_le_bytes([frame[4], frame[5]]) != FORMAT
    {
        return None;
    }
    let mut strings = None;
    let mut section = None;
    let mut at = 16;
    for _ in 0..u16::from_le_bytes([frame[6], frame[7]]) {
        let header = frame.get(at..at + 8)?;
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let revision = u16::from_le_bytes([header[2], header[3]]);
        let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let payload = at + 8;
        frame.get(payload..payload.checked_add(length)?)?;
        if kind == section::STRINGS {
            strings = Some((payload, length));
        } else if kind == section::FILLS && section.is_none() {
            section = Some((payload, length, revision));
        }
        at = payload + length;
    }
    let Some((offset, length, revision)) = section else {
        return Some(Vec::new());
    };
    if revision == 0 || revision > 2 {
        return None;
    }
    let string = |index: u32| -> Option<Option<String>> {
        if index == NONE {
            return Some(None);
        }
        let (table, table_length) = strings?;
        let count = u32_at(table)? as usize;
        let bytes = table + 4 + 4 * (count + 1);
        if (index as usize) >= count || bytes > table + table_length {
            return None;
        }
        let start = u32_at(table + 4 + 4 * index as usize)? as usize;
        let end = u32_at(table + 8 + 4 * index as usize)? as usize;
        let text = frame.get(bytes + start..bytes + end)?;
        Some(Some(std::str::from_utf8(text).ok()?.to_owned()))
    };
    let align8 = |n: usize| n.div_ceil(8) * 8;
    let count = u32_at(offset)? as usize;
    let data = align8(offset + 4 + count * 12);
    let symbols = data + count * 16;
    let ids = symbols + count * 4;
    if ids + if revision == 2 { count * 4 } else { 0 } > offset + length {
        return None;
    }
    (0..count)
        .map(|i| {
            let side = match u32_at(offset + 4 + (2 * count + i) * 4)? {
                0 => crate::model::OrderSide::Buy,
                1 => crate::model::OrderSide::Sell,
                _ => return None,
            };
            Some(crate::engine::Fill {
                id: if revision == 2 {
                    string(u32_at(ids + i * 4)?)?
                } else {
                    None
                },
                order_id: string(u32_at(offset + 4 + i * 4)?)??,
                symbol: string(u32_at(symbols + i * 4)?)??,
                side,
                price: f64_at(data + i * 16)?,
                amount: f64_at(data + i * 16 + 8)?,
            })
        })
        .collect()
}

/// A `BOOK` section's market and best bid and ask, if both sides have levels.
pub fn book_top(payload: &[u8]) -> Option<(u32, f64, f64)> {
    let word = |i: usize| u32::from_le_bytes(payload[i * 4..i * 4 + 4].try_into().unwrap());
    let float = |at: usize| f64::from_le_bytes(payload[at..at + 8].try_into().unwrap());
    if payload.len() < 16 {
        return None;
    }
    let (market, bids, asks) = (word(0), word(1) as usize, word(2) as usize);
    if bids == 0 || asks == 0 || payload.len() < 16 + (bids + asks) * 24 {
        return None;
    }
    Some((market, float(16), float(16 + bids * 24)))
}

/// Ticker fields in revision one, including optional midpoint at bit eleven.
pub const TICKER_COLUMNS: usize = TICKER_FIELDS.len();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Candle, Limits, Market, MinMax, Order, OrderExtra, Precision, Ticker};
    use crate::store::Store;

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn interning_resolves_hash_collisions_by_full_string_equality() {
        let mut w = FrameWriter::new(Vec::new(), 0.);
        assert_eq!(w.intern("one", 7), 0);
        assert_eq!(w.intern("two", 7), 1);
        assert_eq!(w.intern("three", 8), 2);
        assert_eq!(w.intern("two", 7), 1);
        assert_eq!(w.intern("one", 7), 0);
        assert_eq!(w.intern("wrap", u32::MAX), 3);
        assert_eq!(w.intern("wrap-two", u32::MAX), 4);
        assert_eq!(w.intern("wrap-two", u32::MAX), 4);
    }

    #[test]
    fn revised_ticker_startup_reserves_numeric_columns_and_identity_tails() {
        let mut store = Store::default();
        store.set_markets(
            (0..128)
                .map(|index| Market {
                    id: format!("native-{index}"),
                    symbol: format!("asset-{index}/USDC"),
                    pretty_symbol: None,
                    base: format!("asset-{index}"),
                    quote: "USDC".into(),
                    active: true,
                    precision: Precision {
                        amount: 1.,
                        price: 0.01,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: 1.,
                            max: 1000.,
                        },
                        leverage: MinMax { min: 1., max: 1. },
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
                })
                .collect(),
        );
        store.set_tickers(
            (0..128)
                .map(|index| Ticker {
                    id: format!("native-{index}"),
                    symbol: format!("asset-{index}/USDC"),
                    last: index as f64 + 1.,
                    mid_price: Some(index as f64 + 0.5),
                    ..Ticker::default()
                })
                .collect(),
        );
        let pending = store.take_pending();
        let mut writer = FrameWriter::new(Vec::new(), 0.);
        store.write(&pending, &mut writer);
        let frame = writer.finish();
        // Shared quote strings leave conservative slack, far below a doubled buffer.
        assert!(
            frame.capacity() <= frame.len() + 2048,
            "startup reservation grew geometrically: capacity={}, length={}",
            frame.capacity(),
            frame.len(),
        );
        let rows = sections(&frame)
            .find(|(kind, _)| *kind == section::TICKERS)
            .unwrap()
            .1;
        assert_eq!(u32_at(rows, 0), 128);
        let strings = sections(&frame)
            .find(|(kind, _)| *kind == section::STRINGS)
            .unwrap()
            .1;
        let string_data = 4 + (u32_at(strings, 0) as usize + 1) * 4;
        let text = |reference: u32| {
            let at = 4 + reference as usize * 4;
            let start = string_data + u32_at(strings, at) as usize;
            let end = string_data + u32_at(strings, at + 4) as usize;
            std::str::from_utf8(&strings[start..end]).unwrap()
        };
        let data = 520;
        for index in 0..128 {
            assert_eq!(u32_at(rows, 4 + index * 4), index as u32);
            let midpoint = data + 128 * 88 + index * 8;
            assert_eq!(
                f64::from_le_bytes(rows[midpoint..midpoint + 8].try_into().unwrap()),
                index as f64 + 0.5,
            );
            assert_eq!(
                text(u32_at(rows, data + 128 * 96 + index * 4)),
                format!("asset-{index}/USDC")
            );
            assert_eq!(
                text(u32_at(rows, data + 128 * 100 + index * 4)),
                format!("native-{index}")
            );
        }
    }

    #[test]
    fn frames_are_aligned_and_self_describing() {
        let mut store = Store::default();
        store.set_markets(vec![Market {
            id: "BTC/USDT:USDT".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 1000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 125.0,
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
        }]);
        store.set_tickers(vec![Ticker {
            id: "BTC/USDT:USDT".into(),
            symbol: "BTCUSDT".into(),
            last: 100.0,
            ..Ticker::default()
        }]);
        let pending = store.take_pending();
        let mut w = FrameWriter::new(Vec::new(), 1234.0);
        store.write(&pending, &mut w);
        let candle = Candle {
            timestamp: 1.0,
            open: 2.0,
            high: 3.0,
            low: 1.5,
            close: 2.5,
            volume: 9.0,
        };
        w.candles(&[(0, "1m", candle)]);
        let frame = w.finish();

        assert_eq!(&frame[..4], b"SCXF");
        assert_eq!(u16::from_le_bytes([frame[4], frame[5]]), FORMAT);
        assert_eq!(f64::from_le_bytes(frame[8..16].try_into().unwrap()), 1234.0);
        let mut kinds = Vec::new();
        let mut at = 16;
        for _ in 0..u16::from_le_bytes([frame[6], frame[7]]) {
            assert_eq!(at % 8, 0, "section header aligned");
            let kind = u16::from_le_bytes([frame[at], frame[at + 1]]);
            let length = u32_at(&frame, at + 4) as usize;
            assert_eq!(length % 8, 0, "payload padded");
            kinds.push(kind);
            at += 8 + length;
        }
        assert_eq!(at, frame.len());
        assert_eq!(
            kinds,
            [
                section::MARKETS,
                section::TICKERS,
                section::STATUS,
                section::CANDLES,
                section::STRINGS
            ]
        );
    }

    #[test]
    fn revision_one_order_tails_preserve_full_stop_leaves_and_all_native_metadata() {
        let order = Order {
            id: "full-stop".into(),
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
            extra: None,
        };
        for native in [None, Some(7.0)] {
            let mut store = Store::default();
            store.set_orders(vec![Order {
                extra: native.map(|remaining_native| OrderExtra {
                    remaining_native: Some(remaining_native),
                    filled_in_placement_units: Some(3.0),
                    remaining_notional: Some(700.0),
                    conditional_order_price: Some(54320.0),
                    tp_trigger_type: Some(StopTriggerType::IndexPrice),
                    sl_trigger_type: Some(StopTriggerType::LastPrice),
                    ..OrderExtra::default()
                }),
                ..order.clone()
            }]);
            let pending = store.take_pending();
            let mut writer = FrameWriter::new(Vec::new(), 1000.0);
            store.write(&pending, &mut writer);
            let frame = writer.finish();
            let payload = sections(&frame)
                .find_map(|(kind, payload)| (kind == section::ORDERS).then_some(payload))
                .unwrap();
            assert_eq!(u32_at(payload, 0), 1);
            // One row: 24 bytes of aligned references/enums, then seven columns.
            assert_eq!(u16::from_le_bytes(frame[18..20].try_into().unwrap()), 1);
            assert_eq!(payload.len(), 24 + 11 * 8 + 8);
            let number =
                |offset| f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
            assert_eq!(
                number(24 + 8),
                0.0,
                "Full stops retain their amount-zero sentinel"
            );
            assert_eq!(
                number(24 + 3 * 8),
                0.00013,
                "base remainder stays in its existing column"
            );
            if let Some(native) = native {
                assert_eq!(
                    number(24 + 7 * 8),
                    native,
                    "native quantity is exact, not rounded base x price"
                );
                assert_eq!(number(24 + 8 * 8), 3.0);
                assert_eq!(number(24 + 9 * 8), 700.0);
                assert_eq!(number(24 + 10 * 8), 54320.0);
                assert_eq!(u32_at(payload, 24 + 11 * 8), 3 | (2 << 8));
            } else {
                for column in 7..11 {
                    assert!(number(24 + column * 8).is_nan());
                }
                assert_eq!(u32_at(payload, 24 + 11 * 8), 0);
            }
            assert_ne!(
                u32_at(payload, 24 + 11 * 8 + 4),
                NONE,
                "orphan order retains its symbol"
            );
        }
    }

    #[test]
    fn native_sections_reject_unsupported_frame_formats() {
        let mut writer = FrameWriter::new(Vec::new(), 0.);
        writer.begin_revision(section::ORDERS, 1);
        writer.u32(0);
        writer.end();
        let mut frame = writer.finish();
        assert_eq!(sections(&frame).count(), 1);
        frame[4..6].copy_from_slice(&(FORMAT + 1).to_le_bytes());
        assert_eq!(sections(&frame).count(), 0);
    }
}
