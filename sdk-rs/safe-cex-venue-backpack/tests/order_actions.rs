use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue as J;
use safe_cex_venue_backpack::{
    account_reads::cancel_order,
    order_utils::{BatchRequests, can_use_native_batch, place_batch_pair},
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
            "/tests/order_actions_oracle.cjs"
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
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture must complete"),
    }
}
fn bits(n: f64) -> String {
    format!("{:016x}", n.to_bits())
}
#[test]
fn actual_typescript_batch_eligibility() {
    let c = oracle();
    let rows = c["eligibility"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "ELIGIBILITY independent cases");
    let labels = [
        "ordinary false",
        "ordinary throws",
        "absent ID",
        "null ID",
        "zero ID",
        "empty ID",
        "string ID",
        "ID getter throws",
        "builder throws",
    ];
    let failures = [
        "none", "ordinary", "none", "none", "none", "none", "none", "id", "build",
    ];
    let ids = [
        json!(["undefined"]),
        json!(["undefined"]),
        json!(["undefined"]),
        json!(["null"]),
        json!(["number", "0000000000000000"]),
        json!(["string", ""]),
        json!(["string", "mine"]),
        json!(["undefined"]),
        json!(["undefined"]),
    ];
    let calls = [1, 1, 3, 2, 2, 2, 2, 2, 3];
    let values = [false, false, true, false, false, false, false, false, false];
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["label"], labels[i]);
        assert_eq!(row["ordinary"], i != 0);
        assert_eq!(
            row["clientOrderId"], ids[i],
            "ELIGIBILITY independent ID content {i}"
        );
        assert_eq!(row["failure"], failures[i]);
        let opts = Rc::new("opts".to_string());
        let original = Rc::new("opaque eligible error".to_string());
        let trace = RefCell::new(Vec::new());
        let result = can_use_native_batch(
            &opts,
            |arg| {
                assert!(Rc::ptr_eq(arg, &opts));
                trace.borrow_mut().push(json!(["ordinary"]));
                if i == 1 {
                    Err(original.clone())
                } else {
                    Ok(i != 0)
                }
            },
            |arg| {
                assert!(Rc::ptr_eq(arg, &opts));
                trace.borrow_mut().push(json!(["id"]));
                if i == 7 {
                    Err(original.clone())
                } else {
                    Ok((3..=6).contains(&i))
                }
            },
            |arg, id| {
                assert!(Rc::ptr_eq(arg, &opts));
                assert_eq!(id, 1.0, "ELIGIBILITY native preflight clientId {i}");
                trace.borrow_mut().push(json!(["build", bits(id)]));
                if i == 8 {
                    Err(original.clone())
                } else {
                    Ok(Rc::new("ignored"))
                }
            },
        );
        if [1, 7].contains(&i) {
            assert!(
                Rc::ptr_eq(&result.err().unwrap(), &original),
                "ELIGIBILITY native original error identity {i}"
            );
            assert_eq!(row["outcome"]["ok"], false);
        } else {
            assert_eq!(
                result.unwrap(),
                values[i],
                "ELIGIBILITY native result case {i}"
            );
            assert_eq!(row["outcome"]["ok"], true);
            assert_eq!(row["outcome"]["value"], values[i]);
        }
        assert_eq!(
            trace.borrow().len(),
            calls[i],
            "ELIGIBILITY independent native callback count {i}"
        );
        assert_eq!(trace.borrow().as_slice(), row["trace"].as_array().unwrap());
    }
}
struct Request {
    symbol: String,
    client_id: Option<f64>,
    tag: String,
}
fn requests_probe(requests: &BatchRequests<Request>) -> Vec<Value> {
    requests
        .borrow()
        .iter()
        .map(|r| json!([r.symbol, r.client_id.map(bits), r.tag]))
        .collect()
}
const PAIR_STAGES: [&str; 9] = [
    "ok",
    "ids",
    "build0",
    "build1",
    "post sync",
    "post async",
    "decode",
    "short ids",
    "mutate requests",
];
#[test]
fn actual_typescript_place_batch_pair() {
    let c = oracle();
    let rows = c["pairs"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "PAIR independent cases");
    let totals = [5, 1, 2, 3, 4, 4, 5, 5, 5];
    let eager = [4, 1, 2, 3, 4, 4, 4, 4, 4];
    let result_token = Rc::new(vec![
        "id λ".encode_utf16().collect::<Vec<_>>(),
        vec![105, 100, 32, 0xd800],
    ]);
    for (i, row) in rows.iter().enumerate() {
        let stage = PAIR_STAGES[i];
        assert_eq!(row["stage"], stage, "PAIR independent stage identity {i}");
        let orders = [Rc::new("first".to_string()), Rc::new("second".to_string())];
        let trace = RefCell::new(Vec::new());
        let posted = RefCell::new(None::<BatchRequests<Request>>);
        let failure = Rc::new("opaque pair failure".to_string());
        let data = Rc::new("response".to_string());
        let index = Cell::new(0usize);
        let action = place_batch_pair(
            &orders,
            |count| {
                assert_eq!(count, 2, "PAIR native ID count {i}");
                trace.borrow_mut().push(json!(["ids", count]));
                if stage == "ids" {
                    Err(failure.clone())
                } else {
                    Ok(if stage == "short ids" {
                        vec![17.0]
                    } else {
                        vec![17.0, 29.0]
                    })
                }
            },
            |order, id| {
                let n = index.replace(index.get() + 1);
                assert!(
                    Rc::ptr_eq(order, &orders[n]),
                    "PAIR native item identity {i}/{n}"
                );
                trace.borrow_mut().push(json!(["build", n, id.map(bits)]));
                if stage == ["build0", "build1"][n] {
                    Err(failure.clone())
                } else {
                    Ok(Request {
                        symbol: format!("REQ{n}"),
                        client_id: id,
                        tag: (**order).clone(),
                    })
                }
            },
            |endpoint, requests| {
                assert_eq!(endpoint, "/api/v1/orders", "PAIR native endpoint {i}");
                assert_eq!(requests.borrow().len(), 2, "PAIR native preflight rows {i}");
                for (n, request) in requests.borrow().iter().enumerate() {
                    assert_eq!(request.symbol, format!("REQ{n}"));
                    assert_eq!(request.tag, ["first", "second"][n]);
                    assert_eq!(
                        request.client_id,
                        if stage == "short ids" && n == 1 {
                            None
                        } else {
                            Some([17.0, 29.0][n])
                        },
                        "PAIR native clientId row{n} case{i}"
                    );
                }
                trace
                    .borrow_mut()
                    .push(json!(["post", endpoint, requests_probe(&requests)]));
                *posted.borrow_mut() = Some(requests.clone());
                if stage == "post sync" {
                    return Err(failure.clone());
                }
                if stage == "mutate requests" {
                    requests.borrow_mut().push(Request {
                        symbol: "ADDED".into(),
                        client_id: Some(31.0),
                        tag: "third".into(),
                    });
                }
                Ok(std::future::ready(if stage == "post async" {
                    Err(failure.clone())
                } else {
                    Ok(data.clone())
                }))
            },
            |response, requests| {
                assert!(
                    Rc::ptr_eq(&response, &data),
                    "PAIR native decoded data identity {i}"
                );
                assert!(
                    Rc::ptr_eq(&requests, posted.borrow().as_ref().unwrap()),
                    "PAIR native request array identity {i}"
                );
                assert_eq!(
                    requests.borrow().len(),
                    if stage == "mutate requests" { 3 } else { 2 },
                    "PAIR native live request length {i}"
                );
                trace
                    .borrow_mut()
                    .push(json!(["decode", requests_probe(&requests)]));
                if stage == "decode" {
                    Err(failure.clone())
                } else {
                    Ok(result_token.clone())
                }
            },
        );
        assert_eq!(
            trace.borrow().len(),
            eager[i],
            "PAIR independent eager count {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            row["atConstruction"].as_array().unwrap()
        );
        let result = ready(action);
        if (1..=6).contains(&i) {
            assert!(
                Rc::ptr_eq(&result.err().unwrap(), &failure),
                "PAIR native original error identity {i}"
            );
            assert_eq!(row["outcome"]["ok"], false);
            assert_eq!(row["outcome"]["sameFailure"], true);
        } else {
            let native = result.unwrap();
            assert!(
                Rc::ptr_eq(&native, &result_token),
                "PAIR native result identity {i}"
            );
            assert_eq!(
                *native,
                vec![
                    "id λ".encode_utf16().collect::<Vec<_>>(),
                    vec![105, 100, 32, 0xd800]
                ],
                "PAIR native UTF16 result content {i}"
            );
            assert_eq!(row["outcome"]["result"], json!(*native));
            assert_eq!(row["outcome"]["sameResult"], true);
            assert_eq!(row["outcome"]["ok"], true);
        }
        assert_eq!(
            trace.borrow().len(),
            totals[i],
            "PAIR independent callback count {i}"
        );
        assert_eq!(trace.borrow().as_slice(), row["trace"].as_array().unwrap());
    }
}
fn error(name: &str, message: &str) -> ExchangeError {
    let mut e = ExchangeError::new(message);
    e.name = name.into();
    e.store_subscriber_ids = vec![4, 9];
    e
}
#[test]
fn actual_typescript_cancel_order() {
    let c = oracle();
    let rows = c["cancels"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "CANCEL independent cases");
    let stages = [
        "ok",
        "missing market",
        "market throws",
        "delete sync",
        "delete async",
    ];
    for (i, row) in rows.iter().enumerate() {
        let stage = stages[i / 2];
        let log_throws = i % 2 == 1;
        assert_eq!(row["stage"], stage);
        assert_eq!(row["logThrows"], log_throws);
        assert_eq!(row["order"], json!({"symbol":"BTC λ","id":"order 29"}));
        let trace = RefCell::new(Vec::new());
        let original = error("ActionError", "action failed");
        let logger = error("LoggerError", "logger failed");
        let action = cancel_order(
            "BTC λ",
            "order 29",
            |symbol| {
                assert_eq!(symbol, "BTC λ");
                trace.borrow_mut().push(json!(["market", symbol]));
                if stage == "market throws" {
                    Err(original.clone())
                } else {
                    Ok(if stage == "missing market" {
                        None
                    } else {
                        Some("VENUE 17".into())
                    })
                }
            },
            |endpoint, body| {
                assert_eq!(endpoint, "/api/v1/order", "CANCEL native endpoint {i}");
                let J::Object(root) = body else {
                    panic!("CANCEL native config object")
                };
                assert_eq!(root.len(), 1);
                assert_eq!(root[0].0, "data");
                let J::Object(fields) = &root[0].1 else {
                    panic!("CANCEL native data object")
                };
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "symbol");
                assert_eq!(
                    fields[0].1,
                    J::String("VENUE 17".into()),
                    "CANCEL native symbol value {i}"
                );
                assert_eq!(fields[1].0, "orderId", "CANCEL native orderId field {i}");
                assert_eq!(
                    fields[1].1,
                    J::String("order 29".into()),
                    "CANCEL native orderId value {i}"
                );
                trace.borrow_mut().push(
                    json!(["delete",endpoint,{"data":{"symbol":"VENUE 17","orderId":"order 29"}}]),
                );
                if stage == "delete sync" {
                    Err(original.clone())
                } else {
                    Ok(std::future::ready(if stage == "delete async" {
                        Err(original.clone())
                    } else {
                        Ok(())
                    }))
                }
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if log_throws {
                    Err(logger.clone())
                } else {
                    Ok(())
                }
            },
        );
        let eager = [2, 2, 2, 3, 2][i / 2];
        assert_eq!(
            trace.borrow().len(),
            eager,
            "CANCEL independent eager effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            row["atConstruction"].as_array().unwrap()
        );
        let result = ready(action);
        if stage == "ok" {
            result.unwrap();
            assert_eq!(row["outcome"]["ok"], true);
        } else {
            let actual = result.unwrap_err();
            let expected = if log_throws {
                logger
            } else if stage == "missing market" {
                ExchangeError::new("Market not found: BTC λ")
            } else {
                original
            };
            assert_eq!(
                actual, expected,
                "CANCEL native original/replacement error {i}"
            );
            assert_eq!(actual.name, row["outcome"]["name"]);
            assert_eq!(actual.message, row["outcome"]["message"]);
            assert_eq!(row["outcome"]["ok"], false);
        }
        assert_eq!(
            trace.borrow().len(),
            [2, 2, 2, 3, 3][i / 2],
            "CANCEL independent final effects {i}"
        );
        assert_eq!(trace.borrow().as_slice(), row["trace"].as_array().unwrap());
    }
}
#[test]
fn actual_typescript_pair_awaits_response() {
    let c = oracle();
    let trace = RefCell::new(Vec::new());
    let resolved = Cell::new(false);
    let decoded = Cell::new(false);
    let orders = ["first", "second"];
    let mut action = place_batch_pair(
        &orders,
        |count| {
            trace.borrow_mut().push(json!(["ids", count]));
            Ok::<_, ()>(vec![17.0, 29.0])
        },
        |tag, id| {
            assert_eq!(id, Some(if *tag == "first" { 17.0 } else { 29.0 }));
            trace
                .borrow_mut()
                .push(json!(["build", tag, bits(id.unwrap())]));
            Ok((*tag, id.unwrap()))
        },
        |endpoint, _requests| {
            trace.borrow_mut().push(json!(["post", endpoint]));
            Ok(std::future::poll_fn(|_| {
                if resolved.get() {
                    Poll::Ready(Ok("resolved"))
                } else {
                    Poll::Pending
                }
            }))
        },
        |data, _requests| {
            assert_eq!(data, "resolved");
            decoded.set(true);
            trace.borrow_mut().push(json!(["decode"]));
            Ok(vec!["done"])
        },
    );
    assert_eq!(trace.borrow().len(), 4, "PAIR-WAIT independent eager calls");
    assert_eq!(
        trace.borrow().as_slice(),
        c["waits"]["before"].as_array().unwrap()
    );
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        matches!(action.as_mut().poll(&mut context), Poll::Pending),
        "PAIR-WAIT native pending response"
    );
    assert!(!decoded.get());
    assert_eq!(trace.borrow().len(), 4);
    assert_eq!(
        trace.borrow().as_slice(),
        c["waits"]["whilePending"].as_array().unwrap()
    );
    resolved.set(true);
    let Poll::Ready(result) = action.as_mut().poll(&mut context) else {
        panic!("PAIR-WAIT native resolved response")
    };
    let native = result.unwrap();
    assert_eq!(native, vec!["done"]);
    assert!(decoded.get());
    assert_eq!(trace.borrow().len(), 5);
    assert_eq!(
        trace.borrow().as_slice(),
        c["waits"]["after"].as_array().unwrap()
    );
    assert_eq!(c["waits"]["result"], json!(native));
}
