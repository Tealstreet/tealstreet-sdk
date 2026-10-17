use super::*;

fn keys(value: &Js) -> Vec<String> {
    value
        .own_keys()
        .iter()
        .map(|key| String::from_utf16_lossy(key))
        .collect()
}

fn observe_errors(assembled: &Assembled) -> Rc<RefCell<Vec<Vec<Js>>>> {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let sink = observed.clone();
    assembled.base.on_args(
        "error",
        Rc::new(move |_, values| {
            sink.borrow_mut().push(values.to_vec());
            Ok(())
        }),
    );
    observed
}

#[test]
fn raw_reporter_retains_native_message_and_original_error_without_reading_code() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    assembled.runtime.worker.set(true);
    let observed = observe_errors(&assembled);
    let message = Js::object(vec![
        ("native", Js::Number(f64::INFINITY)),
        ("missing", Js::Undefined),
    ]);
    let raw = Js::object(vec![("message", message.clone())]);
    raw.define_accessor(
        "code",
        Some(Rc::new(|_| {
            Err(Thrown::type_error("unexpected eager code read"))
        })),
        None,
    );
    assembled
        .this
        .call_method(
            "emitAndLogError",
            vec![
                raw.clone(),
                Js::Undefined,
                Js::object(vec![("suppressConsoleLog", Js::Bool(true))]),
            ],
        )
        .unwrap();
    let events = observed.borrow();
    assert!(
        events[0][0].same_reference(&message),
        "RAW-REPORT native message identity"
    );
    let metadata = &events[0][1];
    assert_eq!(
        keys(metadata),
        [
            "sendToSentry",
            "sentryData",
            "enhancedContext",
            "suppressConsoleLog"
        ]
    );
    assert!(metadata.has_own("enhancedContext"));
    assert!(metadata.get("enhancedContext").unwrap().is_undefined());
    let data = metadata.get("sentryData").unwrap();
    assert_eq!(keys(&data), ["error", "tags", "extra"]);
    assert!(data.get("error").unwrap().same_reference(&raw));
    let extra = data.get("extra").unwrap();
    assert!(
        extra
            .get("originalMessage")
            .unwrap()
            .same_reference(&message)
    );
    let options = extra.get("exchangeOptions").unwrap();
    assert_eq!(keys(&options), ["testnet", "publicDataOnly"]);
    assert!(options.get("testnet").unwrap().is_undefined());
    assert!(options.get("publicDataOnly").unwrap().is_undefined());
}

#[test]
fn raw_reporter_retains_utf16_and_coerces_after_both_console_calls() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    assembled.runtime.worker.set(true);
    let observed = observe_errors(&assembled);
    let raw = Js::String(Rc::from([0xd800, 0x0061]));
    assembled
        .this
        .call_method("emitAndLogError", vec![raw])
        .unwrap();
    let mut expected: Vec<u16> = "Exchange Error: ".encode_utf16().collect();
    expected.extend([0xd800, 0x0061]);
    assert_eq!(observed.borrow()[0][0].as_str().unwrap().as_ref(), expected);
    assembled.runtime.consoled.borrow_mut().clear();
    let runtime = assembled.runtime.clone();
    let source = Js::object(vec![(
        "toString",
        Js::function("toString", move |_, _| {
            assert_eq!(
                runtime
                    .consoled
                    .borrow()
                    .iter()
                    .map(|(severity, _)| severity.as_str())
                    .collect::<Vec<_>>(),
                ["error", "trace"],
                "RAW-REPORT console before source coercion"
            );
            Ok(Js::str("source object"))
        }),
    )]);
    assembled
        .this
        .call_method("emitAndLogError", vec![source])
        .unwrap();
    assert!(observed.borrow()[1][0].strict_equals(&Js::str("source object")));
}

#[test]
fn raw_reporter_main_metadata_retains_own_undefined_without_worker_fields() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    let observed = observe_errors(&assembled);
    assembled
        .this
        .call_method("emitAndLogError", vec![Js::str("plain")])
        .unwrap();
    let events = observed.borrow();
    let metadata = &events[0][1];
    assert_eq!(keys(metadata), ["enhancedContext", "suppressConsoleLog"]);
    assert!(metadata.has_own("enhancedContext"));
    assert!(metadata.get("enhancedContext").unwrap().is_undefined());
    assert!(
        metadata
            .get("suppressConsoleLog")
            .unwrap()
            .strict_equals(&Js::Bool(false))
    );
    assert_eq!(assembled.runtime.exceptions.borrow().len(), 1);
}

#[test]
fn raw_reporter_preserves_native_error_before_any_metadata_projection() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    assembled.runtime.worker.set(true);
    let observed = observe_errors(&assembled);
    let error = Thrown::type_error("native subtype").value;
    error.define_accessor(
        "code",
        Some(Rc::new(|_| {
            Err(Thrown::type_error("unexpected code getter"))
        })),
        None,
    );
    assembled
        .this
        .call_method("emitAndLogError", vec![error.clone()])
        .unwrap();
    let events = observed.borrow();
    assert!(events[0][0].strict_equals(&Js::str("native subtype")));
    let reported = events[0][1]
        .get("sentryData")
        .unwrap()
        .get("error")
        .unwrap();
    assert!(reported.same_reference(&error));
    assert!(
        reported
            .get("name")
            .unwrap()
            .strict_equals(&Js::str("TypeError"))
    );
}

#[test]
fn raw_reporter_uses_source_falsy_and_string_constructor_rules() {
    for (input, expected) in [
        (Js::Undefined, "Unknown Exchange Error"),
        (Js::Null, "Unknown Exchange Error"),
        (Js::Bool(false), "Unknown Exchange Error"),
        (Js::Number(-0.0), "Unknown Exchange Error"),
        (Js::Number(f64::NAN), "Unknown Exchange Error"),
        (Js::str(""), "Unknown Exchange Error"),
        (Js::Bool(true), "true"),
        (Js::Number(17.0), "17"),
        (Js::Number(f64::INFINITY), "Infinity"),
        (Js::Number(f64::NEG_INFINITY), "-Infinity"),
        (Js::str("plain"), "Exchange Error: plain"),
        (Js::SymbolIterator, "Symbol(Symbol.iterator)"),
    ] {
        let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
        assembled.runtime.worker.set(true);
        let observed = observe_errors(&assembled);
        assembled
            .this
            .call_method("emitAndLogError", vec![input])
            .unwrap();
        assert!(
            observed.borrow()[0][0].strict_equals(&Js::str(expected)),
            "RAW-REPORT source primitive formatting: {expected}"
        );
    }
}
#[test]
fn raw_reporter_enhanced_fields_keep_native_context_and_formatter_references() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    assembled.runtime.worker.set(true);
    let observed = observe_errors(&assembled);
    let code = Js::object(vec![
        ("native", Js::Number(f64::INFINITY)),
        ("missing", Js::Undefined),
    ]);
    let context = Js::object(vec![("errorCode", code.clone())]);
    let original_message = Js::object(vec![("zero", Js::Number(-0.0))]);
    let enhanced = Js::utf16(&[0xd800, 0x0061]);
    let raw = Js::object(vec![
        ("context", context.clone()),
        ("enhancedMessage", enhanced.clone()),
        (
            "originalError",
            Js::object(vec![("message", original_message.clone())]),
        ),
    ]);
    assembled
        .this
        .call_method(
            "emitAndLogError",
            vec![
                raw.clone(),
                Js::Undefined,
                Js::object(vec![("suppressConsoleLog", Js::Bool(true))]),
            ],
        )
        .unwrap();
    let events = observed.borrow();
    assert!(events[0][0].strict_equals(&enhanced));
    let metadata = &events[0][1];
    assert!(
        metadata
            .get("enhancedContext")
            .unwrap()
            .same_reference(&context)
    );
    let sentry = metadata.get("sentryData").unwrap();
    assert!(sentry.get("error").unwrap().same_reference(&raw));
    assert!(
        sentry
            .get("tags")
            .unwrap()
            .get("errorCode")
            .unwrap()
            .same_reference(&code)
    );
    assert!(
        sentry
            .get("extra")
            .unwrap()
            .get("originalMessage")
            .unwrap()
            .same_reference(&original_message)
    );
}

#[test]
fn raw_reporter_accessor_reads_follow_actual_source_order() {
    let assembled = assemble(json!({"key":"k", "secret":"s"})).unwrap();
    assembled.runtime.worker.set(true);
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let observed = |label: &'static str, values: Vec<(&str, Js)>| {
        let object = Js::object(vec![]);
        for (key, value) in values {
            let label = format!("{label}.{key}");
            let trace = trace.clone();
            object.define_accessor(
                key,
                Some(Rc::new(move |_| {
                    trace.borrow_mut().push(label.clone());
                    Ok(value.clone())
                })),
                None,
            );
        }
        object
    };
    let config = observed(
        "config",
        vec![
            ("baseURL", Js::str("/native")),
            ("timeout", Js::Number(20.0)),
            (
                "headers",
                Js::object(vec![("Authorization", Js::str("fixture"))]),
            ),
        ],
    );
    let context = observed(
        "context",
        vec![
            ("isNetworkError", Js::Bool(false)),
            ("errorCode", Js::Number(0.0)),
            ("statusCode", Js::Number(401.0)),
            ("isTimeout", Js::Bool(false)),
            ("isCORS", Js::Bool(false)),
            ("url", Js::str("/native")),
            ("method", Js::str("GET")),
            ("statusText", Js::str("Unauthorized")),
            ("timestamp", Js::str("now")),
        ],
    );
    let original = observed("original", vec![("message", Js::str("original"))]);
    let error = observed(
        "error",
        vec![
            ("context", context),
            ("enhancedMessage", Js::str("enhanced")),
            ("config", config),
            ("originalError", original),
            ("code", Js::str("CODE")),
        ],
    );
    assembled
        .this
        .call_method(
            "emitAndLogError",
            vec![
                error,
                Js::Undefined,
                Js::object(vec![("suppressConsoleLog", Js::Bool(true))]),
            ],
        )
        .unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = std::env::var_os("SAFE_CEX_TYPESCRIPT_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("../target/asterdex-oracle-deps/node_modules/typescript"));
    let output = std::process::Command::new("node")
        .arg(root.join("tests/raw-error-source.cjs"))
        .arg("--trace")
        .env("SAFE_CEX_TYPESCRIPT_PATH", compiler)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source getter oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let profiles: Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = profiles
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile["worker"] == true)
        .unwrap();
    let expected: Vec<String> = serde_json::from_value(source["reads"].clone()).unwrap();
    assert_eq!(
        *trace.borrow(),
        expected,
        "RAW-REPORT actual source getter order"
    );
}
