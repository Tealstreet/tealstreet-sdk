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

fn selection(symbols: &[&str]) -> TickerSelection {
    Some(Arc::new(symbols.iter().map(|s| (*s).to_owned()).collect()))
}
fn apply_all(view: &mut Store, state: &mut IngestState, frames: &[Arc<[u8]>]) {
    for bytes in frames {
        ingest::apply(view, state, &ingest::parse(bytes).unwrap(), false).unwrap();
    }
}
fn lasts(view: &Store) -> Vec<(String, f64)> {
    view.tickers()
        .iter()
        .map(|t| (t.symbol.clone(), t.last))
        .collect()
}

#[test]
fn requested_tickers_parse_null_lists_and_refuse_everything_else() {
    use serde_json::json;
    assert_eq!(
        RequestedTickers::parse(&json!(null)),
        RequestedTickers::Absent
    );
    assert_eq!(
        RequestedTickers::parse(&json!(["ETHUSDT", "BTCUSDT", "ETHUSDT"])).selection(),
        Some(selection(&["BTCUSDT", "ETHUSDT"]))
    );
    assert_eq!(
        RequestedTickers::parse(&json!([])).selection(),
        Some(selection(&[]))
    );
    for value in [
        json!("BTCUSDT"),
        json!(1),
        json!({}),
        json!([null]),
        json!([""]),
        json!(["x".repeat(MAX_TICKER_SYMBOL_BYTES + 1)]),
        json!(vec!["BTCUSDT"; MAX_TICKER_SYMBOLS + 1]),
    ] {
        assert_eq!(RequestedTickers::parse(&value).selection(), None, "{value}");
    }
}

#[test]
fn paced_selection_replaces_the_table_and_remaps_deltas_and_observations() {
    let (mut publisher, mut source) = seeded();
    let mut view = Store::default();
    let mut state = IngestState::default();
    let frames = publisher.set_tickers(selection(&["ETHUSDT"])).unwrap();
    assert_eq!(frames.len(), 1);
    apply_all(&mut view, &mut state, &frames);
    assert_eq!(lasts(&view), [("ETHUSDT".to_owned(), 200.)]);
    source.update_ticker("BTCUSDT", |t| t.last = 101.);
    source.update_ticker("ETHUSDT", |t| t.last = 201.);
    publisher.receive(encode(&mut source), false).unwrap();
    let mut writer = FrameWriter::new(Vec::new(), 20.);
    writer.begin(section::TICKER_OBSERVATIONS);
    writer.f64s([0., 1., 2., 3.]);
    writer.f64s([1., 4., 5., 6.]);
    writer.end();
    publisher.receive(writer.finish().into(), false).unwrap();
    let frames = publisher.flush().unwrap();
    assert_eq!(frames.len(), 1);
    let frame = ingest::parse(&frames[0]).unwrap();
    let delta = frame
        .sections
        .iter()
        .find(|s| s.kind == section::TICKER_DELTAS)
        .unwrap();
    let mut cursor = Cursor::new(delta.payload);
    assert_eq!([cursor.f64().unwrap(), cursor.f64().unwrap()], [0., 4.]);
    assert_eq!(cursor.f64().unwrap(), 201.);
    assert_eq!(cursor.at, delta.payload.len(), "unselected delta escaped");
    let observations = frame
        .sections
        .iter()
        .find(|s| s.kind == section::TICKER_OBSERVATIONS)
        .unwrap();
    assert_eq!(observations.payload.len(), 32);
    assert_eq!(Cursor::new(observations.payload).f64().unwrap(), 0.);
    apply_all(&mut view, &mut state, &frames);
    assert_eq!(lasts(&view), [("ETHUSDT".to_owned(), 201.)]);
}

#[test]
fn unpaced_selection_strips_unselected_rows_and_keeps_other_frames_byte_identical() {
    let mut publisher = Publisher::new(0, 16 * 1024 * 1024);
    assert!(
        publisher
            .set_tickers(selection(&["BTCUSDT"]))
            .unwrap()
            .is_empty(),
        "nothing may precede the snapshot"
    );
    let mut source = Store::default();
    source.set_markets(vec![market("BTCUSDT"), market("ETHUSDT")]);
    source.set_tickers(vec![ticker("BTCUSDT", 100.), ticker("ETHUSDT", 200.)]);
    let pending = source.take_pending();
    let mut writer = FrameWriter::new(Vec::new(), 1.);
    source.write(&pending, &mut writer);
    writer.begin(section::STATUS);
    writer.u32s([0, 0]);
    writer.f64(0.);
    writer.end();
    let frames = publisher.receive(writer.finish().into(), true).unwrap();
    assert_eq!(frames.len(), 3);
    assert!(kinds(&frames[0]).contains(&section::MARKETS));
    assert!(!kinds(&frames[0]).iter().any(|k| ticker_section(*k)));
    let data_kinds = |bytes: &[u8]| {
        kinds(bytes)
            .into_iter()
            .filter(|kind| *kind != section::STRINGS)
            .collect::<Vec<_>>()
    };
    assert_eq!(data_kinds(&frames[1]), [section::TICKERS]);
    let status = data_kinds(&frames[2]);
    assert!(!status.is_empty() && status.iter().all(|kind| *kind == section::STATUS));
    let mut view = Store::default();
    let mut state = IngestState::default();
    apply_all(&mut view, &mut state, &frames);
    assert_eq!(lasts(&view), [("BTCUSDT".to_owned(), 100.)]);

    source.update_ticker("ETHUSDT", |t| t.last = 201.);
    assert!(
        publisher
            .receive(encode(&mut source), false)
            .unwrap()
            .is_empty()
    );
    source.update_ticker("BTCUSDT", |t| t.last = 101.);
    let frames = publisher.receive(encode(&mut source), false).unwrap();
    apply_all(&mut view, &mut state, &frames);
    assert_eq!(lasts(&view), [("BTCUSDT".to_owned(), 101.)]);

    let mut book = Book::default();
    book.apply(BookSide::Bids, 99., 1.);
    let mut writer = FrameWriter::new(Vec::new(), 2.);
    book_output::write(&book, 0, &mut writer);
    let bytes: Arc<[u8]> = writer.finish().into();
    let output = publisher.receive(bytes.clone(), false).unwrap();
    assert!(Arc::ptr_eq(&output[0], &bytes));

    // Clearing the selection sends the whole table once, then source deltas pass through.
    let frames = publisher.set_tickers(None).unwrap();
    assert_eq!(frames.len(), 1);
    apply_all(&mut view, &mut state, &frames);
    assert_eq!(
        lasts(&view),
        [("BTCUSDT".to_owned(), 101.), ("ETHUSDT".to_owned(), 201.)]
    );
    source.update_ticker("ETHUSDT", |t| t.last = 202.);
    let bytes = encode(&mut source);
    let output = publisher.receive(bytes.clone(), false).unwrap();
    assert!(Arc::ptr_eq(&output[0], &bytes));
    apply_all(&mut view, &mut state, &output);
    assert_eq!(view.ticker("ETHUSDT").unwrap().last, 202.);
}

#[test]
fn a_catalogue_reload_rebuilds_the_selected_rows_in_source_order() {
    let (mut publisher, mut source) = seeded();
    publisher
        .set_tickers(selection(&["ETHUSDT", "SOLUSDT"]))
        .unwrap();
    source.set_tickers(vec![
        ticker("SOLUSDT", 300.),
        ticker("BTCUSDT", 101.),
        ticker("ETHUSDT", 201.),
    ]);
    publisher.receive(encode(&mut source), false).unwrap();
    let frames = publisher.flush().unwrap();
    let mut view = Store::default();
    let mut state = IngestState::default();
    apply_all(&mut view, &mut state, &frames);
    assert_eq!(
        lasts(&view),
        [("SOLUSDT".to_owned(), 300.), ("ETHUSDT".to_owned(), 201.)]
    );
    source.update_ticker("ETHUSDT", |t| t.last = 202.);
    publisher.receive(encode(&mut source), false).unwrap();
    apply_all(&mut view, &mut state, &publisher.flush().unwrap());
    assert_eq!(view.ticker("ETHUSDT").unwrap().last, 202.);
}

#[test]
fn an_unchanged_selection_publishes_nothing() {
    let (mut publisher, _) = seeded();
    assert_eq!(
        publisher
            .set_tickers(selection(&["BTCUSDT"]))
            .unwrap()
            .len(),
        1
    );
    assert!(
        publisher
            .set_tickers(selection(&["BTCUSDT"]))
            .unwrap()
            .is_empty()
    );
    assert!(publisher.set_tickers(None).unwrap().len() == 1);
    assert!(publisher.set_tickers(None).unwrap().is_empty());
}

/// AGT-107 measurement: `cargo test -p safe-cex-exchange-server --lib --release --
/// publication::tests::measure_twelve_accounts --ignored --nocapture`.
#[test]
#[ignore = "measurement harness"]
fn measure_twelve_accounts() {
    const ACCOUNTS: usize = 12;
    const TICKERS: usize = 500;
    const SOURCE_FRAMES_PER_SECOND: usize = 10;
    const SECONDS: usize = 30;
    let symbols: Vec<String> = (0..TICKERS).map(|i| format!("SYM{i}USDT")).collect();
    let selected: Vec<&str> = symbols.iter().take(20).map(String::as_str).collect();
    for (label, interval) in [("unpaced", 0u16), ("paced 500ms", 500)] {
        for selection_label in ["full table", "20 selected"] {
            let mut source = Store::default();
            source.set_markets(symbols.iter().map(|s| market(s)).collect());
            source.set_tickers(
                symbols
                    .iter()
                    .enumerate()
                    .map(|(i, s)| ticker(s, 100. + i as f64))
                    .collect(),
            );
            let mut publishers: Vec<Publisher> = (0..ACCOUNTS)
                .map(|_| {
                    let mut publisher = Publisher::new(interval, 16 * 1024 * 1024);
                    if selection_label != "full table" {
                        publisher.set_tickers(selection(&selected)).unwrap();
                    }
                    publisher
                })
                .collect();
            let snapshot = encode(&mut source);
            let mut views: Vec<(Store, IngestState)> =
                (0..ACCOUNTS).map(|_| Default::default()).collect();
            for (publisher, (view, state)) in publishers.iter_mut().zip(&mut views) {
                apply_all(
                    view,
                    state,
                    &publisher.receive(snapshot.clone(), true).unwrap(),
                );
            }
            let (mut frames, mut bytes) = (0usize, 0usize);
            let mut server = std::time::Duration::ZERO;
            let mut client = std::time::Duration::ZERO;
            let mut seed = 7u64;
            let step_ms = 1000 / SOURCE_FRAMES_PER_SECOND;
            for step in 0..SECONDS * SOURCE_FRAMES_PER_SECOND {
                for row in 0..TICKERS {
                    seed = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    if (seed >> 33) % 10 < 6 {
                        let delta = ((seed >> 40) % 100) as f64 / 100.;
                        source.update_ticker_at(row as u32, |t| {
                            t.last += delta;
                            t.mark += delta;
                        });
                    }
                }
                let bytes_in = encode(&mut source);
                let due = interval > 0 && (step * step_ms) % interval as usize == 0;
                for (publisher, (view, state)) in publishers.iter_mut().zip(&mut views) {
                    let started = std::time::Instant::now();
                    let mut output = publisher.receive(bytes_in.clone(), false).unwrap();
                    if due {
                        output.extend(publisher.flush().unwrap());
                    }
                    server += started.elapsed();
                    frames += output.len();
                    bytes += output.iter().map(|frame| frame.len()).sum::<usize>();
                    let started = std::time::Instant::now();
                    apply_all(view, state, &output);
                    client += started.elapsed();
                }
            }
            let expected = if selection_label == "full table" {
                TICKERS
            } else {
                selected.len()
            };
            assert!(
                views
                    .iter()
                    .all(|(view, _)| view.tickers().len() == expected)
            );
            let seconds = SECONDS as f64;
            println!(
                "{label:>11} | {selection_label:>11} | {ACCOUNTS} accounts: {:>6.1} frames/s, {:>9.0} B/s ({:>6.1} KB/s); server publish {:>6.1} µs/s; client ingest {:>7.1} µs/s",
                frames as f64 / seconds,
                bytes as f64 / seconds,
                bytes as f64 / seconds / 1024.,
                server.as_secs_f64() * 1e6 / seconds,
                client.as_secs_f64() * 1e6 / seconds,
            );
        }
    }
}
