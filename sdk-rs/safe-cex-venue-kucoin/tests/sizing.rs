use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{Order, PlaceOrderOpts};
use safe_cex_venue_kucoin::sizing::*;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn live_symbol_sizing_and_remainder_reconciliation() {
    let market = json!({"id":"XBTUSDTM","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":100}},"contractSize":0.001});
    let order = json!({"id":"order","symbol":"BTCUSDT","type":"limit","side":"buy","status":"open","price":1,"amount":0.005,"filled":0.001,"remaining":0.004,"reduceOnly":false,"timeInForce":"GoodTillCancel","lastUpdate":1700000000777_i64,"extra":{"remainingNative":4,"filledInPlacementUnits":1}});
    let position = json!({"id":"position","symbol":"BTCUSDT","side":"short","entryPrice":1,"notional":2,"leverage":3,"unrealizedPnl":19,"contracts":0.005,"liquidationPrice":23});
    let ticker = json!({"id":"ticker-BTC","symbol":"BTC-USDT","bid":1,"ask":3,"last":2,"mark":5,"index":7,"percentage":11,"openInterest":13,"fundingRate":17,"fundingTime":19,"volume":23,"quoteVolume":29});
    let mut cases = vec![];
    for contract in [0.001, 1.0, 0.0, -0.001] {
        for previous in [None, Some(-1.0), Some(0.0), Some(2.0), Some(6.0)] {
            for amount in [0.0, 3.0, 6.0] {
                for ty in [
                    "stop_market",
                    "take_profit_market",
                    "trailing_stop_market",
                    "limit",
                ] {
                    let mut market = market.clone();
                    market["contractSize"] = json!(contract);
                    let mut existing = order.clone();
                    existing["extra"]
                        .as_object_mut()
                        .unwrap()
                        .remove("remainingNative");
                    if let Some(n) = previous {
                        existing["extra"]["remainingNative"] = json!(n);
                    }
                    let mut unrelated = order.clone();
                    unrelated["id"] = json!("new-id");
                    cases.push(json!({"store":{"markets":[market],"tickers":[ticker],"positions":[position],"orders":[existing]},"fetched":[order,unrelated],"order":order,"opts":{"symbol":"BTCUSDT","type":ty,"side":"buy","amount":amount},"symbol":"BTCUSDT","currency":"BTC","testnet":false}));
                }
            }
        }
    }
    for symbol in [
        None,
        Some(""),
        Some("XBTUSDTM"),
        Some("ETHUSDTM"),
        Some("BTCUSDT"),
        Some("MISSING"),
    ] {
        let mut c = cases[0].clone();
        c["symbol"] = json!(symbol);
        c["testnet"] = json!(true);
        c["currency"] = json!("USDC");
        cases.push(c);
    }
    // Missing/negative fetched remainders and duplicate snapshot ids.
    for remainder in [None, Some(-1.0), Some(0.0), Some(9.0)] {
        let mut c = cases[0].clone();
        c["fetched"][0]["extra"]
            .as_object_mut()
            .unwrap()
            .remove("remainingNative");
        if let Some(n) = remainder {
            c["fetched"][0]["extra"]["remainingNative"] = json!(n);
        }
        let mut older = order.clone();
        older["extra"]["remainingNative"] = json!(1);
        let mut newer = order.clone();
        newer["extra"]["remainingNative"] = json!(3);
        c["store"]["orders"] = json!([older, newer]);
        cases.push(c);
    }
    for quotes in [
        vec![],
        vec![("USDC", 3.0)],
        vec![("USD", 4.0)],
        vec![("USDT", 0.0), ("USDC", 3.0)],
        vec![("USDT", 2.0), ("USDC", 3.0)],
    ] {
        let mut c = cases[0].clone();
        c["store"]["tickers"] = json!(
            quotes
                .into_iter()
                .map(|(quote, last)| {
                    let mut t = ticker.clone();
                    t["symbol"] = json!(format!("BTC-{quote}"));
                    t["id"] = t["symbol"].clone();
                    t["last"] = json!(last);
                    t
                })
                .collect::<Vec<_>>()
        );
        cases.push(c);
    }
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/sizing.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 255, "KUCOIN-CARDINALITY sizing input cases");
    assert_eq!(
        expected.len(),
        255,
        "KUCOIN-CARDINALITY sizing source cases"
    );
    assert_eq!(
        expected
            .iter()
            .map(|v| v["reconcileFetchedOrderRemainders"]["ok"]
                .as_array()
                .unwrap()
                .len())
            .sum::<usize>(),
        510,
        "KUCOIN-CARDINALITY sizing source output total"
    );
    let mut comparisons = 0;
    for index in 0..255 {
        let c = &cases[index];
        let expected = &expected[index];
        let store = DefaultStore::new();
        let update: StoreUpdate = serde_json::from_value(c["store"].clone()).unwrap();
        Store::update(&store, update, false).unwrap();
        let symbol = c["symbol"].as_str();
        let order: Order = serde_json::from_value(c["order"].clone()).unwrap();
        let opts: PlaceOrderOpts = serde_json::from_value(c["opts"].clone()).unwrap();
        let fetched: Vec<Order> = serde_json::from_value(c["fetched"].clone()).unwrap();
        let plan = match get_order_plan_type_and_amount(&opts, &store) {
            Ok(p) => json!({"ok":p}),
            Err(e) => json!({"error":e}),
        };
        assert_eq!(
            expected["reconcileFetchedOrderRemainders"]["ok"]
                .as_array()
                .unwrap()
                .len(),
            2,
            "KUCOIN-CARDINALITY sizing source per case"
        );
        let reconciled = reconcile_fetched_order_remainders(fetched, &store);
        assert_eq!(
            reconciled.len(),
            2,
            "KUCOIN-CARDINALITY sizing native retained orders"
        );
        for row in 0..2 {
            assert_eq!(reconciled[row].id, ["order", "new-id"][row]);
            comparisons += 1;
        }
        let actual = json!({"toKucoinSymbol":{"ok":to_kucoin_symbol(symbol)},"toInternalSymbol":{"ok":to_internal_symbol(symbol)},"toApiSymbol":{"ok":to_api_symbol(symbol,&store)},"contractSizeFor":{"ok":contract_size_for(symbol,&store)},"nativeAmountForCancelReplace":{"ok":native_amount_for_cancel_replace(&order,&store)},"reconcileFetchedOrderRemainders":{"ok":reconciled},"getOrderPlanTypeAndAmount":plan,"apiMarginCoin":{"ok":api_margin_coin(c["testnet"].as_bool().unwrap())},"getUsdPrice":{"ok":get_usd_price(c["currency"].as_str().unwrap(),&store)}});
        compare(&actual, expected);
    }
    assert_eq!(
        comparisons, 510,
        "KUCOIN-CARDINALITY sizing native comparisons"
    );
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b)
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                compare(a, b.get(k).unwrap())
            }
        }
        _ => assert_eq!(a, b),
    }
}
