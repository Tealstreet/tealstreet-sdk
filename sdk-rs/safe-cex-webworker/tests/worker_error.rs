use safe_cex_webworker::worker_logging::forward_error;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

fn native(row: &Value) -> Value {
    let cooldowns = RefCell::new(HashMap::new());
    let error = json!({"safeCexSentryReportKey":"same","safeCexSentryReportCooldownMs":100,"safeCexSentryTags":{"source":"control","control":true},"safeCexSentryExtra":{"winner":"control"}});
    let extra = match row["extraKind"].as_str().unwrap() {
        "absent" => None,
        "suppress" => Some(json!({"suppressConsoleLog":true})),
        "mixed" => Some(
            json!({"winner":"plain","sentryTags":{"source":"split","module":"override","split":true},"sentryExtra":{"winner":"split","suppressedRepeatedReports":99}}),
        ),
        _ => panic!("source extra inventory"),
    };
    let post_error = Rc::new("post");
    let calls: Vec<_> = [0,50,100,90,200].into_iter().map(|now| {
        let events=RefCell::new(Vec::new());
        let result = forward_error(
            &cooldowns,
            (safe_cex_types::ExchangeName::Binance,"account"),
            "worker message", &error, extra.as_ref(), row["send"].as_bool().unwrap(),
            (|| {events.borrow_mut().push(json!({"clock":now}));Ok(f64::from(now))},
             || {if row["errorKind"]=="string-throw" {events.borrow_mut().push(json!({"stringify":true}));return Err(Rc::new("serialization"));}
                 Ok(if row["errorKind"]=="native" {json!({"message":"source message","name":"Error","stack":"host-stack"})}else{json!({"message":safe_cex_num::js_string(Some(&error)).unwrap(),"name":"UnknownError","stack":"host-stack"})})},
             |payload| {events.borrow_mut().push(json!({"post":payload}));if row["postMode"]!="normal" {Err(post_error.clone())}else{Ok(())}},
             |prefix,message,original,cause| {assert!(std::ptr::eq(original,&error),"WORKER-ERROR fallback original identity");assert!(Rc::ptr_eq(&cause,&post_error),"WORKER-ERROR fallback post identity");events.borrow_mut().push(json!({"fallback":{"prefix":prefix,"message":message}}));if row["postMode"]=="console-throw"{Err(Rc::new("console"))}else{Ok(())}}
            ),
        );
        let state:Vec<_>=cooldowns.borrow().iter().map(|(key,(last,count))|json!([key,{"lastReportedAt":last,"suppressedCount":count}])).collect();
        json!({"now":now,"events":events.into_inner(),"failure":result.err().map(|e|*e),"cooldowns":state})
    }).collect();
    json!({"calls":calls})
}

#[test]
fn reporting_matches_live_cooldown_delivery_and_callback_effects() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_error_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Preserve the source lone-surrogate witness as escaped JSON text; Value's
    // UTF-8 ingress intentionally refuses that one string.
    let raw = String::from_utf8(output.stdout).unwrap();
    assert!(
        raw.contains("\\ud800"),
        "WORKER-ERROR escaped source surrogate witness"
    );
    assert!(
        serde_json::from_str::<Value>(&raw).is_err(),
        "CHOSEN WORKER-ERROR UTF8 ingress refuses lone surrogate"
    );
    let source: Value = serde_json::from_str(&raw.replace("\\ud800", "SURROGATE_WITNESS")).unwrap();
    for case in source["textCases"].as_array().unwrap().iter().take(2) {
        let message = case["message"].as_str().unwrap();
        let sent = RefCell::new(None);
        forward_error(
            &RefCell::new(HashMap::new()),
            (safe_cex_types::ExchangeName::Binance, "account"),
            message,
            &json!({}),
            None,
            true,
            (
                || Ok::<_, ()>(0.0),
                || Ok(json!({"message":message,"name":"Error","stack":"host-stack"})),
                |p| {
                    *sent.borrow_mut() = p.data;
                    Ok(())
                },
                |_, _, _, _| Err(()),
            ),
        )
        .unwrap();
        assert_eq!(
            sent.borrow().as_ref().unwrap()[1]["sentryData"]["error"]["message"],
            case["reported"],
            "WORKER-ERROR scalar Unicode positive"
        );
    }
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 72);
    for probe in source["clockBits"].as_array().unwrap() {
        let raw: Value = serde_json::from_str(probe["token"].as_str().unwrap()).unwrap();
        let now = safe_cex_num::numeric_ingress::js_value_as_f64(&raw).unwrap();
        let cooldowns = RefCell::new(HashMap::new());
        let error = json!({"safeCexSentryReportKey":"same","safeCexSentryReportCooldownMs":100});
        forward_error(
            &cooldowns,
            (safe_cex_types::ExchangeName::Binance, "account"),
            "bits",
            &error,
            None,
            true,
            (
                || Ok::<_, ()>(now),
                || Ok(json!({})),
                |_| Ok(()),
                |_, _, _, _| Err(()),
            ),
        )
        .unwrap();
        assert_eq!(
            format!(
                "{:016x}",
                cooldowns
                    .borrow()
                    .get("same")
                    .expect("WORKER-ERROR cooldown entry must be committed")
                    .0
                    .to_bits()
            ),
            probe["last"].as_str().unwrap(),
            "WORKER-ERROR clock bits before serialization"
        );
    }
    let caller = RefCell::new(json!({"safeCexSentryTags":{"phase":"before"}}));
    let error = caller.borrow().clone();
    let sent = RefCell::new(None);
    forward_error(
        &RefCell::new(HashMap::new()),
        (safe_cex_types::ExchangeName::Binance, "account"),
        "alias",
        &error,
        None,
        true,
        (
            || Ok::<_, ()>(0.0),
            || {
                caller.borrow_mut()["safeCexSentryTags"]["phase"] = json!("after");
                Ok(json!({"message":"alias-error","name":"UnknownError","stack":"host-stack"}))
            },
            |p| {
                *sent.borrow_mut() = p.data;
                Ok(())
            },
            |_, _, _, _| Err(()),
        ),
    )
    .unwrap();
    assert_eq!(
        source["alias"],
        json!({"caller":"after","reported":"after"}),
        "WORKER-ERROR source live coercion alias"
    );
    assert_eq!(
        caller.borrow()["safeCexSentryTags"]["phase"],
        "after",
        "WORKER-ERROR caller clone mutation occurred"
    );
    assert_eq!(
        sent.borrow().as_ref().unwrap()[1]["sentryData"]["tags"]["phase"],
        "before",
        "CHOSEN WORKER-ERROR owned control tags snapshot"
    );
    for row in rows {
        let result = native(row);
        let expected = if row["errorKind"] == "getter-throw" {
            // The JSON materialization boundary cannot retain a callable getter.
            for call in row["calls"].as_array().unwrap() {
                assert_eq!(
                    call["failure"], "serialization",
                    "CHOSEN WORKER-ERROR source getter throws before clock"
                );
                assert_eq!(call["events"], json!([{"getter":true}]));
            }
            for call in result["calls"].as_array().unwrap() {
                assert_eq!(
                    call["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|event| event.get("post").is_some())
                        .count(),
                    1,
                    "CHOSEN WORKER-ERROR materialized JSON getter omitted"
                );
            }
            rows.iter()
                .find(|r| {
                    r["errorKind"] == "plain"
                        && r["postMode"] == row["postMode"]
                        && r["extraKind"] == row["extraKind"]
                        && r["send"] == row["send"]
                })
                .unwrap()
        } else {
            row
        };
        assert_numeric_tree_eq(
            &result["calls"],
            &expected["calls"],
            "WORKER-ERROR sequence",
        );
    }
}
fn assert_numeric_tree_eq(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(_), Value::Number(_)) => assert_eq!(
            safe_cex_num::numeric_ingress::js_value_as_f64(actual)
                .unwrap()
                .to_bits(),
            safe_cex_num::numeric_ingress::js_value_as_f64(expected)
                .unwrap()
                .to_bits(),
            "{path} numeric bits"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path} array length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_numeric_tree_eq(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path} object keys"
            );
            for (k, a) in a {
                assert_numeric_tree_eq(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn reporting_releases_cooldown_borrows_and_preserves_native_clock_bits() {
    let cooldowns = RefCell::new(HashMap::new());
    let error = json!({"safeCexSentryReportKey":"same","safeCexSentryReportCooldownMs":100});
    let events = RefCell::new(Vec::new());
    let failure = Rc::new("materialize");
    let result = forward_error(
        &cooldowns,
        (safe_cex_types::ExchangeName::Binance, "account"),
        "message",
        &error,
        None,
        true,
        (
            || {
                cooldowns.borrow_mut().insert("other".into(), (1.0, 2.0));
                events.borrow_mut().push("clock");
                Ok(-0.0)
            },
            || {
                assert_eq!(
                    cooldowns
                        .borrow()
                        .get("same")
                        .expect("WORKER-ERROR cooldown entry must be committed")
                        .0
                        .to_bits(),
                    (-0.0_f64).to_bits(),
                    "WORKER-ERROR native signed zero committed before materialization"
                );
                events.borrow_mut().push("materialize");
                Err(failure.clone())
            },
            |_| {
                events.borrow_mut().push("post");
                Ok(())
            },
            |_, _, _, _| {
                events.borrow_mut().push("fallback");
                Ok(())
            },
        ),
    );
    assert!(
        Rc::ptr_eq(&result.unwrap_err(), &failure),
        "WORKER-ERROR materialization failure identity"
    );
    assert_eq!(
        *events.borrow(),
        vec!["clock", "materialize"],
        "WORKER-ERROR materialization failure effects"
    );
}
