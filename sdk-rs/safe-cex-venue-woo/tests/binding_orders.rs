//! WOO order members (legacy_order_control / legacy_order_writes) driven through
//! the production WooExchange and the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)] // Shared support: each test crate uses a subset.
mod binding_harness;
use binding_harness::{Harness, Http, harness, options};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Order, PlaceOrderOpts, Position};
use serde_json::{Value, json};

fn market() -> Value {
    json!({"id":"PERP_BTC_USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.001,"price":0.1},
        "limits":{"amount":{"min":0.001,"max":10},"leverage":{"min":1,"max":50}},
        "info":{"price_range":0.03}})
}

fn ticker() -> Value {
    json!({"id":"PERP_BTC_USDT","symbol":"BTCUSDT","bid":99.9,"ask":100.1,"last":100,"mark":100,
        "index":100,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,
        "quoteVolume":0})
}

fn typed<T: serde::de::DeserializeOwned>(v: Value) -> T {
    serde_json::from_value(v).expect("fixture shape")
}

fn seed(h: &Harness, orders: Vec<Value>, positions: Vec<Value>) {
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(vec![typed(market())]),
            tickers: Some(vec![typed(ticker())]),
            orders: Some(orders.into_iter().map(typed).collect()),
            positions: Some(positions.into_iter().map(typed).collect()),
            ..Default::default()
        },
        false,
    )
    .expect("seed store");
}

fn one_way(http: &Http) {
    http.ok("GET", "/v3/accountinfo", json!({"positionMode": "ONE_WAY"}));
}

fn hedged(http: &Http) {
    http.ok(
        "GET",
        "/v3/accountinfo",
        json!({"positionMode": "HEDGE_MODE"}),
    );
}

fn place(h: &Harness, opts: Value) -> Result<Vec<String>, String> {
    let opts: PlaceOrderOpts = typed(opts);
    h.drive(Box::pin(h.venue().base().place_order(h.venue(), opts)))
        .map_err(|e| e.message)
}

fn bodies(h: &Harness, method: &str, path: &str) -> Vec<Value> {
    h.http
        .calls(path)
        .into_iter()
        .filter(|r| r.method == method)
        .map(|r| r.data)
        .collect()
}

const BROKER_ID: &str = "ab82cb09-cfec-4473-80a3-b740779d0644";
const BROKER: &str = "broker_id=ab82cb09-cfec-4473-80a3-b740779d0644";

fn order(id: &str, parent: Option<&str>, ty: &str, side: &str, price: f64, amount: f64) -> Value {
    let mut o = json!({"id":id,"status":"open","symbol":"BTCUSDT","type":ty,"side":side,
        "price":price,"amount":amount,"filled":0,"remaining":amount,"reduceOnly":true,"lastUpdate":0});
    if let Some(parent) = parent {
        o["parentId"] = json!(parent);
    }
    o
}

fn long_position(contracts: f64) -> Value {
    json!({"id":"BTCUSDT","symbol":"BTCUSDT","side":"long","entryPrice":95,"notional":190,
        "leverage":10,"unrealizedPnl":0,"contracts":contracts,"liquidationPrice":0})
}

fn log(h: &Harness) -> String {
    format!(
        "{:#?}\nreports: {:?}",
        h.http.requests.borrow(),
        h.runtime.reports.borrow()
    )
}

fn algo_ok(http: &Http, ids: &[u64]) {
    for id in ids {
        http.ok("POST", "/v3/algo/order", json!({"rows": [{"orderId": id}]}));
    }
}

fn place_ok(http: &Http, ids: &[u64]) {
    for id in ids {
        http.route(
            "POST",
            "/v1/order",
            200.0,
            json!({"success": true, "order_id": id}),
        );
    }
}

fn book(http: &Http, ask: f64, bid: f64) {
    http.route(
        "GET",
        "/v1/public/orderbook/PERP_BTC_USDT",
        200.0,
        json!({"success": true, "asks": [{"price": ask}], "bids": [{"price": bid}]}),
    );
}

// ---- placeOrder ----

#[test]
fn place_limit_order_posts_one_lot() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[123]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1.5,"price":99.5,"reduceOnly":false}),
    );
    assert_eq!(ids, Ok(vec!["123".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![json!(format!(
            "{BROKER}&order_price=99.5&order_quantity=1.5&order_type=LIMIT&reduce_only=false&side=BUY&symbol=PERP_BTC_USDT"
        ))]
    );
}

#[test]
fn place_market_order_prices_an_ioc_from_the_touch() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        book(http, 101.5, 99.0);
        place_ok(http, &[7]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}),
    );
    assert_eq!(ids, Ok(vec!["7".to_string()]), "{}", log(&h));
    // fair 100 * 1.009 misses the 101.5 ask, which is inside the 3% WOO cap.
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![json!(format!(
            "{BROKER}&order_price=101.5&order_quantity=1&order_type=IOC&side=BUY&symbol=PERP_BTC_USDT"
        ))]
    );
}

#[test]
fn place_market_order_falls_back_to_fair_slippage() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        book(http, 100.2, 99.8);
        place_ok(http, &[8]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":1,"maxSlippagePct":0.5}),
    );
    assert_eq!(ids, Ok(vec!["8".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![json!(format!(
            "{BROKER}&order_price=99.5&order_quantity=1&order_type=IOC&side=SELL&symbol=PERP_BTC_USDT"
        ))]
    );
}

#[test]
fn post_only_crossing_fair_price_is_refused_before_any_read() {
    let h = harness(ExchangeName::Woo, options(), one_way);
    seed(&h, vec![], vec![]);
    let result = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100.5,"timeInForce":"PostOnly"}),
    );
    assert_eq!(
        result,
        Err("WOO post-only buy order would cross current fair price 100".to_string())
    );
    assert!(h.http.requests.borrow().is_empty(), "{}", log(&h));
}

#[test]
fn post_only_crossing_the_touch_is_refused() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        book(http, 99.8, 99.7);
    });
    seed(&h, vec![], vec![]);
    let result = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":99.9,"timeInForce":"PostOnly"}),
    );
    assert_eq!(
        result,
        Err("WOO post-only buy order would cross current ask 99.8".to_string()),
        "{}",
        log(&h)
    );
    assert!(bodies(&h, "POST", "/v1/order").is_empty());
}

#[test]
fn post_only_resting_order_is_sent_as_post_only() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        book(http, 100.1, 99.9);
        place_ok(http, &[9]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":99.5,"timeInForce":"PostOnly"}),
    );
    assert_eq!(ids, Ok(vec!["9".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![json!(format!(
            "{BROKER}&order_price=99.5&order_quantity=1&order_type=POST_ONLY&side=BUY&symbol=PERP_BTC_USDT"
        ))]
    );
}

#[test]
fn caller_client_order_id_is_sent_verbatim() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[10]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"sell","amount":1,"price":101,"clientOrderId":"9007199254740991"}),
    );
    assert_eq!(ids, Ok(vec!["10".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![json!(format!(
            "{BROKER}&client_order_id=9007199254740991&order_price=101&order_quantity=1&order_type=LIMIT&side=SELL&symbol=PERP_BTC_USDT"
        ))]
    );
}

#[test]
fn invalid_client_order_id_is_rejected_before_any_request() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[10]);
    });
    seed(&h, vec![], vec![]);
    let result = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"sell","amount":1,"price":101,"clientOrderId":"01"}),
    );
    assert_eq!(
        result,
        Err("WOO clientOrderId must match /^[1-9]\\d*$/".to_string())
    );
    assert!(h.http.requests.borrow().is_empty(), "{}", log(&h));
}

#[test]
fn large_order_is_split_into_max_size_lots_plus_remainder() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[1, 2, 3, 4]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":25,"price":99}),
    );
    assert_eq!(
        ids,
        Ok(vec!["1".into(), "2".into(), "3".into(), "4".into()]),
        "{}",
        log(&h)
    );
    let lot = |q: &str| {
        json!(format!(
            "{BROKER}&order_price=99&order_quantity={q}&order_type=LIMIT&side=BUY&symbol=PERP_BTC_USDT"
        ))
    };
    // ts: lots = ceil(25 / 10) = 3, rest = 25 % 10 = 5, lotSize = adjust(20 / 3) = 6.667.
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![lot("6.667"), lot("6.667"), lot("6.667"), lot("5")]
    );
}

#[test]
fn caller_client_order_id_on_a_split_order_is_refused_before_posting() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[1]);
    });
    seed(&h, vec![], vec![]);
    let result = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":25,"price":99,"clientOrderId":"42"}),
    );
    assert!(result.is_err(), "{result:?}");
    assert!(bodies(&h, "POST", "/v1/order").is_empty(), "{}", log(&h));
}

#[test]
fn hedged_limit_order_carries_position_side() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        hedged(http);
        place_ok(http, &[11]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":99,"reduceOnly":true}),
    );
    assert_eq!(ids, Ok(vec!["11".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v1/order"),
        vec![json!(format!(
            "{BROKER}&order_price=99&order_quantity=1&order_type=LIMIT&position_side=SHORT&reduce_only=true&side=BUY&symbol=PERP_BTC_USDT"
        ))]
    );
}

#[test]
fn limit_order_with_tp_sl_places_a_positional_sibling() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[123]);
        http.ok(
            "POST",
            "/v3/algo/order",
            json!({"rows": [{"orderId": 7}, {"orderId": 8}]}),
        );
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":99,"stopLoss":90,"takeProfit":110,"tpTriggerType":"last_price"}),
    );
    assert_eq!(
        ids,
        Ok(vec!["123".into(), "7".into(), "8".into()]),
        "{}",
        log(&h)
    );
    assert_eq!(
        bodies(&h, "POST", "/v3/algo/order"),
        vec![
            json!({"reduceOnly":true,"symbol":"PERP_BTC_USDT","algoType":"POSITIONAL_TP_SL","childOrders":[
            {"reduceOnly":true,"algoType":"STOP_LOSS","type":"CLOSE_POSITION","triggerPriceType":"MARK_PRICE","side":"SELL","triggerPrice":"90"},
            {"reduceOnly":true,"algoType":"TAKE_PROFIT","type":"CLOSE_POSITION","triggerPriceType":"MARKET_PRICE","side":"SELL","triggerPrice":"110"}],"brokerId":BROKER_ID})
        ]
    );
}

// ---- algo placement ----

#[test]
fn stop_market_with_amount_posts_a_stop_algo() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        hedged(http);
        algo_ok(http, &[31]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":95.04,"slTriggerType":"last_price"}),
    );
    assert_eq!(ids, Ok(vec!["31".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v3/algo/order"),
        vec![
            json!({"symbol":"PERP_BTC_USDT","type":"MARKET","algoType":"STOP","side":"SELL",
            "triggerPrice":95.0,"triggerPriceType":"MARKET_PRICE","reduceOnly":true,"positionSide":"LONG","quantity":1.0,
            "brokerId":BROKER_ID})
        ]
    );
}

#[test]
fn stop_market_lots_are_split_and_flattened() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        algo_ok(http, &[41, 42, 43]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":12,"price":110}),
    );
    assert_eq!(
        ids,
        Ok(vec!["41".into(), "42".into(), "43".into()]),
        "{}",
        log(&h)
    );
    let quantities: Vec<Value> = bodies(&h, "POST", "/v3/algo/order")
        .iter()
        .map(|b| b["quantity"].clone())
        .collect();
    // ts: lots = ceil(12 / 10) = 2, rest = 2, lotSize = adjust(10 / 2) = 5.
    assert_eq!(quantities, vec![json!(5.0), json!(5.0), json!(2.0)]);
}

#[test]
fn take_profit_without_amount_is_positional() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        algo_ok(http, &[51]);
    });
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":110}),
    );
    assert_eq!(ids, Ok(vec!["51".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v3/algo/order"),
        vec![
            json!({"reduceOnly":true,"symbol":"PERP_BTC_USDT","algoType":"POSITIONAL_TP_SL","childOrders":[
            {"reduceOnly":true,"algoType":"TAKE_PROFIT","type":"CLOSE_POSITION","triggerPriceType":"MARK_PRICE","side":"SELL","triggerPrice":"110"}],"brokerId":BROKER_ID})
        ]
    );
}

#[test]
fn positional_take_profit_absorbs_and_cancels_an_open_stop_sibling() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        http.ok("DELETE", "/v3/algo/order/900", json!({}));
        algo_ok(http, &[52, 53]);
    });
    seed(
        &h,
        vec![order("901", Some("900"), "stop_market", "sell", 90.0, 0.0)],
        vec![],
    );
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":110}),
    );
    assert_eq!(ids, Ok(vec!["52".to_string()]), "{}", log(&h));
    assert_eq!(h.http.calls("/v3/algo/order/900").len(), 1);
    let children = bodies(&h, "POST", "/v3/algo/order")[0]["childOrders"].clone();
    assert_eq!(
        children
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["algoType"].clone(), c["triggerPrice"].clone()))
            .collect::<Vec<_>>(),
        vec![
            (json!("STOP_LOSS"), json!("90")),
            (json!("TAKE_PROFIT"), json!("110"))
        ]
    );
}

#[test]
fn already_canceled_sibling_is_marked_canceled_and_placement_continues() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        http.route(
            "DELETE",
            "/v3/algo/order/900",
            400.0,
            json!({"success": false, "message": "order already canceled"}),
        );
        algo_ok(http, &[54]);
    });
    seed(
        &h,
        vec![order("901", Some("900"), "stop_market", "sell", 90.0, 0.0)],
        vec![],
    );
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":110}),
    );
    assert_eq!(ids, Ok(vec!["54".to_string()]), "{}", log(&h));
    let status = serde_json::to_value(h.store.orders().unwrap()[0].status).unwrap();
    assert_eq!(status, json!("canceled"));
}

#[test]
fn algo_order_without_price_is_reported_and_places_nothing() {
    let h = harness(ExchangeName::Woo, options(), one_way);
    seed(&h, vec![], vec![]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1}),
    );
    assert_eq!(ids, Ok(vec![]), "{}", log(&h));
    assert!(h.http.requests.borrow().is_empty());
}

#[test]
fn trailing_stop_posts_callback_distance_and_position_size() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        algo_ok(http, &[61]);
    });
    seed(&h, vec![], vec![long_position(2.0)]);
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"sell","amount":2,"price":95}),
    );
    assert_eq!(ids, Ok(vec!["61".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "POST", "/v3/algo/order"),
        vec![
            json!({"symbol":"PERP_BTC_USDT","type":"MARKET","algoType":"TRAILING_STOP","callbackValue":"5",
            "side":"SELL","reduceOnly":true,"quantity":"2","triggerPriceType":"MARK_PRICE","brokerId":BROKER_ID})
        ]
    );
}

#[test]
fn trailing_stop_without_a_position_is_refused() {
    let h = harness(ExchangeName::Woo, options(), one_way);
    seed(&h, vec![], vec![]);
    let result = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"sell","amount":2,"price":95}),
    );
    assert_eq!(
        result,
        Err("Position BTCUSDT and side long not found".to_string())
    );
    assert!(bodies(&h, "POST", "/v3/algo/order").is_empty());
}

// ---- updateOrder ----

fn update(h: &Harness, order: Value, update: Value) -> Result<Vec<String>, String> {
    let opts = typed(json!({"order": order, "update": update}));
    h.drive(h.venue().update_order(opts)).map_err(|e| e.message)
}

#[test]
fn update_limit_order_always_sends_quantity() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("PUT", "/v3/order/555", json!({}));
    });
    seed(&h, vec![], vec![]);
    let ids = update(
        &h,
        order("555", None, "limit", "buy", 99.0, 1.0),
        json!({"price": 101}),
    );
    assert_eq!(ids, Ok(vec!["555".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "PUT", "/v3/order/555"),
        vec![json!({"orderId":"555","price":"101","quantity":"1"})]
    );
}

#[test]
fn update_limit_order_amount_only() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("PUT", "/v3/order/556", json!({}));
    });
    seed(&h, vec![], vec![]);
    let ids = update(
        &h,
        order("556", None, "limit", "buy", 99.0, 1.0),
        json!({"amount": 3}),
    );
    assert_eq!(ids, Ok(vec!["556".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "PUT", "/v3/order/556"),
        vec![json!({"orderId":"556","quantity":"3"})]
    );
}

#[test]
fn update_limit_order_failure_is_rethrown() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "PUT",
            "/v3/order/557",
            400.0,
            json!({"success": false, "message": "bad edit"}),
        );
    });
    seed(&h, vec![], vec![]);
    let result = update(
        &h,
        order("557", None, "limit", "buy", 99.0, 1.0),
        json!({"price": 98}),
    );
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn update_algo_order_with_amount_moves_trigger_price() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("PUT", "/v3/algo/order/900", json!({}));
    });
    seed(&h, vec![], vec![]);
    let ids = update(
        &h,
        order("901", Some("900"), "stop_market", "sell", 90.0, 1.0),
        json!({"price": 96}),
    );
    assert_eq!(ids, Ok(vec!["901".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "PUT", "/v3/algo/order/900"),
        vec![json!({"triggerPrice":"96"})]
    );
}

#[test]
fn update_positional_child_moves_its_child_order() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("PUT", "/v3/algo/order/900", json!({}));
    });
    seed(&h, vec![], vec![]);
    let ids = update(
        &h,
        order("902", Some("900"), "take_profit_market", "sell", 110.0, 0.0),
        json!({"price": 112}),
    );
    assert_eq!(ids, Ok(vec!["902".to_string()]), "{}", log(&h));
    assert_eq!(
        bodies(&h, "PUT", "/v3/algo/order/900"),
        vec![json!({"childOrders":[{"algoOrderId":"902","triggerPrice":"112"}]})]
    );
}

#[test]
fn update_trailing_stop_cancels_and_replaces() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        http.ok("DELETE", "/v3/algo/order/700", json!({}));
        algo_ok(http, &[71]);
    });
    seed(&h, vec![], vec![long_position(2.0)]);
    let ids = update(
        &h,
        order(
            "701",
            Some("700"),
            "trailing_stop_market",
            "sell",
            95.0,
            2.0,
        ),
        json!({"price": 97}),
    );
    assert_eq!(ids, Ok(vec!["71".to_string()]), "{}", log(&h));
    let methods: Vec<(String, String)> = h
        .http
        .requests
        .borrow()
        .iter()
        .filter(|r| r.url != "/v3/accountinfo")
        .map(|r| (r.method.clone(), r.url.clone()))
        .collect();
    assert_eq!(
        methods,
        vec![
            ("DELETE".into(), "/v3/algo/order/700".into()),
            ("POST".into(), "/v3/algo/order".into())
        ]
    );
    assert_eq!(
        bodies(&h, "POST", "/v3/algo/order")[0]["callbackValue"],
        json!("3")
    );
}

// ---- cancelOrders ----

#[test]
fn cancel_orders_cancels_limits_and_each_algo_parent_once() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("DELETE", "/v1/order", json!({}));
        http.ok("DELETE", "/v3/algo/order/900", json!({}));
    });
    seed(&h, vec![], vec![]);
    let orders: Vec<Order> = vec![
        typed(order("123", None, "limit", "buy", 99.0, 1.0)),
        typed(order("901", Some("900"), "stop_market", "sell", 90.0, 0.0)),
        typed(order(
            "902",
            Some("900"),
            "take_profit_market",
            "sell",
            110.0,
            0.0,
        )),
    ];
    h.drive(h.venue().cancel_orders(orders)).expect("cancel");
    let urls: Vec<(String, String)> = h
        .http
        .requests
        .borrow()
        .iter()
        .map(|r| (r.method.clone(), r.url.clone()))
        .collect();
    assert_eq!(
        urls,
        vec![
            ("DELETE".into(), "/v1/order".into()),
            ("DELETE".into(), "/v3/algo/order/900".into())
        ]
    );
    assert_eq!(
        bodies(&h, "DELETE", "/v1/order"),
        vec![json!("order_id=123&symbol=PERP_BTC_USDT")]
    );
}

#[test]
fn cancel_orders_failure_is_rethrown() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "DELETE",
            "/v1/order",
            400.0,
            json!({"success": false, "message": "unknown order"}),
        );
    });
    seed(&h, vec![], vec![]);
    let orders: Vec<Order> = vec![typed(order("123", None, "limit", "buy", 99.0, 1.0))];
    assert!(h.drive(h.venue().cancel_orders(orders)).is_err());
}

// ---- reads ----

const LIMIT_PAGE: &str = "/v1/orders";
const ALGO_PAGE: &str = "/v3/algo/orders";

fn queries(h: &Harness, path: &str) -> Vec<Option<String>> {
    h.http.calls(path).into_iter().map(|r| r.query).collect()
}

#[test]
fn fetch_orders_concatenates_limit_then_algo_rows() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            LIMIT_PAGE,
            200.0,
            json!({"success": true, "rows": [{"order_id": 11, "symbol": "PERP_BTC_USDT", "side": "BUY",
                "type": "LIMIT", "price": 99, "quantity": 2, "executed": 0, "status": "NEW",
                "reduce_only": false}], "meta": {"total": 1, "records_per_page": 100}}),
        );
        http.route(
            "GET",
            ALGO_PAGE,
            200.0,
            json!({"success": true, "data": {"rows": [{"algoOrderId": 21, "rootAlgoOrderId": 20,
                "symbol": "PERP_BTC_USDT", "algoType": "STOP_LOSS", "side": "SELL", "triggerPrice": 90,
                "quantity": 1, "triggerPriceType": "MARK_PRICE", "reduceOnly": true,
                "totalExecutedQuantity": 0}], "meta": {"total": 1, "records_per_page": 100}}}),
        );
    });
    seed(&h, vec![], vec![]);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("orders");
    let summary: Vec<Value> = orders
        .iter()
        .map(|o| {
            json!({"id": o.id, "parentId": o.parent_id, "type": o.r#type, "side": o.side,
                "price": o.price, "amount": o.amount})
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            json!({"id":"11","parentId":null,"type":"limit","side":"buy","price":99.0,"amount":2.0}),
            json!({"id":"21","parentId":"20","type":"stop_market","side":"sell","price":90.0,"amount":1.0}),
        ],
        "{}",
        log(&h)
    );
    // v1 signing sorts the query; v3 keeps qs insertion order.
    assert_eq!(
        queries(&h, LIMIT_PAGE),
        vec![Some("page=1&status=INCOMPLETE".into())]
    );
    assert_eq!(
        queries(&h, ALGO_PAGE),
        vec![Some("status=INCOMPLETE&page=1".into())]
    );
}

#[test]
fn fetch_orders_accepts_documented_null_legacy_metadata_through_the_binding() {
    let row = json!({"order_id":11,"symbol":"PERP_BTC_USDT","side":"BUY",
        "type":"LIMIT","price":99,"quantity":2,"executed":0,"status":"NEW",
        "reduce_only":false,"amount":null,"client_order_id":null,
        "realized_pnl":null,"average_executed_price":null});
    let h =
        harness(ExchangeName::Woo, options(), |http| {
            http.route("GET", LIMIT_PAGE, 200.0,
            json!({"success":true,"rows":[row.clone()],"meta":{"total":1,"records_per_page":100}}));
            http.route("GET", ALGO_PAGE, 200.0,
            json!({"success":true,"data":{"rows":[],"meta":{"total":0,"records_per_page":100}}}));
        });
    seed(&h, vec![], vec![]);
    let orders = h
        .drive(h.venue().fetch_orders(None))
        .expect("legacy nullable order read");
    assert_eq!(
        orders.len(),
        1,
        "documented nullable metadata must not drop the order: {}",
        log(&h)
    );
    let order = &orders[0];
    assert_eq!(order.id, "11");
    assert_eq!(order.symbol, "BTCUSDT");
    assert_eq!(
        (
            order.price.to_number(),
            order.amount,
            order.filled,
            order.remaining
        ),
        (99.0, 2.0, 0.0, 2.0)
    );
    let info = order.info.as_ref().expect("original raw order metadata");
    for field in [
        "amount",
        "client_order_id",
        "realized_pnl",
        "average_executed_price",
    ] {
        assert_eq!(
            info.get(field),
            Some(&safe_cex_types::retained_json::RetainedJsonInput::Null),
            "retained own null {field}"
        );
    }
    assert_eq!(h.http.calls(LIMIT_PAGE).len(), 1);
}

#[test]
fn fetch_orders_read_failure_yields_the_other_side() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            LIMIT_PAGE,
            500.0,
            json!({"success": false, "message": "boom"}),
        );
        http.route("GET", ALGO_PAGE, 200.0, json!({"success": true, "data": {"rows": [], "meta": {"total": 0, "records_per_page": 100}}}));
    });
    seed(&h, vec![], vec![]);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("orders");
    assert!(orders.is_empty());
    assert_eq!(h.http.calls(LIMIT_PAGE).len(), 1);
    assert_eq!(h.http.calls(ALGO_PAGE).len(), 1, "{}", log(&h));
}

fn history_row(id: u64, created: &str) -> Value {
    json!({"order_id": id, "symbol": "PERP_BTC_USDT", "side": "BUY", "type": "LIMIT",
        "status": "FILLED", "price": "99", "quantity": "1", "executed": "1",
        "created_time": created, "realized_pnl": "0", "position_side": "LONG"})
}

fn history_query(page: u32) -> Option<String> {
    Some(format!(
        "end_t=2&page={page}&realized_pnl=true&size=100&start_t=1&symbol=PERP_BTC_USDT"
    ))
}

#[test]
fn fetch_historical_orders_pages_sorts_and_limits() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            "/v1/orders",
            200.0,
            json!({"success": true, "rows": [history_row(1, "1700000000.5"), history_row(2, "1700000100.25")],
                "meta": {"total": 3, "records_per_page": 2}}),
        );
        http.route(
            "GET",
            "/v1/orders",
            200.0,
            json!({"success": true, "rows": [history_row(3, "1700000050"), history_row(2, "1700000100.25")],
                "meta": {"total": 3, "records_per_page": 2}}),
        );
    });
    seed(&h, vec![], vec![]);
    let opts = typed(json!({"symbol": "BTCUSDT", "from": 1, "to": 2, "limit": 2}));
    let orders = h
        .drive(h.venue().fetch_historical_orders(Some(opts)))
        .expect("history");
    let ids: Vec<(String, f64, String)> = orders
        .iter()
        .map(|o| (o.id.clone(), o.created_time, o.symbol.clone()))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("2".into(), 1_700_000_100_250.0, "BTCUSDT".into()),
            ("3".into(), 1_700_000_050_000.0, "BTCUSDT".into())
        ],
        "{}",
        log(&h)
    );
    assert_eq!(orders[0].realized_pnl, Some(0.0));
    assert_eq!(
        queries(&h, "/v1/orders"),
        vec![history_query(1), history_query(2)]
    );
}

#[test]
fn fetch_historical_orders_unknown_symbol_reads_nothing() {
    let h = harness(ExchangeName::Woo, options(), |_| {});
    seed(&h, vec![], vec![]);
    let opts = typed(json!({"symbol": "ETHUSDT"}));
    let orders = h
        .drive(h.venue().fetch_historical_orders(Some(opts)))
        .expect("history");
    assert!(orders.is_empty());
    assert!(h.http.requests.borrow().is_empty());
}

#[test]
fn fetch_positions_reads_open_positions() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        http.ok(
            "GET",
            "/v3/positions",
            json!({"positions": [{"symbol": "PERP_BTC_USDT", "holding": 2, "averageOpenPrice": 95,
                "leverage": 10, "estLiqPrice": 50, "marginMode": "CROSS"}]}),
        );
    });
    seed(&h, vec![], vec![]);
    let positions: Vec<Position> = h
        .drive(h.venue().fetch_positions(Some(vec!["BTCUSDT".into()])))
        .expect("positions");
    assert_eq!(positions.len(), 1);
    let info = positions[0].info.value().expect("position info");
    let mut serialized = serde_json::to_value(&positions).unwrap();
    for (field, expected) in [
        ("holding", 2.0_f64),
        ("averageOpenPrice", 95.0),
        ("leverage", 10.0),
        ("estLiqPrice", 50.0),
    ] {
        assert_eq!(
            info.get(field).unwrap().as_f64().unwrap().to_bits(),
            expected.to_bits(),
            "{field}"
        );
        let serialized_number = serialized[0]["info"]
            .as_object_mut()
            .unwrap()
            .remove(field)
            .unwrap();
        assert_eq!(serialized_number.as_f64(), Some(expected), "{field}");
    }
    assert_eq!(
        serialized,
        json!([{"id":"BTCUSDT","symbol":"BTCUSDT","side":"long","entryPrice":95.0,"notional":200.0,
            "leverage":10.0,"unrealizedPnl":10.0,"contracts":2.0,"liquidationPrice":50.0,
            "positionMode":"oneway","marginType":"cross","info":{"symbol": "PERP_BTC_USDT", "marginMode": "CROSS"}}]),
        "{}",
        log(&h)
    );
}

#[test]
fn after_change_position_mode_in_worker_caches_the_hedged_mode() {
    // ts: afterMethodCalledInWorker sets _accountPositionMode from args[0].
    let h = harness(ExchangeName::Woo, options(), |http| {
        one_way(http);
        place_ok(http, &[11]);
    });
    seed(&h, vec![], vec![]);
    h.venue()
        .after_method_called_in_worker(
            &Value::Null,
            safe_cex_exchange::worker::CallableMethodsInWorker::ChangePositionMode,
            &[json!(true)],
        )
        .unwrap();
    let ids = place(
        &h,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":99,"reduceOnly":true}),
    );
    assert_eq!(ids, Ok(vec!["11".to_string()]), "{}", log(&h));
    assert!(
        bodies(&h, "POST", "/v1/order")[0]
            .as_str()
            .unwrap()
            .contains("position_side=SHORT"),
        "{}",
        log(&h)
    );
}

#[test]
fn cancel_all_orders_deletes_every_pending_order() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("DELETE", "/v3/orders/pending", json!({}));
    });
    h.drive(h.venue().cancel_all_orders()).unwrap();
    assert_eq!(h.http.calls("/v3/orders/pending").len(), 1);
}

#[test]
fn cancel_symbol_orders_deletes_by_venue_symbol_and_reports_failure() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "DELETE",
            "/v1/orders",
            400.0,
            json!({"success": false, "message": "symbol has no orders"}),
        );
    });
    let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, payload| {
            sink.borrow_mut().push(payload.to_display());
            Ok(())
        }),
    );
    // ts: the catch emits and resolves; the caller sees success.
    h.drive(h.venue().cancel_symbol_orders("BTCUSDT".into()))
        .unwrap();
    let calls = h.http.calls("/v1/orders");
    assert_eq!(calls.len(), 1);
    // ts: signV1 sends a mutation's params as the sorted form body.
    assert_eq!(
        bodies(&h, "DELETE", "/v1/orders"),
        vec![json!("symbol=PERP_BTC_USDT")]
    );
    assert!(
        errors
            .borrow()
            .iter()
            .any(|e| e.contains("symbol has no orders"))
    );
}

#[test]
fn fetch_orders_reports_raw_page_messages_and_preserves_completed_pages() {
    use safe_cex_exchange::js_runtime::value::to_json;
    use std::{cell::RefCell, rc::Rc};
    for (message, nested, expected) in [
        (json!(123), Value::Null, "123"),
        (
            json!({"reason":"venue rejection"}),
            Value::Null,
            "[object Object]",
        ),
        (json!(false), json!(["bad", 7]), "bad,7"),
        (json!(""), json!(true), "true"),
        (
            json!(0),
            Value::Null,
            "Exchange Error: Failed to fetch orders",
        ),
        (
            json!("venue rejection"),
            Value::Null,
            "Exchange Error: venue rejection",
        ),
    ] {
        let h = harness(ExchangeName::Woo, options(), |http| {
            http.route(
                "GET",
                LIMIT_PAGE,
                200.0,
                json!({"success":true,"rows":[{
                "order_id":11,"symbol":"PERP_BTC_USDT","side":"BUY","type":"LIMIT",
                "price":99,"quantity":2,"executed":0,"status":"NEW","reduce_only":false
            }],"meta":{"total":2,"records_per_page":1}}),
            );
            http.route(
                "GET",
                LIMIT_PAGE,
                200.0,
                json!({"success":false,"message":message,"meta":{"message":nested}}),
            );
            http.route("GET", ALGO_PAGE, 200.0, json!({"success":true,"data":{"rows":[],"meta":{"total":0,"records_per_page":100}}}));
        });
        seed(&h, vec![], vec![]);
        let errors = Rc::new(RefCell::new(vec![]));
        let saved = errors.clone();
        h.venue().base().on(
            "error",
            Rc::new(move |_, value| {
                saved.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        let orders = h
            .drive(h.venue().fetch_orders(None))
            .expect("source reports failed page and returns completed prefix");
        let events: Vec<_> = errors
            .borrow()
            .iter()
            .map(|v| to_json(v).unwrap())
            .collect();
        assert_eq!(
            events,
            vec![json!(expected)],
            "source error text for {message}"
        );
        assert_eq!(
            orders.len(),
            1,
            "raw error must not discard completed pages: {message}"
        );
        assert_eq!(orders[0].id, "11");
        assert_eq!(h.http.calls(LIMIT_PAGE).len(), 2);
        assert_eq!(h.http.calls(ALGO_PAGE).len(), 1);
    }
}
