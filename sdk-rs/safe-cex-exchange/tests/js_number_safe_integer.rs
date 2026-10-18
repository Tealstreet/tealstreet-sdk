//! Number.isSafeInteger through the installed native runtime, compared with Node.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use serde_json::Value;
use std::{cell::Cell, process::Command, rc::Rc, sync::Arc};

#[test]
fn number_is_safe_integer_matches_node_without_coercion() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_number_safe_integer_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["rows"].as_array().unwrap().len(), 239);
    assert_eq!(source["types"].as_array().unwrap().len(), 13);
    assert_eq!(source["calls"], 0);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
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
        let number = shared.globals.get("Number").unwrap();
        for row in source["rows"].as_array().unwrap() {
            let bits = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
            let observed = number
                .call_method("isSafeInteger", vec![Js::Number(f64::from_bits(bits))])
                .expect("installed Number.isSafeInteger must accept native numbers");
            assert!(
                observed.strict_equals(&Js::Bool(row["valid"].as_bool().unwrap())),
                "raw number {row}"
            );
        }
        let calls = Rc::new(Cell::new(0));
        let watched = calls.clone();
        let poison = Js::object(vec![(
            "valueOf",
            Js::function("valueOf", move |_, _| {
                watched.set(watched.get() + 1);
                Err(Thrown::error("Error", "must not coerce"))
            }),
        )]);
        for row in source["types"].as_array().unwrap() {
            let kind = row["kind"].as_str().unwrap();
            let args = match kind {
                "omitted" => vec![],
                "undefined" => vec![Js::Undefined],
                "null" => vec![Js::Null],
                "false" => vec![Js::Bool(false)],
                "true" => vec![Js::Bool(true)],
                "string" => vec![Js::str("1")],
                "empty_string" => vec![Js::str("")],
                "array" => vec![Js::array(vec![Js::Number(1.)])],
                "object" => vec![Js::object(vec![])],
                "poison" => vec![poison.clone()],
                "function" => vec![Js::function("one", |_, _| Ok(Js::Number(1.)))],
                "bigint" => vec![Js::BigInt(Rc::new(1.into()))],
                "symbol" => vec![Js::SymbolIterator],
                _ => panic!("unexpected source case {kind}"),
            };
            let observed = number.call_method("isSafeInteger", args).unwrap();
            assert!(
                observed.strict_equals(&Js::Bool(row["valid"].as_bool().unwrap())),
                "argument type {kind}"
            );
        }
        assert_eq!(
            calls.get(),
            0,
            "Number.isSafeInteger must not coerce objects"
        );
    });
}
