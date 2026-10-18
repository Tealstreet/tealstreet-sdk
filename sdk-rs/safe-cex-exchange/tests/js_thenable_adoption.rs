//! Thenable adoption through production NativeRuntime and installed globals.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        promise::{self, Promise, await_js},
        scope::{Scope, ScopeShared},
        value::{Js, Thrown, arg},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::Arc,
};

type Trace = Rc<RefCell<Vec<String>>>;
const MODES: &[&str] = &[
    "fulfill",
    "nan",
    "reject",
    "getter-throw",
    "call-throw",
    "resolve-then-throw",
    "reject-then-resolve",
    "nested",
    "noncallable",
    "pending-first",
];
fn run<F: Future<Output = ()>>(body: impl FnOnce(Scope<()>) -> F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        body(Scope {
            shared,
            this: Js::Undefined,
        })
        .await;
    });
}
fn oracle() -> &'static Value {
    static SOURCE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_thenable_adoption_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn thenable(
    scope: &Scope<()>,
    mode: &'static str,
    label: &'static str,
    token: Js,
    trace: Trace,
) -> Js {
    let value = Js::object(vec![]);
    let runtime = scope.exchange_runtime();
    let constructor = scope.global("Promise").unwrap();
    value.define_accessor(
        "then",
        Some(Rc::new(move |owner| {
            trace.borrow_mut().push(format!("get:{label}"));
            if mode == "getter-throw" {
                return Err(Thrown::new(token.clone()));
            }
            if mode == "noncallable" {
                return Ok(Js::Number(7.));
            }
            // Capture the actual accessor receiver, never a test replacement Promise.
            let owner = owner.clone();
            let trace = trace.clone();
            let token = token.clone();
            let runtime = runtime.clone();
            let constructor = constructor.clone();
            let inner = if mode == "nested" {
                let nested = Js::object(vec![]);
                let seen = trace.clone();
                nested.define_accessor(
                    "then",
                    Some(Rc::new(move |owner| {
                        seen.borrow_mut().push("get:inner".into());
                        let owner = owner.clone();
                        let seen = seen.clone();
                        Ok(Js::function("innerThen", move |this, args| {
                            seen.borrow_mut()
                                .push(format!("call:inner:{}", this.same_reference(&owner)));
                            arg(&args, 0).call(Js::Undefined, vec![Js::Number(-0.)])?;
                            Ok(Js::Undefined)
                        }))
                    })),
                    None,
                );
                nested
            } else {
                Js::Undefined
            };
            Ok(Js::function("then", move |this, args| {
                trace
                    .borrow_mut()
                    .push(format!("call:{label}:{}", this.same_reference(&owner)));
                let resolve = arg(&args, 0);
                let reject = arg(&args, 1);
                match mode {
                    "reject" | "reject-then-resolve" => {
                        reject.call(Js::Undefined, vec![token.clone()])?;
                        if mode == "reject-then-resolve" {
                            resolve.call(Js::Undefined, vec![Js::Number(-0.)])?;
                        }
                    }
                    "call-throw" => return Err(Thrown::new(token.clone())),
                    "nested" => {
                        resolve.call(Js::Undefined, vec![inner.clone()])?;
                    }
                    "pending-first" => {
                        let pending = Promise::pending(&runtime);
                        resolve.call(Js::Undefined, vec![Js::Promise(pending.clone())])?;
                        reject.call(Js::Undefined, vec![token.clone()])?;
                        constructor.call_method("resolve", vec![])?.call_method(
                            "then",
                            vec![Js::function("settle", move |_, _| {
                                pending.settle(Ok(Js::Number(-0.)));
                                Ok(Js::Undefined)
                            })],
                        )?;
                    }
                    _ => {
                        resolve.call(
                            Js::Undefined,
                            vec![Js::Number(if mode == "nan" { f64::NAN } else { -0. })],
                        )?;
                        if mode == "resolve-then-throw" {
                            return Err(Thrown::new(token.clone()));
                        }
                    }
                }
                Ok(Js::Undefined)
            }))
        })),
        None,
    );
    value
}
// Observe native identity, number bits and UTF16 units BEFORE any JSON projection.
fn project(result: Result<Js, Thrown>, input: &Js, token: &Js) -> Value {
    let (ok, value) = match result {
        Ok(value) => (true, value),
        Err(error) => (false, error.value),
    };
    let bits = match &value {
        Js::Number(n) if n.is_nan() => Some("nan".into()),
        Js::Number(n) => Some(format!("{:016x}", n.to_bits())),
        _ => None,
    };
    let units = if !ok {
        match value.get("unit").unwrap() {
            Js::String(s) => Some(s.to_vec()),
            _ => None,
        }
    } else {
        None
    };
    json!({"ok":ok,"same":value.same_reference(if ok {input} else {token}),"bits":bits,"units":units})
}
async fn matrix(scope: Scope<()>, route: &'static str) {
    let mut rows = vec![];
    for &mode in MODES {
        let trace = Trace::default();
        let token = Js::object(vec![("unit", Js::String(Rc::from([0xd800_u16])))]);
        let input = thenable(&scope, mode, "outer", token.clone(), trace.clone());
        let runtime = scope.exchange_runtime();
        let result = if route == "await" {
            // Capture INSIDE the async function: outer async-return adoption must
            // never hide a direct-await failure.
            let observed = Rc::new(RefCell::new(None));
            let sink = observed.clone();
            let value = input.clone();
            let driver = Promise::spawn(
                &runtime,
                Box::pin(async move {
                    *sink.borrow_mut() = Some(await_js(value).await);
                    Ok(Js::Undefined)
                }),
            );
            trace.borrow_mut().push("returned".into());
            driver.wait().await.unwrap();
            observed.borrow_mut().take().unwrap()
        } else if route == "return" {
            let value = input.clone();
            let driver = Promise::spawn(&runtime, Box::pin(async move { Ok(value) }));
            trace.borrow_mut().push("returned".into());
            driver.wait().await
        } else {
            let value = if route == "resolve" {
                input.clone()
            } else {
                Js::array(vec![input.clone()])
            };
            let constructor = scope.global("Promise").unwrap();
            let pending = if route == "nativeAll" {
                Js::Promise(promise::all(&runtime, vec![input.clone()]))
            } else if route == "nativeAllSettled" {
                Js::Promise(promise::all_settled(&runtime, vec![input.clone()]))
            } else if matches!(route, "then" | "finally") {
                let returned = input.clone();
                constructor
                    .call_method("resolve", vec![Js::Number(-0.)])
                    .unwrap()
                    .call_method(
                        route,
                        vec![Js::function("callback", move |_, _| Ok(returned.clone()))],
                    )
                    .unwrap()
            } else {
                constructor.call_method(route, vec![value]).unwrap()
            };
            let Js::Promise(promise) = pending else {
                panic!("native promise")
            };
            trace.borrow_mut().push("returned".into());
            match promise.wait().await {
                Ok(value) if matches!(route, "all" | "nativeAll") => Ok(value.get("0").unwrap()),
                Ok(value) if matches!(route, "allSettled" | "nativeAllSettled") => {
                    let row = value.get("0").unwrap();
                    if row.get("status").unwrap().to_display() == "rejected" {
                        Err(Thrown::new(row.get("reason").unwrap()))
                    } else {
                        Ok(row.get("value").unwrap())
                    }
                }
                result => result,
            }
        };
        rows.push(
            json!({"mode":mode,"trace":*trace.borrow(),"observed":project(result,&input,&token)}),
        );
    }
    assert_eq!(
        json!(rows),
        oracle()[route],
        "{route}: production adoption vs engine"
    );
}
#[test]
fn direct_await_adopts_thenables() {
    run(|scope| matrix(scope, "await"));
}
#[test]
fn async_return_adopts_thenables() {
    run(|scope| matrix(scope, "return"));
}
#[test]
fn installed_all_adopts_thenables() {
    run(|scope| matrix(scope, "all"));
}
#[test]
fn installed_all_settled_adopts_thenables() {
    run(|scope| matrix(scope, "allSettled"));
}
#[test]
fn canonical_resolve_control() {
    run(|scope| matrix(scope, "resolve"));
}
#[test]
fn installed_then_adopts_callback_thenables() {
    run(|scope| matrix(scope, "then"));
}
#[test]
fn installed_finally_adopts_callback_thenables() {
    run(|scope| matrix(scope, "finally"));
}
#[test]
fn native_all_adopts_thenables() {
    run(|scope| matrix(scope, "nativeAll"));
}
#[test]
fn native_all_settled_adopts_thenables() {
    run(|scope| matrix(scope, "nativeAllSettled"));
}

#[test]
fn installed_combinators_interleave_iterator_steps_and_then_lookup() {
    run(|scope| async move {
        let mut rows = vec![];
        for method in ["all", "allSettled"] {
            for abrupt in ["none", "next", "done", "value", "then"] {
                let trace = Trace::default();
                let token = Js::object(vec![]);
                let index = Rc::new(Cell::new(0));
                let seen = trace.clone();
                let error = token.clone();
                let iterator = Js::object(vec![
                    (
                        "next",
                        Js::function("next", move |_, _| {
                            let i = index.replace(index.get() + 1);
                            seen.borrow_mut().push(format!("next:{i}"));
                            if i == 1 && abrupt == "next" {
                                return Err(Thrown::new(error.clone()));
                            }
                            if i == 2 {
                                return Ok(Js::object(vec![("done", Js::Bool(true))]));
                            }
                            let value = Js::object(vec![]);
                            let calls = seen.clone();
                            let thrown = error.clone();
                            value.define_accessor(
                                "then",
                                Some(Rc::new(move |_| {
                                    calls.borrow_mut().push(format!("get:{i}"));
                                    if i == 0 && abrupt == "then" {
                                        return Err(Thrown::new(thrown.clone()));
                                    }
                                    let calls = calls.clone();
                                    Ok(Js::function("then", move |_, args| {
                                        calls.borrow_mut().push(format!("call:{i}"));
                                        arg(&args, 0)
                                            .call(Js::Undefined, vec![Js::Number(i as f64)])?;
                                        Ok(Js::Undefined)
                                    }))
                                })),
                                None,
                            );
                            let row = Js::object(vec![("done", Js::Bool(false)), ("value", value)]);
                            if i == 1 && matches!(abrupt, "done" | "value") {
                                let seen = seen.clone();
                                let error = error.clone();
                                row.define_accessor(
                                    abrupt,
                                    Some(Rc::new(move |_| {
                                        seen.borrow_mut().push(format!("{abrupt}:throw"));
                                        Err(Thrown::new(error.clone()))
                                    })),
                                    None,
                                );
                            }
                            Ok(row)
                        }),
                    ),
                    ("return", {
                        let seen = trace.clone();
                        Js::function("return", move |_, _| {
                            seen.borrow_mut().push("return".into());
                            Ok(Js::object(vec![]))
                        })
                    }),
                ]);
                let iterable = Js::object(vec![]);
                let seen = trace.clone();
                scope
                    .global("Object")
                    .unwrap()
                    .call_method(
                        "defineProperty",
                        vec![
                            iterable.clone(),
                            Js::SymbolIterator,
                            Js::object(vec![(
                                "value",
                                Js::function("iterator", move |_, _| {
                                    seen.borrow_mut().push("iterator".into());
                                    Ok(iterator.clone())
                                }),
                            )]),
                        ],
                    )
                    .unwrap();
                let Js::Promise(promise) = scope
                    .global("Promise")
                    .unwrap()
                    .call_method(method, vec![iterable])
                    .unwrap()
                else {
                    panic!("native promise")
                };
                trace.borrow_mut().push("returned".into());
                let same = promise
                    .wait()
                    .await
                    .err()
                    .is_some_and(|e| e.value.same_reference(&token));
                rows.push(
                    json!({"method":method,"abrupt":abrupt,"trace":*trace.borrow(),"same":same}),
                );
            }
        }
        assert_eq!(json!(rows), oracle()["iterators"]);
    });
}
#[test]
fn scalar_and_native_promise_await_keep_one_fifo_boundary() {
    run(|scope| async move {
        let mut rows = vec![];
        for kind in ["scalar", "promise"] {
            let trace = Trace::default();
            let seen = trace.clone();
            let runtime = scope.exchange_runtime();
            let value = if kind == "scalar" {
                Js::Number(-0.)
            } else {
                scope
                    .global("Promise")
                    .unwrap()
                    .call_method("resolve", vec![Js::Number(-0.)])
                    .unwrap()
            };
            let body = Promise::spawn(
                &runtime,
                Box::pin(async move {
                    seen.borrow_mut().push("body".into());
                    await_js(value).await?;
                    seen.borrow_mut().push("resumed".into());
                    Ok(Js::Undefined)
                }),
            );
            let seen = trace.clone();
            scope
                .global("Promise")
                .unwrap()
                .call_method("resolve", vec![])
                .unwrap()
                .call_method(
                    "then",
                    vec![Js::function("reaction", move |_, _| {
                        seen.borrow_mut().push("reaction".into());
                        Ok(Js::Undefined)
                    })],
                )
                .unwrap();
            trace.borrow_mut().push("returned".into());
            body.wait().await.unwrap();
            rows.push(json!({"kind":kind,"trace":*trace.borrow()}));
        }
        assert_eq!(json!(rows), oracle()["fifo"]);
    });
}

#[test]
fn await_and_async_return_preserve_engine_job_turns() {
    run(|scope| async move {
        let mut rows = vec![];
        for route in ["await", "return"] {
            for kind in ["scalar", "promise", "thenable"] {
                let trace = Trace::default();
                let runtime = scope.exchange_runtime();
                let constructor = scope.global("Promise").unwrap();
                let value = match kind {
                    "scalar" => Js::Number(-0.),
                    "promise" => constructor
                        .call_method("resolve", vec![Js::Number(-0.)])
                        .unwrap(),
                    _ => {
                        let value = Js::object(vec![]);
                        let seen = trace.clone();
                        value.define_accessor(
                            "then",
                            Some(Rc::new(move |_| {
                                seen.borrow_mut().push("get".into());
                                let seen = seen.clone();
                                Ok(Js::function("then", move |_, args| {
                                    seen.borrow_mut().push("call".into());
                                    arg(&args, 0).call(Js::Undefined, vec![Js::Number(-0.)])?;
                                    Ok(Js::Undefined)
                                }))
                            })),
                            None,
                        );
                        value
                    }
                };
                let seen = trace.clone();
                let body = Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        seen.borrow_mut().push("body".into());
                        if route == "await" {
                            await_js(value).await?;
                            seen.borrow_mut().push("resumed".into());
                            Ok(Js::Undefined)
                        } else {
                            Ok(value)
                        }
                    }),
                );
                let seen = trace.clone();
                let Js::Promise(done) = Js::Promise(body)
                    .call_method(
                        "then",
                        vec![Js::function("done", move |_, _| {
                            seen.borrow_mut().push("done".into());
                            Ok(Js::Undefined)
                        })],
                    )
                    .unwrap()
                else {
                    panic!("native promise")
                };
                let mut tail = constructor.call_method("resolve", vec![]).unwrap();
                for label in ["q1", "q2", "q3"] {
                    let seen = trace.clone();
                    tail = tail
                        .call_method(
                            "then",
                            vec![Js::function(label, move |_, _| {
                                seen.borrow_mut().push(label.into());
                                Ok(Js::Undefined)
                            })],
                        )
                        .unwrap();
                }
                trace.borrow_mut().push("returned".into());
                done.wait().await.unwrap();
                let Js::Promise(tail) = tail else {
                    panic!("native promise")
                };
                tail.wait().await.unwrap();
                rows.push(json!({"route":route,"kind":kind,"trace":*trace.borrow()}));
            }
        }
        assert_eq!(json!(rows), oracle()["turns"]);
    });
}
