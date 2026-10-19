use super::BybitDependencies;
use crate::{
    core_snapshot_readers::SnapshotHost, exchange::BybitExchange,
    hyro_private_session::SessionHost, variant_account_refresh::AccountRefreshHost,
};
use safe_cex_exchange::{exchange::Exchange, native_store::DefaultStore};
use safe_cex_exchange::{
    js_runtime::{
        promise::await_js,
        value::{Js, Thrown, arg},
    },
    native::{NativeHttpHost, native_runtime},
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

struct NoWire;
impl RestAdapter for NoWire {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("native receiver witness must not perform HTTP")
    }
}
struct Clock(Instant);
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.0.elapsed().as_millis() as u64
    }
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(milliseconds)).await;
            Ok(())
        })
    }
}
fn run(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(tokio::task::LocalSet::new().run_until(future));
}
fn venue(name: ExchangeName) -> Rc<BybitExchange> {
    venue_with_options(
        name,
        serde_json::from_value(serde_json::json!({"key":"", "secret":"", "publicDataOnly":true}))
            .unwrap(),
    )
}
fn venue_with_options(
    name: ExchangeName,
    options: safe_cex_types::ExchangeOptions,
) -> Rc<BybitExchange> {
    let runtime = native_runtime();
    BybitExchange::create(
        name,
        options,
        Rc::new(DefaultStore::new()),
        BybitDependencies {
            runtime: runtime.clone(),
            remote_signer: None,
            wire: Arc::new(NoWire),
            http_host: Rc::new(NativeHttpHost::default()),
            retry_clock: Rc::new(Clock(Instant::now())),
            clock: VirtualClock::default(),
            external_public_data: None,
            relay_enabled: false,
        },
    )
    .unwrap()
}
fn units(value: &Js) -> Vec<u16> {
    match value {
        Js::String(value) => value.to_vec(),
        _ => panic!("expected native text"),
    }
}

#[test]
fn divergence_audit_reinit_continuations_survive_dropped_waits() {
    run(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitinverse,
            ExchangeName::Bybitspot,
        ] {
            for initially in [false, true] {
                for rejects in [false, true] {
                    let exchange = venue(name.clone());
                    exchange.needs_reinit.set(initially);
                    let runtime = exchange.base.runtime.clone();
                    let pending =
                        safe_cex_exchange::js_runtime::promise::Promise::pending(&runtime);
                    let result = exchange
                        .js
                        .get()
                        .unwrap()
                        .this
                        .call_method("reinitOnThrow", vec![Js::Promise(pending.clone())])
                        .unwrap();
                    let Js::Promise(result) = result else {
                        panic!("async receiver returns a Promise")
                    };
                    drop(result.wait());
                    drop(result);
                    assert_eq!(
                        exchange.needs_reinit.get(),
                        initially,
                        "no premature rejection effect"
                    );
                    if rejects {
                        pending.settle(Err(Thrown::new(Js::object(vec![(
                            "marker",
                            Js::Number(73.0),
                        )]))));
                    } else {
                        pending.settle(Ok(Js::object(vec![("marker", Js::Number(73.0))])));
                    }
                    tokio::task::yield_now().await;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    assert_eq!(
                        exchange.needs_reinit.get(),
                        initially || rejects,
                        "live {name:?} receiver keeps the source continuation after the caller drops its wait"
                    );
                    exchange.base.dispose().unwrap();
                }
            }
        }
    });
}

#[test]
fn divergence_audit_balance_reads_are_eager_and_keep_native_results() {
    run(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitinverse,
            ExchangeName::Bybitspot,
        ] {
            for disposed in [false, true] {
                let exchange = venue(name.clone());
                let runtime = exchange.base.runtime.clone();
                let this = &exchange.js.get().unwrap().this;
                let pending = safe_cex_exchange::js_runtime::promise::Promise::pending(&runtime);
                let called = Rc::new(Cell::new(false));
                let observed = called.clone();
                let request = pending.clone();
                this.define(
                    "fetchRawBalance",
                    Js::function("fetchRawBalance", move |_, _| {
                        observed.set(true);
                        Ok(Js::Promise(request.clone()))
                    }),
                )
                .unwrap();
                let raw = Js::object(vec![("raw", Js::Number(-0.0))]);
                let formatted = Js::object(vec![("total", Js::Number(91.0))]);
                let raw_reference = raw.clone();
                let result_reference = formatted.clone();
                let formatted_calls = Rc::new(Cell::new(0));
                let count = formatted_calls.clone();
                this.define(
                    "formatBalance",
                    Js::function("formatBalance", move |_, args| {
                        assert!(
                            arg(&args, 0).strict_equals(&raw_reference),
                            "raw receiver identity"
                        );
                        count.set(count.get() + 1);
                        Ok(result_reference.clone())
                    }),
                )
                .unwrap();
                let result = this.call_method("fetchBalance", vec![]).unwrap();
                assert!(
                    called.get(),
                    "raw request is invoked before returning the native Promise"
                );
                assert_eq!(
                    formatted_calls.get(),
                    0,
                    "formatter must wait for the raw request"
                );
                if disposed {
                    exchange.base.dispose().unwrap();
                }
                pending.settle(Ok(raw));
                let resolved =
                    tokio::time::timeout(Duration::from_secs(1), await_js(&runtime, result))
                        .await
                        .expect("live balance continuation settles after its raw request")
                        .unwrap();
                if disposed {
                    assert_eq!(
                        formatted_calls.get(),
                        0,
                        "disposed source branch skips formatting"
                    );
                    assert_eq!(resolved.get("total").unwrap().to_number().unwrap(), 0.0);
                } else {
                    assert_eq!(formatted_calls.get(), 1);
                    assert!(
                        resolved.strict_equals(&formatted),
                        "original formatted value is forwarded"
                    );
                    exchange.base.dispose().unwrap();
                }
            }
        }
    });
}

#[test]
fn native_receiver_properties_literals_and_spreads() {
    run(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            let venue = venue(name);
            let rt = venue.js.get().unwrap();
            let child = Js::object(vec![
                ("opaque", Js::utf16(&[0xd800])),
                ("nan", Js::Number(f64::NAN)),
            ]);
            let raw = Js::object(vec![("child", child.clone()), ("missing", Js::Undefined)]);
            assert!(
                SessionHost::get(rt, &raw, "child")
                    .unwrap()
                    .same_reference(&child)
            );
            assert!(
                SnapshotHost::get(rt, &raw, "child")
                    .unwrap()
                    .same_reference(&child)
            );
            assert!(
                SessionHost::optional(rt, &Js::Null, "x")
                    .unwrap()
                    .is_undefined()
            );
            assert!(
                SnapshotHost::optional(rt, &Js::Undefined, "x")
                    .unwrap()
                    .is_undefined()
            );
            assert!(
                SessionHost::optional(rt, &raw, "child")
                    .unwrap()
                    .same_reference(&child)
            );
            assert!(
                SnapshotHost::optional(rt, &raw, "child")
                    .unwrap()
                    .same_reference(&child)
            );
            assert!(SessionHost::get(rt, &Js::Null, "x").is_err());
            assert!(SnapshotHost::get(rt, &Js::Undefined, "x").is_err());
            SessionHost::set(rt, &raw, "zero", Js::Number(-0.0)).unwrap();
            SnapshotHost::set(rt, &raw, "other", child.clone()).unwrap();
            SessionHost::define(rt, &raw, "ownUndefined", Js::Undefined).unwrap();
            assert!(raw.has_own("ownUndefined"));
            assert_eq!(
                raw.get("zero").unwrap().to_number().unwrap().to_bits(),
                (-0.0f64).to_bits()
            );
            assert!(raw.get("other").unwrap().same_reference(&child));
            let marker = Js::object(vec![("reason", child.clone())]);
            let thrown = marker.clone();
            raw.define_accessor(
                "throw",
                Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
                None,
            )
            .unwrap();
            assert!(
                SessionHost::get(rt, &raw, "throw")
                    .unwrap_err()
                    .value
                    .same_reference(&marker)
            );
            assert!(
                SnapshotHost::optional(rt, &raw, "throw")
                    .unwrap_err()
                    .value
                    .same_reference(&marker)
            );
            for text in ["literal {not JSON}", "null\0"] {
                assert_eq!(
                    units(&SessionHost::literal(rt, text)),
                    text.encode_utf16().collect::<Vec<_>>()
                );
                assert_eq!(
                    units(&SnapshotHost::literal(rt, text)),
                    text.encode_utf16().collect::<Vec<_>>()
                );
            }
            let objects = [
                SessionHost::object(
                    rt,
                    vec![("child", child.clone()), ("missing", Js::Undefined)],
                ),
                SnapshotHost::object(
                    rt,
                    vec![("child", child.clone()), ("missing", Js::Undefined)],
                ),
            ];
            for object in objects {
                assert!(object.get("child").unwrap().same_reference(&child));
                assert!(object.has_own("missing"));
                assert_eq!(
                    units(&object.get("child").unwrap().get("opaque").unwrap()),
                    [0xd800]
                );
                assert!(
                    object
                        .get("child")
                        .unwrap()
                        .get("nan")
                        .unwrap()
                        .to_number()
                        .unwrap()
                        .is_nan()
                );
            }
            let base = Js::object(vec![("child", child.clone()), ("flag", Js::Bool(false))]);
            for result in [
                SnapshotHost::spread(rt, base.clone(), vec![("flag", Js::Bool(true))]).unwrap(),
                AccountRefreshHost::spread(rt, base.clone(), vec![("flag", Js::Bool(true))])
                    .unwrap(),
            ] {
                assert!(!result.same_reference(&base));
                assert!(result.get("child").unwrap().same_reference(&child));
                assert!(result.get("flag").unwrap().truthy());
                assert!(!base.get("flag").unwrap().truthy());
            }
            assert!(
                SnapshotHost::spread(rt, Js::Null, vec![("new", Js::Undefined)])
                    .unwrap()
                    .has_own("new")
            );
            assert!(
                AccountRefreshHost::spread(rt, Js::Undefined, vec![("new", child.clone())])
                    .unwrap()
                    .get("new")
                    .unwrap()
                    .same_reference(&child)
            );
        }
    });
}

#[test]
fn native_receiver_selected_members_preserve_owner_and_capture() {
    run(async {
        let venue = venue(ExchangeName::Bybit);
        let rt = venue.js.get().unwrap();
        let child = Js::object(vec![("opaque", Js::utf16(&[0xdfff]))]);
        let method = Js::function("method", |this, args| {
            Ok(Js::array(vec![this, arg(&args, 0)]))
        });
        let owner = Js::object(vec![("method", method.clone())]);
        let selected = [
            SessionHost::select(rt, owner.clone(), "method").unwrap(),
            SnapshotHost::select(rt, owner.clone(), "method").unwrap(),
        ];
        owner.set("method", Js::Undefined).unwrap();
        for selected in selected {
            let result = selected.call(Js::Null, vec![child.clone()]).unwrap();
            assert!(result.get("0").unwrap().same_reference(&owner));
            assert!(result.get("1").unwrap().same_reference(&child));
        }
        let bind = SnapshotHost::select(rt, method, "bind").unwrap();
        let bound = bind
            .call(Js::Undefined, vec![owner.clone(), child.clone()])
            .unwrap();
        let result = bound.call(Js::Null, vec![]).unwrap();
        assert!(result.get("0").unwrap().same_reference(&owner));
        assert!(result.get("1").unwrap().same_reference(&child));
        let array = Js::array(vec![child.clone()]);
        let map = SessionHost::select(rt, array.clone(), "map").unwrap();
        assert!(map.get("__selectedOwner").unwrap().same_reference(&array));
        assert_eq!(
            units(&map.get("__selectedName").unwrap()),
            units(&Js::str("map"))
        );
        let result = SessionHost::apply(
            rt,
            map,
            vec![Js::function("identity", |_, a| Ok(arg(&a, 0)))],
        )
        .unwrap();
        assert!(result.get("0").unwrap().same_reference(&child));
        let nested = Js::array(vec![
            Js::holey_array(vec![None, Some(child.clone())]),
            Js::array(vec![Js::array(vec![child.clone()])]),
        ]);
        let flat = SnapshotHost::select(rt, nested, "flat")
            .unwrap()
            .call(Js::Null, vec![])
            .unwrap();
        assert_eq!(
            flat.array_values()
                .expect("fixture array property reads")
                .unwrap()
                .len(),
            2
        );
        assert!(flat.get("0").unwrap().same_reference(&child));
        assert!(flat.get("1").unwrap().is_array());
        assert!(SessionHost::select(rt, Js::Null, "method").is_err());
        assert!(SnapshotHost::select(rt, Js::Undefined, "method").is_err());
    });
}

#[test]
fn native_receiver_intrinsics_and_callback_effects() {
    run(async {
        let venue = venue(ExchangeName::Bybit);
        let rt = venue.js.get().unwrap();
        let child = Js::object(vec![("opaque", Js::utf16(&[0xd800]))]);
        let raw = Js::object(vec![("missing", Js::Undefined)]);
        SessionHost::intrinsic(
            rt,
            "computed assignment",
            vec![raw.clone(), Js::utf16(&[0xdfff]), child.clone()],
        )
        .unwrap();
        let result =
            SessionHost::intrinsic(rt, "keyGet", vec![raw.clone(), Js::utf16(&[0xdfff])]).unwrap();
        assert!(result.same_reference(&child));
        assert_eq!(
            units(&SessionHost::intrinsic(rt, "template", vec![Js::utf16(&[0xd800])]).unwrap()),
            [0xd800]
        );
        assert!(
            SessionHost::intrinsic(rt, "throw value", vec![child.clone()])
                .unwrap_err()
                .value
                .same_reference(&child)
        );
        assert_eq!(
            SnapshotHost::intrinsic(
                rt,
                "template",
                vec![Js::utf16(&[0xd800]), Js::utf16(&[0xdfff])]
            )
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
            [0xd800, 0xdfff]
        );
        assert!(
            SnapshotHost::intrinsic(rt, "get-key", vec![raw.clone(), Js::utf16(&[0xdfff])])
                .unwrap()
                .same_reference(&child)
        );
        let minimum =
            SnapshotHost::intrinsic(rt, "Math.min", vec![Js::Number(0.), Js::Number(-0.)]).unwrap();
        assert_eq!(minimum.to_number().unwrap().to_bits(), (-0.0f64).to_bits());
        let map = SessionHost::intrinsic(rt, "new Map", vec![]).unwrap();
        map.call_method("set", vec![Js::str("opaque"), child.clone()])
            .unwrap();
        assert!(
            map.call_method("get", vec![Js::str("opaque")])
                .unwrap()
                .same_reference(&child)
        );
        let resolved = SessionHost::intrinsic(rt, "Promise.resolve", vec![child.clone()]).unwrap();
        assert!(
            await_js(&rt.exchange_runtime(), resolved)
                .await
                .unwrap()
                .same_reference(&child)
        );
        let rows = Js::array(vec![]);
        let callback = SnapshotHost::callback(rt, rt.this.clone(), "orders_then", vec![]).unwrap();
        assert!(
            callback
                .call(Js::Null, vec![rows.clone()])
                .unwrap()
                .same_reference(&rows)
        );
        assert!(
            rt.this
                .get("store")
                .unwrap()
                .get("loaded")
                .unwrap()
                .get("orders")
                .unwrap()
                .truthy()
        );
        let generation = Js::Number(7.);
        let socket = Js::object(vec![]);
        let session = Js::object(vec![
            ("isDisposed", Js::Bool(false)),
            ("generation", generation.clone()),
            ("ws", socket.clone()),
        ]);
        let current =
            SessionHost::callback(rt, session.clone(), "current", vec![generation, socket])
                .unwrap();
        assert!(current.call(Js::Null, vec![]).unwrap().truthy());
        session.set("generation", Js::Number(8.)).unwrap();
        assert!(!current.call(Js::Null, vec![]).unwrap().truthy());
        session.set("generation", Js::Number(7.)).unwrap();
        assert!(current.call(Js::Null, vec![]).unwrap().truthy());
        drop(session);
        assert!(!current.call(Js::Null, vec![]).unwrap().truthy());
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let iterator = Js::object(vec![(
            "return",
            Js::function("return", move |_, _| {
                observed.set(observed.get() + 1);
                Ok(Js::object(vec![]))
            }),
        )]);
        SnapshotHost::intrinsic(rt, "iterator.close", vec![iterator]).unwrap();
        assert_eq!(calls.get(), 1);
        let effects = Rc::new(RefCell::new(vec![]));
        let seen = effects.clone();
        let owner = Js::object(vec![
            ("isDisposed", Js::Bool(false)),
            (
                "store",
                Js::object(vec![
                    ("loaded", Js::object(vec![])),
                    (
                        "update",
                        Js::function("update", move |this, args| {
                            seen.borrow_mut().push((this, args));
                            Ok(Js::Undefined)
                        }),
                    ),
                ]),
            ),
        ]);
        let callback = SessionHost::callback(rt, owner, "orders_then", vec![]).unwrap();
        assert!(
            callback
                .call(Js::Null, vec![rows.clone()])
                .unwrap()
                .same_reference(&rows)
        );
        assert!(
            effects.borrow()[0].1[0]
                .get("orders")
                .unwrap()
                .same_reference(&rows)
        );
    });
}

#[test]
fn native_protective_side_errors_match_actual_source() {
    use safe_cex_exchange::exchange::Exchange;

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/protective-side-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 242);
    run(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            let venue = venue(name);
            let rt = venue.js.get().unwrap();
            let mut compared = 0;
            for (index, case) in cases.iter().enumerate() {
                if case["error"].is_null() {
                    continue;
                }
                let thrown = rt
                    .this
                    .get("inferProtectiveOrderSide")
                    .unwrap()
                    .call(
                        rt.this.clone(),
                        vec![safe_cex_exchange::js_runtime::value::from_json(
                            &case["order"],
                        )],
                    )
                    .unwrap_err();
                assert_eq!(
                    safe_cex_exchange::js_runtime::value::to_json(
                        &thrown.value.get("name").unwrap()
                    )
                    .unwrap(),
                    case["error"],
                    "PROTECTIVE native receiver class case {index}"
                );
                assert_eq!(
                    safe_cex_exchange::js_runtime::value::to_json(
                        &thrown.value.get("message").unwrap()
                    )
                    .unwrap(),
                    case["message"],
                    "PROTECTIVE native receiver message case {index}"
                );
                compared += 1;
            }
            assert_eq!(compared, 44, "PROTECTIVE native receiver error coverage");
            venue.dispose().unwrap();
        }
    });
}

#[test]
fn divergence_sdk_balance_starts_before_poll_and_keeps_its_dropped_continuation() {
    run(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitinverse,
            ExchangeName::Bybitspot,
        ] {
            for dispose in [false, true] {
                let exchange = venue(name.clone());
                let this = &exchange.js.get().unwrap().this;
                let runtime = exchange.base.runtime.clone();
                let pending = safe_cex_exchange::js_runtime::promise::Promise::pending(&runtime);
                let started = Rc::new(Cell::new(0));
                let count = started.clone();
                let response = pending.clone();
                this.set(
                    "fetchRawBalance",
                    Js::function("raw terminal", move |_, _| {
                        count.set(count.get() + 1);
                        Ok(Js::Promise(response.clone()))
                    }),
                )
                .unwrap();
                let formatted = Rc::new(Cell::new(0));
                let count = formatted.clone();
                let raw = Js::object(vec![
                    ("zero", Js::Number(-0.0)),
                    ("opaque", Js::utf16(&[0xd800])),
                ]);
                let expected = raw.clone();
                this.set(
                    "formatBalance",
                    Js::function("format terminal", move |_, args| {
                        assert!(
                            arg(&args, 0).same_reference(&expected),
                            "native raw identity survives the source await"
                        );
                        count.set(count.get() + 1);
                        Ok(Js::object(vec![("total", Js::Number(-0.0))]))
                    }),
                )
                .unwrap();
                let waiter = exchange.invoke::<safe_cex_types::Balance>("fetchBalance", vec![]);
                assert_eq!(
                    started.get(),
                    1,
                    "BYBIT SDK typed call invokes the original member before waiter polling: {name:?}"
                );
                assert_eq!(
                    formatted.get(),
                    0,
                    "held raw response does not format early"
                );
                drop(waiter);
                if dispose {
                    exchange.base.dispose().unwrap();
                }
                pending.settle(Ok(raw));
                for _ in 0..32 {
                    tokio::task::yield_now().await;
                }
                assert_eq!(
                    formatted.get(),
                    usize::from(!dispose),
                    "dropped SDK waiter preserves source continuation and disposal branch: {name:?}"
                );
                if !dispose {
                    exchange.base.dispose().unwrap();
                }
            }
        }
    });
}

#[test]
fn divergence_sdk_mode_starts_original_helper_before_waiter_poll() {
    run(async {
        let exchange = venue(ExchangeName::Bybit);
        let this = &exchange.js.get().unwrap().this;
        let started = Rc::new(Cell::new(0));
        let count = started.clone();
        let pending =
            safe_cex_exchange::js_runtime::promise::Promise::pending(&exchange.base.runtime);
        let result = pending.clone();
        this.set(
            "changePositionModeHelper",
            Js::function("mode terminal", move |_, args| {
                assert!(arg(&args, 0).strict_equals(&Js::Bool(true)));
                assert!(arg(&args, 1).strict_equals(&Js::str("linear")));
                assert!(arg(&args, 2).strict_equals(&Js::str("USDT")));
                count.set(count.get() + 1);
                Ok(Js::Promise(result.clone()))
            }),
        )
        .unwrap();
        let waiter = Exchange::change_position_mode(exchange.as_ref(), true, None);
        assert_eq!(
            started.get(),
            1,
            "BYBIT SDK unit call starts the real mode helper before waiter polling"
        );
        drop(waiter);
        pending.settle(Ok(Js::Undefined));
        tokio::task::yield_now().await;
        exchange.base.dispose().unwrap();
    });
}

#[test]
fn divergence_sdk_account_starts_original_client_before_waiter_poll() {
    run(async {
        let exchange = venue(ExchangeName::Bybit);
        let xhr = exchange.js.get().unwrap().this.get("xhr").unwrap();
        let started = Rc::new(Cell::new(0));
        let count = started.clone();
        let pending =
            safe_cex_exchange::js_runtime::promise::Promise::pending(&exchange.base.runtime);
        let result = pending.clone();
        xhr.set(
            "get",
            Js::function("account terminal", move |_, _| {
                count.set(count.get() + 1);
                Ok(Js::Promise(result.clone()))
            }),
        )
        .unwrap();
        let waiter = Exchange::get_account(exchange.as_ref());
        assert_eq!(
            started.get(),
            1,
            "BYBIT SDK account call starts original client before waiter polling"
        );
        pending.settle(Ok(Js::object(vec![(
            "data",
            Js::object(vec![
                ("retCode", Js::Number(0.0)),
                (
                    "result",
                    Js::object(vec![
                        ("userID", Js::Number(73.0)),
                        ("inviterID", Js::Number(91.0)),
                    ]),
                ),
            ]),
        )])));
        let result = waiter.await.unwrap();
        assert_eq!(result.user_id, "73");
        assert_eq!(
            result.affiliate_id.as_ref().and_then(|v| v.as_deref()),
            Some("91")
        );
        exchange.base.dispose().unwrap();
    });
}

#[test]
fn hyro_divergence_account_configuration_receivers() {
    if crate::types::HYROTRADER_PROD_URL["livenet"]
        .as_str()
        .unwrap_or("")
        .is_empty()
    {
        let directory =
            std::env::temp_dir().join(format!("bybit-hyro-account-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("configured.log");
        let log = std::fs::File::create(&path).unwrap();
        let mut child =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or("cargo".into()))
                .args([
                    "test",
                    "--locked",
                    "-p",
                    "safe-cex-venue-bybit",
                    "--lib",
                    "hyro_divergence_account_configuration_receivers",
                    "--jobs",
                    "1",
                    "--",
                    "--test-threads=1",
                    "--nocapture",
                ])
                .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
                .env("CARGO_INCREMENTAL", "0")
                .env(
                    "NEXT_PUBLIC_HYROTRADER_PROD_REST_URL",
                    "https://hyro-audit.example",
                )
                .env(
                    "NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL",
                    "https://hyro-sim-audit.example",
                )
                .stdout(std::process::Stdio::from(log.try_clone().unwrap()))
                .stderr(std::process::Stdio::from(log))
                .spawn()
                .unwrap();
        let deadline = Instant::now() + Duration::from_secs(600);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                let output = std::fs::read_to_string(&path).unwrap();
                assert!(status.success(), "configured Hyro audit failed: {output}");
                assert!(output.contains("1 passed; 0 failed"));
                return;
            }
            if Instant::now() > deadline {
                child.kill().expect("stop timed-out configured Hyro audit");
                child.wait().unwrap();
                panic!("configured Hyro audit timed out: {}", path.display());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    run(async {
        for mode in ["production", "demo"] {
            let options = serde_json::from_value(serde_json::json!({
                "key":"", "secret":"", "publicDataOnly":true,
                "enableOptionMarkets":true, "enableInverseMarkets":true,
                "extra":{"hyrotraderMode":mode}
            }))
            .unwrap();
            let exchange = venue_with_options(ExchangeName::Hyrotrader, options);
            let this = &exchange.js.get().unwrap().this;
            for name in ["optionsEnabled", "inverseEnabled"] {
                assert!(
                    this.call_method(name, vec![])
                        .unwrap()
                        .strict_equals(&Js::Bool(false)),
                    "hyro_capability_flags_ignore_optins"
                );
            }
            let poison = Js::object(vec![]);
            poison
                .define_accessor(
                    "startTime",
                    Some(Rc::new(|_| panic!("empty range must ignore arguments"))),
                    None,
                )
                .unwrap();
            let first = this
                .call_method(
                    "fetchIncomeHistoryRange",
                    vec![
                        poison.clone(),
                        Js::Number(f64::NAN),
                        Js::String(vec![0xd800].into()),
                    ],
                )
                .unwrap();
            let second = this
                .call_method("fetchIncomeHistoryRange", vec![poison])
                .unwrap();
            assert!(
                !first.strict_equals(&second),
                "fresh source Promise per call"
            );
            let first = await_js(&exchange.base.runtime, first).await.unwrap();
            let second = await_js(&exchange.base.runtime, second).await.unwrap();
            assert_eq!(
                first.array_values().unwrap().unwrap().len(),
                0,
                "hyro_empty_income_range"
            );
            assert!(!first.strict_equals(&second), "fresh empty array per call");
            let messages = Rc::new(RefCell::new(Vec::<Js>::new()));
            let seen = messages.clone();
            this.define(
                "emitAndLogError",
                Js::function("emitAndLogError", move |_, args| {
                    seen.borrow_mut().push(arg(&args, 0));
                    Ok(Js::Undefined)
                }),
            )
            .unwrap();
            let balance = Js::object(vec![
                ("total", Js::Number(-0.0)),
                ("opaque", Js::String(vec![0xd800].into())),
            ]);
            let result_balance = balance.clone();
            this.define(
                "fetchBalance",
                Js::function("fetchBalance", move |_, _| Ok(result_balance.clone())),
            )
            .unwrap();
            let result = this.call_method("validateAccount", vec![]).unwrap();
            let result = await_js(&exchange.base.runtime, result).await.unwrap();
            assert!(result.get("balance").unwrap().strict_equals(&balance));
            assert!(result.get("error").unwrap().strict_equals(&Js::str("")));
            let published = Rc::new(RefCell::new(Vec::<Js>::new()));
            let observed = published.clone();
            this.get("store")
                .unwrap()
                .define(
                    "update",
                    Js::function("update", move |_, args| {
                        observed.borrow_mut().push(arg(&args, 0));
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
            for disposed in [false, true] {
                let pending = safe_cex_exchange::js_runtime::promise::Promise::pending(
                    &exchange.base.runtime,
                );
                let request = pending.clone();
                let calls = Rc::new(Cell::new(0));
                let count = calls.clone();
                this.define(
                    "fetchBalance",
                    Js::function("fetchBalance", move |_, _| {
                        count.set(count.get() + 1);
                        Ok(Js::Promise(request.clone()))
                    }),
                )
                .unwrap();
                let result = this.call_method("fetchMarginAccountInfos", vec![]).unwrap();
                assert_eq!(calls.get(), 1, "Hyro balance begins before caller polling");
                let before = published.borrow().len();
                if disposed {
                    exchange.base.dispose().unwrap();
                }
                pending.settle(Ok(balance.clone()));
                let settled = tokio::time::timeout(
                    Duration::from_secs(1),
                    await_js(&exchange.base.runtime, result),
                )
                .await
                .unwrap()
                .unwrap();
                assert!(settled.is_undefined());
                assert_eq!(
                    published.borrow().len(),
                    before + usize::from(!disposed),
                    "hyro_margin_disposal_guard"
                );
                if !disposed {
                    assert!(
                        published
                            .borrow()
                            .last()
                            .unwrap()
                            .get("balance")
                            .unwrap()
                            .strict_equals(&balance)
                    );
                }
            }
        }
        let exchange = venue(ExchangeName::Hyrotrader);
        let this = &exchange.js.get().unwrap().this;
        let rt = exchange.js.get().unwrap();
        let prototype = rt.global("Error").unwrap().get("prototype").unwrap();
        let reason = Js::object(vec![("message", Js::String(vec![0xd800, 0x61].into()))]);
        rt.global("Object")
            .unwrap()
            .call_method("setPrototypeOf", vec![reason.clone(), prototype])
            .unwrap();
        let rejected = reason.clone();
        this.define(
            "fetchBalance",
            Js::function("fetchBalance", move |_, _| {
                Err(Thrown::new(rejected.clone()))
            }),
        )
        .unwrap();
        let logged = Rc::new(RefCell::new(Js::Undefined));
        let sink = logged.clone();
        this.define(
            "emitAndLogError",
            Js::function("emitAndLogError", move |_, args| {
                *sink.borrow_mut() = arg(&args, 0);
                Ok(Js::Undefined)
            }),
        )
        .unwrap();
        let result = this.call_method("validateAccount", vec![]).unwrap();
        let result = await_js(&exchange.base.runtime, result).await.unwrap();
        assert_eq!(
            units(&result.get("error").unwrap()),
            [0xd800, 0x61],
            "hyro_error_prototype_message"
        );
        assert_eq!(units(&logged.borrow()), [0xd800, 0x61]);
        let raw_message = Js::object(vec![("zero", Js::Number(-0.0))]);
        let raw_error = rt.global("Error").unwrap().construct(vec![], None).unwrap();
        raw_error.set("message", raw_message.clone()).unwrap();
        let detached = rt
            .global("Error")
            .unwrap()
            .construct(vec![Js::str("original")], None)
            .unwrap();
        rt.global("Object")
            .unwrap()
            .call_method("setPrototypeOf", vec![detached.clone(), Js::Null])
            .unwrap();
        detached
            .define(
                "toString",
                Js::function("toString", |_, _| Ok(Js::utf16(&[0xd800, 0x62]))),
            )
            .unwrap();
        let custom = Js::object(vec![(
            "toString",
            Js::function("toString", |_, _| Ok(Js::utf16(&[0xd800, 0x63]))),
        )]);
        for (reason, expected) in [
            (custom, Js::utf16(&[0xd800, 0x63])),
            (Js::utf16(&[0xdfff]), Js::utf16(&[0xdfff])),
            (raw_error, raw_message),
            (detached, Js::utf16(&[0xd800, 0x62])),
        ] {
            for synchronous in [false, true] {
                let failure = reason.clone();
                let runtime = exchange.base.runtime.clone();
                this.define(
                    "fetchBalance",
                    Js::function("fetchBalance", move |_, _| {
                        if synchronous {
                            Err(Thrown::new(failure.clone()))
                        } else {
                            Ok(Js::Promise(
                                safe_cex_exchange::js_runtime::promise::Promise::settled(
                                    &runtime,
                                    Err(Thrown::new(failure.clone())),
                                ),
                            ))
                        }
                    }),
                )
                .unwrap();
                let result = this.call_method("validateAccount", vec![]).unwrap();
                let result = await_js(&exchange.base.runtime, result).await.unwrap();
                assert!(result.get("error").unwrap().strict_equals(&expected));
                assert!(logged.borrow().strict_equals(&expected));
                assert!(!result.has_own("balance"));
            }
        }
        let logging = Js::object(vec![("opaque", Js::utf16(&[0xd800]))]);
        let thrown = logging.clone();
        this.define(
            "emitAndLogError",
            Js::function("emitAndLogError", move |_, _| {
                Err(Thrown::new(thrown.clone()))
            }),
        )
        .unwrap();
        let result = this.call_method("validateAccount", vec![]).unwrap();
        assert!(
            await_js(&exchange.base.runtime, result)
                .await
                .unwrap_err()
                .value
                .strict_equals(&logging)
        );
        assert!(
            safe_cex_exchange::capabilities::is_base_capability_fallback(
                &super::capabilities(&ExchangeName::Hyrotrader).fetch_income_history_range,
                safe_cex_exchange::capabilities::BaseCapabilityMethod::FetchIncomeHistoryRange,
            )
        );
        exchange.base.dispose().unwrap();
    });
}

#[test]
fn divergence_audit_order_index_keeps_live_options_and_rejection_identity() {
    run(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitinverse,
        ] {
            let exchange = venue(name);
            let runtime = exchange.base.runtime.clone();
            let this = &exchange.js.get().unwrap().this;
            let symbol = Js::object(vec![("opaque", Js::String(vec![0xd800].into()))]);
            let opts = Js::object(vec![
                ("symbol", symbol.clone()),
                ("side", Js::str("buy")),
                ("reduceOnly", Js::Bool(true)),
            ]);
            for reject in [false, true] {
                let pending = safe_cex_exchange::js_runtime::promise::Promise::pending(&runtime);
                let request = pending.clone();
                let original_symbol = symbol.clone();
                let calls = Rc::new(Cell::new(0));
                let count = calls.clone();
                this.define(
                    "fetchPositionMode",
                    Js::function("fetchPositionMode", move |_, args| {
                        assert!(arg(&args, 0).strict_equals(&original_symbol));
                        count.set(count.get() + 1);
                        Ok(Js::Promise(request.clone()))
                    }),
                )
                .unwrap();
                let result = this
                    .call_method("getOrderPositionIdx", vec![opts.clone()])
                    .unwrap();
                assert_eq!(calls.get(), 1, "mode request starts before returning");
                opts.set("side", Js::str("sell")).unwrap();
                opts.set("reduceOnly", Js::object(vec![])).unwrap();
                let reason = Js::object(vec![("message", Js::String(vec![0xd800].into()))]);
                pending.settle(if reject {
                    Err(Thrown::new(reason.clone()))
                } else {
                    Ok(Js::str("hedged"))
                });
                let result = await_js(&runtime, result).await;
                if reject {
                    assert!(result.unwrap_err().value.strict_equals(&reason));
                } else {
                    assert!(
                        result.unwrap().strict_equals(&Js::Number(1.)),
                        "post-await reads see the same options graph"
                    );
                }
            }
            exchange.base.dispose().unwrap();
        }
        let exchange = venue(ExchangeName::Bybitspot);
        let this = &exchange.js.get().unwrap().this;
        let opts = Js::object(vec![]);
        opts.define_accessor(
            "symbol",
            Some(Rc::new(|_| panic!("Spot must ignore options"))),
            None,
        )
        .unwrap();
        let result = this.call_method("getOrderPositionIdx", vec![opts]).unwrap();
        assert!(
            await_js(&exchange.base.runtime, result)
                .await
                .unwrap()
                .strict_equals(&Js::Number(0.))
        );
        exchange.base.dispose().unwrap();
    });
}

#[test]
fn divergence_audit_primary_stop_index_is_a_live_member() {
    run(async {
        let exchange = venue(ExchangeName::Bybit);
        let this = &exchange.js.get().unwrap().this;
        let called = Rc::new(Cell::new(0));
        let calls = called.clone();
        this.define(
            "getOrderPositionIdx",
            Js::function("getOrderPositionIdx", move |_, args| {
                calls.set(calls.get() + 1);
                assert!(!arg(&args, 0).has_own("reduceOnly"));
                Ok(Js::Number(1.))
            }),
        )
        .unwrap();
        let result = this
            .call_method(
                "getStopOrderPositionIdx",
                vec![Js::object(vec![("reduceOnly", Js::Bool(true))])],
            )
            .expect("stop_index_live_member");
        assert!(
            await_js(&exchange.base.runtime, result)
                .await
                .unwrap()
                .strict_equals(&Js::Number(2.))
        );
        assert_eq!(called.get(), 1);
        exchange.base.dispose().unwrap();
    });
}

#[test]
fn divergence_audit_stop_index_copies_inherited_values_and_rejects_asynchronously() {
    run(async {
        for name in [
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
        ] {
            let exchange = venue(name);
            let this = &exchange.js.get().unwrap().this;
            let rt = exchange.js.get().unwrap();
            let opaque = Js::object(vec![
                ("text", Js::String(vec![0xd800].into())),
                ("zero", Js::Number(-0.0)),
            ]);
            let proto = Js::object(vec![("inherited", opaque.clone())]);
            let opts = Js::object(vec![]);
            rt.global("Object")
                .unwrap()
                .call_method("setPrototypeOf", vec![opts.clone(), proto])
                .unwrap();
            opts.set("side", Js::str("buy")).unwrap();
            opts.set("reduceOnly", Js::Bool(true)).unwrap();
            let request =
                safe_cex_exchange::js_runtime::promise::Promise::pending(&exchange.base.runtime);
            let pending = request.clone();
            let shared = opaque.clone();
            let original = opts.clone();
            this.define(
                "getOrderPositionIdx",
                Js::function("getOrderPositionIdx", move |_, args| {
                    let copied = arg(&args, 0);
                    assert!(!copied.strict_equals(&original));
                    assert!(!copied.has_own("reduceOnly"));
                    assert!(
                        copied.get("inherited")?.strict_equals(&shared),
                        "stop_index_inherited_alias"
                    );
                    assert!(original.has_own("reduceOnly"));
                    Ok(Js::Promise(pending.clone()))
                }),
            )
            .unwrap();
            let result = this
                .call_method("getStopOrderPositionIdx", vec![opts])
                .unwrap();
            request.settle(Ok(Js::str("1")));
            assert!(
                await_js(&exchange.base.runtime, result)
                    .await
                    .unwrap()
                    .strict_equals(&Js::Number(2.)),
                "source table uses property-key coercion"
            );
            for index in [
                Js::Number(0.),
                Js::Number(1.),
                Js::Number(2.),
                Js::Number(3.),
                Js::str("constructor"),
            ] {
                let returned = index.clone();
                this.define(
                    "getOrderPositionIdx",
                    Js::function("getOrderPositionIdx", move |_, _| Ok(returned.clone())),
                )
                .unwrap();
                let result = this
                    .call_method("getStopOrderPositionIdx", vec![Js::object(vec![])])
                    .unwrap();
                let value = await_js(&exchange.base.runtime, result).await.unwrap();
                if index.is_string() {
                    assert!(value.is_callable(), "inherited table member remains native");
                } else if index.strict_equals(&Js::Number(3.)) {
                    assert!(value.is_undefined());
                } else {
                    let expected = [0., 2., 1.][index.to_number().unwrap() as usize];
                    assert!(value.strict_equals(&Js::Number(expected)));
                }
            }
            let reason = Js::object(vec![("opaque", Js::String(vec![0xdfff].into()))]);
            let thrown = reason.clone();
            let opts = Js::object(vec![]);
            opts.define_accessor(
                "side",
                Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
                None,
            )
            .unwrap();
            let result = this
                .call_method("getStopOrderPositionIdx", vec![opts])
                .expect("async member must return rejected Promise");
            assert!(
                await_js(&exchange.base.runtime, result)
                    .await
                    .unwrap_err()
                    .value
                    .strict_equals(&reason)
            );
            exchange.base.dispose().unwrap();
        }
    });
}

#[test]
fn divergence_audit_position_mode_maps_with_the_original_array_and_receiver() {
    run(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            let exchange = venue(name);
            let this = &exchange.js.get().unwrap().this;
            let rows = Js::array(vec![Js::object(vec![("symbol", Js::str("BTCUSDT"))])]);
            let response = Js::object(vec![(
                "data",
                Js::object(vec![
                    ("retCode", Js::Number(0.)),
                    ("result", Js::object(vec![("list", rows.clone())])),
                ]),
            )]);
            let count = Rc::new(Cell::new(0));
            let mapped = Js::object(vec![("positionMode", Js::String(vec![0xd800].into()))]);
            let mapped_row = mapped.clone();
            let input_rows = rows.clone();
            let receiver = this.clone();
            let calls = count.clone();
            this.define(
                "mapPosition",
                Js::function("mapPosition", move |actual_this, args| {
                    assert!(actual_this.strict_equals(&receiver));
                    if args.len() != 3 {
                        return Err(Thrown::error("Error", "position_mode_mapper_arguments"));
                    }
                    assert!(arg(&args, 2).strict_equals(&input_rows));
                    assert!(arg(&args, 0).strict_equals(&input_rows.get("0")?));
                    assert!(arg(&args, 1).strict_equals(&Js::Number(0.)));
                    calls.set(calls.get() + 1);
                    Ok(mapped_row.clone())
                }),
            )
            .unwrap();
            let published = Rc::new(RefCell::new(Js::Undefined));
            let observed = published.clone();
            this.define(
                "store",
                Js::object(vec![
                    ("positions", Js::array(vec![])),
                    (
                        "addOrUpdatePositions",
                        Js::function("addOrUpdatePositions", move |_, args| {
                            *observed.borrow_mut() = arg(&args, 0);
                            Ok(Js::Undefined)
                        }),
                    ),
                ]),
            )
            .unwrap();
            this.define(
                "xhr",
                Js::object(vec![(
                    "get",
                    Js::function("get", move |_, _| Ok(response.clone())),
                )]),
            )
            .unwrap();
            let result = this
                .call_method("fetchPositionMode", vec![Js::str("BTCUSDT")])
                .unwrap();
            let mode = await_js(&exchange.base.runtime, result).await.unwrap();
            assert_eq!(units(&mode), [0xd800]);
            assert_eq!(count.get(), 1);
            assert!(published.borrow().get("0").unwrap().strict_equals(&mapped));
            exchange.base.dispose().unwrap();
        }
    });
}

#[test]
fn retained_audit_ticker_native_lookup_and_prefix() {
    run(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
        ] {
            let exchange = venue(name.clone());
            let rt = exchange.js.get().unwrap();
            let cache = Js::array(vec![Js::object(vec![("opaque", Js::utf16(&[0xd800]))])]);
            let market = Js::object(vec![
                ("id", Js::str("native")),
                ("symbol", Js::str("BTC")),
                ("isInverse", Js::Bool(true)),
                ("isExpiring", Js::Bool(true)),
            ]);
            rt.this
                .define(
                    "store",
                    Js::object(vec![
                        ("tickers", cache.clone()),
                        ("marketsBySymbol", Js::object(vec![("BTC", market)])),
                    ]),
                )
                .unwrap();
            let cached = rt.this.call_method("parseTickers", vec![Js::Null]).unwrap();
            assert!(
                cached.strict_equals(&cache),
                "ticker_cached_identity {name:?}"
            );
            let row = Js::object(vec![
                ("symbol", Js::str("BTC")),
                ("lastPrice", Js::utf16(&[50, 0xd800])),
                ("volume24h", Js::str("-0")),
                ("turnover24h", Js::Number(f64::INFINITY)),
            ]);
            row.define_accessor(
                "unused",
                Some(Rc::new(|_| panic!("unread ticker field"))),
                None,
            )
            .unwrap();
            let store = rt.this.get("store").unwrap();
            row.define_accessor(
                "lastPrice",
                Some(Rc::new(move |_| {
                    store.set(
                        "marketsBySymbol",
                        Js::object(vec![(
                            "ETH",
                            Js::object(vec![("id", Js::str("next")), ("symbol", Js::str("ETH"))]),
                        )]),
                    )?;
                    Ok(Js::utf16(&[50, 0xd800]))
                })),
                None,
            )
            .unwrap();
            let second = Js::object(vec![
                ("symbol", Js::str("ETH")),
                ("lastPrice", Js::str("3")),
                ("volume24h", Js::str("1")),
            ]);
            let result = rt
                .this
                .call_method("parseTickers", vec![Js::array(vec![row, second])])
                .unwrap();
            assert_eq!(
                result.get("length").unwrap().to_number().unwrap(),
                2.,
                "ticker_live_dictionary"
            );
            assert!(
                result
                    .get("1")
                    .unwrap()
                    .get("id")
                    .unwrap()
                    .strict_equals(&Js::str("next"))
            );
            let ticker = result.get("0").unwrap();
            assert_eq!(
                ticker.get("last").unwrap().to_number().unwrap(),
                2.,
                "ticker_utf16_prefix"
            );
            let spot = name == ExchangeName::Bybitspot;
            let volume = ticker.get("volume").unwrap().to_number().unwrap();
            assert_eq!(
                volume.to_bits(),
                if spot {
                    (-0.0f64).to_bits()
                } else {
                    f64::INFINITY.to_bits()
                },
                "ticker_inverse_volume"
            );
            assert_eq!(
                ticker
                    .get("quoteVolume")
                    .unwrap()
                    .to_number()
                    .unwrap()
                    .to_bits(),
                (-0.0f64).to_bits()
            );
            assert!(ticker.get("bid").unwrap().to_number().unwrap().is_nan());
            assert_eq!(ticker.get("fundingRate").unwrap().to_number().unwrap(), 0.);
            exchange.dispose().unwrap();
        }
    });
}

#[test]
fn retained_audit_spot_market_native_info_and_eager_request() {
    run(async {
        let exchange = venue(ExchangeName::Bybitspot);
        let rt = exchange.js.get().unwrap();
        let info = Js::object(vec![
            ("symbol", Js::str("BTCUSDT")),
            ("baseCoin", Js::str("BTC")),
            ("quoteCoin", Js::str("USDT")),
            ("status", Js::str("Trading")),
            (
                "lotSizeFilter",
                Js::object(vec![
                    ("qtyStep", Js::str("0.001")),
                    ("minOrderQty", Js::str("0.001")),
                    ("maxOrderQty", Js::str("100")),
                ]),
            ),
            (
                "priceFilter",
                Js::object(vec![("tickSize", Js::str("0.1"))]),
            ),
            (
                "opaque",
                Js::object(vec![
                    ("missing", Js::Undefined),
                    ("text", Js::utf16(&[0xd800])),
                    ("number", Js::Number(f64::INFINITY)),
                ]),
            ),
        ]);
        let response = Js::object(vec![(
            "data",
            Js::object(vec![
                ("retMsg", Js::str("OK")),
                ("result", Js::object(vec![("list", Js::array(vec![info]))])),
            ]),
        )]);
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        rt.this
            .define(
                "xhr",
                Js::object(vec![(
                    "get",
                    Js::function("get", move |_, args| {
                        assert_eq!(
                            crate::binding::host::text(&args[0])?,
                            "/v5/market/instruments-info"
                        );
                        assert_eq!(
                            crate::binding::host::text(&args[1].get("params")?.get("category")?)?,
                            "spot"
                        );
                        count.set(count.get() + 1);
                        Ok(response.clone())
                    }),
                )]),
            )
            .unwrap();
        let pending = rt
            .this
            .call_method("fetchMarketsHelper", vec![Js::str("spot")])
            .unwrap();
        assert_eq!(calls.get(), 1, "spot_market_eager_request");
        let markets = await_js(&exchange.base.runtime, pending).await.unwrap();
        let market = markets.get("0").unwrap();
        let opaque = market.get("info").unwrap().get("opaque").unwrap();
        assert_eq!(
            units(&opaque.get("text").unwrap()),
            [0xd800],
            "spot_market_utf16_info"
        );
        assert!(opaque.has_own("missing"), "spot_market_own_undefined");
        assert!(matches!(opaque.get("missing").unwrap(), Js::Undefined));
        assert_eq!(
            opaque.get("number").unwrap().to_number().unwrap(),
            f64::INFINITY
        );
        assert_eq!(
            market
                .get("limits")
                .unwrap()
                .get("leverage")
                .unwrap()
                .get("max")
                .unwrap()
                .to_number()
                .unwrap(),
            1.
        );
        exchange.dispose().unwrap();
    });
}

#[test]
fn retained_audit_startup_dropped_waiter_and_native_warning() {
    run(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitspot] {
            let options =
                serde_json::from_value(serde_json::json!({"key":"key", "secret":"secret"}))
                    .unwrap();
            let exchange = venue_with_options(name.clone(), options);
            let rt = exchange.js.get().unwrap();
            let runtime = exchange.base.runtime.clone();
            let clock = safe_cex_exchange::js_runtime::promise::Promise::pending(&runtime);
            let clock_result = clock.clone();
            rt.global("virtualClock")
                .unwrap()
                .define(
                    "start",
                    Js::function("start", move |_, _| Ok(Js::Promise(clock_result.clone()))),
                )
                .unwrap();
            rt.this
                .define(
                    "initExternalPublicData",
                    Js::function("external", |_, _| Ok(Js::Bool(true))),
                )
                .unwrap();
            let cache_error = Js::object(vec![("message", Js::utf16(&[0xdfff]))]);
            let cache_reason = cache_error.clone();
            rt.this
                .define(
                    "fetchAndCacheApiKeyInfo",
                    Js::function("cache", move |_, _| {
                        Ok(Js::Promise(
                            safe_cex_exchange::js_runtime::promise::Promise::settled(
                                &runtime,
                                Err(Thrown::new(cache_reason.clone())),
                            ),
                        ))
                    }),
                )
                .unwrap();
            let reports = Rc::new(RefCell::new(Vec::<Js>::new()));
            let reported = reports.clone();
            rt.this
                .define(
                    "emitAndLogError",
                    Js::function("report", move |_, args| {
                        reported.borrow_mut().push(arg(&args, 0));
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
            let reason = Js::object(vec![("message", Js::utf16(&[0xd800, 120]))]);
            let probe =
                safe_cex_exchange::js_runtime::promise::Promise::pending(&exchange.base.runtime);
            let held = probe.clone();
            rt.this
                .define(
                    "fetchMarginAccountInfos",
                    Js::function("probe", move |_, _| Ok(Js::Promise(held.clone()))),
                )
                .unwrap();
            let messages = Rc::new(RefCell::new(Vec::<Vec<Js>>::new()));
            let logs = messages.clone();
            rt.this
                .define(
                    "log",
                    Js::function("log", move |_, args| {
                        logs.borrow_mut().push(args);
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
            let connected = Rc::new(Cell::new(0));
            let count = connected.clone();
            rt.this
                .define(
                    "privateWebsocket",
                    Js::object(vec![(
                        "connectAndSubscribe",
                        Js::function("connect", move |_, _| {
                            count.set(count.get() + 1);
                            Ok(Js::Undefined)
                        }),
                    )]),
                )
                .unwrap();
            rt.this
                .define(
                    "fetchOrdersPositionsAndBalance",
                    Js::function("snapshots", |_, _| Ok(Js::Undefined)),
                )
                .unwrap();
            rt.this
                .define("tick", Js::function("tick", |_, _| Ok(Js::Undefined)))
                .unwrap();
            drop(exchange.start());
            clock.settle(Ok(Js::Undefined));
            probe.settle(Err(Thrown::new(reason)));
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert_eq!(connected.get(), 1, "startup_dropped_waiter {name:?}");
            let logs = messages.borrow();
            assert_eq!(logs.len(), 1);
            let mut expected: Vec<u16> =
                "Bybit market data is ready, but the private API was rejected ("
                    .encode_utf16()
                    .collect();
            expected.extend([0xd800, 120]);
            expected.extend(") — trading is unavailable".encode_utf16());
            assert_eq!(
                units(&logs[0][0]),
                expected,
                "startup_native_warning {name:?}"
            );
            assert_eq!(
                units(&logs[0][1]),
                "warning".encode_utf16().collect::<Vec<_>>()
            );
            assert_eq!(
                reports.borrow().len(),
                usize::from(name == ExchangeName::Bybit),
                "startup_cache_failure_visible"
            );
            if name == ExchangeName::Bybit {
                assert!(
                    reports.borrow()[0].strict_equals(&cache_error),
                    "startup_cache_failure_identity"
                );
            }
            drop(logs);
            exchange.dispose().unwrap();
        }
    });
}

#[test]
fn retained_audit_startup_preserves_family_request_order() {
    run(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitspot] {
            let exchange = venue(name.clone());
            let rt = exchange.js.get().unwrap();
            let runtime = exchange.base.runtime.clone();
            rt.global("virtualClock")
                .unwrap()
                .define("start", Js::function("start", |_, _| Ok(Js::Undefined)))
                .unwrap();
            rt.this
                .define(
                    "initExternalPublicData",
                    Js::function("external", |_, _| Ok(Js::Bool(false))),
                )
                .unwrap();
            let markets = safe_cex_exchange::js_runtime::promise::Promise::pending(&runtime);
            let held = markets.clone();
            rt.this
                .define(
                    "fetchMarkets",
                    Js::function("markets", move |_, _| Ok(Js::Promise(held.clone()))),
                )
                .unwrap();
            let requests = Rc::new(Cell::new(0));
            let calls = requests.clone();
            let raw = Js::array(vec![]);
            let raw_result = raw.clone();
            rt.this
                .define(
                    "fetchTickersWithoutParse",
                    Js::function("raw", move |_, _| {
                        calls.set(calls.get() + 1);
                        Ok(raw_result.clone())
                    }),
                )
                .unwrap();
            let parsed = Rc::new(Cell::new(0));
            let calls = parsed.clone();
            rt.this
                .define(
                    "parseTickers",
                    Js::function("parse", move |_, args| {
                        assert!(args[0].strict_equals(&raw));
                        calls.set(calls.get() + 1);
                        Ok(Js::array(vec![]))
                    }),
                )
                .unwrap();
            for key in ["publicWebsocket", "inversePublicWebsocket"] {
                rt.this
                    .define(
                        key,
                        Js::object(vec![(
                            "connectAndSubscribe",
                            Js::function("connect", |_, _| Ok(Js::Undefined)),
                        )]),
                    )
                    .unwrap();
            }
            rt.this
                .define("log", Js::function("log", |_, _| Ok(Js::Undefined)))
                .unwrap();
            let pending = rt.this.call_method("start", vec![]).unwrap();
            for _ in 0..20 {
                tokio::task::yield_now().await;
            }
            assert_eq!(
                requests.get(),
                usize::from(name == ExchangeName::Bybit),
                "startup_parallel_vs_sequential"
            );
            assert_eq!(parsed.get(), 0);
            markets.settle(Ok(Js::array(vec![])));
            await_js(&runtime, pending).await.unwrap();
            assert_eq!(requests.get(), 1);
            assert_eq!(parsed.get(), 1);
            let loaded = rt.this.get("store").unwrap().get("loaded").unwrap();
            for key in ["markets", "tickers", "balance", "orders", "positions"] {
                assert!(
                    loaded.get(key).unwrap().truthy(),
                    "startup_loaded_flag {key}"
                );
            }
            exchange.dispose().unwrap();
        }
    });
}

#[test]
fn audit_unknown_outcome_preserves_native_submission_rejection() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/primary-batch-binding-oracle.cjs"
        ))
        .arg("--unknown-outcome")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 28);
    run(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            for (profile, expected) in source.iter().enumerate() {
                let exchange = venue(name.clone());
                let rt = exchange.js.get().unwrap();
                if profile >= 12 {
                    let events = Rc::new(RefCell::new(Vec::<String>::new()));
                    let marker = Js::object(vec![("marker", Js::Number(73.))]);
                    let input = Js::object(vec![("message", Js::str("timeout"))]);
                    let input = match profile {
                        12 => {
                            for (key, value) in [
                                ("code", Js::Undefined),
                                ("message", Js::str("timeout")),
                                ("name", Js::Undefined),
                                ("response", Js::Undefined),
                            ] {
                                let trace = events.clone();
                                input
                                    .define_accessor(
                                        key,
                                        Some(Rc::new(move |_| {
                                            trace.borrow_mut().push(key.into());
                                            Ok(value.clone())
                                        })),
                                        None,
                                    )
                                    .unwrap();
                            }
                            input
                        }
                        13 => {
                            let reason = marker.clone();
                            input
                                .define_accessor(
                                    "name",
                                    Some(Rc::new(move |_| Err(Thrown::new(reason.clone())))),
                                    None,
                                )
                                .unwrap();
                            input
                        }
                        14 => {
                            let trace = events.clone();
                            input
                                .set(
                                    "message",
                                    Js::object(vec![(
                                        "toString",
                                        Js::function("toString", move |_, _| {
                                            trace.borrow_mut().push("string".into());
                                            Ok(Js::str("timeout"))
                                        }),
                                    )]),
                                )
                                .unwrap();
                            input
                        }
                        15 => {
                            let reason = marker.clone();
                            input
                                .set(
                                    "message",
                                    Js::object(vec![(
                                        "toString",
                                        Js::function("toString", move |_, _| {
                                            Err(Thrown::new(reason.clone()))
                                        }),
                                    )]),
                                )
                                .unwrap();
                            input
                        }
                        16 | 17 => {
                            let reason = marker.clone();
                            let response = Js::object(vec![(
                                "status",
                                Js::Number(if profile == 16 { 503. } else { 400. }),
                            )]);
                            response
                                .define_accessor(
                                    "data",
                                    Some(Rc::new(move |_| Err(Thrown::new(reason.clone())))),
                                    None,
                                )
                                .unwrap();
                            input.define("response", response).unwrap();
                            input
                        }
                        18 => {
                            let f = Js::function("error", |_, _| Ok(Js::Undefined));
                            f.define("message", Js::str("timeout")).unwrap();
                            f
                        }
                        19 => {
                            let array = Js::array(vec![]);
                            rt.global("Object")
                                .unwrap()
                                .call_method(
                                    "setPrototypeOf",
                                    vec![
                                        array.clone(),
                                        Js::object(vec![("message", Js::str("timeout"))]),
                                    ],
                                )
                                .unwrap();
                            array
                        }
                        20 => {
                            input.set("message", Js::Null).unwrap();
                            input
                        }
                        21 => {
                            let trace = events.clone();
                            let status = Js::object(vec![(
                                "valueOf",
                                Js::function("valueOf", move |_, _| {
                                    trace.borrow_mut().push("status".into());
                                    Ok(Js::Number(400.))
                                }),
                            )]);
                            let trace = events.clone();
                            let code = Js::object(vec![(
                                "valueOf",
                                Js::function("valueOf", move |_, _| {
                                    trace.borrow_mut().push("venue".into());
                                    Ok(Js::Number(-1007.))
                                }),
                            )]);
                            input
                                .define(
                                    "response",
                                    Js::object(vec![
                                        ("status", status),
                                        ("data", Js::object(vec![("code", code)])),
                                    ]),
                                )
                                .unwrap();
                            input
                        }
                        22..=27 => {
                            let big = |text: &str| rt.global("BigInt").unwrap().call(Js::Undefined, vec![Js::str(text)]).unwrap();
                            let huge = big(&format!("1{}", "0".repeat(400)));
                            let status = match profile {
                                22 => big("503"),
                                23 => Js::Number(400.),
                                24 => {
                                    let trace = events.clone();
                                    let value = big("503");
                                    Js::object(vec![("valueOf", Js::function("valueOf", move |_, _| {
                                        trace.borrow_mut().push("big-status".into());
                                        Ok(value.clone())
                                    }))])
                                },
                                26 => huge.clone(),
                                _ => big("499"),
                            };
                            let code = if profile == 25 {
                                let trace = events.clone();
                                let value = big("-1007");
                                Js::object(vec![("valueOf", Js::function("valueOf", move |_, _| {
                                    trace.borrow_mut().push("big-venue".into());
                                    Ok(value.clone())
                                }))])
                            } else if profile == 27 { huge } else { big("-1007") };
                            input.define("response", Js::object(vec![("status", status), ("data", Js::object(vec![("code", code)]))])).unwrap();
                            input
                        }
                        _ => unreachable!(),
                    };
                    let result = rt
                        .global("isUnknownOutcome")
                        .unwrap()
                        .call(Js::Undefined, vec![input]);
                    let value = result.as_ref().ok().map(Js::truthy);
                    let observed = serde_json::json!({"value":value,"original":result.as_ref().err().is_some_and(|e|e.value.same_reference(&marker)),"events":*events.borrow()});
                    assert_eq!(
                        &observed, expected,
                        "UNKNOWN_OUTCOME_PROPERTY {name:?} profile {profile}"
                    );
                    exchange.base.dispose().unwrap();
                    continue;
                }
                let reason = Js::object(vec![("message", Js::str("timeout"))]);
                match profile {
                    0 => reason
                        .define(
                            "opaque",
                            Js::object(vec![
                                ("text", Js::utf16(&[0xd800])),
                                ("number", Js::Number(f64::NAN)),
                                ("missing", Js::Undefined),
                            ]),
                        )
                        .unwrap(),
                    1 => rt
                        .global("Object")
                        .unwrap()
                        .call_method(
                            "defineProperty",
                            vec![
                                reason.clone(),
                                Js::str("message"),
                                Js::object(vec![
                                    ("value", Js::str("timeout")),
                                    ("enumerable", Js::Bool(false)),
                                ]),
                            ],
                        )
                        .map(|_| ())
                        .unwrap(),
                    2 => {
                        reason.set("message", Js::str("rejected")).unwrap();
                        rt.global("Object")
                            .unwrap()
                            .call_method(
                                "setPrototypeOf",
                                vec![
                                    reason.clone(),
                                    Js::object(vec![("code", Js::str("ECONNRESET"))]),
                                ],
                            )
                            .unwrap();
                    }
                    3 => reason
                        .define(
                            "response",
                            Js::object(vec![("status", Js::Number(f64::INFINITY))]),
                        )
                        .unwrap(),
                    4 => {
                        reason
                            .set("message", Js::str("Remote signer unavailable"))
                            .unwrap();
                        reason
                            .define("response", Js::object(vec![("status", Js::Number(503.))]))
                            .unwrap();
                    }
                    5 => reason
                        .define("response", Js::object(vec![("status", Js::Number(400.))]))
                        .unwrap(),
                    6 => {
                        reason.set("message", Js::str("rejected")).unwrap();
                        reason
                            .define(
                                "response",
                                Js::object(vec![
                                    ("status", Js::Number(400.)),
                                    ("data", Js::object(vec![("code", Js::Number(-1007.))])),
                                ]),
                            )
                            .unwrap();
                    }
                    7 => {
                        reason.set("message", Js::str("rejected")).unwrap();
                        reason.define("name", Js::str("TradeTunnel")).unwrap();
                    }
                    8 => reason
                        .define_accessor(
                            "unused",
                            Some(Rc::new(|_| Err(Thrown::error("Error", "unread getter")))),
                            None,
                        )
                        .unwrap(),
                    9 => {
                        reason.set("message", Js::str("rejected")).unwrap();
                        reason
                            .define(
                                "name",
                                Js::utf16(&[
                                    84, 114, 97, 100, 101, 84, 117, 110, 110, 101, 108, 0xd800,
                                ]),
                            )
                            .unwrap();
                    }
                    10 => {
                        let response = Js::object(vec![("status", Js::Number(503.))]);
                        response
                            .define_accessor(
                                "unused",
                                Some(Rc::new(|_| {
                                    Err(Thrown::error("Error", "unread response field"))
                                })),
                                None,
                            )
                            .unwrap();
                        reason.define("response", response).unwrap();
                    }
                    11 => {}
                    _ => unreachable!(),
                }
                let sends = Rc::new(Cell::new(0));
                let count = sends.clone();
                let rejected = reason.clone();
                let runtime = exchange.base.runtime.clone();
                rt.this
                    .get("xhr")
                    .unwrap()
                    .define(
                        "post",
                        Js::function("post", move |_, _| {
                            count.set(count.get() + 1);
                            Ok(Js::Promise(
                                safe_cex_exchange::js_runtime::promise::Promise::settled(
                                    &runtime,
                                    Err(Thrown::new(rejected.clone())),
                                ),
                            ))
                        }),
                    )
                    .unwrap();
                let reports = Rc::new(Cell::new(0));
                let count = reports.clone();
                rt.this
                    .define(
                        "emitAndLogError",
                        Js::function("report", move |_, _| {
                            count.set(count.get() + 1);
                            Ok(Js::Undefined)
                        }),
                    )
                    .unwrap();
                let result = rt
                    .this
                    .call_method(
                        "submitCreateOrders",
                        vec![
                            Js::array(vec![Js::object(vec![])]),
                            Js::object(vec![(
                                "clientOrderId",
                                if profile == 11 {
                                    Js::Undefined
                                } else {
                                    Js::str("caller")
                                },
                            )]),
                        ],
                    )
                    .unwrap();
                let result = await_js(&exchange.base.runtime, result).await;
                let observed = serde_json::json!({"rejected":result.is_err(), "original":result.as_ref().err().is_some_and(|e| e.value.same_reference(&reason)), "sends":sends.get(), "reports":reports.get()});
                assert_eq!(
                    &observed, expected,
                    "UNKNOWN_OUTCOME_BINDING {name:?} profile {profile}: {result:?}"
                );
                exchange.base.dispose().unwrap();
            }
        }
    });
}
