use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue;
use safe_cex_venue_hyperliquid::ws_public::{HyperliquidPublicWebSocket, SubscriptionError};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

fn object(entries: &[(&str, JsJsonValue)]) -> JsJsonValue {
    JsJsonValue::Object(
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(value: &str) -> JsJsonValue {
    JsJsonValue::String(value.into())
}

#[test]
fn public_subscription_state_and_sender_effects_match_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-subscription-identities.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source subscription corpus {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 23, "subscription corpus inventory");
    let comparisons = Cell::new(0usize);
    let category = |kind: &str, count: usize| {
        let rows: Vec<&Value> = cases.iter().filter(|row| row["kind"] == kind).collect();
        assert_eq!(
            rows.len(),
            count,
            "SUBSCRIPTION-CARDINALITY source category {kind}"
        );
        rows
    };
    let key_rows = category("key-identity", 5);
    let string_rows = category("string-boundary", 5);
    let evaluation_rows = category("evaluation-count", 2);
    let reentry_rows = category("reentry", 1);
    for failure in [false, true] {
        let state = HyperliquidPublicWebSocket::default();
        let events = RefCell::new(Vec::<Value>::new());
        let sender = |message: &str| {
            events
                .borrow_mut()
                .push(json!({"message":message,"active":state.active_subscriptions()}));
            if failure {
                Err(ExchangeError::new("send witness"))
            } else {
                Ok(())
            }
        };
        let rows: Vec<&Value> = cases
            .iter()
            .filter(|row| row["failure"] == failure)
            .collect();
        assert_eq!(
            rows.len(),
            5,
            "SUBSCRIPTION-CARDINALITY source failure category"
        );
        for index in 0..5 {
            let row = rows
                .get(index)
                .expect("SUBSCRIPTION-CARDINALITY native failure index coverage");
            comparisons.set(comparisons.get() + 1);
            let event_count = [1, 1, 2, 3, 4][index];
            let active_count = [1, 1, 0, 1, 1][index];
            assert_eq!(
                row["events"].as_array().unwrap().len(),
                event_count,
                "SUBSCRIPTION-CARDINALITY source per-case events"
            );
            assert_eq!(
                row["active"].as_array().unwrap().len(),
                active_count,
                "SUBSCRIPTION-CARDINALITY source per-case active keys"
            );
            let subscription = JsJsonValue::from(row["subscription"].clone());
            let result = if row["operation"] == "subscribe" {
                state.send_subscription(&subscription, &sender).map(Some)
            } else {
                state
                    .send_unsubscription(&subscription, &sender)
                    .map(|()| None)
            };
            assert_eq!(
                result.is_err(),
                row.get("error").is_some(),
                "unsubscribe sender failure must propagate"
            );
            match result {
                Ok(value) => assert_eq!(
                    value,
                    row.get("result").map(|v| v.as_bool().unwrap()),
                    "native subscription return identity"
                ),
                Err(SubscriptionError::Send(error)) => {
                    assert_eq!(row["error"]["name"], "Error", "source sender error class");
                    assert_eq!(
                        error.message, row["error"]["message"],
                        "sender error retains committed state"
                    );
                }
                Err(error) => panic!("unexpected coercion {error:?}"),
            }
            assert_eq!(
                events.borrow().len(),
                event_count,
                "SUBSCRIPTION-CARDINALITY independent native events"
            );
            assert_eq!(
                state.active_subscriptions().len(),
                active_count,
                "SUBSCRIPTION-CARDINALITY independent native active keys"
            );
            assert_eq!(
                state.active_subscriptions(),
                serde_json::from_value::<Vec<String>>(row["active"].clone()).unwrap(),
                "native active subscription identity"
            );
            assert_eq!(
                *events.borrow(),
                *row["events"].as_array().unwrap(),
                "mutation precedes sender callback"
            );
        }
    }
    let base = || vec![("type", text("bbo")), ("coin", text("BTC"))];
    let depth = |value| {
        let mut v = base();
        v.push(("depth", value));
        object(&v)
    };
    let pairs = [
        [
            object(&base()),
            object(&[("coin", text("BTC")), ("type", text("bbo"))]),
        ],
        [
            depth(JsJsonValue::Number(-0.0)),
            depth(JsJsonValue::Number(0.0)),
        ],
        [
            depth(JsJsonValue::Number(f64::NAN)),
            depth(JsJsonValue::Null),
        ],
        [
            depth(JsJsonValue::Number(f64::INFINITY)),
            depth(JsJsonValue::Null),
        ],
        [
            object(&[
                ("2", text("second")),
                ("1", text("first")),
                ("type", text("bbo")),
            ]),
            object(&[
                ("1", text("first")),
                ("2", text("second")),
                ("type", text("bbo")),
            ]),
        ],
    ];
    assert_eq!(
        pairs.len(),
        key_rows.len(),
        "SUBSCRIPTION-CARDINALITY independent expected/native key cases"
    );
    for index in 0..5 {
        let pair = pairs
            .get(index)
            .expect("SUBSCRIPTION-CARDINALITY native pair index coverage");
        let row = key_rows
            .get(index)
            .expect("SUBSCRIPTION-CARDINALITY expected pair index coverage");
        comparisons.set(comparisons.get() + 1);
        assert_eq!(
            row["results"].as_array().unwrap().len(),
            2,
            "SUBSCRIPTION-CARDINALITY source pair results"
        );
        assert_eq!(
            row["nativeDepths"].as_array().unwrap().len(),
            2,
            "SUBSCRIPTION-CARDINALITY source pair descriptors"
        );
        let event_count = [2, 1, 1, 1, 1][index];
        assert_eq!(
            row["events"].as_array().unwrap().len(),
            event_count,
            "SUBSCRIPTION-CARDINALITY source pair events"
        );
        let native_depths: Vec<Value> = pair.iter().map(|input| {
            let JsJsonValue::Object(entries) = input else { panic!("native subscription object") };
            match entries.iter().find(|(key,_)| key == "depth").map(|(_,value)|value) {
                None => json!({"kind":"missing"}),
                Some(JsJsonValue::Null) => json!({"kind":"null"}),
                Some(JsJsonValue::Number(value)) => json!({"kind":"number","bits":format!("{:016x}", if value.is_nan() {0x7ff8000000000000} else {value.to_bits()})}),
                _ => panic!("unexpected native depth"),
            }
        }).collect();
        assert_eq!(
            json!(native_depths),
            row["nativeDepths"],
            "pre-stringify native depth identity"
        );
        let state = HyperliquidPublicWebSocket::default();
        let events = RefCell::new(Vec::<String>::new());
        let sender = |message: &str| {
            events.borrow_mut().push(message.into());
            Ok(())
        };
        let results: Vec<bool> = pair
            .iter()
            .map(|v| state.send_subscription(v, &sender).unwrap())
            .collect();
        assert_eq!(
            results.len(),
            2,
            "SUBSCRIPTION-CARDINALITY independent native pair results"
        );
        assert_eq!(
            events.borrow().len(),
            event_count,
            "SUBSCRIPTION-CARDINALITY independent native pair events"
        );
        assert_eq!(
            state.active_subscriptions().len(),
            event_count,
            "SUBSCRIPTION-CARDINALITY independent native pair active keys"
        );
        assert_eq!(
            json!(results),
            row["results"],
            "JSON key collision retains duplicate suppression"
        );
        assert_eq!(
            json!(*events.borrow()),
            row["events"],
            "ordered envelope field identity"
        );
        assert_eq!(
            json!(state.active_subscriptions()),
            row["active"],
            "ordered subscription set identity"
        );
    }
    let state = HyperliquidPublicWebSocket::default();
    let events = RefCell::new(Vec::<String>::new());
    let entered = Cell::new(false);
    let subscription = object(&[
        ("type", text("candle")),
        ("coin", text("SOL")),
        ("interval", text("5m")),
    ]);
    let record = |message: &str| {
        events.borrow_mut().push(message.into());
        Ok(())
    };
    let sender = |message: &str| {
        record(message)?;
        if !entered.replace(true) {
            state.send_unsubscription(&subscription, &record).unwrap();
            state.send_subscription(&subscription, &record).unwrap();
        }
        Ok(())
    };
    assert!(state.send_subscription(&subscription, &sender).unwrap());
    let row = reentry_rows[0];
    comparisons.set(comparisons.get() + 1);
    assert_eq!(
        row["events"].as_array().unwrap().len(),
        3,
        "SUBSCRIPTION-CARDINALITY source reentry events"
    );
    assert_eq!(
        events.borrow().len(),
        3,
        "SUBSCRIPTION-CARDINALITY independent native reentry events"
    );
    assert_eq!(
        json!(*events.borrow()),
        row["events"],
        "subscription reentry sees committed state without live borrow"
    );
    assert_eq!(
        json!(state.active_subscriptions()),
        row["active"],
        "reentry restores active key"
    );
    let mut representation_refusals = 0;
    for index in 0..5 {
        let row = string_rows
            .get(index)
            .expect("SUBSCRIPTION-CARDINALITY string index coverage");
        comparisons.set(comparisons.get() + 1);
        let input = row["serializedInput"].as_str().unwrap();
        match serde_json::from_str::<Value>(input) {
            Ok(value) => {
                let state = HyperliquidPublicWebSocket::default();
                let events = RefCell::new(Vec::<String>::new());
                let sender = |message: &str| {
                    events.borrow_mut().push(message.into());
                    Ok(())
                };
                assert!(
                    state
                        .send_subscription(&JsJsonValue::from(value), &sender)
                        .unwrap()
                );
                assert_eq!(
                    json!(*events.borrow()),
                    row["events"],
                    "scalar subscription strings retain source wire identity"
                );
            }
            Err(_) => {
                assert!(
                    input.contains("\\ud800") || input.contains("\\udfff"),
                    "only lone surrogate input witnesses refuse"
                );
                assert_eq!(
                    row["result"], true,
                    "source accepts surrogate subscription data"
                );
                assert_eq!(row["events"].as_array().unwrap().len(), 1);
                representation_refusals += 1;
            }
        }
    }
    assert_eq!(
        representation_refusals, 2,
        "subscription UTF-8 representation refusal inventory"
    );
    struct Evaluated {
        count: Cell<usize>,
        throw_on_second: bool,
    }
    impl safe_cex_venue_hyperliquid::ws_public::SubscriptionInput for Evaluated {
        fn evaluate(&self) -> Result<JsJsonValue, safe_cex_num::JsCoercionError> {
            let count = self.count.get() + 1;
            self.count.set(count);
            if self.throw_on_second && count == 2 {
                return Err(safe_cex_num::JsCoercionError {
                    name: "TypeError",
                    message: "second evaluation witness",
                });
            }
            Ok(object(&[
                ("type", text("bbo")),
                ("coin", text(if count % 2 == 1 { "BTC" } else { "ETH" })),
            ]))
        }
    }
    for index in 0..2 {
        let row = evaluation_rows
            .get(index)
            .expect("SUBSCRIPTION-CARDINALITY evaluation index coverage");
        let state = HyperliquidPublicWebSocket::default();
        let input = Evaluated {
            count: Cell::new(0),
            throw_on_second: row["throwOnSecond"].as_bool().unwrap(),
        };
        let events = RefCell::new(Vec::<String>::new());
        let sender = |message: &str| {
            events.borrow_mut().push(message.into());
            Ok(())
        };
        let steps = row["steps"].as_array().unwrap();
        assert_eq!(
            steps.len(),
            2,
            "SUBSCRIPTION-CARDINALITY source evaluation steps"
        );
        for step_index in 0..2 {
            let step = steps
                .get(step_index)
                .expect("SUBSCRIPTION-CARDINALITY native evaluation index coverage");
            comparisons.set(comparisons.get() + 1);
            assert_eq!(
                step["evaluations"].as_u64().unwrap(),
                [2, 3][step_index],
                "SUBSCRIPTION-CARDINALITY source evaluation count"
            );
            match state.send_subscription(&input, &sender) {
                Ok(value) => assert_eq!(
                    json!(value),
                    step["result"],
                    "native evaluation result identity"
                ),
                Err(SubscriptionError::Coercion(error)) => {
                    assert_eq!(error.name, step["error"]["name"]);
                    assert_eq!(error.message, step["error"]["message"]);
                }
                Err(error) => panic!("unexpected evaluation sender error {error:?}"),
            }
            assert_eq!(
                input.count.get(),
                [2, 3][step_index],
                "SUBSCRIPTION-CARDINALITY independent native evaluations"
            );
            assert_eq!(
                json!(input.count.get()),
                step["evaluations"],
                "public subscription two evaluation count"
            );
            assert_eq!(
                json!(*events.borrow()),
                step["events"],
                "second evaluation controls envelope independently of key"
            );
            assert_eq!(
                json!(state.active_subscriptions()),
                step["active"],
                "second evaluation failure retains committed first key"
            );
        }
    }
    assert_eq!(
        comparisons.get(),
        25,
        "SUBSCRIPTION-CARDINALITY native comparison total"
    );
}

#[test]
fn public_topics_preserve_send_sequence_and_callback_failures() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-topic-effects.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "public topic source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(capture["cases"].as_array().unwrap().len(), 5);
    let cases = capture["cases"].as_array().unwrap();
    let comparisons = Cell::new(0usize);
    let modes = ["none", "send-1", "send-2", "user"];
    for index in 0..4 {
        let rows: Vec<&Value> = cases
            .iter()
            .filter(|row| row["failure"] == modes[index])
            .collect();
        assert_eq!(
            rows.len(),
            1,
            "PUBLIC-TOPIC-CARDINALITY source handler inventory"
        );
        let row = rows
            .first()
            .expect("PUBLIC-TOPIC-CARDINALITY native handler index coverage");
        comparisons.set(comparisons.get() + 1);
        let count = [6, 1, 2, 3][index];
        assert_eq!(
            row["effects"].as_array().unwrap().len(),
            count,
            "PUBLIC-TOPIC-CARDINALITY source per-handler effects"
        );
        let state = HyperliquidPublicWebSocket::default();
        let all_mids = object(&[("type", text("allMids"))]);
        assert!(state.send_subscription(&all_mids, &|_| Ok(())).unwrap());
        let effects = RefCell::new(Vec::<Value>::new());
        let sends = Cell::new(0);
        let failure = row["failure"].as_str().unwrap();
        let sender = |message: &str| {
            sends.set(sends.get() + 1);
            effects.borrow_mut().push(
                json!({"kind":"send","message":message,"active":state.active_subscriptions()}),
            );
            if failure == format!("send-{}", sends.get()) {
                Err(ExchangeError::new(failure))
            } else {
                Ok(())
            }
        };
        let user = || {
            effects
                .borrow_mut()
                .push(json!({"kind":"user","active":state.active_subscriptions()}));
            if failure == "user" {
                Err(ExchangeError::new(failure))
            } else {
                Ok(())
            }
        };
        let result = state
            .subscribe_public_topics(&sender, &user)
            .and_then(|()| {
                if failure == "none" {
                    state.subscribe_public_topics(&sender, &user)
                } else {
                    Ok(())
                }
            });
        assert_eq!(
            result.is_err(),
            row.get("error").is_some(),
            "public topic failure must propagate"
        );
        match result {
            Err(SubscriptionError::Send(error)) => {
                assert_eq!(
                    row["error"]["name"], "Error",
                    "public topic source error class"
                );
                assert_eq!(
                    error.message, row["error"]["message"],
                    "public topic error identity"
                );
            }
            Err(error) => panic!("unexpected public topic error {error:?}"),
            Ok(()) => {}
        }
        assert_eq!(
            effects.borrow().len(),
            count,
            "PUBLIC-TOPIC-CARDINALITY independent native effects"
        );
        assert_eq!(
            json!(*effects.borrow()),
            row["effects"],
            "public topics ordered effects and no deduplication"
        );
        assert_eq!(
            json!(state.active_subscriptions()),
            row["active"],
            "public topics leave subscription tracking untouched"
        );
    }
    let state = HyperliquidPublicWebSocket::default();
    let effects = RefCell::new(Vec::<Value>::new());
    let entered = Cell::new(false);
    let nested = object(&[("type", text("bbo")), ("coin", text("REENTRY"))]);
    let record = |message: &str| {
        effects
            .borrow_mut()
            .push(json!({"kind":"send","message":message,"active":state.active_subscriptions()}));
        Ok(())
    };
    let sender = |message: &str| {
        record(message)?;
        if !entered.replace(true) {
            state.send_subscription(&nested, &record).unwrap();
        }
        Ok(())
    };
    let user = || {
        effects
            .borrow_mut()
            .push(json!({"kind":"user","active":state.active_subscriptions()}));
        state.send_unsubscription(&nested, &record).unwrap();
        Ok(())
    };
    state.subscribe_public_topics(&sender, &user).unwrap();
    let row = capture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["failure"] == "reentry")
        .unwrap();
    comparisons.set(comparisons.get() + 1);
    assert_eq!(
        row["effects"].as_array().unwrap().len(),
        5,
        "PUBLIC-TOPIC-CARDINALITY source reentry effects"
    );
    assert_eq!(
        effects.borrow().len(),
        5,
        "PUBLIC-TOPIC-CARDINALITY independent native reentry effects"
    );
    assert_eq!(
        comparisons.get(),
        5,
        "PUBLIC-TOPIC-CARDINALITY native comparison total"
    );
    assert_eq!(
        json!(*effects.borrow()),
        row["effects"],
        "public topic reentry effects"
    );
    assert_eq!(
        json!(state.active_subscriptions()),
        row["active"],
        "public topic reentry tracking"
    );
}

fn subscription_fields(input: &JsJsonValue) -> Value {
    let JsJsonValue::Object(entries) = input else {
        panic!("native subscription record")
    };
    Value::Array(
        entries
            .iter()
            .map(|(key, value)| match value {
                JsJsonValue::Undefined => json!({"key":key,"kind":"undefined"}),
                JsJsonValue::String(value) => json!({"key":key,"kind":"string","value":value}),
                _ => panic!("unexpected native subscription field"),
            })
            .collect(),
    )
}
fn subscription_json(input: &JsJsonValue) -> Value {
    serde_json::from_str(&safe_cex_num::js_json_stringify(input).unwrap().unwrap()).unwrap()
}

#[test]
fn user_topics_preserve_native_fields_and_owned_key_snapshot() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/user-topic-effects.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "user topic source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let missing = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/user-topic-effects.cjs"
        ))
        .arg("--missing-runtime-control")
        .output()
        .unwrap();
    assert!(
        !missing.status.success(),
        "missing source runtime must fail loudly"
    );
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("MODULE_NOT_FOUND"),
        "missing source runtime must name MODULE_NOT_FOUND"
    );
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "user-topic corpus inventory");
    let rows: Vec<&Value> = cases
        .iter()
        .filter(|row| row.get("kind").is_none())
        .collect();
    assert_eq!(
        rows.len(),
        27,
        "USER-TOPIC-CARDINALITY source handler total"
    );
    let counts = [
        0, 0, 0, 3, 1, 2, 3, 1, 2, 3, 1, 2, 2, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 6, 1, 2,
    ];
    let comparisons = Cell::new(0usize);
    let native_outputs = Cell::new(0usize);
    for index in 0..27 {
        let row = rows
            .get(index)
            .expect("USER-TOPIC-CARDINALITY expected/native index coverage");
        comparisons.set(comparisons.get() + 1);
        assert_eq!(
            row["effects"].as_array().unwrap().len(),
            counts[index],
            "USER-TOPIC-CARDINALITY source per-case effects"
        );
        assert_eq!(
            row["nativeEffects"].as_array().unwrap().len(),
            counts[index],
            "USER-TOPIC-CARDINALITY source per-case descriptors"
        );
        let state = HyperliquidPublicWebSocket::default();
        let effects = RefCell::new(Vec::new());
        let native_effects = RefCell::new(Vec::new());
        let calls = Cell::new(0);
        let callback = |input: &JsJsonValue| {
            calls.set(calls.get() + 1);
            effects.borrow_mut().push(subscription_json(input));
            native_effects.borrow_mut().push(subscription_fields(input));
            if calls.get() == row["failAt"].as_u64().unwrap() as usize {
                Err(SubscriptionError::Send(ExchangeError::new(
                    "user topic send witness",
                )))
            } else {
                Ok(false)
            }
        };
        let result = state.subscribe_user_topics(
            serde_json::from_value(row["ownKeys"].clone()).unwrap(),
            &callback,
        );
        assert_eq!(
            result.is_err(),
            row.get("error").is_some(),
            "user-topic callback error must propagate"
        );
        match result {
            Err(SubscriptionError::Send(error)) => {
                assert_eq!(row["error"]["name"], "Error");
                assert_eq!(
                    error.message, row["error"]["message"],
                    "user-topic error identity"
                );
            }
            Err(error) => panic!("unexpected user-topic error {error:?}"),
            Ok(()) => {}
        }
        assert_eq!(
            calls.get(),
            counts[index],
            "USER-TOPIC-CARDINALITY independent native output count"
        );
        assert_eq!(
            native_effects.borrow().len(),
            counts[index],
            "USER-TOPIC-CARDINALITY independent native descriptor count"
        );
        native_outputs.set(native_outputs.get() + calls.get());
        assert_eq!(
            calls.get(),
            row["nativeEffects"].as_array().unwrap().len(),
            "user-topic false return never stops enumeration"
        );
        assert_eq!(
            json!(*native_effects.borrow()),
            row["nativeEffects"],
            "user-topic pre-stringify native field identity"
        );
        assert_eq!(
            json!(*effects.borrow()),
            row["effects"],
            "user-topic ordered effects ignore false return"
        );
    }
    assert_eq!(
        native_outputs.get(),
        50,
        "USER-TOPIC-CARDINALITY native handler output total"
    );
    let reentry_rows: Vec<&Value> = cases
        .iter()
        .filter(|row| row["kind"] == "snapshot-reentry")
        .collect();
    assert_eq!(
        reentry_rows.len(),
        1,
        "USER-TOPIC-CARDINALITY source reentry inventory"
    );
    let row = reentry_rows[0];
    comparisons.set(comparisons.get() + 1);
    let state = HyperliquidPublicWebSocket::default();
    let keys = RefCell::new(serde_json::from_value::<Vec<String>>(row["keys"].clone()).unwrap());
    let effects = RefCell::new(Vec::new());
    let native_effects = RefCell::new(Vec::new());
    let entered = Cell::new(false);
    let record = |input: &JsJsonValue| {
        effects.borrow_mut().push(subscription_json(input));
        native_effects.borrow_mut().push(subscription_fields(input));
        Ok(false)
    };
    let callback = |input: &JsJsonValue| {
        record(input)?;
        if !entered.replace(true) {
            {
                let mut keys = keys.borrow_mut();
                keys.retain(|key| key != "l2Book.ETH");
                keys.push("candle.SOL.5m".into());
            }
            let snapshot = keys.borrow().clone();
            state.subscribe_user_topics(snapshot, &record)?;
        }
        Ok(false)
    };
    let snapshot = keys.borrow().clone();
    state.subscribe_user_topics(snapshot, &callback).unwrap();
    assert_eq!(
        row["nativeEffects"].as_array().unwrap().len(),
        5,
        "USER-TOPIC-CARDINALITY source reentry outputs"
    );
    assert_eq!(
        native_effects.borrow().len(),
        5,
        "USER-TOPIC-CARDINALITY independent native reentry outputs"
    );
    assert_eq!(
        json!(*native_effects.borrow()),
        row["nativeEffects"],
        "user-topic reentry retains original snapshot native fields"
    );
    assert_eq!(
        json!(*effects.borrow()),
        row["effects"],
        "user-topic original snapshot survives dictionary mutation"
    );
    assert_eq!(
        json!(*keys.borrow()),
        row["finalKeys"],
        "user-topic subsequent snapshot sees mutation"
    );
    let mut refusals = 0;
    let string_rows: Vec<&Value> = cases
        .iter()
        .filter(|row| row["kind"] == "string-boundary")
        .collect();
    assert_eq!(
        string_rows.len(),
        4,
        "USER-TOPIC-CARDINALITY source string inventory"
    );
    for index in 0..4 {
        let row = string_rows
            .get(index)
            .expect("USER-TOPIC-CARDINALITY native string index coverage");
        comparisons.set(comparisons.get() + 1);
        let source_records: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_str(row["serializedEffects"].as_str().unwrap()).unwrap();
        assert_eq!(
            source_records.len(),
            1,
            "USER-TOPIC-CARDINALITY source string records"
        );
        match serde_json::from_str::<Vec<String>>(row["serializedKeys"].as_str().unwrap()) {
            Ok(keys) => {
                let state = HyperliquidPublicWebSocket::default();
                let effects = RefCell::new(Vec::new());
                let native = RefCell::new(Vec::new());
                let callback = |input: &JsJsonValue| {
                    effects.borrow_mut().push(subscription_json(input));
                    native.borrow_mut().push(subscription_fields(input));
                    Ok(false)
                };
                state.subscribe_user_topics(keys, &callback).unwrap();
                assert_eq!(
                    serde_json::to_string(&*effects.borrow()).unwrap(),
                    row["serializedEffects"],
                    "scalar user-topic string payload identity"
                );
                assert_eq!(
                    serde_json::to_string(&*native.borrow()).unwrap(),
                    row["serializedNativeEffects"],
                    "scalar user-topic native field identity"
                );
            }
            Err(_) => {
                assert!(
                    row["serializedKeys"].as_str().unwrap().contains("\\ud800")
                        || row["serializedKeys"].as_str().unwrap().contains("\\udfff")
                );
                refusals += 1;
            }
        }
    }
    assert_eq!(refusals, 2, "user-topic UTF-8 refusal witnesses");
    assert_eq!(
        comparisons.get(),
        32,
        "USER-TOPIC-CARDINALITY native comparison total"
    );
}
