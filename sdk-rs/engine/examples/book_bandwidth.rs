//! Deterministic serialization measurement, not a network or load test.
//! Optional argument: directory in which to write the two compatible frames.
use safe_cex_engine::Store;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::wire::FrameWriter;
use serde::Serialize;

#[derive(Serialize)]
struct Measurement {
    books: usize,
    levels_per_side: usize,
    changed_levels_per_side_per_book: usize,
    flushes_per_second: usize,
    full_frame_bytes: usize,
    delta_frame_bytes: usize,
    full_bytes_per_second: usize,
    delta_bytes_per_second: usize,
    reduction_percent: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut books = [Book::default(), Book::default()];
    for book in &mut books {
        for level in 0..1000 {
            book.apply(BookSide::Bids, 10_000. - level as f64, 1.);
            book.apply(BookSide::Asks, 10_001. + level as f64, 2.);
        }
    }
    let previous = books.clone();
    for book in &mut books {
        for level in 0..10 {
            book.apply(BookSide::Bids, 10_000. - level as f64, 3.);
            book.apply(BookSide::Asks, 10_001. + level as f64, 4.);
        }
    }
    let mut full = FrameWriter::new(Vec::new(), 250.);
    let mut delta = FrameWriter::new(Vec::new(), 250.);
    for (index, book) in books.iter().enumerate() {
        book.write(index as u32, &mut full);
        book.write_delta(&previous[index], index as u32, &mut delta);
    }
    let (full, delta) = (full.finish(), delta.finish());
    assert_eq!((full.len(), delta.len()), (96_064, 704));
    if let Some(directory) = std::env::args_os().nth(1) {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        // The baseline plus delta must equal full-after.bin on a consumer.
        let mut baseline = FrameWriter::new(Vec::new(), 0.);
        for (index, book) in previous.iter().enumerate() {
            book.write(index as u32, &mut baseline);
        }
        std::fs::write(directory.join("baseline.bin"), baseline.finish())?;
        std::fs::write(directory.join("full-after.bin"), &full)?;
        std::fs::write(directory.join("delta.bin"), &delta)?;
        let mut store = Store::default();
        store.set_markets(
            [("BTCUSDT", "BTC"), ("ETHUSDT", "ETH")]
                .into_iter()
                .map(|(symbol, base)| Market {
                    id: symbol.into(),
                    symbol: symbol.into(),
                    pretty_symbol: None,
                    base: base.into(),
                    quote: "USDT".into(),
                    active: true,
                    precision: Precision {
                        amount: 0.001,
                        price: 0.1,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: 0.001,
                            max: 1000.,
                        },
                        leverage: MinMax { min: 1., max: 100. },
                        price: None,
                    },
                    contract_size: None,
                    is_expiring: None,
                    is_inverse: None,
                    is_spot: None,
                    is_perpetual: Some(true),
                    is_read_only: None,
                    taker_fee: None,
                    maker_fee: None,
                })
                .collect(),
        );
        let mut markets = FrameWriter::new(Vec::new(), 0.);
        store.write(&store.everything(), &mut markets);
        std::fs::write(directory.join("markets.bin"), markets.finish())?;
    }
    println!(
        "{}",
        serde_json::to_string(&Measurement {
            books: 2,
            levels_per_side: 1000,
            changed_levels_per_side_per_book: 10,
            flushes_per_second: 4,
            full_frame_bytes: full.len(),
            delta_frame_bytes: delta.len(),
            full_bytes_per_second: full.len() * 4,
            delta_bytes_per_second: delta.len() * 4,
            reduction_percent: (1. - delta.len() as f64 / full.len() as f64) * 100.,
        })?
    );
    Ok(())
}
