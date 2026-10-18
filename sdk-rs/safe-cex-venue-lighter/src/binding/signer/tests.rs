//! The signer world through the real realm globals, against the offline
//! TypeScript/WASM capture in tests/fixtures/signing-preimages.json.
use crate::binding::{
    globals,
    realm::{LighterExtra, Realm},
    value::Microtasks,
};
use num_bigint::BigInt;
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        globals::{install_builtins, install_enums},
        library::install_library,
        promise::{JsFuture, Promise, await_js},
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

const NOW: f64 = 1_700_000_000_000.0;

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
        NOW
    }
    fn now_ms(&self) -> f64 {
        NOW
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected background exception: {error}")
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker request")
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unexpected tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("unexpected dial")
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
struct Http(Rc<Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("the signer sends through its apiClient, never the realm HTTP host")
    }
}

/// A realm built the way LighterNativeExchange::new builds one, with the
/// capture's fixed 0x42 entropy in place of the host CSPRNG.
fn realm() -> (Realm, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    let shared = Rc::new_cyclic(|weak| {
        let globals = Js::object(vec![]);
        install_builtins(&globals, weak.clone());
        install_enums(&globals);
        install_library(&globals, weak.clone());
        ScopeShared::new(
            runtime.clone(),
            Rc::new(Http(runtime.clone())),
            Rc::new(DefaultStore::new()),
            globals,
            LighterExtra {
                microtasks: Microtasks::new(runtime.clone()),
                supers: Default::default(),
                registry: Default::default(),
            },
        )
    });
    globals::install(&shared.globals).unwrap();
    let realm = Realm(shared);
    super::install_with_entropy(
        &realm,
        Rc::new(|bytes| {
            bytes.fill(0x42);
            Ok(())
        }),
    )
    .unwrap();
    (realm, runtime)
}

pub(super) fn classifier_host() -> (Realm, super::Host) {
    let (realm, _) = realm();
    let host = super::Host::new(&realm);
    (realm, host)
}

fn settle(runtime: &Runtime, value: Js) -> Result<Js, Thrown> {
    let mut future = Box::pin(await_js(value));
    for _ in 0..10_000 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
        for mut job in jobs {
            if job
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                runtime.jobs.borrow_mut().push(job);
            }
        }
    }
    panic!("promise never settled")
}

fn fixture() -> &'static Value {
    static FIXTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/signing-preimages.json"
        ))
        .unwrap()
    })
}

fn case(name: &str) -> &'static Value {
    fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("fixture case {name}"))
}

/// Fixture JSON (with `$bigint`/`$undefined` tags) as a JS value.
fn from_fixture(value: &Value) -> Js {
    match value {
        Value::Object(map) if map.contains_key("$bigint") => {
            Js::BigInt(Rc::new(map["$bigint"].as_str().unwrap().parse().unwrap()))
        }
        Value::Object(map) if map.contains_key("$undefined") => Js::Undefined,
        Value::Object(map) => Js::object(
            map.iter()
                .map(|(k, v)| (k.as_str(), from_fixture(v)))
                .collect(),
        ),
        Value::Array(items) => Js::array(items.iter().map(from_fixture).collect()),
        Value::String(s) => Js::str(s),
        Value::Bool(b) => Js::Bool(*b),
        Value::Number(n) => Js::Number(n.as_f64().unwrap()),
        Value::Null => Js::Null,
    }
}

/// A JS value in the fixture's tagged JSON shape, own keys in order.
fn to_fixture(value: &Js) -> Value {
    match value {
        Js::BigInt(n) => json!({ "$bigint": n.to_string() }),
        Js::Undefined => json!({ "$undefined": true }),
        Js::Number(n) if n.fract() == 0.0 && n.abs() < 9e15 => json!(*n as i64),
        Js::Number(n) => json!(n),
        Js::String(s) => json!(String::from_utf16_lossy(s)),
        Js::Bool(b) => json!(b),
        Js::Null => Value::Null,
        Js::Array(_) => Value::Array(
            value
                .array_values()
                .unwrap()
                .iter()
                .map(to_fixture)
                .collect(),
        ),
        _ => Value::Object(
            value
                .own_entries()
                .unwrap()
                .into_iter()
                .map(|(k, v)| (String::from_utf16_lossy(&k), to_fixture(&v)))
                .collect(),
        ),
    }
}

fn keys(value: &Js) -> Vec<String> {
    value
        .own_keys()
        .iter()
        .map(|k| String::from_utf16_lossy(k))
        .collect()
}

#[derive(Default)]
struct Calls {
    nonce: RefCell<Vec<Value>>,
    sent: RefCell<Vec<Js>>,
}

/// LighterApiClient stand-in: getNextNonce answers 7n, sendRawTx a fixed hash.
fn api_client(runtime: &Rc<Runtime>, calls: &Rc<Calls>) -> Js {
    let rt: Rc<dyn ExchangeRuntime> = runtime.clone();
    let (nonce_rt, nonce_calls) = (rt.clone(), calls.clone());
    let (send_rt, send_calls) = (rt, calls.clone());
    Js::object(vec![
        (
            "getNextNonce",
            Js::function("getNextNonce", move |_, a| {
                nonce_calls
                    .nonce
                    .borrow_mut()
                    .push(Value::Array(a.iter().map(to_fixture).collect()));
                let nonce = Js::BigInt(Rc::new(BigInt::from(7)));
                Ok(Js::Promise(Promise::settled(&nonce_rt, Ok(nonce))))
            }),
        ),
        (
            "sendRawTx",
            Js::function("sendRawTx", move |_, a| {
                send_calls.sent.borrow_mut().push(a[0].clone());
                Ok(Js::Promise(Promise::settled(
                    &send_rt,
                    Ok(Js::str("0xhash")),
                )))
            }),
        ),
    ])
}

fn adapter(
    realm: &Realm,
    runtime: &Rc<Runtime>,
    calls: &Rc<Calls>,
    provider: Js,
) -> Result<Js, Thrown> {
    let params = Js::object(vec![
        ("exchangeName", Js::str("lighter")),
        (
            "options",
            Js::object(vec![
                ("secret", Js::str(fixture()["secret"].as_str().unwrap())),
                ("testnet", Js::Bool(false)),
                (
                    "extra",
                    Js::object(vec![("lighterSignerProvider", provider)]),
                ),
            ]),
        ),
        ("apiClient", api_client(runtime, calls)),
        ("accountIndex", Js::BigInt(Rc::new(BigInt::from(42)))),
        ("apiKeyIndex", Js::Number(3.0)),
    ]);
    let create = realm.global("createLighterSignerAdapter")?;
    settle(runtime, create.call(Js::Undefined, vec![params])?)
}

fn sdk_result(name: &str) -> &'static Value {
    &case(name)["sdkResults"][0]["result"]
}

#[test]
fn poseidon_index_contract_and_wasm_compatible_crypto() {
    let (realm, runtime) = realm();
    let get = realm.global("getCryptoApi").unwrap();
    let before = get
        .call(Js::Undefined, vec![])
        .expect_err("getCryptoApi before initWasm");
    assert_eq!(
        before.message(),
        "WASM not initialized. Call initWasm() first.",
        "getCryptoApi before initWasm"
    );
    let ready = || {
        realm
            .global("isWasmReady")
            .unwrap()
            .call(Js::Undefined, vec![])
            .unwrap()
    };
    assert!(!ready().truthy(), "isWasmReady before initWasm");
    let init = realm
        .global("initWasm")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap();
    assert!(matches!(init, Js::Promise(_)), "initWasm returns a promise");
    assert!(
        settle(&runtime, init).unwrap().is_undefined(),
        "initWasm resolves undefined"
    );
    assert!(ready().truthy(), "isWasmReady true after initWasm");
    let api = get.call(Js::Undefined, vec![]).unwrap();
    assert_eq!(
        keys(&api),
        [
            "isReady",
            "poseidon2Hash",
            "poseidon2HashFp5",
            "poseidon2HashTwoToOne",
            "schnorrGenerateKeys",
            "schnorrPkFromSk",
            "schnorrSign",
            "schnorrVerify"
        ],
        "PoseidonCrypto member order"
    );
    let call = |name: &str, args: Vec<Js>| api.call_method(name, args).unwrap();
    let secret = Js::str(fixture()["secret"].as_str().unwrap());
    assert_eq!(
        to_fixture(&call("schnorrPkFromSk", vec![secret.clone()])),
        fixture()["publicKey"],
        "schnorrPkFromSk of the capture key"
    );
    for (name, method) in [
        ("lighter/livenet/cancel-large-index", "poseidon2Hash"),
        ("lighter/livenet/create-complete", "poseidon2HashFp5"),
    ] {
        for crypto in case(name)["crypto"].as_array().unwrap() {
            let input = Js::str(crypto["inputs"][0].as_str().unwrap());
            let actual = match crypto["method"].as_str().unwrap() {
                "schnorrSign" => call("schnorrSign", vec![input, secret.clone()]),
                other => {
                    assert_eq!(other, method);
                    call(other, vec![input])
                }
            };
            assert_eq!(
                to_fixture(&actual),
                crypto["result"],
                "{name} {method} WASM parity"
            );
        }
    }
    let signature = call(
        "schnorrSign",
        vec![Js::str("0000000000000000"), secret.clone()],
    );
    let public = call("schnorrPkFromSk", vec![secret]);
    assert!(
        call(
            "schnorrVerify",
            vec![Js::str("0000000000000000"), public, signature]
        )
        .truthy(),
        "schnorrVerify accepts its own signature"
    );
    assert_eq!(
        to_fixture(&call("poseidon2Hash", vec![Js::str("0g")])),
        json!({"error": "invalid hex string: encoding/hex: invalid byte: U+0067 'g'"}),
        "go hex invalid-byte text"
    );
    assert_eq!(
        to_fixture(&call("poseidon2Hash", vec![Js::str("000")])),
        json!({"error": "invalid hex string: encoding/hex: odd length hex string"}),
        "go hex odd-length text"
    );
    assert_eq!(
        to_fixture(&call("poseidon2Hash", vec![Js::str("0000")])),
        json!({"error": "input length must be multiple of 8 bytes"}),
        "go word alignment text"
    );
}

#[test]
fn create_order_signs_the_captured_transaction() {
    let (realm, runtime) = realm();
    let calls = Rc::new(Calls::default());
    let signer = adapter(&realm, &runtime, &calls, Js::str("tealstreet-sdk"))
        .expect("createLighterSignerAdapter resolves");
    assert!(
        calls.nonce.borrow().is_empty(),
        "initialize allocates no nonce"
    );
    let name = "lighter/livenet/create-complete";
    let request = from_fixture(&case(name)["outer"]["args"][0]);
    let result = settle(
        &runtime,
        signer.call_method("createOrder", vec![request]).unwrap(),
    )
    .expect("createOrder resolves sendRawTx");
    assert_eq!(
        to_fixture(&result),
        json!("0xhash"),
        "createOrder resolves sendRawTx"
    );
    assert_eq!(
        *calls.nonce.borrow(),
        [case(name)["nonceRequest"].clone()],
        "nonce request args"
    );
    let sent = calls.sent.borrow();
    assert_eq!(sent.len(), 1, "one sendRawTx call");
    assert_eq!(
        to_fixture(&sent[0]),
        *sdk_result(name),
        "create order tx_info"
    );
    assert_eq!(
        keys(&sent[0]),
        [
            "type",
            "data",
            "signature",
            "accountIndex",
            "apiKeyIndex",
            "nonce",
            "expiredAt",
            "orderDetails"
        ],
        "create order key order"
    );
}

#[test]
fn cancel_signs_the_captured_transaction_then_serves_cached_nonce() {
    let (realm, runtime) = realm();
    let calls = Rc::new(Calls::default());
    let signer = adapter(&realm, &runtime, &calls, Js::Undefined)
        .expect("createLighterSignerAdapter resolves");
    let name = "lighter/livenet/cancel-large-index";
    let request = from_fixture(&case(name)["outer"]["args"][0]);
    let result = settle(
        &runtime,
        signer.call_method("cancelOrder", vec![request]).unwrap(),
    )
    .expect("cancelOrder resolves sendRawTx");
    assert_eq!(
        to_fixture(&result),
        json!("0xhash"),
        "cancelOrder resolves sendRawTx"
    );
    assert_eq!(
        to_fixture(&calls.sent.borrow()[0]),
        *sdk_result(name),
        "cancel tx_info"
    );

    let next = from_fixture(&case("lighter/livenet/create-missing-optionals")["outer"]["args"][0]);
    settle(
        &runtime,
        signer.call_method("createOrder", vec![next]).unwrap(),
    )
    .expect("second createOrder resolves");
    assert_eq!(
        calls.nonce.borrow().len(),
        1,
        "second nonce served from cache"
    );
    let sent = calls.sent.borrow();
    assert_eq!(
        to_fixture(&sent[1].get("nonce").unwrap()),
        json!({"$bigint": "8"}),
        "cached nonce increments"
    );
}

#[test]
fn auth_token_matches_capture() {
    let (realm, runtime) = realm();
    let calls = Rc::new(Calls::default());
    let signer = adapter(&realm, &runtime, &calls, Js::Undefined)
        .expect("createLighterSignerAdapter resolves");
    let name = "lighter/livenet/auth";
    let token = settle(
        &runtime,
        signer
            .call_method("getAuthToken", vec![Js::Number(3600.0)])
            .unwrap(),
    );
    let token = token.expect("getAuthToken resolves");
    assert_eq!(to_fixture(&token), case(name)["result"], "auth token");
    assert!(
        calls.nonce.borrow().is_empty(),
        "auth token allocates no nonce"
    );
}

#[test]
fn unsupported_provider_rejects_before_construction() {
    let (realm, runtime) = realm();
    let calls = Rc::new(Calls::default());
    let error = adapter(&realm, &runtime, &calls, Js::str("legacy"))
        .expect_err("unsupported provider rejects");
    assert_eq!(
        error.message(),
        "Unsupported Lighter signer provider \"legacy\". Legacy third-party signer providers are removed; use tealstreet-sdk.",
        "unsupported provider"
    );
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn parameter_validation_precedence_matches_live_transaction_client() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/binding-signer-validation-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 9, "independent source validation floor");
    for case in cases {
        let params = from_fixture(&case["params"]);
        let result = if case["kind"] == "leverage" {
            super::codec::update_leverage(&params).map(|_| ())
        } else {
            super::codec::create_grouped_orders(&params).map(|_| ())
        };
        let error = result.expect_err(case["label"].as_str().unwrap());
        assert_eq!(
            to_fixture(&error.value.get("name").unwrap()),
            case["error"]["name"],
            "{} error name",
            case["label"]
        );
        assert_eq!(
            error.message(),
            case["error"]["message"].as_str().unwrap(),
            "{} validation order",
            case["label"]
        );
    }
}
