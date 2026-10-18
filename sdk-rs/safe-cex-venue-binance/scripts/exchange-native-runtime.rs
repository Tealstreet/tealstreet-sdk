// Actual production class/Store/HTTP observations. Only terminal IO is scripted.
#[path = "../tests/support/inverse_rest_runtime.rs"]
pub mod host;
use crate::exchange_classes::{ExchangeDependencies, ExchangeOwner};
use crate::private_session::Venue;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
pub struct Parts {
    pub runtime: Rc<host::Runtime>,
    pub http: Arc<host::Http>,
    pub store: Rc<DefaultStore>,
    pub options: ExchangeOptions,
    pub deps: Option<ExchangeDependencies>,
}
pub fn parts(input: &Value) -> Parts {
    let runtime = Rc::new(host::Runtime::default());
    runtime.now.set(1_700_000_000_000.);
    let http = Arc::new(host::Http::default());
    let store = Rc::new(DefaultStore::new());
    Store::update(store.as_ref(), serde_json::from_value(json!({"markets":[{"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.1},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":50}}}]})).unwrap(), false).unwrap();
    let mut options = host::options();
    options.testnet = input["testnet"].as_bool();
    let deps = crate::native_host::native_dependencies(
        runtime.clone(),
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    Parts {
        runtime,
        http,
        store,
        options,
        deps: Some(deps),
    }
}
pub fn family(input: &Value) -> (Venue, ExchangeName) {
    match input["family"].as_str().unwrap_or("futures") {
        "futures" => (Venue::Futures, ExchangeName::Binance),
        "pm" => (Venue::PM, ExchangeName::Binancepm),
        "spot" => (Venue::Spot, ExchangeName::Binancespot),
        _ => panic!("unclassified family"),
    }
}
pub fn construct(input: &Value) -> (Parts, Rc<ExchangeOwner>) {
    let mut parts = parts(input);
    // Factory callers that need the consumed deps should use parts directly.
    // This helper retains only the native runtime/HTTP/Store after construction.
    let (venue, name) = family(input);
    let deps = parts.deps.take().unwrap();
    let subject = ExchangeOwner::new(
        venue,
        name,
        parts.options.clone(),
        parts.store.clone(),
        deps,
    )
    .unwrap();
    (parts, subject)
}
pub fn drive<T>(runtime: &host::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..128 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
    }
    panic!("bounded primitive HTTP did not settle")
}
pub fn raw(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Null => json!({"kind":"null"}),
        Js::Bool(value) => json!({"bool":value}),
        Js::Number(value) => json!({"bits":format!("{:016x}",value.to_bits())}),
        _ if value.is_string() => json!({"units":value.to_js_string().unwrap()}),
        Js::Array(_) => {
            let Js::Number(length) = value.get("length").unwrap() else {
                panic!("native array length")
            };
            json!({"array":(0..length as usize).map(|i|raw(&value.get(&i.to_string()).unwrap())).collect::<Vec<_>>()})
        }
        Js::Object(_) => {
            json!({"object":value.own_keys().iter().map(|key|json!([key,raw(&value.get_key(key).unwrap())])).collect::<Vec<_>>()})
        }
        _ => json!({"kind":value.type_of()}),
    }
}
pub fn wire(http: &host::Http) -> Value {
    json!(http.sent.lock().unwrap().iter().map(|request| {
        let query=request.url.split_once('?').map(|(_,q)|q).unwrap_or("").split('&').filter(|field|!field.is_empty() && !["timestamp=","recvWindow=","signature="].iter().any(|key|field.starts_with(key))).collect::<Vec<_>>();
        json!({"method":request.method,"base":request.base_url,"path":request.url.split('?').next().unwrap(),"query":query,"signed":request.url.contains("signature=")})
    }).collect::<Vec<_>>())
}
