//! Structural alias witnesses through a constructed LighterNativeExchange.
use super::*;
use num_bigint::BigInt;
use safe_cex_exchange::{
    js_runtime::{client::JsHttpHost, promise::JsFuture},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_transport::{TransportProvider, TransportRegistry, socket::TunnelHost};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

// Public deterministic key from tests/fixtures/signing-preimages.json.
const TEST_SECRET: &str =
    "01000000000000000000000000000000000000000000000000000000000000000000000000000000";

#[derive(Default)]
struct Runtime {
    now: Cell<f64>,
    logs: RefCell<Vec<String>>,
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
        self.now.get()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, severity: &str, args: &[Js]) {
        self.logs.borrow_mut().push(format!(
            "{severity}: {}",
            safe_cex_exchange::runtime::console_text(args)
        ));
    }
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
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
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
    sent: RefCell<Vec<Js>>,
    replies: RefCell<VecDeque<Js>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.sent.borrow_mut().push(config);
        let data = self
            .replies
            .borrow_mut()
            .pop_front()
            .expect("scripted reply");
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(200.0)),
                ("data", data),
            ]))
        })
    }
}

struct World {
    runtime: Rc<Runtime>,
    http: Rc<Http>,
    venue: Rc<LighterNativeExchange>,
}
impl World {
    fn new() -> Self {
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(1_700_000_000_000.0);
        let http = Rc::new(Http {
            runtime: runtime.clone(),
            sent: RefCell::default(),
            replies: RefCell::default(),
        });
        let options = serde_json::from_value(
            json!({"key":"", "secret":TEST_SECRET, "apiKeyIndex":"3", "applicationId":"42"}),
        )
        .unwrap();
        let venue = LighterNativeExchange::new(
            ExchangeName::Lighter,
            options,
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
            LighterDependencies {
                runtime: runtime.clone(),
                http: http.clone(),
                registry: Rc::new(TransportRegistry::default()),
                signing_entropy: Some(Rc::new(|bytes| {
                    bytes.fill(0x42);
                    Ok(())
                })),
            },
        )
        .expect("actual parent constructor");
        Self {
            runtime,
            http,
            venue,
        }
    }
    fn member(&self, name: &str) -> Js {
        self.venue.this.borrow().get(name).unwrap()
    }
    fn settle(&self, value: Js) -> Result<Js, Thrown> {
        let runtime: Rc<dyn ExchangeRuntime> = self.runtime.clone();
        let mut future = Box::pin(await_js(&runtime, value));
        for _ in 0..128 {
            if let Poll::Ready(value) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                return value;
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
        panic!("native promise did not settle");
    }
}

#[test]
fn structural_alias_query_keeps_native_values_through_real_sdk_and_http() {
    let world = World::new();
    let cursor = Js::utf16(&[0xd800, 0x61, 0xdc00]);
    let identity = Js::object(vec![("native", Js::Number(f64::INFINITY))]);
    let response = Js::object(vec![("original", identity.clone())]);
    world.http.replies.borrow_mut().push_back(Js::object(vec![
        ("success", Js::Bool(true)),
        ("data", response.clone()),
    ]));
    let promise = world
        .member("orderApi")
        .call_method(
            "getAccountInactiveOrders",
            vec![
                Js::BigInt(Rc::new(BigInt::from(42))),
                Js::object(vec![
                    ("cursor", cursor.clone()),
                    ("sortBy", identity.clone()),
                    ("sortOrder", Js::Number(f64::INFINITY)),
                ]),
            ],
        )
        .unwrap();
    let result = world.settle(promise).unwrap();
    assert!(
        result.strict_equals(&response),
        "native response object identity"
    );
    let sent = world.http.sent.borrow();
    assert_eq!(sent.len(), 1);
    let params = sent[0].get("params").unwrap();
    assert!(
        params.get("cursor").unwrap().strict_equals(&cursor),
        "UTF16 record cursor code units"
    );
    assert!(
        params.get("sort_by").unwrap().strict_equals(&identity),
        "native query object identity"
    );
    assert!(
        matches!(params.get("sort_order").unwrap(), Js::Number(n) if n == f64::INFINITY),
        "native query Infinity"
    );
    assert_eq!(params.get("account_index").unwrap().to_display(), "42");
}

#[test]
fn structural_alias_private_warning_map_is_shared_and_keeps_cooldown() {
    let world = World::new();
    let events: Rc<RefCell<Vec<Vec<Js>>>> = Rc::default();
    let sink = events.clone();
    world.venue.base().on_args(
        "warning",
        Rc::new(move |_, args| {
            sink.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    let socket = world.member("privateWebsocket");
    let class = world.venue.realm.global("LighterPrivateWebsocket").unwrap();
    let map = class.get("readonlyFallbackWarningAtByAccount").unwrap();
    let warn = || {
        socket
            .call_method("emitReadonlyFallbackWarning", vec![Js::Bool(true)])
            .unwrap()
    };
    let before = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
    warn();
    let after = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
    let Js::Map(times) = &map else {
        panic!("actual class warning map");
    };
    let first = times.borrow().clone();
    assert_eq!(first.len(), 1, "first warning records one account");
    assert!(
        matches!(&first[0].1, Js::Number(n) if *n >= before && *n <= after),
        "class view mirrors native virtual warning timestamp"
    );
    assert_eq!(events.borrow().len(), 1, "actual parent emits one warning");
    assert_eq!(
        events.borrow()[0][0].to_display(),
        crate::ws_private_warnings::GEORESTRICTED_MESSAGE
    );
    assert!(events.borrow()[0][1].get("forceToast").unwrap().truthy());
    let count = world.runtime.logs.borrow().len();
    let sibling = class
        .construct(vec![world.venue.this.borrow().clone()], None)
        .unwrap();
    sibling
        .call_method("emitReadonlyFallbackWarning", vec![Js::Bool(true)])
        .unwrap();
    assert_eq!(
        world.runtime.logs.borrow().len(),
        count,
        "one class cooldown spans distinct socket instances"
    );
    warn();
    assert_eq!(
        world.runtime.logs.borrow().len(),
        count,
        "shared native cooldown suppresses repeated warning"
    );
    world.runtime.now.set(world.runtime.now.get() + 30_000.0);
    warn();
    assert_eq!(
        world.runtime.logs.borrow().len(),
        count,
        "raw host clock cannot bypass virtual-clock cooldown"
    );
    assert!(
        class
            .get("readonlyFallbackWarningAtByAccount")
            .unwrap()
            .strict_equals(&map),
        "same class map identity"
    );
    assert!(
        times.borrow()[0].1.strict_equals(&first[0].1),
        "cooldown preserves timestamp in retained view"
    );
    assert_eq!(
        events.borrow().len(),
        1,
        "cooldown suppresses both log and emit"
    );
}

#[test]
fn structural_alias_exported_validation_returns_original_native_outcomes() {
    use crate::sdk_validation::{ValidationFailure, ValidationValue, validate_account_index};
    let valid = ValidationValue::BigInt(BigInt::from(42));
    assert!(
        validate_account_index(&valid).is_ok(),
        "native exported valid account"
    );
    let invalid = ValidationValue::BigInt(BigInt::from(-1));
    let failure = validate_account_index(&invalid).expect_err("native negative account");
    let ValidationFailure::Validation(error) = failure else {
        panic!("original native validation variant");
    };
    assert_eq!(String::from_utf16(&error.name).unwrap(), "ValidationError");
    assert_eq!(
        String::from_utf16(&error.message).unwrap(),
        "Account index must be non-negative, got -1"
    );
    let failure =
        validate_account_index(&ValidationValue::Number(42.0)).expect_err("number is not bigint");
    assert!(
        matches!(failure, ValidationFailure::Validation(error) if String::from_utf16(&error.message).unwrap() == "Account index must be a BigInt, got number")
    );
}

#[test]
fn structural_alias_transaction_promises_build_two_orders_with_one_native_nonce_read() {
    let world = World::new();
    let realm = &world.venue.realm;
    world
        .settle(
            realm
                .global("initWasm")
                .unwrap()
                .call(Js::Undefined, vec![])
                .unwrap(),
        )
        .unwrap();
    let crypto = realm
        .global("getCryptoApi")
        .unwrap()
        .call(Js::Undefined, vec![])
        .unwrap();
    let big = |n: i64| Js::BigInt(Rc::new(BigInt::from(n)));
    let client = realm
        .global("TransactionClient")
        .unwrap()
        .construct(
            vec![Js::object(vec![
                ("httpClient", world.member("lighterApiClient")),
                ("crypto", crypto),
                ("chainId", Js::Number(304.0)),
                ("privateKey", Js::str(TEST_SECRET)),
                ("accountIndex", big(42)),
                ("apiKeyIndex", Js::Number(3.0)),
            ])],
            None,
        )
        .unwrap();
    world.http.replies.borrow_mut().push_back(Js::object(vec![
        ("code", Js::Number(200.0)),
        ("nonce", Js::Number(17.0)),
    ]));
    let order = |id| {
        Js::object(vec![
            ("orderId", big(id)),
            ("marketId", Js::Number(1.0)),
            ("isBuy", Js::Bool(true)),
            ("price", big(500_000)),
            ("amount", big(5000)),
        ])
    };
    let result = world
        .settle(
            client
                .call_method(
                    "getCreateOrderTransactions",
                    vec![Js::array(vec![order(100), order(101)])],
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        result.get("length").unwrap().as_f64(),
        Some(2.0),
        "native batch promise settles both cells"
    );
    let first = result.get("0").unwrap();
    let second = result.get("1").unwrap();
    for (tx, nonce, id) in [(&first, 17, 100), (&second, 18, 101)] {
        assert!(
            tx.get("nonce").unwrap().strict_equals(&big(nonce)),
            "native nonce queue increments once per transaction"
        );
        assert!(
            tx.get("orderDetails")
                .unwrap()
                .get("orderId")
                .unwrap()
                .strict_equals(&big(id)),
            "native batch order identity"
        );
        assert_eq!(
            tx.get("signature").unwrap().as_str().unwrap().len(),
            160,
            "native Schnorr signature uses all 80 bytes"
        );
    }
    assert!(
        !first.strict_equals(&second),
        "separate transaction cells remain distinct"
    );
    let sent = world.http.sent.borrow();
    assert_eq!(
        sent.len(),
        1,
        "real SDK caches the nonce across batch awaits"
    );
    assert_eq!(
        sent[0].get("url").unwrap().to_display(),
        "/api/v1/nextNonce"
    );
}
