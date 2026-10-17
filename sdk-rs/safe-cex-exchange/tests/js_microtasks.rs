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

#[test]
fn native_pending_reactions_use_settlement_order_and_primitive_returns_do_not_yield() {
    let output = Command::new("node").args(["-e", r#"
(async()=>{const rows=[];for(const kind of ['then','await']){
const trace=[];let resolve;const p=new Promise(r=>resolve=r);
const a=kind==='then'?p.then(()=>trace.push('pending')):(async()=>{await p;trace.push('pending')})();
const b=Promise.resolve().then(()=>trace.push('earlier'));resolve();await Promise.all([a,b]);rows.push(trace);
}const trace=[];const p=Promise.resolve();await Promise.all([p.then(()=>1).then(()=>trace.push('C')),p.then().then(()=>trace.push('D'))]);rows.push(trace);console.log(JSON.stringify(rows))})()
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let runtime: Rc<dyn ExchangeRuntime> = native;
        let mut rows = Vec::new();
        for asynchronous in [false, true] {
            let trace = Rc::new(RefCell::new(Vec::<String>::new()));
            let promise = Promise::pending(&runtime);
            let pending = trace.clone();
            let a = if asynchronous {
                let promise = Js::Promise(promise.clone());
                Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        await_js(promise).await?;
                        pending.borrow_mut().push("pending".into());
                        Ok(Js::Undefined)
                    }),
                ))
            } else {
                Js::Promise(promise.clone())
                    .call_method(
                        "then",
                        vec![Js::function("pending", move |_, _| {
                            pending.borrow_mut().push("pending".into());
                            Ok(Js::Undefined)
                        })],
                    )
                    .unwrap()
            };
            let earlier = trace.clone();
            let b = Js::Promise(Promise::settled(&runtime, Ok(Js::Undefined)))
                .call_method(
                    "then",
                    vec![Js::function("earlier", move |_, _| {
                        earlier.borrow_mut().push("earlier".into());
                        Ok(Js::Undefined)
                    })],
                )
                .unwrap();
            promise.settle(Ok(Js::Undefined));
            await_js(a).await.unwrap();
            await_js(b).await.unwrap();
            rows.push(trace.borrow().clone());
        }
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let p = Js::Promise(Promise::settled(&runtime, Ok(Js::Undefined)));
        let c = trace.clone();
        let a = p
            .call_method(
                "then",
                vec![Js::function("one", |_, _| Ok(Js::Number(1.0)))],
            )
            .unwrap()
            .call_method(
                "then",
                vec![Js::function("C", move |_, _| {
                    c.borrow_mut().push("C".into());
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        let d = trace.clone();
        let b = p
            .call_method("then", vec![])
            .unwrap()
            .call_method(
                "then",
                vec![Js::function("D", move |_, _| {
                    d.borrow_mut().push("D".into());
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        await_js(a).await.unwrap();
        await_js(b).await.unwrap();
        rows.push(trace.borrow().clone());
        assert_eq!(rows, expected);
    });
}

#[test]
fn native_cleared_timer_releases_its_callback_and_idle_driver() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let weak_runtime = Rc::downgrade(&native);
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
        let probe = Rc::new(());
        let weak = Rc::downgrade(&probe);
        let handle = shared
            .globals
            .get("setTimeout")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    Js::function("cancelled", move |_, _| {
                        let _keep = &probe;
                        panic!("cleared timer ran")
                    }),
                    Js::Number(100_000.0),
                ],
            )
            .unwrap();
        native.sleep(0).await.unwrap();
        shared
            .globals
            .get("clearTimeout")
            .unwrap()
            .call(Js::Undefined, vec![handle])
            .unwrap();
        assert!(weak.upgrade().is_none(), "cancel drops the queued callback");
        drop(shared);
        drop(native);
        tokio::task::yield_now().await;
        assert!(
            weak_runtime.upgrade().is_none(),
            "cleared final timer must release its idle driver/runtime"
        );
    });
}
