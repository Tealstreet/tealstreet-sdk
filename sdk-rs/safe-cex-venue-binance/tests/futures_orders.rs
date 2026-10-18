#[path = "support/precision_identity.rs"]
mod precision_identity;

use safe_cex_exchange::{client_order_id::*, exchange::*, runtime::*};
use safe_cex_num::number_from_string;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_binance::futures_orders::format_futures_create_order;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "test-id".into()
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
fn numeric(c: &Value, name: &str) -> f64 {
    number_from_string(c[name].as_str().unwrap())
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn futures_payloads_and_preflight_match_live_typescript() {
    run_cases(false);
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn futures_main_precision_identity() {
    run_cases(true);
}

fn run_cases(precision_supplement: bool) {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/futures-orders.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .args(if precision_supplement {
            vec!["--precision-identity"]
        } else {
            vec![]
        })
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let expected_total = if precision_supplement {
        precision_identity::CASES
    } else {
        2732
    };
    assert_eq!(
        cases.len(),
        expected_total,
        "SOURCE-CARDINALITY futures_orders total"
    );
    assert_eq!(
        cases.iter().filter(|c| c["fieldIdentity"] == true).count(),
        if precision_supplement { 0 } else { 12 }
    );
    let mut source_brackets = 0;
    let mut native_brackets = 0;
    let mut completed_comparisons = 0;
    for (index, c) in cases.iter().enumerate() {
        if precision_supplement {
            precision_identity::assert_input(c, index);
        }
        let mut opts: PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":c["type"],"side":c["side"],"amount":1}),
        )
        .unwrap();
        opts.amount = numeric(c, "amount");
        opts.close = c.get("close").and_then(Value::as_bool);
        opts.reduce_only = c.get("reduceOnly").and_then(Value::as_bool);
        opts.sl_trigger_type = c
            .get("sl")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        opts.tp_trigger_type = c
            .get("tp")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        opts.price = c.get("price").map(|_| numeric(c, "price"));
        opts.stop_loss = c.get("stopLoss").map(|_| numeric(c, "stopLoss"));
        opts.take_profit = c.get("takeProfit").map(|_| numeric(c, "takeProfit"));
        opts.time_in_force = c
            .get("tif")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        opts.client_order_id = c.get("id").map(|v| v.as_str().unwrap().to_owned());
        let mut market:Market=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.1,"amount":1},"limits":{"amount":{"min":0,"max":1},"leverage":{"min":1,"max":100}}})).unwrap();
        market.precision.price = c.get("priceStep").map_or(0.1, |v| v.as_f64().unwrap());
        market.precision.amount = numeric(c, "step");
        if c["fieldIdentity"] == true {
            assert_eq!(
                market.precision.price,
                c["priceStep"].as_f64().unwrap(),
                "futures native price step identity case {index}"
            );
            assert_eq!(
                market.precision.amount,
                numeric(c, "step"),
                "futures native amount step identity case {index}"
            );
            assert_ne!(
                market.precision.price, market.precision.amount,
                "futures bracket steps must differ case {index}"
            );
        }
        if precision_supplement {
            precision_identity::assert_steps(
                index,
                market.precision.price,
                market.precision.amount,
            );
        }
        market.limits.amount.max = numeric(c, "max");
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(if c["market"] == true {
                    vec![market]
                } else {
                    vec![]
                }),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation succeeds");
        Store::set_setting(store.as_ref(), "isHedged", c["hedged"] == true)
            .expect("fixture Store mutation succeeds");
        let ticker:Ticker=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":0,"ask":0,"last":100,"mark":0,"index":0,"percentage":0,"volume":0,"quoteVolume":0,"fundingRate":0,"fundingTime":0,"openInterest":0})).unwrap();
        let mut position:Position=serde_json::from_value(json!({"id":"p","symbol":"BTCUSDT","side":if opts.side==OrderSide::Buy {"short"}else{"long"},"entryPrice":0,"notional":0,"leverage":0,"unrealizedPnl":0,"contracts":7,"liquidationPrice":0})).unwrap();
        match c["position"].as_str() {
            Some("wrong-symbol") => position.symbol = "OTHER".into(),
            Some("wrong-side") => {
                position.side = if opts.side == OrderSide::Buy {
                    safe_cex_types::ModelField::Value(PositionSide::Long)
                } else {
                    safe_cex_types::ModelField::Value(PositionSide::Short)
                }
            }
            Some("zero") => position.contracts = 0.0,
            Some("negative") => position.contracts = -7.0,
            Some("nan") => position.contracts = f64::NAN,
            Some("infinite") => position.contracts = f64::INFINITY,
            _ => {}
        }
        Store::update(
            store.as_ref(),
            StoreUpdate {
                tickers: Some(vec![ticker]),
                positions: Some(if c["position"] == "none" {
                    vec![]
                } else {
                    vec![position]
                }),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation succeeds");
        let rule = pattern_client_order_id_rule(
            "Binance".into(),
            "/^[.A-Z:/a-z0-9_-]{1,36}$/".into(),
            Rc::new(|id| {
                !id.is_empty()
                    && id.len() <= 36
                    && id.bytes().all(|c| {
                        c.is_ascii_alphanumeric()
                            || c == b'_'
                            || c == b'-'
                            || c == b'.'
                            || c == b':'
                            || c == b'/'
                    })
            }),
            Default::default(),
        );
        let base = BaseExchange::new(
            ExchangeName::Binance,
            serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            store,
            Rc::new(Runtime),
            ExchangeSettings {
                client_order_id_rule: Some(rule),
                ..Default::default()
            },
        );
        let emissions = Rc::new(RefCell::new(Vec::new()));
        let capture = emissions.clone();
        base.on_args(
            "error",
            Rc::new(move |_, message| {
                let message: Vec<serde_json::Value> = message
                    .iter()
                    .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                    .collect();
                capture.borrow_mut().push(serde_json::json!(message));
                Ok(())
            }),
        );
        let mut minted = 0;
        let actual = match format_futures_create_order(&base, &opts, || {
            minted += 1;
            format!("uuid-{minted}")
        }) {
            Ok(payloads) => {
                if precision_supplement {
                    assert!(
                        payloads.algo.is_empty(),
                        "PRECISION-IDENTITY unexpected native algo rows"
                    );
                    precision_identity::assert_payloads(&payloads.regular, index);
                }
                if c["fieldIdentity"] == true {
                    let expected = c["expected"]["value"]["algo"].as_array().unwrap();
                    let input_brackets = usize::from(c.get("stopLoss").is_some())
                        + usize::from(c.get("takeProfit").is_some());
                    let expected_brackets = expected
                        .iter()
                        .filter(|p| p.get("triggerPrice").is_some())
                        .count();
                    assert_eq!(
                        expected_brackets, input_brackets,
                        "futures source bracket cardinality case {index}"
                    );
                    source_brackets += expected_brackets;
                    assert_eq!(
                        payloads.algo.len(),
                        expected.len(),
                        "futures bracket identity count case {index}"
                    );
                    let mut checked = 0;
                    for (payload, expected) in payloads.algo.iter().zip(expected) {
                        if let Some(price) = expected.get("triggerPrice") {
                            assert_eq!(
                                payload.get("triggerPrice"),
                                Some(price),
                                "futures bracket price precision identity case {index}"
                            );
                            checked += 1;
                        }
                    }
                    assert_eq!(
                        checked, input_brackets,
                        "futures native bracket comparison cardinality case {index}"
                    );
                    native_brackets += checked;
                    assert!(
                        checked > 0,
                        "futures bracket identity must execute case {index}"
                    );
                }
                assert_eq!(
                    json!(payloads.regular),
                    c["expected"]["value"]["regular"],
                    "native regular payload routing case {index}"
                );
                assert_eq!(
                    json!(payloads.algo),
                    c["expected"]["value"]["algo"],
                    "native algo payload routing case {index}"
                );
                json!({"value":payloads,"keys":{"regular":payloads.regular.iter().map(|p|p.keys().collect::<Vec<_>>()).collect::<Vec<_>>(),"algo":payloads.algo.iter().map(|p|p.keys().collect::<Vec<_>>()).collect::<Vec<_>>() }})
            }
            Err(error) => {
                assert!(
                    !precision_supplement,
                    "PRECISION-IDENTITY native success case {index}: {error:?}"
                );
                json!({"error":error.message})
            }
        };
        assert_eq!(actual, c["expected"], "futures order case {index}: {c}");
        assert_eq!(json!(minted), c["minted"], "UUID timing case {index}: {c}");
        assert_eq!(
            emissions.borrow().len(),
            c["emitted"].as_array().unwrap().len(),
            "error emission count case {index}: {c}"
        );
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, expected_total,
        "NATIVE-CARDINALITY futures_orders completed comparisons"
    );
    assert_eq!(
        source_brackets,
        if precision_supplement { 0 } else { 16 },
        "futures source bracket total cardinality"
    );
    assert_eq!(
        native_brackets,
        if precision_supplement { 0 } else { 16 },
        "futures native bracket comparison total cardinality"
    );
    eprintln!("{} live futures order comparisons", cases.len());
}
