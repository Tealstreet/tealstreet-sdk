#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{StoreUpdate, store::DefaultStore};
use safe_cex_types::ExchangeName;
use safe_cex_webworker::{
    chaser::ChaserOrderInstance, chaser_manager::*, task_types::ChaserOrderOptions,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug, PartialEq)]
enum Event {
    Process(String, usize, bool),
    Cleanup(String, Vec<String>, String, usize),
    Complete(String, usize),
    Emit(String, String),
    Dispose(usize, usize),
}
struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
}
type CleanupHook = Rc<dyn Fn(&Rc<ChaserOrderManager>, &Rc<ChaserOrderInstance>)>;
struct Ops {
    manager: Rc<ChaserOrderManager>,
    events: Rc<RefCell<Vec<Event>>>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    process_mode: Cell<&'static str>,
    released: Rc<Cell<bool>>,
    cleanup_error: Cell<bool>,
    cleanup_hook: RefCell<Option<CleanupHook>>,
}
impl ChaserSupervisor for Ops {
    fn construct_supervision(
        &self,
        m: Rc<ChaserOrderManager>,
        i: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        let list = m.instances();
        let list = list.borrow();
        let present = list.iter().any(|c| Rc::ptr_eq(c, &i));
        assert!(present, "MANAGER native add visible before process");
        self.events
            .borrow_mut()
            .push(Event::Process(i.id().into(), list.len(), present));
        let mode = self.process_mode.get();
        let released = self.released.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if mode == "pending" && !released.get() {
                Poll::Pending
            } else if mode == "error" {
                Poll::Ready(Err(ExchangeError::new("process failed")))
            } else {
                Poll::Ready(Ok(()))
            }
        }))
    }
    fn construct_cleanup(
        &self,
        m: Rc<ChaserOrderManager>,
        i: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        self.events.borrow_mut().push(Event::Cleanup(
            i.id().into(),
            i.current_order_ids(),
            i.state().borrow().stop_reason.clone(),
            m.instances().borrow().len(),
        ));
        let hook = self.cleanup_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(&m, &i);
        }
        let failure = self.cleanup_error.get();
        let released = self.released.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if !released.get() {
                Poll::Pending
            } else if failure {
                Poll::Ready(Err(ExchangeError::new("cleanup failed")))
            } else {
                Poll::Ready(Ok(()))
            }
        }))
    }
    fn construct_completion(&self, i: Rc<ChaserOrderInstance>) -> ExchangeFuture<'static, ()> {
        self.events.borrow_mut().push(Event::Complete(
            i.id().into(),
            self.manager.instances().borrow().len(),
        ));
        Box::pin(async { Ok(()) })
    }
    fn spawn(&self, f: ExchangeFuture<'static, ()>) {
        self.tasks.borrow_mut().push(f);
    }
}
impl Ops {
    fn new(m: Rc<ChaserOrderManager>, events: Rc<RefCell<Vec<Event>>>) -> Rc<Self> {
        Rc::new(Self {
            manager: m,
            events,
            tasks: RefCell::default(),
            process_mode: Cell::new("ok"),
            released: Rc::new(Cell::new(true)),
            cleanup_error: Cell::new(false),
            cleanup_hook: RefCell::default(),
        })
    }
    fn drain(&self) {
        for mut f in std::mem::take(&mut *self.tasks.borrow_mut()) {
            match poll(&mut f) {
                Poll::Pending => self.tasks.borrow_mut().push(f),
                Poll::Ready(result) => result.unwrap(),
            }
        }
    }
}
fn poll(f: &mut ExchangeFuture<'static, ()>) -> Poll<Result<(), ExchangeError>> {
    f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}
fn fixture() -> (
    Rc<ChaserOrderManager>,
    Rc<Ops>,
    Rc<runtime::Runtime>,
    Rc<dyn Exchange>,
) {
    let m = ChaserOrderManager::new();
    let events = Rc::new(RefCell::new(vec![]));
    let ops = Ops::new(m.clone(), events.clone());
    let rt = Rc::new(runtime::Runtime::default());
    rt.advance(0);
    let ex = venue(rt.clone(), events);
    (m, ops, rt, ex)
}
fn venue(rt: Rc<runtime::Runtime>, events: Rc<RefCell<Vec<Event>>>) -> Rc<dyn Exchange> {
    let ex: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            rt,
            Default::default(),
        ),
        caps: Default::default(),
    });
    for name in ["log", "warning", "error"] {
        let ev = events.clone();
        ex.base().on(
            name,
            Rc::new(move |event, data| {
                ev.borrow_mut().push(Event::Emit(
                    event.into(),
                    String::from_utf16_lossy(data.as_str().unwrap()),
                ));
                Ok(())
            }),
        );
    }
    ex
}
fn instance(ex: Rc<dyn Exchange>, rt: &runtime::Runtime, id: &str) -> Rc<ChaserOrderInstance> {
    *rt.id.borrow_mut() = Some(id.into());
    let o:ChaserOrderOptions=serde_json::from_value(json!({"symbol":"BTC","size":3,"side":"buy","maxAttempts":10,"delay":100,"reduceOnly":false})).unwrap();
    Rc::new(ChaserOrderInstance::new(ex, Rc::new(RefCell::new(o))))
}
fn profile(e: &[Event]) -> Value {
    json!(
        e.iter()
            .map(|e| match e {
                Event::Process(id, n, p) => json!(["process", id, n, p]),
                Event::Cleanup(id, ids, reason, n) => json!(["cleanup", id, ids, reason, n]),
                Event::Complete(id, n) => json!(["complete", id, n]),
                Event::Emit(e, m) => json!(["emit", e, m]),
                Event::Dispose(n, ids) => json!(["dispose", n, ids]),
            })
            .collect::<Vec<_>>()
    )
}
fn source() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_manager_dispatch_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "MANAGER source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn list_registration_and_detached_startup_error_have_live_identity() {
    let (m, h, rt, ex) = fixture();
    let list = m.instances();
    assert!(
        Rc::ptr_eq(&list, &m.instances()),
        "MANAGER native live list identity"
    );
    assert!(list.borrow().is_empty(), "MANAGER native constructor empty");
    let singleton = chaser_order_manager_instance();
    let again = chaser_order_manager_instance();
    assert!(
        Rc::ptr_eq(&singleton, &again),
        "MANAGER native singleton identity"
    );
    assert!(
        !Rc::ptr_eq(&singleton, &m),
        "MANAGER native fresh manager independent"
    );
    assert!(singleton.instances().borrow().is_empty());
    let a = instance(ex.clone(), &rt, "a");
    m.add(a.clone(), h.clone());
    assert_eq!(list.borrow().len(), 1, "MANAGER native add count");
    assert!(
        Rc::ptr_eq(&list.borrow()[0], &a),
        "MANAGER native original instance retained"
    );
    assert_eq!(&*h.events.borrow(), &[Event::Process("a".into(), 1, true)]);
    h.drain();
    let b = instance(ex, &rt, "b");
    h.process_mode.set("error");
    m.add(b.clone(), h.clone());
    assert!(!b.is_stopped(), "MANAGER native startup error deferred");
    assert_eq!(h.tasks.borrow().len(), 1);
    h.drain();
    assert!(b.is_stopped(), "MANAGER native startup failure stops");
    assert_eq!(b.state().borrow().stop_reason, "Chaser startup failed");
    assert_eq!(
        list.borrow().len(),
        2,
        "MANAGER native distinct append count"
    );
    h.process_mode.set("ok");
    m.add(a.clone(), h.clone());
    h.drain();
    assert_eq!(
        list.borrow().len(),
        3,
        "MANAGER native repeated instance append count"
    );
    assert!(
        Rc::ptr_eq(&list.borrow()[2], &a),
        "MANAGER native repeated original instance identity"
    );
    list.borrow_mut().swap(0, 1);
    assert!(
        Rc::ptr_eq(&m.instances().borrow()[0], &b),
        "MANAGER native external reorder visible"
    );
    assert_eq!(
        &*h.events.borrow(),
        &[
            Event::Process("a".into(), 1, true),
            Event::Process("b".into(), 2, true),
            Event::Emit(
                "log".into(),
                "[CHASERb] Chaser supervisor failed during startup for BTC: process failed".into()
            ),
            Event::Emit(
                "error".into(),
                "Chaser supervisor failed during startup for BTC: process failed".into()
            ),
            Event::Cleanup("b".into(), vec![], "Chaser startup failed".into(), 2),
            Event::Process("a".into(), 3, true)
        ],
        "MANAGER native startup effects"
    );
    let src = source();
    assert_eq!(src["list"], profile(&h.events.borrow()));
}
#[test]
fn supervisor_error_prefix_preserves_live_flags_and_failure_cutovers() {
    let specs = [
        ("error", "normal"),
        ("string", "normal"),
        ("null", "normal"),
        ("array", "normal"),
        ("object", "normal"),
        ("error", "stopped"),
        ("error", "log-stop"),
        ("error", "log-throw"),
        ("error", "error-throw"),
        ("error", "cleanup-throw"),
    ];
    let mut native = vec![];
    for (kind, mode) in specs {
        let (m, h, rt, ex) = fixture();
        let i = instance(ex.clone(), &rt, "a");
        m.instances().borrow_mut().push(i.clone());
        if mode == "stopped" {
            i.mark_complete(Some("prior"));
        }
        if matches!(mode, "log-stop" | "log-throw") {
            let i = i.clone();
            ex.base().on(
                "log",
                Rc::new(move |_, _| {
                    if mode == "log-stop" {
                        i.mark_complete(Some("listener"));
                        Ok(())
                    } else {
                        Err(ExchangeError::new("log failed"))
                    }
                }),
            );
        }
        if mode == "error-throw" {
            ex.base().on(
                "error",
                Rc::new(|_, _| Err(ExchangeError::new("error listener failed"))),
            );
        }
        h.cleanup_error.set(mode == "cleanup-throw");
        h.released.set(false);
        let (err, detail) = match kind {
            "error" => (
                SupervisorFailure::Error(ExchangeError::new("fixture")),
                "fixture",
            ),
            "string" => (SupervisorFailure::Value(json!("raw")), "raw"),
            "null" => (SupervisorFailure::Value(Value::Null), "null"),
            "array" => (SupervisorFailure::Value(json!([1, null, "x"])), "1,,x"),
            "object" => (
                SupervisorFailure::Value(json!({"message":"ignored"})),
                "[object Object]",
            ),
            _ => unreachable!(),
        };
        let mut future = m.handle_supervisor_error(i.clone(), err, "order update", h.clone());
        let stopped = mode != "log-throw";
        assert_eq!(
            i.is_stopped(),
            stopped,
            "MANAGER native supervisor stopped cutover"
        );
        let reason = match mode {
            "stopped" => "prior",
            "log-stop" => "listener",
            "log-throw" => "",
            _ => "Chaser order update failed",
        };
        assert_eq!(
            i.state().borrow().stop_reason,
            reason,
            "MANAGER native supervisor reason"
        );
        let message = format!("Chaser supervisor failed during order update for BTC: {detail}");
        let mut expected = vec![Event::Emit("log".into(), format!("[CHASERa] {message}"))];
        if mode != "log-throw" {
            expected.push(Event::Emit("error".into(), message));
        }
        if !matches!(mode, "log-throw" | "error-throw") {
            expected.push(Event::Cleanup("a".into(), vec![], reason.into(), 1));
        }
        assert_eq!(
            &*h.events.borrow(),
            &expected,
            "MANAGER native supervisor prefix ordering"
        );
        let failure = match mode {
            "log-throw" => Some("log failed"),
            "error-throw" => Some("error listener failed"),
            "cleanup-throw" => Some("cleanup failed"),
            _ => None,
        };
        if !matches!(mode, "log-throw" | "error-throw") {
            assert!(
                poll(&mut future).is_pending(),
                "MANAGER native cleanup is awaited"
            );
        }
        h.released.set(true);
        let result = match poll(&mut future) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("released cleanup pending"),
        };
        assert_eq!(
            result.err().as_ref().map(|e| e.message.as_str()),
            failure,
            "MANAGER native supervisor result"
        );
        native.push(json!({"kind":kind,"mode":mode,"events":profile(&h.events.borrow()),"stopped":stopped,"reason":reason,"error":failure}));
        ex.base().off("log", None);
    }
    assert_eq!(
        native.len(),
        10,
        "MANAGER native supervisor independent scenario floor"
    );
    assert_eq!(source()["supervisor"], json!(native));
}
#[test]
fn exchange_cleanup_snapshots_pointer_matches_and_retains_warning_cutover() {
    let (m, h, rt, ex) = fixture();
    let other_ex = venue(rt.clone(), h.events.clone());
    let a = instance(ex.clone(), &rt, "a");
    let b = instance(ex.clone(), &rt, "b");
    let foreign = instance(other_ex, &rt, "foreign");
    let new = instance(ex.clone(), &rt, "new");
    m.instances()
        .borrow_mut()
        .extend([a.clone(), foreign.clone(), b.clone(), a.clone()]);
    let stopped = b.clone();
    ex.base().on(
        "warning",
        Rc::new(move |_, _| {
            stopped.mark_complete(Some("listener stop"));
            Ok(())
        }),
    );
    let inserted = Rc::new(Cell::new(false));
    let inserted_hook = inserted.clone();
    *h.cleanup_hook.borrow_mut() = Some(Rc::new(move |m, i| {
        let list = m.instances();
        let index = list.borrow().iter().position(|other| Rc::ptr_eq(other, i));
        if let Some(n) = index {
            list.borrow_mut().remove(n);
        }
        if !inserted_hook.replace(true) {
            list.borrow_mut().push(new.clone());
        }
    }));
    m.cleanup_exchange(ex.clone(), None, h.clone()).unwrap();
    assert!(a.is_stopped());
    assert!(b.is_stopped());
    assert!(
        !foreign.is_stopped(),
        "MANAGER native exchange pointer filter"
    );
    assert_eq!(
        m.instances().borrow().len(),
        2,
        "MANAGER native cleanup snapshot floor"
    );
    assert_eq!(m.instances().borrow()[1].id(), "new");
    assert!(
        !m.instances().borrow()[1].is_stopped(),
        "MANAGER native new member excluded from snapshot"
    );
    assert_eq!(
        b.state().borrow().stop_reason,
        "listener stop",
        "MANAGER native live stopped check"
    );
    assert_eq!(
        &*h.events.borrow(),
        &[
            Event::Emit(
                "warning".into(),
                "Chaser for BTC was stopped because the exchange connection was disposed".into()
            ),
            Event::Cleanup(
                "a".into(),
                vec![],
                "Exchange disposed while chase was active".into(),
                4
            ),
            Event::Cleanup("b".into(), vec![], "listener stop".into(), 4),
            Event::Cleanup(
                "a".into(),
                vec![],
                "Exchange disposed while chase was active".into(),
                3
            )
        ],
        "MANAGER native cleanup snapshot effects"
    );
    let events = profile(&h.events.borrow());
    let (fail, fops, frt, fex) = fixture();
    let x = instance(fex.clone(), &frt, "x");
    let y = instance(fex.clone(), &frt, "y");
    fail.instances().borrow_mut().extend([x.clone(), y.clone()]);
    fex.base().on(
        "warning",
        Rc::new(|_, _| Err(ExchangeError::new("warning failed"))),
    );
    assert_eq!(
        fail.cleanup_exchange(fex.clone(), Some("custom"), fops.clone())
            .unwrap_err()
            .message,
        "warning failed"
    );
    assert!(x.is_stopped());
    assert_eq!(x.state().borrow().stop_reason, "custom");
    assert!(
        !y.is_stopped(),
        "MANAGER native warning throw stops later members"
    );
    assert_eq!(
        fops.events.borrow().len(),
        1,
        "MANAGER native warning throw skips cleanup"
    );
    assert!(fops.tasks.borrow().is_empty());
    assert_eq!(
        source()["exchange"],
        json!({"events":events,"remaining":["foreign","new"],"throwStopped":[true,false],"throwEvents":profile(&fops.events.borrow())})
    );
    ex.base().off("warning", None);
    h.tasks.borrow_mut().clear();
    h.cleanup_hook.borrow_mut().take();
}
#[test]
fn remove_and_keep_select_identity_and_recheck_index_after_disposal() {
    let (m, h, rt, ex) = fixture();
    let first = instance(ex.clone(), &rt, "same");
    let second = instance(ex.clone(), &rt, "same");
    first.set_current_order_ids(vec!["o".into()]);
    m.instances()
        .borrow_mut()
        .extend([first.clone(), second.clone()]);
    ex.base().store.update(StoreUpdate{orders:Some(vec![serde_json::from_value(json!({"id":"o","symbol":"BTC","status":"closed","type":"limit","side":"buy","price":100,"amount":3,"filled":3,"remaining":0,"reduceOnly":false,"lastUpdate":0})).unwrap()]),..Default::default()},true).unwrap();
    let list = m.instances();
    let keep_first = first.clone();
    ex.base().on(
        "log",
        Rc::new(move |_, _| {
            list.borrow_mut().retain(|i| !Rc::ptr_eq(i, &keep_first));
            Ok(())
        }),
    );
    let selected = Rc::new(Cell::new(false));
    let selected_hook = selected.clone();
    let expected_first = first.clone();
    *h.cleanup_hook.borrow_mut() = Some(Rc::new(move |_, i| {
        selected_hook.set(Rc::ptr_eq(i, &expected_first))
    }));
    m.remove("missing", h.clone()).unwrap();
    assert!(h.events.borrow().is_empty());
    m.remove("same", h.clone()).unwrap();
    assert!(
        selected.get(),
        "MANAGER native first selected identity retained"
    );
    assert!(
        !second.is_stopped(),
        "MANAGER native second same ID untouched"
    );
    assert_eq!(
        first.state().borrow().stop_reason,
        "Completed",
        "MANAGER native completion before cleanup"
    );
    assert!(first.current_order_ids().is_empty());
    assert_eq!(
        &*h.events.borrow(),
        &[
            Event::Emit("log".into(), "[CHASERsame] my order ids o:closed".into()),
            Event::Cleanup("same".into(), vec![], "Completed".into(), 1)
        ],
        "MANAGER native remove effects"
    );
    let removed = profile(&h.events.borrow());
    let (k, kh, krt, kex) = fixture();
    let p = instance(kex.clone(), &krt, "same");
    let q = instance(kex.clone(), &krt, "same");
    let z = instance(kex, &krt, "z");
    p.set_current_order_ids(vec!["live".into()]);
    k.instances()
        .borrow_mut()
        .extend([p.clone(), p.clone(), q.clone(), z.clone()]);
    let ids_seen = p.clone();
    let list = k.instances();
    let ev = kh.events.clone();
    p.register_disposer(Box::new(move || {
        assert!(
            ids_seen.current_order_ids().is_empty(),
            "MANAGER native keep clears before dispose"
        );
        ev.borrow_mut().push(Event::Dispose(
            list.borrow().len(),
            ids_seen.current_order_ids().len(),
        ));
        list.borrow_mut().remove(0);
        list.borrow_mut().reverse();
        Ok(())
    }));
    k.remove_keep_order("missing", kh.clone());
    assert!(kh.events.borrow().is_empty());
    k.remove_keep_order("same", kh.clone());
    assert!(p.is_stopped());
    assert_eq!(
        p.state().borrow().stop_reason,
        "User cancelled, kept order in book"
    );
    assert!(!q.is_stopped());
    assert_eq!(
        k.instances().borrow().len(),
        2,
        "MANAGER native keep removes one current identity"
    );
    assert!(Rc::ptr_eq(&k.instances().borrow()[0], &z));
    assert!(Rc::ptr_eq(&k.instances().borrow()[1], &q));
    assert_eq!(
        p.ever_tracked_order_ids()
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        ["live"]
    );
    assert!(
        !p.claim_complete_emission(),
        "MANAGER native keep completion already claimed"
    );
    k.instances().borrow_mut().clear();
    k.instances().borrow_mut().push(p.clone());
    k.remove_keep_order("same", kh.clone());
    assert!(k.instances().borrow().is_empty());
    assert_eq!(
        &*kh.events.borrow(),
        &[Event::Dispose(4, 0), Event::Complete("same".into(), 2)],
        "MANAGER native single keep completion and no cleanup"
    );
    let kept = profile(&kh.events.borrow());
    assert_eq!(source()["removal"], json!({"remove":removed,"keep":kept}));
    ex.base().off("log", None);
    h.tasks.borrow_mut().clear();
    kh.tasks.borrow_mut().clear();
    h.cleanup_hook.borrow_mut().take();
}
