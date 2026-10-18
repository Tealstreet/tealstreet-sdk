#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use serde::Deserialize;
use std::{process::Command, rc::Rc, sync::Arc};

#[derive(Deserialize)]
struct Case {
    input: Vec<u16>,
    expected: Vec<u16>,
}

fn check_node(method: &str) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_string_utf16_source.cjs"
        ))
        .arg(method)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4109);
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
        for (index, case) in cases.iter().enumerate() {
            let value = constructor
                .call(Js::Undefined, vec![Js::utf16(&case.input)])
                .unwrap();
            let actual = value.call_method(method, vec![]).unwrap();
            assert_eq!(
                actual.to_js_string().unwrap().as_ref(),
                case.expected,
                "{method} Node case {index}, UTF-16 input {:?}",
                case.input
            );
        }
    });
}

#[test]
fn native_uppercase_preserves_utf16_and_matches_node() {
    check_node("toUpperCase");
}

#[test]
fn native_lowercase_preserves_utf16_and_matches_node() {
    check_node("toLowerCase");
}

#[test]
fn native_trim_preserves_utf16_and_matches_node() {
    check_node("trim");
}
