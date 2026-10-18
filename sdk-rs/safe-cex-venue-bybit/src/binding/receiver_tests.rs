use super::BybitDependencies;
use crate::{
    core_snapshot_readers::SnapshotHost, exchange::BybitExchange,
    hyro_private_session::SessionHost, variant_account_refresh::AccountRefreshHost,
};
use safe_cex_exchange::native_store::DefaultStore;
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
