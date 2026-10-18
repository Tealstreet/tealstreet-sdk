#[path = "support/model_identity.rs"]
mod model_identity;
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask};
use safe_cex_webworker::{
    task_types::{TwapOptions, TwapState},
    twap::create_new_state,
};
use serde_json::{Value, json};
use std::{cell::Cell, future::Future, pin::Pin};
struct DrawRuntime {
    draws: Vec<f64>,
    used: Cell<usize>,
}
impl ExchangeRuntime for DrawRuntime {
    fn monotonic_now_ms(&self) -> f64 {
        panic!("state creation must not read the elapsed timer clock")
    }
    fn now_ms(&self) -> f64 {
        123456789.0
    }
    fn random(&self) -> f64 {
        let i = self.used.get();
        self.used.set(i + 1);
        self.draws[i % self.draws.len()]
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("state creation must not sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("state creation must not spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {
        panic!("state creation must not log")
    }
    fn unique_id(&self) -> String {
        panic!("state creation must not mint IDs")
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {
        panic!("state creation must not report")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("state creation must not dispatch")
    }
}
fn number(v: &Value) -> f64 {
    let s = v.as_str().unwrap();
    if s == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap())
    }
}
fn bits(v: f64) -> String {
    if v.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", v.to_bits())
    }
}
fn options() -> TwapOptions {
    serde_json::from_value(json!({"accountName":" account🚀\t","activeMarket":{"id":"BTC","symbol":" BTC🚀\t","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.01},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}},"side":"buy","size":1,"ordersCount":2,"randomnessPercentage":0,"duration":1,"chaserDelay":100,"reduceOnly":false,"useLimitOrders":false,"useChaserOrders":false})).unwrap()
}
fn state(value: &TwapState) -> Value {
    use safe_cex_types::{
        OrderSide,
        store_model::{TaskOrigin, TwapStatus},
    };
    let mut json = json!({
        "accountName": value.account_name, "activeMarket": value.active_market,
        "side": match value.side { OrderSide::Buy=>"buy",OrderSide::Sell=>"sell" },
        "status": match value.status { TwapStatus::Idle=>"idle",TwapStatus::Running=>"running",TwapStatus::Paused=>"paused",TwapStatus::Completed=>"completed" },
    });
    if let Some(v) = value.origin {
        json["origin"] = json!(match v {
            TaskOrigin::Web => "web",
            TaskOrigin::Cli => "cli",
            TaskOrigin::Webhook => "webhook",
            TaskOrigin::Automation => "automation",
        });
    }
    if let Some(v) = value.mark_for_cleanup {
        json["markForCleanup"] = json!(v);
    }
    if let Some(v) = value.started_at {
        json["startedAt"] = json!(v);
    }
    for (key, n) in [
        ("size", value.size),
        ("ordersCount", value.orders_count),
        ("ordersExecuted", value.orders_executed),
        ("nextOrderAt", value.next_order_at),
        ("createdAt", value.created_at.unwrap()),
    ] {
        json[key] = bits(n).into();
    }
    json["ordersSizes"] = json!(
        value
            .orders_sizes
            .iter()
            .map(|v| bits(*v))
            .collect::<Vec<_>>()
    );
    json
}
fn normalize_json_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalize_json_numbers).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, normalize_json_numbers(v)))
                .collect(),
        ),
        value => value,
    }
}
#[test]
fn new_state_matches_live_lodash_draws_math_errors_and_alias_witnesses() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/twap_state_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let identities = data["marketIdentity"].as_array().unwrap();
    // Existing 24 cases plus false/true for the new isPerpetual boolean.
    assert_eq!(identities.len(), 36, "TWAP-MARKET identity cases execute");
    let mut perpetual_cases = 0;
    for case in identities {
        let mut input = options();
        input.active_market = serde_json::from_value(case["input"].clone()).unwrap();
        let runtime = DrawRuntime {
            draws: vec![0.0],
            used: Cell::new(0),
        };
        let output = create_new_state(&input, &runtime).unwrap();
        if case["path"] == "/activeMarket/isPerpetual" {
            let expected = match perpetual_cases {
                0 => false,
                1 => true,
                _ => panic!("unexpected extra perpetual witness"),
            };
            assert_eq!(
                output.active_market.is_perpetual,
                Some(expected),
                "TWAP-MARKET native perpetual boolean"
            );
            perpetual_cases += 1;
        }
        assert_eq!(
            model_identity::market(&output.active_market),
            case["expected"],
            "TWAP-MARKET native identity {}",
            case["path"]
        );
    }
    assert_eq!(
        perpetual_cases, 2,
        "TWAP-MARKET fixed perpetual witness count"
    );
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4320);
    let mut errors = 0;
    let mut empty_property = 0;
    for (index, row) in rows.iter().enumerate() {
        let mut options = options();
        options.side = serde_json::from_value(row["side"].clone()).unwrap();
        options.size = number(&row["size"]);
        options.orders_count = number(&row["count"]);
        options.randomness_percentage = number(&row["pct"]);
        options.origin = row
            .get("origin")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let runtime = DrawRuntime {
            draws: row["draws"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect(),
            used: Cell::new(0),
        };
        match create_new_state(&options, &runtime) {
            Ok(value) => {
                assert_eq!(
                    normalize_json_numbers(state(&value)),
                    normalize_json_numbers(row["result"]["state"].clone()),
                    "TWAP-STATE row {index}"
                );
                let named = &row["result"]["nonIndex"];
                if value.orders_sizes.is_empty() {
                    assert_eq!(
                        *named,
                        json!({"-1":"NaN"}),
                        "EMPTY-ARRAY-PROPERTY source bug must remain witnessed"
                    );
                    assert_eq!(
                        serde_json::to_value(&value.orders_sizes).unwrap(),
                        json!([]),
                        "FINDING Vec retains no named -1 property"
                    );
                    empty_property += 1;
                } else {
                    assert_eq!(*named, json!({}));
                }
            }
            Err(error) => {
                assert_eq!(
                    json!({"name":error.name,"message":error.message}),
                    row["result"]["error"],
                    "TWAP-STATE-ERROR row {index}"
                );
                errors += 1;
            }
        }
        assert_eq!(
            runtime.used.get(),
            row["used"].as_u64().unwrap() as usize,
            "RANDOM-DRAW-COUNT row {index}"
        );
    }
    assert_eq!(errors, 864);
    assert_eq!(empty_property, 864);
    assert_eq!(
        data["alias"],
        json!({"identical":true,"stateAfterOptions":"via-options","optionsAfterState":"via-state"})
    );
    let mut opts = options();
    let runtime = DrawRuntime {
        draws: vec![0.0],
        used: Cell::new(0),
    };
    let mut value = create_new_state(&opts, &runtime).unwrap();
    opts.active_market.symbol = "via-options".into();
    assert_eq!(
        value.active_market.symbol, " BTC🚀\t",
        "SNAPSHOT options-to-state remains frozen"
    );
    value.active_market.base = "via-state".into();
    assert_eq!(
        opts.active_market.base, "BTC",
        "SNAPSHOT state-to-options remains independent"
    );
    assert_eq!(data["loneSurrogate"]["codeUnit"], 0xd800);
    let text = data["loneSurrogate"]["json"].as_str().unwrap();
    assert!(text.contains("\\ud800"));
    assert!(
        serde_json::from_str::<Value>(text).is_err(),
        "shared UTF-8 refusal of valid source string"
    );
}

#[test]
fn source_market_mutation_cannot_rewrite_the_captured_input() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/twap_state_oracle.mjs"
        ))
        .arg("--mutate-market-copy")
        .output()
        .unwrap();
    assert!(!output.status.success(), "source copy mutation must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("SOURCE-TWAP-MARKET copy identity"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
