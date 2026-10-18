use super::*;
use std::{
    cell::RefCell,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
struct Failure {
    name: String,
    message: String,
    response: Option<Value>,
}
fn error(name: &'static str, message: String) -> Rc<Failure> {
    Rc::new(Failure {
        name: name.into(),
        message,
        response: None,
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn position_pagination_reinitialization_and_error_identity_match_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-fetch-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, c) in cases.iter().enumerate() {
        let symbols: Option<Vec<String>> = c
            .get("symbols")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let reinit = Cell::new(c["initial"] == true);
        let requests = RefCell::new(vec![]);
        let events = RefCell::new(vec![]);
        let mapped = RefCell::new(vec![]);
        let original = RefCell::new(None);
        let index = Cell::new(0);
        let result = ready(fetch_positions_helper(
            symbols.as_deref(),
            Some("inverse"),
            Some("BTC"),
            &reinit,
            |url, params| {
                requests
                    .borrow_mut()
                    .push(json!({"url":url,"params":params}));
                let page = c["pages"][index.get()].clone();
                index.set(index.get() + 1);
                let response = if let Some(e) = page.get("requestError") {
                    let e = Rc::new(Failure {
                        name: "Error".into(),
                        message: e["message"].as_str().unwrap().into(),
                        response: e.get("response").cloned(),
                    });
                    *original.borrow_mut() = Some(e.clone());
                    Err(e)
                } else {
                    Ok(page)
                };
                std::future::ready(response)
            },
            |_| Ok("inverse".into()),
            |row| {
                mapped.borrow_mut().push(row.clone());
                if row.get("id") == c.get("mapFail") {
                    return Err(error("Error", "mapper refused".into()));
                }
                Ok(json!({"mapped":row}))
            },
            |msg| {
                let mut event = json!({"kind":"error"});
                if let Some(msg) = msg {
                    event["value"] = msg.clone();
                }
                events.borrow_mut().push(event);
                Ok(())
            },
            error,
        ));
        assert_eq!(json!(*requests.borrow()), c["requests"], "requests case{i}");
        assert_eq!(json!(*events.borrow()), c["events"], "events case{i}");
        assert_eq!(
            json!(*mapped.borrow()),
            c["mapped"],
            "mapping sequence case{i}"
        );
        assert_eq!(
            reinit.get(),
            c["needsReinit"].as_bool().unwrap(),
            "reinit case{i}"
        );
        match result {
            Ok(rows) => {
                assert!(c.get("error").is_none(), "source error case{i}");
                assert_eq!(json!(rows), c["result"], "result case{i}");
            }
            Err(e) => {
                assert_eq!(json!(e.name), c["error"]["name"], "error class case{i}");
                assert_eq!(
                    json!(e.message),
                    c["error"]["message"],
                    "error message case{i}"
                );
                assert_eq!(
                    e.response.as_ref(),
                    c["error"].get("response"),
                    "error response case{i}"
                );
                assert_eq!(
                    original
                        .borrow()
                        .as_ref()
                        .is_some_and(|original| Rc::ptr_eq(original, &e)),
                    c["error"]["sameRequestError"].as_bool().unwrap(),
                    "error identity case{i}"
                );
            }
        }
    }
    assert_eq!(cases.len(), 150);
}
#[test]
fn dropping_pending_position_read_does_not_mark_reinit_or_map() {
    let reinit = Cell::new(false);
    let calls = Cell::new(0);
    let mapped = Cell::new(0);
    let mut future = Box::pin(fetch_positions_helper(
        None,
        Some("inverse"),
        None,
        &reinit,
        |_, _| {
            calls.set(calls.get() + 1);
            std::future::pending::<Result<Value, Rc<Failure>>>()
        },
        |_| Ok("inverse".into()),
        |_| {
            mapped.set(mapped.get() + 1);
            Ok(())
        },
        |_| Ok(()),
        error,
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert_eq!(calls.get(), 1);
    assert_eq!(mapped.get(), 0);
    assert!(!reinit.get());
}

#[test]
fn dropping_between_position_pages_keeps_state_and_mapping_untouched() {
    let reinit = Cell::new(false);
    let calls = Cell::new(0);
    let mapped = Cell::new(0);
    let mut future = Box::pin(fetch_positions_helper(
        None,
        Some("inverse"),
        None,
        &reinit,
        |_, _| -> std::pin::Pin<Box<dyn Future<Output = Result<Value, Rc<Failure>>> + '_>> {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Box::pin(std::future::ready(Ok(
                    json!({"retMsg":"OK","result":{"list":[{"id":1}],"nextPageCursor":"next"}}),
                )))
            } else {
                Box::pin(std::future::pending())
            }
        },
        |_| Ok("inverse".into()),
        |_| {
            mapped.set(mapped.get() + 1);
            Ok(())
        },
        |_| Ok(()),
        error,
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert_eq!(calls.get(), 2);
    assert_eq!(mapped.get(), 0);
    assert!(!reinit.get());
}

#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn inverse_composition_defers_refusal_until_rows_and_requests_finish() {
    assert_inverse_composition(false);
}

#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn inverse_outer_fanout_preserves_later_errors_and_requests() {
    assert_inverse_composition(true);
}

fn assert_inverse_composition(fanout: bool) {
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_types::{MarginType, Market, Ticker};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-position-fixtures.mjs"
        ))
        .arg(if fanout { "--fanout" } else { "--composition" })
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), if fanout { 85 } else { 125 });
    for c in cases {
        let label = c["label"].as_str().unwrap();
        let store = DefaultStore::new();
        let market: Market = serde_json::from_value(json!({"id":"market","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":100000},"leverage":{"min":1,"max":100}},"isInverse":true})).unwrap();
        let ticker: Ticker = serde_json::from_value(json!({"id":"ticker","symbol":"BTCUSD","bid":0,"ask":0,"mark":20000,"last":19000,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market]),
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let symbols: Option<Vec<String>> = c
            .get("symbols")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let mut pages = c["pages"].as_array().unwrap().iter();
        let mut requests = Vec::new();
        let mut logs = Vec::new();
        let mut clocks = 0;
        let needs_reinit = Cell::new(false);
        let request_failure = error("Error", "later request rejected".into());
        let actions = RefCell::new(Vec::new());
        let events = RefCell::new(Vec::new());
        let get = |url, params: Value| {
            actions.borrow_mut().push(format!(
                "request:{}:{}",
                params["category"].as_str().unwrap(),
                params["settleCoin"].as_str().unwrap_or("")
            ));
            requests.push(json!({"url":url,"params":params}));
            let page = pages.next().expect("request inventory").clone();
            std::future::ready(if page.get("requestError").is_some() {
                Err(request_failure.clone())
            } else {
                Ok(page)
            })
        };
        let category_for_symbol = |_: &str| Ok("inverse".into());
        let emit = |value: Option<&Value>| {
            events
                .borrow_mut()
                .push(value.cloned().unwrap_or(Value::Null));
            if c["plan"]["reportError"] == true {
                Err(error("Error", "reporter rejected".into()))
            } else {
                Ok(())
            }
        };
        let now = || {
            clocks += 1;
            123456789.
        };
        let log = |message| logs.push(message);
        let result = if !fanout {
            ready(fetch_inverse_positions(
                symbols.as_deref(),
                Some("inverse"),
                Some("BTC"),
                &needs_reinit,
                get,
                category_for_symbol,
                emit,
                error,
                &store,
                Some(MarginType::Cross),
                now,
                log,
            ))
        } else if c["plan"].as_object().unwrap().is_empty() {
            ready(fetch_all_inverse_positions(
                symbols.as_deref(),
                &needs_reinit,
                get,
                category_for_symbol,
                emit,
                error,
                &store,
                Some(MarginType::Cross),
                now,
                log,
            ))
        } else {
            ready(fetch_inverse_position_fanout(
                symbols.as_deref(),
                || {
                    actions.borrow_mut().push("categories".into());
                    Ok(serde_json::from_value(c["plan"]["categories"].clone()).unwrap())
                },
                |category| {
                    actions.borrow_mut().push(format!("settle:{category}"));
                    if c["plan"]["failCategory"] == category {
                        return Err(error("Error", "settlement getter rejected".into()));
                    }
                    Ok(if c["plan"]["emptyCoins"] == true {
                        vec![]
                    } else if category == "linear" {
                        vec![Some("USDT".into()), Some("USDC".into())]
                    } else {
                        vec![None]
                    })
                },
                &needs_reinit,
                get,
                category_for_symbol,
                emit,
                error,
                &store,
                Some(MarginType::Cross),
                now,
                log,
            ))
        };
        if fanout {
            let expected_actions: Vec<Value> = c["actions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|value| {
                    !c["plan"].as_object().unwrap().is_empty()
                        || value.as_str().unwrap().starts_with("request:")
                })
                .cloned()
                .collect();
            assert_eq!(
                json!(*actions.borrow()),
                json!(expected_actions),
                "fanout lazy getter/request order: {label}"
            );
            assert_eq!(
                json!(*events.borrow()),
                c["events"],
                "fanout reporting: {label}"
            );
        }
        assert_eq!(
            json!(requests),
            c["requests"],
            "all source requests before refusal: {label}"
        );
        assert_eq!(
            json!(logs),
            c["logs"],
            "all source logs before refusal: {label}"
        );
        assert_eq!(
            json!(clocks),
            c["clockCalls"],
            "all source rows before refusal: {label}"
        );
        assert_eq!(
            json!(needs_reinit.get()),
            c["needsReinit"],
            "reinit: {label}"
        );
        if let Some(expected) = c.get("error") {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.name, expected["name"],
                "source error identity: {label}"
            );
            assert_eq!(
                failure.message, expected["message"],
                "later source error wins: {label}"
            );
            if label.ends_with("request-error") {
                assert!(Rc::ptr_eq(&failure, &request_failure));
            }
        } else if label.starts_with("ordinary") {
            let positions = result.unwrap();
            assert_eq!(json!(positions.len()), c["resultCount"]);
            assert_eq!(
                json!(positions.iter().map(|p| &p.id).collect::<Vec<_>>()),
                c["ids"]
            );
        } else if label.starts_with("symbol-") {
            let failure = result.unwrap_err();
            assert_eq!(
                failure.name, "RepresentationError",
                "symbol refusal after all work: {label}"
            );
            assert_eq!(
                failure.message,
                "Bybit typed Position requires string symbol"
            );
        } else {
            // Capture the inherited value before JSON would omit the function.
            let prototype = label.starts_with("__proto__:");
            assert_eq!(
                c["mapped"][0]["kind"],
                if prototype { "object" } else { "function" }
            );
            assert_eq!(c["mapped"][0]["truthy"], true);
            let failure = result.unwrap_err();
            assert_eq!(
                failure.name, "RepresentationError",
                "deferred output refusal: {label}"
            );
            assert_eq!(
                failure.message,
                format!(
                    "Bybit typed Position cannot represent inherited side key {}",
                    label.split(':').next().unwrap()
                )
            );
        }
    }
}

#[test]
fn inverse_fanout_drop_releases_request_without_later_category() {
    struct RequestDrop(Rc<Cell<usize>>);
    impl Drop for RequestDrop {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    for poll in [false, true] {
        let calls = Cell::new(0);
        let drops = Rc::new(Cell::new(0));
        let categories = RefCell::new(Vec::new());
        let reinit = Cell::new(false);
        let store = safe_cex_exchange::native_store::DefaultStore::new();
        let mut future = Box::pin(fetch_inverse_position_fanout(
            None,
            || Ok(vec!["linear".into(), "inverse".into()]),
            |category| {
                categories.borrow_mut().push(category.to_owned());
                Ok(vec![Some("USDT".into()), Some("USDC".into())])
            },
            &reinit,
            |_, _| {
                let index = calls.get();
                calls.set(index + 1);
                let guard = RequestDrop(drops.clone());
                async move {
                    let _guard = guard;
                    if index == 1 {
                        std::future::pending::<()>().await;
                    }
                    Ok::<_, Rc<Failure>>(json!({"retMsg":"OK","result":{"list":[]}}))
                }
            },
            |_| Ok("inverse".into()),
            |_| Ok(()),
            error,
            &store,
            None,
            || 0.,
            |_| panic!("empty pages must not map"),
        ));
        if poll {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(
                drops.get(),
                1,
                "first request settled before pending second"
            );
        }
        drop(future);
        assert_eq!(
            calls.get(),
            if poll { 2 } else { 0 },
            "drop never starts next request"
        );
        assert_eq!(drops.get(), calls.get(), "drop releases pending request");
        assert_eq!(
            *categories.borrow(),
            if poll { vec!["linear"] } else { vec![] },
            "drop never resolves later category"
        );
        assert!(!reinit.get(), "drop is not a request rejection");
    }
}
