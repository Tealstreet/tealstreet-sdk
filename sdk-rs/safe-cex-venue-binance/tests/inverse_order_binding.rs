//! Maintained-source money-path observations through the constructed inverse
//! exchange, real Store, shared HTTP signing and terminal-only scripted I/O.
#[path = "support/inverse_rest_runtime.rs"]
mod support;

use safe_cex_exchange::{
    js_runtime::{
        promise::await_js,
        value::{Js, from_json, to_json},
    },
    runtime::ExchangeFuture,
};
use safe_cex_store::Store;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const CAPTURE: &str = include_str!("fixtures/inverse-order-source.json");

fn corpus() -> Vec<Value> {
    serde_json::from_str(CAPTURE).unwrap()
}

// These corpus values are finite by construction. Compare every numeric
// observation by binary64 bits, before a JSON number spelling can hide drift.
fn numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => json!({"$f64":format!("{:016x}",n.as_f64().unwrap().to_bits())}),
        Value::Array(a) => Value::Array(a.into_iter().map(numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        v => v,
    }
}
fn normalize(value: &mut Value) {
    match value {
        Value::Array(a) => a.iter_mut().for_each(normalize),
        Value::Object(o) => {
            for (key, v) in o {
                if key == "newClientOrderId" {
                    let id = v.as_str().unwrap();
                    assert!(id.starts_with("x-s3vFFRQp") && id.len() > 9);
                    *v = json!("x-s3vFFRQp<generated>");
                } else {
                    normalize(v);
                }
            }
        }
        _ => (),
    }
}
fn decode_query(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut input = s.bytes();
    while let Some(b) = input.next() {
        bytes.push(match b {
            b'+' => b' ',
            b'%' => {
                let a = (input.next().unwrap() as char).to_digit(16).unwrap();
                let b = (input.next().unwrap() as char).to_digit(16).unwrap();
                (16 * a + b) as u8
            }
            b => b,
        });
    }
    String::from_utf8(bytes).unwrap()
}
fn run(spec: &Value) {
    let name = spec["name"].as_str().unwrap();
    let h = support::harness(support::options(), |http| {
        for route in spec["routes"].as_array().unwrap() {
            http.ok(
                route["method"].as_str().unwrap(),
                route["url"].as_str().unwrap(),
                route["data"].clone(),
            );
        }
    });
    Store::update(
        h.store.as_ref(),
        serde_json::from_value(spec["seed"].clone()).unwrap(),
        false,
    )
    .unwrap();
    h.store
        .set_setting("isHedged", spec["hedged"].as_bool().unwrap())
        .unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    h.exchange.base().on(
        "error",
        Rc::new(move |_, value| {
            observed.borrow_mut().push(to_json(value)?);
            Ok(())
        }),
    );
    let receiver = h.exchange.receiver();
    let invoked = receiver.call_method(
        spec["method"].as_str().unwrap(),
        spec["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(from_json)
            .collect(),
    );
    let outcome = match invoked {
        Ok(pending) => {
            let future: ExchangeFuture<'_, Js> = Box::pin({
                let await_runtime: std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
                    h.runtime.clone();
                async move {
                    await_js(&await_runtime, pending)
                        .await
                        .map_err(|e| e.to_exchange_error())
                }
            });
            h.drive(future)
        }
        Err(e) => Err(e.to_exchange_error()),
    };
    let (result, error) = match outcome {
        Ok(v) => (to_json(&v).unwrap(), Value::Null),
        Err(e) => (Value::Null, json!(e.message)),
    };
    let calls: Vec<_> = h
        .http
        .sent
        .lock()
        .unwrap()
        .iter()
        .map(|r| {
            assert!(
                r.body.is_none(),
                "{name}: signed payload must move to query"
            );
            assert_eq!(
                r.headers.get("X-MBX-APIKEY").map(String::as_str),
                Some("synthetic-key"),
                "{name}"
            );
            let (path, query) = r.url.split_once('?').unwrap();
            let pairs: Vec<_> = query
                .split('&')
                .map(|p| {
                    let (k, v) = p.split_once('=').unwrap();
                    (decode_query(k), decode_query(v))
                })
                .collect();
            let signature = &pairs.iter().find(|(k, _)| k == "signature").unwrap().1;
            assert!(signature.len() == 64 && signature.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(
                pairs
                    .iter()
                    .find(|(k, _)| k == "timestamp")
                    .unwrap()
                    .1
                    .parse::<f64>()
                    .unwrap()
                    .is_finite()
            );
            let params: Vec<_> = pairs
                .into_iter()
                .filter(|(k, _)| k != "timestamp" && k != "signature")
                .map(|(k, v)| {
                    let mut value = if k == "batchOrders" {
                        serde_json::from_str(&v).unwrap()
                    } else {
                        json!(v)
                    };
                    if k == "newClientOrderId" {
                        let mut wrapped = json!({"newClientOrderId":value});
                        normalize(&mut wrapped);
                        value = wrapped["newClientOrderId"].take();
                    } else {
                        normalize(&mut value);
                    }
                    json!([k, value])
                })
                .collect();
            json!({"method":r.method.to_uppercase(),"path":path,"params":params})
        })
        .collect();
    // Read the live Store facade. Do not replace mutators or reimplement venue
    // state transitions in the harness.
    let store = receiver.get("store").unwrap();
    let rows = |key: &str, fields: &[&str]| {
        let source = store.get(key).unwrap();
        (0..source.get("length").unwrap().as_f64().unwrap() as usize)
            .map(|i| {
                let row = source.get(&i.to_string()).unwrap();
                let mut out = serde_json::Map::new();
                for field in fields {
                    out.insert((*field).into(), to_json(&row.get(field).unwrap()).unwrap());
                }
                Value::Object(out)
            })
            .collect::<Vec<_>>()
    };
    let state = json!({"orders":rows("orders",&["id","status"]),"positions":rows("positions",&["id","leverage","positionMode"]),"isHedged":to_json(&store.get("options").unwrap().get("isHedged").unwrap()).unwrap(),"fetchedLeverage":to_json(&store.get("fetchedLeverage").unwrap()).unwrap()});
    assert_eq!(
        numbers(
            json!({"result":result,"error":error,"calls":calls,"events":events.borrow().clone(),"state":state})
        ),
        spec["expected"],
        "maintained source case {name}"
    );
}
fn group(names: &[&str]) {
    let cases = corpus();
    assert_eq!(cases.len(), 25);
    for name in names {
        run(cases.iter().find(|c| c["name"] == *name).unwrap());
    }
}
#[test]
fn placement_split_stops_batch_ids_and_preflight_match_source() {
    group(&[
        "limit",
        "market",
        "split-attached",
        "hedged-attached",
        "trailing-min",
        "trailing-max",
        "full-close",
        "full-close-missing",
        "format-all-before-send",
    ]);
}
#[test]
fn cancellation_and_amendment_preserve_request_and_store_ordering() {
    group(&[
        "cancel-partial-unknown",
        "cancel-group-order",
        "cancel-symbol",
        "amend-zero-price",
        "amend-cancel-failed",
        "amend-unknown-order",
    ]);
}
#[test]
fn leverage_clamping_and_failures_match_both_store_sides() {
    group(&[
        "leverage-min",
        "leverage-max",
        "leverage-unchanged",
        "leverage-failure",
        "leverage-missing",
    ]);
}
#[test]
fn position_mode_and_margin_type_match_confirmed_source_transitions() {
    group(&[
        "mode-confirmed",
        "mode-already-set",
        "mode-occupied",
        "margin-type",
        "margin-failure",
    ]);
}
#[test]
#[ignore = "requires installed maintained TypeScript/Axios dependencies"]
fn live_maintained_order_capture_is_current() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-order-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let current: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(current, serde_json::from_str::<Value>(CAPTURE).unwrap());
}
