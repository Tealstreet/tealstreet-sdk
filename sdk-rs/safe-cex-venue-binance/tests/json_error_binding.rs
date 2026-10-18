#![cfg(feature = "native")]
#[path = "support/inverse_rest_runtime.rs"]
mod host;
use safe_cex_exchange::{exchange::Exchange, js_runtime::value::Js};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_binance::exchange_classes::{BinanceExchange, BinancePMExchange};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
// A native HTTP terminal rejects non-success responses with RestFailure;
// a fulfilled RestResponse(status=400) would script a successful request.
struct RejectedHttp {
    msg: Value,
    requests: Mutex<Vec<RestRequest>>,
}
impl RestAdapter for RejectedHttp {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(request.clone());
        let response = RestResponse {
            status: 400,
            reason: Some("Bad Request".into()),
            headers: Default::default(),
            body: RestResponseBody::Encoded(serde_json::to_vec(&json!({"msg":self.msg})).unwrap()),
        };
        Box::pin(async move {
            Err(RestFailure {
                code: Some("ERR_BAD_REQUEST".into()),
                message: "Request failed with status code 400".into(),
                transport_error: None,
                response: Some(Box::new(response)),
                request: Box::new(request),
                context: None,
            })
        })
    }
}
fn drive<T>(runtime: &host::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
    }
    panic!("bounded terminal HTTP witness did not settle");
}
fn observe(pm: bool, profile: &str) -> Value {
    let runtime = Rc::new(host::Runtime::default());
    let msg = match profile {
        "object" => json!({"name":"VenueError","message":"raw venue reason","detail":17}),
        "number" => json!(7),
        "string" => json!("plain venue reason"),
        _ => panic!("unknown finite source profile"),
    };
    let endpoint = if pm {
        "/papi/v1/um/leverage"
    } else {
        "/fapi/v1/leverage"
    };
    let http = Arc::new(RejectedHttp {
        msg,
        requests: Mutex::new(vec![]),
    });
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        http.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let store = Rc::new(DefaultStore::new());
    Store::update(store.as_ref(), serde_json::from_value(json!({
        "markets":[{"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.1},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":50}}}],
        "positions":[{"id":"witness-position","symbol":"BTCUSDT","side":"long","entryPrice":100,"notional":100,"contracts":1,"leverage":10,"unrealizedPnl":0,"liquidationPrice":0}]
    })).unwrap(), false).unwrap();
    let events = Rc::new(RefCell::new(Vec::<Js>::new()));
    let options: ExchangeOptions = serde_json::from_value(json!({"key":"synthetic-key","secret":"synthetic-secret","tuning":{"consoleLogging":false}})).unwrap();
    fn invoke<E: Exchange>(
        subject: &E,
        runtime: &host::Runtime,
        events: &Rc<RefCell<Vec<Js>>>,
    ) -> f64 {
        let seen = events.clone();
        subject.base().on(
            "error",
            Rc::new(move |_, value| {
                seen.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        drive(runtime, subject.set_leverage("BTCUSDT".into(), 20.0, None)).unwrap();
        let positions = subject.base().store.positions();
        assert_eq!(positions.len(), 1);
        let leverage = positions[0].leverage;
        assert_eq!(
            leverage.to_bits(),
            10.0_f64.to_bits(),
            "rejected write leaves the Store unchanged"
        );
        drop(positions);
        subject.dispose().unwrap();
        leverage
    }
    let leverage = if pm {
        let subject = BinancePMExchange::new(
            ExchangeName::Binancepm,
            options,
            store.clone(),
            dependencies,
        )
        .unwrap();
        invoke(&subject, &runtime, &events)
    } else {
        let subject =
            BinanceExchange::new(ExchangeName::Binance, options, store.clone(), dependencies)
                .unwrap();
        invoke(&subject, &runtime, &events)
    };
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests[0].url.split('?').next(), Some(endpoint));
    assert_eq!(requests.len(), 1, "a failed write never retries");
    assert!(requests[0].method.eq_ignore_ascii_case("POST"));
    assert!(
        requests[0].url.contains("signature="),
        "actual configured signer ran"
    );
    let values = events.borrow();
    let units = values
        .last()
        .expect("real Base error event")
        .to_js_string()
        .unwrap()
        .to_vec();
    json!({"stem":if pm {"binancepm"} else {"binance"},"profile":profile,"messageUnits":units,"requests":requests.len(),"leverageBits":format!("{:016x}",leverage.to_bits())})
}
fn assert_messages(pm: bool) {
    for (profile, message) in [
        ("object", "raw venue reason"),
        ("number", "7"),
        ("string", "Exchange Error: plain venue reason"),
    ] {
        let actual = observe(pm, profile);
        assert_eq!(
            actual["messageUnits"],
            json!(message.encode_utf16().collect::<Vec<_>>()),
            "constructed native family/profile: {} {profile}",
            if pm { "PM" } else { "Futures" }
        );
    }
}
#[test]
fn futures_leverage_error_messages_keep_raw_json_shape() {
    assert_messages(false);
}
#[test]
fn pm_leverage_error_messages_keep_raw_json_shape() {
    assert_messages(true);
}
#[test]
#[ignore = "executes the whole maintained TypeScript constructor and real Axios/Store"]
fn maintained_json_error_binding_matches_native() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/json-error-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 6);
    for row in expected {
        let actual = observe(row["stem"] == "binancepm", row["profile"].as_str().unwrap());
        assert_eq!(actual, row);
    }
}
