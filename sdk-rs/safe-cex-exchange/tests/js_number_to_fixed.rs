//! Number#toFixed through the production native runtime and builtin dispatcher.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use std::{rc::Rc, sync::Arc};

#[test]
fn native_number_to_fixed_matches_node_and_preserves_coercion_errors() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(runtime.clone(), Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
        let shared = Rc::new(ScopeShared::new(runtime, http,
            Rc::new(safe_cex_store::store::DefaultStore::new()), Js::object(vec![]), ()));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let values = [10.125, -10.125, 0.5, -0.0, -0.001, 1e21, 1e20,
            f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1.005];
        let digits = [f64::NAN, -1.0, 0.0, 2.0, 2.9, 100.0, 101.0, f64::INFINITY];
        let output = std::process::Command::new("node").args(["-e", r#"
const values=[10.125,-10.125,0.5,-0,-0.001,1e21,1e20,Infinity,-Infinity,NaN,1.005];
const digits=[NaN,-1,0,2,2.9,100,101,Infinity];
console.log(JSON.stringify(values.flatMap(n=>digits.map(d=>{try{return {text:n.toFixed(d)}}catch(e){return {error:e.name}}}))));
"#]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        let mut index = 0;
        for value in values {
            for digits in digits {
                let result = Js::Number(value).call_method("toFixed", vec![Js::Number(digits)]);
                let actual = match result {
                    Ok(text) => serde_json::json!({"text":text.to_display()}),
                    Err(error) => serde_json::json!({"error":error.value.get("name").unwrap().to_display()}),
                };
                assert_eq!(actual, expected[index], "toFixed value={value:?} digits={digits:?}");
                index += 1;
            }
        }
        for args in [vec![], vec![Js::Undefined], vec![Js::Null], vec![Js::str("2")]] {
            let expected = if matches!(args.first(), Some(Js::String(_))) {"10.13"} else {"10"};
            assert_eq!(Js::Number(10.125).call_method("toFixed", args).unwrap().to_display(), expected);
        }
        let marker = Js::object(vec![("identity", Js::str("digits conversion"))]);
        let thrown = marker.clone();
        let digits = Js::object(vec![("toString", Js::function("toString", move |_, _| Err(Thrown::new(thrown.clone()))))]);
        let error = Js::Number(10.125).call_method("toFixed", vec![digits]).unwrap_err();
        assert!(error.value.strict_equals(&marker), "original coercion throw identity");
    });
}
