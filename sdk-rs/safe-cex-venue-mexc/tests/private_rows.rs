#[path = "support/private_store.rs"]
mod observed_store;
use observed_store::ObservedStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_venue_mexc::{utils::normalize_market, ws_private::MexcPrivateHandlers};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn fixture(kind: &str) -> Value {
    match kind {
        "order" => {
            json!({"orderId":"row","symbol":"BTC_USDT","side":1,"state":1,"vol":2,"dealVol":0,"price":10,"createTime":1000})
        }
        "position" => {
            json!({"symbol":"BTC_USDT","positionType":1,"holdVol":6,"holdAvgPrice":20,"createTime":1000})
        }
        "plan" => {
            json!({"id":"row","symbol":"BTC_USDT","side":1,"state":2,"vol":2,"triggerPrice":21,"createTime":1000})
        }
        "stop" => {
            json!({"stopPlanOrderId":"row","symbol":"BTC_USDT","positionType":1,"state":2,"vol":2,"stopLossPrice":5,"createTime":1000})
        }
        _ => unreachable!(),
    }
}

#[test]
fn private_rows_preserve_lookup_order_and_typed_null_coercion_failures() {
    const SCENARIOS: [(&str, usize, usize, bool); 8] = [
        ("null-frame", 0, 0, true),
        ("null-row", 1, 0, true),
        ("later-null", 2, 0, true),
        ("contract-only", 1, 1, false),
        ("bad-symbol", 1, 0, true),
        ("empty-array", 0, 0, false),
        ("primitive-row", 1, 0, false),
        ("primitive-frame", 1, 0, false),
    ];
    let (mut reads_total, mut first_total, mut later_total, mut failures) = (0, 0, 0, 0);
    for kind in ["order", "position", "plan", "stop"] {
        for (scenario, reads, writes, fails) in SCENARIOS {
            let store = ObservedStore::new();
            let market = normalize_market(
                &serde_json::Value::Object(
                    (json!({"symbol":"BTC_USDT","contractSize":0.5,"maxLeverage":20})
                        .as_object()
                        .unwrap())
                    .clone(),
                )
                .into(),
            )
            .unwrap();
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(vec![market]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            let first = Rc::new(Cell::new(0));
            let later = Rc::new(Cell::new(0));
            let count = first.clone();
            let _first = Store::subscribe(
                &store,
                Box::new(move |_, _| {
                    count.set(count.get() + 1);
                    Ok(())
                }),
            );
            let count = later.clone();
            let _later = Store::subscribe(
                &store,
                Box::new(move |_, _| {
                    count.set(count.get() + 1);
                    Ok(())
                }),
            );
            let effects = Rc::new(RefCell::new(Vec::<String>::new()));
            let log = effects.clone();
            let reject = Rc::new(move |id: String, _: f64| {
                log.borrow_mut().push(format!("rejected:{id}"));
                Ok(())
            });
            let log = effects.clone();
            let seen = Rc::new(move |id: String| {
                log.borrow_mut().push(format!("seen:{id}"));
            });
            let log = effects.clone();
            let shield = Rc::new(move |id: &str| {
                log.borrow_mut().push(format!("shield:{id}"));
                false
            });
            let handlers = MexcPrivateHandlers::new(
                Rc::new(|v| v),
                None,
                reject,
                seen,
                shield,
                Rc::new(|_| -> Result<(), ExchangeError> { panic!("no fill") }),
            );
            let mut row = fixture(kind);
            if scenario == "contract-only" {
                row.as_object_mut().unwrap().remove("symbol");
                row["contract"] = json!("BTC_USDT");
            }
            if scenario == "bad-symbol" {
                row["symbol"] = json!({"toString":0,"valueOf":0});
            }
            let frame = match scenario {
                "null-frame" => Value::Null,
                "null-row" => json!({"data":[null]}),
                "later-null" => json!({"data":[row,null]}),
                "empty-array" => json!({"data":[]}),
                "primitive-row" => json!({"data":[false]}),
                "primitive-frame" => json!(0),
                _ => json!({"data":[row]}),
            };
            let result = match kind {
                "order" => handlers.handle_order(&store, &frame),
                "position" => handlers.handle_position(&store, &frame),
                "plan" => handlers.handle_plan_order(&store, &frame),
                "stop" => handlers.handle_stop_order(&store, &frame),
                _ => unreachable!(),
            };
            assert_eq!(
                store.market_reads.get(),
                reads,
                "private native exact row lookup count {kind}/{scenario}"
            );
            assert_eq!(
                first.get(),
                writes,
                "private native row first subscriber count"
            );
            assert_eq!(
                later.get(),
                writes,
                "private native row later subscriber count"
            );
            assert_eq!(
                store.writes.borrow().len(),
                writes,
                "private native row Store write count"
            );
            let expected_effects = if scenario == "contract-only" && matches!(kind, "plan" | "stop")
            {
                vec!["seen:row".to_owned()]
            } else {
                vec![]
            };
            assert_eq!(
                *effects.borrow(),
                expected_effects,
                "private native no partial stop hooks before later null"
            );
            if fails {
                let error = result.expect_err("private native row rejects null or bad symbol");
                assert_eq!(
                    error.name, "TypeError",
                    "private native exact TypeError name"
                );
                let expected = if scenario == "bad-symbol" {
                    "Cannot convert object to primitive value".to_owned()
                } else {
                    format!(
                        "Cannot read properties of null (reading '{}')",
                        if scenario == "null-frame" {
                            "data"
                        } else {
                            "symbol"
                        }
                    )
                };
                assert_eq!(
                    error.message, expected,
                    "private native exact TypeError message"
                );
                failures += 1;
                assert_eq!(
                    store.orders().unwrap().len(),
                    0,
                    "private native refusal leaves no order write"
                );
                assert_eq!(
                    store.positions().unwrap().len(),
                    0,
                    "private native refusal leaves no position write"
                );
            } else {
                result.unwrap();
                if scenario == "contract-only" {
                    if kind == "position" {
                        let positions = store.positions().unwrap();
                        assert_eq!(positions.len(), 1);
                        assert_eq!(
                            positions[0].contracts.to_bits(),
                            6f64.to_bits(),
                            "private native outer position symbol only"
                        );
                    } else {
                        let orders = store.orders().unwrap();
                        assert_eq!(orders.len(), 1);
                        assert_eq!(
                            orders[0].amount.to_bits(),
                            2f64.to_bits(),
                            "private native outer order symbol only"
                        );
                    }
                } else {
                    assert_eq!(store.orders().unwrap().len(), 0);
                    assert_eq!(store.positions().unwrap().len(), 0);
                }
            }
            reads_total += store.market_reads.get();
            first_total += first.get();
            later_total += later.get();
        }
    }
    assert_eq!(
        reads_total, 28,
        "private native independent row getter floor"
    );
    assert_eq!(first_total, 4, "private native independent row first floor");
    assert_eq!(later_total, 4, "private native independent row later floor");
    assert_eq!(failures, 16, "private native independent TypeError floor");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_row_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual private row source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 32, "private source independent row root floor");
    for (i, kind) in ["order", "position", "plan", "stop"]
        .into_iter()
        .enumerate()
    {
        for (j, (scenario, reads, writes, fails)) in SCENARIOS.into_iter().enumerate() {
            let row = &rows[i * 8 + j];
            assert_eq!(row["kind"], kind);
            assert_eq!(row["scenario"], scenario);
            assert_eq!(row["reads"], reads);
            assert_eq!(row["first"], writes);
            assert_eq!(row["later"], writes);
            assert_eq!(!row["error"].is_null(), fails);
        }
    }
}
