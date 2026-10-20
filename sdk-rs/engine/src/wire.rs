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
    pub const FILLS: u16 = 11;
    /// Scalar leverage retains its offset; optional buy/sell f64 columns follow it.
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

    pub fn is_empty(&self) -> bool {
        self.sections == 0
    }

    /// Reserve known table sizes before writing, avoiding overlapping growth allocations.
    pub fn reserve(&mut self, bytes: usize, strings: usize, indexed_strings: usize) {
        self.buf.reserve_exact(bytes);
        self.strings.reserve_exact(strings);
        self.string_index.reserve(indexed_strings);
    }

    /// Append identities directly; deduplication is only useful for shared labels.
    pub fn unshared_string(&mut self, value: &'a str) -> u32 {
        let index = self.strings.len() as u32;
        self.strings.push(value);
        index
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
    use crate::model::{Candle, Limits, Market, MinMax, Order, OrderExtra, Precision, Ticker};
    use crate::store::Store;

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn reservation_preserves_bytes_and_reuses_the_output_buffer() {
        fn write(mut writer: FrameWriter<'_>) -> Vec<u8> {
            writer.begin(section::MARKETS);
            let first = writer.string("BTCUSDT");
            let second = writer.string("USDT");
            writer.u32s([first, second, first]);
            writer.f64(96000.0);
            writer.end();
            writer.finish()
        }
        let expected = write(FrameWriter::new(Vec::new(), 1.0));
        let mut writer = FrameWriter::new(Vec::new(), 1.0);
        writer.reserve(expected.len() - 16, 2, 2);
        let pointer = writer.buf.as_ptr();
        let actual = write(writer);
        assert_eq!(actual, expected);
        assert_eq!(actual.as_ptr(), pointer);
        let mut writer = FrameWriter::new(actual, 1.0);
        writer.reserve(expected.len() - 16, 2, 2);
        assert_eq!(writer.buf.as_ptr(), pointer);
        assert_eq!(write(writer), expected);
    }

    #[test]
    fn unshared_repeated_labels_decode_with_the_same_string_values() {
        let mut writer = FrameWriter::new(Vec::new(), 1.0);
        writer.begin(section::MARKETS);
        let first_base = writer.unshared_string("BTC");
        let first_quote = writer.string("USDT");
        let second_base = writer.unshared_string("BTC");
        let second_quote = writer.string("USDT");
        assert_ne!(first_base, second_base);
        assert_eq!(first_quote, second_quote);
        assert_eq!(writer.string_index.len(), 1);
        writer.u32s([first_base, first_quote, second_base, second_quote]);
        writer.end();
        let frame = writer.finish();
        let strings = sections(&frame)
            .find(|(kind, _)| *kind == section::STRINGS)
            .unwrap()
            .1;
        let data = 4 + (u32_at(strings, 0) as usize + 1) * 4;
        let read = |index: u32| {
            let from = u32_at(strings, (index as usize + 1) * 4) as usize;
            let to = u32_at(strings, (index as usize + 2) * 4) as usize;
            std::str::from_utf8(&strings[data + from..data + to]).unwrap()
        };
        assert_eq!(read(first_base), "BTC");
        assert_eq!(read(second_base), "BTC");
        assert_eq!(read(first_quote), "USDT");
        assert_eq!(read(second_quote), "USDT");
    }

    #[test]
    fn large_market_tables_reserve_the_initial_frame_without_geometric_slack() {
        let mut store = Store::default();
        let markets: Vec<_> = (0..697)
            .map(|index| Market {
                id: format!("TOKEN{index}/USDT"),
                symbol: format!("TOKEN{index}USDT"),
                pretty_symbol: None,
                base: format!("TOKEN{index}"),
                quote: "USDT".into(),
                active: true,
                precision: Precision {
                    amount: 0.001,
                    price: 0.1,
                },
                limits: Limits {
                    amount: MinMax {
                        min: 0.0,
                        max: 100.0,
                    },
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
            })
            .collect();
        store.set_tickers(
            markets
                .iter()
                .map(|market| Ticker {
                    id: market.id.clone(),
                    symbol: market.symbol.clone(),
                    ..Ticker::default()
                })
                .collect(),
        );
        store.set_markets(markets);
        let pending = store.everything();
        let mut writer = FrameWriter::new(Vec::new(), 1.0);
        store.write(&pending, &mut writer);
        assert_eq!(writer.strings.len(), 3 * 697 + 1);
        assert_eq!(writer.strings.capacity(), 3 * 697 + 1);
        assert_eq!(writer.string_index.len(), 1);
        assert!(writer.string_index.capacity() < 8);
        let pointer = writer.buf.as_ptr();
        let capacity = writer.buf.capacity();
        let frame = writer.finish();
        assert_eq!(frame.as_ptr(), pointer, "the strings trailer must fit");
        assert!(
            capacity < frame.len() + 16,
            "bounded estimate, not geometric slack"
        );
        let mut writer = FrameWriter::new(frame, 1.0);
        store.write(&pending, &mut writer);
        assert_eq!(writer.buf.capacity(), capacity);
        let frame = writer.finish();
        assert_eq!(frame.as_ptr(), pointer);
    }

    #[test]
    fn settings_reservation_includes_optional_leverage_columns() {
        for rails in [false, true] {
            let mut store = Store::default();
            store.set_settings((0..64).map(|market| {
                (
                    market,
                    crate::store::MarketSetting {
                        leverage: 10.0,
                        buy_leverage: rails.then_some(20.0),
                        sell_leverage: rails.then_some(30.0),
                        ..crate::store::MarketSetting::default()
                    },
                )
            }));
            let pending = store.everything();
            let mut writer = FrameWriter::new(Vec::new(), 1.0);
            store.write(&pending, &mut writer);
            let capacity = writer.buf.capacity();
            let frame = writer.finish();
            assert!(capacity < frame.len() + 16, "reserve every settings column");
            let payload = sections(&frame)
                .find(|(kind, _)| *kind == section::SETTINGS)
                .unwrap()
                .1;
            assert_eq!(payload.len(), 8 + 64 * (8 + if rails { 24 } else { 8 }));
        }
    }

    #[test]
    fn collateral_snapshot_reservation_includes_columns_and_strings() {
        let mut store = Store::default();
        store.set_coin_balances(
            (0..64)
                .map(|index| crate::model::CoinBalance {
                    base: format!("COIN{index}"),
                    amount: 1.0,
                    notional: 100.0,
                })
                .collect(),
        );
        let pending = store.everything();
        let mut writer = FrameWriter::new(Vec::new(), 1.0);
        store.write(&pending, &mut writer);
        let capacity = writer.buf.capacity();
        assert_eq!(writer.strings.capacity(), 64);
        let pointer = writer.buf.as_ptr();
        let frame = writer.finish();
        assert_eq!(frame.as_ptr(), pointer, "collateral strings must fit");
        assert!(
            capacity < frame.len() + 16,
            "reserve the complete collateral tail"
        );
    }

    #[test]
    fn currency_snapshot_reservation_includes_empty_collateral_and_all_columns() {
        for count in [0, 63, 64] {
            for collateral in [false, true] {
                let mut store = Store::default();
                if collateral {
                    store.set_coin_balances(
                        (0..count)
                            .map(|index| crate::model::CoinBalance {
                                base: format!("COIN{index}"),
                                amount: 1.0,
                                notional: 100.0,
                            })
                            .collect(),
                    );
                }
                store.set_currency_balances(
                    (0..count)
                        .map(|index| crate::model::CurrencyBalance {
                            currency: format!("CURRENCY{index}"),
                            balance: crate::model::Balance {
                                used: 1.0,
                                free: 2.0,
                                total: 3.0,
                                upnl: 4.0,
                            },
                        })
                        .collect(),
                );
                let pending = store.everything();
                let mut writer = FrameWriter::new(Vec::new(), 1.0);
                store.write(&pending, &mut writer);
                let capacity = writer.buf.capacity();
                assert_eq!(
                    writer.strings.capacity(),
                    count * if collateral { 2 } else { 1 }
                );
                let pointer = writer.buf.as_ptr();
                let frame = writer.finish();
                assert_eq!(frame.as_ptr(), pointer, "currency strings must fit");
                assert!(
                    capacity < frame.len() + 16,
                    "reserve the full currency tail"
                );
                let payload = sections(&frame)
                    .find(|(kind, _)| *kind == section::BALANCE)
                    .unwrap()
                    .1;
                let coins = if collateral { count } else { 0 };
                assert_eq!(
                    payload.len(),
                    32 + (4 + coins * 4).next_multiple_of(8)
                        + coins * 16
                        + (4 + count * 4).next_multiple_of(8)
                        + count * 32
                );
            }
        }
    }

    #[test]
    fn orphan_ticker_reservation_includes_identity_columns_and_strings() {
        for count in [63, 64] {
            let mut store = Store::default();
            store.set_tickers(
                (0..count)
                    .map(|index| Ticker {
                        id: format!("native-retired-{index}"),
                        symbol: format!("RETIRED{index}USDT"),
                        ..Ticker::default()
                    })
                    .collect(),
            );
            let pending = store.everything();
            let mut writer = FrameWriter::new(Vec::new(), 1.0);
            store.write(&pending, &mut writer);
            let capacity = writer.buf.capacity();
            assert_eq!(writer.strings.capacity(), count * 2);
            let pointer = writer.buf.as_ptr();
            let frame = writer.finish();
            assert_eq!(frame.as_ptr(), pointer, "ticker identity strings must fit");
            assert!(
                capacity < frame.len() + 16,
                "reserve both ticker identity columns"
            );
            let payload = sections(&frame)
                .find(|(kind, _)| *kind == section::TICKERS)
                .unwrap()
                .1;
            assert_eq!(
                payload.len(),
                (4 + count * 4).next_multiple_of(8) + count * (TICKER_COLUMNS * 8 + 8)
            );
        }
    }

    #[test]
    fn position_identity_reservation_includes_optional_tail_and_strings() {
        for retained in [false, true] {
            for count in [63, 64] {
                let mut store = Store::default();
                if retained {
                    store.retain_position_ids();
                }
                store.set_positions(
                    (0..count)
                        .map(|index| crate::model::Position {
                            id: format!("BTCUSDT:Isolated:{index}"),
                            symbol: "BTCUSDT".into(),
                            side: PositionSide::Long,
                            entry_price: 100.0,
                            notional: 100.0,
                            leverage: 10.0,
                            margin_type: Some(MarginType::Isolated),
                            position_mode: Some(PositionMode::Oneway),
                            unrealized_pnl: 0.0,
                            realized_pnl: None,
                            contracts: 1.0,
                            liquidation_price: 90.0,
                            margin_size: Some(10.0),
                            last_update: None,
                        })
                        .collect(),
                );
                let pending = store.everything();
                let mut writer = FrameWriter::new(Vec::new(), 1.0);
                store.write(&pending, &mut writer);
                let capacity = writer.buf.capacity();
                assert_eq!(writer.strings.capacity(), if retained { count } else { 0 });
                let pointer = writer.buf.as_ptr();
                let frame = writer.finish();
                assert_eq!(frame.as_ptr(), pointer, "position strings must fit");
                assert!(capacity < frame.len() + 16, "reserve the position ID tail");
                let payload = sections(&frame)
                    .find(|(kind, _)| *kind == section::POSITIONS)
                    .unwrap()
                    .1;
                assert_eq!(
                    payload.len(),
                    8 + count * 80
                        + if retained {
                            (count * 4).next_multiple_of(8)
                        } else {
                            0
                        }
                );
            }
        }
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
    fn optional_order_native_column_preserves_full_stop_leaves_without_changing_legacy_columns() {
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
            assert_eq!(
                payload.len(),
                24 + 7 * 8 + usize::from(native.is_some()) * 8
            );
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
            }
            store.set_orders(
                (0..64)
                    .map(|index| Order {
                        id: format!("stop-{index}"),
                        extra: native.map(|remaining_native| OrderExtra {
                            remaining_native: Some(remaining_native),
                            ..OrderExtra::default()
                        }),
                        ..order.clone()
                    })
                    .collect(),
            );
            let pending = store.everything();
            let mut writer = FrameWriter::new(Vec::new(), 1000.0);
            store.write(&pending, &mut writer);
            let capacity = writer.buf.capacity();
            let pointer = writer.buf.as_ptr();
            let frame = writer.finish();
            assert_eq!(frame.as_ptr(), pointer, "order strings must fit");
            assert!(capacity < frame.len() + 16, "reserve every order column");
        }
    }
}
