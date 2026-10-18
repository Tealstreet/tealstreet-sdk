use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_number_output,
    numeric_ingress::{
        JsNumberInput, JsNumberOutput, js_deserialize_passthrough, js_from_str, js_value_as_f64,
    },
};
use safe_cex_venue_breakoutprop::market_reads::fetch_combined_instruments;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn, ready},
    process::Command,
    sync::LazyLock,
    task::{Context, Poll, Waker},
};
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/combined_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["cases"].as_array().unwrap().len(),
        19,
        "COMBINED-SOURCE total"
    );
    assert_eq!(
        value["lifecycle"].as_array().unwrap().len(),
        3,
        "COMBINED-SOURCE lifecycle total"
    );
    value
});
#[derive(Serialize)]
#[serde(transparent)]
struct Raw {
    value: Value,
}
impl JsNumberOutput for Raw {
    fn check_number_output(&self) {
        js_number_output!(self,struct {} passthrough {value});
    }
}
fn native(input: JsNumberInput) -> Value {
    input
        .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
        .unwrap()
        .value
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => {
            let n = js_value_as_f64(v).unwrap();
            json!({"number":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Value::Array(a) => Value::Array(a.iter().map(observe).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let mut f = std::pin::pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture should complete"),
    }
}
#[test]
fn combined_source_parity() {
    let names = [
        "ordered-duplicates",
        "empty-instruments",
        "empty-details",
        "no-match",
        "left-falsy-0",
        "right-falsy-0",
        "left-falsy-1",
        "right-falsy-1",
        "left-falsy-2",
        "right-falsy-2",
        "left-falsy-3",
        "right-falsy-3",
        "left-falsy-4",
        "right-falsy-4",
        "unicode",
        "left-reject",
        "right-reject",
        "missing-unused",
        "surrogate",
    ];
    let counts = [3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 3, 3];
    let mut comparisons = 0;
    for i in 0..19 {
        let case = &ORACLE["cases"][i];
        let name = names[i];
        assert_eq!(case["name"], name, "COMBINED-SOURCE case identity");
        if i != 15 && i != 16 {
            assert_eq!(
                case["expected"]["ok"].as_array().unwrap().len(),
                counts[i],
                "COMBINED-SOURCE per-case output count"
            );
        }
        if i != 18 {
            let source_left: JsNumberInput = js_from_str(case["left"].as_str().unwrap()).unwrap();
            let source_right: JsNumberInput = js_from_str(case["right"].as_str().unwrap()).unwrap();
            let l = native(source_left);
            let r = native(source_right);
            if !(4..14).contains(&i) || i % 2 != 0 {
                let expected = if i == 1 {
                    vec![]
                } else if i == 14 {
                    vec!["é😀USD", "B", "A"]
                } else {
                    vec!["A", "B", "A"]
                };
                let rows = l["instruments"].as_array().unwrap();
                assert_eq!(
                    rows.len(),
                    expected.len(),
                    "COMBINED-INPUT instrument count"
                );
                for index in 0..expected.len() {
                    assert_eq!(
                        rows[index]["symbol"], expected[index],
                        "COMBINED-INPUT instrument duplicate order"
                    );
                }
            }
            if !(4..14).contains(&i) || i % 2 == 0 {
                let expected = if i == 2 {
                    vec![]
                } else if i == 3 {
                    vec!["MISS"; 4]
                } else if i == 14 {
                    vec!["B", "é😀USD", "MISS", "A"]
                } else {
                    vec!["B", "A", "MISS", "A"]
                };
                let rows = r["instrumentDetails"].as_array().unwrap();
                assert_eq!(rows.len(), expected.len(), "COMBINED-INPUT detail count");
                for index in 0..expected.len() {
                    assert_eq!(
                        rows[index]["symbol"], expected[index],
                        "COMBINED-INPUT detail duplicate order"
                    );
                }
            }
        }
        let left = js_from_str::<JsNumberInput>(case["left"].as_str().unwrap());
        let right = js_from_str::<JsNumberInput>(case["right"].as_str().unwrap());
        if name == "surrogate" {
            assert_eq!(case["expected"]["ok"][1]["instrumentUnit"], 55296);
            let captured: safe_cex_types::retained_json::RetainedJsonInput =
                left.as_ref().unwrap().clone().decode().unwrap();
            assert_eq!(
                captured.get("instruments").unwrap().as_array().unwrap()[0]
                    .get("symbol")
                    .unwrap()
                    .as_text(),
                Some([0xd800].as_slice())
            );
            assert!(right.is_ok(), "retained right input admission");
        }
        let mut original = ExchangeError::new("original-rejection");
        original.store_subscriber_ids = vec![19, 31];
        let trace = RefCell::new(Vec::<String>::new());
        let future = fetch_combined_instruments(
            || {
                trace.borrow_mut().push("instruments".into());
                ready(if name == "left-reject" {
                    Err(original.clone())
                } else {
                    Ok(Some(left.unwrap()))
                })
            },
            || {
                trace.borrow_mut().push("details".into());
                ready(if name == "right-reject" {
                    Err(original.clone())
                } else {
                    Ok(Some(right.unwrap()))
                })
            },
        );
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "COMBINED-EAGER both factories"
        );
        let result = run(future);
        if name.ends_with("reject") {
            assert_eq!(case["expected"]["sameError"], true);
            assert_eq!(
                result.err().unwrap(),
                original,
                "COMBINED-ERROR unchanged metadata"
            );
        } else if name == "surrogate" {
            let output: safe_cex_types::retained_json::RetainedJsonInput =
                result.unwrap().decode().unwrap();
            let rows = output.as_array().unwrap();
            assert_eq!(rows.len(), counts[i], "native retained join count");
            for (row, expected) in rows.iter().zip(case["expected"]["ok"].as_array().unwrap()) {
                for (key, field) in [
                    ("instrument", "instrumentUnit"),
                    ("instrumentDetails", "detailUnit"),
                ] {
                    let units = row
                        .get(key)
                        .unwrap()
                        .get("symbol")
                        .unwrap()
                        .as_text()
                        .unwrap();
                    assert_eq!(u64::from(units[0]), expected[field].as_u64().unwrap());
                    assert_eq!(units.len(), 1, "source join symbols are one code unit");
                }
            }
        } else {
            let output = native(result.unwrap());
            assert_eq!(
                output.as_array().unwrap().len(),
                counts[i],
                "COMBINED-NATIVE independent output count {name}"
            );
            assert_eq!(
                observe(&output),
                case["expected"]["ok"],
                "COMBINED-NATIVE fields {name}"
            );
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 19, "COMBINED-COMPARISONS fixed total");
}
#[test]
fn combined_concurrent_poll_and_drop() {
    for case in ORACLE["lifecycle"].as_array().unwrap() {
        assert_eq!(
            case["finished"], 2,
            "COMBINED-SOURCE discarded/rejected promise continuation"
        );
    }
    let left_ready = Cell::new(false);
    let right_polled = Cell::new(false);
    let left_polls = Cell::new(0);
    let right_polls = Cell::new(0);
    let future = fetch_combined_instruments(
        || {
            poll_fn(|_| {
                left_polls.set(left_polls.get() + 1);
                if left_ready.get() {
                    Poll::Ready(Ok(None))
                } else {
                    Poll::Pending
                }
            })
        },
        || {
            poll_fn(|_| {
                right_polls.set(right_polls.get() + 1);
                right_polled.set(true);
                Poll::Ready(Ok(None))
            })
        },
    );
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert!(
        right_polled.get(),
        "COMBINED-CONCURRENT right polled while left pending"
    );
    left_ready.set(true);
    assert!(future.as_mut().poll(&mut cx).is_ready());
    assert_eq!(
        right_polls.get(),
        1,
        "COMBINED-CONCURRENT completed child not repolled"
    );
    drop(future);
    // Dropping the borrowed host-driven future does not detach background work.
    let polls = Cell::new(0);
    {
        let future = fetch_combined_instruments(
            || {
                poll_fn(|_| {
                    polls.set(polls.get() + 1);
                    Poll::Pending::<Result<Option<JsNumberInput>, ExchangeError>>
                })
            },
            || ready(Ok(None)),
        );
        drop(future);
    }
    assert_eq!(polls.get(), 0, "COMBINED-DROP unpolled child");
    {
        let mut future = Box::pin(fetch_combined_instruments(
            || {
                poll_fn(|_| {
                    polls.set(polls.get() + 1);
                    Poll::Pending::<Result<Option<JsNumberInput>, ExchangeError>>
                })
            },
            || ready(Ok(None)),
        ));
        assert!(future.as_mut().poll(&mut cx).is_pending());
        drop(future);
    }
    assert_eq!(polls.get(), 1, "COMBINED-DROP pending child stops");
}
#[test]
fn combined_owned_snapshot() {
    assert_eq!(
        ORACLE["aliases"],
        json!({"instrument":"changed","details":"changed"})
    );
    let case = &ORACLE["cases"][0];
    let left: JsNumberInput = js_from_str(case["left"].as_str().unwrap()).unwrap();
    let right: JsNumberInput = js_from_str(case["right"].as_str().unwrap()).unwrap();
    let left_saved = left.clone();
    let right_saved = right.clone();
    let mut output = native(
        run(fetch_combined_instruments(
            || ready(Ok(Some(left))),
            || ready(Ok(Some(right))),
        ))
        .unwrap(),
    );
    output[0]["instrument"]["description"] = json!("changed");
    output[0]["instrumentDetails"]["account"] = json!("changed");
    assert_ne!(
        native(left_saved)["instruments"][1]["description"],
        "changed",
        "COMBINED-OWNED instrument"
    );
    assert_ne!(
        native(right_saved)["instrumentDetails"][0]["account"],
        "changed",
        "COMBINED-OWNED details"
    );
}

#[test]
fn combined_host_poll_order_boundary() {
    assert_eq!(
        ORACLE["rejectionOrder"], "right",
        "COMBINED-SOURCE rejection settlement order"
    );
    let error = run(fetch_combined_instruments(
        || ready(Err::<Option<JsNumberInput>, _>(ExchangeError::new("left"))),
        || ready(Err::<Option<JsNumberInput>, _>(ExchangeError::new("right"))),
    ))
    .err()
    .unwrap();
    assert_eq!(
        error.message, "left",
        "COMBINED-HOST simultaneous ready rejection poll order"
    );
}

struct PendingChild<'a> {
    drops: &'a Cell<usize>,
}
impl Future for PendingChild<'_> {
    type Output = Result<Option<JsNumberInput>, ExchangeError>;
    fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for PendingChild<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
#[test]
fn combined_rejection_releases_sibling() {
    for (i, name) in ["discard", "reject-left", "reject-right"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            ORACLE["lifecycle"][i]["name"], *name,
            "COMBINED-LIFECYCLE pinned mode"
        );
        assert_eq!(
            ORACLE["lifecycle"][i]["caught"],
            i != 0,
            "COMBINED-LIFECYCLE original rejection caught before sibling finishes"
        );
    }
    let mut original = ExchangeError::new("identity");
    original.store_subscriber_ids = vec![23, 41];
    let drops = Cell::new(0);
    let result = run(fetch_combined_instruments(
        || ready(Err(original.clone())),
        || PendingChild { drops: &drops },
    ));
    assert_eq!(
        result.err().unwrap(),
        original,
        "COMBINED-REJECT left metadata"
    );
    assert_eq!(drops.get(), 1, "COMBINED-REJECT right sibling released");
    let result = run(fetch_combined_instruments(
        || PendingChild { drops: &drops },
        || ready(Err(original.clone())),
    ));
    assert_eq!(
        result.err().unwrap(),
        original,
        "COMBINED-REJECT right metadata"
    );
    assert_eq!(drops.get(), 2, "COMBINED-REJECT left sibling released");
}
