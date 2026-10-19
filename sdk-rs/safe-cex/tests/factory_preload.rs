#![cfg(not(target_arch = "wasm32"))]
use safe_cex::factory::{ExchangeConstructor, ExchangeFactory, ExchangeLoader};
use safe_cex_exchange::{native::native_runtime, runtime::ExchangeRuntime};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::TransportRegistry;
use std::{cell::RefCell, future::Future, rc::Rc};

fn run<F: Future<Output = ()>>(body: impl FnOnce(Rc<dyn ExchangeRuntime>) -> F) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async { body(native_runtime()).await });
}

async fn drain() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

fn constructor() -> ExchangeConstructor<()> {
    Rc::new(|_, _, _| Ok(()))
}

#[test]
fn preload_starts_every_loader_before_the_waiter_is_polled() {
    run(|runtime| async move {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let loaders = ["first", "second", "third"]
            .into_iter()
            .map(|name| {
                let trace = trace.clone();
                let loader: ExchangeLoader<()> = Rc::new(move || {
                    trace.borrow_mut().push(name);
                    Box::pin(async { Ok(constructor()) })
                });
                (name.into(), loader)
            })
            .collect();
        let factory = ExchangeFactory::new(
            loaders,
            VirtualClock::default(),
            TransportRegistry::default(),
            runtime,
        );
        let names = ["first".into(), "second".into(), "third".into()];
        let waiter = factory.preload_exchanges(&names);
        assert_eq!(*trace.borrow(), ["first", "second", "third"]);
        assert!(names.iter().all(|name| !factory.is_exchange_loaded(name)));
        waiter.await.unwrap();
        assert!(names.iter().all(|name| factory.is_exchange_loaded(name)));
    });
}

#[test]
fn rejected_preload_keeps_other_loaders_alive_and_caches_them() {
    run(|runtime| async move {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let bad: ExchangeLoader<()> = Rc::new({
            let trace = trace.clone();
            move || {
                trace.borrow_mut().push("bad");
                Box::pin(async { Err("loader failed".into()) })
            }
        });
        let (release, held) = tokio::sync::oneshot::channel();
        let held = Rc::new(RefCell::new(Some(held)));
        let slow: ExchangeLoader<()> = Rc::new({
            let trace = trace.clone();
            move || {
                trace.borrow_mut().push("slow");
                let held = held.borrow_mut().take().expect("one loader call");
                Box::pin(async move {
                    held.await.expect("release retained loader");
                    Ok(constructor())
                })
            }
        });
        let ready: ExchangeLoader<()> = Rc::new({
            let trace = trace.clone();
            move || {
                trace.borrow_mut().push("ready");
                Box::pin(async { Ok(constructor()) })
            }
        });
        let factory = ExchangeFactory::new(
            [
                ("bad".into(), bad),
                ("slow".into(), slow),
                ("ready".into(), ready),
            ]
            .into_iter()
            .collect(),
            VirtualClock::default(),
            TransportRegistry::default(),
            runtime,
        );
        let names = ["bad".into(), "slow".into(), "ready".into()];
        assert_eq!(
            factory.preload_exchanges(&names).await.unwrap_err(),
            "loader failed"
        );
        assert_eq!(*trace.borrow(), ["bad", "slow", "ready"]);
        assert!(factory.is_exchange_loaded("ready"));
        assert!(!factory.is_exchange_loaded("slow"));
        release.send(()).unwrap();
        drain().await;
        assert!(factory.is_exchange_loaded("slow"));
        factory
            .preload_exchanges(&["slow".into(), "ready".into(), "slow".into()])
            .await
            .unwrap();
        assert_eq!(trace.borrow().len(), 3);
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/factory_preload_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(serde_json::json!(*trace.borrow()), source["prefix"]);
        assert_eq!(source["cachedLoaderCalls"], trace.borrow().len());
    });
}

#[test]
fn discarding_a_preload_waiter_keeps_the_loader_and_factory_cache_alive() {
    run(|runtime| async move {
        let (release, held) = tokio::sync::oneshot::channel();
        let held = Rc::new(RefCell::new(Some(held)));
        let loader: ExchangeLoader<()> = Rc::new(move || {
            let held = held.borrow_mut().take().expect("one loader call");
            Box::pin(async move {
                held.await.expect("loader must outlive the waiter");
                Ok(constructor())
            })
        });
        let factory = ExchangeFactory::new(
            [("held".into(), loader)].into_iter().collect(),
            VirtualClock::default(),
            TransportRegistry::default(),
            runtime,
        );
        drop(factory.preload_exchanges(&["held".into()]));
        release.send(()).unwrap();
        drain().await;
        assert!(factory.is_exchange_loaded("held"));
        factory.preload_exchanges(&[]).await.unwrap();
    });
}

#[test]
fn preload_reports_the_first_settled_rejection_instead_of_input_order() {
    run(|runtime| async move {
        let (first_send, first_recv) = tokio::sync::oneshot::channel::<()>();
        let (second_send, second_recv) = tokio::sync::oneshot::channel::<()>();
        let loaders = [("first", first_recv), ("second", second_recv)]
            .into_iter()
            .map(|(name, receive)| {
                let receive = RefCell::new(Some(receive));
                let loader: ExchangeLoader<()> = Rc::new(move || {
                    let receive = receive.borrow_mut().take().unwrap();
                    Box::pin(async move {
                        receive.await.unwrap();
                        Err(name.into())
                    })
                });
                (name.into(), loader)
            })
            .collect();
        let factory = ExchangeFactory::new(
            loaders,
            VirtualClock::default(),
            TransportRegistry::default(),
            runtime,
        );
        let pending = factory.preload_exchanges(&["first".into(), "second".into()]);
        second_send.send(()).unwrap();
        first_send.send(()).unwrap();
        assert_eq!(pending.await.unwrap_err(), "second");
    });
}
