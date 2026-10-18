//! Every `not_implemented()` default of the Exchange trait, driven on a constructed
//! BitunixVenue through `dyn Exchange` against a scripted transport.
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Order, PlaceOrderOpts, Position, UpdateOrderOpts};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[test]
fn public_market_data_uses_futures_frames_and_distinct_candle_volumes() {
    use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
    use safe_cex_transport::socket::SocketFrame;
    use std::cell::RefCell;
    let (venue, rt, store, wire) = support::fixture();
    script(
        &wire,
        "/api/v1/futures/account/get_leverage_margin_mode",
        json!({"code":0,"data":{"symbol":"BTCUSDT","leverage":10,"marginMode":"CROSS"}}),
    );
    script(
        &wire,
        "/api/v1/futures/market/kline",
        json!({"code":0,"data":[{"time":1700000000000u64,"open":"99","high":"103","low":"98","close":"101","baseVol":"111","quoteVol":"7"}]}),
    );
    let history = drive(
        &rt,
        venue.fetch_ohlcv(
            serde_json::from_value(
                json!({"symbol":"BTCUSDT","interval":"1m","limit":2,"to":1700000000000u64}),
            )
            .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(
        (history[0].timestamp, history[0].volume),
        (1700000000.0, 7.0)
    );
    rt.dial.set(true);
    let core = venue.base().get_socket(SocketSlot::Futures).unwrap();
    drive(
        &rt,
        core.create_websocket("ws://futures-fixture".into(), vec![], None),
    )
    .unwrap();
    let pump = || {
        for _ in 0..12 {
            rt.pump();
        }
    };
    pump();
    let socket = rt.dialed.borrow()[0].1.clone();
    let books = Rc::new(RefCell::new(Vec::new()));
    let seen = books.clone();
    let close_book = venue
        .listen_order_book(
            "BTCUSDT".into(),
            Rc::new(move |book| seen.borrow_mut().push(book)),
        )
        .unwrap();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let seen = candles.clone();
    let close_candle = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(move |candle| seen.borrow_mut().push(candle)),
        )
        .unwrap();
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("market_btcusdt_depthop_0.1")),
        "book uses price precision, not amount0.001"
    );
    assert!(
        socket
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("market_btcusdt_klineop_1min"))
    );
    let frame = |channel: &str, data: Value| {
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"channel":channel,"data":data}).to_string(),
            )));
        pump();
        assert!(rt.errors.borrow().is_empty(), "{:?}", rt.errors.borrow());
    };
    frame(
        "market_btcusdt_depthop_0.1",
        json!({"bids":[["99","2"],["98","3"]],"asks":[["101","4"]]}),
    );
    assert_eq!(books.borrow().len(), 1);
    assert_eq!(
        (
            books.borrow()[0].bids[1].total,
            books.borrow()[0].asks[0].amount
        ),
        (5.0, 4.0)
    );
    frame(
        "market_btcusdt_depthop_0.1",
        json!({"bids":[["97","6"]],"asks":[["104","7"]]}),
    );
    assert_eq!(books.borrow().len(), 2);
    assert_eq!(
        (
            books.borrow()[1].bids.len(),
            books.borrow()[1].bids[0].price
        ),
        (1, 97.0),
        "each futures book frame replaces both sides"
    );
    frame(
        "market_btcusdt_klineop_1min",
        json!({"time":1700000060000u64,"open":"100","high":"104","low":"99","close":"102","volume":"8","quoteVol":"999"}),
    );
    assert_eq!(candles.borrow().len(), 1);
    assert_eq!(
        (
            candles.borrow()[0].timestamp,
            candles.borrow()[0].close,
            candles.borrow()[0].volume
        ),
        (1700000060000.0, 102.0, 8.0)
    );
    assert_eq!(
        store.tickers().unwrap()[0].last,
        102.0,
        "candle parser updates the real ticker before callback"
    );
    let sent = socket.sent.borrow().len();
    close_book().unwrap();
    close_candle().unwrap();
    assert_eq!(
        socket.sent.borrow().len(),
        sent,
        "source cleanup removes handlers without an unsubscribe frame"
    );
    frame(
        "market_btcusdt_depthop_0.1",
        json!({"bids":[["96","1"]],"asks":[["105","1"]]}),
    );
    frame(
        "market_btcusdt_klineop_1min",
        json!({"time":1700000120000u64,"open":"100","high":"104","low":"99","close":"103","volume":"9"}),
    );
    assert_eq!((books.borrow().len(), candles.borrow().len()), (2, 1));
    assert_eq!(store.tickers().unwrap()[0].last, 102.0);
    venue.dispose().unwrap();
}

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
    wire.bodies.lock().unwrap().insert(path.into(), body);
}
/// (METHOD, path, body) sent since the last drain.
fn calls(wire: &support::Wire) -> Vec<(String, String, Value)> {
    wire.requests
        .lock()
        .unwrap()
        .drain(..)
        .map(|r| {
            let body = match r.body {
                Some(safe_cex_transport::RestBody::Untransformed(v)) => v.into_value().unwrap(),
                Some(safe_cex_transport::RestBody::Encoded(b)) => {
                    serde_json::from_slice(&b).unwrap()
                }
                Some(safe_cex_transport::RestBody::Transformed(t)) => {
                    match &t.value().clone().into_value().unwrap() {
                        Value::String(s) => serde_json::from_str(s).unwrap(),
                        v => v.clone(),
                    }
                }
                None => Value::Null,
            };
            (r.method.to_uppercase(), r.url, body)
        })
        .collect()
}
/// Headers and transmitted body text of the last request to `path`, without draining.
fn wire_request(wire: &support::Wire, path: &str) -> (Vec<(String, String)>, String) {
    let requests = wire.requests.lock().unwrap();
    let r = requests
        .iter()
        .rev()
        .find(|r| r.url == path)
        .expect("request sent");
    let raw = match &r.body {
        Some(safe_cex_transport::RestBody::Transformed(t)) => {
            match &t.value().clone().into_value().unwrap() {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            }
        }
        Some(safe_cex_transport::RestBody::Untransformed(v)) => {
            v.clone().into_value().unwrap().to_string()
        }
        Some(safe_cex_transport::RestBody::Encoded(b)) => String::from_utf8(b.clone()).unwrap(),
        None => String::new(),
    };
    let headers = r
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    (headers, raw)
}
fn sent(log: &[(String, String, Value)], method: &str, path: &str) -> Option<Value> {
    log.iter()
        .find(|(m, u, _)| m == method && u == path)
        .map(|(_, _, b)| b.clone())
}
const MARKETS: &str = "/api/v1/futures/market/trading_pairs";
const TICKERS: &str = "/api/v1/futures/market/tickers";
const ACCOUNT: &str = "/api/v1/futures/account";
const POSITIONS: &str = "/api/v1/futures/position/get_pending_positions";
const OPEN_ORDERS: &str = "/api/v1/futures/trade/get_pending_orders";
const OPEN_TPSL: &str = "/api/v1/futures/tpsl/get_pending_orders";
const ORDER: &str = "/api/v1/futures/trade/place_order";
const MODIFY: &str = "/api/v1/futures/trade/modify_order";
const CANCEL: &str = "/api/v1/futures/trade/cancel_orders";
const LEVERAGE: &str = "/api/v1/futures/account/change_leverage";
const HEDGE: &str = "/api/v1/futures/account/change_position_mode";
const MARGIN: &str = "/api/v1/futures/account/change_margin_mode";

#[test]
fn all_sixteen_not_implemented_defaults_are_bound_on_the_constructed_venue() {
    let (o, rt, store, wire) = support::fixture();
    let venue: Rc<dyn Exchange> = o.clone();
    script(
        &wire,
        MARKETS,
        json!({"code":0,"msg":"Success","data":[
            {"symbol":"BTCUSDT","base":"BTC","quote":"USDT","minTradeVolume":"0.0001","maxLimitOrderVolume":"500","maxMarketOrderVolume":"300","basePrecision":3,"quotePrecision":1,"maxLeverage":125,"minLeverage":1,"defaultLeverage":20,"defaultMarginMode":"2","symbolStatus":"OPEN"},
            {"symbol":"ETHUSDC","base":"ETH","quote":"USDC","maxLimitOrderVolume":"10","basePrecision":2,"quotePrecision":2,"symbolStatus":"OPEN"}
        ]}),
    );
    script(
        &wire,
        TICKERS,
        json!({"code":0,"msg":"Success","data":[{"symbol":"BTCUSDT","markPrice":"101","lastPrice":"100","open":"80","last":"100","quoteVol":"200","baseVol":"2","high":"110","low":"70"}]}),
    );

    // 1 fetch_markets
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    assert_eq!(markets.len(), 1, "only OPEN USDT markets");
    assert_eq!(markets[0].id, "BTC/USDT");
    assert_eq!(markets[0].precision.amount.to_bits(), 0.001f64.to_bits());
    assert_eq!(
        safe_cex_num::numeric_value_to_number(&markets[0].limits.amount.max).to_bits(),
        500f64.to_bits()
    );
    assert!(sent(&calls(&wire), "GET", MARKETS).is_some());

    // 2 fetch_tickers
    let tickers = drive(&rt, venue.fetch_tickers()).unwrap();
    assert_eq!(
        tickers[0].mark.to_bits(),
        101f64.to_bits(),
        "mark from markPrice"
    );
    assert_eq!(
        tickers[0].percentage.to_bits(),
        25f64.to_bits(),
        "(last-open)/open"
    );
    assert!(sent(&calls(&wire), "GET", TICKERS).is_some());

    // 3 start (public-data-only fixture: markets, tickers, then account slots loaded)
    drive(&rt, venue.start()).unwrap();
    let loaded = store.loaded().unwrap().clone();
    assert!(loaded.markets && loaded.tickers, "start loads public state");
    assert!(
        loaded.orders && loaded.balance,
        "public-only start marks account state loaded"
    );
    let log = calls(&wire);
    assert!(sent(&log, "GET", MARKETS).is_some() && sent(&log, "GET", TICKERS).is_some());

    // 4 get_account: inherited BaseExchange rejection (base.ts:734)
    let account = drive(&rt, venue.get_account()).unwrap_err();
    assert_eq!(account.message, "Not implemented");
    assert!(
        calls(&wire).is_empty(),
        "inherited getAccount sends nothing"
    );

    // 5 validate_account
    script(
        &wire,
        ACCOUNT,
        json!({"code":0,"msg":"Success","data":{"marginCoin":"USDT","available":"7","frozen":"1","margin":"2","transfer":"7","positionMode":"HEDGE","crossUnrealizedPNL":"0.5","isolationUnrealizedPNL":"0.25","bonus":"0"}}),
    );
    let validation = drive(&rt, venue.validate_account()).unwrap();
    assert_eq!(validation.error.as_value(), &json!(""), "code 0 validates");
    let balance = validation.balance.expect("validation balance");
    assert_eq!(
        balance.total.to_bits(),
        10f64.to_bits(),
        "available+margin+frozen"
    );
    assert_eq!(balance.upnl.to_bits(), 0.75f64.to_bits());
    let log = calls(&wire);
    assert_eq!(
        log.iter()
            .filter(|(m, u, _)| m == "GET" && u == ACCOUNT)
            .count(),
        2,
        "validate then fetchBalance"
    );

    // 6 fetch_positions
    script(
        &wire,
        POSITIONS,
        json!({"code":0,"msg":"Success","data":[{"positionId":"pos-1","symbol":"BTCUSDT","qty":"0.5","entryValue":"50","side":"BUY","positionMode":"HEDGE","marginMode":"CROSS","leverage":"20","fee":"0","funding":"0","realizedPNL":"0","margin":"2.5","unrealizedPNL":"1","liqPrice":"50","avgOpenPrice":"100","ctime":"1700000000000","mtime":"1700000000000"}]}),
    );
    let positions = drive(&rt, venue.fetch_positions(None)).unwrap();
    let long = positions
        .iter()
        .find(|p| p.id == "BTCUSDT:long")
        .expect("long row");
    assert_eq!(long.contracts.to_bits(), 0.5f64.to_bits());
    assert_eq!(long.leverage.to_bits(), 20f64.to_bits());
    assert!(
        positions
            .iter()
            .any(|p| p.id == "BTCUSDT:short" && p.contracts == 0.0),
        "fake short leg"
    );
    assert!(sent(&calls(&wire), "GET", POSITIONS).is_some());

    // 7 fetch_orders (regular + TP/SL families)
    script(
        &wire,
        OPEN_ORDERS,
        json!({"code":0,"msg":"Success","data":{"orderList":[{"orderId":"111","qty":"0.2","tradeQty":"0","price":"90","symbol":"BTCUSDT","positionMode":"HEDGE","marginMode":"CROSS","leverage":20,"status":"NEW_","fee":"0","realizedPNL":"0","type":"LIMIT","orderType":"LIMIT","effect":"GTC","reduceOnly":false,"clientId":"c-1","side":"BUY","ctime":1700000000000i64,"mtime":1700000000001i64}],"total":"1"}}),
    );
    script(
        &wire,
        OPEN_TPSL,
        json!({"code":0,"msg":"Success","data":[]}),
    );
    let orders = drive(&rt, venue.fetch_orders(None)).unwrap();
    assert_eq!(orders.len(), 1, "one regular LIMIT order");
    assert_eq!(orders[0].id, "111");
    assert_eq!(orders[0].amount.to_bits(), 0.2f64.to_bits());
    let log = calls(&wire);
    assert!(sent(&log, "GET", OPEN_ORDERS).is_some() && sent(&log, "GET", OPEN_TPSL).is_some());
    let order: Order = orders[0].clone();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(vec![order.clone()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();

    // 8 place_order (through the validated BaseExchange entry)
    script(
        &wire,
        ORDER,
        json!({"code":0,"msg":"Success","data":{"orderId":"o-1","clientId":"c-2"}}),
    );
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.25,"price":95}),
    )
    .unwrap();
    let ids = drive(&rt, venue.base().place_order(venue.as_ref(), opts)).unwrap();
    assert_eq!(ids, ["o-1"]);
    let (headers, raw) = wire_request(&wire, ORDER);
    let header = |k: &str| {
        headers
            .iter()
            .find(|(h, _)| h == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let digest = safe_cex_venue_bitunix::utils::sha256_hex(&format!(
        "{}{}fixture-key{raw}",
        header("nonce"),
        header("timestamp")
    ));
    assert_eq!(header("api-key"), "fixture-key");
    assert_eq!(
        header("sign"),
        safe_cex_venue_bitunix::utils::sha256_hex(&format!("{digest}fixture-secret")),
        "request interceptor signs the transmitted body"
    );
    let body = sent(&calls(&wire), "POST", ORDER).expect("single order POST");
    assert_eq!(body["side"], json!("BUY"));
    assert_eq!(body["orderType"], json!("LIMIT"));
    assert_eq!(
        (body["qty"].clone(), body["price"].clone()),
        (json!("0.25"), json!("95"))
    );

    // 9 update_order: native LIMIT amendment
    script(
        &wire,
        MODIFY,
        json!({"code":0,"msg":"Success","data":{"orderId":"111"}}),
    );
    let update: UpdateOrderOpts =
        serde_json::from_value(json!({"order": order, "update": {"price": 91}})).unwrap();
    let ids = drive(&rt, venue.update_order(update)).unwrap();
    assert_eq!(ids, ["111"]);
    let body = sent(&calls(&wire), "POST", MODIFY).expect("modify POST");
    assert_eq!(
        (body["orderId"].clone(), body["price"].clone()),
        (json!("111"), json!("91"))
    );

    // 10 cancel_orders
    drive(&rt, venue.cancel_orders(vec![order])).unwrap();
    let body = sent(&calls(&wire), "POST", CANCEL).expect("cancel POST");
    assert_eq!(
        body,
        json!({"symbol":"BTCUSDT","orderList":[{"orderId":"111"}]})
    );
    assert!(
        store
            .orders()
            .unwrap()
            .iter()
            .all(|o| o.status == safe_cex_types::OrderStatus::Canceled)
    );

    // 11 set_leverage
    drive(&rt, venue.set_leverage("BTCUSDT".into(), 5.0, None)).unwrap();
    let body = sent(&calls(&wire), "POST", LEVERAGE).expect("leverage POST");
    assert_eq!(
        body,
        json!({"marginCoin":"USDT","symbol":"BTCUSDT","leverage":5})
    );
    assert!(
        store
            .positions()
            .unwrap()
            .iter()
            .all(|p: &Position| p.leverage == 5.0),
        "both legs updated"
    );

    // 12 change_position_mode (no open contracts in the store)
    drive(&rt, venue.change_position_mode(false, None)).unwrap();
    let body = sent(&calls(&wire), "POST", HEDGE).expect("hedge-mode POST");
    assert_eq!(body, json!({"positionMode":"ONE_WAY"}));
    assert!(
        !store.options().unwrap().is_hedged,
        "isHedged setting follows"
    );

    // 13 change_margin_type
    drive(&rt, venue.change_margin_type(true, Some("BTCUSDT".into()))).unwrap();
    let body = sent(&calls(&wire), "POST", MARGIN).expect("margin POST");
    assert_eq!(
        body,
        json!({"symbol":"BTCUSDT","marginMode":"ISOLATION","marginCoin":"USDT"})
    );

    // 14-16 inherited BaseExchange rejections (base.ts:799, :989, :1138)
    let max = drive(&rt, venue.fetch_symbol_max_leverage("BTCUSDT".into())).unwrap_err();
    assert_eq!(max.message, "Not implemented");
    let schedule = drive(&rt, venue.schedule_cancel(Some(Some(60_000.0)))).unwrap_err();
    assert_eq!(schedule.message, "Not implemented");
    let trades = drive(&rt, venue.fetch_public_trades("BTCUSDT".into(), None)).unwrap_err();
    assert_eq!(trades.message, "Not implemented");
    assert!(calls(&wire).is_empty(), "inherited rejections send nothing");
    assert!(
        rt.errors.borrow().is_empty(),
        "no unhandled task errors: {:?}",
        rt.errors.borrow()
    );
}

#[test]
fn bound_defaults_beyond_the_sixteen_reach_their_ported_bodies() {
    let (o, rt2, store2, wire2) = support::fixture();
    let venue: Rc<dyn Exchange> = o;

    // fetch_ohlcv
    script(
        &wire2,
        "/api/v1/futures/market/kline",
        json!({"code":0,"msg":"Success","data":[{"open":"1","high":"3","low":"0.5","close":"2","quoteVol":"40","baseVol":"20","time":1700000060000i64,"type":"LAST_PRICE"}]}),
    );
    let candles = drive(
        &rt2,
        venue.fetch_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(candles[0].timestamp.to_bits(), 1_700_000_060f64.to_bits());
    assert_eq!(
        candles[0].volume.to_bits(),
        40f64.to_bits(),
        "quoteVol volume"
    );
    assert!(sent(&calls(&wire2), "GET", "/api/v1/futures/market/kline").is_some());

    // fetch_historical_fills: public-data-only accounts read no fills
    let fills = drive(
        &rt2,
        venue.fetch_historical_fills(serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap()),
    )
    .unwrap();
    assert!(
        fills.is_empty() && calls(&wire2).is_empty(),
        "isPublicDataOnly short-circuits"
    );

    // update_symbol_metadata overlays leverage/margin on matching positions
    script(
        &wire2,
        "/api/v1/futures/account/get_leverage_margin_mode",
        json!({"code":0,"msg":"Success","data":{"symbol":"BTCUSDT","marginCoin":"USDT","leverage":25,"marginMode":"ISOLATION"}}),
    );
    drive(&rt2, venue.update_symbol_metadata("BTCUSDT".into())).unwrap();
    assert!(
        store2
            .positions()
            .unwrap()
            .iter()
            .all(|p| p.leverage == 25.0
                && p.margin_type == Some(safe_cex_types::MarginType::Isolated))
    );
    calls(&wire2);

    // place_orders batches two legs of one symbol into batch_order
    script(
        &wire2,
        "/api/v1/futures/trade/batch_order",
        json!({"code":0,"msg":"Success","data":{"successList":[{"orderId":"b-1","clientId":"x"},{"orderId":"b-2","clientId":"y"}],"failureList":[]}}),
    );
    let batch: Vec<PlaceOrderOpts> = serde_json::from_value(json!([
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.1,"price":90},
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.1,"price":89}
    ]))
    .unwrap();
    let ids = drive(&rt2, venue.base().place_orders(venue.as_ref(), batch)).unwrap();
    assert_eq!(ids, ["b-1", "b-2"]);
    let body =
        sent(&calls(&wire2), "POST", "/api/v1/futures/trade/batch_order").expect("batch POST");
    assert_eq!(body["orderList"].as_array().map(Vec::len), Some(2));

    // cancel_symbol_orders selects the symbol's store orders
    let order: Order = serde_json::from_value(json!({"id":"222","status":"open","symbol":"BTCUSDT","type":"limit","side":"sell","price":120,"amount":0.1,"filled":0,"remaining":0.1,"reduceOnly":false,"lastUpdate":1})).unwrap();
    Store::update(
        store2.as_ref(),
        StoreUpdate {
            orders: Some(vec![order]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    drive(&rt2, venue.cancel_symbol_orders("BTCUSDT".into())).unwrap();
    let body = sent(&calls(&wire2), "POST", CANCEL).expect("cancel POST");
    assert_eq!(
        body,
        json!({"symbol":"BTCUSDT","orderList":[{"orderId":"222"}]})
    );

    // listen_ohlcv / listen_order_book register on the futures socket; cleanup unregisters
    let candle_cleanup = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            Rc::new(|_| {}),
        )
        .unwrap();
    let book_cleanup = venue
        .listen_order_book("BTCUSDT".into(), Rc::new(|_| {}))
        .unwrap();
    candle_cleanup().unwrap();
    book_cleanup().unwrap();

    // dispose tears down the base and every socket
    venue.dispose().unwrap();
    assert!(venue.base().is_disposed());
    assert!(
        rt2.errors.borrow().is_empty(),
        "no unhandled task errors: {:?}",
        rt2.errors.borrow()
    );
}

#[test]
fn signed_account_history_axes_walk_their_bitunix_pages() {
    let (o, rt, _, wire) = support::fixture_with(false);
    let venue: Rc<dyn Exchange> = o;
    script(
        &wire,
        "/api/v1/futures/trade/get_history_trades",
        json!({"code":0,"msg":"Success","data":{"tradeList":[{"tradeId":"t-1","orderId":"111","qty":"0.1","positionMode":"HEDGE","marginMode":"CROSS","leverage":20,"price":"100","side":"BUY","orderType":"LIMIT","effect":"GTC","clientId":"c-1","reduceOnly":false,"fee":"0.02","realizedPNL":"1.5","ctime":"1700000000000","roleType":"TAKER","symbol":"BTCUSDT","marginCoin":"USDT"}],"total":"1"}}),
    );
    let income = drive(&rt, venue.fetch_income_history(None)).unwrap();
    assert_eq!(
        income
            .iter()
            .map(|r| r.income_type.as_str())
            .collect::<Vec<_>>(),
        ["REALIZED_PNL", "COMMISSION"]
    );
    assert!(
        sent(
            &calls(&wire),
            "GET",
            "/api/v1/futures/trade/get_history_trades"
        )
        .is_some()
    );
    let ranged = drive(
        &rt,
        venue.fetch_income_history_range(1_600_000_000_000.0, 1_800_000_000_000.0, None),
    )
    .unwrap();
    assert_eq!(ranged.len(), 2, "range delegates to fetchIncomeHistory");
    calls(&wire);

    let fills = drive(
        &rt,
        venue.fetch_historical_fills(serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap()),
    )
    .unwrap();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].order_id.as_deref(), Some("111"));
    calls(&wire);

    script(
        &wire,
        "/api/v1/futures/position/get_history_positions",
        json!({"code":0,"msg":"Success","data":{"positionList":[{"positionId":"p-9","symbol":"BTCUSDT","maxQty":"0.1","qty":"0.1","entryPrice":"100","closePrice":"110","liqQty":"0","side":"BUY","positionMode":"HEDGE","marginMode":"CROSS","leverage":"20","fee":"0.02","funding":"0","realizedPNL":"0.98","liqPrice":"50","ctime":"1700000000000","mtime":"1700000100000"}],"total":"1"}}),
    );
    let pnl = drive(
        &rt,
        venue.fetch_closed_pnl(
            serde_json::from_value(json!({"symbol":"BTCUSDT","accountName":"main"})).unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(pnl.len(), 1);
    assert_eq!(pnl[0].closed_time.to_bits(), 1_700_000_100_000f64.to_bits());
    let records = drive(
        &rt,
        venue.fetch_historical_positions(safe_cex_exchange::journal::HistoricalPositionOptions {
            account: "main".into(),
            from: None,
            to: None,
        }),
    )
    .unwrap();
    assert_eq!(records.len(), 1, "reported closed axis maps one record");
    let log = calls(&wire);
    assert_eq!(
        log.iter()
            .filter(|(_, u, _)| u == "/api/v1/futures/position/get_history_positions")
            .count(),
        2
    );

    script(
        &wire,
        "/api/v1/futures/trade/get_history_orders",
        json!({"code":0,"msg":"Success","data":{"orderList":[{"orderId":"h-1","qty":"0.1","tradeQty":"0.1","price":"100","avgPrice":"100","symbol":"BTCUSDT","status":"FILLED","side":"SELL","orderType":"LIMIT","reduceOnly":true,"ctime":"1700000000000","mtime":"1700000001000"}],"total":"1"}}),
    );
    let history = drive(&rt, venue.fetch_historical_orders(None)).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].id, "h-1");
    assert!(
        sent(
            &calls(&wire),
            "GET",
            "/api/v1/futures/trade/get_history_orders"
        )
        .is_some()
    );
    assert!(
        rt.errors.borrow().is_empty(),
        "no unhandled task errors: {:?}",
        rt.errors.borrow()
    );
}

#[test]
fn signed_start_logs_in_on_the_private_socket_before_its_ping_loop() {
    let (o, rt, store, wire) = support::fixture_with(false);
    let venue: Rc<dyn Exchange> = o;
    rt.dial.set(true);
    script(
        &wire,
        MARKETS,
        json!({"code":0,"msg":"Success","data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","maxLimitOrderVolume":"500","basePrecision":3,"quotePrecision":1,"symbolStatus":"OPEN"}]}),
    );
    script(
        &wire,
        TICKERS,
        json!({"code":0,"msg":"Success","data":[{"symbol":"BTCUSDT","markPrice":"101","open":"80","last":"100","quoteVol":"200","baseVol":"2"}]}),
    );
    script(
        &wire,
        ACCOUNT,
        json!({"code":0,"msg":"Success","data":{"marginCoin":"USDT","available":"7","frozen":"1","margin":"2","positionMode":"ONE_WAY"}}),
    );
    script(
        &wire,
        POSITIONS,
        json!({"code":0,"msg":"Success","data":[]}),
    );
    script(
        &wire,
        OPEN_ORDERS,
        json!({"code":0,"msg":"Success","data":{"orderList":[]}}),
    );
    script(
        &wire,
        OPEN_TPSL,
        json!({"code":0,"msg":"Success","data":[]}),
    );
    drive(&rt, venue.start()).unwrap();
    for _ in 0..4 {
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    let loaded = store.loaded().unwrap().clone();
    assert!(
        loaded.orders && loaded.balance && loaded.positions,
        "signed start loads account state"
    );
    assert!(
        !store.options().unwrap().is_hedged,
        "fetchPositionMode reads ONE_WAY from the account row"
    );
    let dialed = rt.dialed.borrow();
    let urls: Vec<&str> = dialed.iter().map(|(u, _)| u.as_str()).collect();
    assert!(
        urls.contains(&"wss://fapi.bitunix.com/private/"),
        "private socket dialed: {urls:?}"
    );
    let private = &dialed
        .iter()
        .find(|(u, _)| u.ends_with("/private/"))
        .unwrap()
        .1;
    let frames = private.sent.borrow().clone();
    let login = frames
        .iter()
        .position(|f| f.contains(r#""op":"login""#))
        .expect("login frame sent");
    assert_eq!(frames[0], r#"{"op":"ping"}"#, "onOpen pings first");
    assert!(
        frames[1..login].iter().all(|f| f == r#"{"op":"ping"}"#),
        "nothing but pings precede login: {frames:?}"
    );
    let args: Value = serde_json::from_str::<Value>(&frames[login]).unwrap()["args"][0].clone();
    let digest = safe_cex_venue_bitunix::utils::sha256_hex(&format!(
        "{}{}fixture-key",
        args["nonce"].as_str().unwrap(),
        args["timestamp"]
    ));
    assert_eq!(args["apiKey"], json!("fixture-key"));
    assert_eq!(
        args["sign"],
        json!(safe_cex_venue_bitunix::utils::sha256_hex(&format!(
            "{digest}fixture-secret"
        ))),
        "double-sha256 login signature"
    );
    let public = &dialed
        .iter()
        .find(|(u, _)| u.ends_with("/public/"))
        .unwrap()
        .1;
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|f| f.contains(r#""op":"subscribe""#) && f.contains(r#""ch":"ticker""#)),
        "public socket subscribes its market shard"
    );
    drop(dialed);
    venue.dispose().unwrap();
    assert!(
        rt.errors.borrow().is_empty(),
        "no unhandled task errors: {:?}",
        rt.errors.borrow()
    );
}

#[test]
fn position_adapter_dependencies_use_real_account_and_store_quote_resolution() {
    let (venue, rt, store, wire) = support::fixture_with(false);
    let mut markets = store.markets().to_vec();
    markets[0].quote = "USDC".into();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let rows: Vec<Value> = ["BTCUSDT", "ETHUSDT"]
        .into_iter()
        .map(|symbol| {
            json!({
                "positionId":format!("dependency-{symbol}"), "symbol":symbol, "qty":"2", "maxQty":"3",
                "entryPrice":"100", "closePrice":"110", "side":"BUY", "positionMode":"HEDGE",
                "leverage":"20", "fee":"2", "funding":"1", "realizedPNL":"8",
                "ctime":"1700000000000", "mtime":"1700000100000"
            })
        })
        .collect();
    script(
        &wire,
        "/api/v1/futures/position/get_history_positions",
        json!({"code":0,"data":{"positionList":rows,"total":"2"}}),
    );
    let account = "history-account-α";
    let records = drive(
        &rt,
        venue.fetch_historical_positions(safe_cex_exchange::journal::HistoricalPositionOptions {
            account: account.into(),
            from: None,
            to: None,
        }),
    )
    .unwrap();
    assert_eq!(records.len(), 2);
    for (record, symbol, quote) in [
        (&records[0], "BTCUSDT", "USDC"),
        (&records[1], "ETHUSDT", "USDT"),
    ] {
        assert_eq!(record["account"], account);
        assert_eq!(record["symbol"], symbol);
        assert_eq!(
            record["settleCurrency"], quote,
            "actual dependency reads Store then source USDT fallback"
        );
        assert_eq!(
            record["key"],
            format!("{account}|{symbol}|long|dependency-{symbol}")
        );
        assert_eq!(
            record["realizedPnl"], 10.0,
            "production dependency keeps realized-is-net-of-funding false"
        );
        assert_eq!(record["fees"], 2.0);
        assert_eq!(record["funding"], 1.0);
    }
    let requests = wire.requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|request| request.url == "/api/v1/futures/position/get_history_positions")
        .unwrap();
    assert_eq!(request.headers["api-key"], "fixture-key");
    assert_eq!(request.headers["sign"].len(), 64);
    drop(requests);
    venue.dispose().unwrap();
}
