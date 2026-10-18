// Native observations only. The included host scripts IO, never venue bodies.
#[path = "../tests/support/inverse_rest_runtime.rs"]
pub mod host;
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, Thrown, from_json},
};
use safe_cex_store::Store;
use serde_json::{Value, json};
use std::rc::Rc;

pub fn number(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", n.to_bits())
    }
}
pub fn observe(value: Result<Js, Thrown>) -> Value {
    match value {
        Err(e) => json!({"error":e.value.get("name").unwrap().to_display()}),
        Ok(v) => match &v {
            Js::Undefined => json!({"kind":"undefined"}),
            Js::Null => json!({"kind":"null"}),
            Js::Number(n) => json!({"bits":number(*n)}),
            Js::Bool(b) => json!({"bool":b}),
            _ if v.is_string() => json!({"units":v.to_js_string().unwrap()}),
            _ => json!({"kind":v.type_of(),"keys":v.own_keys()}),
        },
    }
}
pub fn raw(input: &Value) -> Js {
    match input["profile"].as_str() {
        Some("undefined") => Js::Undefined,
        Some("inherited") => {
            let v = Js::object(vec![]);
            let Js::Object(data) = &v else { unreachable!() };
            *data.proto.borrow_mut() = Some(Js::object(vec![(
                "toString",
                Js::function("inherited", |_, _| {
                    panic!("in must not invoke inherited function")
                }),
            )]));
            v
        }
        Some("negative-zero") => Js::Number(-0.0),
        Some("infinity") => Js::Number(f64::INFINITY),
        Some("lone-unit") => Js::utf16(&[0xd800]),
        _ => from_json(&input["value"]),
    }
}
pub fn make(input: &Value) -> host::Harness {
    let mut options = host::options();
    options.testnet = input["testnet"].as_bool();
    let h = host::harness(options, |http| {
        http.ok("GET", "/dapi/v1/positionRisk", json!([]));
        http.ok("GET","/dapi/v1/openOrders",json!([{"orderId":17,"symbol":"BTCUSD_PERP","type":"LIMIT","side":"BUY","price":"123.5","origQty":"2.5","executedQty":"0.5","reduceOnly":false}]));
        http.ok("GET","/dapi/v1/trades",json!([{"id":7,"price":"103","qty":"2","baseQty":"0.04","time":4000,"isBuyerMaker":false}]));
        for endpoint in [
            "/dapi/v1/leverage",
            "/dapi/v1/positionSide/dual",
            "/dapi/v1/marginType",
        ] {
            http.ok("POST", endpoint, json!({"code":200}));
        }
        http.ok("DELETE", "/dapi/v1/allOpenOrders", json!({"code":200}));
        http.ok(
            "GET",
            "/dapi/v1/depth",
            json!({"lastUpdateId":10,"bids":[["101","2"]],"asks":[["103","3"]]}),
        );
    });
    Store::update(h.store.as_ref(),serde_json::from_value(json!({"markets":[{"id":"BTC/USD:BTC","symbol":"BTCUSD_PERP","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10},"leverage":{"min":1,"max":50}},"contractSize":100,"isInverse":true}],"positions":[{"id":"BTCUSD_PERP:long","symbol":"BTCUSD_PERP","side":"long","entryPrice":90,"notional":200,"leverage":10,"unrealizedPnl":4,"contracts":2,"liquidationPrice":40,"marginSize":20,"marginType":"cross"}]})).unwrap(),false).unwrap();
    h
}
pub fn rt(h: &host::Harness) -> crate::inverse::Rt {
    h.exchange.scope.clone()
}
pub fn await_value(h: &host::Harness, value: Js) -> Result<Js, Thrown> {
    h.drive(Box::pin({
        let await_runtime: std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
            h.runtime.clone();
        async move { Ok(await_js(&await_runtime, value).await) }
    }))
    .unwrap()
}
pub fn wire(h: &host::Harness) -> Value {
    Value::Array(h.http.sent.lock().unwrap().iter().map(|r| {
        let query=r.url.split_once('?').map(|(_,q)|q).unwrap_or("").split('&').filter(|p|!p.is_empty() && !["timestamp=","recvWindow=","signature="].iter().any(|key|p.starts_with(key))).map(str::to_owned).collect::<Vec<_>>();
        json!({"method":r.method,"path":r.url.split('?').next().unwrap(),"base":r.base_url,"query":query})
    }).collect())
}
pub fn labels(input: &Value) -> (&str, &str) {
    (
        input["key"].as_str().unwrap(),
        input["family"].as_str().unwrap_or("OrderSide"),
    )
}
pub fn sync_body(rt: &crate::inverse::Rt, args: &[Js]) -> Result<Js, Thrown> {
    Ok(Js::array(vec![
        rt.this.get("acceptedLabel")?,
        Js::Number(args.len() as f64),
        safe_cex_exchange::js_runtime::value::arg(args, 0),
    ]))
}
pub fn async_body(
    rt: crate::inverse::Rt,
    args: Vec<Js>,
) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
    Box::pin(async move { sync_body(&rt, &args) })
}
pub fn installed(h: &host::Harness, name: &str, input: &Value) -> Value {
    let opaque = raw(input);
    let v = h
        .exchange
        .receiver()
        .get(name)
        .unwrap()
        .call(Js::Null, vec![opaque.clone(), Js::Undefined])
        .unwrap();
    let v = if matches!(v, Js::Promise(_)) {
        await_value(h, v).unwrap()
    } else {
        v
    };
    json!({"label":v.get("0").unwrap().to_display(),"arityBits":number(v.get("1").unwrap().as_f64().unwrap()),"same":v.get("2").unwrap().strict_equals(&opaque)})
}
pub fn scalar_case(input: &Value) -> Value {
    input.clone()
}
pub fn new_parts(
    input: &Value,
) -> (
    Rc<host::Runtime>,
    std::sync::Arc<host::Http>,
    Rc<safe_cex_store::store::DefaultStore>,
    safe_cex_types::ExchangeOptions,
) {
    let runtime = Rc::new(host::Runtime::default());
    runtime.now.set(1700000000000.0);
    runtime.headers_capable.set(true);
    let http = std::sync::Arc::new(host::Http::default());
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let mut options = host::options();
    options.testnet = input["testnet"].as_bool();
    (runtime, http, store, options)
}
