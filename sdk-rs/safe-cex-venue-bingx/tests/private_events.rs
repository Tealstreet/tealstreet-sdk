#[path = "support/oracle_clock.rs"]
mod oracle_clock;
#[path = "support/retained_observe.rs"]
mod retained_observe;

use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::*;
use safe_cex_venue_bingx::{
    data_reads::{BingxDataReads, ReadTransport},
    private_events::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private_events_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "BINGX-PRIVATE source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("fixture must resolve in 1000 polls")
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
fn order(v: &Order) -> Value {
    let mut out = json!({"id":v.id,"status":v.status,"symbol":v.symbol,"type":v.r#type,"side":v.side,"price":number((v.price).to_number()),"amount":number(v.amount),"filled":number(v.filled),"remaining":number(v.remaining),"reduceOnly":v.reduce_only,"lastUpdate":number(v.last_update)});
    if let Some(b) = v.close.into_value() {
        out["close"] = json!(b);
    }
    if let Some(t) = v.time_in_force.into_value() {
        out["timeInForce"] = json!(t);
    }
    if let Some(t) = v.tp_trigger_type.into_value() {
        out["tpTriggerType"] = json!(t);
    }
    if let Some(t) = v.sl_trigger_type.into_value() {
        out["slTriggerType"] = json!(t);
    }
    if let Some(info) = v.info.as_ref() {
        out["info"] = retained_observe::observe(info);
    }
    assert!(
        v.parent_id.is_none()
            && v.trigger_type.is_none()
            && v.take_profit.is_none()
            && v.stop_loss.is_none()
            && v.extra.is_none(),
        "BINGX-PRIVATE native order shape"
    );
    out
}
fn position(v: &Position) -> Value {
    let mut out = json!({"id":v.id,"symbol":v.symbol,"side":v.side,"entryPrice":number(v.entry_price),"notional":number(v.notional),"leverage":number(v.leverage),"unrealizedPnl":number(v.unrealized_pnl),"contracts":number(v.contracts),"liquidationPrice":number(v.liquidation_price)});
    if let Some(m) = v.margin_type.into_value() {
        out["marginType"] = json!(m);
    }
    if let Some(m) = v.position_mode.into_value() {
        out["positionMode"] = json!(m);
    }
    if let Some(n) = v.margin_size.into_value() {
        out["marginSize"] = number(n);
    }
    if let Some(info) = v.info.as_ref() {
        out["info"] = retained_observe::observe(info);
    }
    assert!(
        v.realized_pnl.is_none() && v.extra.is_none() && v.last_update.is_none(),
        "BINGX-PRIVATE native position shape"
    );
    out
}
type Timer = (f64, Rc<dyn Fn() -> Result<(), ExchangeError>>);
struct Host {
    input: Value,
    now: Cell<f64>,
    disposed: Cell<bool>,
    calls: RefCell<Vec<Value>>,
    fills: RefCell<Vec<Value>>,
    timers: RefCell<Vec<Timer>>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    exceptions: RefCell<Vec<ExchangeError>>,
    exchange_exceptions: RefCell<Vec<ExchangeError>>,
}
impl Host {
    fn call(&self, v: Value) {
        self.calls.borrow_mut().push(v);
    }
    fn drain(&self) {
        for _ in 0..1000 {
            let tasks = self.tasks.take();
            if tasks.is_empty() {
                return;
            }
            for mut f in tasks {
                match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Ready(r) => r.unwrap(),
                    Poll::Pending => self.tasks.borrow_mut().push(f),
                }
            }
        }
        panic!("task fixture failed to settle")
    }
}
impl PrivateEventHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<(), ExchangeError> {
        self.timers.borrow_mut().push((delay, callback));
        Ok(())
    }
    fn spawn(&self, mut f: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError> {
        match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(r) => r,
            Poll::Pending => {
                self.tasks.borrow_mut().push(f);
                Ok(())
            }
        }
    }
    fn emit_fill(
        &self,
        payload: safe_cex_types::retained_json::RetainedJsonInput,
    ) -> Result<(), ExchangeError> {
        self.fills
            .borrow_mut()
            .push(retained_observe::observe(&payload));
        Ok(())
    }
    fn report_exception(&self, error: &ExchangeError) {
        self.exceptions.borrow_mut().push(error.clone());
    }
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("no exchange timers")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no exchange tasks")
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        if severity == "error" {
            self.call(json!([
                "error",
                message.strip_prefix("exchange error ").unwrap()
            ]));
        }
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
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.exchange_exceptions.borrow_mut().push(error.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no workers")
    }
}
impl ReadTransport for Host {
    fn send(&self, request: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        assert_eq!(request.method, "GET");
        assert!(request.params.is_none() && request.body.is_none());
        self.call(json!(["get", request.url]));
        let result = if request.url == "/openApi/swap/v1/positionSide/dual" {
            if self.input["modeFailure"] == true {
                Err(ExchangeError::new("mode cause"))
            } else {
                Ok(json!({"code":0,"data":{"dualSidePosition":self.input["mode"]}}))
            }
        } else {
            assert_eq!(request.url, "/openApi/swap/v2/user/positions");
            if self.input["restFailure"] == true {
                Err(ExchangeError::new("positions cause"))
            } else {
                Ok(json!({"code":0,"data":self.input["restRows"]}))
            }
        };
        Box::pin(async move {
            result.map(|v| serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap())
        })
    }
}
fn fixture(input: &Value) -> (Rc<BingxPrivateEvents>, Rc<DefaultStore>, Rc<Host>) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![
                serde_json::from_value(input["market"].clone()).unwrap(),
            ]),
            positions: Some(serde_json::from_value(input["positions"].clone()).unwrap()),
            orders: Some(serde_json::from_value(input["orders"].clone()).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let host = Rc::new(Host {
        input: input.clone(),
        now: Cell::new(input["now"].as_f64().unwrap()),
        disposed: Cell::new(input["disposed"] == true),
        calls: RefCell::default(),
        fills: RefCell::default(),
        timers: RefCell::default(),
        tasks: RefCell::default(),
        exceptions: RefCell::default(),
        exchange_exceptions: RefCell::default(),
    });
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        store.clone(),
        host.clone(),
        ExchangeSettings::default(),
    ));
    let venue = BingxPrivateEvents::new(
        Rc::new(BingxDataReads::new(base, host.clone())),
        host.clone(),
    );
    venue
        .last_position_fetch_time
        .set(input["last"].as_f64().unwrap());
    venue
        .position_fetch_throttle_ms
        .set(input["throttle"].as_f64().unwrap());
    venue
        .position_fetch_scheduled
        .set(input["scheduled"] == true);
    venue
        .has_pending_position_events
        .set(input["pending"] == true);
    *venue.emitted_fill_qty_by_order_id.borrow_mut() =
        serde_json::from_value(input["emitted"].clone()).unwrap();
    (venue, store, host)
}
fn state(v: &BingxPrivateEvents, h: &Host) -> Value {
    json!({"last":number(v.last_position_fetch_time.get()),"scheduled":v.position_fetch_scheduled.get(),"pending":v.has_pending_position_events.get(),"emitted":observe(&json!(*v.emitted_fill_qty_by_order_id.borrow())),"timers":h.timers.borrow().iter().map(|(n,_)|number(*n)).collect::<Vec<_>>()})
}
fn run(c: &Value) {
    let method = c["method"].as_str().unwrap();
    let input = &c["input"];
    let (v, store, host) = fixture(input);
    let events: Vec<safe_cex_types::retained_json::RetainedJsonInput> = input
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            vec![if method == "handleAccountEvents" {
                input["account"].clone()
            } else {
                json!({"o":input["wireOrder"]})
            }]
        })
        .into_iter()
        .map(Into::into)
        .collect();
    let clock = oracle_clock::ClockWindow::start();
    let result: Result<Value, ExchangeError> = match method {
        "mapOrder" => map_order(
            &input["wireOrder"].clone().into(),
            &store.orders().unwrap(),
            5678.0,
        )
        .map(|o| o.as_ref().map_or(json!({"undefined":true}), order)),
        "normalizeSymbol" => v
            .normalize_symbol(Some(&input["symbol"].clone().into()))
            .map(Value::String),
        "extractAccountPositions" => Ok(Value::Array(
            v.extract_account_positions(&input["account"].clone().into())
                .iter()
                .map(retained_observe::observe)
                .collect(),
        )),
        "schedulePositionRefresh" => v
            .schedule_position_refresh()
            .map(|_| json!({"undefined":true})),
        "executePositionRefresh" => {
            ready(v.execute_position_refresh());
            Ok(json!({"undefined":true}))
        }
        "handleOrderEvents" => v
            .handle_order_events(&events)
            .map(|_| json!({"undefined":true})),
        "handleAccountEvents" => {
            ready(v.handle_account_events(&events)).map(|_| json!({"undefined":true}))
        }
        "ping" => private_ping(
            host.disposed.get(),
            || Ok(1234.0),
            |n| {
                host.call(json!(["pingAt", n]));
                Ok(())
            },
            |frame| {
                host.call(json!(["send", frame]));
                if input["failSend"] == true {
                    Err(ExchangeError::new("send cause"))
                } else {
                    Ok(())
                }
            },
        )
        .map(|_| json!({"undefined":true})),
        _ => unreachable!(),
    };
    host.drain();
    let clock = clock.finish();
    let mut native_orders = json!(
        store
            .orders()
            .unwrap()
            .iter()
            .map(order)
            .collect::<Vec<_>>()
    );
    if method != "mapOrder" {
        clock.normalize_orders(&mut native_orders, &c["orders"]);
    }
    let output = match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    };
    let variant = &c["variant"];
    assert_eq!(
        output, c["output"],
        "BINGX-PRIVATE native {method} variant {variant}"
    );
    assert_eq!(
        observe(&json!(*host.calls.borrow())),
        c["calls"],
        "BINGX-PRIVATE calls {method} variant {variant}"
    );
    assert_eq!(
        json!(*host.fills.borrow()),
        c["fills"],
        "BINGX-PRIVATE fill payloads {method} variant {variant}"
    );
    assert_eq!(
        native_orders, c["orders"],
        "BINGX-PRIVATE native orders {method} variant {variant}"
    );
    let positions = json!(
        store
            .positions()
            .unwrap()
            .iter()
            .map(position)
            .collect::<Vec<_>>()
    );
    let mut expected_positions = c["positions"].clone();
    for (index, row) in expected_positions
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        for key in ["info", "extra"] {
            if row.get(key) == Some(&json!({"undefined":true})) {
                assert!(
                    positions[index].get(key).is_none(),
                    "BINGX-PRIVATE CHOSEN absent flat-position {key}"
                );
                row.as_object_mut().unwrap().remove(key);
            }
        }
    }
    assert_eq!(
        positions, expected_positions,
        "BINGX-PRIVATE native positions {method} variant {variant}"
    );
    assert_eq!(
        serde_json::to_value(Store::loaded(store.as_ref()).unwrap()).unwrap(),
        c["loaded"],
        "BINGX-PRIVATE loaded flag {method} variant {variant}"
    );
    assert_eq!(
        state(&v, &host),
        c["state"],
        "BINGX-PRIVATE refresh state {method} variant {variant}"
    );
    let reports = host.exceptions.borrow();
    assert_eq!(
        reports.len(),
        usize::from(method == "handleAccountEvents" && input["modeFailure"] == true),
        "BINGX-PRIVATE CHOSEN mode-failure reporting"
    );
    if !reports.is_empty() {
        assert_eq!(reports[0].message, "mode cause");
    }
}
#[test]
fn private_events_match_actual_typescript() {
    let names = [
        "mapOrder",
        "normalizeSymbol",
        "extractAccountPositions",
        "schedulePositionRefresh",
        "executePositionRefresh",
        "handleOrderEvents",
        "handleAccountEvents",
        "ping",
    ];
    assert_eq!(
        corpus()["names"],
        json!(names),
        "BINGX-PRIVATE source census"
    );
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "BINGX-PRIVATE case floor");
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
        fingerprint, 0xbbd1b521471ed9f5,
        "BINGX-PRIVATE independent input fingerprint"
    );
    let mut counts = [0; 8];
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["method"], names[i / 4]);
        assert_eq!(c["variant"], json!(i % 4));
        counts[i / 4] += 1;
        run(c);
    }
    assert_eq!(counts, [4; 8]);
    println!("BINGX-PRIVATE source runtime {}", corpus()["runtime"]);
}
#[test]
fn fill_sequences_preserve_source_cache_and_strict_ids() {
    let extras = corpus()["extras"].as_array().unwrap();
    assert_eq!(extras.len(), 3);
    for c in extras {
        run(c);
    }
    assert_eq!(
        extras[0]["fills"].as_array().unwrap().len(),
        2,
        "BINGX-PRIVATE source terminal replay after cache deletion"
    );
    assert_eq!(
        extras[2]["orders"][0]["status"], "open",
        "BINGX-PRIVATE numeric id strict status match"
    );
}
#[test]
fn refresh_timer_coalesces_and_executes_at_boundary() {
    let (v, store, host) = fixture(&corpus()["cases"][12]["input"]);
    v.schedule_position_refresh().unwrap();
    v.schedule_position_refresh().unwrap();
    assert_eq!(host.timers.borrow().len(), 1);
    assert_eq!(host.timers.borrow()[0].0, 650.0);
    host.now.set(750.0);
    let callback = host.timers.borrow()[0].1.clone();
    callback().unwrap();
    host.drain();
    let expected = &corpus()["boundary"]["timer"];
    assert_eq!(observe(&json!(*host.calls.borrow())), expected["calls"]);
    let actual = state(&v, &host);
    for key in ["last", "scheduled", "pending", "timers"] {
        assert_eq!(
            actual[key], expected["state"][key],
            "BINGX-PRIVATE timer {key}"
        );
    }
    assert_eq!(
        serde_json::to_value(Store::loaded(store.as_ref()).unwrap()).unwrap(),
        expected["loaded"]
    );
}
#[test]
fn private_ping_preserves_opaque_error_identity() {
    let e = Rc::new("opaque");
    let now = Cell::new(0);
    let assigned = Cell::new(0);
    let error = private_ping(
        false,
        || {
            now.set(now.get() + 1);
            Ok(f64::INFINITY)
        },
        |n| {
            assert!(n.is_infinite());
            assigned.set(assigned.get() + 1);
            Ok(())
        },
        |frame| {
            assert_eq!(frame, "Ping");
            Err(e.clone())
        },
    )
    .unwrap_err();
    assert!(Rc::ptr_eq(&e, &error));
    assert_eq!(now.get(), 1);
    assert_eq!(assigned.get(), 1);
    assert_eq!(
        corpus()["boundary"]["pingIdentity"],
        true,
        "BINGX-PRIVATE source opaque ping identity"
    );
    let clock_error = private_ping(
        false,
        || Err(e.clone()),
        |_| panic!("clock failure assignment"),
        |_| panic!("clock failure send"),
    )
    .unwrap_err();
    assert!(
        Rc::ptr_eq(&e, &clock_error),
        "BINGX-PRIVATE clock error identity"
    );
    assert_eq!(corpus()["boundary"]["pingClockIdentity"], true);
    private_ping::<()>(
        true,
        || panic!("disposed clock"),
        |_| panic!("disposed assignment"),
        |_| panic!("disposed send"),
    )
    .unwrap();
}
#[test]
fn typed_order_and_snapshot_boundaries_are_explicit() {
    let input = &corpus()["cases"][0]["input"];
    let (_, store, _) = fixture(input);
    let mut o = input["wireOrder"].clone();
    o["timeInForce"] = json!("constructor");
    let e = map_order(&o.clone().into(), &store.orders().unwrap(), 1234.0).unwrap_err();
    assert!(e.message.contains("inherited callable"));
    assert!(
        corpus()["boundary"]["inherited"]["output"]["ok"]["timeInForce"]
            .get("callable")
            .is_some(),
        "BINGX-PRIVATE source inherited TIF"
    );
    o["timeInForce"] = json!("GTC");
    o["o"] = json!("STOP_MARKET");
    let mapped = map_order(&o.clone().into(), &store.orders().unwrap(), 1234.0)
        .unwrap()
        .unwrap();
    assert!(mapped.sl_trigger_type.is_none());
    assert_eq!(
        corpus()["boundary"]["undefinedTrigger"]["output"]["ok"]["slTriggerType"],
        json!({"undefined":true}),
        "BINGX-PRIVATE source own undefined trigger"
    );
    let (v, _, _) = fixture(input);
    let mut event = input["account"].clone();
    let snapshot = v.extract_account_positions(&event.clone().into());
    assert_eq!(
        corpus()["boundary"]["aliasPrice"],
        "changed",
        "BINGX-PRIVATE source live row alias"
    );
    event["a"]["P"][0]["ep"] = json!("changed");
    assert_eq!(
        snapshot[0].get("ep").unwrap(),
        &safe_cex_types::retained_json::RetainedJsonInput::from(json!("12")),
        "BINGX-PRIVATE CHOSEN owned row snapshot"
    );
    assert_eq!(
        corpus()["boundary"]["surrogate"]["output"]["ok"],
        json!({"utf16":[55296,88]})
    );
}
#[test]
fn refresh_catch_reports_original_subscriber_error() {
    let (v, store, host) = fixture(&corpus()["cases"][16]["input"]);
    let cleanup = Store::subscribe(
        store.as_ref(),
        Box::new(|_, _| {
            let mut cause = ExchangeError::new("subscriber cause");
            cause.name = "ListenerError".into();
            cause.store_subscriber_ids = vec![71, 83];
            cause.response = Some(json!({"data":{"msg":"subscriber denied"}}));
            Err(StoreSubscriberError::new("subscriber cause").with_cause(Rc::new(cause)))
        }),
    );
    ready(v.execute_position_refresh());
    let reports = host.exceptions.borrow();
    assert_eq!(
        reports.len(),
        1,
        "BINGX-PRIVATE CHOSEN visible refresh catch"
    );
    assert_eq!(reports[0].name, "ListenerError");
    assert_eq!(reports[0].message, "subscriber cause");
    assert_eq!(&reports[0].store_subscriber_ids[..2], &[71, 83]);
    assert_eq!(reports[0].store_subscriber_ids.len(), 3);
    assert_eq!(
        reports[0].response,
        Some(json!({"data":{"msg":"subscriber denied"}}))
    );
    assert_eq!(
        store.loaded().unwrap().positions,
        corpus()["boundary"]["caughtRefresh"]["loaded"] == true
    );
    assert!(!v.has_pending_position_events.get());
    assert_eq!(v.last_position_fetch_time.get(), 100.0);
    cleanup();
}
#[test]
fn refresh_timer_rejects_released_native_owner() {
    let (v, _, host) = fixture(&corpus()["cases"][12]["input"]);
    v.schedule_position_refresh().unwrap();
    let timer = host.timers.borrow()[0].1.clone();
    drop(v);
    assert_eq!(
        timer().unwrap_err().message,
        "BingX private event owner released",
        "BINGX-PRIVATE CHOSEN timer lifetime"
    );
}
