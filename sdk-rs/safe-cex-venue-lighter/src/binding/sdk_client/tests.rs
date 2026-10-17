//! LighterApiClient/OrderApi/CandlestickApi over a scripted JsHttpHost.
use super::*;
use crate::binding::exchange::{LighterDependencies, build_realm};
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{from_json, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::Store;
use safe_cex_transport::{TransportProvider, TransportRegistry, socket::TunnelHost};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::Arc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        1_000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn report_raw_exception(&self, _: &Value, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("no tunnel in sdk client tests")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("no sockets in sdk client tests")
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

/// Records every wire config and answers with the next scripted `data`.
struct Http {
    runtime: Rc<Runtime>,
    sent: RefCell<Vec<Value>>,
    replies: RefCell<VecDeque<Value>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.sent.borrow_mut().push(to_json(&config).unwrap());
        let data = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("scripted reply");
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(200.0)),
                ("data", from_json(&data)),
            ]))
        })
    }
}

struct World {
    runtime: Rc<Runtime>,
    http: Rc<Http>,
    realm: Realm,
    emitted: Rc<RefCell<Vec<String>>>,
    client: Js,
}

impl World {
    fn new() -> Self {
        let runtime = Rc::new(Runtime::default());
        let http = Rc::new(Http {
            runtime: runtime.clone(),
            sent: RefCell::default(),
            replies: RefCell::default(),
        });
        let deps = LighterDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            registry: Rc::new(TransportRegistry::default()),
            signing_entropy: None,
        };
        let store: Rc<dyn Store> = Rc::new(safe_cex_store::store::DefaultStore::new());
        let options = serde_json::from_value(json!({ "key": "", "secret": "" })).unwrap();
        let realm = build_realm(&ExchangeName::Lighter, &options, &store, &deps)
            .unwrap_or_else(|e| panic!("realm: {}", e.message()));
        let emitted: Rc<RefCell<Vec<String>>> = Rc::default();
        let log = emitted.clone();
        let exchange = Js::object(vec![
            ("name", Js::str("lighter")),
            (
                "emitAndLogError",
                Js::function("emitAndLogError", move |_, a| {
                    log.borrow_mut().push(arg(&a, 0).to_display());
                    Ok(Js::Undefined)
                }),
            ),
        ]);
        let client = realm
            .global("createLighterSDKClient")
            .unwrap()
            .call(Js::Undefined, vec![Js::object(vec![]), exchange])
            .unwrap();
        Self {
            runtime,
            http,
            realm,
            emitted,
            client,
        }
    }
    fn reply(&self, data: Value) {
        self.http.replies.borrow_mut().push_back(data);
    }
    fn sent(&self) -> Vec<Value> {
        self.http.sent.borrow().clone()
    }
    fn settle(&self, value: Js) -> Result<Js, Thrown> {
        let mut future = Box::pin(await_js(value));
        for _ in 0..64 {
            if let Poll::Ready(v) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                return v;
            }
            let jobs = std::mem::take(&mut *self.runtime.jobs.borrow_mut());
            for mut job in jobs {
                if job
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    self.runtime.jobs.borrow_mut().push(job);
                }
            }
        }
        panic!("promise never settled")
    }
    /// `JSON.stringify` through the realm: what the TS caller would serialize.
    fn json(&self, value: &Js) -> String {
        let text = self.realm.global("JSON.stringify").unwrap();
        text.call(Js::Undefined, vec![value.clone()])
            .unwrap()
            .to_display()
    }
    fn call(&self, member: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.settle(self.client.call_method(member, args).unwrap())
    }
}

fn big(n: i64) -> Js {
    Js::BigInt(Rc::new(BigInt::from(n)))
}

fn form(body: &Value) -> Vec<(String, String)> {
    form_urlencoded::parse(body.as_str().unwrap().as_bytes())
        .into_owned()
        .collect()
}

#[test]
fn get_strips_unsupported_headers_and_returns_data() {
    let w = World::new();
    w.reply(json!({ "success": true, "data": { "accountIndex": 7 } }));
    let headers = Js::object(vec![
        ("X-Timestamp", Js::str("1")),
        ("x-account-index", Js::str("2")),
        ("Keep", Js::str("yes")),
    ]);
    let config = Js::object(vec![
        ("params", Js::object(vec![("by", Js::str("index"))])),
        ("headers", headers),
    ]);
    let data = w
        .call("get", vec![Js::str("/api/v1/account"), config])
        .unwrap();
    assert_eq!(
        w.json(&data),
        r#"{"success":true,"data":{"accountIndex":7}}"#
    );
    let sent = &w.sent()[0];
    assert_eq!(sent["url"], "/api/v1/account");
    assert_eq!(sent["method"], "get");
    assert_eq!(sent["params"], json!({ "by": "index" }));
    assert_eq!(
        sent["headers"],
        json!({ "Content-Type": "application/json", "Keep": "yes" })
    );
    let base = w.client.call_method("getBaseUrl", vec![]).unwrap();
    assert_eq!(sent["baseURL"], to_json(&base).unwrap());
    assert_eq!(sent["timeout"], crate::types::RECV_WINDOW as f64);
    assert!(w.emitted.borrow().is_empty());
}

#[test]
fn post_sends_data_and_config() {
    let w = World::new();
    w.reply(json!({ "ok": 1 }));
    let data = Js::object(vec![("tier", Js::str("premium"))]);
    let out = w
        .call(
            "post",
            vec![Js::str("/api/v1/changeAccountTier"), data, Js::Undefined],
        )
        .unwrap();
    assert_eq!(w.json(&out), r#"{"ok":1}"#);
    let sent = &w.sent()[0];
    assert_eq!(sent["method"], "post");
    assert_eq!(sent["url"], "/api/v1/changeAccountTier");
    assert_eq!(sent["data"], json!({ "tier": "premium" }));
}

#[test]
fn response_interceptor_emits_and_throws_on_error_payloads() {
    let w = World::new();
    w.reply(json!({ "error": { "code": 21500, "message": "bad nonce" } }));
    let err = w.call("get", vec![Js::str("/api/v1/x")]).unwrap_err();
    assert_eq!(err.message(), "[21500] bad nonce");
    w.reply(json!({ "success": false }));
    let err = w.call("get", vec![Js::str("/api/v1/y")]).unwrap_err();
    assert_eq!(err.message(), "Request failed");
    assert_eq!(*w.emitted.borrow(), ["[21500] bad nonce", "Request failed"]);
}

fn cancel_tx(order: i64) -> Js {
    Js::object(vec![
        ("type", Js::str("cancel_order")),
        ("accountIndex", big(7)),
        ("apiKeyIndex", Js::Number(3.0)),
        ("nonce", big(9)),
        ("signature", Js::str("0x0102ff01")),
        (
            "cancelDetails",
            Js::object(vec![
                ("orderId", big(order)),
                ("marketId", Js::Number(1.0)),
                ("expiredAt", big(1000)),
            ]),
        ),
    ])
}

const CANCEL_INFO: &str = r#"{"AccountIndex":7,"ApiKeyIndex":3,"MarketIndex":1,"Index":5,"ExpiredAt":1000,"Nonce":9,"Sig":"AQL/AQ=="}"#;

#[test]
fn send_raw_tx_posts_the_serialized_form_and_returns_the_hash() {
    let w = World::new();
    w.reply(json!({ "code": 200, "tx_hash": "0xabc" }));
    let hash = w.call("sendRawTx", vec![cancel_tx(5)]).unwrap();
    assert_eq!(hash.to_display(), "0xabc");
    let sent = &w.sent()[0];
    assert_eq!(sent["method"], "post");
    assert_eq!(sent["url"], "/api/v1/sendTx");
    assert_eq!(
        sent["headers"]["Content-Type"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(
        form(&sent["data"]),
        [
            ("tx_type".to_owned(), "15".to_owned()),
            ("tx_info".to_owned(), CANCEL_INFO.to_owned()),
        ]
    );
    w.reply(json!({ "code": 400, "message": "rejected" }));
    let err = w.call("sendRawTx", vec![cancel_tx(5)]).unwrap_err();
    assert_eq!(
        err.value.get("name").unwrap().to_display(),
        "LighterApiError"
    );
    assert_eq!(err.value.get("code").unwrap().to_display(), "400");
    assert_eq!(err.message(), "rejected");
}

#[test]
fn send_raw_tx_batch_posts_both_lists_and_normalizes_the_reply() {
    let w = World::new();
    let empty = w.call("sendRawTxBatch", vec![Js::array(vec![])]).unwrap();
    assert_eq!(w.json(&empty), r#"{"txHashes":[]}"#);
    assert!(w.sent().is_empty());
    w.reply(json!({ "code": 200, "tx_hash": ["a", "b"], "predicted_execution_time_ms": 5 }));
    let out = w
        .call(
            "sendRawTxBatch",
            vec![Js::array(vec![cancel_tx(5), cancel_tx(5)])],
        )
        .unwrap();
    assert_eq!(
        w.json(&out),
        r#"{"txHashes":["a","b"],"predictedExecutionTimeMs":5}"#
    );
    assert!(out.has_own("volumeQuotaRemaining"));
    let sent = &w.sent()[0];
    assert_eq!(sent["url"], "/api/v1/sendTxBatch");
    let infos = serde_json::to_string(&[CANCEL_INFO, CANCEL_INFO]).unwrap();
    assert_eq!(
        form(&sent["data"]),
        [
            ("tx_types".to_owned(), "[15,15]".to_owned()),
            ("tx_infos".to_owned(), infos),
        ]
    );
}

#[test]
fn get_next_nonce_reads_the_axios_payload() {
    let w = World::new();
    w.reply(json!({ "code": 200, "nonce": 12 }));
    let nonce = w
        .call("getNextNonce", vec![big(7), Js::Number(3.0)])
        .unwrap();
    assert!(nonce.strict_equals(&big(12)));
    let sent = &w.sent()[0];
    assert_eq!(sent["url"], "/api/v1/nextNonce");
    assert_eq!(sent["params"]["account_index"], "7");
    assert_eq!(sent["params"]["api_key_index"], 3.0);
}

#[test]
fn order_api_order_book_orders_merges_params_and_normalizes() {
    let w = World::new();
    let api = w
        .realm
        .global("OrderApi")
        .unwrap()
        .call(Js::Undefined, vec![w.client.clone()])
        .unwrap();
    assert!(api.get("client").unwrap().same_reference(&w.client));
    w.reply(json!({ "code": 200, "bids": [{ "price": "1" }], "timestamp": 77 }));
    let config = Js::object(vec![
        (
            "params",
            Js::object(vec![("limit", Js::str("5")), ("x", Js::str("1"))]),
        ),
        ("__suppressExchangeError", Js::Bool(true)),
    ]);
    let book = w
        .settle(
            api.call_method(
                "getOrderBookOrders",
                vec![Js::Number(3.0), Js::Number(100.0), config],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        w.json(&book),
        r#"{"marketId":3,"bids":[{"price":"1"}],"asks":[],"timestamp":77}"#
    );
    let sent = &w.sent()[0];
    assert_eq!(sent["url"], "/api/v1/orderBookOrders");
    assert_eq!(
        sent["params"],
        json!({ "limit": "5", "x": "1", "market_id": "3" })
    );
    assert_eq!(sent["__suppressExchangeError"], true);
}

#[test]
fn candlestick_api_retains_its_client() {
    let w = World::new();
    let api = w
        .realm
        .global("CandlestickApi")
        .unwrap()
        .call(Js::Undefined, vec![w.client.clone()])
        .unwrap();
    assert!(api.get("client").unwrap().same_reference(&w.client));
    assert!(api.get("getCandlesticks").unwrap().is_callable());
}

#[test]
fn put_delete_and_config_accessors() {
    let w = World::new();
    w.reply(json!({ "put": 1 }));
    w.reply(json!({ "deleted": 1 }));
    let data = Js::object(vec![("a", Js::Number(1.0))]);
    let put = w.call("put", vec![Js::str("/p"), data]).unwrap();
    assert_eq!(w.json(&put), r#"{"put":1}"#);
    let params = Js::object(vec![("params", Js::object(vec![("q", Js::str("1"))]))]);
    let deleted = w.call("delete", vec![Js::str("/d"), params]).unwrap();
    assert_eq!(w.json(&deleted), r#"{"deleted":1}"#);
    let sent = w.sent();
    assert_eq!(
        (&sent[0]["method"], &sent[0]["url"]),
        (&json!("put"), &json!("/p"))
    );
    assert_eq!(sent[0]["data"], json!({ "a": 1.0 }));
    assert_eq!(
        (&sent[1]["method"], &sent[1]["url"]),
        (&json!("delete"), &json!("/d"))
    );
    assert_eq!(sent[1]["params"], json!({ "q": "1" }));
    let config = w.client.call_method("getConfig", vec![]).unwrap();
    let own = w.client.get("config").unwrap();
    assert!(!config.same_reference(&own));
    assert_eq!(w.json(&config), w.json(&own));
    assert_eq!(config.get("debug").unwrap().to_display(), "false");
    let interceptors = w.client.get("interceptors").unwrap();
    for kind in ["request", "response", "error"] {
        let list = interceptors.get(kind).unwrap();
        assert_eq!(list.get("length").unwrap().to_display(), "1", "{kind}");
    }
}

#[test]
fn api_keys_and_transfer_fee_reads() {
    let w = World::new();
    w.reply(json!({ "success": true, "data": { "apiKeys": [{ "k": 1 }] } }));
    let keys = w.call("getApiKeys", vec![big(7), Js::Number(3.0)]).unwrap();
    assert_eq!(w.json(&keys), r#"[{"k":1}]"#);
    w.reply(json!({ "success": true, "data": {
        "fee": "1", "minAmount": "2", "maxAmount": "3", "feePercentage": 0.5 } }));
    let fee = w
        .call("getTransferFeeInfo", vec![big(7), big(8), Js::str("tok")])
        .unwrap();
    assert!(fee.get("fee").unwrap().strict_equals(&big(1)));
    assert!(fee.get("maxAmount").unwrap().strict_equals(&big(3)));
    assert_eq!(fee.get("feePercentage").unwrap().to_display(), "0.5");
    let sent = w.sent();
    assert_eq!(sent[0]["url"], "/api/v1/accounts/7/api-keys");
    assert_eq!(sent[0]["params"]["apiKeyIndex"], 3.0);
    assert_eq!(sent[1]["url"], "/api/v1/accounts/7/transfer-fee");
    assert_eq!(sent[1]["params"], json!({ "toAccountIndex": "8" }));
    assert_eq!(sent[1]["headers"]["Authorization"], "Bearer tok");
}

fn construct(w: &World, class: &str) -> Js {
    w.realm
        .global(class)
        .unwrap()
        .call(Js::Undefined, vec![w.client.clone()])
        .unwrap()
}

#[test]
fn order_api_ordinary_queries() {
    let w = World::new();
    let api = construct(&w, "OrderApi");
    let options = Js::object(vec![
        ("limit", Js::Number(10.0)),
        ("marketId", Js::Number(1.0)),
        (
            "betweenTimestamps",
            Js::array(vec![Js::Number(1.0), Js::Number(2.0)]),
        ),
        ("cursor", Js::str("c")),
    ]);
    let trades = Js::object(vec![("accountIndex", big(7))]);
    let cases: Vec<(&str, Vec<Js>, &str, Value)> = vec![
        (
            "getAccountActiveOrders",
            vec![big(7), Js::Number(2.0)],
            "/api/v1/accountActiveOrders",
            json!({ "account_index": "7", "market_id": "2" }),
        ),
        (
            "getAccountInactiveOrders",
            vec![big(7), options],
            "/api/v1/accountInactiveOrders",
            json!({ "account_index": "7", "limit": "10", "market_id": "1",
                "start_time": "1", "end_time": "2", "cursor": "c" }),
        ),
        (
            "getOrderBooks",
            vec![Js::Number(4.0)],
            "/api/v1/order-books",
            json!({ "market_id": "4" }),
        ),
        (
            "getOrderBookDetails",
            vec![],
            "/api/v1/order-books/details",
            json!({}),
        ),
        (
            "getRecentTrades",
            vec![Js::Number(3.0)],
            "/api/v1/trades/recent/3",
            json!({ "limit": "50" }),
        ),
        (
            "getTrades",
            vec![trades],
            "/api/v1/trades",
            json!({ "limit": "50", "account_index": "7" }),
        ),
        ("getExchangeStats", vec![], "/api/v1/stats", Value::Null),
    ];
    for (i, (method, args, url, params)) in cases.into_iter().enumerate() {
        w.reply(json!({ "success": true, "data": { "n": i } }));
        let out = w.settle(api.call_method(method, args).unwrap()).unwrap();
        assert_eq!(w.json(&out), format!(r#"{{"n":{i}}}"#), "{method}");
        let sent = &w.sent()[i];
        assert_eq!(sent["url"], url, "{method}");
        assert_eq!(sent["params"], params, "{method}");
    }
    // `success: false` never gets here: the SDK client interceptor throws first.
    w.reply(json!({ "success": true, "data": null }));
    let err = w
        .settle(api.call_method("getOrderBooks", vec![]).unwrap())
        .unwrap_err();
    assert_eq!(err.message(), "Failed to get order books");
}

#[test]
fn candlestick_api_reads_and_lighter_api_error() {
    let w = World::new();
    let api = construct(&w, "CandlestickApi");
    let args = || {
        vec![
            Js::Number(1.0),
            Js::str("1h"),
            Js::Number(10.0),
            Js::Number(20.0),
            Js::Number(5.0),
        ]
    };
    w.reply(json!({ "success": true, "data": { "candlesticks": [] } }));
    let out = w
        .settle(api.call_method("getCandlesticks", args()).unwrap())
        .unwrap();
    assert_eq!(w.json(&out), r#"{"candlesticks":[]}"#);
    let sent = &w.sent()[0];
    assert_eq!(sent["url"], "/api/v1/candlesticks");
    assert_eq!(
        sent["params"],
        json!({ "marketId": 1.0, "resolution": "1h", "startTimestamp": 10.0,
            "endTimestamp": 20.0, "countBack": 5.0 })
    );
    w.reply(json!({ "success": true }));
    let err = w
        .settle(api.call_method("getFundings", args()).unwrap())
        .unwrap_err();
    assert_eq!(w.sent()[1]["url"], "/api/v1/fundings");
    assert_eq!(err.message(), "Failed to get fundings");
    assert_eq!(
        err.value.get("name").unwrap().to_display(),
        "LighterApiError"
    );
    assert_eq!(
        err.value.get("code").unwrap().to_display(),
        "GET_FUNDINGS_FAILED"
    );
    let class = w.realm.global("LighterApiError").unwrap();
    let error = class.call(Js::Undefined, vec![Js::str("m")]).unwrap();
    assert_eq!(error.get("code").unwrap().to_display(), "UNKNOWN_ERROR");
    assert!(error.get("details").unwrap().is_undefined());
}
fn signature_input(label: &str) -> Js {
    match label {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "false" => Js::Bool(false),
        "true" => Js::Bool(true),
        "zero" => Js::Number(0.),
        "one" => Js::Number(1.),
        "empty" => Js::str(""),
        "text" => Js::str("yes"),
        "array" => Js::array(vec![]),
        "object" => Js::object(vec![]),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "negative-zero" => Js::Number(-0.),
        "big-zero" => big(0),
        "big-one" => big(1),
        "symbol" => Js::SymbolIterator,
        "function" => Js::function("input", |_, _| panic!("flags are never coerced")),
        _ => panic!("unknown flag input {label}"),
    }
}

#[test]
fn transaction_client_preserves_raw_l1_signature_metadata_and_wire_truthiness() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/signer-l1-signature-oracle.cjs"))
        .output().expect("maintained SDK with real WASM crypto");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 17);
    let w = World::new();
    let init = w
        .realm
        .global("initWasm")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap();
    w.settle(init).unwrap();
    let crypto = w
        .realm
        .global("getCryptoApi")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap();
    let client=w.realm.global("TransactionClient").unwrap().call(Js::Undefined,vec![Js::object(vec![
        ("chainId",Js::Number(304.)),("accountIndex",big(42)),("apiKeyIndex",Js::Number(3.)),
        ("privateKey",Js::str("01000000000000000000000000000000000000000000000000000000000000000000000000000000")),
        ("httpClient",w.client.clone()),("crypto",crypto),
    ])]).unwrap();
    w.reply(json!({"code":200,"nonce":7}));

    let mut results=vec![];
    for case in source.as_array().unwrap() {
        let label=case["label"].as_str().unwrap();
        let raw=signature_input(label);
        let params=Js::object(vec![
            ("integratorAccountIndex",big(8)),("maxPerpsTakerFee",Js::Number(250.)),
            ("maxPerpsMakerFee",Js::Number(0.)),("maxSpotTakerFee",Js::Number(300.)),
            ("maxSpotMakerFee",Js::Number(0.)),("approvalExpiry",big(1000000)),
            ("l1Signature",raw.clone()),
        ]);
        let result=w.settle(client.call_method("getApproveIntegratorTransaction",vec![params]).unwrap());
        let observed=match result {
            Ok(tx)=>{
                assert!(!tx.get("signature").unwrap().as_str().unwrap().is_empty());
                let value=tx.get("approveIntegratorDetails").unwrap().get("l1Signature").unwrap();
                let nan=matches!(value,Js::Number(n) if n.is_nan());
                let retained=value.strict_equals(&raw)||(nan&&matches!(raw,Js::Number(n) if n.is_nan()));
                if !raw.truthy() {
                    w.reply(json!({"code":200,"tx_hash":"proof"}));
                    assert_eq!(w.call("sendRawTx",vec![tx.clone()]).unwrap().to_display(),"proof");
                    let sent=w.sent();
                    let body=form(&sent.last().unwrap()["data"]);
                    let wire: Value=serde_json::from_str(&body.iter().find(|(k,_)|k=="tx_info").unwrap().1).unwrap();
                    assert!(wire.get("L1Sig").is_none(), "falsey {label} omitted from actual request");
                }
                json!({"label":label,"data":tx.get("data").unwrap().to_display(),"retained":retained,
                    "type":value.type_of(),"negativeZero":matches!(value,Js::Number(n) if n.to_bits()==(-0_f64).to_bits()),"nan":nan})
            }
            Err(error)=>json!({"label":label,"error":error.message()}),
        };
        results.push(observed);
    }
    assert_eq!(results.len(),17);
    assert_eq!(results,source.as_array().unwrap().clone());
}
