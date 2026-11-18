use super::*;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};

fn market(symbol: &str) -> Market {
    Market {
        id: symbol.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.01,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 1000.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: Some(1.),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn ticker(symbol: &str, price: f64) -> Ticker {
    Ticker {
        id: symbol.into(),
        symbol: symbol.into(),
        last: price,
        mark: price + 1.,
        ..Ticker::default()
    }
}
fn encode(source: &mut Store) -> Arc<[u8]> {
    let pending = source.take_pending();
    let mut writer = FrameWriter::new(Vec::new(), 10.);
    source.write(&pending, &mut writer);
    writer.finish().into()
}
fn seeded() -> (Publisher, Store) {
    let mut source = Store::default();
    source.set_markets(vec![market("BTCUSDT"), market("ETHUSDT")]);
    source.set_tickers(vec![ticker("BTCUSDT", 100.), ticker("ETHUSDT", 200.)]);
    let mut publisher = Publisher::new(500, 16 * 1024 * 1024);
    publisher.receive(encode(&mut source), true).unwrap();
    (publisher, source)
}
fn kinds(bytes: &[u8]) -> Vec<u16> {
    wire::sections(bytes).map(|(kind, _)| kind).collect()
}

#[test]
fn unpaced_output_keeps_original_bytes_and_all_unknown_sections() {
    let mut writer = FrameWriter::new(Vec::new(), 123.);
    let label = writer.string("opaque-control");
    writer.begin_revision(65000, 7);
    writer.u32(label);
    writer.pad();
    writer.end();
    let bytes: Arc<[u8]> = writer.finish().into();
    let mut publisher = Publisher::new(0, 4096);
    for initial in [true, false] {
        let output = publisher.receive(bytes.clone(), initial).unwrap();
        assert_eq!(output.len(), 1);
        assert!(Arc::ptr_eq(&output[0], &bytes));
    }
    let malformed: Arc<[u8]> = Arc::from(&b"unknown future wire"[..]);
    for _ in 0..2 {
        assert!(Arc::ptr_eq(
            &publisher.receive(malformed.clone(), false).unwrap()[0],
            &malformed
        ));
    }
    assert!(publisher.set_interval(500).is_err());
}

#[test]
fn mixed_frame_retains_private_revisions_strings_and_values_immediately() {
    let (mut publisher, _) = seeded();
    let mut writer = FrameWriter::new(Vec::new(), 20.);
    writer.begin(section::TICKER_DELTAS);
    writer.f64s([0., 4., 111.]);
    writer.end();
    let value = writer.string("private-control-canary");
    writer.begin_revision(65000, 7);
    writer.u32(value);
    writer.pad();
    writer.end();
    let bytes = writer.finish();
    let immediate = publisher.receive(bytes.into(), false).unwrap();
    assert_eq!(immediate.len(), 1);
    let frame = ingest::parse(&immediate[0]).unwrap();
    assert_eq!(frame.sections.len(), 1);
    assert_eq!(
        (frame.sections[0].kind, frame.sections[0].revision),
        (65000, 7)
    );
    let reference = Cursor::new(frame.sections[0].payload).u32().unwrap();
    assert_eq!(
        frame.strings.required(reference).unwrap(),
        "private-control-canary"
    );
    let public = publisher.flush().unwrap();
    assert_eq!(public.len(), 1);
    assert_eq!(kinds(&public[0]), [section::TICKER_DELTAS]);
    assert!(!String::from_utf8_lossy(&public[0]).contains("private-control-canary"));
}

#[test]
fn observations_and_full_ticker_reordering_keep_the_latest_row_identity() {
    let (mut publisher, mut source) = seeded();
    let mut writer = FrameWriter::new(Vec::new(), 20.);
    writer.begin(section::TICKER_OBSERVATIONS);
    writer.f64s([0., 100., 101., 102.]);
    writer.end();
    assert!(
        publisher
            .receive(writer.finish().into(), false)
            .unwrap()
            .is_empty()
    );
    source.set_tickers(vec![ticker("ETHUSDT", 222.), ticker("BTCUSDT", 111.)]);
    publisher.receive(encode(&mut source), false).unwrap();
    let mut writer = FrameWriter::new(Vec::new(), 30.);
    writer.begin(section::TICKER_OBSERVATIONS);
    writer.f64s([1., 300., 301., 302.]);
    writer.end();
    publisher.receive(writer.finish().into(), false).unwrap();
    let frames = publisher.flush().unwrap();
    assert_eq!(frames.len(), 1);
    let frame = ingest::parse(&frames[0]).unwrap();
    let mut view = Store::default();
    let mut state = IngestState::default();
    ingest::apply(&mut view, &mut state, &frame, false).unwrap();
    assert_eq!(state.source_tickers, ["ETHUSDT", "BTCUSDT"]);
    assert_eq!(
        view.tickers().iter().map(|t| t.last).collect::<Vec<_>>(),
        [222., 111.]
    );
    let observations = frame
        .sections
        .iter()
        .find(|s| s.kind == section::TICKER_OBSERVATIONS)
        .unwrap();
    assert_eq!(observations.payload.len(), 32);
    let mut cursor = Cursor::new(observations.payload);
    assert_eq!(
        [
            cursor.f64().unwrap(),
            cursor.f64().unwrap(),
            cursor.f64().unwrap(),
            cursor.f64().unwrap()
        ],
        [1., 300., 301., 302.]
    );
}

#[test]
fn readiness_follows_catalogue_and_public_rows_without_holding_private_sections() {
    let mut publisher = Publisher::new(5000, 16 * 1024 * 1024);
    let mut initial = FrameWriter::new(Vec::new(), 0.);
    initial.begin(section::STATUS);
    initial.u32s([0, 0]);
    initial.f64(0.);
    initial.end();
    publisher.receive(initial.finish().into(), true).unwrap();
    let mut source = Store::default();
    source.set_markets(vec![market("BTCUSDT")]);
    source.set_tickers(vec![ticker("BTCUSDT", 100.)]);
    let pending = source.take_pending();
    let mut writer = FrameWriter::new(Vec::new(), 1.);
    source.write(&pending, &mut writer);
    let mut book = Book::default();
    book.apply(BookSide::Bids, 99., 1.);
    book_output::write(&book, 0, &mut writer);
    writer.begin_revision(65000, 1);
    writer.u32s([123, 0]);
    writer.end();
    let frames = publisher.receive(writer.finish().into(), false).unwrap();
    assert_eq!(frames.len(), 3);
    assert!(kinds(&frames[0]).contains(&section::MARKETS));
    assert!(kinds(&frames[0]).contains(&65000));
    assert!(!kinds(&frames[0]).contains(&section::STATUS));
    assert!(kinds(&frames[1]).contains(&section::TICKERS));
    assert!(kinds(&frames[1]).contains(&section::BOOK));
    assert!(kinds(&frames[2]).contains(&section::STATUS));
    let mut view = Store::default();
    let mut state = IngestState::default();
    for bytes in &frames {
        ingest::apply(&mut view, &mut state, &ingest::parse(bytes).unwrap(), false).unwrap();
    }
    assert_eq!(state.source_markets, ["BTCUSDT"]);
    assert_eq!(view.ticker("BTCUSDT").unwrap().last, 100.);
    assert!(publisher.deadline().is_none());
}

#[test]
fn book_baselines_follow_symbols_when_the_market_table_moves() {
    let (mut publisher, mut source) = seeded();
    let mut book = Book::default();
    book.apply(BookSide::Bids, 99., 1.);
    let mut writer = FrameWriter::new(Vec::new(), 20.);
    book_output::write(&book, 0, &mut writer);
    publisher.receive(writer.finish().into(), false).unwrap();
    publisher.flush().unwrap();
    source.set_markets(vec![market("ETHUSDT"), market("BTCUSDT")]);
    publisher.receive(encode(&mut source), false).unwrap();
    book.apply(BookSide::Bids, 99., 0.);
    book.apply(BookSide::Bids, 98., 2.);
    let mut writer = FrameWriter::new(Vec::new(), 30.);
    book_output::write(&book, 1, &mut writer);
    publisher.receive(writer.finish().into(), false).unwrap();
    let frames = publisher.flush().unwrap();
    let frame = ingest::parse(&frames[0]).unwrap();
    let part = frame
        .sections
        .iter()
        .find(|s| s.kind == section::BOOK)
        .unwrap();
    let mut cursor = Cursor::new(part.payload);
    assert_eq!(cursor.u32().unwrap(), 1);
    assert_eq!(cursor.u32().unwrap(), 1);
    assert_eq!(cursor.u32().unwrap(), 0);
    cursor.u32().unwrap();
    assert_eq!(cursor.f64().unwrap(), 98.);
    assert_eq!(cursor.f64().unwrap(), 2.);
}

#[test]
fn split_keeps_string_tables_and_refuses_a_single_oversized_section() {
    let mut writer = FrameWriter::new(Vec::new(), 0.);
    let reference = writer.string("unchanged-reference");
    for kind in [65000, 65001] {
        writer.begin(kind);
        writer.u32s([reference, 0]);
        writer.end();
    }
    let bytes = writer.finish();
    let limit = bytes.len() - 16;
    let frames = split(bytes.clone(), limit).unwrap();
    assert_eq!(frames.len(), 2);
    for bytes in frames {
        assert!(bytes.len() <= limit);
        let frame = ingest::parse(&bytes).unwrap();
        assert_eq!(frame.sections.len(), 1);
        assert_eq!(
            frame
                .strings
                .required(Cursor::new(frame.sections[0].payload).u32().unwrap())
                .unwrap(),
            "unchanged-reference"
        );
    }
    assert!(split(bytes, 32).is_err());
}
