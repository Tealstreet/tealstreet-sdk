//! The flush frame: everything a consumer needs from one flush, in one
//! transferable buffer of typed columns. Layout: docs/plans/RUST_REWRITE.md,
//! "Data shapes: the flush frame". The decoder is hosts/wasm/js/wire.mjs.
use crate::model::{
    Candle, MarginType, OrderSide, OrderStatus, OrderType, PositionMode, PositionSide,
    RejectReason, StopTriggerType, TimeInForce,
};
use crate::store::TICKER_FIELDS;
use std::collections::HashMap;

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
    pub const FILLS: u16 = 11;
    pub const SETTINGS: u16 = 12;
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
    string_bytes: usize,
    string_index: HashMap<&'a str, u32>,
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
            string_bytes: 0,
            string_index: HashMap::new(),
        }
    }

    /// Reserve a whole section, including strings accumulated by earlier
    /// sections. Exact growth avoids transient doubling of large wasm frames.
    pub fn reserve(&mut self, additional: usize) {
        let strings = if self.strings.is_empty() {
            0
        } else {
            8 + (8 + 4 * self.strings.len() + self.string_bytes).next_multiple_of(8)
        };
        self.buf.reserve_exact(additional + strings);
    }

    pub fn is_empty(&self) -> bool {
        self.sections == 0
    }

    pub fn string(&mut self, value: &'a str) -> u32 {
        if let Some(&index) = self.string_index.get(value) {
            return index;
        }
        let index = self.strings.len() as u32;
        self.strings.push(value);
        self.string_bytes += value.len();
        self.string_index.insert(value, index);
        index
    }

    pub fn optional_string(&mut self, value: Option<&'a str>) -> u32 {
        value.map_or(NONE, |v| self.string(v))
    }

    pub fn begin(&mut self, kind: u16) {
        debug_assert!(self.open.is_none(), "sections do not nest");
        self.buf.extend_from_slice(&kind.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
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
        let interval_bytes: usize = rows.iter().map(|(_, interval, _)| 4 + interval.len()).sum();
        self.reserve(32 + 56 * rows.len() + interval_bytes.next_multiple_of(8));
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
            self.reserve(0);
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
    let count = if frame.len() >= 16 && frame[..4] == MAGIC {
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

/// Number of f64 ticker columns in `TICKERS` and fields in `TICKER_DELTAS`.
pub const TICKER_COLUMNS: usize = TICKER_FIELDS.len();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
    use crate::store::Store;

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn candles_reserve_their_string_section_before_writing() {
        let mut writer = FrameWriter::new(Vec::new(), 0.0);
        writer.candles(&[(
            0,
            "1m",
            Candle {
                timestamp: 1.0,
                open: 2.0,
                high: 3.0,
                low: 1.0,
                close: 2.5,
                volume: 4.0,
            },
        )]);
        let allocation = writer.buf.as_ptr();
        let capacity = writer.buf.capacity();
        let frame = writer.finish();
        assert_eq!(frame.len(), 112);
        assert_eq!(capacity, frame.capacity());
        assert_eq!(
            allocation,
            frame.as_ptr(),
            "the final strings fit without reallocating"
        );
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

        let mut reused = FrameWriter::new(vec![255; frame.len() * 2], 1234.0);
        store.write(&pending, &mut reused);
        reused.candles(&[(0, "1m", candle)]);
        assert_eq!(
            frame,
            reused.finish(),
            "buffer capacity does not change the wire"
        );

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
}
