//! Installed BigNumber facade observations against the actual npm dependency.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, library, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{process::Command, rc::Rc, sync::Arc};
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_bignumber_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn input(spec: &Value) -> Js {
    let s = spec["value"].as_str().unwrap();
    if spec["kind"] == "number" {
        Js::Number(safe_cex_num::number_from_string(s))
    } else {
        Js::str(s)
    }
}
fn host(run: impl FnOnce(&Js)) {
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
        library::install_library(&shared.globals, Rc::downgrade(&shared));
        run(&shared.globals);
    });
}
fn view(g: &Js, n: &Js) -> Value {
    let call = |key, args| n.call_method(key, args).unwrap();
    let number = call("toNumber", vec![]).as_f64().unwrap();
    let number = if number == 0. && number.is_sign_negative() {
        "-0".into()
    } else {
        safe_cex_num::js_number_string(number)
    };
    json!({"str":call("toString",vec![]).to_display(),"value":call("valueOf",vec![]).to_display(),"fixed":[call("toFixed",vec![]).to_display(),call("toFixed",vec![Js::Number(0.)]).to_display(),call("toFixed",vec![Js::Number(2.)]).to_display(),call("toFixed",vec![Js::Number(6.)]).to_display()],"json":g.get("JSON").unwrap().call_method("stringify",vec![Js::object(vec![("sz",n.clone())])]).unwrap().to_display(),"nan":call("isNaN",vec![]).truthy(),"finite":call("isFinite",vec![]).truthy(),"number":number})
}
#[test]
fn bignumber_construct_and_call_match_the_installed_package() {
    let source = source();
    let cases = source["scalars"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    host(|g| {
        let ctor = g.get("BigNumber").unwrap();
        assert!(ctor.is_constructor(), "BigNumber must support source `new`");
        for row in cases {
            let called = ctor.call(Js::Undefined, vec![input(&row["spec"])]).unwrap();
            assert_eq!(view(g, &called), row["call"], "call {}", row["spec"]);
            let constructed = ctor.construct(vec![input(&row["spec"])], None).unwrap();
            assert_eq!(view(g, &constructed), row["result"], "new {}", row["spec"]);
            let cloned = ctor.construct(vec![constructed], None).unwrap();
            assert_eq!(view(g, &cloned), row["clone"], "new clone {}", row["spec"]);
        }
    });
}

#[test]
fn bignumber_scalars_preserve_signed_zero_nonfinite_values_and_json() {
    let source = source();
    let cases = source["scalars"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    host(|g| {
        for row in cases {
            let ctor = g.get("BigNumber").unwrap();
            let n = ctor.call(Js::Undefined, vec![input(&row["spec"])]).unwrap();
            assert_eq!(view(g, &n), row["result"], "scalar {}", row["spec"]);
            let clone = ctor.call(Js::Undefined, vec![n]).unwrap();
            assert_eq!(view(g, &clone), row["clone"], "clone {}", row["spec"]);
        }
    });
}
#[test]
fn bignumber_arithmetic_and_comparisons_preserve_nonfinite_and_zero_signs() {
    let source = source();
    let cases = source["pairs"].as_array().unwrap();
    assert_eq!(cases.len(), 49);
    host(|g| {
        for row in cases {
            let n = g
                .get("BigNumber")
                .unwrap()
                .call(Js::Undefined, vec![input(&row["left"])])
                .unwrap();
            for (method, expected) in row["operations"].as_object().unwrap() {
                let result = n.call_method(method, vec![input(&row["right"])]).unwrap();
                assert_eq!(
                    view(g, &result),
                    *expected,
                    "{} {method} {}",
                    row["left"],
                    row["right"]
                );
            }
            for (method, expected) in row["comparisons"].as_object().unwrap() {
                let result = n.call_method(method, vec![input(&row["right"])]).unwrap();
                assert_eq!(
                    json!(result.truthy()),
                    *expected,
                    "{} {method} {}",
                    row["left"],
                    row["right"]
                );
            }
        }
    });
}
#[test]
fn bignumber_extrema_keep_infinities_nan_and_signed_zero_ties() {
    let source = source();
    let cases = source["extremes"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    host(|g| {
        for row in cases {
            for (method, expected) in row["result"].as_object().unwrap() {
                let inputs = row["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(input)
                    .collect();
                let n = g
                    .get("BigNumber")
                    .unwrap()
                    .call_method(method, inputs)
                    .unwrap();
                assert_eq!(view(g, &n), *expected, "{method} {}", row["inputs"]);
            }
        }
    });
}
