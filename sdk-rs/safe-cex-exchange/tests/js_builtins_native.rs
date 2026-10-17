//! Installed builtin surface on NativeRuntime with the real native HTTP host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        scope::{Scope, ScopeShared},
        value::{Js, to_json},
    },
    native::native_runtime,
};
use serde_json::json;
use std::{rc::Rc, sync::Arc};

#[test]
fn native_host_executes_source_array_dayjs_property_and_console_calls() {
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
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        library::install_library(&shared.globals, Rc::downgrade(&shared));
        let scope = Scope {
            shared,
            this: Js::Undefined,
        };
        let array = Js::array(vec![Js::Number(1.), Js::Number(2.), Js::Number(3.)]);
        assert_eq!(array.call_method("pop", vec![]).unwrap().as_f64(), Some(3.));
        assert!(
            array
                .call_method("reverse", vec![])
                .unwrap()
                .same_reference(&array)
        );
        assert_eq!(to_json(&array).unwrap(), json!([2., 1.]));
        let day = scope
            .global("dayjs")
            .unwrap()
            .call(Js::Undefined, vec![Js::str("2024-03-10 08:15:00")])
            .unwrap();
        let adjusted = day
            .call_method("utcOffset", vec![Js::Number(480.), Js::Bool(true)])
            .unwrap();
        assert_eq!(
            adjusted.call_method("valueOf", vec![]).unwrap().as_f64(),
            Some(1_710_029_700_000.)
        );
        let midnight = adjusted
            .call_method("startOf", vec![Js::str("date")])
            .unwrap();
        assert_eq!(
            midnight.call_method("valueOf", vec![]).unwrap().as_f64(),
            Some(1_710_000_000_000.)
        );
        let object = Js::object(vec![("visible", Js::Number(1.))]);
        scope
            .global("Object.defineProperty")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    object.clone(),
                    Js::str("hidden"),
                    Js::object(vec![("value", Js::Number(2.))]),
                ],
            )
            .unwrap();
        let keys = scope
            .global("Object.getOwnPropertyNames")
            .unwrap()
            .call(Js::Undefined, vec![object])
            .unwrap();
        assert_eq!(to_json(&keys).unwrap(), json!(["visible", "hidden"]));
        scope
            .global("console.log")
            .unwrap()
            .call(Js::Undefined, vec![Js::str("native builtin regression")])
            .unwrap();
    });
}
