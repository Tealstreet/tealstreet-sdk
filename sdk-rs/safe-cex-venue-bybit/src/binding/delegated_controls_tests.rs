//! Real position-control Promise and dropped-waiter witnesses.
#[cfg(test)]
mod fixtures {
    use super::super::{
        BybitDependencies,
        promise::{Promise, await_js},
        value::{Js, arg},
    };
    use crate::exchange::BybitExchange;
    use safe_cex_exchange::{
        exchange::Exchange,
        native::{NativeHttpHost, native_runtime},
        native_store::DefaultStore,
    };
    use safe_cex_store::virtual_clock::VirtualClock;
    use safe_cex_transport::{
        RestAdapter, RestFuture, RestRequest, RestResponse, TransportFuture,
        rate_limit::RateLimitClock,
    };
    use safe_cex_types::ExchangeName;
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    struct NoWire;
    impl RestAdapter for NoWire {
        fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
            panic!("position control witness must not perform HTTP")
        }
    }
    struct Clock;
    impl RateLimitClock for Clock {
        fn now_ms(&self) -> u64 {
            0
        }
        fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }
    fn venue(name: ExchangeName) -> Rc<BybitExchange> {
        BybitExchange::create(
            name,
            serde_json::from_value(
                serde_json::json!({"key":"", "secret":"", "publicDataOnly":true}),
            )
            .unwrap(),
            Rc::new(DefaultStore::new()),
            BybitDependencies {
                runtime: native_runtime(),
                remote_signer: None,
                wire: Arc::new(NoWire),
                http_host: Rc::new(NativeHttpHost::default()),
                retry_clock: Rc::new(Clock),
                clock: VirtualClock::default(),
                external_public_data: None,
                relay_enabled: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn delegated_controls_preserve_eager_prefix_and_dropped_waiter_continuations() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/delegated-controls-fixtures.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let profiles: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(profiles.len(), 8);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(tokio::task::LocalSet::new().run_until(check_profiles(profiles)));
    }

    async fn check_profiles(profiles: Vec<serde_json::Value>) {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            for case in &profiles {
                let exchange = venue(name.clone());
                let rt = exchange.js.get().unwrap();
                let this = &rt.this;
                let runtime = exchange.base.runtime.clone();
                let leverage = case["kind"] == "leverage";
                let selected = case["selected"].as_bool().unwrap();
                let first = Js::array(vec![Js::object(vec![
                    ("symbol", Js::str("BTCUSDT")),
                    ("leverage", Js::Number(2.)),
                ])]);
                let refreshed = Js::array(vec![Js::object(vec![
                    ("symbol", Js::str("BTCUSDT")),
                    ("leverage", Js::Number(-0.)),
                    ("opaque", Js::utf16(&[0xd800])),
                ])]);
                let limits = Js::object(vec![("min", Js::Number(1.)), ("max", Js::Number(5.))]);
                let store = Js::object(vec![]);
                this.set("store", store.clone()).unwrap();
                store
                    .set(
                        "marketsBySymbol",
                        Js::object(vec![(
                            "BTCUSDT",
                            Js::object(vec![(
                                "limits",
                                Js::object(vec![("leverage", limits.clone())]),
                            )]),
                        )]),
                    )
                    .unwrap();
                store
                    .set(
                        "positions",
                        if leverage && selected {
                            first.clone()
                        } else {
                            Js::array(vec![])
                        },
                    )
                    .unwrap();
                let trace = Rc::new(RefCell::new(Vec::<String>::new()));
                for member in ["addOrUpdatePositions", "update"] {
                    let (trace, first, refreshed) =
                        (trace.clone(), first.clone(), refreshed.clone());
                    store
                    .set(
                        member,
                        Js::function(member, move |_, args| {
                            let value = arg(&args, 0);
                            let value = if member == "update" {
                                value.get("positions")?
                            } else {
                                value
                            };
                            assert!(
                                value.same_reference(&first) || value.same_reference(&refreshed),
                                "original fetched allocation survives await and Store publication"
                            );
                            trace.borrow_mut().push(
                                if member == "update" {
                                    "update"
                                } else if value.same_reference(&first) {
                                    "upsert-initial"
                                } else {
                                    "upsert-refresh"
                                }
                                .into(),
                            );
                            Ok(Js::Undefined)
                        }),
                    )
                    .unwrap();
                }
                let initial = Promise::pending(&runtime);
                let post = Promise::pending(&runtime);
                let all = Promise::pending(&runtime);
                let symbol = Promise::pending(&runtime);
                let (initial_result, all_result, symbol_result) =
                    (initial.clone(), all.clone(), symbol.clone());
                let seen = trace.clone();
                let first_call = Rc::new(std::cell::Cell::new(true));
                this.set(
                    "fetchPositions",
                    Js::function("held positions", move |_, args| {
                        if leverage && !selected && first_call.replace(false) {
                            seen.borrow_mut().push("fetch-initial".into());
                            return Ok(Js::Promise(initial_result.clone()));
                        }
                        let has_symbol = !arg(&args, 0).is_undefined();
                        seen.borrow_mut().push(
                            if has_symbol {
                                "fetch-symbol"
                            } else {
                                "fetch-all"
                            }
                            .into(),
                        );
                        Ok(Js::Promise(if !leverage && has_symbol {
                            symbol_result.clone()
                        } else {
                            all_result.clone()
                        }))
                    }),
                )
                .unwrap();
                let seen = trace.clone();
                let post_result = post.clone();
                this.get("xhr")
                    .unwrap()
                    .set(
                        "post",
                        Js::function("held post", move |_, args| {
                            let payload = arg(&args, 1);
                            assert!(payload.get("category")?.strict_equals(&Js::str("linear")));
                            if leverage {
                                assert!(
                                    arg(&args, 0)
                                        .strict_equals(&Js::str("/v5/position/set-leverage"))
                                );
                                let expected = if selected { "5" } else { "4" };
                                assert!(
                                    payload
                                        .get("buyLeverage")?
                                        .strict_equals(&Js::str(expected))
                                );
                                assert!(
                                    payload
                                        .get("sellLeverage")?
                                        .strict_equals(&Js::str(expected))
                                );
                                assert!(payload.get("symbol")?.strict_equals(&Js::str("BTCUSDT")));
                                seen.borrow_mut().push(format!("post-{expected}"));
                            } else {
                                assert!(
                                    arg(&args, 0)
                                        .strict_equals(&Js::str("/v5/position/switch-mode"))
                                );
                                assert!(payload.get("mode")?.strict_equals(&Js::Number(3.)));
                                assert!(payload.get("coin")?.strict_equals(&Js::str("USDT")));
                                assert!(payload.get("symbol")?.strict_equals(&if selected {
                                    Js::str("BTCUSDT")
                                } else {
                                    Js::Undefined
                                }));
                                seen.borrow_mut().push("post-mode".into());
                            }
                            Ok(Js::Promise(post_result.clone()))
                        }),
                    )
                    .unwrap();
                let seen = trace.clone();
                this.set(
                    "emitAndLogError",
                    Js::function("observe error", move |_, args| {
                        seen.borrow_mut()
                            .push(format!("emit-{}", arg(&args, 0).to_display()));
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
                let raw = if leverage {
                    let waiter =
                        Exchange::set_leverage(exchange.as_ref(), "BTCUSDT".into(), 7., None);
                    drop(waiter);
                    None
                } else {
                    let value = this
                        .call_method(
                            "changePositionModeHelper",
                            vec![
                                Js::Bool(true),
                                Js::str("linear"),
                                Js::str("USDT"),
                                if selected {
                                    Js::str("BTCUSDT")
                                } else {
                                    Js::Undefined
                                },
                            ],
                        )
                        .unwrap();
                    drop(await_js(&runtime, value.clone()));
                    Some(value)
                };
                let check = |stage: &str| {
                    assert_eq!(
                        serde_json::json!(*trace.borrow()),
                        case["checkpoints"][stage],
                        "source/native {stage}: {name:?}/{case}"
                    )
                };
                check("prefix");
                if leverage && !selected {
                    limits.set("max", Js::Number(4.)).unwrap();
                    initial.settle(Ok(first));
                }
                flush().await;
                check("afterInitial");
                post.settle(Ok(Js::object(vec![(
                    "data",
                    Js::object(vec![("retMsg", Js::str(case["reply"].as_str().unwrap()))]),
                )])));
                flush().await;
                check("afterPost");
                all.settle(Ok(refreshed.clone()));
                flush().await;
                check("afterAll");
                symbol.settle(Ok(refreshed.clone()));
                flush().await;
                check("settled");
                assert!(
                    refreshed
                        .get("0")
                        .unwrap()
                        .get("leverage")
                        .unwrap()
                        .strict_equals(&Js::Number(-0.))
                );
                if let Js::Number(zero) = refreshed.get("0").unwrap().get("leverage").unwrap() {
                    assert!(zero.is_sign_negative());
                } else {
                    panic!("retained signed zero");
                }
                assert!(
                    matches!(refreshed.get("0").unwrap().get("opaque").unwrap(), Js::String(s) if s.as_ref() == [0xd800])
                );
                if let Some(value) = raw {
                    let result = await_js(&runtime, value).await;
                    if case["reply"] == "refused" {
                        assert_eq!(
                            result
                                .unwrap_err()
                                .value
                                .get("message")
                                .unwrap()
                                .to_display(),
                            "refused"
                        );
                    } else {
                        result.unwrap();
                    }
                }
                exchange.dispose().unwrap();
            }
        }
    }

    async fn flush() {
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
    }
}
