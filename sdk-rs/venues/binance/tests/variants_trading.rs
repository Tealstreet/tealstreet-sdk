use support::Options;
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event};
use safe_cex_venue_binance::Variant;
use support::Harness;

fn ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let (prefix, info) = if variant == Variant::Spot {
        (
            "/api/v3",
            r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"5"}]}]}"#,
        )
    } else {
        (
            "/dapi/v1",
            r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"5"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"5"}]}]}"#,
        )
    };
    h.respond(&format!("{prefix}/exchangeInfo"), 200, info);
    if variant == Variant::Spot {
        h.respond(&format!("{prefix}/ticker/24hr"),200,r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"1","volume":"2","quoteVolume":"200"}]"#);
    } else {
        h.respond(&format!("{prefix}/ticker/24hr"),200,r#"[{"symbol":"BTCUSD_PERP","lastPrice":"100","priceChangePercent":"1","volume":"2","baseVolume":"2"}]"#);
        h.respond(
            &format!("{prefix}/ticker/bookTicker"),
            200,
            r#"[{"symbol":"BTCUSD_PERP","bidPrice":"99","askPrice":"101"}]"#,
        );
        h.respond(&format!("{prefix}/premiumIndex"),200,r#"[{"symbol":"BTCUSD_PERP","markPrice":"100","indexPrice":"100","lastFundingRate":"0","nextFundingTime":1}]"#);
    }
    h.requests.clear();
    h
}
fn order(variant: Variant, fields: &str) -> PlaceOrderOpts {
    let symbol = if variant == Variant::Spot {
        "BTCUSDT"
    } else {
        "BTCUSD_PERP"
    };
    serde_json::from_str(&format!(
        r#"{{"symbol":"{symbol}","type":"limit","side":"buy","amount":2,"price":99,{fields}}}"#
    ))
    .unwrap()
}
fn place(h: &mut Harness, orders: Vec<PlaceOrderOpts>) {
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders,
    }));
}

#[test]
fn spot_post_only_and_native_identity() {
    let mut h = ready(Variant::Spot);
    place(
        &mut h,
        vec![order(
            Variant::Spot,
            r#""timeInForce":"PostOnly","clientOrderId":"caller_1""#,
        )],
    );
    let r = h.take("/api/v3/order");
    assert!(r.url.contains("type=LIMIT_MAKER"));
    assert!(!r.url.contains("timeInForce"));
    assert!(!r.url.contains("positionSide"));
    assert!(r.url.contains("newClientOrderId=caller_1"));
    h.reply(r.id, 200, "", r#"{"orderId":123}"#);
    assert_eq!(
        h.done.last().unwrap().1.as_ref().unwrap(),
        r#"["caller_1"]"#
    );
}
#[test]
fn invalid_second_spot_order_rejects_whole_batch_before_writes() {
    for fields in [
        r#""type":"trailing_stop_loss""#,
        r#""reduceOnly":true"#,
        r#""close":true"#,
        r#""clientOrderId":"illegal.dot""#,
        r#""type":"stop_loss","price":0"#,
    ] {
        let mut h = ready(Variant::Spot);
        // Merge fields using a deterministic object to avoid duplicate JSON fields.
        let mut bad = order(Variant::Spot, r#""reduceOnly":false"#);
        match fields {
            r#""type":"trailing_stop_loss""# => {
                bad.kind = safe_cex_engine::model::OrderType::TrailingStopLoss
            }
            r#""reduceOnly":true"# => bad.reduce_only = Some(true),
            r#""close":true"# => bad.close = Some(true),
            r#""clientOrderId":"illegal.dot""# => bad.client_order_id = Some("illegal.dot".into()),
            _ => {
                bad.kind = safe_cex_engine::model::OrderType::StopLoss;
                bad.price = Some(0.0);
            }
        }
        place(
            &mut h,
            vec![order(Variant::Spot, r#""reduceOnly":false"#), bad],
        );
        assert!(h.requests.is_empty(), "{fields}");
        assert!(h.done.last().unwrap().1.is_err(), "{fields}");
    }
}
#[test]
fn spot_brackets_have_no_limit_only_fields() {
    let mut h = ready(Variant::Spot);
    place(
        &mut h,
        vec![order(Variant::Spot, r#""stopLoss":90,"takeProfit":110"#)],
    );
    let entry = h.take("/api/v3/order");
    h.reply(entry.id, 200, "", r#"{"orderId":1}"#);
    let sl = h.take("/api/v3/order");
    assert!(sl.url.contains("type=STOP_LOSS"));
    assert!(sl.url.contains("stopPrice=90"));
    assert!(!sl.url.contains("timeInForce="));
    assert!(!sl.url.contains("&price="));
    h.reply(sl.id, 200, "", r#"{"orderId":2}"#);
    let tp = h.take("/api/v3/order");
    assert!(tp.url.contains("type=TAKE_PROFIT"));
    assert!(tp.url.contains("quantity=2"));
}
#[test]
fn coin_m_stops_use_regular_endpoint_and_native_counts() {
    let mut h = ready(Variant::CoinM);
    let mut o = order(Variant::CoinM, r#""reduceOnly":true"#);
    o.kind = safe_cex_engine::model::OrderType::StopLoss;
    place(&mut h, vec![o]);
    let r = h.take("/dapi/v1/order");
    assert!(r.url.contains("type=STOP_MARKET"));
    assert!(r.url.contains("stopPrice=99"));
    assert!(r.url.contains("quantity=2"));
    assert!(!r.url.contains("algo"));
}
#[test]
fn coin_m_all_native_order_families_cancel_in_one_call() {
    let mut h = ready(Variant::CoinM);
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 88,
        symbol: "BTCUSD_PERP".into(),
    }));
    assert_eq!(h.requests.len(), 1);
    assert!(h.requests[0].url.contains("/dapi/v1/allOpenOrders"));
}
#[test]
fn spot_derivative_settings_rejected_without_http() {
    let mut h = ready(Variant::Spot);
    for c in [
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
            side: None,
        },
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 3,
            hedged: true,
            symbol: None,
        },
    ] {
        h.event(Event::Command(c));
    }
    assert!(h.requests.is_empty());
    assert_eq!(h.done.len(), 3);
    assert!(h.done.iter().all(|(_, r)| r.is_err()));
}

fn param(url: &str, name: &str) -> Option<String> {
    let value = url.split_once('?')?.1.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name).then_some(value)
    })?;
    let mut decoded = Vec::new();
    let bytes = value.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            decoded.push(u8::from_str_radix(&value[at + 1..at + 3], 16).unwrap());
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    Some(String::from_utf8(decoded).unwrap())
}

#[derive(serde::Deserialize)]
struct NativeQuantity {
    #[serde(rename = "type")]
    kind: String,
    quantity: String,
}

#[test]
fn spot_and_inverse_splits_preserve_entry_and_both_bracket_totals() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        let mut o = order(variant, r#""stopLoss":90,"takeProfit":110"#);
        o.amount = 11.0;
        place(&mut h, vec![o]);
        let mut totals = std::collections::BTreeMap::<String, f64>::new();
        let mut native_orders = 0;
        while let Some(r) = h.requests.pop() {
            let rows = if let Some(batch) = param(&r.url, "batchOrders") {
                serde_json::from_str::<Vec<NativeQuantity>>(&batch).unwrap()
            } else {
                vec![NativeQuantity {
                    kind: param(&r.url, "type").unwrap(),
                    quantity: param(&r.url, "quantity").unwrap(),
                }]
            };
            let count = rows.len();
            for row in rows {
                let qty = row.quantity.parse::<f64>().unwrap();
                assert!(qty > 0.0 && qty <= 5.0, "{variant:?} {qty}");
                *totals.entry(row.kind).or_default() += qty;
                native_orders += 1;
            }
            let body = if count == 1 {
                format!(r#"{{"orderId":{native_orders}}}"#)
            } else {
                format!(
                    "[{}]",
                    (0..count)
                        .map(|i| format!(r#"{{"orderId":{}}}"#, native_orders + i))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            h.reply(r.id, 200, "", &body);
        }
        assert_eq!(native_orders, 9, "{variant:?}");
        assert_eq!(totals.len(), 3, "{variant:?}");
        assert!(
            totals.values().all(|total| *total == 11.0),
            "{variant:?} {totals:?}"
        );
        assert!(h.result(77).unwrap().is_ok());
    }
}

#[test]
fn plain_trigger_intent_rejects_both_variants_before_any_batch_write() {
    for variant in [Variant::Spot, Variant::CoinM] {
        for kind in [
            safe_cex_engine::model::OrderType::Limit,
            safe_cex_engine::model::OrderType::Market,
        ] {
            let mut h = ready(variant);
            let mut bad = order(variant, r#""reduceOnly":false"#);
            bad.kind = kind;
            bad.trigger_price = Some(110.0);
            place(&mut h, vec![order(variant, r#""reduceOnly":false"#), bad]);
            assert!(h.requests.is_empty(), "{variant:?} {kind:?}");
            assert!(h.result(77).unwrap().is_err());
        }
    }
}

fn seed_spot_maker(h: &mut Harness) {
    let socket = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":0}}"#,
    );
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":1,"s":"BTCUSDT","c":"original-caller","S":"BUY","o":"LIMIT_MAKER","X":"NEW","x":"NEW","p":"99","L":"0","q":"2","z":"0","l":"0"}}"#);
}

#[test]
fn spot_amend_preserves_post_only_and_mints_new_identity() {
    let mut h = ready(Variant::Spot);
    seed_spot_maker(&mut h);
    h.event(Event::Command(Command::AmendOrder {
        command: 88,
        id: "original-caller".into(),
        price: Some(98.0),
        amount: None,
    }));
    let cancel = h.take("/api/v3/order");
    assert_eq!(cancel.method, safe_cex_engine::Method::Delete);
    assert_eq!(
        param(&cancel.url, "origClientOrderId").as_deref(),
        Some("original-caller")
    );
    h.reply(cancel.id, 200, "", r#"{"symbol":"BTCUSDT","orderId":1,"origClientOrderId":"original-caller","clientOrderId":"cancel-original","status":"CANCELED","origQty":"2","executedQty":"0"}"#);
    let replacement = h.take("/api/v3/order");
    assert_eq!(
        param(&replacement.url, "type").as_deref(),
        Some("LIMIT_MAKER")
    );
    assert!(param(&replacement.url, "timeInForce").is_none());
    assert_ne!(
        param(&replacement.url, "newClientOrderId").as_deref(),
        Some("original-caller")
    );
}

#[test]
fn invalid_spot_amend_cannot_cancel_the_live_order() {
    let mut h = ready(Variant::Spot);
    seed_spot_maker(&mut h);
    h.event(Event::Command(Command::AmendOrder {
        command: 88,
        id: "original-caller".into(),
        price: Some(0.0),
        amount: None,
    }));
    assert!(h.requests.is_empty());
    assert!(h.result(88).unwrap().is_err());
    assert_eq!(
        h.engine.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn malformed_spot_placement_response_cannot_advance_brackets() {
    let mut h = ready(Variant::Spot);
    place(
        &mut h,
        vec![order(Variant::Spot, r#""stopLoss":90,"takeProfit":110"#)],
    );
    let request = h.take("/api/v3/order");
    h.reply(request.id, 200, "", "{}");
    assert!(h.requests.is_empty());
    assert!(h.result(77).unwrap().is_err());
}

#[test]
fn inverse_short_or_rejected_batch_cannot_advance_the_remaining_brackets() {
    for body in [
        "[]",
        r#"[{"orderId":1},{"orderId":2},{"code":-2010,"msg":"rejected"},{"orderId":4},{"orderId":5}]"#,
    ] {
        let mut h = ready(Variant::CoinM);
        let mut o = order(Variant::CoinM, r#""stopLoss":90,"takeProfit":110"#);
        o.amount = 11.0;
        place(&mut h, vec![o]);
        let request = h.take("/dapi/v1/batchOrders");
        h.reply(request.id, 200, "", body);
        assert!(h.requests.is_empty(), "{body}");
        assert!(h.result(77).unwrap().is_err(), "{body}");
    }
}

#[test]
fn inverse_partial_rejection_retains_all_proven_ids_on_the_error() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Partial {
        message: String,
        partial_order_ids: Vec<String>,
    }
    for (body, expected) in [
        (
            r#"[{"orderId":1},{"orderId":2},{"code":-2010,"msg":"insufficient margin"},{"orderId":4},{"orderId":5}]"#,
            vec!["1", "2", "4", "5"],
        ),
        (r#"[{"orderId":1},{"orderId":2}]"#, vec!["1", "2"]),
    ] {
        let mut h = ready(Variant::CoinM);
        let mut o = order(Variant::CoinM, r#""stopLoss":90,"takeProfit":110"#);
        o.amount = 11.0;
        place(&mut h, vec![o]);
        let request = h.take("/dapi/v1/batchOrders");
        h.reply(request.id, 200, "", body);
        let error = h.result(77).unwrap().as_ref().unwrap_err();
        let details: Partial = serde_json::from_str(error).expect("host-readable partial error");
        assert_eq!(details.partial_order_ids, expected);
        assert!(!details.message.is_empty());
        assert!(h.requests.is_empty(), "remaining bracket steps must stop");
    }
}

#[test]
fn inverse_later_step_failure_preserves_previously_placed_ids() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Partial {
        message: String,
        partial_order_ids: Vec<String>,
    }
    let mut h = ready(Variant::CoinM);
    let orders = (0..6)
        .map(|_| order(Variant::CoinM, r#""reduceOnly":false"#))
        .collect();
    place(&mut h, orders);
    let first = h.take("/dapi/v1/batchOrders");
    h.reply(
        first.id,
        200,
        "",
        r#"[{"orderId":1},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#,
    );
    let second = h.take("/dapi/v1/order");
    h.reply(
        second.id,
        400,
        "",
        r#"{"code":-2010,"msg":"insufficient margin"}"#,
    );
    let details: Partial = serde_json::from_str(h.result(77).unwrap().as_ref().unwrap_err())
        .expect("retain earlier successful steps");
    assert_eq!(details.partial_order_ids, ["1", "2", "3", "4", "5"]);
    assert_eq!(details.message, "insufficient margin");
    assert!(h.requests.is_empty());
}

#[test]
fn whitespace_negative_success_response_cannot_advance_brackets_or_retry() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        place(
            &mut h,
            vec![order(variant, r#""stopLoss":90,"takeProfit":110"#)],
        );
        let request = h.requests.pop().unwrap();
        h.reply(
            request.id,
            200,
            "",
            " { \"code\" : -2010, \"msg\" : \"rejected\" } ",
        );
        assert!(h.requests.is_empty(), "{variant:?}");
        assert!(h.result(77).unwrap().is_err(), "{variant:?}");
    }
}

#[test]
fn spot_unsupported_queries_complete_without_derivative_reads() {
    use safe_cex_engine::Query;
    let mut h = ready(Variant::Spot);
    h.event(Event::Command(Command::Query {
        command: 80,
        query: Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        },
    }));
    h.event(Event::Command(Command::Query {
        command: 81,
        query: Query::OrderByClientId {
            symbol: "BTCUSDT".into(),
            client_order_id: "caller".into(),
        },
    }));
    h.event(Event::Command(Command::FetchLeverage {
        command: 82,
        symbol: "BTCUSDT".into(),
    }));
    assert!(h.requests.is_empty());
    assert_eq!(h.done.len(), 3);
    assert!(h.done.iter().all(|(_, result)| result.is_err()));
}

fn seed_inverse_long_and_full_stop(h: &mut Harness) {
    use safe_cex_engine::model::{
        MarginType, Order, OrderSide, OrderStatus, OrderType, Position, PositionMode, PositionSide,
    };
    h.engine.store_mut().set_positions(vec![Position {
        id: "BTCUSD_PERP".into(),
        symbol: "BTCUSD_PERP".into(),
        side: PositionSide::Long,
        entry_price: 100.0,
        notional: 700.0,
        leverage: 20.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 7.0,
        liquidation_price: 50.0,
        margin_size: Some(0.35),
        last_update: Some(1000.0),
    }]);
    h.engine.store_mut().set_orders(vec![Order {
        id: "999".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSD_PERP".into(),
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        price: 80.0,
        amount: 0.0,
        filled: 0.0,
        remaining: 0.0,
        reduce_only: false,
        close: Some(true),
        client_order_id: None,
        time_in_force: None,
        last_update: 1000.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }]);
}

#[test]
fn inverse_subcontract_or_negative_stop_quantity_cannot_become_an_unintended_full_stop() {
    use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
    for amount in [-0.1, -0.49, 0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = ready(Variant::CoinM);
        seed_inverse_long_and_full_stop(&mut h);
        let before = h.engine.store().positions().to_vec();
        let mut stop = order(Variant::CoinM, r#""reduceOnly":true"#);
        stop.kind = OrderType::StopLoss;
        stop.side = OrderSide::Sell;
        stop.amount = amount;
        stop.price = Some(90.0);
        place(
            &mut h,
            vec![order(Variant::CoinM, r#""reduceOnly":false"#), stop],
        );
        assert!(
            h.requests.is_empty(),
            "{amount} cannot place, cancel or trigger a read"
        );
        assert!(h.result(77).unwrap().is_err(), "{amount}");
        assert_eq!(h.engine.store().positions(), before.as_slice());
        assert_eq!(h.engine.store().orders()[0].status, OrderStatus::Open);
    }
}

#[test]
fn inverse_full_stop_requires_zero_input_or_an_explicit_close_intent() {
    use safe_cex_engine::model::{OrderSide, OrderType};
    for (amount, close) in [(0.0, None), (0.1, Some(true))] {
        let mut h = ready(Variant::CoinM);
        seed_inverse_long_and_full_stop(&mut h);
        let mut stop = order(Variant::CoinM, r#""reduceOnly":true"#);
        stop.kind = OrderType::StopLoss;
        stop.side = OrderSide::Sell;
        stop.amount = amount;
        stop.price = Some(90.0);
        stop.close = close;
        place(&mut h, vec![stop]);
        let cancel = h.take("/dapi/v1/order");
        assert_eq!(cancel.method, safe_cex_engine::Method::Delete);
        h.reply(cancel.id, 200, "", r#"{"symbol":"BTCUSD_PERP","orderId":999,"status":"CANCELED","origQty":"0","executedQty":"0"}"#);
        let placed = h.take("/dapi/v1/order");
        assert_eq!(param(&placed.url, "closePosition").as_deref(), Some("true"));
        assert!(param(&placed.url, "quantity").is_none());
    }
}

#[test]
fn generated_split_and_bracket_receipts_precede_each_actual_write() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        let mut o = order(variant, r#""reduceOnly":false"#);
        o.amount = 7.0;
        o.stop_loss = Some(90.0);
        o.take_profit = Some(110.0);
        place(&mut h, vec![o]);
        let mut ids = std::collections::HashSet::new();
        let mut writes = 0;
        while let Some(request) = h.requests.first() {
            let request_id = request.id;
            let index = h
                .progress_at_request
                .iter()
                .find(|(id, _)| *id == request_id)
                .unwrap()
                .1;
            let (command, receipts) = &h.progress[index - 1];
            assert_eq!(*command, 77);
            assert!(!receipts.is_empty());
            for receipt in receipts {
                assert_eq!(
                    receipt.kind,
                    safe_cex_engine::OrderIdentityKind::ClientOrderId
                );
                assert_eq!(
                    receipt.symbol,
                    if variant == Variant::Spot {
                        "BTCUSDT"
                    } else {
                        "BTCUSD_PERP"
                    }
                );
                assert!(request.url.contains(&receipt.identity));
                assert!(ids.insert(receipt.identity.clone()));
            }
            let request = h.requests.remove(0);
            writes += 1;
            let response = if request.url.contains("batchOrders") {
                r#"[{"orderId":1},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#
            } else {
                r#"{"orderId":1}"#
            };
            // Return one acknowledgement per submitted lot, preserving strict COIN-M counts.
            let response = if request.url.contains("batchOrders") {
                format!(
                    "[{}]",
                    (0..receipts.len())
                        .map(|i| format!(r#"{{"orderId":{}}}"#, i + 1))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            } else {
                response.into()
            };
            h.reply(request.id, 200, "", &response);
        }
        assert_eq!(
            ids.len(),
            6,
            "every split entry and SL/TP has its actual identity"
        );
        assert!(writes > 0);
        assert!(h.result(77).unwrap().is_ok());
    }
}

#[test]
fn spot_market_take_profit_still_amends_while_spot_take_profit_limit_refuses() {
    for (native, refused) in [("TAKE_PROFIT", false), ("TAKE_PROFIT_LIMIT", true)] {
        let mut h = ready(Variant::Spot);
        let socket = h.socket("ws-api/v3").unwrap();
        h.event(Event::WsOpened { socket });
        h.message(
            "ws-api/v3",
            r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":0}}"#,
        );
        h.message("ws-api/v3", &format!(r#"{{"subscriptionId":0,"event":{{"e":"executionReport","i":1,"s":"BTCUSDT","c":"tp","S":"SELL","o":"{native}","X":"NEW","x":"NEW","p":"0","P":"120","L":"0","q":"2","z":"0","l":"0"}}}}"#));
        assert!(h.engine.store().order("tp").is_some(), "{native}");
        h.event(Event::Command(Command::AmendOrder {
            command: 88,
            id: "tp".into(),
            price: Some(125.0),
            amount: None,
        }));
        let failed = h
            .failures
            .iter()
            .any(|(c, e)| *c == 88 && e.kind == Some(safe_cex_engine::ErrorKind::Unsupported));
        assert_eq!(failed, refused, "{native}");
        assert_eq!(h.requests.is_empty(), refused, "{native}");
    }
}

#[test]
fn unproven_spot_and_inverse_ids_keep_the_cancellable_namespace() {
    let mut observations = Vec::new();
    for (variant, body, expected) in [
        (
            Variant::Spot,
            r#"{"orderId":12,"clientOrderId":"caller_1"}"#,
            vec!["caller_1"],
        ),
        (Variant::Spot, r#"{"orderId":12}"#, vec![]),
        (
            Variant::Spot,
            r#"{"orderId":12,"clientOrderId":"other"}"#,
            vec![],
        ),
        (Variant::Spot, r#"{"clientOrderId":"caller_1"}"#, vec![]),
        (Variant::Spot, r#"{"orderId":12,"clientOrderId":7}"#, vec![]),
        (
            Variant::Spot,
            r#"{"orderId":12,"clientOrderId":"caller_1","clientOrderId":"other"}"#,
            vec![],
        ),
        (
            Variant::Spot,
            r#"{"orderId":12,"clientOrderId":"caller_1","code":"0"}"#,
            vec![],
        ),
        (Variant::CoinM, r#"{"orderId":12}"#, vec!["12"]),
        (Variant::CoinM, r#"{"algoId":13}"#, vec![]),
    ] {
        let mut h = ready(variant);
        let mut input = order(variant, r#""clientOrderId":"caller_1""#);
        if variant == Variant::CoinM {
            input.kind = safe_cex_engine::model::OrderType::StopLoss;
            input.client_order_id = None;
        }
        let before = h.engine.store().orders().to_vec();
        place(&mut h, vec![input]);
        let request = h.take(if variant == Variant::Spot {
            "/api/v3/order"
        } else {
            "/dapi/v1/order"
        });
        h.reply(request.id, 503, "", body);
        let failure = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
        assert_eq!(failure.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let ids = failure
            .partial
            .as_ref()
            .map(|p| p.order_ids.clone())
            .unwrap_or_default();
        assert_eq!(h.engine.store().orders(), before);
        observations.push((variant, body, ids, expected));
    }
    for (variant, body, ids, expected) in observations {
        assert_eq!(ids, expected, "variant={variant:?} body={body}");
    }
}

const SIGNATURE_REJECTED: &str =
    r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#;

#[test]
fn a_signature_rejected_write_is_re_signed_and_resent_once() {
    let mut h = ready(Variant::Spot);
    place(
        &mut h,
        vec![order(Variant::Spot, r#""clientOrderId":"caller_1""#)],
    );
    let first = h.take("/api/v3/order");
    h.now += 5.0;
    h.reply(first.id, 400, "", SIGNATURE_REJECTED);
    let second = h.take("/api/v3/order");
    assert!(h.requests.is_empty(), "exactly one resend");
    assert_ne!(second.id, first.id);
    assert!(h.done.is_empty(), "the resend decides the outcome");
    assert!(second.url.contains("newClientOrderId=caller_1"));
    assert_ne!(
        second.url, first.url,
        "the resend is re-signed at its own timestamp"
    );
    h.reply(second.id, 200, "", r#"{"orderId":123}"#);
    assert_eq!(
        h.done.last().unwrap().1.as_ref().unwrap(),
        r#"["caller_1"]"#
    );
}

#[test]
fn a_second_signature_rejection_fails_without_a_third_send() {
    let mut h = ready(Variant::Spot);
    place(&mut h, vec![order(Variant::Spot, r#""reduceOnly":false"#)]);
    let first = h.take("/api/v3/order");
    h.reply(first.id, 400, "", SIGNATURE_REJECTED);
    let second = h.take("/api/v3/order");
    h.reply(second.id, 400, "", SIGNATURE_REJECTED);
    assert!(h.requests.is_empty(), "no third send");
    let (_, error) = h.failures.last().expect("the write fails");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Rejected));
}

#[test]
fn other_write_refusals_are_never_resent() {
    let mut h = ready(Variant::Spot);
    place(&mut h, vec![order(Variant::Spot, r#""reduceOnly":false"#)]);
    let first = h.take("/api/v3/order");
    h.reply(first.id, 400, "", r#"{"code":-2010,"msg":"rejected"}"#);
    assert!(h.requests.is_empty());
    assert!(h.done.last().unwrap().1.is_err());
}
