//! Comparator failures through the actual constructed public receiver.
#[allow(dead_code, reason = "shared fixture supplies other exchange workflows")]
mod support {
    use crate as safe_cex_venue_paradex;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));
}
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, Thrown},
};
use std::{cell::Cell, rc::Rc};

#[test]
fn constructed_book_sort_stops_at_first_throw() {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/book-sort-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let observation: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        observation,
        serde_json::json!({"reads": 1, "books": 2, "levels": 22})
    );
    let (venue, runtime, store, _) = support::fixture_with(
        serde_json::json!({"key":"", "secret":"", "publicDataOnly":true}),
        |_| {},
    );
    let deliveries = Rc::new(Cell::new(0));
    let sink = deliveries.clone();
    let cleanup = venue
        .listen_order_book(
            "BTC-USD-PERP".into(),
            Rc::new(move |_| sink.set(sink.get() + 1)),
        )
        .unwrap();
    let reads = Rc::new(Cell::new(0));
    let levels = (0..32)
        .map(|_| {
            let level = Js::object(vec![("side", Js::str("BUY")), ("size", Js::str("2"))]);
            let reads = reads.clone();
            level
                .define_accessor(
                    "price",
                    Some(Rc::new(move |_| {
                        reads.set(reads.get() + 1);
                        Err(Thrown::type_error(&format!("sort getter {}", reads.get())))
                    })),
                    None,
                )
                .unwrap();
            level
        })
        .chain(std::iter::once(Js::object(vec![
            ("side", Js::str("SELL")),
            ("price", Js::str("102")),
            ("size", Js::str("3")),
        ])))
        .collect();
    let socket = venue.rt.this.get("publicWebsocket").unwrap();
    socket
        .call_method(
            "handleOrderbookUpdate",
            vec![Js::object(vec![(
                "data",
                Js::object(vec![
                    ("market", Js::str("BTC-USD-PERP")),
                    ("inserts", Js::array(levels)),
                ]),
            )])],
        )
        .unwrap();
    assert_eq!(
        reads.get(),
        1,
        "JS sort stops at the first comparator error"
    );
    assert!(
        runtime
            .console
            .borrow()
            .iter()
            .any(|line| line.contains("sort getter 1"))
    );
    assert_eq!(deliveries.get(), 0, "failed sorting never publishes a book");
    assert!(
        store.tickers().unwrap().is_empty(),
        "failed sorting never updates a ticker"
    );
    cleanup().unwrap();
    venue.dispose().unwrap();
}
