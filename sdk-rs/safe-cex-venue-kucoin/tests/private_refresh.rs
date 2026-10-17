use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kucoin::private_refresh::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<ExchangeError>;
#[derive(Clone)]
enum Handle {
    Number(i32),
    Object(Rc<u8>),
}
fn truthy(h: &Handle) -> bool {
    match h {
        Handle::Number(n) => *n != 0,
        Handle::Object(_) => true,
    }
}
fn tag(h: Option<&Handle>) -> String {
    match h {
        None => "none".into(),
        Some(Handle::Number(n)) => format!("number:{n}"),
        Some(Handle::Object(o)) => format!("object:{}", **o),
    }
}
fn error() -> Error {
    let mut e = ExchangeError::new("refresh failure");
    e.name = "TypeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::Js::str("REFRESH"));
    e.store_subscriber_ids = vec![7, 3];
    Rc::new(e)
}
struct Awaited<T> {
    ready: Rc<Cell<bool>>,
    result: Option<Result<T, Error>>,
}
impl<T: Unpin> Future for Awaited<T> {
    type Output = Result<T, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let s = self.get_mut();
        if s.ready.get() {
            Poll::Ready(s.result.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
enum Callback<'a> {
    Sync(Box<dyn FnOnce() -> Result<(), Error> + 'a>),
    Async(Box<dyn FnOnce() -> RefreshFuture<'a, (), Error> + 'a>),
}
fn fire<'a>(callback: Callback<'a>) -> RefreshFuture<'a, (), Error> {
    match callback {
        Callback::Sync(f) => {
            let result = f();
            Box::pin(async move { result })
        }
        Callback::Async(f) => f(),
    }
}
fn assert_error(result: Result<(), Error>, fails: bool, cause: &Error, label: &str) -> Value {
    if fails {
        let e = result.expect_err(label);
        assert_eq!(*e, **cause, "{label} native error fields");
        assert!(Rc::ptr_eq(&e, cause), "{label} native error identity");
        json!({"error":true})
    } else {
        result.expect(label);
        json!({"ok":true})
    }
}
#[test]
fn private_refresh_native_before_source() {
    let scenarios = [
        "none",
        "zero",
        "number",
        "object",
        "disposed",
        "clear-error",
        "set-error",
        "dispose-fire",
        "inline",
        "fetch-error",
        "dispose-await",
        "loaded-mutate",
        "reentrant",
        "superseded",
    ];
    let mut cases = vec![];
    let mut native = vec![];
    for kind in ["snapshot", "orders", "balance"] {
        for scenario in scenarios {
            let old_object = Rc::new(13);
            let new_object = Rc::new(9);
            let replaced = Rc::new(17);
            let balance = Rc::new(55);
            let old = match scenario {
                "zero" => Some(Handle::Number(0)),
                "number" | "clear-error" => Some(Handle::Number(7)),
                "object" => Some(Handle::Object(old_object.clone())),
                _ => None,
            };
            let timer = RefCell::new(old.clone());
            let disposed = Cell::new(scenario == "disposed");
            let events = RefCell::new(Vec::<String>::new());
            let ready = Rc::new(Cell::new(false));
            let loaded_orders = Cell::new(false);
            let update = RefCell::new(None::<(Rc<u8>, bool)>);
            let cause = error();
            let fetched = || {
                events
                    .borrow_mut()
                    .push(format!("fetch:{}", tag(timer.borrow().as_ref())));
                if scenario == "reentrant" {
                    *timer.borrow_mut() = Some(Handle::Object(replaced.clone()));
                }
            };
            let queued = RefCell::new(None::<Callback<'_>>);
            let pending = RefCell::new(None::<RefreshFuture<'_, (), Error>>);
            let schedule = |delay: u32, callback| -> Result<Handle, Error> {
                events
                    .borrow_mut()
                    .push(format!("set:{delay}:{}", tag(timer.borrow().as_ref())));
                if scenario == "set-error" {
                    return Err(cause.clone());
                }
                if scenario == "inline" {
                    *pending.borrow_mut() = Some(fire(callback));
                } else {
                    *queued.borrow_mut() = Some(callback);
                }
                Ok(Handle::Object(new_object.clone()))
            };
            let scheduled = match kind {
                "snapshot" => refresh_snapshot_soon(
                    &timer,
                    &disposed,
                    truthy,
                    |h| {
                        events.borrow_mut().push(format!("clear:{}", tag(Some(&h))));
                        if let Handle::Object(o) = &h {
                            assert!(
                                Rc::ptr_eq(o, &old_object),
                                "REFRESH native old clear handle identity"
                            );
                        }
                        if scenario == "clear-error" {
                            return Err(cause.clone());
                        }
                        Ok(())
                    },
                    |delay, cb| schedule(delay, Callback::Sync(cb)),
                    || {
                        fetched();
                        if scenario == "fetch-error" {
                            Err(cause.clone())
                        } else {
                            Ok(())
                        }
                    },
                ),
                "orders" => refresh_orders_soon(
                    &timer,
                    &disposed,
                    truthy,
                    |delay, cb| schedule(delay, Callback::Async(cb)),
                    || {
                        fetched();
                        Box::pin(Awaited {
                            ready: ready.clone(),
                            result: Some(if scenario == "fetch-error" {
                                Err(cause.clone())
                            } else {
                                Ok(())
                            }),
                        })
                    },
                ),
                _ => refresh_balance_soon(
                    &timer,
                    &disposed,
                    truthy,
                    |delay, cb| schedule(delay, Callback::Async(cb)),
                    || {
                        fetched();
                        Box::pin(Awaited {
                            ready: ready.clone(),
                            result: Some(if scenario == "fetch-error" {
                                Err(cause.clone())
                            } else {
                                Ok(balance.clone())
                            }),
                        })
                    },
                    |b| {
                        events.borrow_mut().push("update".into());
                        *update.borrow_mut() = Some((b, loaded_orders.get()));
                        Ok(())
                    },
                ),
            };
            let skip = kind != "snapshot"
                && matches!(scenario, "number" | "object" | "clear-error" | "disposed");
            let clear =
                kind == "snapshot" && matches!(scenario, "number" | "object" | "clear-error");
            let schedule_failure = !skip
                && (scenario == "set-error" || scenario == "clear-error" && kind == "snapshot");
            let scheduled_result =
                assert_error(scheduled, schedule_failure, &cause, "REFRESH schedule");
            let mut expected = vec![];
            if clear {
                expected.push(format!("clear:{}", tag(old.as_ref())));
            }
            let fires = !skip && !schedule_failure;
            if !skip && !(clear && scenario == "clear-error") {
                expected.push(format!(
                    "set:{}:{}",
                    if kind == "snapshot" { 500 } else { 250 },
                    tag(old.as_ref())
                ));
            }
            let fetch = fires && !matches!(scenario, "disposed" | "dispose-fire");
            if scenario == "inline" && fetch {
                expected.push("fetch:none".into());
            }
            assert_eq!(
                *events.borrow(),
                expected,
                "REFRESH native schedule order/delay"
            );
            let expected_slot = if skip || schedule_failure {
                tag(old.as_ref())
            } else {
                "object:9".into()
            };
            assert_eq!(
                tag(timer.borrow().as_ref()),
                expected_slot,
                "REFRESH native assignment after schedule"
            );
            if !skip && !schedule_failure {
                if let Some(Handle::Object(h)) = timer.borrow().as_ref() {
                    assert!(
                        Rc::ptr_eq(h, &new_object),
                        "REFRESH native new handle identity"
                    );
                } else {
                    panic!("REFRESH new handle absent");
                }
            }
            let mut callback_result = json!(null);
            if fires {
                if scenario == "dispose-fire" {
                    disposed.set(true);
                }
                if scenario == "superseded" {
                    *timer.borrow_mut() = Some(Handle::Object(replaced.clone()));
                }
                if let Some(cb) = queued.borrow_mut().take() {
                    *pending.borrow_mut() = Some(fire(cb));
                }
                if scenario != "inline" && fetch {
                    expected.push("fetch:none".into());
                }
                assert_eq!(
                    *events.borrow(),
                    expected,
                    "REFRESH native firing prefix before await polling"
                );
                let after_fire = if scenario == "inline" {
                    "object:9"
                } else if scenario == "reentrant" {
                    "object:17"
                } else {
                    "none"
                };
                assert_eq!(
                    tag(timer.borrow().as_ref()),
                    after_fire,
                    "REFRESH native current timer reset before fetch"
                );
                let mut future = pending.borrow_mut().take().unwrap();
                let waker = Waker::noop().clone();
                let mut cx = Context::from_waker(&waker);
                let first = future.as_mut().poll(&mut cx);
                if kind != "snapshot" && fetch {
                    assert!(first.is_pending(), "REFRESH native real await boundary");
                } else {
                    assert!(first.is_ready(), "REFRESH native no unnecessary await");
                }
                if scenario == "dispose-await" {
                    disposed.set(true);
                }
                if scenario == "loaded-mutate" {
                    loaded_orders.set(true);
                }
                ready.set(true);
                let resolved = match first {
                    Poll::Ready(v) => v,
                    Poll::Pending => match future.as_mut().poll(&mut cx) {
                        Poll::Ready(v) => v,
                        Poll::Pending => panic!("REFRESH pending after resolve"),
                    },
                };
                callback_result = assert_error(
                    resolved,
                    fetch && scenario == "fetch-error",
                    &cause,
                    "REFRESH callback",
                );
                let updated = kind == "balance"
                    && fetch
                    && scenario != "fetch-error"
                    && scenario != "dispose-await";
                if updated {
                    expected.push("update".into());
                    let value = update.borrow();
                    let (b, orders) = value
                        .as_ref()
                        .expect("REFRESH native balance update missing");
                    assert!(
                        Rc::ptr_eq(b, &balance),
                        "REFRESH native decoded balance identity"
                    );
                    assert_eq!(**b, 55, "REFRESH native balance pointee");
                    assert_eq!(
                        *orders,
                        scenario == "loaded-mutate",
                        "REFRESH native current loaded state"
                    );
                } else {
                    assert!(
                        update.borrow().is_none(),
                        "REFRESH native forbidden balance update"
                    );
                }
                assert_eq!(
                    *events.borrow(),
                    expected,
                    "REFRESH native await completion order"
                );
            }
            cases.push(json!({"kind":kind,"scenario":scenario}));
            native.push(json!({"trace":events.borrow().clone(),"timer":tag(timer.borrow().as_ref()),"schedule":scheduled_result,"callback":callback_result,"update":update.borrow().as_ref().map(|(b,o)|json!({"balance":**b,"markets":true,"orders":o,"loadedBalance":true}))}));
        }
    }
    assert_eq!(cases.len(), 42, "REFRESH independent scenario floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-refresh.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json!(native),
        source,
        "source/wire comparison after all native assertions"
    );
}
