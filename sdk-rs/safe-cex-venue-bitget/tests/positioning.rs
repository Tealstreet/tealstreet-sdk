use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{MarginType, PositionMode};
use safe_cex_venue_bitget::positioning::{get_or_fetch_margin_type, get_or_fetch_position_mode};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("POSITIONING fixture must settle"),
    }
}
fn failure(name: &str, message: &str) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error.context = Some(json!({"original":name}));
    error
}
#[derive(Debug, PartialEq)]
enum Selected {
    Mode(PositionMode),
    Margin(MarginType),
}

#[test]
#[ignore = "requires Node and TypeScript; venue oracle includes ignored tests"]
fn live_positioning_consumers() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/positioning-capture.cjs"
        ))
        .output()
        .expect("POSITIONING Node required");
    assert!(
        output.status.success(),
        "POSITIONING source capture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value =
        serde_json::from_slice(&output.stdout).expect("POSITIONING complete capture");
    let cases = capture
        .get("cases")
        .and_then(Value::as_array)
        .expect("POSITIONING cases required");
    assert_eq!(cases.len(), 128, "POSITIONING source total");
    let mut index = 0;
    let mut branches = [0; 4];
    for method in ["getOrFetchPositionMode", "getOrFetchMarginType"] {
        for symbol in ["", "BTCUSDT", "Σ😀", "\u{feff}"] {
            for uta in [false, true] {
                for mode in ["first", "second", "missing", "reject"] {
                    for report_fails in [false, true] {
                        let case = cases.get(index).expect("POSITIONING required native input");
                        assert_eq!(
                            case.get("input"),
                            Some(
                                &json!({"method":method,"symbol":symbol,"uta":uta,"mode":mode,"reportFails":report_fails,"sibling":"throw"})
                            ),
                            "POSITIONING input content {index}"
                        );
                        let calls = RefCell::new(Vec::new());
                        let resolver_error = failure("ResolverFailure", "resolver failure");
                        let resolver_id = std::ptr::from_ref(resolver_error.metadata.as_ref());
                        let reporter_error = failure("ReporterFailure", "reporter failure");
                        let reporter_id = std::ptr::from_ref(reporter_error.metadata.as_ref());
                        let uses_uta = || {
                            calls.borrow_mut().push(json!(["uta"]));
                            uta
                        };
                        let report = |message: &str| {
                            calls.borrow_mut().push(json!(["report", message]));
                            if report_fails {
                                Err(reporter_error)
                            } else {
                                Ok(())
                            }
                        };
                        let result = if method == "getOrFetchPositionMode" {
                            ready(get_or_fetch_position_mode(
                                symbol,
                                |seen| {
                                    calls.borrow_mut().push(json!(["resolve", seen]));
                                    if mode == "reject" {
                                        std::future::ready(Err(resolver_error))
                                    } else {
                                        calls.borrow_mut().push(json!(["field", "positionMode"]));
                                        std::future::ready(Ok(match mode {
                                            "first" => Some(PositionMode::Hedged),
                                            "second" => Some(PositionMode::OneWay),
                                            _ => None,
                                        }))
                                    }
                                },
                                uses_uta,
                                report,
                            ))
                            .map(Selected::Mode)
                        } else {
                            ready(get_or_fetch_margin_type(
                                symbol,
                                |seen| {
                                    calls.borrow_mut().push(json!(["resolve", seen]));
                                    if mode == "reject" {
                                        std::future::ready(Err(resolver_error))
                                    } else {
                                        calls.borrow_mut().push(json!(["field", "marginType"]));
                                        std::future::ready(Ok(match mode {
                                            "first" => Some(MarginType::Isolated),
                                            "second" => Some(MarginType::Cross),
                                            _ => None,
                                        }))
                                    }
                                },
                                uses_uta,
                                report,
                            ))
                            .map(Selected::Margin)
                        };
                        let source = case.get("output").expect("POSITIONING output required");
                        match mode {
                            "first" | "second" => {
                                let (native, wire) = match (method, mode) {
                                    ("getOrFetchPositionMode", "first") => {
                                        (Selected::Mode(PositionMode::Hedged), "hedged")
                                    }
                                    ("getOrFetchPositionMode", _) => {
                                        (Selected::Mode(PositionMode::OneWay), "oneway")
                                    }
                                    (_, "first") => {
                                        (Selected::Margin(MarginType::Isolated), "isolated")
                                    }
                                    _ => (Selected::Margin(MarginType::Cross), "cross"),
                                };
                                assert_eq!(
                                    result.unwrap(),
                                    native,
                                    "POSITIONING native enum {index}"
                                );
                                assert_eq!(
                                    calls.borrow().len(),
                                    2,
                                    "POSITIONING success effects {index}"
                                );
                                assert_eq!(
                                    source,
                                    &json!({"value":wire}),
                                    "POSITIONING source enum {index}"
                                );
                                branches[0] += 1;
                            }
                            _ => {
                                let error = result.unwrap_err();
                                let (name, message, identity, branch) = if mode == "reject" {
                                    (
                                        "ResolverFailure",
                                        "resolver failure".to_owned(),
                                        Some(resolver_id),
                                        1,
                                    )
                                } else if report_fails {
                                    (
                                        "ReporterFailure",
                                        "reporter failure".to_owned(),
                                        Some(reporter_id),
                                        2,
                                    )
                                } else {
                                    let axis = if method == "getOrFetchPositionMode" {
                                        "position mode"
                                    } else {
                                        "margin type"
                                    };
                                    let prefix = if uta { "Bitget UTA " } else { "" };
                                    (
                                        "Error",
                                        format!(
                                            "Could not determine {prefix}{axis} for symbol {symbol}"
                                        ),
                                        None,
                                        3,
                                    )
                                };
                                assert_eq!(
                                    error.name, name,
                                    "POSITIONING native error name {index}"
                                );
                                assert_eq!(
                                    error.message, message,
                                    "POSITIONING native diagnostic {index}"
                                );
                                if let Some(identity) = identity {
                                    assert_eq!(
                                        std::ptr::from_ref(error.metadata.as_ref()),
                                        identity,
                                        "POSITIONING original failure identity {index}"
                                    );
                                    assert_eq!(
                                        error.context,
                                        Some(json!({"original":name})),
                                        "POSITIONING original metadata {index}"
                                    );
                                }
                                assert_eq!(
                                    calls.borrow().len(),
                                    if mode == "reject" { 1 } else { 4 },
                                    "POSITIONING failure effects {index}"
                                );
                                assert_eq!(
                                    source,
                                    &json!({"error":{"name":name,"message":message,"sameResolver":mode=="reject","sameReporter":mode=="missing"&&report_fails}}),
                                    "POSITIONING source error {index}"
                                );
                                branches[branch] += 1;
                            }
                        }
                        assert_eq!(
                            case.get("calls"),
                            Some(&Value::Array(calls.into_inner())),
                            "POSITIONING effect order {index}"
                        );
                        index += 1;
                    }
                }
            }
        }
    }
    assert_eq!(
        branches,
        [64, 32, 16, 16],
        "POSITIONING native branch comparisons"
    );
    assert_eq!(index, 128, "POSITIONING comparison count");
    assert_eq!(cases.len(), index, "POSITIONING source consumed count");
    let boundaries = capture
        .get("boundaries")
        .and_then(Value::as_array)
        .expect("POSITIONING boundaries required");
    assert_eq!(boundaries.len(), 8, "POSITIONING boundary total");
    for (method_index, method) in ["getOrFetchPositionMode", "getOrFetchMarginType"]
        .iter()
        .enumerate()
    {
        for (mode_index, mode) in ["open", "null-object", "field-throw", "surrogate"]
            .iter()
            .enumerate()
        {
            let case = &boundaries[method_index * 4 + mode_index];
            assert_eq!(
                case.get("method").and_then(Value::as_str),
                Some(*method),
                "POSITIONING boundary method"
            );
            assert_eq!(
                case.get("mode").and_then(Value::as_str),
                Some(*mode),
                "POSITIONING boundary input"
            );
            let output = case.get("output").expect("POSITIONING boundary output");
            match *mode {
                "open" => {
                    assert_eq!(output, &json!({"value":"OPEN"}));
                    assert!(serde_json::from_str::<PositionMode>("\"OPEN\"").is_err());
                    assert!(serde_json::from_str::<MarginType>("\"OPEN\"").is_err());
                }
                "surrogate" => {
                    let encoded = case.get("symbol").and_then(Value::as_str).unwrap();
                    assert!(
                        serde_json::from_str::<String>(encoded).is_err(),
                        "POSITIONING UTF8 boundary"
                    );
                    assert_eq!(output["error"]["name"], "Error");
                    assert!(
                        output["error"]["message"]
                            .as_str()
                            .unwrap()
                            .contains("\\ud800")
                    );
                }
                _ => assert_eq!(
                    output["error"]["name"], "TypeError",
                    "POSITIONING dynamic resolver boundary"
                ),
            }
        }
    }
}

#[test]
fn positioning_awaits_resolution_and_owns_cancellation() {
    struct DropProbe(Rc<Cell<usize>>);
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    for margin in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let calls = Cell::new(0);
        let probe = DropProbe(drops.clone());
        if margin {
            let future = get_or_fetch_margin_type(
                "BTC",
                |_| async move {
                    let _probe = probe;
                    std::future::pending::<()>().await;
                    Ok(None)
                },
                || {
                    calls.set(calls.get() + 1);
                    false
                },
                |_| {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            );
            let mut future = Box::pin(future);
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            drop(future);
        } else {
            let future = get_or_fetch_position_mode(
                "BTC",
                |_| async move {
                    let _probe = probe;
                    std::future::pending::<()>().await;
                    Ok(None)
                },
                || {
                    calls.set(calls.get() + 1);
                    false
                },
                |_| {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            );
            let mut future = Box::pin(future);
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            drop(future);
        }
        assert_eq!(calls.get(), 0, "POSITIONING no effects before resolution");
        assert_eq!(drops.get(), 1, "POSITIONING owned resolver cancellation");
    }
}
