use safe_cex_store::sequenced_delta_order_book::*;
use safe_cex_types::{OrderBook, OrderBookOrders};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn number(value: &Value) -> f64 {
    if let Some(number) = value.as_f64() {
        return number;
    }
    match value["number"].as_str().unwrap() {
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        "-0" => -0.0,
        other => panic!("unknown numeric tag {other}"),
    }
}
fn encoded_number(value: f64) -> Value {
    if value.is_nan() {
        json!({"number":"NaN"})
    } else if value == f64::INFINITY {
        json!({"number":"Infinity"})
    } else if value == f64::NEG_INFINITY {
        json!({"number":"-Infinity"})
    } else if value == 0.0 && value.is_sign_negative() {
        json!({"number":"-0"})
    } else {
        json!(value)
    }
}
fn sequence(value: &Value) -> OrderBookSequence {
    if let Some(text) = value.as_str() {
        OrderBookSequence::String(text.into())
    } else if let Some(text) = value["bigint"].as_str() {
        OrderBookSequence::BigInt(text.parse().unwrap())
    } else {
        OrderBookSequence::Number(number(value))
    }
}
fn levels(value: &Value) -> Vec<SequencedBookLevel> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_array().unwrap().iter().map(number).collect())
        .collect()
}
fn encoded_book(book: &OrderBook) -> Value {
    let side = |levels: &[OrderBookOrders]| -> Vec<Value> {
        levels.iter().map(|l| json!({"price":encoded_number(l.price),"amount":encoded_number(l.amount),"total":encoded_number(l.total)})).collect()
    };
    json!({"bids":side(&book.bids),"asks":side(&book.asks)})
}

fn compare(actual: &Value, expected: &Value, name: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{name}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{name}: array length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{name}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{name}: object keys");
            for (key, value) in a {
                compare(
                    value,
                    b.get(key)
                        .unwrap_or_else(|| panic!("{name}: missing {key}")),
                    &format!("{name}.{key}"),
                );
            }
        }
        _ => assert_eq!(actual, expected, "{name}"),
    }
}

#[test]
fn sequenced_book_live_typescript_every_value_and_transition() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/sequenced_book_oracle.mjs"
        ))
        .output()
        .expect("live TypeScript oracle needs Node and SAFE_CEX_TYPESCRIPT_PATH");
    assert!(
        output.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 50, "live oracle case census");
    let mut count = 0;
    for case in &cases {
        let book = Arc::new(Mutex::new(OrderBook {
            bids: vec![OrderBookOrders {
                price: 5.0,
                amount: 7.0,
                total: 99.0,
            }],
            asks: vec![OrderBookOrders {
                price: 8.0,
                amount: 9.0,
                total: f64::INFINITY,
            }],
        }));
        let retained = Arc::clone(&book);
        let mut machine = SequencedDeltaOrderBook::new(Arc::clone(&book));
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            count += 1;
            let action = &step["action"];
            let result = match action["op"].as_str().unwrap() {
                "inspect" => None,
                "reset" => {
                    machine.reset();
                    None
                }
                "total" => {
                    retained.lock().unwrap().bids[0].total = number(&action["total"]);
                    None
                }
                "snapshot" => Some(machine.apply_snapshot(&SequencedBookFrame {
                    sequence: sequence(&action["sequence"]),
                    bids: levels(&action["bids"]),
                    asks: levels(&action["asks"]),
                })),
                "delta" => Some(machine.apply_delta(&SequencedBookDelta {
                    sequence: sequence(&action["sequence"]),
                    previous_sequence: sequence(&action["previousSequence"]),
                    bids: levels(&action["bids"]),
                    asks: levels(&action["asks"]),
                })),
                op => panic!("unexpected {op}"),
            };
            let actual = json!({"result":result,"sequence":machine.current_sequence(),"ready":machine.is_ready(),"recovering":machine.is_recovering(),"book":encoded_book(&retained.lock().unwrap())});
            let mut expected = step.clone();
            expected.as_object_mut().unwrap().remove("action");
            compare(
                &actual,
                &expected,
                &format!("{} step {index}: {action}", case["name"]),
            );
        }
    }
    assert_eq!(count, 682, "live oracle transition census");
    println!(
        "{} cases, {count} transitions: all results, states, sequences and book values match live TypeScript",
        cases.len()
    );
}
