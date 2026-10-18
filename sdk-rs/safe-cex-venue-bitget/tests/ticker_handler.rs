use safe_cex_num::js_number_string;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{StoreSubscriberError, Ticker};
use safe_cex_venue_bitget::ticker::{TickerHandlerError, handle_ticker_topic};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
fn decode(value: &Value) -> Option<Value> {
    if value.get("$undefined").is_some() {
        return None;
    }
    if let Some(text) = value.get("$number").and_then(Value::as_str) {
        return Some(
            serde_json::from_str(match text {
                "Infinity" => "1e400",
                "-Infinity" => "-1e400",
                text => text,
            })
            .unwrap(),
        );
    }
    Some(match value {
        Value::Array(rows) => Value::Array(
            rows.iter()
                .map(|v| decode(v).unwrap_or(Value::Null))
                .collect(),
        ),
        Value::Object(rows) => Value::Object(
            rows.iter()
                .filter_map(|(k, v)| decode(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
        value => value.clone(),
    })
}
fn snapshot(rows: &[Ticker]) -> Value {
    Value::Array(rows.iter().map(|t| {
        let mut value=json!({"id":t.id,"symbol":t.symbol});
        for (key,n) in [("bid",t.bid),("ask",t.ask),("last",t.last),("mark",t.mark),("index",t.index.unwrap()),("percentage",t.percentage),("openInterest",t.open_interest),("fundingRate",t.funding_rate),("fundingTime",t.funding_time),("volume",t.volume),("quoteVolume",t.quote_volume)] {
            value[key]=json!({"$number":if n==0.0&&n.is_sign_negative(){"-0".into()}else{js_number_string(n)},"bits":format!("{:016x}",n.to_bits())});
        }
        value
    }).collect())
}
fn check(oracle: &Value) {
    for source in oracle.as_array().unwrap() {
        for case in source["cases"].as_array().unwrap() {
            if case["mode"] == "input-domain-finding" {
                assert!(
                    serde_json::Number::from_f64(f64::NAN).is_none(),
                    "ticker handler JSON domain excludes native NaN"
                );
                assert_eq!(
                    case["after"], case["seed"],
                    "source NaN blocks later bid alias"
                );
                assert!(
                    !case["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["phase"] == "first"),
                    "source empty patch does not notify"
                );
                continue;
            }
            let store = DefaultStore::new();
            let seed: Vec<Ticker> = serde_json::from_value(decode(&case["seed"]).unwrap()).unwrap();
            Store::update(
                &store,
                StoreUpdate {
                    tickers: Some(seed),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            let notifications = Rc::new(RefCell::new(Vec::new()));
            let first = notifications.clone();
            let throws = case["mode"] == "throw";
            let cause = StoreSubscriberError::with_response(
                "subscriber witness",
                json!({"source":"bitget subscriber","code":17}),
            )
            .with_cause(Rc::new(std::io::Error::other("original subscriber cause")));
            let first_cause = cause.clone();
            let _first = Store::subscribe(
                &store,
                Box::new(move |data, _| {
                    first
                        .borrow_mut()
                        .push(json!({"phase":"first","state":snapshot(&data.tickers)}));
                    if throws {
                        Err(first_cause.clone())
                    } else {
                        Ok(())
                    }
                }),
            );
            let second = notifications.clone();
            let _second = Store::subscribe(
                &store,
                Box::new(move |_, _| {
                    second.borrow_mut().push(json!({"phase":"second"}));
                    Ok(())
                }),
            );
            let input = decode(&case["input"]);
            let result = handle_ticker_topic(&store, input.as_ref());
            if throws {
                let Err(TickerHandlerError::Store(
                    safe_cex_store::StoreMutationError::Notification(error),
                )) = &result
                else {
                    panic!(
                        "ticker notification identity: expected original Notification, got {result:?}"
                    );
                };
                assert_eq!(
                    error.subscriber_id(),
                    0,
                    "ticker first subscriber identity within this Store"
                );
                assert_eq!(
                    error.cause(),
                    &cause,
                    "ticker notification original cause identity and response"
                );
            }
            if case["mode"] == "prototype-write" {
                let rows = Store::tickers(&store);
                assert_eq!(rows.len(), 2, "TICKER native prototype witness row floor");
                assert_eq!(
                    rows[0].last.to_bits(),
                    0.0_f64.to_bits(),
                    "TICKER native prototype row unchanged"
                );
                assert_eq!(
                    rows[1].last.to_bits(),
                    9.0_f64.to_bits(),
                    "TICKER native poisoned symbol updates real sentinel"
                );
            }
            let error = match result {
                Ok(()) => Value::Null,
                Err(TickerHandlerError::Coercion(e)) => json!({"name":e.name,"message":e.message}),
                Err(TickerHandlerError::Store(e)) => {
                    json!({"name":e.name(),"message":e.to_string()})
                }
            };
            let actual = snapshot(&store.tickers());
            let expected_notifications: Vec<_> = case["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| matches!(e["phase"].as_str(), Some("first" | "second")))
                .cloned()
                .collect();
            if case["mode"] == "throw" {
                assert_eq!(
                    case["error"]["message"], "subscriber witness",
                    "source notification error surfaces"
                );
                assert_eq!(
                    expected_notifications.len(),
                    1,
                    "source later listener and batch rows skipped"
                );
                assert_eq!(
                    case["after"][0]["last"]["bits"],
                    format!("{:016x}", 1.0_f64.to_bits()),
                    "source first ticker commits before subscriber throw"
                );
                assert_eq!(
                    case["after"][1]["last"]["bits"],
                    format!("{:016x}", 0.0_f64.to_bits()),
                    "source second ticker never runs after subscriber throw"
                );
                assert_eq!(
                    expected_notifications[0]["state"], case["after"],
                    "source throwing subscriber observes committed state"
                );
                assert_eq!(error, case["error"], "ticker notification error surfaces");
                assert_eq!(
                    *notifications.borrow(),
                    expected_notifications,
                    "ticker notification stops later listeners and rows"
                );
                assert_eq!(
                    actual, case["after"],
                    "ticker notification commits first row and aborts remaining rows"
                );
            } else if case["mode"] == "prototype-write" {
                assert!(error.is_null());
                assert_eq!(
                    expected_notifications.len(),
                    2,
                    "source prototype sentinel write notifies both subscribers"
                );
                assert_eq!(
                    actual, case["after"],
                    "ticker real prototype sentinel state"
                );
                assert_eq!(
                    *notifications.borrow(),
                    expected_notifications,
                    "ticker real prototype notifications"
                );
            } else {
                assert_eq!(
                    error, case["error"],
                    "ticker handler error {} {}",
                    source["file"], case["name"]
                );
                assert_eq!(
                    actual, case["after"],
                    "ticker handler state {} {}",
                    source["file"], case["name"]
                );
                assert_eq!(
                    *notifications.borrow(),
                    expected_notifications,
                    "ticker handler notifications {} {}",
                    source["file"],
                    case["name"]
                );
            }
            if case["name"].as_str().unwrap().starts_with("inherited:") {
                let events = case["events"].as_array().unwrap();
                let write = events.iter().find(|e| e["phase"] == "write");
                assert!(
                    write.is_some(),
                    "ticker handler inherited write boundary {} {}",
                    source["file"],
                    case["name"]
                );
                let write = write.unwrap();
                assert_eq!(
                    write["id"],
                    json!({"$undefined":true}),
                    "inherited identity before serialization"
                );
                assert_eq!(
                    write["key"], "undefined",
                    "source Store property key coercion"
                );
                assert_eq!(
                    write["type"],
                    if case["name"].as_str().unwrap().contains(":__proto__:") {
                        "object"
                    } else {
                        "function"
                    }
                );
            }
        }
        for witness in source["utf16"].as_array().unwrap() {
            assert!(
                serde_json::from_str::<String>(witness["input"].as_str().unwrap()).is_err(),
                "UTF8 key refusal"
            );
            assert_eq!(
                witness["last"], 9,
                "source surrogate key reaches Store write"
            );
        }
    }
}
#[test]
fn frozen_ticker_handlers() {
    check(&serde_json::from_str(include_str!("ticker-handler-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_ticker_handlers() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-handler-capture.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = serde_json::from_slice(&output.stdout).unwrap();
    check(&value);
    assert_eq!(
        value,
        serde_json::from_str::<Value>(include_str!("ticker-handler-oracle.json")).unwrap(),
        "ticker handler source drift"
    );
}

#[test]
#[ignore = "requires live Node and TypeScript"]
fn ticker_handler_field_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-handler-capture.mjs"
        ))
        .arg("--field-identity")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(oracle.as_array().unwrap().len(), 2, "both source handlers");
    for source in oracle.as_array().unwrap() {
        assert_eq!(
            source["cases"].as_array().unwrap().len(),
            3,
            "field identity case inventory"
        );
    }
    check(&oracle);
}

#[test]
fn ticker_notification_preserves_second_subscriber_identity() {
    let store = DefaultStore::new();
    Store::update(
        &store,
        StoreUpdate {
            tickers: Some(vec![Ticker {
                id: "btc".into(),
                symbol: "BTC".into(),
                bid: 0.0,
                ask: 0.0,
                last: 0.0,
                mark: 0.0,
                index: Some(0.0),
                mid_price: None,
                timestamp: None,
                percentage: 0.0,
                open_interest: 0.0,
                funding_rate: 0.0,
                funding_time: 0.0,
                volume: 0.0,
                quote_volume: 0.0,
                info: None,
            }]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let first_events = events.clone();
    let _first = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            first_events.borrow_mut().push("first");
            Ok(())
        }),
    );
    let cause = StoreSubscriberError::with_response("second subscriber", json!({"code":29}))
        .with_cause(Rc::new(std::io::Error::other("second original cause")));
    let second_cause = cause.clone();
    let second_events = events.clone();
    let _second = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            second_events.borrow_mut().push("second");
            Err(second_cause.clone())
        }),
    );
    let third_events = events.clone();
    let _third = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            third_events.borrow_mut().push("third");
            Ok(())
        }),
    );
    let result = handle_ticker_topic(
        &store,
        Some(&json!({"data":[{"instId":"BTC","lastPr":"7"},{"instId":"BTC","lastPr":"9"}]})),
    );
    let Err(TickerHandlerError::Store(safe_cex_store::StoreMutationError::Notification(error))) =
        result
    else {
        panic!("ticker second subscriber expected Notification, got {result:?}");
    };
    assert_eq!(
        error.subscriber_id(),
        1,
        "ticker second subscriber native identity"
    );
    assert_eq!(
        error.cause(),
        &cause,
        "ticker second subscriber original cause"
    );
    let projected = safe_cex_exchange::runtime::ExchangeError::from(
        safe_cex_store::StoreMutationError::Notification(error.clone()),
    );
    assert_eq!(
        projected.store_subscriber_ids,
        [1],
        "ticker shared projection retains subscriber identity"
    );
    assert_eq!(
        projected.response_json_view(),
        cause.response().cloned(),
        "ticker shared projection retains response"
    );
    assert_eq!(
        *events.borrow(),
        ["first", "second"],
        "ticker second subscriber stops fanout and batch"
    );
    assert_eq!(
        store.tickers()[0].last.to_bits(),
        7.0_f64.to_bits(),
        "ticker second subscriber sees committed first row"
    );
}
