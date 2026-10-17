//! The actual constructed venue's socket consumes maintained-source iterable updates.
#[allow(dead_code)]
mod fx {
    use safe_cex_venue_kucoin as venue_crate;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/kucoin-io-fixture.rs"
    ));
}
use safe_cex_exchange::{exchange::Exchange, socket_lifecycle::SocketSlot};
use safe_cex_types::{ExchangeName, OrderBook};
use safe_cex_venue_kucoin::kucoin_exchange::KucoinExchange;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn bits(value: f64) -> String {
    if value.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", value.to_bits())
    }
}
fn observe(book: &OrderBook) -> Value {
    let rows = |levels: &[safe_cex_types::OrderBookOrders]| {
        levels
            .iter()
            .map(|row| [bits(row.price), bits(row.amount), bits(row.total)])
            .collect::<Vec<_>>()
    };
    json!({"bids":rows(&book.bids),"asks":rows(&book.asks)})
}

#[test]
fn native_book_updates_match_source_iteration_for_wire_payloads() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/native_iteration_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 16, "NATIVE-ITERATION source cardinality");
    assert_eq!(
        cases.iter().filter(|row| !row["error"].is_null()).count(),
        6,
        "NATIVE-ITERATION non-iterable source failures"
    );
    for row in cases {
        let (h, parts, deps, _transport) = fx::io::connected_parts(ExchangeName::Kucoin);
        let venue = KucoinExchange::new(parts, deps).unwrap();
        fx::seed_market(
            h.store.as_ref(),
            fx::market("XBTUSDTM", "BTCUSDT", Some(0.5)),
        );
        h.wire.script("POST","/api/v1/bullet-public",json!({"code":"200000","data":{"token":"native-token","instanceServers":[{"endpoint":"wss://native.invalid","pingInterval":18000}]}}));
        h.wire.script(
            "GET",
            "/api/v1/level2/snapshot",
            json!({"code":"200000","data":{"bids":[],"asks":[],"sequence":1}}),
        );
        let socket = venue.base().get_socket(SocketSlot::Public).unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert!(
            socket.is_connected(),
            "NATIVE-ITERATION real socket connection"
        );
        let books = Rc::new(RefCell::new(Vec::new()));
        let received = books.clone();
        let cleanup = venue
            .listen_order_book(
                "BTCUSDT".into(),
                Rc::new(move |book| received.borrow_mut().push(observe(&book))),
            )
            .unwrap();
        h.runtime.pump();
        h.runtime.pump();
        assert_eq!(books.borrow().len(), 1, "NATIVE-ITERATION initial snapshot");
        let mut message = row["message"].clone();
        message["topic"] = json!("/contractMarket/level2:XBTUSDTM");
        let reported_before = h.runtime.errors.borrow().len();
        socket
            .on_message(&safe_cex_transport::socket::SocketFrame::Text(
                message.to_string(),
            ))
            .unwrap();
        // The actual socket reports listener throws through the runtime hook.
        let reported = h.runtime.errors.borrow().len() - reported_before;
        assert_eq!(
            reported,
            usize::from(!row["error"].is_null()),
            "NATIVE-ITERATION {}/{} reported exception count",
            row["side"],
            row["index"]
        );
        assert_eq!(
            json!(*books.borrow()),
            row["books"],
            "NATIVE-ITERATION {}/{} callback values",
            row["side"],
            row["index"]
        );
        cleanup().unwrap();
        venue.dispose().unwrap();
    }
}
