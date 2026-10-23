//! Diagnostic wall-time benchmark; CI replay medians remain the performance gate.
use safe_cex_engine::{Store, model::Ticker};
use std::{collections::BTreeSet, hint::black_box, time::Instant};

fn main() {
    let symbols: Vec<String> = match std::env::args().nth(1) {
        Some(path) => serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap(),
        None => (0..968).map(|i| format!("ASSET{i:04}-QUOTE")).collect(),
    };
    assert!(!symbols.is_empty());
    let unique = symbols.iter().collect::<BTreeSet<_>>();
    let mut store = Store::default();
    store.set_tickers(
        unique
            .iter()
            .enumerate()
            .map(|(i, symbol)| Ticker {
                id: (*symbol).clone(),
                symbol: (*symbol).clone(),
                last: i as f64 + 1.,
                ..Ticker::default()
            })
            .collect(),
    );
    let lookups = 2_500_000;
    let mut checksum = 0.;
    let start = Instant::now();
    for symbol in symbols.iter().cycle().take(lookups) {
        checksum += black_box(&store).ticker(black_box(symbol)).unwrap().last;
    }
    let elapsed = start.elapsed();
    println!(
        "symbols={} lookups={lookups} checksum={checksum} elapsed_ns={} ns_per_lookup={:.3}",
        unique.len(),
        elapsed.as_nanos(),
        elapsed.as_nanos() as f64 / lookups as f64,
    );
    black_box(checksum);
}
