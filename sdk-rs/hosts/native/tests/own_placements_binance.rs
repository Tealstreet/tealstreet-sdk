//! AGT-196 through real Binance USDⓈ-M frames: the real Binance engine places,
//! fills and triggers; its receipts, completions, `OrderTriggered` notices and
//! fills go through the same ledger calls the account actor makes.
//!
//! Frames follow Binance's published examples (USDⓈ-M "User Data Streams":
//! `ORDER_TRADE_UPDATE`, `ALGO_UPDATE`; "New Order" and "New Algo Order"
//! responses), with only ids, prices and states changed: docs-derived, not live
//! captures. Liquidation and ADL client ids are the documented `autoclose-` and
//! `adl_autoclose` prefixes.
#![cfg(not(target_arch = "wasm32"))]

#[path = "../../../venues/binance/tests/support/mod.rs"]
mod support;

use safe_cex_engine::{Command, Event, Fill};
use safe_cex_native::own_placements::OwnPlacements;
use support::{Harness, Options};

const SYMBOL: &str = "BTCUSDT";

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.load_public();
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"audit-key"}"#);
    let socket = h.socket("/private/ws").unwrap();
    h.event(Event::WsOpened { socket });
    let _ = h.engine.take_frame(h.now);
    h
}

/// Place one order as the actor would, recording what the actor records.
fn place(h: &mut Harness, ledger: &mut OwnPlacements, command: u32, order: serde_json::Value) {
    let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_value(order).unwrap();
    let legs = [(order.symbol.clone(), order.client_order_id.clone())];
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![order],
    }));
    for (_, receipts) in h.progress.iter().filter(|(id, _)| *id == command) {
        ledger.record_receipts(&[SYMBOL.into()], &legs, receipts);
    }
}

fn complete(h: &Harness, ledger: &mut OwnPlacements, command: u32) {
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == command)
        .and_then(|(_, result)| result.clone().ok())
        .expect("the placement completed");
    // These placements carry no caller id: Binance's are engine-generated.
    ledger.record_completion(&[SYMBOL.into()], &[(SYMBOL.into(), None)], &result);
}

fn query(url: &str, name: &str) -> String {
    url.split(['?', '&'])
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("{name} in {url}"))
        .to_owned()
}

/// The fills one private frame produced, after its link notices applied.
fn fills(h: &mut Harness, ledger: &mut OwnPlacements, frame: &str) -> Vec<Fill> {
    h.message("/private/ws", frame);
    for (symbol, order_id, by) in h.triggered.drain(..) {
        ledger.link(&symbol, &order_id, &by);
    }
    h.engine
        .take_frame(h.now)
        .and_then(safe_cex_engine::wire::fills)
        .unwrap_or_default()
}

fn trade(client: &str, order_type: &str, id: u64, trade: u64) -> String {
    format!(
        r#"{{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{{"s":"BTCUSDT","c":"{client}","S":"SELL","o":"{order_type}","f":"GTC","q":"0.1","p":"0","ap":"100","sp":"0","x":"TRADE","X":"FILLED","i":{id},"l":"0.1","z":"0.1","L":"100","N":"USDT","n":"0","T":1568879465650,"t":{trade},"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"{order_type}","ps":"BOTH","cp":false,"rp":"0","pP":false,"si":0,"ss":0,"V":"EXPIRE_TAKER","pm":"NONE","gtd":0}}}}"#
    )
}

fn claimed(ledger: &OwnPlacements, fills: &[Fill]) -> Vec<bool> {
    fills
        .iter()
        .map(|fill| ledger.placed(&fill.symbol, &fill.order_id))
        .collect()
}

/// A limit this login placed is claimed through its real fill frame; a
/// liquidation and an ADL fill on the same account never are.
#[test]
fn a_placed_limit_is_claimed_and_liquidation_and_adl_fills_are_not() {
    let mut h = ready();
    let mut ledger = OwnPlacements::default();
    place(
        &mut h,
        &mut ledger,
        7,
        serde_json::json!({"symbol": SYMBOL, "type": "limit", "side": "sell", "amount": 0.1, "price": 100.0}),
    );
    let order = h.take("/fapi/v1/order");
    let client = query(&order.url, "newClientOrderId");
    h.reply(
        order.id,
        200,
        "",
        &format!(
            r#"{{"clientOrderId":"{client}","cumQty":"0","cumQuote":"0","executedQty":"0","orderId":22542179,"avgPrice":"0.00000","origQty":"0.1","price":"100","reduceOnly":false,"side":"SELL","positionSide":"BOTH","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC","type":"LIMIT","origType":"LIMIT","updateTime":1566818724722}}"#
        ),
    );
    complete(&h, &mut ledger, 7);

    let mut seen = fills(&mut h, &mut ledger, &trade(&client, "LIMIT", 22542179, 1));
    seen.extend(fills(
        &mut h,
        &mut ledger,
        &trade("autoclose-1568879465650", "LIQUIDATION", 22542180, 2),
    ));
    seen.extend(fills(
        &mut h,
        &mut ledger,
        &trade("adl_autoclose", "MARKET", 22542181, 3),
    ));
    let ids: Vec<&str> = seen.iter().map(|fill| fill.order_id.as_str()).collect();
    assert_eq!(
        ids,
        ["22542179", "22542180", "22542181"],
        "each frame filled"
    );
    assert_eq!(claimed(&ledger, &seen), [true, false, false]);
}

/// Item 6: a conditional placed through the Algo Order API fires as a new
/// matching-engine order; `ALGO_UPDATE.ai` names it, so its fill is the
/// placer's. Another algo's trigger links nothing here.
#[test]
fn a_fired_algo_orders_fill_is_claimed_through_its_algo() {
    let mut h = ready();
    let mut ledger = OwnPlacements::default();
    let mark = h.engine.store().ticker(SYMBOL).unwrap().mark;
    place(
        &mut h,
        &mut ledger,
        8,
        serde_json::json!({"symbol": SYMBOL, "type": "conditional", "side": "buy",
            "amount": 0.1, "triggerPrice": mark * 1.1}),
    );
    let algo = h.take("/fapi/v1/algoOrder");
    let client = query(&algo.url, "clientAlgoId");
    h.reply(
        algo.id,
        200,
        "",
        &format!(
            r#"{{"algoId":2148719,"clientAlgoId":"{client}","algoType":"CONDITIONAL","orderType":"STOP_MARKET","symbol":"BTCUSDT","side":"BUY","positionSide":"BOTH","timeInForce":"GTC","quantity":"0.1","algoStatus":"NEW","triggerPrice":"110","price":"0","reduceOnly":false,"workingType":"CONTRACT_PRICE","createTime":1750485492076,"updateTime":1750485492076}}"#
        ),
    );
    complete(&h, &mut ledger, 8);

    let fired = |aid: u64, caid: &str, ai: &str| {
        format!(
            r#"{{"e":"ALGO_UPDATE","T":1750515742297,"E":1750515742303,"o":{{"caid":"{caid}","aid":{aid},"at":"CONDITIONAL","o":"STOP_MARKET","s":"BTCUSDT","S":"BUY","ps":"BOTH","f":"GTC","q":"0.1","X":"TRIGGERED","ai":"{ai}","ap":"0.00000","aq":"0.00000","act":"0","tp":"110","p":"0","V":"EXPIRE_MAKER","wt":"CONTRACT_PRICE","pm":"NONE","cp":false,"pP":false,"R":false,"tt":0,"gtd":0}}}}"#
        )
    };
    assert!(fills(&mut h, &mut ledger, &fired(2148719, &client, "8886774")).is_empty());
    assert!(
        fills(
            &mut h,
            &mut ledger,
            &fired(2148720, "someone-else", "8886775")
        )
        .is_empty()
    );
    let mut seen = fills(&mut h, &mut ledger, &trade("", "MARKET", 8886774, 4));
    seen.extend(fills(&mut h, &mut ledger, &trade("", "MARKET", 8886775, 5)));
    let ids: Vec<&str> = seen.iter().map(|fill| fill.order_id.as_str()).collect();
    assert_eq!(ids, ["8886774", "8886775"]);
    assert_eq!(claimed(&ledger, &seen), [true, false]);
}

// Binance portfolio margin's documented `/papi` startup payloads, as
// `venues/binance/tests/portfolio_margin.rs` answers them (docs-derived).
const PM_POSITION_RISK: &str = r#"[{"entryPrice":"95000.0","leverage":"20","markPrice":"96000.0","maxNotionalValue":"6000000.0","positionAmt":"0.010","notional":"960.0","symbol":"BTCUSDT","unRealizedProfit":"10.0","liquidationPrice":"80000.5","positionSide":"BOTH","updateTime":1720736417660}]"#;
const PM_SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":"false","leverage":20,"maxNotionalValue":"6000000"}]"#;
const PM_BALANCE: &str = r#"[{"asset":"USDT","totalWalletBalance":"1000.0","crossMarginAsset":"0","crossMarginBorrowed":"0","crossMarginFree":"900.0","crossMarginInterest":"0","crossMarginLocked":"0","umWalletBalance":"1000.0","umUnrealizedPNL":"10.0","cmWalletBalance":"0","cmUnrealizedPNL":"0","updateTime":0,"negativeBalance":"0"}]"#;
const PM_OPEN_ORDERS: &str = r#"[{"avgPrice":"0.00000","clientOrderId":"x-s3vFFRQpresting","cumQuote":"0","executedQty":"0","orderId":1917641,"origQty":"0.40","origType":"LIMIT","price":"90000","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"NEW","symbol":"BTCUSDT","time":1579276756075,"timeInForce":"GTC","type":"LIMIT","updateTime":1579276756075,"selfTradePreventionMode":"NONE","goodTillDate":0}]"#;

fn pm_ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        portfolio_margin: true,
        ..Options::default()
    });
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(
        "/papi/v1/um/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]"#,
    );
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"pmkey"}"#);
    h.respond(
        "/papi/v1/um/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/papi/v1/um/positionRisk", 200, PM_POSITION_RISK);
    h.respond("/papi/v1/um/symbolConfig", 200, PM_SYMBOL_CONFIG);
    h.respond("/papi/v1/balance", 200, PM_BALANCE);
    h.respond("/papi/v1/um/openOrders", 200, PM_OPEN_ORDERS);
    h.respond("/papi/v1/um/algo/openAlgoOrders", 200, "[]");
    for path in ["/public/ws", "/market/ws", "/pm/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h.answer_open_position_read();
    h
}

/// What the actor's `retire_closed_caller_ids` reads: every store row by id
/// and client id, with whether it is closed or canceled.
fn retire(h: &Harness, ledger: &mut OwnPlacements, now: std::time::Instant) {
    use safe_cex_engine::model::OrderStatus;
    let rows: Vec<(String, String, bool)> = h
        .engine
        .store()
        .orders()
        .iter()
        .flat_map(|order| {
            let terminal = matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled);
            std::iter::once(order.id.clone())
                .chain(order.client_order_id.clone())
                .map(move |identity| (order.symbol.clone(), identity, terminal))
        })
        .collect();
    let store = h.engine.store();
    ledger.retire_caller_ids(
        rows.iter()
            .map(|(symbol, identity, terminal)| (symbol.as_str(), identity.as_str(), *terminal)),
        |symbol| store.orders_reads(symbol),
        now,
    );
}

/// Review B (e6e681d490) and review A (5f75821a55) through real Binance
/// portfolio-margin frame shapes: PM keys orders by client id and allows reuse
/// after close. A caller's `desk` claims once the venue acks it. Only the
/// stream creates its row, and the stream drops between the ack and the NEW
/// frame: the order fills and closes in the gap, and the reconnect's
/// open-orders read omits it. The id stops claiming after `TERMINAL_GRACE`
/// instead of 24 h, though no row was ever seen.
#[test]
fn a_pm_caller_id_retires_when_the_reconnect_snapshot_drops_its_order() {
    use safe_cex_native::own_placements::TERMINAL_GRACE;
    let mut h = pm_ready();
    let mut ledger = OwnPlacements::default();
    let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_value(serde_json::json!({
        "symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 0.002,
        "price": 91000.0, "clientOrderId": "desk"
    }))
    .unwrap();
    let legs = [(SYMBOL.to_owned(), Some("desk".to_owned()))];
    h.event(Event::Command(Command::PlaceOrders {
        command: 1,
        orders: vec![order],
    }));
    for (_, receipts) in h.progress.iter().filter(|(id, _)| *id == 1) {
        assert!(
            ledger
                .record_receipts(&[SYMBOL.into()], &legs, receipts)
                .is_empty(),
            "a caller id is never claimed from its receipt"
        );
    }
    let sent = h.take("/papi/v1/um/order");
    assert_eq!(query(&sent.url, "newClientOrderId"), "desk");
    h.reply(
        sent.id,
        200,
        "",
        r#"{"clientOrderId":"desk","cumQty":"0","cumQuote":"0","executedQty":"0","orderId":8886774,"avgPrice":"0.00000","origQty":"0.002","price":"91000","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC","type":"LIMIT","origType":"LIMIT","updateTime":1566818724722}"#,
    );
    let result = h.result(1).cloned().expect("completed").expect("placed");
    ledger.record_completion(&[SYMBOL.into()], &legs, &result);
    assert!(
        ledger.placed(SYMBOL, "desk"),
        "acked: PM's order id is `desk`"
    );

    assert!(
        h.engine.store().order("desk").is_none(),
        "the REST ack stores no row"
    );
    let now = std::time::Instant::now();
    retire(&h, &mut ledger, now);
    let later = now + TERMINAL_GRACE;
    assert!(
        ledger.placed_at(SYMBOL, "desk", later),
        "no read since the ack: still claimed"
    );

    // The stream drops before `desk`'s NEW frame; it fills and closes unseen;
    // the reconnect re-reads.
    let old = h.socket("/pm/ws").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"pmkey2"}"#);
    let fresh = h.socket("/pm/ws").unwrap();
    assert_ne!(old, fresh);
    h.event(Event::WsOpened { socket: fresh });
    // Answer the reconnect's account re-read, whatever order it asks in.
    // Binance re-reads open orders on a timer after the reconnect.
    for _ in 0..8 {
        h.advance(5_000.0);
        for (path, body) in [
            ("/papi/v1/um/positionRisk", PM_POSITION_RISK),
            ("/papi/v1/um/symbolConfig", PM_SYMBOL_CONFIG),
            ("/papi/v1/balance", PM_BALANCE),
            ("/papi/v1/um/algo/openAlgoOrders", "[]"),
            ("/papi/v1/um/openOrders", PM_OPEN_ORDERS),
        ] {
            if h.has_request(path) {
                h.respond(path, 200, body);
            }
        }
    }
    assert!(
        h.engine.store().order("desk").is_none(),
        "the snapshot omits it"
    );
    retire(&h, &mut ledger, now);
    assert!(ledger.placed_at(SYMBOL, "desk", now), "its own last fills");
    assert!(
        !ledger.placed_at(SYMBOL, "desk", later),
        "a later reuse of `desk`"
    );
}

/// Binance spot's documented startup payloads (`venues/binance/tests/
/// spot_execution_delivery.rs`): one BTCUSDT market, the WebSocket API user
/// stream, no balances.
fn spot_ready() -> Harness {
    let mut h = Harness::started(Options {
        variant: safe_cex_venue_binance::Variant::Spot,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/api/v3/exchangeInfo", 200, r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}]}"#);
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"10000","bidPrice":"9999","askPrice":"10001","priceChangePercent":"0","volume":"1","quoteVolume":"10000"}]"#);
    let socket = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket });
    spot_reads(&mut h, 0.0);
    h
}

/// Answer the spot user stream's subscribe and its account and open-orders
/// reads (none open), however many rounds the engine asks for, advancing
/// `step_ms` before each round (a reconnect re-reads on a timer).
fn spot_reads(h: &mut Harness, step_ms: f64) {
    for _ in 0..8 {
        if step_ms > 0.0 {
            h.advance(step_ms);
        }
        for path in ["/api/v3/account", "/api/v3/openOrders"] {
            if h.has_request(path) {
                let body = if path.ends_with("account") {
                    r#"{"balances":[]}"#
                } else {
                    "[]"
                };
                h.respond(path, 200, body);
            }
        }
        let pending = h.sent.iter().rev().find_map(|(_, text)| {
            text.contains("userDataStream.subscribe.signature")
                .then(|| serde_json::from_str::<serde_json::Value>(text).ok())
                .flatten()
        });
        if let Some(id) = pending.as_ref().and_then(|sent| sent["id"].as_str()) {
            let reply = format!(r#"{{"id":"{id}","status":200,"result":{{"subscriptionId":0}}}}"#);
            h.sent.retain(|(_, text)| !text.contains(id));
            h.message("ws-api/v3", &reply);
        }
    }
}

/// Review A (5f75821a55) through real Binance spot frame shapes: spot keys
/// orders by client id and allows reuse after close. A caller's `desk` claims
/// once acked; the user stream drops before its `executionReport`, the order
/// fills and closes in the gap, and the reconnect's open-orders read omits it.
/// The id stops claiming after `TERMINAL_GRACE`.
#[test]
fn a_spot_caller_id_retires_when_the_reconnect_read_omits_its_order() {
    use safe_cex_native::own_placements::TERMINAL_GRACE;
    let mut h = spot_ready();
    let mut ledger = OwnPlacements::default();
    let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_value(serde_json::json!({
        "symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1.0,
        "price": 9000.0, "clientOrderId": "desk"
    }))
    .unwrap();
    let legs = [(SYMBOL.to_owned(), Some("desk".to_owned()))];
    h.event(Event::Command(Command::PlaceOrders {
        command: 1,
        orders: vec![order],
    }));
    for (_, receipts) in h.progress.iter().filter(|(id, _)| *id == 1) {
        assert!(
            ledger
                .record_receipts(&[SYMBOL.into()], &legs, receipts)
                .is_empty()
        );
    }
    let sent = h.take("/api/v3/order");
    assert_eq!(query(&sent.url, "newClientOrderId"), "desk");
    h.reply(
        sent.id,
        200,
        "",
        r#"{"symbol":"BTCUSDT","orderId":28,"orderListId":-1,"clientOrderId":"desk","transactTime":1507725176595,"price":"9000.00000000","origQty":"1.00000000","executedQty":"0.00000000","cummulativeQuoteQty":"0.00000000","status":"NEW","timeInForce":"GTC","type":"LIMIT","side":"BUY","workingTime":1507725176595,"selfTradePreventionMode":"NONE"}"#,
    );
    let result = h.result(1).cloned().expect("completed").expect("placed");
    ledger.record_completion(&[SYMBOL.into()], &legs, &result);
    assert!(
        ledger.placed(SYMBOL, "desk"),
        "acked: spot's order id is `desk`"
    );
    assert!(
        h.engine.store().order("desk").is_none(),
        "the REST ack stores no row"
    );
    let now = std::time::Instant::now();
    let later = now + TERMINAL_GRACE;
    retire(&h, &mut ledger, now);
    assert!(
        ledger.placed_at(SYMBOL, "desk", later),
        "no read since the ack"
    );

    let old = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    // The reconnect backs off before it dials again.
    for _ in 0..60 {
        if h.socket("ws-api/v3") != Some(old) {
            break;
        }
        h.advance(1_000.0);
    }
    let fresh = h.socket("ws-api/v3").unwrap();
    assert_ne!(old, fresh, "the user stream reconnected");
    h.event(Event::WsOpened { socket: fresh });
    spot_reads(&mut h, 5_000.0);
    assert!(
        h.engine.store().order("desk").is_none(),
        "the read omits it"
    );
    retire(&h, &mut ledger, now);
    assert!(ledger.placed_at(SYMBOL, "desk", now), "its own last fills");
    assert!(
        !ledger.placed_at(SYMBOL, "desk", later),
        "a later reuse of `desk`"
    );
}

/// Review B (1acdf351b7) through real Binance frame shapes: portfolio margin
/// and spot fills of two orders reusing the client id `desk` each carry their
/// own native order id (`i`), which the fill claim keys on.
#[test]
fn client_keyed_fills_name_each_orders_native_id() {
    let mut pm = pm_ready();
    for (native, trade) in [(8886774, 1), (8886775, 2)] {
        pm.message(
            "/pm/ws",
            &format!(
                r#"{{"fs":"UM","e":"ORDER_TRADE_UPDATE","T":1,"E":1,"o":{{"s":"BTCUSDT","c":"desk","S":"BUY","o":"LIMIT","f":"GTC","q":"0.002","p":"91000","ap":"91000","sp":"0","x":"TRADE","X":"FILLED","i":{native},"l":"0.002","z":"0.002","L":"91000","T":1,"t":{trade},"b":"0","a":"0","m":false,"R":false,"ps":"BOTH","rp":"0"}}}}"#
            ),
        );
    }
    assert_eq!(
        pm.native_ids,
        [
            (
                "BTCUSDT".to_owned(),
                "desk".to_owned(),
                "8886774".to_owned()
            ),
            (
                "BTCUSDT".to_owned(),
                "desk".to_owned(),
                "8886775".to_owned()
            ),
        ]
    );

    let mut spot = spot_ready();
    for (native, trade) in [(28, 101), (29, 102)] {
        spot.message(
            "ws-api/v3",
            &format!(
                r#"{{"subscriptionId":0,"event":{{"e":"executionReport","i":{native},"s":"BTCUSDT","c":"desk","S":"BUY","o":"LIMIT","f":"GTC","X":"NEW","x":"NEW","p":"9000","L":"0","q":"1","z":"0","l":"0"}}}}"#
            ),
        );
        spot.message(
            "ws-api/v3",
            &format!(
                r#"{{"subscriptionId":0,"event":{{"e":"executionReport","i":{native},"s":"BTCUSDT","c":"desk","S":"BUY","o":"LIMIT","f":"GTC","X":"FILLED","x":"TRADE","p":"9000","L":"9000","q":"1","z":"1","l":"1","t":{trade}}}}}"#
            ),
        );
    }
    assert_eq!(
        spot.native_ids,
        [
            ("BTCUSDT".to_owned(), "desk".to_owned(), "28".to_owned()),
            ("BTCUSDT".to_owned(), "desk".to_owned(), "29".to_owned()),
        ]
    );
}
