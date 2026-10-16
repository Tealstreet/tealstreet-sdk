use safe_cex_venue_woo::{
    runtime_selector::WooV3MigrationRouteId as Route,
    shadow_compare::{self as port, ShadowOps},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type V = Rc<Native>;
type Callback = Rc<dyn Fn(V, Vec<V>) -> Result<V, V>>;
enum Native {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Object(RefCell<Vec<(String, V)>>),
    Error(Vec<u16>),
    Function(Callback),
    Builtin(&'static str),
}
impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native shadow value")
    }
}
fn undefined() -> V {
    Rc::new(Native::Undefined)
}
fn string(s: &str) -> V {
    Rc::new(Native::String(s.encode_utf16().collect()))
}
fn object(fields: Vec<(&str, V)>) -> V {
    Rc::new(Native::Object(RefCell::new(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn set(v: &V, key: &str, value: V) {
    let Native::Object(fields) = v.as_ref() else {
        panic!("object")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        fields.push((key.into(), value));
    }
}
fn get(v: &V, key: &str) -> Result<V, V> {
    match v.as_ref() {
        Native::Object(fields) => Ok(fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undefined)),
        Native::Builtin("hasOwnProperty") if key == "call" => Ok(Rc::new(Native::Builtin("call"))),
        Native::Undefined | Native::Null => Err(Rc::new(Native::Error(
            "property access".encode_utf16().collect(),
        ))),
        _ => Ok(undefined()),
    }
}
fn units(v: &V) -> Vec<u16> {
    let Native::String(s) = v.as_ref() else {
        panic!("SHADOW-NATIVE string")
    };
    s.clone()
}
fn text(v: &V) -> String {
    String::from_utf16(&units(v)).unwrap()
}
fn keys(v: &V) -> Vec<String> {
    let Native::Object(f) = v.as_ref() else {
        panic!("object")
    };
    f.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn kind(v: &V) -> &'static str {
    match v.as_ref() {
        Native::Undefined => "undefined",
        Native::Bool(_) => "boolean",
        Native::Number(_) => "number",
        Native::String(_) => "string",
        Native::Function(_) | Native::Builtin(_) => "function",
        _ => "object",
    }
}
struct Host {
    events: RefCell<Vec<String>>,
}
impl Host {
    fn call(&self, function: V, receiver: V, args: Vec<V>) -> Result<V, V> {
        match function.as_ref() {
            Native::Function(body) => body(receiver, args),
            Native::Builtin("freeze") => {
                self.events.borrow_mut().push("freeze".into());
                Ok(args[0].clone())
            }
            Native::Builtin("call") => {
                assert!(matches!(
                    receiver.as_ref(),
                    Native::Builtin("hasOwnProperty")
                ));
                let own = match args[0].as_ref() {
                    Native::Object(fields) => {
                        fields.borrow().iter().any(|(k, _)| k == &text(&args[1]))
                    }
                    _ => false,
                };
                Ok(Rc::new(Native::Bool(own)))
            }
            Native::Builtin("String") => {
                let value = &args[0];
                let encoded = match value.as_ref() {
                    Native::String(s) => return Ok(Rc::new(Native::String(s.clone()))),
                    Native::Bool(b) => b.to_string(),
                    Native::Number(n) => safe_cex_num::js_number_string(*n),
                    Native::Undefined => "undefined".into(),
                    Native::Null => "null".into(),
                    Native::Object(_) => "[object Object]".into(),
                    _ => panic!("out-of-floor String"),
                };
                Ok(string(&encoded))
            }
            _ => panic!("out-of-floor callable"),
        }
    }
}
fn host() -> (ShadowOps<V>, Rc<Host>) {
    let host = Rc::new(Host {
        events: RefCell::new(vec![]),
    });
    let calls = host.clone();
    let properties = host.clone();
    let globals = host.clone();
    let ops = ShadowOps {
        get: Rc::new(move |v, key| {
            properties.events.borrow_mut().push(format!("get:{key}"));
            get(v, key)
        }),
        global: Rc::new(move |name| {
            globals.events.borrow_mut().push(format!("global:{name}"));
            Ok(match name {
                "Object" => object(vec![
                    (
                        "prototype",
                        object(vec![(
                            "hasOwnProperty",
                            Rc::new(Native::Builtin("hasOwnProperty")),
                        )]),
                    ),
                    ("freeze", Rc::new(Native::Builtin("freeze"))),
                ]),
                "Error" => Rc::new(Native::Builtin("Error")),
                "String" => Rc::new(Native::Builtin("String")),
                _ => panic!("global {name}"),
            })
        }),
        call: Rc::new(move |f, r, a| calls.call(f, r, a)),
        construct: Rc::new(|f, a| {
            assert!(matches!(f.as_ref(), Native::Builtin("Error")));
            Ok(Rc::new(Native::Error(units(&a[0]))))
        }),
        object: Rc::new(object),
        string: Rc::new(string),
        undefined: Rc::new(undefined),
        kind: Rc::new(kind),
        null: Rc::new(|v| matches!(v.as_ref(), Native::Null)),
        truthy: Rc::new(|v| match v.as_ref() {
            Native::Undefined | Native::Null => false,
            Native::Bool(b) => *b,
            Native::Number(n) => *n != 0. && !n.is_nan(),
            Native::String(s) => !s.is_empty(),
            _ => true,
        }),
        strict_equal: Rc::new(|a, b| match (a.as_ref(), b.as_ref()) {
            (Native::String(a), Native::String(b)) => a == b,
            (Native::Number(a), Native::Number(b)) => a == b,
            (Native::Bool(a), Native::Bool(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }),
        instance_of: Rc::new(|value, constructor| {
            assert!(matches!(constructor.as_ref(), Native::Builtin("Error")));
            Ok(matches!(value.as_ref(), Native::Error(_)))
        }),
        template_append: Rc::new(|a, b| {
            let mut out = units(&a);
            out.extend(units(&b));
            Ok(Rc::new(Native::String(out)))
        }),
    };
    (ops, host)
}
fn source(modes: &[&str]) -> Value {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/shadow-compare-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(modes).unwrap().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "source shadow witness: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
#[derive(Clone)]
struct ErrorReference(V);
impl port::ShadowErrorReference for ErrorReference {
    type Value = V;
    type Failure = V;
    fn native(&self) -> &V {
        &self.0
    }
    fn name(&self) -> Result<Vec<u16>, V> {
        Ok(units(&get(&self.0, "name")?))
    }
    fn message(&self) -> Result<Vec<u16>, V> {
        Ok(units(&get(&self.0, "message")?))
    }
    fn stack(&self) -> Result<Option<Vec<u16>>, V> {
        let value = get(&self.0, "stack")?;
        Ok(if matches!(value.as_ref(), Native::Undefined) {
            None
        } else {
            Some(units(&value))
        })
    }
    fn cause(&self) -> Result<Option<V>, V> {
        let Native::Object(fields) = self.0.as_ref() else {
            panic!("structural error")
        };
        Ok(fields
            .borrow()
            .iter()
            .find(|(k, _)| k == "cause")
            .map(|(_, v)| v.clone()))
    }
}
#[test]
fn immutable_native_declaration_fields_and_code_units() {
    let observed = Rc::new(17);
    let value = port::WooV3ObservedCanonical::new(vec![0xd800, 65], observed.clone());
    assert_eq!(
        value.canonical(),
        [0xd800, 65],
        "SHADOW-NATIVE observed canonical UTF16"
    );
    assert!(Rc::ptr_eq(value.observed(), &observed));
    let legacy = Rc::new(19);
    let success = port::WooV3ComparedRead::new(
        true,
        Route::Markets,
        legacy.clone(),
        vec![66, 0xdfff],
        observed.clone(),
    );
    assert_eq!(success.status(), "match");
    assert_eq!(success.route_id(), Route::Markets);
    assert!(Rc::ptr_eq(success.authoritative(), &legacy));
    assert_eq!(
        success.observed_canonical(),
        [66, 0xdfff],
        "SHADOW-NATIVE comparison UTF16"
    );
    assert!(Rc::ptr_eq(success.observed(), &observed));
    assert_eq!(success.store_action(), "retain-legacy");
    let mismatch = port::WooV3ComparedRead::new(
        false,
        Route::Markets,
        legacy.clone(),
        vec![65],
        observed.clone(),
    );
    assert_eq!(
        mismatch.status(),
        "mismatch",
        "SHADOW-NATIVE mismatch carrier status"
    );
    use port::ShadowErrorReference;
    let error = ErrorReference(object(vec![
        ("name", Rc::new(Native::String(vec![0xd800, 67]))),
        ("message", Rc::new(Native::String(vec![68, 0xdfff]))),
        ("stack", Rc::new(Native::String(vec![69, 0xd801]))),
    ]));
    let native_error = error.native().clone();
    assert_eq!(
        error.name().unwrap(),
        [0xd800, 67],
        "SHADOW-NATIVE Error name"
    );
    assert_eq!(
        error.message().unwrap(),
        [68, 0xdfff],
        "SHADOW-NATIVE Error message"
    );
    assert_eq!(
        error.stack().unwrap(),
        Some(vec![69, 0xd801]),
        "SHADOW-NATIVE Error stack"
    );
    assert!(error.cause().unwrap().is_none());
    let failure = port::WooV3FailedRead::new(Route::Tickers, legacy.clone(), error);
    assert_eq!(failure.status(), "decode-or-map-failed");
    assert_eq!(failure.route_id(), Route::Tickers);
    assert!(Rc::ptr_eq(failure.authoritative(), &legacy));
    assert!(
        Rc::ptr_eq(failure.error().native(), &native_error),
        "SHADOW-NATIVE structural Error identity"
    );
    set(
        &native_error,
        "name",
        Rc::new(Native::String(vec![71, 0xd802])),
    );
    let cause = Rc::new(Native::Number(41.));
    set(&native_error, "cause", cause.clone());
    assert!(
        Rc::ptr_eq(&failure.error().cause().unwrap().unwrap(), &cause),
        "SHADOW-NATIVE Error unknown cause identity"
    );
    set(&native_error, "stack", undefined());
    assert!(
        failure.error().stack().unwrap().is_none(),
        "SHADOW-NATIVE optional stack"
    );
    assert_eq!(
        failure.error().name().unwrap(),
        [71, 0xd802],
        "SHADOW-NATIVE live Error alias"
    );
    assert_eq!(failure.store_action(), "retain-legacy");
    let compared: port::WooV3ShadowReadComparison<_, _, ErrorReference> =
        port::WooV3ShadowReadComparison::Compared(success);
    let port::WooV3ShadowReadComparison::Compared(v) = compared else {
        panic!("compared")
    };
    assert_eq!(v.status(), "match");
    let failed: port::WooV3ShadowReadComparison<_, Rc<i32>, _> =
        port::WooV3ShadowReadComparison::Failed(failure);
    let port::WooV3ShadowReadComparison::Failed(v) = failed else {
        panic!("failed")
    };
    assert_eq!(v.status(), "decode-or-map-failed");
}
#[test]
fn normalize_error_preserves_native_error_and_stringifies_unknown() {
    let (ops, _) = host();
    let original = Rc::new(Native::Error(vec![0xd800, 65]));
    let retained = port::normalize_error(&ops, original.clone()).unwrap();
    assert!(
        Rc::ptr_eq(&retained, &original),
        "SHADOW-NATIVE original Error identity"
    );
    let normalized = port::normalize_error(&ops, Rc::new(Native::Bool(false))).unwrap();
    let Native::Error(message) = normalized.as_ref() else {
        panic!("Error")
    };
    assert_eq!(
        message,
        &"Unknown WOO V3 shadow error: false"
            .encode_utf16()
            .collect::<Vec<_>>(),
        "SHADOW-NATIVE normalized message"
    );
    let expected = source(&["normalize-error", "normalize-bool"]);
    assert_eq!(expected[0]["identity"], true);
    assert_eq!(expected[0]["units"], json!([0xd800, 65]));
    assert_eq!(expected[1]["units"], json!(message));
}
#[test]
fn shadow_comparison_native_values_precede_source_observations() {
    let modes = [
        "match",
        "mismatch",
        "undefined-observed",
        "null",
        "missing-canonical",
        "missing-observed",
        "number-canonical",
        "function",
        "throw-error",
        "throw-bool",
        "mutate-throw",
        "surrogate",
    ];
    let invalid = "WOO V3 shadow decoder must return a canonical string and its observed value";
    let mut native = vec![];
    for mode in modes {
        let (ops, h) = host();
        let snapshot = object(vec![("legacy", Rc::new(Native::Number(17.)))]);
        let observed = object(vec![("v3", Rc::new(Native::Number(19.)))]);
        let response = object(vec![("response", Rc::new(Native::Number(23.)))]);
        let original = Rc::new(Native::Error("original error".encode_utf16().collect()));
        let canonical = if mode == "surrogate" {
            Rc::new(Native::String(vec![0xd800, 65]))
        } else {
            string("same")
        };
        let selection = object(vec![("routeId", string("markets"))]);
        let input = object(vec![
            ("selection", selection.clone()),
            ("legacySnapshot", snapshot.clone()),
            ("legacyCanonical", canonical.clone()),
            ("v3Response", response.clone()),
        ]);
        let calls = Rc::new(RefCell::new(0));
        let count = calls.clone();
        let expected_input = input.clone();
        let expected_response = response.clone();
        let error = original.clone();
        let visible = observed.clone();
        let decoder = Rc::new(Native::Function(Rc::new(move |receiver, args| {
            assert!(
                Rc::ptr_eq(&receiver, &expected_input),
                "SHADOW-NATIVE decoder this"
            );
            assert_eq!(args.len(), 1, "SHADOW-NATIVE decoder argument floor");
            assert!(
                Rc::ptr_eq(&args[0], &expected_response),
                "SHADOW-NATIVE original response"
            );
            *count.borrow_mut() += 1;
            match mode {
                "null" => Ok(Rc::new(Native::Null)),
                "missing-canonical" => Ok(object(vec![("observed", visible.clone())])),
                "missing-observed" => Ok(object(vec![("canonical", canonical.clone())])),
                "number-canonical" => Ok(object(vec![
                    ("canonical", Rc::new(Native::Number(29.))),
                    ("observed", visible.clone()),
                ])),
                "function" => Ok(Rc::new(Native::Function(Rc::new(|_, _| Ok(undefined()))))),
                "throw-error" => Err(error.clone()),
                "throw-bool" => Err(Rc::new(Native::Bool(false))),
                "mutate-throw" => {
                    set(&selection, "routeId", string("tickers"));
                    Err(error.clone())
                }
                _ => Ok(object(vec![
                    (
                        "canonical",
                        if mode == "mismatch" {
                            string("different")
                        } else {
                            canonical.clone()
                        },
                    ),
                    (
                        "observed",
                        if mode == "undefined-observed" {
                            undefined()
                        } else {
                            visible.clone()
                        },
                    ),
                ])),
            }
        })));
        set(&input, "decodeAndCanonicalizeV3", decoder);
        let output = port::compare_shadow_read(&ops, &input).unwrap();
        let success = matches!(
            mode,
            "match" | "mismatch" | "undefined-observed" | "surrogate"
        );
        let expected_status = if success {
            if mode == "mismatch" {
                "mismatch"
            } else {
                "match"
            }
        } else {
            "decode-or-map-failed"
        };
        assert_eq!(
            text(&get(&output, "status").unwrap()),
            expected_status,
            "SHADOW-NATIVE status {mode}"
        );
        assert_eq!(
            text(&get(&output, "routeId").unwrap()),
            if mode == "mutate-throw" {
                "tickers"
            } else {
                "markets"
            },
            "SHADOW-NATIVE route read timing"
        );
        assert!(
            Rc::ptr_eq(&get(&output, "authoritative").unwrap(), &snapshot),
            "SHADOW-NATIVE authoritative identity"
        );
        assert_eq!(text(&get(&output, "storeAction").unwrap()), "retain-legacy");
        assert_eq!(*calls.borrow(), 1, "SHADOW-NATIVE decoder calls");
        let trace = h.events.borrow();
        let freeze = trace.iter().position(|v| v == "get:freeze").unwrap();
        let legacy = trace
            .iter()
            .position(|v| v == "get:legacySnapshot")
            .unwrap();
        assert!(
            freeze < legacy,
            "SHADOW-NATIVE freeze callee before authoritative getter"
        );
        assert_eq!(
            trace.last().unwrap(),
            "freeze",
            "SHADOW-NATIVE freeze invoked"
        );
        drop(trace);
        if success {
            assert_eq!(
                keys(&output),
                [
                    "status",
                    "routeId",
                    "authoritative",
                    "observedCanonical",
                    "observed",
                    "storeAction"
                ],
                "SHADOW-NATIVE success ordered keys"
            );
            let value = get(&output, "observedCanonical").unwrap();
            assert_eq!(
                units(&value),
                if mode == "surrogate" {
                    vec![0xd800, 65]
                } else {
                    if mode == "mismatch" {
                        "different"
                    } else {
                        "same"
                    }
                    .encode_utf16()
                    .collect()
                },
                "SHADOW-NATIVE canonical value"
            );
            let value = get(&output, "observed").unwrap();
            if mode == "undefined-observed" {
                assert!(
                    matches!(value.as_ref(), Native::Undefined),
                    "SHADOW-NATIVE observed undefined"
                );
            } else {
                assert!(
                    Rc::ptr_eq(&value, &observed),
                    "SHADOW-NATIVE observed identity"
                );
            }
        } else {
            assert_eq!(
                keys(&output),
                ["status", "routeId", "authoritative", "error", "storeAction"],
                "SHADOW-NATIVE failure ordered keys"
            );
            let value = get(&output, "error").unwrap();
            let Native::Error(message) = value.as_ref() else {
                panic!("SHADOW-NATIVE Error")
            };
            let expected = match mode {
                "throw-error" | "mutate-throw" => "original error",
                "throw-bool" => "Unknown WOO V3 shadow error: false",
                _ => invalid,
            };
            assert_eq!(
                message,
                &expected.encode_utf16().collect::<Vec<_>>(),
                "SHADOW-NATIVE error value {mode}"
            );
            if mode == "throw-error" || mode == "mutate-throw" {
                assert!(
                    Rc::ptr_eq(&value, &original),
                    "SHADOW-NATIVE caught Error identity"
                );
            }
        }
        native.push((mode, output));
    }
    assert_eq!(native.len(), 12, "SHADOW-NATIVE comparison fixture floor");
    let rows = source(&modes);
    for ((mode, value), expected) in native.iter().zip(rows.as_array().unwrap()) {
        assert_eq!(
            text(&get(value, "status").unwrap()),
            expected["status"],
            "source status {mode}"
        );
        assert_eq!(text(&get(value, "routeId").unwrap()), expected["routeId"]);
        assert_eq!(
            keys(value),
            serde_json::from_value::<Vec<String>>(expected["keys"].clone()).unwrap()
        );
        assert_eq!(expected["authoritativeIdentity"], true);
        assert_eq!(expected["frozen"], true);
        assert_eq!(expected["calls"], 1);
        if matches!(
            *mode,
            "match" | "mismatch" | "undefined-observed" | "surrogate"
        ) {
            assert_eq!(
                json!(units(&get(value, "observedCanonical").unwrap())),
                expected["canonicalUnits"]
            );
            assert_eq!(expected["observedIdentity"], *mode != "undefined-observed");
        } else {
            let error = get(value, "error").unwrap();
            let Native::Error(message) = error.as_ref() else {
                panic!("Error")
            };
            assert_eq!(json!(message), expected["errorUnits"]);
        }
    }
}
#[test]
fn declaration_rejects_integer_error_but_accepts_structural_error() {
    let directory = std::env::temp_dir().join(format!("woo-shadow-shape-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/shadow-error-shape.rs"
    );
    let invoke = |positive| {
        let mut command = Command::new(&compiler);
        command
            .args([
                "--edition=2021",
                "--emit=metadata",
                "--crate-name=woo_shadow_shape",
            ])
            .arg(fixture)
            .arg("--out-dir")
            .arg(&directory)
            .env(
                "WOO_SOURCE_DIR",
                concat!(env!("CARGO_MANIFEST_DIR"), "/src"),
            );
        if positive {
            command.args(["--cfg", "positive"]);
        }
        command
            .output()
            .expect("installed Rust compiler for type constraint witness")
    };
    let positive = invoke(true);
    assert!(
        positive.status.success(),
        "SHADOW-NATIVE structural Error acceptance: {}",
        String::from_utf8_lossy(&positive.stderr)
    );
    let negative = invoke(false);
    assert!(
        !negative.status.success(),
        "SHADOW-NATIVE integer Error rejection"
    );
    let diagnostic = String::from_utf8_lossy(&negative.stderr);
    assert!(
        diagnostic.contains("E0277") && diagnostic.contains("ShadowErrorReference"),
        "SHADOW-NATIVE named Error constraint diagnostic: {diagnostic}"
    );
    std::fs::remove_dir_all(&directory).unwrap();
}
