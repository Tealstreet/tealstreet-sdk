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
        Rc::new(safe_cex_store::store::DefaultStore::new()),
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
