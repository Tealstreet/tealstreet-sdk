#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        promise::{Promise, await_js},
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc};

fn describe(value: &Js) -> Value {
    match value {
        Js::Number(value) => json!({"bits":format!("{:016x}",value.to_bits())}),
        Js::Undefined => json!({"undefined":true}),
        Js::Null => json!({"null":true}),
        _ => json!({"kind":value.type_of()}),
    }
}
fn scope() -> Rc<ScopeShared<()>> {
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

async fn observe(shared: &Rc<ScopeShared<()>>, method: &str, label: &str) -> Value {
    let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
    let token = Js::object(vec![]);
    let explicit = Js::object(vec![]);
    let runtime: Rc<dyn ExchangeRuntime> = shared.runtime.clone();
    let array = match label {
        "holes" => Js::holey_array(vec![None, Some(Js::Number(8.0)), None, None]),
        "promised-item" => Js::array(vec![Js::Promise(Promise::settled(
            &runtime,
            Ok(Js::Number(7.0)),
        ))]),
        "item-rejects" => Js::array(vec![Js::Promise(Promise::settled(
            &runtime,
            Err(Thrown::new(token.clone())),
        ))]),
        "return-reference" => Js::array(vec![token.clone()]),
        "inherited" => {
            let object = Js::object(vec![("length", Js::Number(1.0))]);
            let Js::Object(data) = &object else {
                unreachable!()
            };
            *data.proto.borrow_mut() = Some(Js::object(vec![("0", Js::Number(7.0))]));
            object
        }
        "call-before-value" | "call-throws" | "value-throws" => {
            let object = Js::object(vec![("length", Js::Number(1.0))]);
            let events = trace.clone();
            let error = token.clone();
            let throws = label == "value-throws";
            object.define_accessor(
                "0",
                Some(Rc::new(move |_| {
                    events.borrow_mut().push(json!("value"));
                    if throws {
                        Err(Thrown::new(error.clone()))
                    } else {
                        Ok(Js::Number(7.0))
                    }
                })),
                None,
            );
            object
        }
        "string" => Js::str("ab"),
        _ => Js::array(vec![Js::Number(7.0), Js::Number(8.0)]),
    };
    let module = shared.globals.get("p-iteration").unwrap();
    let events = trace.clone();
    let input = array.clone();
    let expected_receiver = explicit.clone();
    let error = token.clone();
    let scenario = label.to_owned();
    let callback = Js::function("callback", move |this, args| {
        let index = args[1].as_f64().unwrap() as usize;
        events.borrow_mut().push(json!(format!("callback{index}")));
        if scenario == "grow" && index == 0 {
            args[2].push(Js::Number(9.0))?;
        }
        if scenario == "shrink" && index == 0 {
            args[2].set("length", Js::Number(1.0))?;
        }
        if scenario == "delete" && index == 0 {
            args[2].delete(&"1".encode_utf16().collect::<Vec<_>>());
        }
        if scenario == "callback-rejects" {
            return Ok(Js::Promise(Promise::settled(
                &runtime,
                Err(Thrown::new(error.clone())),
            )));
        }
        events
            .borrow_mut()
            .push(json!({"value":describe(&args[0]),"index":index,
            "sourceSame":args[2].same_reference(&input),"thisModule":this.same_reference(&module),
            "thisExplicit":this.same_reference(&expected_receiver)}));
        Ok(args[0].clone())
    });
    let call = callback.get("call").unwrap();
    let events = trace.clone();
    let error = token.clone();
    let throws = label == "call-throws";
    callback.define_accessor(
        "call",
        Some(Rc::new(move |_| {
            events.borrow_mut().push(json!("getCall"));
            if throws {
                Err(Thrown::new(error.clone()))
            } else {
                Ok(call.clone())
            }
        })),
        None,
    );
    let receiver = match label {
        "this-object" => explicit,
        "this-false" => Js::Bool(false),
        "this-zero" => Js::Number(0.0),
        "this-nan" => Js::Number(f64::NAN),
        _ => Js::Undefined,
    };
    let pending = shared
        .globals
        .get(method)
        .unwrap()
        .call(Js::Undefined, vec![array, callback, receiver])
        .unwrap();
    trace.borrow_mut().push(json!("returned"));
    let output =
        match await_js(pending).await {
            Ok(value) if value.is_undefined() => json!({"undefined":true}),
            Ok(value) => {
                let length = value.get("length").unwrap().as_f64().unwrap() as usize;
                let slots: Vec<Value> = (0..length).map(|index| {
                let key = index.to_string();
                json!({"own":value.has_own(&key),"value":describe(&value.get(&key).unwrap())})
            }).collect();
                json!({"length":length,"slots":slots,"same":if label=="return-reference" {
                json!(value.get("0").unwrap().same_reference(&token))
            } else {Value::Null}})
            }
            Err(error) if error.value.same_reference(&token) => json!({"original":true}),
            Err(error) => json!({"error":error.value.get("name").unwrap().to_display()}),
        };
    json!({"method":method,"label":label,"trace":*trace.borrow(),"output":output})
}

#[test]
fn installed_series_helpers_match_package_live_slots_awaits_and_receivers() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_series_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 38);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let shared = scope();
        for case in expected {
            let method = case["method"].as_str().unwrap();
            let label = case["label"].as_str().unwrap();
            assert_eq!(
                observe(&shared, method, label).await,
                case,
                "SERIES_SOURCE {method}/{label} native package observations"
            );
        }
    });
}
