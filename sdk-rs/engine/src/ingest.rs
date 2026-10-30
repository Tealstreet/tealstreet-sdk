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
use crate::model::{Balance, CoinBalance, CurrencyBalance, MarginType};
use crate::store::{MarketSetting, Store};
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

fn orders(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
    Ok(())
}

fn positions(
    _: &mut Store,
    _: &IngestState,
    _: &Frame<'_>,
    _: &Section<'_>,
) -> Result<(), IngestError> {
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

    #[test]
    fn an_empty_snapshot_round_trips() {
        let (_, applied) = round_trip(&Store::default());
        assert!(applied.venue_state.is_none());
    }
}
