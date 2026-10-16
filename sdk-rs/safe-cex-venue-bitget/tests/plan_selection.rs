use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::js_number_string;
use safe_cex_types::{PlaceOrderOpts, Position};
use safe_cex_venue_bitget::order_logic::{PlanSelectionOptions, get_order_plan_type_and_amount};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
fn position(contracts: f64) -> Position {
    serde_json::from_value(json!({"id":"p","symbol":"BTCUSDT","side":"short","entryPrice":1,"notional":1,"leverage":1,"unrealizedPnl":0,"contracts":contracts,"liquidationPrice":0})).unwrap()
}
fn check(oracle: &Value) {
    for case in oracle["cases"].as_array().unwrap() {
        let mut opts: PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":case["type"],"side":case["side"],"amount":0}),
        )
        .unwrap();
        opts.amount = match case["amount"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            s => s.parse().unwrap(),
        };
        let mut order:safe_cex_types::Order=serde_json::from_value(json!({"id":"existing-order","symbol":"BTCUSDT","status":"open","type":case["type"],"side":case["side"],"price":1,"amount":0,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0,"info":{"retained":"metadata"}})).unwrap();
        order.amount = opts.amount;
        let argument = if case["shape"] == "order" {
            PlanSelectionOptions::Order(&order)
        } else {
            PlanSelectionOptions::Place(&opts)
        };
        let calls = RefCell::new(Vec::new());
        let failure = case["failure"].as_str().unwrap();
        let original = Box::new(ExchangeError::new(format!("original {failure}")));
        let pointer = std::ptr::from_ref(original.as_ref());
        let original = RefCell::new(Some(original));
        let result = run(get_order_plan_type_and_amount(
            argument,
            || {
                calls.borrow_mut().push(json!(["store"]));
                vec![position(7.0)]
            },
            |symbol, side| {
                calls.borrow_mut().push(json!(["resolve", symbol, side]));
                std::future::ready(if failure == "resolve" {
                    Err(original.borrow_mut().take().unwrap())
                } else {
                    Ok(vec![position(13.0), position(2.0)])
                })
            },
            |seen, positions| {
                let (same, amount, id, info) = match seen {
                    PlanSelectionOptions::Place(seen) => (
                        std::ptr::eq(seen, &opts),
                        seen.amount,
                        Value::Null,
                        Value::Null,
                    ),
                    PlanSelectionOptions::Order(seen) => (
                        std::ptr::eq(seen, &order),
                        seen.amount,
                        json!(seen.id),
                        json!(seen.info),
                    ),
                };
                calls.borrow_mut().push(json!([
                    "select",
                    same,
                    if positions[0].contracts == 7.0 {
                        "store"
                    } else {
                        "resolved"
                    }
                ]));
                if failure == "select" {
                    return Err(original.borrow_mut().take().unwrap());
                }
                Ok(json!([
                    positions
                        .iter()
                        .map(|p| p.contracts as i64)
                        .collect::<Vec<_>>(),
                    if amount == 0.0 && amount.is_sign_negative() {
                        "-0".into()
                    } else {
                        js_number_string(amount)
                    },
                    id,
                    info
                ]))
            },
        ));
        assert_eq!(
            *calls.borrow(),
            *case["calls"].as_array().unwrap(),
            "getOrderPlanTypeAndAmount calls {}",
            case["name"]
        );
        let output = match result {
            Ok(v) => json!({"value":v}),
            Err(e) => {
                json!({"error":e.message,"sameError":std::ptr::eq(std::ptr::from_ref(e.as_ref()),pointer)})
            }
        };
        assert_eq!(
            output, case["output"],
            "getOrderPlanTypeAndAmount result {}",
            case["name"]
        );
    }
    assert_eq!(
        oracle["pendingAlias"]["calls"],
        json!([["BTCUSDT", "short"]]),
        "source resolver sees original options before await"
    );
    assert_eq!(
        oracle["pendingAlias"]["afterAwait"],
        json!({"same":true,"amount":9,"symbol":"ETHUSDT","side":"sell"}),
        "source selector sees caller mutation during await"
    );
    assert_eq!(
        oracle["alias"]["afterSelector"],
        json!([19, 23]),
        "source selector edits reach store array"
    );
    assert_eq!(
        oracle["alias"]["afterStore"],
        json!([31, 23]),
        "source store edits reach returned array"
    );
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"stop_market","side":"buy","amount":0}),
    )
    .unwrap();
    let mut store = vec![position(7.0)];
    let returned = run(get_order_plan_type_and_amount(
        PlanSelectionOptions::Place(&opts),
        || store.clone(),
        |_, _| std::future::ready(Ok::<_, ()>(Vec::new())),
        |_, positions| {
            let mut snapshot = positions.to_vec();
            snapshot[0].contracts = 19.0;
            snapshot.push(position(23.0));
            Ok(snapshot)
        },
    ))
    .unwrap();
    assert_eq!(
        store[0].contracts, 7.0,
        "CHOSEN selector snapshot edits do not reach store"
    );
    assert_eq!(
        store.len(),
        1,
        "CHOSEN selector snapshot growth does not reach store"
    );
    store[0].contracts = 31.0;
    assert_eq!(
        returned[0].contracts, 19.0,
        "CHOSEN store edits do not reach returned snapshot"
    );
    assert_eq!(returned.len(), 2);
    for witness in oracle["utf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<PlaceOrderOpts>(witness["input"].as_str().unwrap()).is_err(),
            "plan selector UTF8 ingress refusal"
        );
        assert!(
            witness["calls"].as_str().unwrap().contains("\\ud"),
            "source resolver retains lone surrogate"
        );
        assert_eq!(witness["result"], json!(["pos_loss", 0]));
    }
}
#[test]
fn frozen_plan_selection() {
    check(&serde_json::from_str(include_str!("plan-selection-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_plan_selection() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/plan-selection-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle = serde_json::from_slice(&output.stdout).unwrap();
    check(&oracle);
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("plan-selection-oracle.json")).unwrap(),
        "plan selector source drift"
    );
}
#[test]
fn dropping_selection_drops_owned_resolution() {
    use std::{cell::Cell, rc::Rc};
    struct Pending(Rc<Cell<bool>>);
    impl Future for Pending {
        type Output = Result<Vec<Position>, ()>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for Pending {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let opts: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"stop_market","side":"buy","amount":1}),
    )
    .unwrap();
    let mut future = Box::pin(get_order_plan_type_and_amount(
        PlanSelectionOptions::Place(&opts),
        || panic!("store branch must not run"),
        |_, _| Pending(dropped.clone()),
        |_, _| Ok::<_, ()>(()),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert!(!dropped.get());
    drop(future);
    assert!(dropped.get(), "plan selection owns pending resolution");
}

#[test]
#[ignore = "requires live Node and TypeScript"]
fn plan_selection_field_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/plan-selection-capture.cjs"
        ))
        .arg("--field-identity")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        cases.as_array().unwrap().len(),
        2,
        "plan field identity branch inventory"
    );
    for case in cases.as_array().unwrap() {
        let opts: PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"stop_market","side":"buy","amount":case["amount"]}),
        )
        .unwrap();
        let store: Vec<Position> = serde_json::from_value(case["store"].clone()).unwrap();
        let resolved: Vec<Position> = serde_json::from_value(case["resolved"].clone()).unwrap();
        let actual = run(get_order_plan_type_and_amount(
            PlanSelectionOptions::Place(&opts),
            || store,
            |_, _| std::future::ready(Ok::<_, ()>(resolved)),
            |_, positions| {
                let expected = case["output"].as_array().unwrap();
                assert_eq!(
                    positions.len(),
                    expected.len(),
                    "plan native position count"
                );
                for (index, (p, expected)) in positions.iter().zip(expected).enumerate() {
                    assert_eq!(p.id, expected["id"], "plan native id identity {index}");
                    assert_eq!(
                        p.symbol, expected["symbol"],
                        "plan native symbol identity {index}"
                    );
                    let side = if expected["side"] == "long" {
                        safe_cex_types::PositionSide::Long
                    } else {
                        safe_cex_types::PositionSide::Short
                    };
                    assert_eq!(p.side, side, "plan native side identity {index}");
                    for (field, number) in [
                        ("contracts", p.contracts),
                        ("entryPrice", p.entry_price),
                        ("notional", p.notional),
                        ("leverage", p.leverage),
                        ("unrealizedPnl", p.unrealized_pnl),
                        ("liquidationPrice", p.liquidation_price),
                    ] {
                        assert_eq!(
                            number.to_bits(),
                            expected[field].as_f64().unwrap().to_bits(),
                            "plan native position field identity {field} {index}"
                        );
                    }
                }
                Ok(())
            },
        ));
        actual.unwrap();
    }
}
