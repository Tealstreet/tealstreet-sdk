pub use safe_cex_engine::{book, model, wire};
#[path = "../src/book_output.rs"]
mod book_output;
#[path = "../src/candle_output.rs"]
mod candle_output;

use book::{Book, BookSide};
use model::Candle;
use std::collections::BTreeMap;
use wire::{FrameWriter, section};

fn packed(write: impl FnOnce(&mut FrameWriter<'_>)) -> Vec<u8> {
    let mut writer = FrameWriter::new(Vec::new(), 1_700_000_040_000.0);
    write(&mut writer);
    writer.finish()
}

fn candle(volume: f64) -> Candle {
    Candle {
        timestamp: 1_700_000_040_000.0,
        open: 0.0,
        high: 2.0,
        low: 0.0,
        close: 1.0,
        volume,
    }
}

fn market(writer: &mut FrameWriter<'_>) {
    let strings = ["zero", "ZERO/USD", "ZERO", "USD"].map(|s| writer.string(s));
    writer.begin(section::MARKETS);
    writer.u32s([1, strings[0], strings[1], strings[2], strings[3], 1]);
    writer.pad();
    writer.f64s([
        0.001,
        0.01,
        0.001,
        100.0,
        1.0,
        100.0,
        f64::NAN,
        1.0,
        0.0,
        0.0,
    ]);
    writer.end();
}

fn main() {
    let mut initial = Book::default();
    initial.load_sorted(BookSide::Bids, [(2.0, 1.0), (0.0, 2.0)]);
    initial.load_sorted(BookSide::Asks, [(3.0, 4.0)]);
    let mut current = initial.clone();
    current.apply(BookSide::Bids, 2.0, 0.0);
    current.apply(BookSide::Bids, 1.0, 5.0);
    current.apply(BookSide::Asks, 3.0, 0.0);
    current.apply(BookSide::Asks, 0.0, 3.0);
    let mut frames = BTreeMap::new();
    frames.insert(
        "snapshot",
        packed(|writer| {
            market(writer);
            book_output::write(&initial, 0, writer);
            candle_output::write(writer, &[(0, "1m", candle(0.0))]);
        }),
    );
    frames.insert(
        "bookOnly",
        packed(|writer| {
            market(writer);
            book_output::write(&initial, 0, writer);
        }),
    );
    frames.insert(
        "candleOnly",
        packed(|writer| {
            market(writer);
            candle_output::write(writer, &[(0, "1m", candle(0.0))]);
        }),
    );
    frames.insert(
        "bookUpdate",
        packed(|writer| book_output::write(&initial, 0, writer)),
    );
    frames.insert(
        "candleUpdate",
        packed(|writer| candle_output::write(writer, &[(0, "1m", candle(0.0))])),
    );
    frames.insert(
        "delta",
        packed(|writer| {
            assert!(book_output::write_delta(&current, &initial, 0, writer));
        }),
    );
    frames.insert(
        "empty",
        packed(|writer| {
            assert!(book_output::write_delta(
                &Book::default(),
                &current,
                0,
                writer
            ));
        }),
    );
    frames.insert(
        "idle",
        packed(|writer| {
            assert!(!book_output::write_delta(&current, &current, 0, writer));
        }),
    );
    for (name, levels) in [
        ("invalid", vec![(f64::NAN, 1.0)]),
        ("unordered", vec![(0.0, 1.0), (2.0, 1.0)]),
        ("negative", vec![(0.0, -1.0)]),
    ] {
        let mut book = Book::default();
        book.load_sorted(BookSide::Bids, levels);
        frames.insert(name, packed(|writer| book_output::write(&book, 0, writer)));
    }
    frames.insert(
        "invalidTotal",
        packed(|writer| {
            writer.begin(section::BOOK);
            writer.u32s([0, 1, 0, 0]);
            writer.f64s([2.0, 1.0, f64::INFINITY]);
            writer.end();
        }),
    );
    frames.insert(
        "invalidDelta",
        packed(|writer| {
            book_output::write_delta(&current, &initial, 0, writer);
            writer.begin(section::BOOK_DELTA);
            writer.u32s([0, 1, 0, 0]);
            writer.f64s([f64::NAN, 1.0]);
            writer.end();
        }),
    );
    frames.insert(
        "unknownCandle",
        packed(|writer| candle_output::write(writer, &[(17, "1m", candle(0.0))])),
    );
    frames.insert(
        "invalidCandle",
        packed(|writer| candle_output::write(writer, &[(0, "1m", candle(f64::NAN))])),
    );
    println!("{}", serde_json::to_string(&frames).unwrap());
}
