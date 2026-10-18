use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::{exchange_dispose::*, margin_mode::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("ready fixture did not settle"),
    }
}
fn oracle() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/margin-dispose.mjs"
        ))
        .output()
        .expect("Node margin/disposal oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["dispose"].as_array().unwrap().len(),
        24,
        "fixed disposal fixture count"
    );
    assert_eq!(
        data["margin"].as_array().unwrap().len(),
        44,
        "fixed margin fixture count"
    );
    data
}
fn error(name: &str, message: &str, code: &str, detail: &str, status: u16) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        code
    )));
    error = error.with_retained_response((json!({"status":status,"data":{"msg":detail}})).into());
    error
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Base,
    Public,
    Private,
}
#[derive(Debug, Default)]
struct DisposalState {
    visited: Vec<Operation>,
    disposed: [bool; 3],
}
fn disposal(variant: &str) {
    let source = oracle();
    let mut completed = 0;
    for case in source["dispose"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["variant"] == variant)
    {
        let failure = match case["failureAt"].as_str() {
            Some("base") => Some(Operation::Base),
            Some("public") => Some(Operation::Public),
            Some("private") => Some(Operation::Private),
            None => None,
            other => panic!("unexpected fixture {other:?}"),
        };
        let state = RefCell::new(DisposalState::default());
        let sentinel = Rc::new(error(
            "RangeError",
            "fixture disposer",
            "dispose-code",
            "dispose detail",
            503,
        ));
        let invoke = |operation| {
            let mut state = state.borrow_mut();
            state.visited.push(operation);
            state.disposed[match operation {
                Operation::Base => 0,
                Operation::Public => 1,
                Operation::Private => 2,
            }] = true;
            if Some(operation) == failure {
                Err(sentinel.clone())
            } else {
                Ok(())
            }
        };
        let result = match variant {
            "binance" => dispose_binance_exchange(
                || invoke(Operation::Base),
                || invoke(Operation::Public),
                || invoke(Operation::Private),
            ),
            "binancepm" => dispose_binance_pm_exchange(
                || invoke(Operation::Base),
                || invoke(Operation::Public),
                || invoke(Operation::Private),
            ),
            "binancespot" => dispose_binance_spot_exchange(
                || invoke(Operation::Base),
                || invoke(Operation::Public),
                || invoke(Operation::Private),
            ),
            _ => unreachable!(),
        };
        let state = state.into_inner();
        let expected = match failure {
            Some(Operation::Base) => vec![Operation::Base],
            Some(Operation::Public) => vec![Operation::Base, Operation::Public],
            _ => vec![Operation::Base, Operation::Public, Operation::Private],
        };
        // Assert native side effects, order and original error before source traces.
        assert_eq!(
            state.visited, expected,
            "DISPOSE {variant} case {completed} native order"
        );
        let flags = match failure {
            Some(Operation::Base) => [true, false, false],
            Some(Operation::Public) => [true, true, false],
            _ => [true, true, true],
        };
        assert_eq!(
            state.disposed, flags,
            "DISPOSE {variant} case {completed} native committed flags"
        );
        let count = match failure {
            Some(Operation::Base) => 1,
            Some(Operation::Public) => 2,
            _ => 3,
        };
        assert_eq!(
            state.visited.len(),
            count,
            "DISPOSE {variant} case {completed} native call count"
        );
        match result {
            Ok(()) => {
                assert!(failure.is_none());
                assert!(case["error"].is_null());
                assert_eq!(case["returnedUndefined"], true);
            }
            Err(actual) => {
                assert!(failure.is_some());
                assert!(
                    Rc::ptr_eq(&actual, &sentinel),
                    "DISPOSE {variant} native error identity"
                );
                assert_eq!(
                    *actual, *sentinel,
                    "DISPOSE {variant} native error metadata"
                );
                assert_eq!(actual.name, case["error"]["name"].as_str().unwrap());
                assert_eq!(actual.message, case["error"]["message"].as_str().unwrap());
                assert_eq!(
                    observed_error_code(&actual.code).as_ref(),
                    case["error"].get("code")
                );
                assert_eq!(
                    actual.response_json_view().as_ref(),
                    case["error"].get("response")
                );
                assert_eq!(case["error"]["same"], true);
            }
        }
        for (key, index) in [("base", 0), ("public", 1), ("private", 2)] {
            assert_eq!(
                state.disposed[index],
                case["disposed"][key].as_bool().unwrap(),
                "DISPOSE {variant} native source flag {key}"
            );
        }
        let source_visited: Vec<Operation> = case["visited"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| match v.as_str().unwrap() {
                "base" => Operation::Base,
                "public" => Operation::Public,
                "private" => Operation::Private,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            state.visited, source_visited,
            "DISPOSE {variant} source trace"
        );
        completed += 1;
    }
    assert_eq!(completed, 8, "fixed disposal variant count {variant}");
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn futures_disposal_matches_source() {
    disposal("binance");
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn pm_disposal_matches_source() {
    disposal("binancepm");
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn spot_disposal_matches_source() {
    disposal("binancespot");
}

fn selected_message(index: usize) -> Value {
    match ((index - 6) % 18) / 2 {
        0 | 2..=5 => json!("fixture post"),
        1 => json!("venue detail"),
        6 => json!([]),
        7 => json!({"id":"native"}),
        8 => json!({"toString":null}),
        _ => unreachable!(),
    }
}
fn fresh_error(index: usize) -> ExchangeError {
    match ((index - 6) % 18) / 2 {
        0 | 2..=5 => ExchangeError::new("fixture post"),
        1 => ExchangeError::new("venue detail"),
        6 => ExchangeError::new(""),
        7 => ExchangeError::new("[object Object]"),
        8 => {
            let mut error = ExchangeError::new("Cannot convert object to primitive value");
            error.name = "TypeError".into();
            error
        }
        _ => unreachable!(),
    }
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn futures_margin_type_matches_source() {
    let source = oracle();
    let mut completed = 0;
    for case in source["margin"].as_array().unwrap() {
        let index = case["index"].as_u64().unwrap() as usize;
        let isolated = case["isolated"].as_bool().unwrap();
        let symbol = case.get("symbol").and_then(Value::as_str);
        let missing = symbol.is_none() || symbol == Some("");
        let post_reject = case["postReject"].as_bool().unwrap();
        let emit_reject = case["emitReject"].as_bool().unwrap();
        let mut post_error = error("RangeError", "fixture post", "post-code", "unused", 503);
        post_error = post_error.with_retained_response((json!({"status":503,"data":case.get("msg").map(|msg|json!({"msg":msg})).unwrap_or(json!({}))})).into());
        let emitter_error = error(
            "RangeError",
            "fixture emitter",
            "emitter-code",
            "emitter detail",
            507,
        );
        let mut posts = vec![];
        let mut emissions = vec![];
        let visited = RefCell::new(vec![]);
        let result = run(change_binance_margin_type(
            isolated,
            symbol,
            |path, request| {
                visited.borrow_mut().push("post");
                posts.push((path, request));
                std::future::ready(if post_reject {
                    Err(post_error.clone())
                } else {
                    Ok(())
                })
            },
            |message| {
                visited.borrow_mut().push("emit");
                emissions.push(message);
                if emit_reject {
                    Err(emitter_error.clone())
                } else {
                    Ok(())
                }
            },
        ));
        // Native request enum, string and counts before any source trace/wire view.
        assert_eq!(
            posts.len(),
            if missing { 0 } else { 1 },
            "MARGIN case {index} native post count"
        );
        if !missing {
            assert_eq!(
                posts[0].0, "/fapi/v1/marginType",
                "MARGIN case {index} native endpoint"
            );
            assert_eq!(
                posts[0].1.symbol,
                symbol.unwrap(),
                "MARGIN case {index} native symbol"
            );
            assert_eq!(
                posts[0].1.margin_type,
                if isolated {
                    BinanceMarginType::Isolated
                } else {
                    BinanceMarginType::Crossed
                },
                "MARGIN case {index} native margin type"
            );
        }
        let expected_emissions = if missing {
            vec![json!("Symbol is required")]
        } else if post_reject {
            vec![selected_message(index)]
        } else {
            vec![]
        };
        assert_eq!(
            emissions, expected_emissions,
            "MARGIN case {index} native uncoerced emission"
        );
        let expected_error = if (missing || post_reject) && emit_reject {
            Some(emitter_error.clone())
        } else if missing {
            Some(ExchangeError::new("Symbol is required"))
        } else if post_reject {
            Some(fresh_error(index))
        } else {
            None
        };
        assert_eq!(
            result.as_ref().err(),
            expected_error.as_ref(),
            "MARGIN case {index} native result/error metadata"
        );
        if let Err(actual) = &result {
            assert_eq!(
                actual.name,
                case["error"]["name"].as_str().unwrap(),
                "MARGIN case {index} native source error name"
            );
            assert_eq!(
                actual.message,
                case["error"]["message"].as_str().unwrap(),
                "MARGIN case {index} native source error message"
            );
            assert_eq!(
                observed_error_code(&actual.code).as_ref(),
                case["error"].get("code").filter(|v| !v.is_null()),
                "MARGIN case {index} native source error code"
            );
            assert_eq!(
                actual.response_json_view().as_ref(),
                case["error"].get("response").filter(|v| !v.is_null()),
                "MARGIN case {index} native source response"
            );
            assert_eq!(
                actual == &emitter_error,
                case["error"]["sameEmitter"].as_bool().unwrap(),
                "MARGIN case {index} native emitter rejection preservation"
            );
            assert_eq!(
                actual == &post_error,
                case["error"]["samePost"].as_bool().unwrap(),
                "MARGIN case {index} native fresh Error versus original post error"
            );
        } else {
            assert!(case["error"].is_null());
            assert_eq!(case["returnedUndefined"], true);
        }
        let visited = visited.into_inner();
        let expected_visited = if missing {
            vec!["emit"]
        } else if post_reject {
            vec!["post", "emit"]
        } else {
            vec!["post"]
        };
        assert_eq!(
            visited, expected_visited,
            "MARGIN case {index} native operation order"
        );
        assert_eq!(
            case["posts"].as_array().unwrap().len(),
            if missing { 0 } else { 1 },
            "MARGIN fixed source post count"
        );
        if !missing {
            assert_eq!(posts[0].0, case["posts"][0]["path"].as_str().unwrap());
            assert_eq!(
                posts[0].1.symbol,
                case["posts"][0]["payload"]["symbol"].as_str().unwrap()
            );
            assert_eq!(
                case["posts"][0]["payload"]["marginType"],
                if isolated { "ISOLATED" } else { "CROSSED" }
            );
        }
        assert_eq!(
            emissions,
            serde_json::from_value::<Vec<Value>>(case["emissions"].clone()).unwrap(),
            "MARGIN source emissions"
        );
        assert_eq!(
            visited,
            serde_json::from_value::<Vec<String>>(case["visited"].clone()).unwrap(),
            "MARGIN source trace"
        );
        completed += 1;
    }
    assert_eq!(completed, 44, "fixed margin fixture count");
}

#[test]
fn margin_waits_for_post_before_emission() {
    for isolated in [false, true] {
        let calls = Cell::new(0);
        let settled = Cell::new(false);
        let emissions = RefCell::new(vec![]);
        let mut future = std::pin::pin!(change_binance_margin_type(
            isolated,
            Some("BTCUSDT"),
            |_, _| {
                calls.set(calls.get() + 1);
                std::future::poll_fn(|_| {
                    if settled.get() {
                        Poll::Ready(Err(error(
                            "RangeError",
                            "fixture post",
                            "post-code",
                            "venue detail",
                            503,
                        )))
                    } else {
                        Poll::Pending
                    }
                })
            },
            |message| {
                emissions.borrow_mut().push(message);
                Ok(())
            }
        ));
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(calls.get(), 0, "native margin lazy request");
        for _ in 0..2 {
            assert!(future.as_mut().poll(&mut context).is_pending());
            assert_eq!(calls.get(), 1);
            assert_eq!(
                emissions.borrow().len(),
                0,
                "native margin no early emission"
            );
        }
        settled.set(true);
        assert_eq!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(ExchangeError::new("venue detail"))),
            "native margin fresh post-settlement Error"
        );
        assert_eq!(
            *emissions.borrow(),
            [json!("venue detail")],
            "native margin emission after POST"
        );
        assert_eq!(calls.get(), 1, "native margin no retry");
    }
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
