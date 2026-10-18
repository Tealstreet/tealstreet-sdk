//! The actual Number member binding, observed through the installed native host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{process::Command, rc::Rc, sync::Arc};
fn radix_args(tag: &str) -> Vec<Js> {
    let value = match tag {
        "omitted" => return vec![],
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "zero" => Js::Number(0.),
        "one" => Js::Number(1.),
        "thirty_seven" => Js::Number(37.),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "negative_infinity" => Js::Number(f64::NEG_INFINITY),
        "fraction" => Js::Number(2.9),
        "string" => Js::str("16"),
        "object" => Js::object(vec![(
            "valueOf",
            Js::function("valueOf", |_, _| Ok(Js::Number(16.))),
        )]),
        "throwing" => Js::object(vec![(
            "valueOf",
            Js::function("valueOf", |_, _| {
                Err(Thrown::error("RangeError", "radix coercion"))
            }),
        )]),
        "symbol" => Js::SymbolIterator,
        "bigint" => Js::BigInt(Rc::new(16.into())),
        _ => panic!("unknown source radix {tag}"),
    };
    vec![value]
}
#[test]
fn number_radix_matches_node_for_extremes_fractions_all_radices_and_coercion() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_number_radix_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["cases"].as_array().unwrap().len(), 5320);
    assert_eq!(source["conversions"].as_array().unwrap().len(), 60);
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
        let number = |row: &Value| {
            let n = f64::from_bits(u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap());
            shared
                .globals
                .get("Number")
                .unwrap()
                .call(Js::Undefined, vec![Js::Number(n)])
                .unwrap()
        };
        for row in source["cases"].as_array().unwrap() {
            let observed = number(row)
                .call_method("toString", vec![Js::Number(row["radix"].as_f64().unwrap())])
                .unwrap()
                .to_display();
            assert_eq!(
                json!(observed),
                row["result"],
                "RADIX {} {}",
                row["bits"],
                row["radix"]
            );
        }
        for row in source["conversions"].as_array().unwrap() {
            let observed =
                number(row).call_method("toString", radix_args(row["tag"].as_str().unwrap()));
            let (result, error) = match observed {
                Ok(v) => (Some(v.to_display()), None),
                Err(e) => (None, Some(e.value.get("name").unwrap().to_display())),
            };
            assert_eq!(
                json!({"result":result,"error":error}),
                json!({"result":row["result"],"error":row["error"]}),
                "COERCION {} {}",
                row["bits"],
                row["tag"]
            );
        }
    });
}
