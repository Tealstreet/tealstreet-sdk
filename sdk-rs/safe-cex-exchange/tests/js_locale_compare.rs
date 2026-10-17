#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use serde::Deserialize;
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc};

#[derive(Deserialize)]
struct Case {
    left: String,
    right: String,
    expected: i8,
}

#[test]
fn native_locale_compare_matches_node_default_collation() {
    let output = Command::new("node")
        .args([
            "-e",
            r#"
const assert = require('node:assert/strict');
assert.match(Intl.Collator().resolvedOptions().locale, /^en(?:-|$)/);
const atoms = ['', 'a', 'A', 'b', 'B', 'z', 'Z', 'é', 'e', 'e\u0301', 'ä',
 'å', 'á', 'É', 'Å', 'Æ', 'æ', 'ss', 'ß', 'œ', 'oe', '2', '10', '02', '002',
 '0', '1', '20', '9', '-', '_', ' ', '.', '!', 'call', 'put', 'CALL', 'PUT',
 'BTC-2', 'BTC-10', 'undefined', 'null', 'NaN', 'Infinity', '😀', '中'];
const rows = atoms.flatMap(left => atoms.map(right =>
 ({left, right, expected: Math.sign(left.localeCompare(right))})));
assert.equal(rows.length, 2116);
console.log(JSON.stringify(rows));
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 2116);
    with_native_string(|constructor| {
        for case in cases {
            let source = constructor
                .call(Js::Undefined, vec![Js::str(&case.left)])
                .unwrap();
            let actual = source
                .call_method("localeCompare", vec![Js::str(&case.right)])
                .unwrap();
            assert_eq!(
                actual.to_number().unwrap().partial_cmp(&0.0).unwrap() as i8,
                case.expected,
                "default collation: {:?} vs {:?}",
                case.left,
                case.right
            );
        }
    });
}

fn with_native_string(callback: impl FnOnce(Js)) {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let native = native_runtime();
        let http = client::rest_http_host(
            native.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            native,
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let constructor = shared.globals.get("String").unwrap();
        callback(constructor);
    });
}

#[test]
fn native_locale_compare_coerces_arguments_and_preserves_thrown_identity() {
    let output = Command::new("node")
        .args([
            "-e",
            r#"
const trace = [];
const right = {toString() {trace.push('toString'); return 'é'},
 valueOf() {trace.push('valueOf'); return 'a'}};
const comparison = Math.sign('e'.localeCompare(right));
const defaults = ['undefined'.localeCompare(), 'null'.localeCompare(null),
 'true'.localeCompare(true), '0'.localeCompare(-0), 'NaN'.localeCompare(NaN)];
const sentinel = {};
let sameError = false;
try { 'e'.localeCompare({toString() {throw sentinel}}); }
catch (error) {sameError = error === sentinel}
console.log(JSON.stringify({comparison, trace, defaults, sameError}));
"#,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    with_native_string(|constructor| {
        let source = constructor.call(Js::Undefined, vec![Js::str("e")]).unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let string_trace = trace.clone();
        let number_trace = trace.clone();
        let right = Js::object(vec![
            (
                "toString",
                Js::function("toString", move |_, _| {
                    string_trace.borrow_mut().push("toString");
                    Ok(Js::str("é"))
                }),
            ),
            (
                "valueOf",
                Js::function("valueOf", move |_, _| {
                    number_trace.borrow_mut().push("valueOf");
                    Ok(Js::str("a"))
                }),
            ),
        ]);
        let comparison = source
            .call_method("localeCompare", vec![right])
            .unwrap()
            .to_number()
            .unwrap();
        let mut defaults = Vec::new();
        for (left, right) in [
            ("undefined", vec![]),
            ("null", vec![Js::Null]),
            ("true", vec![Js::Bool(true)]),
            ("0", vec![Js::Number(-0.0)]),
            ("NaN", vec![Js::Number(f64::NAN)]),
        ] {
            let source = constructor
                .call(Js::Undefined, vec![Js::str(left)])
                .unwrap();
            defaults.push(
                source
                    .call_method("localeCompare", right)
                    .unwrap()
                    .to_number()
                    .unwrap() as i64,
            );
        }
        let sentinel = Js::object(vec![]);
        let thrown = sentinel.clone();
        let right = Js::object(vec![(
            "toString",
            Js::function("toString", move |_, _| Err(Thrown::new(thrown.clone()))),
        )]);
        let error = source
            .call_method("localeCompare", vec![right])
            .unwrap_err();
        assert_eq!(
            serde_json::json!({"comparison": comparison as i64,
            "trace": *trace.borrow(), "defaults": defaults,
            "sameError": error.value.strict_equals(&sentinel)}),
            expected
        );
    });
}

#[test]
fn native_locale_compare_pins_authorized_icu_surrogate_residual() {
    let output = Command::new("node").args(["-e", r#"
const pairs = [[0xd800, 0xdfff], [0xd800, 0xfffd], [0xdfff, 0xfffd], [0xfffd, 0xd800]];
console.log(JSON.stringify(pairs.map(([a,b]) => Math.sign(String.fromCharCode(a).localeCompare(String.fromCharCode(b))))));
"#]).output().unwrap();
    assert!(output.status.success());
    let node: Vec<i8> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        node,
        [-1, -1, -1, 1],
        "Node keeps distinct surrogate weights"
    );
    with_native_string(|constructor| {
        let mut icu = Vec::new();
        for (left, right) in [
            (0xd800, 0xdfff),
            (0xd800, 0xfffd),
            (0xdfff, 0xfffd),
            (0xfffd, 0xd800),
        ] {
            let source = constructor
                .call(Js::Undefined, vec![Js::utf16(&[left])])
                .unwrap();
            icu.push(
                source
                    .call_method("localeCompare", vec![Js::utf16(&[right])])
                    .unwrap()
                    .to_number()
                    .unwrap() as i8,
            );
        }
        assert_eq!(
            icu,
            [0, 0, 0, 0],
            "ICU treats each unpaired surrogate as U+FFFD"
        );
        assert_ne!(icu, node, "authorized residual must remain explicit");
    });
}
