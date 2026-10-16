#![allow(dead_code)]
use safe_cex_venue_woo::{
    legacy_lifecycle::LifecycleOps, legacy_metadata::Task, public_canonical::CanonicalOps,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
pub(crate) enum Js {
    U,
    Null,
    B(bool),
    N(f64),
    S(String),
    O(RefCell<Vec<(String, V)>>),
    A(RefCell<Vec<V>>),
    F(Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>),
    P(Rc<Promise>),
}
pub(crate) type V = Rc<Js>;
pub(crate) type E = Rc<String>;
pub(crate) struct Promise {
    pub(crate) task: RefCell<Option<Task<V, E>>>,
    pub(crate) result: RefCell<Option<Result<V, E>>>,
}
pub(crate) fn u() -> V {
    Rc::new(Js::U)
}
pub(crate) fn n(v: f64) -> V {
    Rc::new(Js::N(v))
}
pub(crate) fn s(v: &str) -> V {
    Rc::new(Js::S(v.into()))
}
pub(crate) fn b(v: bool) -> V {
    Rc::new(Js::B(v))
}
pub(crate) fn obj(v: Vec<(&str, V)>) -> V {
    Rc::new(Js::O(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
pub(crate) fn arr(v: Vec<V>) -> V {
    Rc::new(Js::A(RefCell::new(v)))
}
pub(crate) fn function(f: impl Fn(V, Vec<V>) -> Result<V, E> + 'static) -> V {
    Rc::new(Js::F(Rc::new(f)))
}
pub(crate) fn call(f: V, r: V, a: Vec<V>) -> Result<V, E> {
    let Js::F(f) = f.as_ref() else {
        panic!("fixture callable")
    };
    f(r, a)
}
pub(crate) fn text(v: &V) -> String {
    match v.as_ref() {
        Js::S(s) => s.clone(),
        Js::N(n) => safe_cex_num::js_number_string(*n),
        Js::U => "undefined".into(),
        _ => panic!("fixture text"),
    }
}
pub(crate) fn number(v: &V) -> f64 {
    match v.as_ref() {
        Js::N(n) => *n,
        Js::S(s) => s.parse().unwrap_or(f64::NAN),
        Js::Null => 0.,
        _ => f64::NAN,
    }
}
pub(crate) fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Js::U | Js::Null => false,
        Js::B(b) => *b,
        Js::N(n) => *n != 0. && !n.is_nan(),
        Js::S(s) => !s.is_empty(),
        _ => true,
    }
}
pub(crate) fn set(v: &V, key: &str, new: V) {
    let Js::O(fields) = v.as_ref() else {
        panic!("fixture set")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = new
    } else {
        fields.push((key.into(), new))
    }
}
pub(crate) fn get(v: &V, key: &str) -> Result<V, E> {
    match v.as_ref() {
        Js::O(fields) => Ok(fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u)),
        Js::A(rows) => match key {
            "length" => Ok(n(rows.borrow().len() as f64)),
            "push" => Ok(function(|r, a| {
                let Js::A(rows) = r.as_ref() else {
                    panic!("push")
                };
                rows.borrow_mut().extend(a);
                Ok(n(rows.borrow().len() as f64))
            })),
            _ => Ok(key
                .parse::<usize>()
                .ok()
                .and_then(|i| rows.borrow().get(i).cloned())
                .unwrap_or_else(u)),
        },
        Js::U | Js::Null => Err(Rc::new(format!("read {key} on nullish"))),
        _ => Ok(u()),
    }
}
pub(crate) fn literal(v: &V) -> Value {
    match v.as_ref() {
        Js::U => json!({"undefined":true}),
        Js::Null => Value::Null,
        Js::B(v) => json!(v),
        Js::N(v) => json!(v),
        Js::S(v) => json!(v),
        Js::A(v) => Value::Array(v.borrow().iter().map(literal).collect()),
        Js::O(v) => Value::Object(
            v.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), literal(v)))
                .collect(),
        ),
        _ => panic!("fixture literal"),
    }
}
pub(crate) fn decode(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Js::Null),
        Value::Bool(v) => b(*v),
        Value::Number(v) => n(v.as_f64().unwrap()),
        Value::String(v) => s(v),
        Value::Array(v) => arr(v.iter().map(decode).collect()),
        Value::Object(v) => Rc::new(Js::O(RefCell::new(
            v.iter().map(|(k, v)| (k.clone(), decode(v))).collect(),
        ))),
    }
}
pub(crate) fn poll(v: &V) -> Poll<Result<V, E>> {
    let Js::P(p) = v.as_ref() else {
        return Poll::Ready(Ok(v.clone()));
    };
    if let Some(r) = p.result.borrow().clone() {
        return Poll::Ready(r);
    }
    let mut t = p.task.borrow_mut().take().expect("retained promise task");
    let result = t.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    match result {
        Poll::Ready(r) => {
            *p.result.borrow_mut() = Some(r.clone());
            Poll::Ready(r)
        }
        Poll::Pending => {
            *p.task.borrow_mut() = Some(t);
            Poll::Pending
        }
    }
}
pub(crate) fn pending() -> V {
    Rc::new(Js::P(Rc::new(Promise {
        task: RefCell::new(Some(Box::pin(std::future::pending()))),
        result: RefCell::new(None),
    })))
}
pub(crate) fn resolve(v: &V, result: Result<V, E>) {
    let Js::P(p) = v.as_ref() else {
        panic!("promise")
    };
    assert!(p.result.borrow().is_none());
    *p.result.borrow_mut() = Some(result);
}
pub(crate) fn wait(v: V) -> Task<V, E> {
    let mut yielded = false;
    Box::pin(futures_util::future::poll_fn(move |_| {
        if !yielded {
            yielded = true;
            return Poll::Pending;
        }
        poll(&v)
    }))
}
#[derive(Default)]
pub(crate) struct Runtime {
    pub(crate) jobs: RefCell<Vec<V>>,
    pub(crate) trace: RefCell<Vec<String>>,
    pub(crate) args: RefCell<Vec<(String, Vec<V>)>>,
    pub(crate) imports: RefCell<BTreeMap<String, V>>,
}
impl Runtime {
    pub(crate) fn promise(&self, t: Task<V, E>) -> V {
        let v = Rc::new(Js::P(Rc::new(Promise {
            task: RefCell::new(Some(t)),
            result: RefCell::new(None),
        })));
        self.jobs.borrow_mut().push(v.clone());
        let _ = poll(&v);
        v
    }
    pub(crate) fn pump(&self) {
        for _ in 0..32 {
            let jobs = self.jobs.borrow().clone();
            for v in jobs {
                let _ = poll(&v);
            }
        }
    }
    pub(crate) fn bind(self: &Rc<Self>, name: &str, result: V) -> V {
        let h = self.clone();
        let name = name.to_owned();
        function(move |_, a| {
            h.trace.borrow_mut().push(name.clone());
            h.args.borrow_mut().push((name.clone(), a));
            Ok(result.clone())
        })
    }
    pub(crate) fn operations(self: &Rc<Self>) -> Rc<LifecycleOps<V, E>> {
        let h = self.clone();
        let invokes = self.clone();
        let global = self.clone();
        let value = Rc::new(CanonicalOps {
            undefined: Rc::new(u),
            object: Rc::new(obj),
            array: Rc::new(arr),
            get: Rc::new(move |v, k| {
                if let Js::P(_) = v.as_ref() {
                    if k == "then" {
                        let h = h.clone();
                        return Ok(function(move |r, a| {
                            let cb = a[0].clone();
                            let future = wait(r);
                            Ok(h.promise(Box::pin(async move {
                                let v = future.await?;
                                call(cb, u(), vec![v])
                            })))
                        }));
                    }
                }
                get(v, k)
            }),
            global: Rc::new(move |name| {
                Ok(match name {
                    "Date" => obj(vec![("now", global.bind("now", n(3_000_000_000.)))]),
                    "Array" => obj(vec![(
                        "isArray",
                        function(|_, a| Ok(b(matches!(a[0].as_ref(), Js::A(_))))),
                    )]),
                    "Promise" => {
                        let h = global.clone();
                        obj(vec![(
                            "all",
                            function(move |_, a| {
                                let Js::A(values) = a[0].as_ref() else {
                                    panic!("all")
                                };
                                let futures = values
                                    .borrow()
                                    .iter()
                                    .cloned()
                                    .map(wait)
                                    .collect::<Vec<_>>();
                                Ok(h.promise(Box::pin(async move {
                                    let rows = futures_util::future::try_join_all(futures).await?;
                                    Ok(arr(rows))
                                })))
                            }),
                        )])
                    }
                    _ => panic!("global {name}"),
                })
            }),
            invoke: Rc::new(move |f, r, a| {
                let _ = &invokes;
                call(f, r, a)
            }),
            function: Rc::new(|_, _, f| function(move |_, a| f(a))),
            spread: Rc::new(|v| {
                let Js::A(v) = v.as_ref() else {
                    panic!("spread")
                };
                Ok(v.borrow().clone())
            }),
            truthy: Rc::new(truthy),
            strict_equal: Rc::new(|a, b| match (a.as_ref(), b.as_ref()) {
                (Js::N(a), Js::N(b)) => a == b,
                (Js::S(a), Js::S(b)) => a == b,
                _ => Rc::ptr_eq(a, b),
            }),
            string: Rc::new(s),
            number: Rc::new(n),
            subtract: Rc::new(|a, b| Ok(n(number(&a) - number(&b)))),
            get_key: Rc::new(|v, k| get(v, &text(k))),
            error: Rc::new(|s| Rc::new(s.into())),
        });
        let h = self.clone();
        let imports = self.clone();
        Rc::new(LifecycleOps {
            value,
            await_value: Rc::new(wait),
            promise: Rc::new(move |t| h.promise(t)),
            import: Rc::new(move |k| {
                imports
                    .imports
                    .borrow()
                    .get(k)
                    .cloned()
                    .ok_or_else(|| Rc::new(format!("unbound {k}")))
            }),
            spread_object: Rc::new(|v, fields| {
                let Js::O(v) = v.as_ref() else {
                    panic!("object spread")
                };
                let result = Rc::new(Js::O(RefCell::new(v.borrow().clone())));
                for (k, v) in fields {
                    set(&result, k, v);
                }
                Ok(result)
            }),
            is_nullish: Rc::new(|v| matches!(v.as_ref(), Js::U | Js::Null)),
            is_undefined: Rc::new(|v| matches!(v.as_ref(), Js::U)),
            boolean: Rc::new(b),
            add: Rc::new(|a, b| {
                Ok(
                    if matches!(a.as_ref(), Js::S(_)) || matches!(b.as_ref(), Js::S(_)) {
                        s(&format!("{}{}", text(&a), text(&b)))
                    } else {
                        n(number(&a) + number(&b))
                    },
                )
            }),
            remainder: Rc::new(|a, b| Ok(n(number(&a) % number(&b)))),
            less: Rc::new(|a, b| Ok(number(&a) < number(&b))),
            error_value: Rc::new(|e| obj(vec![("message", s(&e))])),
            binding: Rc::new(|v, pattern| {
                assert_eq!(
                    pattern,
                    &[false, true],
                    "LIFECYCLE-NATIVE startup binding elision"
                );
                let Js::A(v) = v.as_ref() else {
                    panic!("binding")
                };
                Ok(pattern
                    .iter()
                    .enumerate()
                    .map(|(i, read)| {
                        if *read {
                            v.borrow().get(i).cloned().unwrap_or_else(u)
                        } else {
                            u()
                        }
                    })
                    .collect())
            }),
            iterate: Rc::new(|v, cb| {
                let Js::A(v) = v.as_ref() else {
                    panic!("iterator")
                };
                let mut i = 0;
                loop {
                    let next = v.borrow().get(i).cloned();
                    let Some(next) = next else { break };
                    cb(next)?;
                    i += 1;
                }
                Ok(())
            }),
        })
    }
}
pub(crate) fn receiver(h: &Rc<Runtime>) -> V {
    let store = obj(vec![("loaded", obj(vec![("old", b(true))]))]);
    let c = h.clone();
    let target = store.clone();
    set(
        &store,
        "update",
        function(move |_, args| {
            let patch = args[0].clone();
            let Js::O(fields) = patch.as_ref() else {
                panic!("patch")
            };
            let keys = fields
                .borrow()
                .iter()
                .map(|(k, _)| k.clone())
                .collect::<Vec<_>>();
            c.trace
                .borrow_mut()
                .push(format!("update:{}", keys.join(",")));
            c.args.borrow_mut().push(("update".into(), args));
            for (k, v) in fields.borrow().iter() {
                set(&target, k, v.clone());
            }
            Ok(u())
        }),
    );
    set(
        &store,
        "addOrUpdatePositionsFull",
        h.bind("positions:update", u()),
    );
    obj(vec![
        ("store", store),
        ("isDisposed", b(false)),
        ("isPublicDataOnly", b(false)),
        (
            "publicWebsocket",
            obj(vec![("connectAndSubscribe", h.bind("public", u()))]),
        ),
        (
            "privateWebsocket",
            obj(vec![("connectAndSubscribe", h.bind("private", u()))]),
        ),
        ("startFundingRatesRefresh", h.bind("funding", u())),
        ("tick", h.bind("tick", u())),
        ("log", h.bind("log", u())),
        ("emitAndLogError", h.bind("emit", u())),
        ("getTickInterval", h.bind("interval", n(137.))),
    ])
}
pub(crate) fn outcome(h: &Runtime, p: &V) -> Result<V, E> {
    h.pump();
    match poll(p) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("LIFECYCLE-NATIVE pending outcome"),
    }
}
pub(crate) fn assert_trace(h: &Runtime, expected: &[&str]) {
    assert_eq!(
        *h.trace.borrow(),
        expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "LIFECYCLE-NATIVE exact call order"
    );
}
pub(crate) fn source_cases(cases: &[Value]) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/legacy-lifecycle-oracle.cjs"
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
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "source lifecycle: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
