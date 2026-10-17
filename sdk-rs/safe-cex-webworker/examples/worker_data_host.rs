#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn post_worker_data(post: js_sys::Function) -> Result<(), JsValue> {
    use safe_cex_exchange::{js_runtime::value::Js, runtime::ExchangeError};
    use safe_cex_webworker::{
        data_post::{WorkerDataPayload, WorkerDataPost},
        payload::StoreWorkerOp,
    };
    use std::rc::Rc;
    let host = WorkerDataPost::for_browser(Rc::new(move |payload| {
        post.call1(&JsValue::UNDEFINED, payload)
            .map(|_| ())
            .map_err(|e| ExchangeError::new(format!("native post failed: {e:?}")))
    }));
    let shared = Js::object(vec![
        ("undefined", Js::Undefined),
        ("nan", Js::Number(f64::NAN)),
        ("positive", Js::Number(f64::INFINITY)),
        ("negative", Js::Number(f64::NEG_INFINITY)),
        ("zero", Js::Number(-0.0)),
        (
            "big",
            Js::BigInt(Rc::new("900719925474099312345".parse().unwrap())),
        ),
        ("bool", Js::Bool(true)),
        ("null", Js::Null),
    ]);
    shared
        .define("__proto__", Js::str("own proto data"))
        .unwrap();
    shared.define_key(&[0xd800], Js::utf16(&[0xdc00])).unwrap();
    shared.define("self", shared.clone()).unwrap();
    let map = Js::new_map();
    map.call_method("set", vec![shared.clone(), Js::Number(f64::NAN)])
        .map_err(|e| js_sys::Error::new(&e.message()))?;
    let set = Js::new_set();
    set.call_method("add", vec![shared.clone()])
        .map_err(|e| js_sys::Error::new(&e.message()))?;
    let data = Js::object(vec![
        ("requestId", Js::str("host")),
        ("result", Js::array(vec![shared.clone(), shared])),
        ("map", map),
        ("set", set),
    ]);
    host.send(WorkerDataPayload {
        op: StoreWorkerOp::CallMethodDone,
        data,
    })
    .map_err(|e| js_sys::Error::new(&e.message).into())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn post_worker_error_data(post: js_sys::Function) -> Result<(), JsValue> {
    use safe_cex_exchange::{
        js_runtime::value::{Js, Thrown},
        runtime::ExchangeError,
    };
    use safe_cex_webworker::{
        data_post::{WorkerDataPayload, WorkerDataPost},
        payload::StoreWorkerOp,
    };
    use std::rc::Rc;
    let host = WorkerDataPost::for_browser(Rc::new(move |payload| {
        post.call1(&JsValue::UNDEFINED, payload)
            .map(|_| ())
            .map_err(|error| ExchangeError::new(format!("native post failed: {error:?}")))
    }));
    let error = Thrown::error("AxiosError", "retained event error").value;
    error
        .define(
            "stack",
            Js::str("original event stack\n    at source-event"),
        )
        .unwrap();
    error.define("code", Js::Number(f64::INFINITY)).unwrap();
    let typed_error = Thrown::type_error("typed event").value;
    let message: Vec<u16> = [0xd800]
        .into_iter()
        .chain("typed event".encode_utf16())
        .collect();
    typed_error.set("message", Js::utf16(&message)).unwrap();
    typed_error
        .define("stack", Js::str("original typed stack"))
        .unwrap();
    host.send(WorkerDataPayload {
        op: StoreWorkerOp::Error,
        data: Js::array(vec![
            Js::str("failed"),
            Js::object(vec![(
                "sentryData",
                Js::object(vec![("error", error), ("typedError", typed_error)]),
            )]),
        ]),
    })
    .map_err(|error| js_sys::Error::new(&error.message).into())
}
