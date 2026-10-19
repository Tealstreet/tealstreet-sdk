#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        scope::ScopeShared,
        value::{Js, from_json, to_json},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{process::Command, rc::Rc, sync::Arc};
fn installed() -> Rc<ScopeShared<()>> {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    library::install_library(&shared.globals, Rc::downgrade(&shared));
    shared
}
fn input(v: &Value) -> Js {
    if let Some(s) = v.get("special").and_then(Value::as_str) {
        return match s {
            "undefined" => Js::Undefined,
            "nan" => Js::Number(f64::NAN),
            "infinity" => Js::Number(f64::INFINITY),
            "negativeInfinity" => Js::Number(f64::NEG_INFINITY),
            "negativeZero" => Js::Number(-0.),
            "bigint" => Js::BigInt(Rc::new(123.into())),
            "surrogate" => Js::utf16(&[0xd800]),
            _ => panic!("unknown input"),
        };
    }
    if let Some(o) = v.as_object() {
        return Js::object(o.iter().map(|(k, v)| (k.as_str(), input(v))).collect());
    }
    from_json(v)
}
fn compare(name: &str) {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_ccxt_imports_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    let shared = installed();
    let mut count = 0;
    for case in cases.iter().filter(|c| c["name"] == name) {
        let args = case["args"].as_array().unwrap().iter().map(input).collect();
        let result = shared.globals.call_method(name, args);
        let actual = match result {
            Ok(Js::Undefined) => json!({"kind":"undefined"}),
            Ok(Js::String(s)) => json!({"kind":"return","value":s.as_ref()}),
            Ok(v) => json!({"kind":"return","value":to_json(&v).unwrap()}),
            Err(e) => {
                json!({"kind":"throw","name":e.value.get("name").unwrap().to_display(),"message":e.value.get("message").unwrap().to_js_string().unwrap().as_ref()})
            }
        };
        assert_eq!(
            actual, case["expected"],
            "CCXT_IMPORT {name} actual maintained-source case {}",
            case["args"]
        );
        count += 1;
    }
    assert!(count >= 10, "independent input floor");
}
#[test]
fn installed_number_to_string_matches_maintained_source() {
    compare("numberToString");
}
#[test]
fn installed_parse_precision_matches_maintained_source() {
    compare("parsePrecision");
}
#[test]
fn installed_decimal_to_precision_matches_maintained_source() {
    compare("decimalToPrecision");
}
#[test]
fn installed_safe_string_matches_maintained_source() {
    compare("safeString");
}

#[test]
fn installed_ccxt_imports_preserve_repeated_reads_original_values_and_coercion_order() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    use std::cell::{Cell, RefCell};
    let out = Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_ccxt_imports_source.cjs"
            ),
            "effects",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
    let shared = installed();
    let g = &shared.globals;
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let token = Js::object(vec![("token", Js::Bool(true))]);
    let value = Js::object(vec![]);
    let observed = log.clone();
    let weak_value = match &value {
        Js::Object(o) => Rc::downgrade(o),
        _ => unreachable!(),
    };
    value
        .define_accessor(
            "toString",
            Some(Rc::new(move |_| {
                observed.borrow_mut().push("string:get".into());
                let observed = observed.clone();
                let weak_value = weak_value.clone();
                Ok(Js::function("toString", move |this, _| {
                    let value = Js::Object(weak_value.upgrade().unwrap());
                    observed
                        .borrow_mut()
                        .push(format!("string:call:{}", this.same_reference(&value)));
                    Ok(Js::utf16(&[0xd800]))
                }))
            })),
            None,
        )
        .expect("test accessor target permits definition");
    let observed = log.clone();
    let key = Js::object(vec![(
        "toString",
        Js::function("toString", move |_, _| {
            observed.borrow_mut().push("key".into());
            Ok(Js::str("x"))
        }),
    )]);
    let o = Js::object(vec![]);
    let observed = log.clone();
    let reads = Cell::new(0);
    o.define_accessor(
        "x",
        Some(Rc::new(move |_| {
            reads.set(reads.get() + 1);
            observed.borrow_mut().push(format!("field:{}", reads.get()));
            Ok(if reads.get() == 3 {
                value.clone()
            } else {
                Js::Number(1.)
            })
        })),
        None,
    )
    .expect("test accessor target permits definition");
    let result = g
        .call_method("safeString", vec![o, key, token.clone()])
        .unwrap();
    let throwing = Js::object(vec![]);
    let observed = log.clone();
    let rejected = token.clone();
    throwing
        .define_accessor(
            "toString",
            Some(Rc::new(move |_| {
                observed.borrow_mut().push("throw:get".into());
                Err(Thrown::new(rejected.clone()))
            })),
            None,
        )
        .expect("test accessor target permits definition");
    let thrown = g
        .call_method("numberToString", vec![throwing])
        .unwrap_err()
        .value
        .same_reference(&token);
    let observed = log.clone();
    let returned_token = token.clone();
    let returning = Js::object(vec![(
        "toString",
        Js::function("toString", move |_, _| {
            observed.borrow_mut().push("return:call".into());
            Ok(returned_token.clone())
        }),
    )]);
    let returned = g
        .call_method("numberToString", vec![returning])
        .unwrap()
        .same_reference(&token);
    let defaults = g
        .call_method(
            "safeString",
            vec![
                Js::object(vec![("x", Js::Null)]),
                Js::str("x"),
                token.clone(),
            ],
        )
        .unwrap()
        .same_reference(&token);
    let f = Js::function("f", |_, _| Ok(Js::Undefined));
    f.define_accessor(
        "x",
        Some(Rc::new(|_| panic!("function is not a TS object operand"))),
        None,
    )
    .expect("test accessor target permits definition");
    let function_default = g
        .call_method("safeString", vec![f, Js::str("x"), token.clone()])
        .unwrap()
        .same_reference(&token);
    let observed = log.clone();
    let precision = g
        .call_method(
            "parsePrecision",
            vec![Js::object(vec![(
                "toString",
                Js::function("toString", move |_, _| {
                    observed.borrow_mut().push("parse:string".into());
                    Ok(Js::str("3x"))
                }),
            )])],
        )
        .unwrap();
    let observed = log.clone();
    let digits = Js::object(vec![(
        "valueOf",
        Js::function("valueOf", move |_, _| {
            observed.borrow_mut().push("digits:number".into());
            Ok(Js::Number(2.))
        }),
    )]);
    let observed = log.clone();
    let x = Js::object(vec![(
        "toString",
        Js::function("toString", move |_, _| {
            observed.borrow_mut().push("decimal:string".into());
            Ok(Js::str("1.2345"))
        }),
    )]);
    let decimal = g
        .call_method(
            "decimalToPrecision",
            vec![x, Js::Number(1.), digits, Js::Number(2.), Js::Number(5.)],
        )
        .unwrap();
    let mode = Js::object(vec![(
        "valueOf",
        Js::function("valueOf", |_, _| {
            panic!("CCXT_IMPORT rounding mode must not coerce")
        }),
    )]);
    let strict_mode = g
        .call_method(
            "decimalToPrecision",
            vec![
                Js::str("1.235"),
                mode,
                Js::Number(2.),
                Js::Number(2.),
                Js::Number(5.),
            ],
        )
        .unwrap();
    let actual = json!({"log":*log.borrow(),"units":result.to_js_string().unwrap().as_ref(),"thrown":thrown,"returned":returned,"defaults":defaults,"functionDefault":function_default,"precision":precision.to_display(),"decimal":decimal.to_display(),"strictMode":strict_mode.to_display()});
    assert_eq!(
        actual, expected,
        "CCXT_IMPORT repeated property/key reads, original referents and strict mode/coercion order"
    );
}

#[test]
fn installed_ccxt_owner_routes_keep_constants_and_strict_modes() {
    let shared = installed();
    let g = &shared.globals;
    for (name, expected) in [
        ("TRUNCATE", 0.),
        ("ROUND", 1.),
        ("ROUND_UP", 2.),
        ("ROUND_DOWN", 3.),
        ("DECIMAL_PLACES", 2.),
        ("SIGNIFICANT_DIGITS", 3.),
        ("TICK_SIZE", 4.),
        ("NO_PADDING", 5.),
        ("PAD_WITH_ZERO", 6.),
    ] {
        assert!(
            g.get(name).unwrap().strict_equals(&Js::Number(expected)),
            "{name}"
        );
    }
    let source = Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_ccxt_imports_source.cjs"
            ),
            "owner-routes",
        ])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    // Each route reaches the private helpers through the actual installed import.
    for (index, (name, args)) in [
        ("numberToString", vec![Js::Number(1e-8)]),
        ("parsePrecision", vec![Js::str("3junk")]),
        (
            "decimalToPrecision",
            vec![Js::str("123.45"), Js::Number(1.), Js::Number(-1.)],
        ),
        (
            "decimalToPrecision",
            vec![
                Js::str("1.2345"),
                Js::Number(1.),
                Js::Number(0.05),
                Js::Number(4.),
            ],
        ),
        (
            "decimalToPrecision",
            vec![Js::str("1.235"), Js::str("1"), Js::Number(2.)],
        ),
        (
            "safeString",
            vec![Js::object(vec![("x", Js::Number(42.))]), Js::str("x")],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(expected[index]["name"], name);
        let result = g.call_method(name, args).unwrap();
        assert_eq!(
            serde_json::json!(result.to_js_string().unwrap().as_ref()),
            expected[index]["units"],
            "{name}"
        );
    }
}

#[test]
fn ccxt_owner_receipts_match_maintained_source_expectations() {
    let source = Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_ccxt_imports_source.cjs"
            ),
            "owner-contracts",
        ])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Value = serde_json::from_slice(&source.stdout).unwrap();
    let contracts: Value = serde_json::from_str(include_str!(
        "../../safe-cex-parity/accepted-set/contracts/safe-cex-exchange.json"
    ))
    .unwrap();
    for (owner, cases) in expected.as_object().unwrap() {
        let key = format!("safe_cex_exchange::js_runtime::ccxt::{owner}");
        assert_eq!(
            &contracts[&key]["cases"], cases,
            "actual TS expectations for {owner}"
        );
        assert_eq!(
            &contracts[&key]["nativeObservation"]["cases"], cases,
            "native input observations for {owner}"
        );
    }
}
