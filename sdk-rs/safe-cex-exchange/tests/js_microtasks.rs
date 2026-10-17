#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        promise::{Promise, await_js},
        scope::ScopeShared,
        value::Js,
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc, time::Duration};

#[test]
fn native_promise_jobs_precede_timers_and_defer_settled_continuations() {
    let output = Command::new("node")
        .args([
            "-e",
            r#"
const trace=['sync'];
Promise.resolve().then(()=>trace.push('then'));
trace.push('after-then');
(async()=>{trace.push('async-start');await Promise.resolve();trace.push('await')})();
trace.push('after-async');
setTimeout(()=>{trace.push('timer');console.log(JSON.stringify(trace))},0);
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let host = client::rest_http_host(
            native.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            native.clone(),
            host,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let trace = Rc::new(RefCell::new(vec!["sync".to_owned()]));
        let promise = shared
            .globals
            .get("Promise")
            .unwrap()
            .call_method("resolve", vec![])
            .unwrap();
        let then = trace.clone();
        promise
            .call_method(
                "then",
                vec![Js::function("then", move |_, _| {
                    then.borrow_mut().push("then".into());
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        trace.borrow_mut().push("after-then".into());
        let continuation = trace.clone();
        let runtime: Rc<dyn ExchangeRuntime> = native.clone();
        let observer = Promise::spawn(
            &runtime,
            Box::pin(async move {
                continuation.borrow_mut().push("async-start".into());
                await_js(promise).await?;
                continuation.borrow_mut().push("await".into());
                Ok(Js::Undefined)
            }),
        );
        trace.borrow_mut().push("after-async".into());
        let timer = trace.clone();
        let fired = Rc::new(tokio::sync::Notify::new());
        let ready = fired.clone();
        shared
            .globals
            .get("setTimeout")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    Js::function("timer", move |_, _| {
                        timer.borrow_mut().push("timer".into());
                        ready.notify_one();
                        Ok(Js::Undefined)
                    }),
                    Js::Number(0.0),
                ],
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), fired.notified())
            .await
            .unwrap();
        assert_eq!(*trace.borrow(), expected);
        await_js(Js::Promise(observer)).await.unwrap();
    });
}

#[test]
fn native_timer_settlement_drains_nested_promise_jobs_before_the_next_timer() {
    let output = Command::new("node")
        .args([
            "-e",
            r#"
const trace=[];let resolve;const p=new Promise(r=>resolve=r);
p.then(()=>{trace.push('reaction');Promise.resolve().then(()=>trace.push('nested'))});
setTimeout(()=>{trace.push('settle');resolve()},0);
setTimeout(()=>{trace.push('next-timer');console.log(JSON.stringify(trace))},0);
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let host = client::rest_http_host(
            native.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            native.clone(),
            host,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let runtime: Rc<dyn ExchangeRuntime> = native.clone();
        let promise = Promise::pending(&runtime);
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let reaction = trace.clone();
        let nested = Js::Promise(Promise::settled(&runtime, Ok(Js::Undefined)));
        Js::Promise(promise.clone())
            .call_method(
                "then",
                vec![Js::function("reaction", move |_, _| {
                    reaction.borrow_mut().push("reaction".into());
                    let reaction = reaction.clone();
                    nested.call_method(
                        "then",
                        vec![Js::function("nested", move |_, _| {
                            reaction.borrow_mut().push("nested".into());
                            Ok(Js::Undefined)
                        })],
                    )?;
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        let settle = trace.clone();
        shared
            .globals
            .get("setTimeout")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    Js::function("settle", move |_, _| {
                        settle.borrow_mut().push("settle".into());
                        promise.settle(Ok(Js::Undefined));
                        Ok(Js::Undefined)
                    }),
                    Js::Number(0.0),
                ],
            )
            .unwrap();
        let next = trace.clone();
        let fired = Rc::new(tokio::sync::Notify::new());
        let ready = fired.clone();
        shared
            .globals
            .get("setTimeout")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    Js::function("next", move |_, _| {
                        next.borrow_mut().push("next-timer".into());
                        ready.notify_one();
                        Ok(Js::Undefined)
                    }),
                    Js::Number(0.0),
                ],
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), fired.notified())
            .await
            .unwrap();
        assert_eq!(*trace.borrow(), expected);
    });
}
