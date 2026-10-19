//! Bitget streams, candles, income and journal through the production Exchange binding.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_exchange::{journal::HistoricalPositionOptions, websocket::WsEvent};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{Candle, ExchangeName, OHLCVOptions, OrderBook};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn market() -> Value {
    json!({"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"100","minLever":"1","maxLever":"125"})
}
fn position(qty: &str) -> Value {
    json!({"symbol":"BTCUSDT","holdSide":"long","total":qty,"openPriceAvg":"50000","markPrice":"50100","marginMode":"crossed","posMode":"one_way_mode","leverage":"10","unrealizedPL":"2.5","liquidationPrice":"40000","achievedProfits":"0","totalFee":"0","deductedFee":"0","marginSize":"50","uTime":if qty=="0.01" {"1700000000000"} else {"1700000000010"}})
}
fn script(http: &Http) {
    http.route(
        "GET",
        "/api/v3/account/settings",
        400.0,
        json!({"code":"40084","msg":"Classic Account mode","requestTime":1,"data":null}),
    );
    http.ok(
        "GET",
        "/api/v2/spot/account/info",
        json!({"userId":"synthetic-user","parentId":null}),
    );
    http.ok("GET", "/api/v2/mix/market/contracts", json!([market()]));
    http.ok("GET","/api/v2/mix/market/tickers",json!([{"symbol":"BTCUSDT","lastPr":"50000","bidPr":"49999","askPr":"50001","indexPrice":"50000.5","changeUtc24h":"0.01","fundingRate":"0.0001","baseVolume":"10","quoteVolume":"500000"}]));
    http.ok("GET","/api/v2/mix/account/accounts",json!([{"marginCoin":"USDT","available":"100","accountEquity":"120","unrealizedPL":"2","locked":"5"}]));
    http.ok(
        "GET",
        "/api/v2/mix/position/all-position",
        json!([position("0.01")]),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-pending",
        json!({"entrustedList":[],"endId":null}),
    );
    http.ok(
        "GET",
        "/api/v2/mix/order/orders-plan-pending",
        json!({"entrustedList":[],"endId":null}),
    );
}
fn started() -> (Harness, Rc<Socket>) {
    let h = harness(ExchangeName::Bitget, options(), |http| {
        script(http);
        http.ok(
            "GET",
            "/api/v2/mix/market/candles",
            json!([["1700000000000", "1", "3", "0.5", "2", "9", "7"]]),
        );
        http.ok("GET", "/api/v2/mix/account/bill", json!({"bills":[{"billId":"b1","businessType":"contract_settle_fee","coin":"USDT","symbol":"BTCUSDT","amount":"-0.25","fee":"0","cTime":"1700000000000"}]}));
        http.ok(
            "GET",
            "/api/v2/mix/order/fills",
            json!({"fillList":[],"endId":null}),
        );
        http.ok(
            "GET",
            "/api/v2/mix/order/fill-history",
            json!({"fillList":[],"endId":null}),
        );
    });
    h.drive(h.venue().start()).unwrap();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url == "wss://ws.bitget.com/v2/ws/public")
        .expect("real public socket")
        .1
        .clone();
    (h, socket)
}
fn push(h: &Harness, socket: &Socket, frame: Value) {
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    h.runtime.pump();
}
fn ohlcv() -> OHLCVOptions {
    serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap()
}

#[test]
fn real_fetch_ohlcv_reads_venue_candles() {
    let (h, _) = started();
    let candles = h.drive(h.venue().fetch_ohlcv(ohlcv())).unwrap();
    assert_eq!(
        serde_json::to_value(&candles).unwrap(),
        json!([{"timestamp":1700000000.0,"open":1.0,"high":3.0,"low":0.5,"close":2.0,"volume":7.0}]),
        "fetchOHLCV must read the real candles route"
    );
    assert_eq!(h.http.calls("/api/v2/mix/market/candles").len(), 1);
}

#[test]
fn real_listen_ohlcv_subscribes_and_delivers_candles() {
    let (h, socket) = started();
    let seen: Rc<RefCell<Vec<Candle>>> = Rc::default();
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(ohlcv(), Rc::new(move |c| sink.borrow_mut().push(c)))
        .unwrap();
    h.runtime.advance(10.0);
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("candle1m") && m.contains("subscribe")),
        "listenOHLCV must subscribe the real public socket: {:?}",
        socket.sent.borrow()
    );
    push(
        &h,
        &socket,
        json!({"action":"update","arg":{"instType":"USDT-FUTURES","channel":"candle1m","instId":"BTCUSDT"},"data":[["1700000060000","2","4","1","3","5","6"]]}),
    );
    assert_eq!(
        seen.borrow().len(),
        1,
        "real candle frame must reach the callback"
    );
    assert_eq!(
        (
            seen.borrow()[0].timestamp,
            seen.borrow()[0].close,
            seen.borrow()[0].volume
        ),
        (1700000060.0, 3.0, 6.0)
    );
    cleanup().unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("unsubscribe") && m.contains("candle1m"))
    );
}

#[test]
fn real_listen_order_book_subscribes_and_delivers_books() {
    let (h, socket) = started();
    let seen: Rc<RefCell<Vec<OrderBook>>> = Rc::default();
    let sink = seen.clone();
    let cleanup = h
        .venue()
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |b| sink.borrow_mut().push(b)),
        )
        .unwrap();
    h.runtime.advance(10.0);
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"books\"") && m.contains("subscribe")),
        "listenOrderBook must subscribe the real public socket: {:?}",
        socket.sent.borrow()
    );
    push(
        &h,
        &socket,
        json!({"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"books","instId":"BTCUSDT"},"data":[{"bids":[["99","2"]],"asks":[["101","3"]],"ts":"1700000000000"}]}),
    );
    h.runtime.advance(1000.0);
    let books = seen.borrow();
    assert!(
        !books.is_empty(),
        "real books frame must reach the callback"
    );
    let last = books.last().unwrap();
    assert_eq!(
        (last.bids[0].price, last.bids[0].amount, last.asks[0].price),
        (99.0, 2.0, 101.0)
    );
    drop(books);
    cleanup().unwrap();
}

#[test]
fn real_income_history_reads_venue_bills() {
    let (h, _) = started();
    let records = h.drive(h.venue().fetch_income_history(None)).unwrap();
    assert_eq!(
        serde_json::to_value(&records).unwrap(),
        json!([{"symbol":"BTCUSDT","incomeType":"contract_settle_fee","asset":"USDT","timestamp":1700000000000.0,"tranId":"b1:funding","income":-0.25,"category":"funding"}]),
        "fetchIncomeHistory must read the real bill route"
    );
    let range = h
        .drive(
            h.venue()
                .fetch_income_history_range(1_699_999_999_900.0, 1_700_000_000_100.0, None),
        )
        .unwrap();
    assert_eq!(
        range, records,
        "shared pager retains the actual signed bill result"
    );
    let via_capability = h
        .drive(
            h.venue()
                .capabilities()
                .fetch_income_history
                .call(h.venue(), None),
        )
        .unwrap();
    assert_eq!(
        via_capability, records,
        "income capability must route to the venue body"
    );
}

#[test]
fn real_historical_positions_fold_from_fills() {
    let (h, _) = started();
    let records = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "synthetic".into(),
                    from: Some(1_690_000_000_000.0),
                    to: Some(1_700_000_000_000.0),
                }),
        )
        .map_err(|e| {
            format!(
                "{e:?} {:?}",
                h.http
                    .requests
                    .borrow()
                    .iter()
                    .map(|r| r.url.clone())
                    .collect::<Vec<_>>()
            )
        })
        .expect("Bitget journal folds closed round trips from fills");
    assert!(records.is_empty());
    let urls: Vec<String> = h
        .http
        .requests
        .borrow()
        .iter()
        .map(|r| r.url.clone())
        .collect();
    assert!(
        urls.iter().any(|u| u.contains("fill")),
        "journal fold must read real fills: {urls:?}"
    );
}
