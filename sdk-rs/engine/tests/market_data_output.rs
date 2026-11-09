pub use safe_cex_engine::{book, model, wire};
#[path = "../src/book_output.rs"]
mod book_output;
#[path = "../src/candle_output.rs"]
mod candle_output;

use book::{Book, BookSide};
use model::Candle;
use wire::FrameWriter;

// Captured from the pre-slice writers at de0744043; never regenerate from the new modules.
fn assert_golden(name: &str, bytes: Vec<u8>) {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/market-data-output.json")).unwrap();
    assert_eq!(serde_json::to_value(bytes).unwrap(), golden[name]);
}

#[test]
fn native_book_output_preserves_existing_snapshot_and_delta_bytes() {
    let mut book = Book::default();
    book.apply(BookSide::Bids, 0.0, 2.0);
    book.apply(BookSide::Bids, 1.0, 3.0);
    book.apply(BookSide::Asks, 2.0, 4.0);
    let previous = book.clone();
    book.apply(BookSide::Bids, 1.0, 0.0);
    book.apply(BookSide::Asks, 2.0, 5.0);
    let mut new = FrameWriter::new(Vec::new(), 42.0);
    book_output::write(&book, 7, &mut new);
    assert_golden("snapshot", new.finish());
    let mut new = FrameWriter::new(Vec::new(), 42.0);
    assert!(book_output::write_delta(&book, &previous, 7, &mut new));
    assert_golden("delta", new.finish());
    let mut empty = FrameWriter::new(Vec::new(), 42.0);
    assert!(!book_output::write_delta(&book, &book, 7, &mut empty));
    assert!(empty.is_empty());
}

#[test]
fn native_candle_output_preserves_existing_bytes_and_milliseconds() {
    let rows = [(
        7,
        "1m",
        Candle {
            timestamp: 1_700_000_040_000.0,
            open: 0.0,
            high: 2.0,
            low: 0.0,
            close: 1.0,
            volume: 0.0,
        },
    )];
    let mut new = FrameWriter::new(Vec::new(), 42.0);
    candle_output::write(&mut new, &rows);
    assert_golden("candles", new.finish());
    let mut empty = FrameWriter::new(Vec::new(), 42.0);
    candle_output::write(&mut empty, &[]);
    assert!(empty.is_empty());
}
