use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown, from_json, to_json},
    runtime::ExchangeError,
};
use safe_cex_webworker::worker_logging::forward_error;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn native(row: &Value) -> Value {
    let cooldowns = Js::new_map();
    let extra = match row["extraKind"].as_str().unwrap() {
        "absent" => None,
        "suppress" => Some(from_json(&json!({"suppressConsoleLog":true}))),
        "mixed" => Some(from_json(
            &json!({"winner":"plain","sentryTags":{"source":"split","module":"override","split":true},"sentryExtra":{"winner":"split","suppressedRepeatedReports":99}}),
        )),
        _ => panic!("source extra inventory"),
    };
    let post_error = Rc::new(ExchangeError::new("post"));
    let calls: Vec<_> = [0,50,100,90,200].into_iter().map(|now| {
        let events=Rc::new(RefCell::new(Vec::new()));
        let error=from_json(&json!({"safeCexSentryReportKey":"same","safeCexSentryReportCooldownMs":100,"safeCexSentryTags":{"source":"control","control":true},"safeCexSentryExtra":{"winner":"control"}}));
        if row["errorKind"]=="getter-throw" {
            let events=events.clone();
            error.define_accessor("safeCexSentryReportKey",Some(Rc::new(move |_| {
                events.borrow_mut().push(json!({"getter":true}));
                Err(Thrown::from(ExchangeError::new("serialization")))
            })),None).unwrap();
        }
        let result = forward_error(
            &cooldowns,
            (safe_cex_types::ExchangeName::Binance,"account"),
            &Js::str("worker message"), &error, extra.as_ref(), row["send"].as_bool().unwrap(),
            (|| {events.borrow_mut().push(json!({"clock":now}));Ok(f64::from(now))},
             || {if row["errorKind"]=="string-throw" {events.borrow_mut().push(json!({"stringify":true}));return Err(Rc::new(ExchangeError::new("serialization")));}
                 Ok(from_json(&if row["errorKind"]=="native" {json!({"message":"source message","name":"Error","stack":"host-stack"})}else{json!({"message":"[object Object]","name":"UnknownError","stack":"host-stack"})}))},
             |payload| {events.borrow_mut().push(json!({"post":{"op":"error","data":to_json(&payload.data).unwrap()}}));if row["postMode"]!="normal" {Err(post_error.clone())}else{Ok(())}},
             |prefix,message,original,cause| {assert!(std::ptr::eq(original,&error),"WORKER-ERROR fallback original identity");assert!(Rc::ptr_eq(&cause,&post_error),"WORKER-ERROR fallback post identity");events.borrow_mut().push(json!({"fallback":{"prefix":prefix,"message":to_json(message).unwrap()}}));if row["postMode"]=="console-throw"{Err(Rc::new(ExchangeError::new("console")))}else{Ok(())}}
            ),
        );
        let previous=cooldowns.call_method("get",vec![Js::str("same")]).unwrap();
        let state=if previous.is_undefined(){json!([])}else{json!([["same",to_json(&previous).unwrap()]])};
        let observed=events.borrow().clone();
        json!({"now":now,"events":observed,"failure":result.err().map(|e|e.message.clone()),"cooldowns":state})
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
    let raw = String::from_utf8(output.stdout).unwrap();
    assert!(
        raw.contains("\\ud800"),
        "WORKER-ERROR escaped source surrogate witness"
    );
    let source: Value = serde_json::from_str(&raw.replace("\\ud800", "SURROGATE_WITNESS")).unwrap();
    for (i, case) in source["textCases"].as_array().unwrap().iter().enumerate() {
        let message = if i == 2 {
            Js::utf16(&[0xd800])
        } else {
            from_json(&case["message"])
        };
        let sent = RefCell::new(None);
        forward_error(
            &Js::new_map(),
            (safe_cex_types::ExchangeName::Binance, "account"),
            &message,
            &Js::object(vec![]),
            None,
            true,
            (
                || Ok::<_, ExchangeError>(0.0),
                || {
                    Ok(Js::object(vec![
                        ("message", message.clone()),
                        ("name", Js::str("Error")),
                        ("stack", Js::str("host-stack")),
                    ]))
                },
                |p| {
                    *sent.borrow_mut() = Some(p.data);
                    Ok(())
                },
                |_, _, _, e| Err(e),
            ),
        )
        .unwrap();
        let reported = sent
            .borrow()
            .as_ref()
            .unwrap()
            .get("1")
            .unwrap()
            .get("sentryData")
            .unwrap()
            .get("error")
            .unwrap()
            .get("message")
            .unwrap();
        assert_eq!(
            reported.to_js_string().unwrap(),
            message.to_js_string().unwrap(),
            "WORKER-ERROR original UTF16 units"
        );
    }
    let map = Js::new_map();
    let object_key = Js::object(vec![(
        "toString",
        Js::function("toString", |_, _| {
            Err(Thrown::type_error("Map key must not stringify"))
        }),
    )]);
    let key_reads = Rc::new(std::cell::Cell::new(0));
    let key_error = Js::object(vec![("safeCexSentryReportCooldownMs", Js::Number(100.0))]);
    let reads = key_reads.clone();
    let key = object_key.clone();
    key_error
        .define_accessor(
            "safeCexSentryReportKey",
            Some(Rc::new(move |_| {
                reads.set(reads.get() + 1);
                Ok(if reads.get() % 3 == 0 {
                    key.clone()
                } else {
                    Js::str("validated")
                })
            })),
            None,
        )
        .unwrap();
    let sends = RefCell::new(Vec::new());
    for _ in 0..2 {
        forward_error(
            &map,
            (safe_cex_types::ExchangeName::Binance, "account"),
            &Js::str("changing-key"),
            &key_error,
            None,
            true,
            (
                || Ok::<_, ExchangeError>(0.0),
                || Ok(Js::object(vec![])),
                |p| {
                    sends
                        .borrow_mut()
                        .push(p.data.get("1")?.get("sendToSentry")?.truthy());
                    Ok(())
                },
                |_, _, _, e| Err(e),
            ),
        )
        .unwrap();
    }
    assert_eq!(
        json!({"reads":key_reads.get(),"size":map.get("size").unwrap().to_number().unwrap() as usize,"identity":map.call_method("has",vec![object_key]).unwrap().truthy(),"sends":*sends.borrow()}),
        source["changingKey"],
        "WORKER-ERROR source native Map key identity without String hop"
    );
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 72);
    for row in rows {
        assert_numeric_tree_eq(
            &native(row)["calls"],
            &row["calls"],
            "WORKER-ERROR sequence",
        );
    }
    let clocks = source["clockBits"].as_array().unwrap();
    assert_eq!(clocks.len(), 6);
    for probe in clocks {
        let token = probe["token"].as_str().unwrap();
        let now = safe_cex_num::number_from_string(token);
        let cooldowns = Js::new_map();
        let error = from_json(
            &json!({"safeCexSentryReportKey":"same","safeCexSentryReportCooldownMs":100}),
        );
        forward_error(
            &cooldowns,
            (safe_cex_types::ExchangeName::Binance, "account"),
            &Js::str("bits"),
            &error,
            None,
            true,
            (
                || Ok::<_, ExchangeError>(now),
                || Ok(Js::object(vec![])),
                |_| Ok(()),
                |_, _, _, e| Err(e),
            ),
        )
        .unwrap();
        assert_eq!(
            format!(
                "{:016x}",
                cooldowns
                    .call_method("get", vec![Js::str("same")])
                    .unwrap()
                    .get("lastReportedAt")
                    .unwrap()
                    .to_number()
                    .unwrap()
                    .to_bits()
            ),
            probe["last"].as_str().unwrap(),
            "WORKER-ERROR native clock bits"
        );
    }
    let tags = Js::object(vec![("phase", Js::str("before"))]);
    let error = Js::object(vec![("safeCexSentryTags", tags.clone())]);
    let sent = RefCell::new(None);
    forward_error(
        &Js::new_map(),
        (safe_cex_types::ExchangeName::Binance, "account"),
        &Js::str("alias"),
        &error,
        None,
        true,
        (
            || Ok::<_, ExchangeError>(0.0),
            || {
                tags.set("phase", Js::str("after"))?;
                Ok(from_json(
                    &json!({"message":"alias-error","name":"UnknownError","stack":"host-stack"}),
                ))
            },
            |p| {
                *sent.borrow_mut() = Some(p.data);
                Ok(())
            },
            |_, _, _, e| Err(e),
        ),
    )
    .unwrap();
    assert_eq!(
        source["alias"],
        json!({"caller":"after","reported":"after"})
    );
    assert_eq!(
        tags.get("phase").unwrap().to_js_string().unwrap().as_ref(),
        "after".encode_utf16().collect::<Vec<_>>().as_slice()
    );
    assert_eq!(
        sent.borrow()
            .as_ref()
            .unwrap()
            .get("1")
            .unwrap()
            .get("sentryData")
            .unwrap()
            .get("tags")
            .unwrap()
            .get("phase")
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        "after".encode_utf16().collect::<Vec<_>>().as_slice(),
        "WORKER-ERROR source live control tags after coercion"
    );
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
    let cooldowns = Js::new_map();
    let key = Js::str("same");
    let error =
        from_json(&json!({"safeCexSentryReportKey":"same","safeCexSentryReportCooldownMs":100}));
    let events = RefCell::new(Vec::new());
    let failure = Rc::new(ExchangeError::new("materialize"));
    let result = forward_error(
        &cooldowns,
        (safe_cex_types::ExchangeName::Binance, "account"),
        &Js::str("message"),
        &error,
        None,
        true,
        (
            || {
                cooldowns
                    .call_method(
                        "set",
                        vec![
                            Js::str("other"),
                            Js::object(vec![
                                ("lastReportedAt", Js::Number(1.0)),
                                ("suppressedCount", Js::Number(2.0)),
                            ]),
                        ],
                    )
                    .unwrap();
                events.borrow_mut().push("clock");
                Ok(-0.0)
            },
            || {
                assert_eq!(
                    cooldowns
                        .call_method("get", vec![key.clone()])
                        .unwrap()
                        .get("lastReportedAt")
                        .unwrap()
                        .to_number()
                        .unwrap()
                        .to_bits(),
                    (-0.0_f64).to_bits()
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
    assert!(Rc::ptr_eq(&result.unwrap_err(), &failure));
    assert_eq!(*events.borrow(), vec!["clock", "materialize"]);
}
