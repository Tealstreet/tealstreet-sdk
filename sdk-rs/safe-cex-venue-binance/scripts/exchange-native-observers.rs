// Actual production class/Store/HTTP observations. Only terminal IO is scripted.
#[path = "exchange-native-runtime.rs"]
mod runtime;
use crate::exchange_classes::ExchangeOwner;
use crate::private_session::Venue;
#[allow(
    unused_imports,
    reason = "shared fixture API serves independent scoped native probes"
)]
pub use runtime::{Parts, construct, drive, family, host, parts, raw, wire};
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};
pub fn observe_constructor(owner: &ExchangeOwner, parts: &Parts) -> Value {
    let clients = ["xhr", "papiXhr", "publicXhr", "fapiXhr", "unlimitedXHR"]
        .map(|key| owner.clients.get(key).unwrap().truthy());
    let path = if owner.venue == Venue::PM {
        "/papi/v1/um/openOrders"
    } else {
        "/fapi/v1/openOrders"
    };
    parts.http.ok("GET", path, json!([]));
    let reply = drive(
        &parts.runtime,
        owner
            .request(
                crate::exchange_classes::Client::Private,
                crate::exchange_classes::HttpMethod::Get,
                path,
                None,
            )
            .unwrap(),
    )
    .result
    .unwrap();
    json!({"family":format!("{:?}",owner.venue),"clients":clients,"publicIdentity":Rc::ptr_eq(&owner.public, &owner.state.public_socket()),"privateIdentity":Rc::ptr_eq(&owner.private,&owner.state.private_socket()),"stateIdentity":Rc::ptr_eq(&owner.state,std::ops::Deref::deref(owner)),"reply":raw(&reply),"requests":wire(&parts.http)})
}
pub fn market_result(result: Result<Js, crate::request_weight::Failure>) -> Value {
    match result {
        Ok(Js::Undefined) => json!({"kind":"undefined"}),
        Ok(value) => {
            json!({"id":value.get("id").unwrap().to_js_string().unwrap(),"symbol":value.get("symbol").unwrap().to_js_string().unwrap(),"amount":raw(&value.get("precision").unwrap().get("amount").unwrap())})
        }
        Err(error) => json!({"error":error.name,"message":error.message}),
    }
}
pub fn request_observation(
    parts: &Parts,
    request: Result<crate::private_rest::SnapshotRequestFuture, crate::request_weight::Failure>,
) -> Value {
    let result = match request {
        Ok(future) => drive(&parts.runtime, future).result,
        Err(error) => Err(error),
    };
    let value = match result {
        Ok(value) => json!({"ok":raw(&value)}),
        Err(error) => json!({"error":error.name,"message":error.message}),
    };
    json!({"result":value,"requests":wire(&parts.http)})
}

#[path = "../tests/support/json_error_runtime.rs"]
pub mod capture;
pub fn emission(
    input: &Value,
    function: impl FnOnce(
        &safe_cex_exchange::exchange::BaseExchange,
        &Value,
    ) -> Result<(), safe_cex_exchange::runtime::ExchangeError>,
) -> Value {
    let runtime = Rc::new(capture::Runtime::default());
    let http = Arc::new(host::Http::default());
    let deps = crate::native_host::native_dependencies(
        runtime.clone(),
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let subject = crate::exchange_classes::BinanceExchange::new(
        ExchangeName::Binance,
        host::options(),
        Rc::new(DefaultStore::new()),
        deps,
    )
    .unwrap();
    let events = Rc::new(std::cell::RefCell::new(Vec::<Js>::new()));
    let seen = events.clone();
    subject.base.on(
        "error",
        Rc::new(move |_, value| {
            seen.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    function(&subject.base, &input["value"]).unwrap();
    let units = events.borrow().last().unwrap().to_js_string().unwrap();
    let report = if input["value"].is_object() {
        let reports = runtime.raw_reports.borrow();
        let report = reports
            .last()
            .expect("raw object reaches the actual reporter");
        let console = runtime.error_console.borrow();
        let args = console.last().expect("actual console emission");
        assert!(
            report.strict_equals(&args[1]),
            "console/reporter retain same native object"
        );
        json!({"name":report.get("name").unwrap().to_js_string().unwrap(),"message":report.get("message").unwrap().to_js_string().unwrap(),"detail":raw(&report.get("detail").unwrap())})
    } else {
        Value::Null
    };
    let count = events.borrow().len();
    subject.base.dispose().unwrap();
    json!({"units":units,"report":report,"events":count,"requests":http.sent.lock().unwrap().len()})
}

#[path = "exchange-bridge-observers.rs"]
mod bridge;
pub fn carrier(value: &Value) -> Value {
    bridge::json_observation(value)
}
