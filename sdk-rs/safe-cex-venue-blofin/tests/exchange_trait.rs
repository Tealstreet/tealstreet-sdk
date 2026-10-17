//! Every `not_implemented()` default of the Exchange trait, driven on a constructed
//! BlofinNativeExchange through `dyn Exchange` against a scripted transport.
#[path = "support/native_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::{OrderSide, OrderType, PlaceOrderOpts, PositionMode, UpdateOrderOpts};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted call must settle");
}
fn script(wire: &support::Wire, path: &str, body: Value) {
    wire.bodies.borrow_mut().insert(path.into(), body);
}
fn calls(wire: &support::Wire) -> Vec<(String, String)> {
    wire.requests
        .borrow_mut()
        .drain(..)
        .map(|r| (r.method.to_uppercase(), support::path(&r.url).to_owned()))
        .collect()
}
fn sent(log: &[(String, String)], method: &str, path: &str) -> bool {
    log.iter().any(|(m, u)| m == method && u == path)
}
fn ok(data: Value) -> Value {
    json!({"code":"0","msg":"success","data":data})
}
const MARKETS: &str = "/api/v1/market/instruments";
const TICKERS: &str = "/api/v1/market/tickers";
const BALANCE: &str = "/api/v1/asset/balances";
const POSITIONS: &str = "/api/v1/account/positions";
const MARGIN_MODE: &str = "/api/v1/account/margin-mode";
const POSITION_MODE: &str = "/api/v1/account/position-mode";
const PENDING: &str = "/api/v1/trade/orders-pending";
const BATCH: &str = "/api/v1/trade/batch-orders";
const CANCEL: &str = "/api/v1/trade/cancel-batch-orders";

#[test]
fn public_frames_preserve_book_sequences_and_candle_volume() {
    use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    use std::cell::RefCell;
    let (venue, rt, store, _) = support::fixture();
    let mut loaded = store.loaded().clone();
    loaded.markets = true;
    Store::update(store.as_ref(), StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.1},"limits":{"amount":{"min":0.0001,"max":10000},"leverage":{"min":1,"max":150}},"contractSize":0.001})).unwrap()]),
        tickers: Some(vec![serde_json::from_value(json!({"id":"BTC-USDT","symbol":"BTCUSDT","bid":99,"ask":101,"last":100,"mark":100,"index":100,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap()]),
        loaded: Some(loaded), ..Default::default()
    }, false).unwrap();
    rt.open.set(true);
    let core = venue.base().get_socket(SocketSlot::Public).unwrap();
    drive(
        &rt,
        core.create_websocket("ws://market-fixture".into(), vec![], None),
    )
    .unwrap();
    let pump = || {
        for _ in 0..12 {
            rt.pump();
        }
    };
    pump();
    let socket = rt.opened.borrow()[0].clone();
    let frame = |channel: &str, action: &str, data: Value| {
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"arg":{"channel":channel,"instId":"BTC-USDT"},"action":action,"data":data})
                    .to_string(),
            )));
        pump();
        assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    };
    let books = Rc::new(RefCell::new(Vec::new()));
    let seen = books.clone();
    let close_book = venue
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |book| seen.borrow_mut().push(book)),
        )
        .unwrap();
    frame(
        "books",
        "snapshot",
        json!([{"seqId":100,"bids":[["99","2"],["98","3"]],"asks":[["101","2"]]}]),
    );
    assert_eq!(books.borrow().len(), 1);
    assert_eq!(books.borrow()[0].bids[1].total, 5.0);
    frame(
        "books",
        "update",
        json!([{"prevSeqId":100,"seqId":101,"bids":[["99","0"],["98","7"]],"asks":[["101","4"]]}]),
    );
    assert_eq!(books.borrow().len(), 2);
    assert_eq!(
        (
            books.borrow()[1].bids.len(),
            books.borrow()[1].bids[0].amount
        ),
        (1, 7.0)
    );
    let sent = socket.sent.borrow().len();
    frame(
        "books",
        "update",
        json!([{"prevSeqId":102,"seqId":103,"bids":[["99","9"]],"asks":[]} ]),
    );
    assert_eq!(books.borrow().len(), 2, "gap suppresses callback");
    assert_eq!(
        socket.sent.borrow().len(),
        sent + 2,
        "gap unsubscribes/resubscribes once"
    );
    frame(
        "books",
        "snapshot",
        json!([{"seqId":200,"bids":[["99","2"],["98","3"]],"asks":[["101","2"]]}]),
    );
    frame(
        "books5",
        "snapshot",
        json!([{"bids":[["99.5","1"]],"asks":[["100.5","1"]]}]),
    );
    assert_eq!(books.borrow().len(), 4);
    assert_eq!(
        (
            books.borrow()[3].bids[0].price,
            books.borrow()[3].bids[1].price,
            books.borrow()[3].bids[1].total
        ),
        (99.5, 98.0, 4.0)
    );
    frame(
        "tickers",
        "snapshot",
        json!([{"instId":"BTC-USDT","last":"100","open24h":"80","bidPrice":"99","askPrice":"101","volCurrency24h":"8","vol24h":"9"}]),
    );
    assert_eq!(
        (
            store.tickers()[0].last,
            store.tickers()[0].percentage,
            store.tickers()[0].volume,
            store.tickers()[0].quote_volume
        ),
        (100.0, 25.0, 8.0, 9.0)
    );
    let candles = Rc::new(RefCell::new(Vec::new()));
    let seen = candles.clone();
    let close_candle = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(move |candle| seen.borrow_mut().push(candle)),
        )
        .unwrap();
    frame(
        "candle1m",
        "snapshot",
        json!([[
            1700000000000u64,
            "99",
            "103",
            "98",
            "101",
            "111",
            "222",
            "7"
        ]]),
    );
    assert_eq!(candles.borrow().len(), 1);
    assert_eq!(
        (
            candles.borrow()[0].timestamp,
            candles.borrow()[0].close,
            candles.borrow()[0].volume
        ),
        (1700000000.0, 101.0, 7.0)
    );
    close_book().unwrap();
    close_candle().unwrap();
    frame(
        "candle1m",
        "snapshot",
        json!([[
            1700000060000u64,
            "99",
            "103",
            "98",
            "101",
            "111",
            "222",
            "7"
        ]]),
    );
    assert_eq!(candles.borrow().len(), 1);
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("unsubscribe") && frame.contains("books5"))
    );
    venue.dispose().unwrap();
}

#[test]
fn all_sixteen_not_implemented_defaults_are_bound_on_the_constructed_venue() {
    let (o, rt, store, wire) = support::fixture();
    let venue: Rc<dyn Exchange> = o.clone();
    assert_eq!(venue.class_name(), "BlofinExchange");
    script(
        &wire,
        MARKETS,
        ok(json!([
            {"instId":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","contractValue":"0.001","listTime":"1700000000000","expireTime":"0","maxLeverage":"150","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","instType":"SWAP","contractType":"linear","maxLimitSize":"100000","maxMarketSize":"10000","state":"live"},
            {"instId":"BTC-USD","baseCurrency":"BTC","quoteCurrency":"USD","contractValue":"1","maxLeverage":"100","minSize":"1","lotSize":"1","tickSize":"0.1","instType":"SWAP","contractType":"inverse","maxLimitSize":"1000","maxMarketSize":"1000","state":"live"}
        ])),
    );
    script(
        &wire,
        TICKERS,
        ok(
            json!([{"instId":"BTC-USDT","last":"100","lastSize":"1","askPrice":"101","askSize":"3","bidPrice":"99","bidSize":"2","open24h":"80","high24h":"110","low24h":"75","volCurrency24h":"5","vol24h":"500","ts":"1710000000000"}]),
        ),
    );
    script(&wire, MARGIN_MODE, ok(json!({"marginMode":"cross"})));
    script(&wire, POSITION_MODE, ok(json!({"positionMode":"net_mode"})));
    script(
        &wire,
        POSITIONS,
        ok(
            json!([{"positionId":"p1","instId":"BTC-USDT","instType":"SWAP","marginMode":"cross","positionSide":"net","adl":"1","positions":"2","availablePositions":"2","averagePrice":"100","markPrice":"102","marginRatio":"0.1","liquidationPrice":"50","unrealizedPnl":"0.004","unrealizedPnlRatio":"0.02","maintenanceMargin":"0.01","createTime":"1709990000000","updateTime":"1709990000000","leverage":"10"}]),
        ),
    );
    script(
        &wire,
        BALANCE,
        ok(json!([{"currency":"USDT","balance":"100","available":"90","frozen":"10","bonus":"0"}])),
    );
    script(
        &wire,
        PENDING,
        ok(
            json!([{"orderId":"o1","clientOrderId":"","instId":"BTC-USDT","marginMode":"cross","positionSide":"net","side":"buy","orderType":"limit","price":"100","size":"2","reduceOnly":"false","leverage":"10","state":"live","filledSize":"0","averagePrice":"0","fee":"0","pnl":"0","createTime":"1709990000000","updateTime":"1709990000000","orderCategory":"normal","tpTriggerPrice":null,"slTriggerPrice":null}]),
        ),
    );

    // 1 fetch_markets: only linear USDT swaps survive; contract value carries sizing
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1, "inverse swap filtered");
    assert_eq!(markets[0].symbol, "BTCUSDT");
    assert_eq!(markets[0].contract_size, Some(0.001));
    assert_eq!(markets[0].limits.leverage.max.to_bits(), 150f64.to_bits());
    assert!(sent(&calls(&wire), "GET", MARKETS));

    // 2 start: markets, tickers, account modes, balance/positions and orders loaded
    drive(&rt, venue.start()).unwrap();
    let loaded = store.loaded().clone();
    assert!(
        loaded.markets && loaded.tickers && loaded.balance && loaded.positions && loaded.orders
    );
    let log = calls(&wire);
    for path in [
        MARKETS,
        TICKERS,
        MARGIN_MODE,
        POSITION_MODE,
        POSITIONS,
        BALANCE,
        PENDING,
    ] {
        assert!(sent(&log, "GET", path), "start reads {path}: {log:?}");
    }
    assert!(
        rt.dials
            .borrow()
            .iter()
            .any(|u| u == "wss://openapi.blofin.com/ws/public"),
        "public socket dialled: {:?}",
        rt.dials.borrow()
    );

    // 3 fetch_tickers: percentage from open24h, bid/ask from the book
    let tickers = drive(&rt, venue.fetch_tickers()).unwrap();
    assert_eq!(tickers[0].symbol, "BTCUSDT");
    assert_eq!(tickers[0].percentage.to_bits(), 25f64.to_bits());
    assert_eq!(tickers[0].bid.to_bits(), 99f64.to_bits());
    assert!(sent(&calls(&wire), "GET", TICKERS));

    // 4 get_account: inherited BaseExchange rejection (base.ts:734), nothing sent
    let account = drive(&rt, venue.get_account()).unwrap_err();
    assert_eq!(account.message, "Not implemented");
    assert!(calls(&wire).is_empty(), "getAccount sends nothing");

    // 5 validate_account: futures balance = available + frozen
    let validation = drive(&rt, venue.validate_account()).unwrap();
    assert_eq!(validation.error.as_value(), &json!(""));
    assert_eq!(
        validation.balance.map(|b| b.total.to_bits()),
        Some(100f64.to_bits())
    );
    assert!(sent(&calls(&wire), "GET", BALANCE));

    // 6 fetch_positions: contracts * contractValue, net side
    let positions = drive(&rt, venue.fetch_positions(None)).unwrap();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].contracts.to_bits(), 0.002f64.to_bits());
    assert_eq!(positions[0].entry_price.to_bits(), 100f64.to_bits());
    assert_eq!(positions[0].position_mode, Some(PositionMode::OneWay));
    assert!(sent(&calls(&wire), "GET", POSITIONS));

    // 7 fetch_orders: paginated normal/TPSL/trigger families
    let orders = drive(&rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o1");
    assert_eq!(orders[0].symbol, "BTCUSDT");
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].r#type, OrderType::Limit);
    let log = calls(&wire);
    for path in [
        PENDING,
        "/api/v1/trade/orders-tpsl-pending",
        "/api/v1/trade/orders-algo-pending",
    ] {
        assert!(sent(&log, "GET", path), "fetchOrders reads {path}");
    }
    let order = orders[0].clone();

    // 8 place_order through the validated BaseExchange entry
    script(
        &wire,
        BATCH,
        ok(json!([{"orderId":"n1","clientOrderId":"c1","msg":"success","code":"0"}])),
    );
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}),
    )
    .unwrap();
    let ids = drive(&rt, o.base().place_order(venue.as_ref(), opts)).unwrap();
    assert_eq!(ids, ["n1"]);
    let placed = wire
        .requests
        .borrow()
        .iter()
        .find(|r| r.url == BATCH)
        .cloned();
    let body = placed
        .and_then(|r| r.body)
        .map(|b| b.untransformed().unwrap().clone().into_value().unwrap());
    assert_eq!(
        body.as_ref().map(|b| &b[0]["instId"]),
        Some(&json!("BTC-USDT"))
    );
    assert_eq!(body.as_ref().map(|b| &b[0]["side"]), Some(&json!("buy")));
    assert!(sent(&calls(&wire), "POST", BATCH));

    // caller clientOrderId contract: BloFin rule installed on the base, refused before any send
    let bad: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"bad-id!"}),
    )
    .unwrap();
    let refused = drive(&rt, o.base().place_order(venue.as_ref(), bad)).unwrap_err();
    assert_eq!(
        refused.message,
        "BloFin clientOrderId must match /^[A-Za-z0-9]{1,32}$/"
    );
    assert!(calls(&wire).is_empty(), "refused caller id sends nothing");

    // 9 update_order: cancel the resting order, then re-place it
    script(
        &wire,
        CANCEL,
        ok(json!([{"orderId":"o1","clientOrderId":"","code":"0","msg":"success"}])),
    );
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": order, "update": {"price": 101}})).unwrap();
    let ids = drive(&rt, venue.update_order(update)).unwrap();
    assert_eq!(ids, ["n1"]);
    let log = calls(&wire);
    let cancel = log
        .iter()
        .position(|c| c == &("POST".into(), CANCEL.into()));
    let place = log.iter().position(|c| c == &("POST".into(), BATCH.into()));
    assert!(
        cancel.is_some() && cancel < place,
        "cancel-and-replace order: {log:?}"
    );

    // 10 cancel_orders
    drive(&rt, venue.cancel_orders(vec![order])).unwrap();
    assert!(sent(&calls(&wire), "POST", CANCEL));

    // 11 set_leverage: clamped, single cross write, position patched
    script(
        &wire,
        "/api/v1/account/set-leverage",
        ok(json!({"instId":"BTC-USDT","leverage":"5","marginMode":"cross","positionSide":"net"})),
    );
    drive(&rt, venue.set_leverage("BTCUSDT".into(), 5.0, None)).unwrap();
    assert!(sent(&calls(&wire), "POST", "/api/v1/account/set-leverage"));
    assert!(
        store
            .positions()
            .iter()
            .filter(|p| p.symbol == "BTCUSDT")
            .all(|p| p.leverage == 5.0),
        "leverage applied to store positions"
    );

    // 12 change_position_mode: confirmed hedged mode is applied
    script(
        &wire,
        "/api/v1/account/set-position-mode",
        ok(json!({"positionMode":"long_short_mode"})),
    );
    drive(&rt, venue.change_position_mode(true, None)).unwrap();
    assert!(sent(
        &calls(&wire),
        "POST",
        "/api/v1/account/set-position-mode"
    ));
    assert!(
        store
            .positions()
            .iter()
            .all(|p| p.position_mode == Some(PositionMode::Hedged)),
        "confirmed mode applied"
    );

    // 13 change_margin_type: write, then refetch the account margin mode
    script(
        &wire,
        "/api/v1/account/set-margin-mode",
        ok(json!({"marginMode":"isolated"})),
    );
    script(&wire, MARGIN_MODE, ok(json!({"marginMode":"isolated"})));
    drive(&rt, venue.change_margin_type(true, None)).unwrap();
    let log = calls(&wire);
    assert!(sent(&log, "POST", "/api/v1/account/set-margin-mode"));
    assert!(
        sent(&log, "GET", MARGIN_MODE),
        "margin mode refetched: {log:?}"
    );
    calls(&wire);

    // 14-16 inherited BaseExchange rejections (base.ts:799, :989, :1138)
    let max = drive(&rt, venue.fetch_symbol_max_leverage("BTCUSDT".into())).unwrap_err();
    assert_eq!(max.message, "Not implemented");
    let schedule = drive(&rt, venue.schedule_cancel(Some(Some(60_000.0)))).unwrap_err();
    assert_eq!(schedule.message, "Not implemented");
    let trades = drive(&rt, venue.fetch_public_trades("BTCUSDT".into(), None)).unwrap_err();
    assert_eq!(trades.message, "Not implemented");
    assert!(calls(&wire).is_empty(), "inherited rejections send nothing");

    assert_eq!(
        *rt.errors.borrow(),
        [
            "Exchange Error: BloFin clientOrderId must match /^[A-Za-z0-9]{1,32}$/",
            "Exchange Error: Changing margin type with open positions / orders is allowed but can cause unexpected behavior. Symbols with open positions will remain synced to previous mode until position is closed.",
        ],
        "only the two source emitAndLogError reports"
    );
    venue.dispose().unwrap();
    assert!(venue.base().is_disposed());
}

#[test]
fn amendment_omits_undefined_type_and_side_on_the_signed_wire() {
    use safe_cex_store::Store;
    let (concrete, rt, store, wire) = support::fixture();
    let venue: Rc<dyn Exchange> = concrete;
    let market = serde_json::from_value(json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.1},"limits":{"amount":{"min":0.1,"max":1000},"leverage":{"min":1,"max":150}},"info":{"lotSize":"0.1"}})).unwrap();
    Store::update(
        store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    script(&wire, MARGIN_MODE, ok(json!({"marginMode":"cross"})));
    script(&wire, POSITION_MODE, ok(json!({"positionMode":"net_mode"})));
    script(
        &wire,
        CANCEL,
        ok(json!([{"orderId":"o1","clientOrderId":"","code":"0","msg":"success"}])),
    );
    script(
        &wire,
        BATCH,
        ok(json!([{"orderId":"n1","clientOrderId":"c1","msg":"success","code":"0"}])),
    );
    let opts = serde_json::from_value(json!({"order":{"id":"o1","symbol":"BTCUSDT","status":"open","price":100,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":1},"update":{"price":101}})).unwrap();
    assert_eq!(drive(&rt, venue.update_order(opts)).unwrap(), ["n1"]);
    let requests = wire.requests.borrow();
    let placed = requests.iter().find(|r| r.url == BATCH).unwrap();
    let body = placed
        .body
        .as_ref()
        .unwrap()
        .untransformed()
        .unwrap()
        .clone()
        .into_value()
        .unwrap();
    let row = body[0].as_object().unwrap();
    assert!(!row.contains_key("side") && !row.contains_key("orderType"));
    drop(requests);
    venue.dispose().unwrap();
}
