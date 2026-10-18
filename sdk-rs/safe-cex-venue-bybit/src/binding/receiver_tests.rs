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
    let runtime = native_runtime();
    BybitExchange::create(
        name,
        serde_json::from_value(serde_json::json!({"key":"", "secret":"", "publicDataOnly":true}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        BybitDependencies {
            runtime,
            remote_signer: None,
            wire: Arc::new(NoWire),
            http_host: Rc::new(NativeHttpHost::default()),
            rate_limit_clock: Arc::new(Clock(Instant::now())),
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
