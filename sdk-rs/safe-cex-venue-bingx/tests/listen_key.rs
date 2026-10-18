use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_venue_bingx::{data_reads::ReadTransport, listen_key::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/listen_key_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "BINGX-KEY source {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("fixture must resolve within 1000 polls")
}
fn number(n: f64) -> Value {
    json!({"number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => number(js_value_as_f64(v).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(v) => {
            let mut keys = v.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&v[k])))
                    .collect(),
            )
        }
        Value::Array(v) => Value::Array(v.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}
type Timer = Rc<dyn Fn() -> Result<(), ExchangeError>>;
struct Host {
    input: Value,
    disposed: Cell<bool>,
    calls: RefCell<Vec<Value>>,
    timers: RefCell<Vec<Timer>>,
    timer_counter: Cell<u64>,
    socket_counter: Cell<u64>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    reports: RefCell<Vec<ExchangeError>>,
    hold_post: Cell<bool>,
    release_post: Cell<bool>,
}
impl Host {
    fn call(&self, value: Value) {
        self.calls.borrow_mut().push(value);
    }
    fn drain(&self) {
        for _ in 0..1000 {
            let tasks = self.tasks.take();
            if tasks.is_empty() {
                return;
            }
            for mut future in tasks {
                match future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                {
                    Poll::Ready(r) => r.unwrap(),
                    Poll::Pending => self.tasks.borrow_mut().push(future),
                }
            }
        }
        panic!("fixture tasks failed to settle")
    }
}
fn listener(l: PrivateListener) -> &'static str {
    match l {
        PrivateListener::Message => "message",
        PrivateListener::Close => "close",
        PrivateListener::Open => "open",
    }
}
impl ListenKeyHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn testnet(&self) -> bool {
        self.input["testnet"] == true
    }
    fn create_socket(&self, url: &str) -> Result<u64, ExchangeError> {
        self.call(json!(["create", url]));
        if self.input["createError"] == true {
            return Err(ExchangeError::new("create cause"));
        }
        let socket = self.socket_counter.get() + 1;
        self.socket_counter.set(socket);
        Ok(socket)
    }
    fn add_listener(&self, s: u64, l: PrivateListener) -> Result<(), ExchangeError> {
        self.call(json!(["add", s, listener(l)]));
        Ok(())
    }
    fn remove_listener(&self, s: u64, l: PrivateListener) -> Result<(), ExchangeError> {
        self.call(json!(["remove", s, listener(l)]));
        Ok(())
    }
    fn close_socket(&self, s: u64) -> Result<(), ExchangeError> {
        self.call(json!(["close", s]));
        if self.input["closeError"] == true {
            Err(ExchangeError::new("close cause"))
        } else {
            Ok(())
        }
    }
    fn retry_failed_connect(&self, message: &str) -> Result<(), ExchangeError> {
        self.call(json!(["retry", message]));
        if self.input["retryError"] == true {
            Err(ExchangeError::new("retry cause"))
        } else {
            Ok(())
        }
    }
    fn warning(&self, message: &str) -> Result<(), ExchangeError> {
        self.call(json!(["warning", message]));
        Ok(())
    }
    fn set_timeout(&self, delay: f64, timer: Timer) -> Result<u64, ExchangeError> {
        let id = self.timer_counter.get() + 1;
        self.timer_counter.set(id);
        self.call(json!(["setTimeout", delay, id]));
        self.timers.borrow_mut().push(timer);
        Ok(id)
    }
    fn clear_timeout(&self, t: u64) -> Result<(), ExchangeError> {
        self.call(json!(["clearTimeout", t]));
        Ok(())
    }
    fn spawn(&self, mut future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(r) => r,
            Poll::Pending => {
                self.tasks.borrow_mut().push(future);
                Ok(())
            }
        }
    }
    fn monotonic_now_ms(&self) -> Result<f64, ExchangeError> {
        Ok(1234.0)
    }
    fn set_ping_at(&self, n: f64) -> Result<(), ExchangeError> {
        self.call(json!(["pingAt", n]));
        Ok(())
    }
    fn send_with_log(&self, frame: &str) -> Result<(), ExchangeError> {
        self.call(json!(["send", frame]));
        Ok(())
    }
    fn base_dispose(&self) -> Result<(), ExchangeError> {
        self.call(json!(["disposeBase"]));
        if self.input["disposeError"] == true {
            return Err(ExchangeError::new("dispose cause"));
        }
        self.disposed.set(true);
        Ok(())
    }
    fn report_exception(&self, error: &ExchangeError) {
        self.reports.borrow_mut().push(error.clone());
    }
}
impl ReadTransport for Host {
    fn send(&self, request: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        assert_eq!(request.url, "/openApi/user/auth/userDataStream");
        assert!(request.body.is_none());
        let post = request.method == "POST";
        let error = if post {
            assert!(request.params.is_none());
            self.call(json!(["post", request.url]));
            &self.input["postError"]
        } else {
            assert_eq!(request.method, "PUT");
            self.call(json!(["put",request.url,{"undefined":true},{"params":request.params}]));
            &self.input["putError"]
        };
        Box::pin(async move {
            if post && self.hold_post.get() {
                std::future::poll_fn(|cx| {
                    if self.release_post.get() {
                        Poll::Ready(())
                    } else {
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                })
                .await;
            }
            if !error.is_null() {
                let mut e = ExchangeError::new(error["message"].as_str().unwrap());
                e = e.with_retained_response(
                    (json!({"data":{"msg":error["msg"]},"status":error["status"]})).into(),
                );
                e.store_subscriber_ids = vec![71, 83];
                return Err(e);
            }
            if post && self.input["disposeDuringPost"] == true {
                self.disposed.set(true);
            }
            let value = if post {
                self.input["post"].clone()
            } else {
                json!({})
            };
            Ok(serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap())
        })
    }
}
fn fixture(input: &Value) -> (Rc<BingxListenKey>, Rc<Host>) {
    let host = Rc::new(Host {
        input: input.clone(),
        disposed: Cell::new(input["disposed"] == true),
        calls: RefCell::default(),
        timers: RefCell::default(),
        timer_counter: Cell::new(100),
        socket_counter: Cell::new(80),
        tasks: RefCell::default(),
        reports: RefCell::default(),
        hold_post: Cell::new(false),
        release_post: Cell::new(false),
    });
    let venue = BingxListenKey::new(host.clone(), host.clone());
    *venue.listen_key.borrow_mut() = input.get("key").filter(|v| !v.is_null()).cloned();
    venue.refresh_timer.set(input["timer"].as_u64());
    venue.socket.set(input["socket"].as_u64());
    venue.connecting.set(input["connecting"] == true);
    venue.rebuilding.set(input["rebuilding"] == true);
    (venue, host)
}
fn state(v: &BingxListenKey, h: &Host) -> Value {
    json!({"key":v.listen_key.borrow().as_ref().map_or(json!({"undefined":true}),observe),"timer":v.refresh_timer.get().map_or(json!({"undefined":true}),|n|number(n as f64)),"socket":v.socket.get().map_or(json!({"undefined":true}),|n|number(n as f64)),"connecting":v.connecting.get(),"rebuilding":v.rebuilding.get(),"disposed":h.disposed.get()})
}
fn run(case: &Value) {
    let (v, host) = fixture(&case["input"]);
    let method = case["method"].as_str().unwrap();
    let result: Result<Value, ExchangeError> = match method {
        "fetchListenKey" => ready(v.fetch_listen_key()).map(|v| observe(&v)),
        "connectAndSubscribe" => {
            ready(v.connect_and_subscribe()).map(|_| json!({"undefined":true}))
        }
        "onOpen" => v.on_open().map(|_| json!({"undefined":true})),
        "updateListenKey" => ready(v.update_listen_key()).map(|_| json!({"undefined":true})),
        "rebuildWithFreshListenKey" => {
            ready(v.rebuild_with_fresh_listen_key()).map(|_| json!({"undefined":true}))
        }
        "scheduleListenKeyRefresh" => v
            .schedule_listen_key_refresh(case["input"]["delay"].as_f64())
            .map(|_| json!({"undefined":true})),
        "scheduleListenKeyRecovery" => v
            .schedule_listen_key_recovery()
            .map(|_| json!({"undefined":true})),
        "clearListenKeyRefreshTimer" => v.clear_timer().map(|_| json!({"undefined":true})),
        "dispose" => v.dispose().map(|_| json!({"undefined":true})),
        _ => unreachable!(),
    };
    let output = match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => {
            if method == "fetchListenKey" && !case["input"]["postError"].is_null() {
                assert_eq!(
                    e.store_subscriber_ids,
                    vec![71, 83],
                    "BINGX-KEY original transport error identity"
                );
            }
            json!({"error":{"name":e.name,"message":e.message}})
        }
    };
    let variant = &case["variant"];
    assert_eq!(
        output, case["output"],
        "BINGX-KEY native {method} variant {variant}"
    );
    assert_eq!(
        observe(&json!(*host.calls.borrow())),
        case["calls"],
        "BINGX-KEY operation order {method} variant {variant}"
    );
    assert_eq!(
        state(&v, &host),
        case["state"],
        "BINGX-KEY native state {method} variant {variant}"
    );
}
#[test]
fn listen_key_matches_actual_typescript() {
    let names = [
        "fetchListenKey",
        "connectAndSubscribe",
        "onOpen",
        "updateListenKey",
        "rebuildWithFreshListenKey",
        "scheduleListenKeyRefresh",
        "scheduleListenKeyRecovery",
        "clearListenKeyRefreshTimer",
        "dispose",
    ];
    assert_eq!(corpus()["names"], json!(names));
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 36, "BINGX-KEY case floor");
    let inputs = json!(
        cases
            .iter()
            .map(|c| json!([c["method"], c["variant"], c["input"]]))
            .collect::<Vec<_>>()
    );
    let encoded = serde_json::to_string(&sorted(&inputs)).unwrap();
    let fingerprint = encoded.bytes().fold(0xcbf29ce484222325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(
        fingerprint, 0x632c05493eab5a18,
        "BINGX-KEY independent input fingerprint"
    );
    let mut counts = [0; 9];
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["method"], names[i / 4]);
        assert_eq!(c["variant"], json!(i % 4));
        counts[i / 4] += 1;
        run(c);
    }
    assert_eq!(counts, [4; 9]);
    println!("BINGX-KEY runtime {}", corpus()["runtime"]);
}
#[test]
fn listen_key_recovery_errors_and_utf16_regex_match_source() {
    let extras = corpus()["extras"].as_array().unwrap();
    assert_eq!(extras.len(), 11);
    for c in extras {
        run(c);
    }
    let regex = corpus()["regex"].as_array().unwrap();
    assert_eq!(regex.len(), 8);
    for c in regex {
        run(c);
    }
    assert_eq!(
        extras[4]["state"]["rebuilding"], true,
        "BINGX-KEY source pre-try close failure retains flag"
    );
    assert_eq!(
        extras[3]["state"]["rebuilding"], false,
        "BINGX-KEY caught create failure clears flag"
    );
}
#[test]
fn timer_callback_executes_local_update_body() {
    let (v, host) = fixture(&corpus()["cases"][20]["input"]);
    v.schedule_listen_key_refresh(Some(17.5)).unwrap();
    let callback = host.timers.borrow()[0].clone();
    callback().unwrap();
    host.drain();
    assert_eq!(
        observe(&json!(*host.calls.borrow())),
        corpus()["timer"]["calls"],
        "BINGX-KEY timer operation order"
    );
    assert_eq!(state(&v, &host), corpus()["timer"]["state"]);
}
#[test]
fn connecting_flag_serializes_overlapping_fetches() {
    let (v, host) = fixture(&corpus()["cases"][4]["input"]);
    host.hold_post.set(true);
    let mut first = std::pin::pin!(v.connect_and_subscribe());
    assert!(
        first
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert!(v.connecting.get());
    ready(v.connect_and_subscribe()).unwrap();
    assert_eq!(
        host.calls
            .borrow()
            .iter()
            .filter(|c| c[0] == "post")
            .count(),
        1
    );
    host.release_post.set(true);
    ready(first).unwrap();
    assert_eq!(corpus()["concurrent"]["before"], true);
    assert_eq!(
        observe(&json!(*host.calls.borrow())),
        corpus()["concurrent"]["calls"]
    );
    assert_eq!(state(&v, &host), corpus()["concurrent"]["state"]);
}
#[test]
fn opaque_clear_preserves_token_error_and_assignment_order() {
    let token = Rc::new("opaque");
    let slot = RefCell::new(Some(token.clone()));
    let same = Cell::new(false);
    let snapshot = slot.borrow().clone();
    clear_refresh_timer::<_, ()>(
        snapshot,
        |_| true,
        |arg| {
            same.set(Rc::ptr_eq(&arg, &token));
            assert!(slot.borrow().is_some());
            Ok(())
        },
        || {
            *slot.borrow_mut() = None;
            Ok(())
        },
    )
    .unwrap();
    assert!(same.get());
    assert!(slot.borrow().is_none());
    let error = Rc::new("error");
    *slot.borrow_mut() = Some(token.clone());
    let snapshot = slot.borrow().clone();
    let out = clear_refresh_timer(
        snapshot,
        |_| true,
        |_| Err(error.clone()),
        || panic!("clear failure must not unset"),
    )
    .unwrap_err();
    assert!(Rc::ptr_eq(&out, &error));
    assert!(Rc::ptr_eq(slot.borrow().as_ref().unwrap(), &token));
    assert_eq!(
        corpus()["opaque"],
        json!({"same":true,"errorSame":true,"retained":true})
    );
    clear_refresh_timer::<_, ()>(
        Some(0),
        |id| *id != 0,
        |_| panic!("falsy token clear"),
        || panic!("falsy token unset"),
    )
    .unwrap();
}
#[test]
fn timer_callback_rejects_released_native_owner() {
    let (v, host) = fixture(&corpus()["cases"][20]["input"]);
    v.schedule_listen_key_recovery().unwrap();
    let timer = host.timers.borrow()[0].clone();
    drop(v);
    assert_eq!(
        timer().unwrap_err().message,
        "BingX listen-key owner released"
    );
}
#[test]
fn configured_json_key_is_an_owned_snapshot() {
    let mut input = corpus()["cases"][0]["input"].clone();
    input["post"] = json!({"listenKey":{"token":"original"}});
    let (v, _) = fixture(&input);
    let mut returned = ready(v.fetch_listen_key()).unwrap();
    returned["token"] = json!("changed");
    assert_eq!(
        corpus()["aliasToken"],
        "changed",
        "BINGX-KEY source returned key aliases state"
    );
    assert_eq!(
        v.listen_key.borrow().as_ref().unwrap()["token"],
        "original",
        "BINGX-KEY CHOSEN owned key snapshot"
    );
}
#[test]
fn recovery_timer_executes_local_rebuild_body() {
    let (v, host) = fixture(&corpus()["cases"][24]["input"]);
    v.schedule_listen_key_recovery().unwrap();
    let timer = host.timers.borrow()[0].clone();
    timer().unwrap();
    host.drain();
    assert_eq!(
        observe(&json!(*host.calls.borrow())),
        corpus()["recoveryTimer"]["calls"],
        "BINGX-KEY recovery timer operation order"
    );
    assert_eq!(state(&v, &host), corpus()["recoveryTimer"]["state"]);
}
