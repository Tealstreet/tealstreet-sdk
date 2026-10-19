//! The Exchange trait reaches every member KrakenExchange / KrakenSpotExchange override.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::{
    exchange::Exchange,
    journal::{HistoricalPositionOptions, JournalFolder},
    runtime::ExchangeFuture,
    websocket::WsEvent,
};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{
    Candle, ExchangeName, OHLCVOptions, Order, OrderBook, ws_auth::WsAuth, ws_auth::WsAuthRequest,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use support::{Extras, harness, harness_with, options};

const SYMBOL: &str = "PF_XBTUSD";
const CHARTS: &str = "https://futures.kraken.com/api/charts/v1/trade/PF_XBTUSD/1h";

fn ohlcv(limit: Option<f64>) -> OHLCVOptions {
    let mut options = json!({"symbol": SYMBOL, "interval": "1h"});
    if let Some(limit) = limit {
        options["limit"] = json!(limit);
    }
    serde_json::from_value(options).unwrap()
}

fn candles() -> Value {
    json!({"candles": [
        {"time": 1_700_003_600_000_i64, "open": "2", "high": "4", "low": "1", "close": "3", "volume": "7"},
        {"time": 1_700_000_000_000_i64, "open": "1", "high": "2", "low": "0.5", "close": "2", "volume": "5"}
    ]})
}

#[test]
fn futures_fetch_ohlcv_reads_the_charts_api() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", CHARTS, candles())
    });
    let candles = h.drive(h.venue().fetch_ohlcv(ohlcv(Some(2.0)))).unwrap();
    assert_eq!(
        h.http.calls(CHARTS).len(),
        1,
        "Kraken fetchOHLCV must reach charts"
    );
    assert_eq!(candles.len(), 2);
    assert_eq!(candles[0].timestamp, 1_700_000_000.0);
    assert_eq!(candles[1].close, 3.0);
}

#[test]
fn futures_listen_ohlcv_polls_rest_and_delivers_candles() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", CHARTS, candles())
    });
    let seen: Rc<RefCell<Vec<Candle>>> = Rc::default();
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(ohlcv(None), Rc::new(move |c| sink.borrow_mut().push(c)))
        .unwrap();
    h.runtime.pump();
    h.runtime.advance(50.0);
    assert!(
        !seen.borrow().is_empty(),
        "Kraken listenOHLCV must poll and deliver candles"
    );
    assert_eq!(seen.borrow()[0].close, 3.0);
    cleanup().unwrap();
}

#[test]
fn futures_listen_order_book_connects_subscribes_and_delivers_snapshots() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let books: Rc<RefCell<Vec<OrderBook>>> = Rc::default();
    let sink = books.clone();
    let _cleanup = h
        .venue()
        .listen_order_book(SYMBOL.into(), Rc::new(move |b| sink.borrow_mut().push(b)))
        .unwrap();
    for _ in 0..10 {
        h.runtime.advance(100.0);
    }
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url == "wss://futures.kraken.com/ws/v1")
        .map(|(_, socket)| socket.clone())
        .expect("Kraken listenOrderBook must connect the public socket");
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"book\"") && m.contains(SYMBOL)),
        "book subscription sent: {:?}",
        socket.sent.borrow()
    );
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"feed": "book_snapshot", "product_id": SYMBOL, "timestamp": 1, "seq": 1,
            "bids": [{"price": 30000, "qty": 2}], "asks": [{"price": 30001, "qty": 3}]})
            .to_string(),
        )));
    for _ in 0..10 {
        h.runtime.advance(100.0);
    }
    let books = books.borrow();
    assert!(!books.is_empty(), "snapshot reaches the callback");
    assert_eq!(books[0].bids[0].price, 30000.0);
    assert_eq!(books[0].asks[0].amount, 3.0);
}

#[test]
fn futures_income_range_reads_the_account_log() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", "/history/v3/account-log", json!({"logs": []}))
    });
    let records = h
        .drive(
            h.venue()
                .fetch_income_history_range(1.0e12, 1.0e12 + 1000.0, None),
        )
        .unwrap();
    assert!(records.is_empty());
    assert!(
        !h.http.calls("/history/v3/account-log").is_empty(),
        "Kraken fetchIncomeHistoryRange must read the account log"
    );
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", "/history/v3/account-log", json!({"logs": []}))
    });
    h.drive(h.venue().fetch_income_history(None)).unwrap();
    assert!(
        !h.http.calls("/history/v3/account-log").is_empty(),
        "Kraken fetchIncomeHistory must read the account log"
    );
}

#[derive(Default)]
struct Folder(RefCell<Vec<(String, bool)>>);
impl JournalFolder for Folder {
    fn build_folded_position_records<'a>(
        &'a self,
        _: &'a dyn Exchange,
        account: &'a str,
        _: f64,
        _: f64,
        symbol_scoped: bool,
        _: Box<dyn Fn(&str) -> Result<String, safe_cex_exchange::runtime::ExchangeError>>,
    ) -> ExchangeFuture<'a, Vec<Value>> {
        self.0.borrow_mut().push((account.into(), symbol_scoped));
        Box::pin(async { Ok(vec![json!({"folded": true})]) })
    }
}

#[test]
fn futures_historical_positions_fold_from_fills_and_spot_refuses() {
    let folder = Rc::new(Folder::default());
    let h = harness_with(
        ExchangeName::Kraken,
        options(),
        Extras {
            journal: Some(folder.clone()),
            ..Extras::default()
        },
        |_| {},
    );
    let records = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "acct".into(),
                    from: Some(1.0),
                    to: Some(2.0),
                }),
        )
        .unwrap();
    assert_eq!(records, vec![json!({"folded": true})]);
    assert_eq!(*folder.0.borrow(), vec![("acct".to_owned(), false)]);

    let spot = harness(ExchangeName::Krakenspot, options(), |_| {});
    let error = spot
        .drive(
            spot.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "acct".into(),
                    from: None,
                    to: None,
                }),
        )
        .unwrap_err();
    assert!(
        error
            .message
            .starts_with("KrakenSpotExchange: no journal records path"),
        "{}",
        error.message
    );
}

fn stale_order(symbol: &str) -> Order {
    serde_json::from_value(json!({
        "id": format!("stale-{symbol}"), "status": "open", "symbol": symbol, "type": "limit",
        "side": "buy", "price": 1, "amount": 1, "filled": 0, "remaining": 1, "reduceOnly": false, "lastUpdate": 1
    }))
    .unwrap()
}

#[test]
fn spot_fetch_and_set_orders_replaces_the_full_set() {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "POST",
            "/0/private/OpenOrders",
            json!({"error": [], "result": {"open": {}}}),
        )
    });
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            orders: Some(vec![stale_order("XBT/USD"), stale_order("ETH/USD")]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.drive(h.venue().fetch_and_set_orders(Some(vec!["XBT/USD".into()])))
        .unwrap();
    assert!(
        safe_cex_store::Store::orders(h.store.as_ref())
            .unwrap()
            .is_empty(),
        "Kraken spot fetchAndSetOrders must replace every order"
    );
}

#[test]
fn futures_sign_ws_auth_uses_the_kraken_challenge_scheme() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let request = WsAuthRequest {
        id: Some(json!("r-1")),
        ws_auth: Some(WsAuth {
            to_sign: Some("challenge-uuid".into()),
            output_format: None,
            extra_data: None,
        }),
        request_type: None,
    };
    let signed = h.drive(h.venue().sign_ws_auth(&request)).unwrap();
    let expected = safe_cex_crypto::sign_kraken_ws_auth(&request, "c2VjcmV0LWJ5dGVz");
    assert!(expected.ws_auth_signature.is_some());
    assert_eq!(
        signed, expected,
        "Kraken signWsAuth must sign the challenge"
    );
}
