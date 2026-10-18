//! Physical transport proofs for the dedicated Inverse source batch composition.
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
    json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":"USD","active":true,"isInverse":true,
        "precision":{"amount":1,"price":0.1},
        "limits":{"amount":{"min":1,"max":1000},"leverage":{"min":1,"max":100}}})
}
fn order(index: usize) -> Value {
    json!({"symbol":"BTCUSD","type":"limit","side":"buy","amount":12.6 + index as f64,"price":100.07})
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
        cases.push(json!({"name":format!("count-{count}"),"orders":orders,"markets":[market("BTCUSD")],"mode":"oneway","replies":replies}));
    }
    for amount in [-1.0, 0.0, 0.49, 0.5, 1.49, 1.5, 12.6, 1001.0] {
        let mut input = order(0);
        input["amount"] = json!(amount);
        cases.push(
            json!({"name":format!("amount-{amount}"),"orders":[input],"markets":[market("BTCUSD")],
            "mode":"oneway","replies":[reply(&["id-0".into()])]}),
        );
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
            json!({"name":name,"orders":orders,"markets":[market("BTCUSD")],
            "mode":if name.contains("hedged") {"hedged"} else {"oneway"}, "replies":[reply(&ids)]}),
        );
    }
    cases.push(json!({"name":"missing-market-middle","orders":[order(0),{"symbol":"MISSING","type":"limit","side":"buy","amount":1,"price":100},order(2)],
        "markets":[market("BTCUSD")],"mode":"oneway","replies":[reply(&["id-0".into(),"id-2".into()])]}));
    let mut partial = reply(&["id-0".into(), "".into()]);
    partial["retExtInfo"] =
        json!({"list":[{"code":0,"msg":"OK"},{"code":30008,"msg":"must agree trading terms"}]});
    cases.push(json!({"name":"partial-errors","orders":[order(0),order(1)],"markets":[market("BTCUSD")],"mode":"oneway","replies":[partial]}));
    let mut linear_metadata = market("BTCUSD");
    linear_metadata["isInverse"] = json!(false);
    cases.push(
        json!({"name":"category-stays-inverse","orders":[order(0)],"markets":[linear_metadata],
        "mode":"oneway","replies":[reply(&["id-0".into()])]}),
    );
    for (tag, qty) in [
        ("NaN", "NaN"),
        ("Infinity", "1000"),
        ("-Infinity", "1"),
        ("-0", "1"),
    ] {
        cases.push(json!({"name":format!("native-amount-{tag}"),"numericAmount":tag,"expectedQty":qty,
            "orders":[order(0)],"markets":[market("BTCUSD")],"mode":"oneway","replies":[reply(&["id-0".into()])]}));
    }
    assert_eq!(cases.len(), 36, "independent inverse batch case floor");
    cases
}

#[test]
fn inverse_batches_match_thirty_six_actual_source_compositions() {
    let cases = cases();
    let mut child = Command::new("node")
        .arg(format!(
            "{}/tests/inverse-batch-binding-oracle.cjs",
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
    assert_eq!(expected.len(), 36);
    for (case, expected) in cases.iter().zip(expected) {
        let mut options = support::options();
        options.enable_inverse_markets = Some(true);
        let h = support::harness_as(safe_cex_types::ExchangeName::Bybitinverse, options, |w| {
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
        let mut orders: Vec<PlaceOrderOpts> =
            serde_json::from_value(case["orders"].clone()).unwrap();
        if let Some(tag) = case["numericAmount"].as_str() {
            let value = match tag {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                "-0" => -0.0,
                _ => unreachable!(),
            };
            for order in &mut orders {
                order.amount = value;
            }
        }
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
        let warnings: Vec<_> = h
            .runtime
            .logs
            .borrow()
            .iter()
            .filter_map(|line| line.strip_prefix("warn: ").map(str::to_owned))
            .collect();
        assert_eq!(
            json!(warnings),
            expected["warnings"],
            "{} source console warnings",
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
                let amount = entry[2].as_str().unwrap().parse::<f64>().unwrap();
                assert_eq!(
                    found.amount.to_bits(),
                    amount.to_bits(),
                    "source cache fallback amount"
                );
                assert_eq!(
                    found.remaining.to_bits(),
                    entry[3].as_str().unwrap().parse::<f64>().unwrap().to_bits()
                );
                let extra = found.extra.as_ref().unwrap();
                assert_eq!(
                    extra.remaining_native.unwrap().to_bits(),
                    entry[4].as_str().unwrap().parse::<f64>().unwrap().to_bits()
                );
                assert_eq!(
                    extra.remaining_notional.unwrap().to_bits(),
                    entry[5].as_str().unwrap().parse::<f64>().unwrap().to_bits()
                );
            }
        }
        let posted = h.wire.calls("/v5/order/create-batch");
        if let Some(qty) = case["expectedQty"].as_str() {
            assert_eq!(
                posted[0].body["request"][0]["qty"], qty,
                "independent native nonfinite sizing"
            );
        }
        if posted.len() > 1 {
            assert!(
                h.runtime.now.get() >= support::START_MS + 1200. * (posted.len() - 1) as f64,
                "batch delay observed"
            );
        }
    }
}
