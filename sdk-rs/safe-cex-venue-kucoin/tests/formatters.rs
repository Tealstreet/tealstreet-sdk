use safe_cex_exchange::{
    client_order_id::{ClientOrderIdRuleOverrides, pattern_client_order_id_rule},
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::{JsJsonValue as J, js_json_stringify};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use safe_cex_venue_kucoin::formatters::{format_algo_order, format_create_order};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
};
#[derive(Default)]
struct Runtime {
    ids: Cell<usize>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        self.ids.set(self.ids.get() + 1);
        "fixture-id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
#[test]
fn live_futures_payloads_and_client_id_preflight() {
    let market = json!({"id":"XBTUSDTM","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":100}},"contractSize":0.001});
    let position = json!({"id":"pos","symbol":"BTCUSDT","side":"long","entryPrice":1,"notional":17,"leverage":3,"unrealizedPnl":19,"contracts":2,"liquidationPrice":23,"marginType":"isolated","positionMode":"hedged"});
    let mut cases = vec![];
    for ty in [
        "limit",
        "market",
        "stop_market",
        "take_profit_market",
        "trailing_stop_market",
    ] {
        for side in ["buy", "sell"] {
            for amount in [0.0, 0.2, 1.5, -1.5, 10001.0] {
                for tif in [
                    "GoodTillCancel",
                    "ImmediateOrCancel",
                    "FillOrKill",
                    "PostOnly",
                ] {
                    cases.push(json!({"markets":[market],"positions":[position],"opts":{"symbol":"BTCUSDT","type":ty,"side":side,"amount":amount,"price":12.35,"timeInForce":tif}}));
                }
            }
        }
    }
    for (key, values) in [
        ("reduceOnly", vec![json!(true), json!(false)]),
        ("price", vec![json!(0), json!(-1.55), json!(0.01)]),
        ("stopLoss", vec![json!(0), json!(12.01)]),
        ("takeProfit", vec![json!(0), json!(13.05)]),
        (
            "clientOrderId",
            vec![
                json!("caller-id"),
                json!(""),
                json!("invalid space"),
                json!("x".repeat(41)),
            ],
        ),
        (
            "tpTriggerType",
            vec![
                json!("mark_price"),
                json!("index_price"),
                json!("last_price"),
            ],
        ),
    ] {
        for value in values {
            let mut c = cases[0].clone();
            c["opts"][key] = value;
            cases.push(c);
        }
    }
    for positions in [
        json!([]),
        json!([position]),
        json!([{"id":"pos","symbol":"BTCUSDT","side":"short","entryPrice":1,"notional":17,"leverage":0,"unrealizedPnl":19,"contracts":2,"liquidationPrice":23,"marginType":"cross","positionMode":"oneway"}]),
    ] {
        let mut c = cases[0].clone();
        c["positions"] = positions;
        cases.push(c);
    }
    for ty in ["limit", "stop_market", "trailing_stop_market"] {
        let mut c = cases[0].clone();
        c["opts"]["type"] = json!(ty);
        c["opts"]["clientOrderId"] = json!("caller-id");
        c["opts"]["amount"] = json!(10001);
        cases.push(c);
    }
    let mut c = cases[0].clone();
    c["markets"] = json!([]);
    cases.push(c);
    for (field, value) in [
        ("close", json!(true)),
        ("stopLoss", json!(0)),
        ("triggerPrice", json!(0)),
    ] {
        let mut c = cases[0].clone();
        c["opts"]["clientOrderId"] = json!("caller-id");
        c["opts"][field] = value;
        cases.push(c);
    }
    for field in ["amount", "price", "takeProfit"] {
        for special in ["NaN", "Infinity", "-Infinity"] {
            let mut c = cases[0].clone();
            c["special"] = json!({"field":field,"value":special});
            cases.push(c);
        }
    }
    for (amount_step, price_step) in [(1.0, 0.1), (0.25, 0.01), (0.01, 0.5)] {
        for side in ["buy", "sell"] {
            for ty in ["limit", "stop_market"] {
                let mut c = cases[0].clone();
                c["markets"][0]["precision"] = json!({"amount":amount_step,"price":price_step});
                c["opts"] = json!({"symbol":"BTCUSDT","type":ty,"side":side,"amount":1.37,"price":12.37,"stopLoss":11.76,"takeProfit":13.28});
                c["precisionIdentity"] = json!(true);
                cases.push(c);
            }
        }
    }
    for (amount_step, price_step) in [(1.0, 0.1), (0.25, 0.01), (0.01, 0.5)] {
        for side in ["buy", "sell"] {
            for ty in ["market", "take_profit_market", "trailing_stop_market"] {
                let mut c = cases[0].clone();
                c["markets"][0]["precision"] = json!({"amount":amount_step,"price":price_step});
                c["opts"] = json!({"symbol":"BTCUSDT","type":ty,"side":side,"amount":1.37,"price":12.37,"stopLoss":11.76,"takeProfit":13.28});
                c["precisionIdentity"] = json!(true);
                cases.push(c);
            }
        }
    }
    assert_eq!(cases.len(), 265, "KUCOIN-CARDINALITY formatter input total");
    assert_eq!(
        cases
            .iter()
            .filter(|c| c["precisionIdentity"] == true)
            .count(),
        30,
        "KUCOIN-CARDINALITY formatter supplement total"
    );
    let expected = oracle(&cases);
    assert_eq!(
        expected.len(),
        265,
        "KUCOIN-CARDINALITY formatter source total"
    );
    let mut comparisons = 0;
    for index in 0..265 {
        let c = &cases[index];
        let expected = &expected[index];
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(serde_json::from_value(c["markets"].clone()).unwrap()),
                positions: Some(serde_json::from_value(c["positions"].clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        if c["precisionIdentity"] == true {
            let markets = store.markets_by_symbol();
            let market = markets.get("BTCUSDT").unwrap();
            for (field, native) in [
                ("amount", market.precision.amount),
                ("price", market.precision.price),
            ] {
                assert_eq!(
                    native.to_bits(),
                    c["markets"][0]["precision"][field]
                        .as_f64()
                        .unwrap()
                        .to_bits(),
                    "KUCOIN-PRECISION native.{field}-step"
                );
            }
        }
        let runtime = Rc::new(Runtime::default());
        let settings = ExchangeSettings {
            client_order_id_rule: Some(pattern_client_order_id_rule(
                "KuCoin".into(),
                "/^[A-Za-z0-9_-]{1,40}$/".into(),
                Rc::new(|id| {
                    !id.is_empty()
                        && id.len() <= 40
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                }),
                ClientOrderIdRuleOverrides::default(),
            )),
            ..Default::default()
        };
        let base = BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            store,
            runtime.clone(),
            settings,
        );
        let mut opts: PlaceOrderOpts = serde_json::from_value(c["opts"].clone()).unwrap();
        if let Some(special) = c.get("special") {
            let value = match special["value"].as_str().unwrap() {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                _ => f64::NEG_INFINITY,
            };
            match special["field"].as_str().unwrap() {
                "amount" => opts.amount = value,
                "price" => opts.price = Some(value),
                _ => opts.take_profit = Some(value),
            }
        }
        runtime.ids.set(0);
        let create_result = format_create_order(&base, &opts).map(J::Array);
        if let Ok(payload) = &create_result {
            assert_native_precision("create", payload, &expected["formatCreateOrder"]);
        }
        let create = project(create_result, runtime.ids.get());
        runtime.ids.set(0);
        let algo_result = format_algo_order(&base, &opts);
        if let Ok(payload) = &algo_result {
            assert_native_precision("algo", payload, &expected["formatAlgoOrder"]);
        }
        let algo = project(algo_result, runtime.ids.get());
        assert_eq!(
            json!({"formatCreateOrder":create,"formatAlgoOrder":algo}),
            *expected,
            "KuCoin formatter price/amount precision identity: {c}"
        );
        comparisons += 2;
    }
    assert_eq!(
        comparisons, 530,
        "KUCOIN-CARDINALITY formatter native comparisons"
    );
}
fn assert_native_precision(role: &str, actual: &J, source: &Value) {
    let wire: Value = serde_json::from_str(source["ok"]["wire"].as_str().unwrap()).unwrap();
    fn check(role: &str, actual: &J, expected: &Value) {
        let J::Object(fields) = actual else {
            panic!("native precision payload object")
        };
        for field in [
            "size",
            "price",
            "stopPrice",
            "triggerStopUpPrice",
            "triggerStopDownPrice",
        ] {
            let actual = fields
                .iter()
                .find(|(key, _)| key == field)
                .map(|(_, value)| value);
            assert_eq!(
                actual.is_some_and(|value| !matches!(value, J::Undefined)),
                expected.get(field).is_some(),
                "KUCOIN-PRECISION {role}.{field} presence"
            );
            if let Some(value) = expected.get(field) {
                let actual = actual.unwrap();
                let J::String(actual) = actual else {
                    panic!("native precision string: {field}")
                };
                assert_eq!(
                    actual,
                    value.as_str().unwrap(),
                    "KUCOIN-PRECISION {role}.{field}"
                );
            }
        }
    }
    match actual {
        J::Array(rows) => {
            let expected = wire.as_array().unwrap();
            assert_eq!(
                expected.len(),
                1,
                "KUCOIN-CARDINALITY formatter source payload rows"
            );
            assert_eq!(
                rows.len(),
                1,
                "KUCOIN-CARDINALITY formatter native payload rows"
            );
            check(role, &rows[0], &expected[0]);
        }
        row => check(role, row, &wire),
    }
}
fn project(result: Result<J, ExchangeError>, ids: usize) -> Value {
    match result {
        Ok(value) => {
            let keys = match &value {
                J::Array(v) => json!(v.iter().map(keys).collect::<Vec<_>>()),
                v => json!(keys(v)),
            };
            json!({"ok":{"wire":js_json_stringify(&value).unwrap().unwrap(),"keys":keys,"ids":ids}})
        }
        Err(error) => json!({"error":error.message,"ids":ids}),
    }
}
fn keys(value: &J) -> Vec<&str> {
    match value {
        J::Object(v) => v.iter().map(|(key, _)| key.as_str()).collect(),
        _ => panic!("expected payload object"),
    }
}

fn oracle(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/formatters.cjs"
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
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected.len(),
        cases.len(),
        "KUCOIN-CARDINALITY formatter source case count"
    );
    expected
}

#[test]
fn inherited_market_payloads_preserve_errors_and_id_calls() {
    let mut cases = vec![];
    for symbol in [
        "constructor",
        "toString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
        "__proto__",
    ] {
        for amount in [0, 1] {
            for kind in [
                safe_cex_types::OrderType::Limit,
                safe_cex_types::OrderType::Market,
                safe_cex_types::OrderType::StopLoss,
                safe_cex_types::OrderType::TakeProfit,
                safe_cex_types::OrderType::TrailingStopLoss,
            ] {
                cases.push(json!({"markets":[],"positions":[],"opts":{"symbol":symbol,"type":kind,"side":"buy","amount":amount,"price":2}}));
            }
        }
    }
    assert_eq!(
        cases.len(),
        120,
        "KUCOIN-CARDINALITY inherited formatter input total"
    );
    let sources = oracle(&cases);
    assert_eq!(
        sources.len(),
        120,
        "KUCOIN-CARDINALITY inherited formatter source total"
    );
    let mut comparisons = 0;
    for index in 0..120 {
        let case = &cases[index];
        let source = &sources[index];
        let runtime = Rc::new(Runtime::default());
        let base = BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let opts: PlaceOrderOpts = serde_json::from_value(case["opts"].clone()).unwrap();
        let create = project(
            format_create_order(&base, &opts).map(J::Array),
            runtime.ids.get(),
        );
        assert_eq!(
            create, source["formatCreateOrder"],
            "inherited create branch and IDs: {case}"
        );
        runtime.ids.set(0);
        let algo = project(format_algo_order(&base, &opts), runtime.ids.get());
        assert_eq!(
            algo, source["formatAlgoOrder"],
            "inherited algo wire, own keys and IDs: {case}"
        );
        comparisons += 2;
    }
    assert_eq!(
        comparisons, 240,
        "KUCOIN-CARDINALITY inherited formatter native comparisons"
    );
}

#[test]
fn replaced_store_prototype_remains_a_shared_write_finding() {
    let market = json!({"id":"prototype-market","symbol":"__proto__","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":100}},"contractSize":0.001});
    let case = json!({"markets":[market],"positions":[],"opts":{"symbol":"precision","type":"limit","side":"buy","amount":0,"price":2}});
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(serde_json::from_value(case["markets"].clone()).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        store.markets().len(),
        1,
        "KUCOIN-STORE native admitted market count"
    );
    {
        let by_symbol = store.markets_by_symbol();
        assert_eq!(by_symbol.len(), 1, "KUCOIN-STORE native own lookup count");
        let market = by_symbol
            .get("__proto__")
            .expect("KUCOIN-STORE native admitted prototype symbol");
        assert_eq!(market.id, "prototype-market");
        assert_eq!(market.symbol, "__proto__");
        assert_eq!(market.base, "BTC");
        assert_eq!(market.quote, "USDT");
        assert!(market.active);
        assert_eq!(market.precision.amount.to_bits(), 1f64.to_bits());
        assert_eq!(market.precision.price.to_bits(), 0.1f64.to_bits());
        assert_eq!(market.contract_size, Some(0.001));
        assert!(
            by_symbol.get("precision").is_none(),
            "KUCOIN-STORE native homogeneous lookup has no precision market"
        );
    }
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoin,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        store,
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let opts: PlaceOrderOpts = serde_json::from_value(case["opts"].clone()).unwrap();
    assert_eq!(
        format_create_order(&base, &opts).unwrap_err().message,
        "Market precision not found"
    );
    assert_eq!(
        format_algo_order(&base, &opts).unwrap_err().message,
        "Market precision not found"
    );
    assert_eq!(runtime.ids.get(), 0);
    let source = oracle(std::slice::from_ref(&case)).remove(0);
    assert_eq!(
        source["formatCreateOrder"]["error"],
        "Cannot read properties of undefined (reading 'price')",
        "source custom-prototype lookup reaches precision object"
    );
    assert_eq!(source["formatCreateOrder"]["ids"], 0);
    assert_eq!(source["formatAlgoOrder"]["ok"]["ids"], 1);
    let wire: Value =
        serde_json::from_str(source["formatAlgoOrder"]["ok"]["wire"].as_str().unwrap()).unwrap();
    assert_eq!(wire["closeOrder"], true);
    assert!(wire.get("symbol").is_none());
}

#[test]
fn own_property_store_substitution_fails_the_replaced_prototype_witness() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "replaced_store_prototype_remains_a_shared_write_finding",
            "--nocapture",
        ])
        .env(
            "NODE_OPTIONS",
            concat!(
                "--require=",
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/oracle-controls.cjs"
            ),
        )
        .env("KUCOIN_ORACLE_MUTATION", "store-own-proto")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("source real Store retains original prototype market")
    );
}
