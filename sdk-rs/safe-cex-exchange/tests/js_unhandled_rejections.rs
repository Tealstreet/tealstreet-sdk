#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        promise::Promise,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use std::{process::Command, rc::Rc, sync::Arc, time::Duration};

#[test]
fn native_unhandled_rejections_match_node_without_reporting_handled_promises() {
    if std::env::var_os("SAFE_CEX_REJECTION_CHILD").is_none() {
        let oracle = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_unhandled_rejections_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let expected: Vec<String> = serde_json::from_slice(&oracle.stdout).unwrap();
        let native = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_unhandled_rejections_match_node_without_reporting_handled_promises",
                "--nocapture",
            ])
            .env("SAFE_CEX_REJECTION_CHILD", "1")
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert!(!stderr.contains("panicked"), "{stderr}");
        assert!(
            native.status.success(),
            "{}\n{stderr}",
            String::from_utf8_lossy(&native.stdout)
        );
        let mut actual: Vec<String> = stderr
            .lines()
            .filter_map(|line| {
                line.strip_prefix("[safe-cex uncaught] Error: ")
                    .map(|line| line.split_whitespace().next().unwrap().to_owned())
            })
            .collect();
        actual.sort();
        assert_eq!(actual, expected, "{stderr}");
        return;
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let http = client::rest_http_host(
            native.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            native.clone(),
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let runtime: Rc<dyn ExchangeRuntime> = native.clone();
        let reject = |message: &str| {
            shared
                .globals
                .get("Promise")
                .unwrap()
                .call_method("reject", vec![Thrown::error("Error", message).value])
                .unwrap()
        };
        let catch = Js::function("catch", |_, _| Ok(Js::Undefined));
        let live = reject("unhandled-live");
        let parked = Promise::pending(&runtime);
        drop(Promise::spawn(
            &runtime,
            Box::pin(async move { parked.wait().await }),
        ));
        let reentrant = Rc::new(std::cell::RefCell::new(Js::Undefined));
        let weak_reentrant = Rc::downgrade(&reentrant);
        let handler = catch.clone();
        let reason = Thrown::error("Error", "initial").value;
        reason.define_accessor(
            "message",
            Some(Rc::new(move |_| {
                if let Some(owner) = weak_reentrant.upgrade() {
                    let promise = owner.borrow().clone();
                    promise.call_method("catch", vec![handler.clone()])?;
                }
                Ok(Js::str("unhandled-reentrant"))
            })),
            None,
        );
        *reentrant.borrow_mut() = shared
            .globals
            .get("Promise")
            .unwrap()
            .call_method("reject", vec![reason])
            .unwrap();
        let async_runtime = runtime.clone();
        drop(Promise::spawn(
            &runtime,
            Box::pin(async move {
                async_runtime.sleep(0).await.map_err(Thrown::from)?;
                Err(Thrown::error("Error", "unhandled-void"))
            }),
        ));
        let timer_runtime = runtime.clone();
        shared
            .globals
            .get("setTimeout")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    Js::function("async timer", move |_, _| {
                        let runtime = timer_runtime.clone();
                        Ok(Js::Promise(Promise::spawn(
                            &timer_runtime,
                            Box::pin(async move {
                                runtime.sleep(0).await.map_err(Thrown::from)?;
                                Err(Thrown::error("Error", "unhandled-timer"))
                            }),
                        )))
                    }),
                    Js::Number(0.0),
                ],
            )
            .unwrap();
        reject("handled-sync")
            .call_method("catch", vec![catch.clone()])
            .unwrap();
        let early = Promise::pending(&runtime);
        Js::Promise(early.clone())
            .call_method("catch", vec![catch.clone()])
            .unwrap();
        early.settle(Err(Thrown::error("Error", "handled-before-rejection")));
        let deferred = reject("handled-microtasks");
        let handler = catch.clone();
        let first = Promise::pending(&runtime);
        let mut prior = Js::Promise(first.clone());
        for _ in 0..8 {
            prior = prior.call_method("then", vec![]).unwrap();
        }
        prior
            .call_method(
                "then",
                vec![Js::function("attach catch", move |_, _| {
                    deferred.call_method("catch", vec![handler.clone()])?;
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        first.settle(Ok(Js::Undefined));
        reject("unhandled-derived")
            .call_method("then", vec![])
            .unwrap();
        shared
            .globals
            .get("Promise")
            .unwrap()
            .call_method(
                "all",
                vec![Js::array(vec![
                    reject("handled-all-first"),
                    reject("handled-all-later"),
                ])],
            )
            .unwrap()
            .call_method("catch", vec![catch.clone()])
            .unwrap();
        let pending = Promise::pending(&runtime);
        shared
            .globals
            .get("Promise")
            .unwrap()
            .call_method(
                "allSettled",
                vec![Js::array(vec![
                    Js::Promise(pending.clone()),
                    reject("handled-allSettled-later"),
                ])],
            )
            .unwrap();
        let late = reject("unhandled-handled-after-report");
        tokio::time::sleep(Duration::from_millis(5)).await;
        late.call_method("catch", vec![catch]).unwrap();
        pending.settle(Ok(Js::Undefined));
        tokio::time::sleep(Duration::from_millis(5)).await;
        reject("unhandled-idle-driver");
        tokio::time::sleep(Duration::from_millis(15)).await;
        assert!(matches!(live, Js::Promise(_)));
    });
}

#[test]
fn native_host_discard_releases_unreported_rejection_values() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    let (capture, runtime) = local.block_on(&rt, async {
        let native = native_runtime();
        let runtime = Rc::downgrade(&native);
        let native: Rc<dyn ExchangeRuntime> = native;
        let captured = Rc::new(());
        let capture = Rc::downgrade(&captured);
        let value = Js::object(vec![(
            "capture",
            Js::function("capture", move |_, _| {
                let _keep = &captured;
                Ok(Js::Undefined)
            }),
        )]);
        drop(Promise::settled(&native, Err(Thrown::new(value))));
        (capture, runtime)
    });
    assert!(
        capture.upgrade().is_some(),
        "checkpoint owns the discarded rejection until its reporting turn"
    );
    drop(local);
    assert!(
        capture.upgrade().is_none(),
        "host discard drops queued rejection captures"
    );
    assert!(
        runtime.upgrade().is_none(),
        "host discard breaks the rejection/queue ownership cycle"
    );
}
