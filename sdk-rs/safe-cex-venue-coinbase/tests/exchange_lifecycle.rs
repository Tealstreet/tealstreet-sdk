use indexmap::IndexMap;
use safe_cex_venue_coinbase::{
    exchange_lifecycle::*,
    portfolios_balances::{Portfolio, PortfolioFailure, PortfolioState, PortfolioThrown},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
const FIXTURE: &[u8] = include_bytes!("fixtures/exchange-lifecycle.json");
const HASH: &str = "70d1b1afe51ade5f3fd82a8ccd153561894f42c23eb8bff0f8591d4303ba4b97";
struct Trace {
    kind: String,
    fields: IndexMap<String, Frame>,
}
struct Host {
    effect_hook: RefCell<Option<Rc<dyn Fn(&str)>>>,
    input: Value,
    trace: RefCell<Vec<Trace>>,
    disposed: Cell<bool>,
    last: Cell<f64>,
    failure: PortfolioFailure,
    timers: RefCell<Vec<(LifecycleTimer, Option<LifecycleCallback>)>>,
    loaded_root: Frame,
    markets: Frame,
    tickers: Frame,
    balance: Frame,
    orders: Frame,
    merged: Frame,
    portfolio: Portfolio,
    update_override: RefCell<Option<Promise<Result<(), PortfolioFailure>>>>,
    tick_override: RefCell<Option<Promise<Result<(), PortfolioFailure>>>>,
}
fn frame(v: Value) -> Frame {
    Frame::from_json(&v)
}
fn portfolio(id: &str) -> Portfolio {
    Rc::new(RefCell::new(PortfolioState {
        uuid: id.into(),
        r#type: "SPOT".into(),
        metadata: indexmap::IndexMap::from_iter([
            ("name".into(), json!("named").into()),
            ("deleted".into(), json!(false).into()),
        ]),
    }))
}
impl Host {
    fn record(&self, kind: &str, fields: IndexMap<String, Frame>) -> Result<(), PortfolioFailure> {
        let hook = self.effect_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(kind);
        }
        self.trace.borrow_mut().push(Trace {
            kind: kind.into(),
            fields,
        });
        if self.input["fail"].as_str() == Some(kind)
            || self.input["alsoFail"].as_str() == Some(kind)
        {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn call<T: Clone + 'static>(
        &self,
        kind: &str,
        value: T,
        fields: IndexMap<String, Frame>,
    ) -> LifecycleAsync<T> {
        let result = self.record(kind, fields);
        if result.is_err() && self.input["sync"].as_bool() == Some(true) {
            return Err(self.failure.clone());
        }
        if self.input["disposeAfter"].as_str() == Some(kind) {
            self.disposed.set(true);
        }
        Ok(Promise::resolved(result.map(|()| value)))
    }
    fn id(&self, timer: &LifecycleTimer) -> usize {
        self.timers
            .borrow()
            .iter()
            .position(|(t, _)| Rc::ptr_eq(t, timer))
            .expect("original timer root")
            + 1
    }
    fn add_old_timer(&self) -> LifecycleTimer {
        let token = Rc::new(RefCell::new(json!({})));
        self.timers.borrow_mut().push((token.clone(), None));
        token
    }
}
fn args(values: impl IntoIterator<Item = (&'static str, Value)>) -> IndexMap<String, Frame> {
    values
        .into_iter()
        .map(|(k, v)| (k.into(), frame(v)))
        .collect()
}
impl LifecycleHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn public_only(&self) -> bool {
        self.input["public"].as_bool().unwrap_or(false)
    }
    fn init_external_public_data(&self) -> LifecycleAsync<bool> {
        self.call(
            "external",
            self.input["external"].as_bool().unwrap_or(false),
            args([]),
        )
    }
    fn find_selected_portfolio(&self) -> LifecycleAsync<Portfolio> {
        self.call("find", self.portfolio.clone(), args([]))
    }
    fn fetch_markets(&self) -> LifecycleAsync<Frame> {
        self.call("markets", self.markets.clone(), args([]))
    }
    fn fetch_tickers_intx(&self) -> LifecycleAsync<Frame> {
        self.call("tickers", self.tickers.clone(), args([]))
    }
    fn fetch_balance(&self) -> LifecycleAsync<Frame> {
        self.call("balance", self.balance.clone(), args([]))
    }
    fn fetch_open_orders_snapshot(&self) -> LifecycleAsync<Frame> {
        self.call("orders", self.orders.clone(), args([]))
    }
    fn tick(&self) -> LifecycleAsync<()> {
        let p = self.call("tick", (), args([]))?;
        Ok(self.tick_override.borrow().clone().unwrap_or(p))
    }
    fn get_markets(&self, endpoint: &'static str) -> LifecycleAsync<()> {
        self.call("get", (), args([("endpoint", json!(endpoint))]))
    }
    fn loaded(&self) -> Frame {
        self.loaded_root.clone()
    }
    fn update(&self, fields: LifecycleUpdate) -> LifecycleAsync<()> {
        let payload = Frame::Object(Rc::new(RefCell::new(
            fields
                .into_iter()
                .map(|(k, v)| (k.encode_utf16().collect(), v))
                .collect(),
        )));
        let p = self.call(
            "update",
            (),
            IndexMap::from_iter([("fields".into(), payload)]),
        )?;
        Ok(self.update_override.borrow().clone().unwrap_or(p))
    }
    fn connect_public(&self) -> Result<(), PortfolioFailure> {
        self.record("connect-public", args([]))
    }
    fn connect_private(&self) -> Result<(), PortfolioFailure> {
        self.record("connect-private", args([]))
    }
    fn dispose_base(&self) -> Result<(), PortfolioFailure> {
        self.record("dispose-base", args([]))?;
        self.disposed.set(true);
        Ok(())
    }
    fn dispose_public(&self) -> Result<(), PortfolioFailure> {
        self.record("dispose-public", args([]))
    }
    fn dispose_private(&self) -> Result<(), PortfolioFailure> {
        self.record("dispose-private", args([]))
    }
    fn clear_timeout(&self, timer: LifecycleTimer) -> Result<(), PortfolioFailure> {
        let id = self.id(&timer);
        self.record("clear", args([("id", json!(id))]))?;
        self.timers.borrow_mut()[id - 1].1 = None;
        Ok(())
    }
    fn set_timeout(
        &self,
        callback: LifecycleCallback,
        delay_ms: f64,
    ) -> Result<LifecycleTimer, PortfolioFailure> {
        self.record("set", args([("delay", json!(delay_ms))]))?;
        let timer = Rc::new(RefCell::new(json!({})));
        self.timers
            .borrow_mut()
            .push((timer.clone(), Some(callback)));
        Ok(timer)
    }
    fn last_private_user_stream_message_at(&self) -> f64 {
        self.last.get()
    }
    fn mark_private_user_stream_degraded(
        &self,
        reason: &'static str,
        force_toast: bool,
    ) -> Result<(), PortfolioFailure> {
        self.record(
            "degraded",
            args([
                ("reason", json!(reason)),
                ("forceToast", json!(force_toast)),
            ]),
        )
    }
    fn merge_rest_orders_with_fresh_ws_state(
        &self,
        orders: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        assert_root(
            &orders,
            &self.orders,
            "REST merge receives original orders root",
        );
        self.record("merge", IndexMap::from_iter([("orders".into(), orders)]))?;
        Ok(self.merged.clone())
    }
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure> {
        assert!(
            Rc::ptr_eq(&error, &self.failure),
            "reconciliation retains original exception root"
        );
        self.record("orders-error", args([("same", json!(true))]))
    }
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure> {
        self.record(
            "emit",
            message.map(|m| args([("message", m)])).unwrap_or_default(),
        )
    }
    fn log(&self, message: &str, severity: Option<&str>) -> Result<(), PortfolioFailure> {
        let mut f = args([("message", json!(message))]);
        if let Some(s) = severity {
            f.insert("severity".into(), frame(json!(s)));
        }
        self.record("log", f)
    }
}
fn setup(input: Value, tasks: &Microtasks) -> Rc<LifecycleReceiver<Host>> {
    let selected = input["selected"]
        .as_bool()
        .unwrap_or(false)
        .then(|| portfolio("cached-A"));
    let failure = Rc::new(PortfolioThrown::from_value(Some(
        input
            .get("error")
            .cloned()
            .unwrap_or(json!({"name":"Error","message":"controlled failure"})),
    )));
    let host = Host {
        effect_hook: RefCell::new(None),
        disposed: Cell::new(input["disposed"].as_bool().unwrap_or(false)),
        last: Cell::new(100.0),
        input,
        trace: RefCell::new(vec![]),
        failure,
        timers: RefCell::new(vec![]),
        loaded_root: frame(
            json!({"markets":false,"orders":false,"retained":"yes","nested":{"kept":31},"array":[9]}),
        ),
        markets: frame(json!([{"id":"market-C"}])),
        tickers: frame(json!([{"symbol":"BTC-USD","bid":7}])),
        balance: frame(json!({"free":13,"total":17})),
        orders: frame(json!([{"id":"order-D"}])),
        merged: frame(json!([{"id":"order-D"},{"id":"preserved-E"}])),
        portfolio: portfolio("found-B"),
        update_override: RefCell::new(None),
        tick_override: RefCell::new(None),
    };
    let (order, write) = if host.input["timers"].as_bool() == Some(true) {
        (Some(host.add_old_timer()), Some(host.add_old_timer()))
    } else {
        (None, None)
    };
    Rc::new(LifecycleReceiver {
        host,
        spawn: tasks.spawner(),
        selected: Rc::new(RefCell::new(selected)),
        order_write_reconcile_timeout: Rc::new(RefCell::new(order)),
        private_user_stream_write_check_timeout: Rc::new(RefCell::new(write)),
    })
}
fn assert_root(a: &Frame, b: &Frame, label: &str) {
    match (a, b) {
        (Frame::Array(a), Frame::Array(b)) => assert!(Rc::ptr_eq(a, b), "{label}"),
        (Frame::Object(a), Frame::Object(b)) => assert!(Rc::ptr_eq(a, b), "{label}"),
        _ => panic!("{label}: expected native root"),
    }
}
fn assert_frame(actual: &Frame, expected: &Value, label: &str) {
    match (actual, expected) {
        (Frame::Null, Value::Null) => (),
        (Frame::Bool(a), Value::Bool(b)) => assert_eq!(a, b, "{label}: native boolean"),
        (Frame::Number(a), Value::Number(b)) => assert_eq!(
            a.to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{label}: native number bits"
        ),
        (Frame::String(a), Value::String(b)) => assert_eq!(
            *a,
            b.encode_utf16().collect::<Vec<_>>(),
            "{label}: native UTF16"
        ),
        (Frame::Array(a), Value::Array(b)) => {
            assert_eq!(a.borrow().len(), b.len(), "{label}: source array length");
            for (a, b) in a.borrow().iter().zip(b) {
                assert_frame(a, b, label);
            }
        }
        (Frame::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.borrow().len(),
                b.len(),
                "{label}: source object key count"
            );
            for (k, v) in b {
                let key = k.encode_utf16().collect::<Vec<_>>();
                assert_frame(
                    a.borrow()
                        .get(&key)
                        .unwrap_or_else(|| panic!("{label}: missing native {k}")),
                    v,
                    &format!("{label}.{k}"),
                );
            }
        }
        _ => panic!("{label}: native kind differs from decoded source expectation"),
    }
}
#[test]
fn lifecycle_live_native_corpus() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE)),
        HASH,
        "independent lifecycle content pin"
    );
    let inputs: Vec<Value> = serde_json::from_slice(FIXTURE).unwrap();
    assert_eq!(inputs.len(), 45, "independent lifecycle corpus floor");
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/exchange-lifecycle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "live TypeScript oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(expected.len(), 45, "independent source result floor");
    for (input, want) in inputs.into_iter().zip(expected) {
        let label = input["label"].as_str().unwrap().to_owned();
        assert_eq!(want["label"], label);
        let tasks = Microtasks::default();
        let r = setup(input.clone(), &tasks);
        let mut failure = None;
        match input["op"].as_str().unwrap() {
            "dispose" => failure = dispose(&r).err(),
            "account" => {
                let p = get_account(r.clone());
                tasks.drain();
                match completed(p) {
                    Ok(v) => assert_eq!(
                        v.borrow().user_id,
                        want["result"]["userId"].as_str().unwrap(),
                        "{label}: native account userId"
                    ),
                    Err(e) => failure = Some(e),
                }
            }
            "validate" => {
                let p = validate_account(r.clone());
                tasks.drain();
                match completed(p) {
                    Ok(v) => {
                        assert_eq!(
                            v.error, want["result"]["error"],
                            "{label}: native validation error"
                        );
                        assert_eq!(
                            v.has_balance,
                            want["hasBalance"].as_bool().unwrap(),
                            "{label}: native balance own-property presence"
                        );
                        match v.balance {
                            Some(v) => {
                                assert_frame(&v, &want["result"]["balance"], &label);
                                assert_root(
                                    &v,
                                    &r.host.balance,
                                    "validation returns original balance root",
                                );
                            }
                            None => assert!(
                                want["result"].get("balance").is_none(),
                                "{label}: native undefined balance"
                            ),
                        }
                    }
                    Err(e) => failure = Some(e),
                }
            }
            "start" | "private" | "public" | "reconcile" => {
                let p = match input["op"].as_str().unwrap() {
                    "start" => start(r.clone()),
                    "private" => start_private(r.clone()),
                    "public" => start_public_data_only(r.clone()),
                    _ => reconcile_open_orders_from_rest(r.clone(), "place".into()),
                };
                tasks.drain();
                failure = completed(p).err();
            }
            "write" => failure = schedule_private_user_stream_write_check(r.clone()).err(),
            "schedule" => {
                failure = schedule_open_orders_rest_reconciliation(r.clone(), "place".into()).err()
            }
            _ => unreachable!(),
        }
        if failure.is_none() && input["fire"].as_bool() == Some(true) {
            if input["disposeBeforeFire"].as_bool() == Some(true) {
                r.host.disposed.set(true);
            }
            if let Some(n) = input.get("afterMessage") {
                r.host.last.set(n.as_f64().unwrap());
            }
            let callback = r.host.timers.borrow().last().and_then(|(_, f)| f.clone());
            if let Some(callback) = callback {
                failure = callback().err();
            }
            tasks.drain();
        }
        assert_eq!(
            failure.is_some(),
            !want["error"].is_null(),
            "{label}: native error classification"
        );
        if let Some(e) = failure {
            assert!(
                Rc::ptr_eq(&e, &r.host.failure),
                "{label}: original exception identity"
            );
            assert_eq!(want["same"], true);
        }
        let trace = r.host.trace.borrow();
        assert_eq!(
            trace.len(),
            want["trace"].as_array().unwrap().len(),
            "{label}: independent source invocation count"
        );
        for (t, w) in trace.iter().zip(want["trace"].as_array().unwrap()) {
            assert_eq!(
                t.kind,
                w["kind"].as_str().unwrap(),
                "{label}: native call order"
            );
            assert_eq!(
                t.fields.len(),
                w.as_object().unwrap().len() - 1,
                "{label}: native argument count"
            );
            for (k, v) in &t.fields {
                assert_frame(v, &w[k], &format!("{label}: {}.{k}", t.kind));
            }
            if t.kind == "update" {
                let payload = &t.fields["fields"];
                let copied = payload.get("loaded").expect("update loaded native field");
                if let (Frame::Object(a), Frame::Object(b)) = (&copied, &r.host.loaded_root) {
                    assert!(
                        !Rc::ptr_eq(a, b),
                        "loaded spread creates a fresh outer root"
                    );
                }
                for key in ["nested", "array"] {
                    assert_root(
                        &copied.get(key).unwrap(),
                        &r.host.loaded_root.get(key).unwrap(),
                        "loaded spread retains nested native roots",
                    );
                }
                for (key, root) in [
                    ("markets", &r.host.markets),
                    ("tickers", &r.host.tickers),
                    ("orders", &r.host.merged),
                ] {
                    if let Some(value) = payload.get(key) {
                        if let Frame::Array(a) = &value {
                            if !a.borrow().is_empty() {
                                assert_root(
                                    &value,
                                    root,
                                    "update preserves original fetched/merged native root",
                                );
                            }
                        }
                    }
                }
            }
        }
        if input["selected"].as_bool() != Some(true) {
            if let Some(selected) = r.selected.borrow().as_ref() {
                assert!(
                    Rc::ptr_eq(selected, &r.host.portfolio),
                    "{label}: selected portfolio original root"
                );
            }
        }
        assert_eq!(
            r.selected
                .borrow()
                .as_ref()
                .map(|p| p.borrow().uuid.clone()),
            want["selected"].as_str().map(str::to_owned),
            "{label}: selected portfolio native state"
        );
        for (actual, key) in [
            (&r.order_write_reconcile_timeout, "orderTimer"),
            (&r.private_user_stream_write_check_timeout, "writeTimer"),
        ] {
            assert_eq!(
                actual.borrow().as_ref().map(|t| r.host.id(t)),
                want[key].as_u64().map(|n| n as usize),
                "{label}: retained timer identity/state"
            );
        }
    }
}
#[test]
fn private_start_is_hot_and_not_awaited_by_start() {
    let tasks = Microtasks::default();
    let r = setup(json!({"external":true}), &tasks);
    let (p, resolve) = Promise::pending();
    *r.host.tick_override.borrow_mut() = Some(p);
    let start = start(r.clone());
    let marker = Rc::new(Cell::new(false));
    let captured = marker.clone();
    tasks.enqueue(move || captured.set(true));
    tasks.drain();
    assert!(
        marker.get(),
        "queued marker proceeds during pending private tick"
    );
    assert!(
        completed(start).is_ok(),
        "outer start settles while private tick is pending"
    );
    assert_eq!(
        r.host
            .trace
            .borrow()
            .iter()
            .map(|t| t.kind.as_str())
            .collect::<Vec<_>>(),
        ["external", "find", "connect-private", "tick"],
        "private startup hot call order"
    );
    resolve.resolve(Ok(()));
    tasks.drain();
}
#[test]
fn public_update_is_unawaited_but_initial_market_update_is_awaited() {
    let tasks = Microtasks::default();
    let r = setup(json!({"public":true}), &tasks);
    let (p, resolve) = Promise::pending();
    *r.host.update_override.borrow_mut() = Some(p);
    let mut outer = start(r.clone());
    tasks.drain();
    assert!(
        is_pending(&mut outer) && is_pending(&mut outer),
        "initial market update must be awaited"
    );
    assert!(
        !r.host
            .trace
            .borrow()
            .iter()
            .any(|t| t.kind == "connect-public"),
        "public socket waits for initial store update"
    );
    resolve.resolve(Ok(()));
    tasks.drain();
    assert!(completed(outer).is_ok());
    let tasks = Microtasks::default();
    let r = setup(json!({}), &tasks);
    let (p, _) = Promise::pending();
    *r.host.update_override.borrow_mut() = Some(p);
    let p = start_public_data_only(r.clone());
    tasks.drain();
    assert!(
        completed(p).is_ok(),
        "public data-only store update is unawaited"
    );
}
#[test]
fn timer_callback_clears_handle_before_reentrant_effect() {
    for reconciliation in [false, true] {
        let tasks = Microtasks::default();
        let r = setup(json!({}), &tasks);
        let weak = Rc::downgrade(&r);
        *r.host.effect_hook.borrow_mut() = Some(Rc::new(move |kind| {
            if kind != if reconciliation { "orders" } else { "degraded" } {
                return;
            }
            let r = weak.upgrade().unwrap();
            if reconciliation {
                assert!(
                    r.order_write_reconcile_timeout.borrow().is_none(),
                    "reconciliation clears handle before hot fetch effect"
                );
                schedule_open_orders_rest_reconciliation(r, "reentrant".into()).unwrap();
            } else {
                assert!(
                    r.private_user_stream_write_check_timeout.borrow().is_none(),
                    "write grace clears handle before health effect"
                );
                schedule_private_user_stream_write_check(r).unwrap();
            }
        }));
        if reconciliation {
            schedule_open_orders_rest_reconciliation(r.clone(), "edit".into()).unwrap();
        } else {
            schedule_private_user_stream_write_check(r.clone()).unwrap();
        }
        let callback = r.host.timers.borrow()[0].1.clone().unwrap();
        callback().unwrap();
        let timer = if reconciliation {
            r.order_write_reconcile_timeout.borrow().clone()
        } else {
            r.private_user_stream_write_check_timeout.borrow().clone()
        };
        assert_eq!(
            r.host.id(&timer.expect("reentrant schedule retained")),
            2,
            "old callback preserves replacement timer root"
        );
        tasks.drain();
    }
}
#[test]
fn account_reads_retained_portfolio_at_the_source_await_boundary() {
    let tasks = Microtasks::default();
    let r = setup(json!({}), &tasks);
    let pending = get_account(r.clone());
    r.host.portfolio.borrow_mut().uuid = "mutated-after-find".into();
    tasks.drain();
    assert_eq!(
        completed(pending).unwrap().borrow().user_id,
        "mutated-after-find",
        "account reads live selected portfolio after await"
    );
    assert!(
        Rc::ptr_eq(r.selected.borrow().as_ref().unwrap(), &r.host.portfolio),
        "selection retains original native portfolio root"
    );
}

#[test]
fn account_promise_retains_one_mutable_native_object() {
    let source = source_class_policy();
    let expected = &source["cases"][10];
    assert_eq!(expected["id"], "account-promise-retains-object-root");
    assert_eq!(expected["samePromiseValue"], true);
    assert_eq!(expected["distinctInvocation"], true);
    let tasks = Microtasks::default();
    let receiver = setup(json!({"selected":true}), &tasks);
    receiver
        .selected
        .borrow()
        .as_ref()
        .unwrap()
        .borrow_mut()
        .uuid = "cfm".into();
    let promise = get_account(receiver.clone());
    let first = completed(promise.clone()).unwrap();
    assert_eq!(first.borrow().user_id, "cfm", "first native account value");
    first.borrow_mut().user_id = "caller-mutated".into();
    let second = completed(promise).unwrap();
    assert!(
        Rc::ptr_eq(&first, &second),
        "same account Promise retains one native object root"
    );
    assert_eq!(
        second.borrow().user_id,
        "caller-mutated",
        "same account Promise retains the native object mutation"
    );
    let fresh = completed(get_account(receiver.clone())).unwrap();
    assert!(
        !Rc::ptr_eq(&fresh, &first),
        "new account invocation allocates a distinct native root"
    );
    assert_eq!(fresh.borrow().user_id, "cfm");
    assert_eq!(
        receiver.selected.borrow().as_ref().unwrap().borrow().uuid,
        "cfm",
        "returned account mutation does not rewrite selected portfolio"
    );
    assert_eq!(
        receiver.host.trace.borrow().len(),
        0,
        "no imported calls for retained selection"
    );
    assert_eq!(second.borrow().user_id, expected["secondUserId"]);
    assert_eq!(fresh.borrow().user_id, expected["freshUserId"]);
}

fn source_class_policy() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/live-class-policy.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live class policy source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "no-credentials",
            "key-without-signer",
            "local-selection-and-account",
            "remote-signer",
            "replace-options-slot",
            "live-support-spot",
            "spot-override-and-captured-base-field",
            "signed-captured-base-calls-spot-finder",
            "retained-base-finder-versus-current-spot",
            "supported-types-promise-retains-array-root",
            "account-promise-retains-object-root",
            "public-only-promise-retains-array-root",
            "spot-promise-retains-array-root",
            "live-policy-after-pending-selection"
        ],
        "independent source policy content floor"
    );
    source
}
