use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, numeric_ingress::js_from_str};
use safe_cex_venue_backpack::{
    account_reads::{cancel_symbol_orders, dispose, set_leverage},
    order_utils::decode_batch_order_response,
    types::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/runtime_methods_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture future must complete"),
    }
}
fn probe(v: &J) -> Value {
    match v {
        J::Undefined => json!(["undefined"]),
        J::Null => json!(["null"]),
        J::Bool(v) => json!(["boolean", v]),
        J::Number(v) => json!(["number", bits(*v)]),
        J::String(v) => json!(["string", v]),
        J::BigInt(v) => json!(["bigint", v.to_string()]),
        J::Array(v) => json!(["array", v.iter().map(probe).collect::<Vec<_>>()]),
        J::Object(v) => json!([
            "object",
            v.iter()
                .map(|(k, v)| json!([k, probe(v)]))
                .collect::<Vec<_>>()
        ]),
    }
}
fn object(fields: Vec<(&str, J)>) -> J {
    J::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn string(value: &str) -> J {
    J::String(value.into())
}
fn request(
    symbol: &str,
    side: BackpackOrderRequestSide,
    kind: BackpackOrderRequestOrderType,
    id: Option<f64>,
) -> BackpackOrderRequest {
    let mut r: BackpackOrderRequest =
        js_from_str(r#"{"symbol":"BTC","side":"Bid","orderType":"Limit"}"#).unwrap();
    r.symbol = symbol.into();
    r.side = side;
    r.orderType = kind;
    r.clientId = id;
    r
}
fn requests() -> Vec<BackpackOrderRequest> {
    vec![
        request(
            "BTC",
            BackpackOrderRequestSide::Bid,
            BackpackOrderRequestOrderType::Limit,
            Some(101.0),
        ),
        request(
            "ETH",
            BackpackOrderRequestSide::Ask,
            BackpackOrderRequestOrderType::Market,
            Some(202.0),
        ),
    ]
}
fn rows() -> J {
    J::Array(vec![
        object(vec![
            ("operation", string("Ok")),
            ("clientId", J::Number(101.0)),
            ("symbol", string("BTC")),
            ("side", string("Bid")),
            ("orderType", string("Limit")),
            ("id", string("order-a")),
        ]),
        object(vec![
            ("operation", string("Ok")),
            ("clientId", J::Number(202.0)),
            ("symbol", string("ETH")),
            ("side", string("Ask")),
            ("orderType", string("Market")),
            ("id", string("order-b")),
        ]),
    ])
}
fn change(data: &mut J, index: usize, key: &str, value: J) {
    let J::Array(rows) = data else { panic!() };
    let J::Object(fields) = &mut rows[index] else {
        panic!()
    };
    if matches!(value, J::Undefined) {
        fields.retain(|(k, _)| k != key);
    } else {
        fields.iter_mut().find(|(k, _)| k == key).unwrap().1 = value;
    }
}
struct DecodeCase {
    label: &'static str,
    data: J,
    requests: Vec<BackpackOrderRequest>,
}
fn decode_cases() -> Vec<DecodeCase> {
    let mut cases = Vec::new();
    let mut add = |label, data, requests| {
        cases.push(DecodeCase {
            label,
            data,
            requests,
        })
    };
    add("forward", rows(), requests());
    let J::Array(mut reversed) = rows() else {
        unreachable!()
    };
    reversed.reverse();
    add("reversed", J::Array(reversed), requests());
    add("empty", J::Array(vec![]), vec![]);
    add("null data", J::Null, requests());
    add("undefined data", J::Undefined, requests());
    add("object data", J::Object(vec![]), requests());
    let J::Array(mut short) = rows() else {
        unreachable!()
    };
    short.pop();
    add("short", J::Array(short), requests());
    let J::Array(mut long) = rows() else {
        unreachable!()
    };
    long.push(long[0].clone());
    add("long", J::Array(long), requests());
    for (label, id) in [
        ("request missing", None),
        ("request zero", Some(0.0)),
        ("request fraction", Some(1.5)),
        ("request nan", Some(f64::NAN)),
        ("request infinity", Some(f64::INFINITY)),
        ("request overflow", Some(4294967296.0)),
    ] {
        let mut req = requests();
        req[0].clientId = id;
        add(label, rows(), req);
    }
    let mut req = requests();
    req[1].clientId = Some(101.0);
    add("request duplicate", rows(), req);
    for (label, row) in [
        ("row null", J::Null),
        ("row array", J::Array(vec![])),
        ("row primitive", J::Number(1.0)),
    ] {
        let J::Array(mut data) = rows() else {
            unreachable!()
        };
        data[0] = row;
        add(label, J::Array(data), requests());
    }
    for (label, value) in [
        ("operation missing", J::Undefined),
        ("operation null", J::Null),
        ("operation zero", J::Number(0.0)),
        ("operation nan", J::Number(f64::NAN)),
        ("operation bigint", J::BigInt(5.into())),
        ("operation object", J::Object(vec![])),
        (
            "operation toString",
            object(vec![("toString", J::Number(0.0))]),
        ),
    ] {
        let mut data = rows();
        change(&mut data, 0, "operation", value);
        add(label, data, requests());
    }
    for (label, id) in [
        ("client missing", J::Undefined),
        ("client string", string("101")),
        ("client zero", J::Number(0.0)),
        ("client negative", J::Number(-1.0)),
        ("client nan", J::Number(f64::NAN)),
        ("client fraction", J::Number(1.5)),
        ("client unknown", J::Number(303.0)),
    ] {
        let mut data = rows();
        change(&mut data, 0, "clientId", id);
        add(label, data, requests());
    }
    let mut data = rows();
    change(&mut data, 1, "clientId", J::Number(101.0));
    add("client duplicate", data, requests());
    for (label, key, value) in [
        ("symbol mismatch", "symbol", string("ETH")),
        ("side mismatch", "side", string("Ask")),
        ("type mismatch", "orderType", string("Market")),
        ("id missing", "id", J::Undefined),
        ("id number", "id", J::Number(42.0)),
        ("id blank", "id", string(" \t\u{feff}")),
    ] {
        let mut data = rows();
        change(&mut data, 0, key, value);
        add(label, data, requests());
    }
    let mut data = rows();
    change(&mut data, 1, "id", string(" order-a "));
    add("id duplicate", data, requests());
    let mut data = rows();
    change(&mut data, 0, "id", string(" \u{feff}order-a\t "));
    change(&mut data, 1, "id", string("\norder-b\r"));
    add("trimmed", data, requests());
    let mut data = rows();
    let mut req = requests();
    change(&mut data, 0, "clientId", J::Number(4294967295.0));
    req[0].clientId = Some(4294967295.0);
    add("uint32 max", data, req);
    let mut data = rows();
    let mut req = requests();
    change(&mut data, 0, "symbol", string("λ🚀"));
    change(&mut data, 0, "id", string("order-λ🚀"));
    req[0].symbol = "λ🚀".into();
    add("unicode", data, req);
    let mut data = rows();
    change(&mut data, 0, "operation", J::Number(0.0));
    change(&mut data, 0, "clientId", J::Number(f64::NAN));
    add("operation precedence", data, requests());
    let mut req = requests();
    req[0].clientId = None;
    add("count precedence", J::Array(vec![]), req);
    let mut data = rows();
    change(&mut data, 0, "clientId", J::Number(303.0));
    let J::Array(data) = &mut data else {
        unreachable!()
    };
    data[1] = J::Null;
    add("row precedence", J::Array(data.clone()), requests());
    let mut data = rows();
    change(&mut data, 0, "id", string("\u{85}order-a\u{85}"));
    add("NEL retained", data, requests());
    cases
}
#[test]
fn actual_typescript_batch_response_decoder() {
    let corpus = oracle();
    let cases = corpus["decode"].as_array().unwrap();
    let specs = decode_cases();
    assert_eq!(cases.len(), 47, "BATCH-DECODE independent source cases");
    assert_eq!(specs.len(), 47, "BATCH-DECODE independent fixture floor");
    let mut compared = [0; 2];
    for (i, (case, spec)) in cases.iter().zip(specs.iter()).enumerate() {
        assert_eq!(
            case["label"], spec.label,
            "BATCH-DECODE independent label {i}"
        );
        assert_eq!(
            case["input"],
            probe(&spec.data),
            "BATCH-DECODE independent input {i}"
        );
        let view:Vec<_>=spec.requests.iter().map(|r|json!({"symbol":r.symbol,"side":match r.side{BackpackOrderRequestSide::Bid=>"Bid",BackpackOrderRequestSide::Ask=>"Ask"},"orderType":match r.orderType{BackpackOrderRequestOrderType::Limit=>"Limit",BackpackOrderRequestOrderType::Market=>"Market"},"clientId":probe(&r.clientId.map_or(J::Undefined,J::Number))})).collect();
        assert_eq!(
            case["requests"],
            json!(view),
            "BATCH-DECODE independent requests {i}"
        );
        let actual = decode_batch_order_response(&spec.data, &spec.requests);
        if [0, 1, 2, 40, 41, 42, 46].contains(&i) {
            let actual = actual.expect("BATCH-DECODE expected success");
            let expected: Vec<&str> = match i {
                2 => vec![],
                42 => vec!["order-λ🚀", "order-b"],
                46 => vec!["\u{85}order-a\u{85}", "order-b"],
                _ => vec!["order-a", "order-b"],
            };
            assert_eq!(actual, expected, "BATCH-DECODE native ordered IDs {i}");
            assert_eq!(case["outcome"]["ok"], true);
            assert_eq!(
                case["outcome"]["ids"].as_array().unwrap().len(),
                expected.len(),
                "BATCH-DECODE independent source IDs {i}"
            );
            for (j, id) in actual.iter().enumerate() {
                assert_eq!(
                    id, &case["outcome"]["ids"][j],
                    "BATCH-DECODE native source ID {i}:{j}"
                );
            }
            compared[0] += 1;
        } else {
            let error = actual.expect_err("BATCH-DECODE expected rejection");
            assert_eq!(case["outcome"]["ok"], false);
            assert_eq!(
                error.name, case["outcome"]["name"],
                "BATCH-DECODE native error name {i}"
            );
            assert_eq!(
                error.message, case["outcome"]["message"],
                "BATCH-DECODE native error message {i}"
            );
            compared[1] += 1;
        }
    }
    assert_eq!(
        compared,
        [7, 40],
        "BATCH-DECODE completed native comparisons"
    );
    typed_utf16::assert_projection(
        corpus["boundary"]["requestRaw"].as_str().unwrap(),
        &(js_from_str::<BackpackOrderRequest>(corpus["boundary"]["requestRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    assert_eq!(corpus["boundary"]["result"], json!(["ok"]));
}
fn failure(name: &str, message: String, id: u64) -> ExchangeError {
    let mut e = ExchangeError::new(message);
    e.name = name.into();
    e.store_subscriber_ids = vec![id];
    e
}
#[test]
fn actual_typescript_dispose() {
    let corpus = oracle();
    let cases = corpus["lifecycle"].as_array().unwrap();
    assert_eq!(cases.len(), 6, "DISPOSE independent source cases");
    for (i, case) in cases.iter().enumerate() {
        let state = Cell::new(i >= 3);
        let trace = RefCell::new(Vec::new());
        let fail = ["none", "public", "private"][i % 3];
        assert_eq!(case["initially"], i >= 3);
        assert_eq!(case["fail"], fail);
        let error = failure("DisposeError", format!("{fail} dispose failed"), 17);
        let actual = dispose(
            &state,
            || {
                trace.borrow_mut().push(json!(["public", state.get()]));
                if fail == "public" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            },
            || {
                trace.borrow_mut().push(json!(["private", state.get()]));
                if fail == "private" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            },
        );
        assert!(state.get(), "DISPOSE native disposed state {i}");
        assert_eq!(case["after"], true);
        let count = if fail == "public" { 1 } else { 2 };
        assert_eq!(
            trace.borrow().len(),
            count,
            "DISPOSE independent native call count {i}"
        );
        assert_eq!(case["trace"].as_array().unwrap().len(), count);
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "DISPOSE native callback order {i}"
        );
        if fail == "none" {
            actual.unwrap();
            assert_eq!(case["outcome"]["ok"], true);
        } else {
            assert_eq!(
                actual.unwrap_err(),
                error,
                "DISPOSE native original failure {i}"
            );
            assert_eq!(case["outcome"]["sameFailure"], true);
        }
    }
    let token = Rc::new(vec![1u8, 2, 3]);
    let state = Cell::new(false);
    let failure = dispose(
        &state,
        || Err(token.clone()),
        || -> Result<(), Rc<Vec<u8>>> { panic!("private callback after opaque failure") },
    )
    .unwrap_err();
    assert!(state.get(), "DISPOSE opaque native state");
    assert!(
        Rc::ptr_eq(&failure, &token),
        "DISPOSE opaque native exception identity"
    );
    assert_eq!(
        corpus["lifecycleOpaque"],
        json!({"after":true,"trace":["public"],"sameOpaque":true})
    );
}
#[test]
fn actual_typescript_set_leverage() {
    let corpus = oracle();
    let cases = corpus["leverage"].as_array().unwrap();
    assert_eq!(cases.len(), 45, "LEVERAGE-WRITE independent source cases");
    let values = [2.5, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
    let stages = [
        "ok",
        "patch sync",
        "patch async",
        "info",
        "positions sync",
        "positions async",
        "update",
        "emit",
        "error log",
    ];
    let counts = [4, 3, 3, 4, 5, 5, 6, 2, 3];
    let mut compared = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        let leverage = values[i / 9];
        let stage = stages[i % 9];
        assert_eq!(case["value"], bits(leverage));
        assert_eq!(case["stage"], stage);
        let state = Cell::new(17.0);
        let trace = RefCell::new(Vec::new());
        let error = failure("StageError", format!("{stage} failed"), 17);
        let replacement = failure("CallbackError", format!("{stage} replacement"), 19);
        let snapshot = Rc::new(leverage);
        let fetched = snapshot.clone();
        let pending = set_leverage(
            &state,
            leverage,
            |endpoint, body| {
                let J::Object(fields) = body else {
                    panic!("LEVERAGE-WRITE native PATCH object {i}")
                };
                assert_eq!(
                    fields.len(),
                    1,
                    "LEVERAGE-WRITE independent PATCH field count {i}"
                );
                assert_eq!(
                    fields[0].0, "leverageLimit",
                    "LEVERAGE-WRITE native PATCH field {i}"
                );
                let J::String(text) = &fields[0].1 else {
                    panic!("LEVERAGE-WRITE native PATCH value {i}")
                };
                trace.borrow_mut().push(json!(["patch", endpoint, text]));
                if stage == "patch sync" {
                    return Err(error.clone());
                }
                Ok(std::future::ready(
                    if ["patch async", "emit", "error log"].contains(&stage) {
                        Err(error.clone())
                    } else {
                        Ok(())
                    },
                ))
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if stage == "info" && severity == "info" {
                    Err(error.clone())
                } else if stage == "error log" && severity == "error" {
                    Err(replacement.clone())
                } else {
                    Ok(())
                }
            },
            |event, e| {
                assert!(
                    e.store_subscriber_ids.is_empty(),
                    "LEVERAGE-WRITE new emitted error metadata {i}"
                );
                trace
                    .borrow_mut()
                    .push(json!(["emit", event, e.name, e.message]));
                if stage == "emit" {
                    Err(replacement.clone())
                } else {
                    Ok(())
                }
            },
            || {
                trace.borrow_mut().push(json!(["positions"]));
                if stage == "positions sync" {
                    return Err(error.clone());
                }
                Ok(std::future::ready(if stage == "positions async" {
                    Err(error.clone())
                } else {
                    Ok(fetched)
                }))
            },
            |payload| {
                let actual = payload.positions;
                assert!(
                    Rc::ptr_eq(&actual, &snapshot),
                    "LEVERAGE-WRITE native positions identity {i}"
                );
                assert_eq!(
                    bits(*actual),
                    bits(leverage),
                    "LEVERAGE-WRITE native fetched position value {i}"
                );
                trace.borrow_mut().push(json!([
                    "update",
                    probe(&object(vec![
                        ("opaque", string("positions")),
                        ("value", J::Number(*actual))
                    ]))
                ]));
                if stage == "update" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            bits(state.get()),
            bits(17.0),
            "LEVERAGE-WRITE native pre-await state {i}"
        );
        assert_eq!(case["before"], bits(17.0));
        let initial = if stage == "patch sync" { 3 } else { 1 };
        assert_eq!(
            trace.borrow().len(),
            initial,
            "LEVERAGE-WRITE independent eager effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["atConstruction"].as_array().unwrap(),
            "LEVERAGE-WRITE native construction trace {i}"
        );
        let actual = ready(pending);
        let after = if ["patch sync", "patch async", "emit", "error log"].contains(&stage) {
            17.0
        } else {
            leverage
        };
        assert_eq!(
            bits(state.get()),
            bits(after),
            "LEVERAGE-WRITE native committed state {i}"
        );
        assert_eq!(case["after"], bits(after));
        assert_eq!(
            trace.borrow().len(),
            counts[i % 9],
            "LEVERAGE-WRITE independent final effects {i}"
        );
        assert_eq!(case["trace"].as_array().unwrap().len(), counts[i % 9]);
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "LEVERAGE-WRITE native full trace {i}"
        );
        if stage == "ok" {
            actual.unwrap();
            assert_eq!(case["outcome"]["ok"], true);
            compared[0] += 1;
        } else {
            let expected = if stage == "emit" || stage == "error log" {
                replacement
            } else {
                error
            };
            assert_eq!(
                actual.unwrap_err(),
                expected,
                "LEVERAGE-WRITE native error precedence {i}"
            );
            assert_eq!(case["outcome"]["name"], expected.name);
            assert_eq!(case["outcome"]["message"], expected.message);
            assert_eq!(
                case["outcome"]["sameReplacement"],
                stage == "emit" || stage == "error log"
            );
            compared[1] += 1;
        }
    }
    assert_eq!(compared, [5, 40], "LEVERAGE-WRITE completed comparisons");
    assert_eq!(corpus["leverageHost"]["after"], bits(17.0));
    assert_eq!(corpus["leverageHost"]["sameHost"], true);
    assert_eq!(
        corpus["leverageHost"]["trace"],
        json!([
            "patch",
            ["emit", "error", "Error", "Failed to set leverage: opaque λ"],
            ["log", "Failed to set leverage: opaque λ", "error"]
        ])
    );
    // The native callback signature uses ExchangeError, so this executable
    // throwable has no native admission path; the manifest names that choice.
}
#[test]
fn actual_typescript_cancel_symbol_orders() {
    let corpus = oracle();
    let cases = corpus["cancel"].as_array().unwrap();
    assert_eq!(cases.len(), 6, "CANCEL-SYMBOL independent source cases");
    let stages = [
        "missing",
        "ok",
        "lookup",
        "delete sync",
        "delete async",
        "logger",
    ];
    for (i, case) in cases.iter().enumerate() {
        let stage = stages[i];
        assert_eq!(case["stage"], stage);
        let trace = RefCell::new(Vec::new());
        let lookups = Cell::new(0);
        let error = failure("StageError", format!("{stage} failed"), 17);
        let replacement = failure("CallbackError", "logger replacement".into(), 19);
        let pending = cancel_symbol_orders(
            "λ🚀",
            |symbol| {
                assert_eq!(symbol, "λ🚀");
                lookups.set(lookups.get() + 1);
                if stage == "lookup" {
                    Err(error.clone())
                } else if stage == "missing" {
                    Ok(None)
                } else {
                    Ok(Some("VENUE-λ".into()))
                }
            },
            |endpoint, body| {
                let J::Object(fields) = body else {
                    panic!("CANCEL-SYMBOL native config object {i}")
                };
                assert_eq!(
                    fields.len(),
                    1,
                    "CANCEL-SYMBOL independent config field count {i}"
                );
                assert_eq!(fields[0].0, "data", "CANCEL-SYMBOL native config field {i}");
                let J::Object(data) = &fields[0].1 else {
                    panic!("CANCEL-SYMBOL native data object {i}")
                };
                assert_eq!(
                    data.len(),
                    1,
                    "CANCEL-SYMBOL independent data field count {i}"
                );
                assert_eq!(data[0].0, "symbol", "CANCEL-SYMBOL native data field {i}");
                let J::String(id) = &data[0].1 else {
                    panic!("CANCEL-SYMBOL native data symbol {i}")
                };
                trace.borrow_mut().push(json!(["delete", endpoint, id]));
                if stage == "delete sync" {
                    return Err(error.clone());
                }
                Ok(std::future::ready(
                    if stage == "delete async" || stage == "logger" {
                        Err(error.clone())
                    } else {
                        Ok(())
                    },
                ))
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if stage == "logger" {
                    Err(replacement.clone())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            lookups.get(),
            1,
            "CANCEL-SYMBOL independent lookup count {i}"
        );
        let eager = [0, 1, 1, 2, 1, 1][i];
        assert_eq!(
            trace.borrow().len(),
            eager,
            "CANCEL-SYMBOL native eager count {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["atConstruction"].as_array().unwrap(),
            "CANCEL-SYMBOL native eager trace {i}"
        );
        let actual = ready(pending);
        assert_eq!(
            trace.borrow().len(),
            [0, 1, 1, 2, 2, 2][i],
            "CANCEL-SYMBOL native final count {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "CANCEL-SYMBOL native full trace {i}"
        );
        if i < 2 {
            actual.unwrap();
            assert_eq!(case["outcome"]["ok"], true);
        } else {
            let expected = if stage == "logger" {
                replacement
            } else {
                error
            };
            assert_eq!(
                actual.unwrap_err(),
                expected,
                "CANCEL-SYMBOL native original error {i}"
            );
            assert_eq!(case["outcome"]["name"], expected.name);
            assert_eq!(case["outcome"]["message"], expected.message);
            assert_eq!(case["outcome"]["sameReplacement"], stage == "logger");
        }
    }
    typed_utf16::assert_projection(
        corpus["cancelBoundary"]["symbolRaw"].as_str().unwrap(),
        &(js_from_str::<String>(corpus["cancelBoundary"]["symbolRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    assert_eq!(corpus["cancelBoundary"]["ok"], true);
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
