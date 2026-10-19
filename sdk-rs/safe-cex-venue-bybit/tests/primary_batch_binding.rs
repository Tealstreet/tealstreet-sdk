//! Physical transport proofs for the primary source batch composition.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, PlaceOrderOpts, Position};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn market(symbol: &str) -> Value {
    json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.001,"price":0.1},
        "limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":100}}})
}
fn order(index: usize) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01 + index as f64 * 0.001,"price":100.07})
}
fn reply(ids: &[String]) -> Value {
    json!({"retCode":0,"retMsg":"OK","result":{"list":ids.iter().map(|id| json!({"orderId":id})).collect::<Vec<_>>()}})
}
fn seed(h: &support::Harness, markets: &[Value], hedged: bool) {
    let markets: Vec<Market> = serde_json::from_value(json!(markets)).unwrap();
    let tickers = markets.iter().map(|m| serde_json::from_value(json!({
        "id":m.id,"symbol":m.symbol,"bid":99,"ask":101,"last":100,"mark":101,"index":102,
        "percentage":0,"openInterest":1,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":100,
    })).unwrap()).collect();
    let positions: Vec<Position> = markets
        .iter()
        .map(|m| {
            serde_json::from_value(json!({
        "id":format!("{}:long", m.symbol),"symbol":m.symbol,"side":"long","entryPrice":100,
        "notional":0,"leverage":10,"unrealizedPnl":0,"realizedPnl":0,"contracts":0,
        "liquidationPrice":0,"positionMode":if hedged {"hedged"} else {"oneway"},"lastUpdate":1,
    })).unwrap()
        })
        .collect();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            tickers: Some(tickers),
            positions: Some(positions),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn cases() -> Vec<Value> {
    let mut cases = vec![];
    for count in [0, 1, 2, 10, 11, 21] {
        let orders: Vec<_> = (0..count).map(order).collect();
        let replies: Vec<_> = (0..count.div_ceil(10))
            .map(|chunk: usize| {
                reply(
                    &(chunk * 10..(chunk * 10 + 10).min(count))
                        .map(|i| format!("id-{i}"))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        cases.push(json!({"name":format!("count-{count}"),"orders":orders,"markets":[market("BTCUSDT")],"mode":"oneway","replies":replies}));
    }
    for (name, patch) in [
        ("market", json!({"type":"market","price":null})),
        (
            "stop",
            json!({"type":"stop_market","triggerPrice":95,"slTriggerType":"last_price"}),
        ),
        (
            "take-profit",
            json!({"type":"take_profit_market","triggerPrice":105,"tpTriggerType":"mark_price"}),
        ),
        (
            "conditional-limit",
            json!({"type":"conditional","triggerPrice":103,"reduceOnly":true}),
        ),
        (
            "conditional-market",
            json!({"type":"conditional","price":null,"triggerPrice":99}),
        ),
        (
            "missing-trigger",
            json!({"type":"conditional","triggerPrice":null}),
        ),
        (
            "rounded-zero-trigger",
            json!({"type":"conditional","triggerPrice":0.01}),
        ),
        ("brackets", json!({"stopLoss":90,"takeProfit":110})),
        ("post-only", json!({"timeInForce":"PostOnly"})),
        ("caller-id", json!({"clientOrderId":"native-id"})),
        ("hedged-buy", json!({"side":"buy"})),
        ("hedged-sell", json!({"side":"sell"})),
        ("hedged-reduce-buy", json!({"side":"buy","reduceOnly":true})),
        (
            "hedged-reduce-sell",
            json!({"side":"sell","reduceOnly":true}),
        ),
        (
            "conditional-hedged-reduce",
            json!({"type":"conditional","triggerPrice":103,"reduceOnly":true}),
        ),
    ] {
        let mut input = order(0);
        for (key, value) in patch.as_object().unwrap() {
            if value.is_null() {
                input.as_object_mut().unwrap().remove(key);
            } else {
                input[key] = value.clone();
            }
        }
        let orders = if name == "brackets" {
            vec![input.clone(), input]
        } else {
            vec![input]
        };
        let ids: Vec<_> = (0..orders.len()).map(|i| format!("id-{i}")).collect();
        cases.push(
            json!({"name":name,"orders":orders,"markets":[market("BTCUSDT")],
            "mode":if name.contains("hedged") {"hedged"} else {"oneway"}, "replies":[reply(&ids)]}),
        );
    }
    cases.push(json!({"name":"missing-market-middle","orders":[order(0),{"symbol":"MISSING","type":"limit","side":"buy","amount":1,"price":100},order(2)],
        "markets":[market("BTCUSDT")],"mode":"oneway","replies":[reply(&["id-0".into(),"id-2".into()])]}));
    let mut partial = reply(&["id-0".into(), "".into()]);
    partial["retExtInfo"] =
        json!({"list":[{"code":0,"msg":"OK"},{"code":30008,"msg":"must agree trading terms"}]});
    cases.push(json!({"name":"partial-errors","orders":[order(0),order(1)],"markets":[market("BTCUSDT")],"mode":"oneway","replies":[partial]}));
    let mut inverse = order(1);
    inverse["symbol"] = json!("BTCUSD");
    cases.push(json!({"name":"category-order","orders":[inverse,order(0)],"markets":[market("BTCUSDT"),market("BTCUSD")],"mode":"oneway",
        "replies":[reply(&["inverse-id".into()]),reply(&["linear-id".into()])]}));
    assert_eq!(cases.len(), 24, "independent native batch case floor");
    cases
}

#[test]
fn primary_batches_match_twenty_four_actual_source_compositions() {
    let cases = cases();
    let mut child = Command::new("node")
        .arg(format!(
            "{}/tests/primary-batch-binding-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source batch oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 24);
    for (case, expected) in cases.iter().zip(expected) {
        let mut options = support::options();
        options.enable_inverse_markets = Some(true);
        let h = support::harness(options, |w| {
            w.ok(
                "GET",
                "/v5/order/realtime",
                json!({"list":[],"nextPageCursor":""}),
            );
            w.ok(
                "GET",
                "/v5/position/list",
                json!({"list":[],"nextPageCursor":""}),
            );
            for reply in case["replies"].as_array().unwrap() {
                w.route("POST", "/v5/order/create-batch", 200, reply.clone());
            }
        });
        seed(
            &h,
            case["markets"].as_array().unwrap(),
            case["mode"] == "hedged",
        );
        let orders: Vec<PlaceOrderOpts> = serde_json::from_value(case["orders"].clone()).unwrap();
        let venue = h.venue();
        let result = h.drive(Box::pin(venue.base().place_orders(venue, orders)));
        let actual = match result {
            Ok(ids) => json!({"ids":ids}),
            Err(error) => json!({"error":error.message}),
        };
        let expected_result = if expected.get("ids").is_some() {
            json!({"ids":expected["ids"]})
        } else {
            json!({"error":expected["error"]})
        };
        assert_eq!(actual, expected_result, "{} result", case["name"]);
        let requests: Vec<_> = h
            .wire
            .calls("/v5/order/create-batch")
            .into_iter()
            .map(|r| json!({"endpoint":r.url,"payload":r.body}))
            .collect();
        assert_eq!(
            json!(requests),
            expected["requests"],
            "{} requests",
            case["name"]
        );
        let errors: Vec<_> = h
            .runtime
            .logs
            .borrow()
            .iter()
            .filter_map(|line| {
                line.strip_prefix("error: exchange error ")
                    .map(str::to_owned)
            })
            .collect();
        assert_eq!(
            json!(errors),
            expected["errors"],
            "{} diagnostics",
            case["name"]
        );
        if let Some(recorded) = expected["recorded"].as_array() {
            let cached = h.drive(h.venue().fetch_orders(None)).unwrap();
            for entry in recorded {
                let found = cached
                    .iter()
                    .find(|row| row.id == entry[0].as_str().unwrap());
                // Source Market orders are deliberately not cached.
                let source_order = case["orders"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|o| o["symbol"] == entry[1]);
                if source_order.is_some_and(|o| o["type"] == "market") {
                    continue;
                }
                let found = found.expect("batch result reaches recently placed order cache");
                assert_eq!(found.symbol, entry[1].as_str().unwrap());
                assert_eq!(
                    found.amount,
                    entry[2].as_str().unwrap().parse::<f64>().unwrap()
                );
            }
        }
        let posted = h.wire.calls("/v5/order/create-batch");
        if posted.len() > 1 && case["name"] != "category-order" {
            assert!(
                h.runtime.now.get() >= support::START_MS + 1200. * (posted.len() - 1) as f64,
                "batch delay observed"
            );
        }
    }
}

#[test]
fn batch_failure_falls_back_to_each_order_and_preserves_successes() {
    let h = support::harness(support::options(), |w| {
        w.ok(
            "GET",
            "/v5/position/list",
            json!({"list":[],"nextPageCursor":""}),
        );
        w.route(
            "POST",
            "/v5/order/create-batch",
            400,
            json!({"retCode":10001,"retMsg":"batch rejected"}),
        );
        w.route(
            "POST",
            "/v5/order/create",
            400,
            json!({"retCode":10001,"retMsg":"first rejected"}),
        );
        w.ok(
            "POST",
            "/v5/order/create",
            json!({"orderId":"fallback-success"}),
        );
    });
    seed(&h, &[market("BTCUSDT")], false);
    let inputs: Vec<PlaceOrderOpts> = serde_json::from_value(json!([order(0), order(1)])).unwrap();
    let venue = h.venue();
    let ids = h
        .drive(Box::pin(venue.base().place_orders(venue, inputs)))
        .unwrap();
    assert_eq!(ids, ["fallback-success"]);
    assert_eq!(h.wire.calls("/v5/order/create-batch").len(), 1);
    assert_eq!(h.wire.calls("/v5/order/create").len(), 2);
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|line| line.contains("Failed to place batch orders:"))
    );
}

#[test]
fn invalid_execution_and_caller_ids_refuse_the_whole_batch_before_writes() {
    for bad in [
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100,"maxSlippagePct":101}),
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"invalid id"}),
    ] {
        let h = support::harness(support::options(), |_| {});
        seed(&h, &[market("BTCUSDT")], false);
        let inputs: Vec<PlaceOrderOpts> = serde_json::from_value(json!([order(0), bad])).unwrap();
        let venue = h.venue();
        assert!(
            h.drive(Box::pin(venue.base().place_orders(venue, inputs)))
                .is_err(),
            "atomic batch preflight"
        );
        assert!(
            h.wire.requests.borrow().is_empty(),
            "rejected batch starts no HTTP"
        );
    }
}

#[test]
fn special_orders_run_before_batches_and_a_failed_special_does_not_stop_them() {
    let symbol = "BTC-25SEP26-130000-C-USDT";
    let mut options = support::options();
    options.enable_option_markets = Some(true);
    let h = support::harness(options, |w| {
        w.ok("POST", "/v5/order/create", json!({"orderId":"bbo-id"}));
        w.ok("POST", "/v5/order/create", json!({"orderId":"option-id"}));
        w.route(
            "POST",
            "/v5/order/create-batch",
            200,
            reply(&["regular-id".into()]),
        );
    });
    seed(&h, &[market("BTCUSDT"), market(symbol)], false);
    let venue = h.venue();
    let ids = h.drive(Box::pin(venue.base().place_orders_json(venue, vec![
        order(0),
        json!({"symbol":"MISSING","type":"trailing_stop_market","side":"buy","amount":1,"price":90}),
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"bybitBbo":{"sideType":"Queue","level":1},"clientOrderId":"bbo-caller"}),
        json!({"symbol":symbol,"type":"limit","side":"buy","amount":1,"price":100}),
    ]))).unwrap();
    assert_eq!(ids, ["bbo-id", "option-id", "regular-id"]);
    let singles = h.wire.calls("/v5/order/create");
    assert_eq!(singles.len(), 2);
    assert_eq!(singles[0].body["bboSideType"], "Queue");
    assert_eq!(singles[0].body["orderLinkId"], "bbo-caller");
    assert!(singles[0].body.get("price").is_none());
    assert_eq!(singles[1].body["category"], "option");
    assert_eq!(
        h.wire.calls("/v5/order/create-batch")[0].body["request"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let posts: Vec<_> = h
        .wire
        .requests
        .borrow()
        .iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.url.clone())
        .collect();
    assert_eq!(
        posts,
        [
            "/v5/order/create",
            "/v5/order/create",
            "/v5/order/create-batch"
        ]
    );
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|line| line.contains("Failed to place special order: Ticker MISSING not found"))
    );
}

#[test]
fn disposal_cancels_the_pending_inter_batch_delay() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let h = support::harness(support::options(), |w| {
        w.route(
            "POST",
            "/v5/order/create-batch",
            200,
            reply(&["first".into()]),
        );
    });
    seed(&h, &[market("BTCUSDT")], false);
    let orders: Vec<PlaceOrderOpts> =
        serde_json::from_value(json!((0..21).map(order).collect::<Vec<_>>())).unwrap();
    let venue = h.venue();
    let mut pending = Box::pin(venue.base().place_orders(venue, orders));
    assert!(matches!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    h.runtime.pump();
    assert_eq!(
        h.wire.calls("/v5/order/create-batch").len(),
        1,
        "first batch precedes delay"
    );
    venue.dispose().unwrap();
    h.advance(5000.);
    assert_eq!(
        h.wire.calls("/v5/order/create-batch").len(),
        1,
        "disposal cancels remaining batches"
    );
}

#[test]
fn retained_audit_placement_extensions_refuse_before_signed_writes() {
    for extension in [
        json!({"bybitBbo":null}),
        json!({"bybitBbo":{"sideType":"Queue","level":0}}),
        json!({"bybitBbo":{"sideType":"Queue","level":6}}),
        json!({"bybitBbo":{"sideType":"Other","level":1}}),
        json!({"bybitSlippageToleranceTicks":null}),
        json!({"bybitBbo":{"sideType":"Queue","level":1},"bybitSlippageToleranceTicks":2}),
    ] {
        let h = support::harness(support::options(), |_| {});
        seed(&h, &[market("BTCUSDT")], false);
        let mut input = order(0);
        input
            .as_object_mut()
            .unwrap()
            .extend(extension.as_object().unwrap().clone());
        let venue = h.venue();
        let result = h.drive(Box::pin(venue.base().place_orders_json(venue, vec![input])));
        assert!(result.is_err(), "placement_closed_extensions {extension}");
        assert!(
            h.wire.requests.borrow().is_empty(),
            "strict placement refuses before signing/HTTP"
        );
        venue.dispose().unwrap();
    }
}
