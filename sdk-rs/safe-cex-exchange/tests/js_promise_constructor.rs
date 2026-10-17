//! Promise constructor on the production native host, without a test runtime.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        promise::{Promise, await_js},
        scope::{Scope, ScopeShared},
        value::{Js, Thrown, arg},
    },
    native::native_runtime,
};
use std::{cell::RefCell, future::Future, rc::Rc, sync::Arc};

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
fn oracle() -> &'static serde_json::Value {
    static ENGINE: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    ENGINE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_promise_constructor_oracle.cjs"
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
fn construct(scope: &Scope<()>, executor: Js) -> Js {
    scope
        .global("Promise")
        .unwrap()
        .call(Js::Undefined, vec![executor])
        .expect("production Promise constructor")
}
#[test]
fn constructor_runs_executor_synchronously_and_first_settlement_wins() {
    run(|scope| async move {
        let calls = Rc::new(RefCell::new(vec![]));
        let seen = calls.clone();
        let thrown = Js::object(vec![("marker", Js::str("late throw"))]);
        let promise = construct(
            &scope,
            Js::function("executor", move |this, args| {
                assert!(this.is_undefined());
                assert_eq!(args.len(), 2);
                seen.borrow_mut().push("executor");
                arg(&args, 0).call(Js::Undefined, vec![Js::Number(7.)])?;
                arg(&args, 1).call(Js::Undefined, vec![Js::Number(8.)])?;
                Err(Thrown::new(thrown.clone()))
            }),
        );
        calls.borrow_mut().push("returned");
        assert_eq!(
            serde_json::to_value(&*calls.borrow()).unwrap(),
            oracle()["firstTrace"]
        );
        assert_eq!(
            await_js(promise).await.unwrap().as_f64(),
            oracle()["firstValue"].as_f64()
        );
        let error = Js::object(vec![("marker", Js::str("executor throw"))]);
        let original = error.clone();
        let promise = construct(
            &scope,
            Js::function("throws", move |_, _| Err(Thrown::new(original.clone()))),
        );
        assert!(
            await_js(promise)
                .await
                .unwrap_err()
                .value
                .same_reference(&error)
        );
        for executor in [Js::Undefined, Js::Null, Js::Number(1.)] {
            assert_eq!(
                scope
                    .global("Promise")
                    .unwrap()
                    .call(Js::Undefined, vec![executor])
                    .unwrap_err()
                    .value
                    .get("name")
                    .unwrap()
                    .to_display(),
                "TypeError"
            );
        }
    });
}
#[test]
fn constructor_resolvers_survive_return_and_lock_before_pending_adoption() {
    run(|scope| async move {
        let captured = Rc::new(RefCell::new(vec![]));
        let sink = captured.clone();
        let promise = construct(
            &scope,
            Js::function("capture", move |_, args| {
                *sink.borrow_mut() = args;
                Ok(Js::Undefined)
            }),
        );
        let Js::Promise(p) = &promise else {
            panic!("native promise")
        };
        assert!(p.result().is_none());
        let runtime = scope.exchange_runtime();
        let inner = Promise::pending(&runtime);
        captured.borrow()[0]
            .call(Js::Undefined, vec![Js::Promise(inner.clone())])
            .unwrap();
        captured.borrow()[1]
            .call(Js::Undefined, vec![Js::str("ignored")])
            .unwrap();
        captured.borrow()[0]
            .call(Js::Undefined, vec![Js::Number(99.)])
            .unwrap();
        assert!(
            p.result().is_none(),
            "resolve to pending promise locks out later reject/resolve"
        );
        let value = Js::object(vec![("marker", Js::Number(42.))]);
        inner.settle(Ok(value.clone()));
        assert!(await_js(promise).await.unwrap().same_reference(&value));
    });
}
#[test]
fn constructor_resolves_thenables_after_return_with_live_getter_and_first_call_rules() {
    run(|scope| async move {
        let trace = Rc::new(RefCell::new(vec![]));
        let thenable = Js::object(vec![]);
        let expected_this = thenable.clone();
        let calls = trace.clone();
        thenable.define_accessor(
            "then",
            Some(Rc::new(move |_| {
                calls.borrow_mut().push("get then");
                let calls = calls.clone();
                let expected_this = expected_this.clone();
                Ok(Js::function("then", move |this, args| {
                    assert!(this.same_reference(&expected_this));
                    calls.borrow_mut().push("call then");
                    arg(&args, 0).call(Js::Undefined, vec![Js::Number(11.)])?;
                    arg(&args, 1).call(Js::Undefined, vec![Js::str("ignored")])?;
                    Err(Thrown::new(Js::str("also ignored")))
                }))
            })),
            None,
        );
        let calls = trace.clone();
        let promise = construct(
            &scope,
            Js::function("executor", move |_, args| {
                calls.borrow_mut().push("executor");
                arg(&args, 0).call(Js::Undefined, vec![thenable.clone()])?;
                calls.borrow_mut().push("after resolve");
                Ok(Js::Undefined)
            }),
        );
        trace.borrow_mut().push("returned");
        assert_eq!(
            serde_json::to_value(&*trace.borrow()).unwrap(),
            oracle()["beforeJob"]
        );
        assert_eq!(
            await_js(promise).await.unwrap().as_f64(),
            oracle()["adoptedValue"].as_f64()
        );
        assert_eq!(
            serde_json::to_value(&*trace.borrow()).unwrap(),
            oracle()["thenableTrace"]
        );
    });
}
#[test]
fn constructor_rejects_self_resolution_and_preserves_then_getter_error() {
    run(|scope| async move {
        let resolver = Rc::new(RefCell::new(Js::Undefined));
        let sink = resolver.clone();
        let promise = construct(
            &scope,
            Js::function("executor", move |_, args| {
                *sink.borrow_mut() = arg(&args, 0);
                Ok(Js::Undefined)
            }),
        );
        resolver
            .borrow()
            .call(Js::Undefined, vec![promise.clone()])
            .unwrap();
        assert_eq!(
            await_js(promise)
                .await
                .unwrap_err()
                .value
                .get("name")
                .unwrap()
                .to_display(),
            "TypeError"
        );
        let error = Js::object(vec![("marker", Js::str("get then throws"))]);
        let thrown = error.clone();
        let thenable = Js::object(vec![]);
        thenable.define_accessor(
            "then",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        );
        let promise = construct(
            &scope,
            Js::function("executor", move |_, args| {
                arg(&args, 0).call(Js::Undefined, vec![thenable.clone()])
            }),
        );
        assert!(
            await_js(promise)
                .await
                .unwrap_err()
                .value
                .same_reference(&error)
        );
    });
}

#[test]
fn dropping_the_constructor_observer_does_not_cancel_its_thenable_job() {
    run(|scope| async move {
        let calls = Rc::new(std::cell::Cell::new(0));
        let seen = calls.clone();
        let thenable = Js::object(vec![(
            "then",
            Js::function("then", move |_, args| {
                seen.set(seen.get() + 1);
                arg(&args, 0).call(Js::Undefined, vec![Js::Number(1.)])
            }),
        )]);
        let promise = construct(
            &scope,
            Js::function("executor", move |_, args| {
                arg(&args, 0).call(Js::Undefined, vec![thenable.clone()])
            }),
        );
        drop(promise);
        tokio::task::yield_now().await;
        assert_eq!(
            calls.get(),
            1,
            "native runtime retains the promise job after observer drop"
        );
    });
}
