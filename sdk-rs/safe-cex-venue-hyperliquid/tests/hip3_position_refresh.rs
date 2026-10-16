#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/hip3_position_refresh_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    hip3_position_refresh::*, spot_account_reads::SpotAccountHost, spot_json::JsString,
    ws_ping_lifecycle::WsPingHost,
};
use std::{cell::RefCell, collections::HashMap, process::Command, rc::Rc};
use support::{Host, State};
const CASES: [(&str, &str); 22] = [
    ("refresh", "empty"),
    ("refresh", "ordinary"),
    ("refresh", "reject-one"),
    ("refresh", "log-error"),
    ("refresh", "sync-error"),
    ("refresh", "disposed"),
    ("refresh", "utf16"),
    ("schedule", "empty"),
    ("schedule", "disposed"),
    ("schedule", "ordinary"),
    ("schedule", "preexisting"),
    ("schedule", "zero-timer"),
    ("schedule", "fresh-timer"),
    ("schedule", "falsy-members"),
    ("schedule", "duplicate-pending"),
    ("schedule", "fire"),
    ("schedule", "fire-disposed"),
    ("schedule", "fire-empty"),
    ("schedule", "replacement"),
    ("schedule", "add-error"),
    ("schedule", "next-error"),
    ("schedule", "set-error"),
];
fn set(values: Vec<N>) -> N {
    N::Set(Rc::new(RefCell::new(values)))
}
fn members(value: &N) -> Vec<N> {
    let N::Set(v) = value else {
        panic!("native Set discriminant")
    };
    v.borrow().clone()
}
fn utf16() -> N {
    N::S(JsString(vec![100, 101, 120, 0xd800]))
}
fn drive(host: &mut Host, value: N) -> Result<N, N> {
    use std::task::{Context, Poll, Waker};
    let mut f = host.await_value(value);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        host.base.drain()
    }
    panic!("native finite promise drive")
}
#[test]
fn hip3_refresh_native_before_source() {
    let mut native_rows = vec![];
    for (family, scene) in CASES {
        let base = account::Host::new(
            object(vec![]),
            object(vec![(
                "LogSeverity",
                object(vec![("Warning", s("Warning"))]),
            )]),
            object(vec![]),
        );
        let pending = set(if scene == "duplicate-pending" {
            vec![s("dex-a")]
        } else {
            vec![]
        });
        let marker = object(vec![
            ("name", s("Error")),
            ("message", N::S(JsString(vec![111, 111, 112, 115, 0xd800]))),
            ("response", object(vec![("code", N::Num(55.))])),
        ]);
        if let N::O(o) = &marker {
            for (k, _, enumerable) in &mut o.borrow_mut().fields {
                if k == "name" || k == "message" {
                    *enumerable = false
                }
            }
        }
        let st = Rc::new(RefCell::new(State {
            scene: scene.into(),
            effects: vec![],
            callbacks: HashMap::new(),
            promises: vec![],
            timer: N::U,
            snapshot: N::U,
            pending: pending.clone(),
            marker: marker.clone(),
            closes: 0,
            timer_reads: 0,
        }));
        let mut h = Host { base, st };
        let e = h.e();
        let old = if scene == "preexisting" || scene == "fresh-timer" {
            N::Num(71.)
        } else if scene == "zero-timer" {
            N::Num(0.)
        } else {
            N::Null
        };
        for (k, v) in [
            ("_pendingHip3PositionRefreshDexes", pending.clone()),
            ("_hip3PositionRefreshDebounceTimer", old.clone()),
            ("HIP3_POSITION_REFRESH_DEBOUNCE_MS", N::Num(37.)),
            ("_lastPositionRefreshTime", N::Num(17.)),
            (
                "fetchHip3Positions",
                N::Builtin("fetchHip3Positions".into()),
            ),
            (
                "refreshHip3PositionsAsync",
                N::Builtin("refreshHip3PositionsAsync".into()),
            ),
            ("isDisposed", N::B(scene == "disposed")),
        ] {
            h.set(&e, k, &v).unwrap()
        }
        let input = set(if scene == "empty" {
            vec![]
        } else if scene == "falsy-members" {
            vec![s(""), N::Num(0.), N::Null, s("dex-a"), s("dex-b")]
        } else if scene == "utf16" {
            vec![utf16(), s("dex-b")]
        } else {
            vec![s("dex-a"), s("dex-b")]
        });
        let original = members(&input);
        let result = if family == "refresh" {
            refresh_hip3_positions_async(&mut h, &e, &input)
        } else {
            schedule_hip3_position_refresh(&mut h, &e, &input)
        };
        let failed = matches!(
            scene,
            "sync-error" | "add-error" | "next-error" | "set-error"
        );
        assert_eq!(
            result.is_err(),
            failed,
            "native error branch {family}/{scene}"
        );
        if failed {
            assert!(
                eq(result.as_ref().unwrap_err(), &marker),
                "native original rejection handle {scene}"
            );
            assert!(
                eq(&get(&get(&marker, "response"), "code"), &N::Num(55.)),
                "native error response field"
            )
        } else {
            assert!(matches!(result, Ok(N::U)), "native void return {scene}")
        }
        assert_eq!(
            members(&input).len(),
            original.len(),
            "native source input Set preserved {scene}"
        );
        for (a, b) in members(&input).iter().zip(&original) {
            assert!(eq(a, b), "native original Set member {scene}")
        }
        if scene == "replacement" {
            schedule_hip3_position_refresh(&mut h, &e, &set(vec![s("dex-c")])).unwrap();
        }
        if matches!(
            scene,
            "fire" | "fire-disposed" | "fire-empty" | "replacement"
        ) {
            if scene == "fire-disposed" {
                h.set(&e, "isDisposed", &N::B(true)).unwrap()
            }
            if scene == "fire-empty" {
                let N::Set(v) = &pending else { unreachable!() };
                v.borrow_mut().clear()
            }
            let callback = h.st.borrow().timer.clone();
            let callback_result = h
                .call(&callback, &object(vec![("isDisposed", N::B(true))]), &[])
                .unwrap();
            assert!(
                matches!(callback_result, N::U),
                "native timer discards refresh return {scene}"
            );
            assert!(
                matches!(get(&e, "_hip3PositionRefreshDebounceTimer"), N::Null),
                "native fired timer resets to null {scene}"
            );
            if scene == "fire" || scene == "replacement" {
                let snapshot = h.st.borrow().snapshot.clone();
                assert!(
                    !eq(&snapshot, &pending),
                    "native detached pending snapshot {scene}"
                );
                let expected = if scene == "replacement" {
                    vec![s("dex-a"), s("dex-b"), s("dex-c")]
                } else {
                    vec![s("dex-a"), s("dex-b")]
                };
                let got = members(&snapshot);
                assert_eq!(
                    got.len(),
                    expected.len(),
                    "native snapshot independent member floor"
                );
                for (a, b) in got.iter().zip(expected.iter()) {
                    assert!(eq(a, b), "native snapshot ordered member {scene}")
                }
                assert!(
                    members(&pending).is_empty(),
                    "native cleared original pending {scene}"
                )
            } else {
                assert!(
                    matches!(h.st.borrow().snapshot, N::U),
                    "native disposed/empty timer skips refresh {scene}"
                )
            }
        }
        h.base.drain();
        let effects = h.st.borrow().effects.clone();
        let names: Vec<String> = effects
            .iter()
            .map(|v| {
                let N::S(v) = get(v, "0") else {
                    panic!("effect")
                };
                String::from_utf16_lossy(&v.0)
            })
            .collect();
        let expected: Vec<&str> = if family == "refresh" {
            match scene {
                "empty" => vec![],
                "sync-error" => vec!["fetch"],
                "reject-one" | "log-error" => vec!["fetch", "fetch", "log"],
                _ => vec!["fetch", "fetch"],
            }
        } else {
            match scene {
                "empty" | "disposed" | "add-error" | "next-error" => vec![],
                "preexisting" | "fresh-timer" => vec!["cancel", "timer"],
                "fire" => vec!["timer", "clear", "refresh"],
                "replacement" => vec!["timer", "cancel", "timer", "clear", "refresh"],
                _ => vec!["timer"],
            }
        };
        let expected = if scene == "add-error" {
            vec!["close"]
        } else {
            expected
        };
        assert_eq!(
            names, expected,
            "native independent effect sequence {family}/{scene}"
        );
        if family == "refresh" {
            assert!(
                eq(
                    &get(&e, "_lastPositionRefreshTime"),
                    &N::Num(if scene == "empty" { 17. } else { 100000. })
                ),
                "native refresh timestamp {scene}"
            );
            let fetches: Vec<_> = effects
                .iter()
                .filter(|v| eq(&get(v, "0"), &s("fetch")))
                .collect();
            for (i, v) in fetches.iter().enumerate() {
                assert!(
                    eq(
                        &get(v, "1"),
                        &if i == 0 && scene == "utf16" {
                            utf16()
                        } else if i == 0 {
                            s("dex-a")
                        } else {
                            s("dex-b")
                        }
                    ),
                    "native exact original fetch dex {scene}"
                );
                let N::A(a) = v else { unreachable!() };
                assert_eq!(a.borrow().len(), 2, "native fetch arity")
            }
            if scene == "reject-one" || scene == "log-error" {
                let log = effects.last().unwrap();
                let mut expected = "Failed to refresh HIP-3 positions for dex-b: Error: oops"
                    .encode_utf16()
                    .collect::<Vec<_>>();
                expected.push(0xd800);
                assert!(
                    eq(&get(log, "1"), &N::S(JsString(expected))),
                    "native full warning UTF16 {scene}"
                );
                assert!(
                    eq(&get(log, "2"), &s("Warning")),
                    "native warning severity {scene}"
                );
                let N::A(a) = log else { unreachable!() };
                assert_eq!(a.borrow().len(), 3, "native warning arity");
                let final_promise = h.st.borrow().promises.last().cloned().unwrap();
                let settled = drive(&mut h, final_promise).unwrap();
                if scene == "log-error" {
                    assert!(
                        matches!(settled, N::U),
                        "native aggregate rejection handled {scene}"
                    )
                } else {
                    let N::A(a) = settled else {
                        panic!("native aggregate fulfilled array")
                    };
                    assert_eq!(
                        a.borrow().len(),
                        2,
                        "native independent aggregate member floor"
                    );
                    for item in a.borrow().iter() {
                        assert!(
                            matches!(item, N::U),
                            "native child catch discards opaque logger return"
                        )
                    }
                }
            }
        } else {
            assert_eq!(
                h.st.borrow().closes,
                usize::from(scene == "add-error"),
                "native close only body throw {scene}"
            );
            assert!(
                eq(&get(&e, "_lastPositionRefreshTime"), &N::Num(17.)),
                "native scheduling preserves refresh timestamp {scene}"
            );
            let expected: Vec<N> = match scene {
                "empty" | "disposed" | "add-error" | "fire" | "replacement" | "fire-empty" => {
                    vec![]
                }
                "next-error" => vec![s("dex-a")],
                _ => vec![s("dex-a"), s("dex-b")],
            };
            let got = members(&pending);
            assert_eq!(
                got.len(),
                expected.len(),
                "native pending independent exact floor {scene}"
            );
            for (a, b) in got.iter().zip(&expected) {
                assert!(eq(a, b), "native pending ordered member {scene}")
            }
            let expected_timer = if matches!(
                scene,
                "empty"
                    | "disposed"
                    | "add-error"
                    | "next-error"
                    | "set-error"
                    | "fire"
                    | "fire-disposed"
                    | "fire-empty"
                    | "replacement"
            ) {
                N::Null
            } else {
                N::Num(99.)
            };
            assert!(
                eq(
                    &get(&e, "_hip3PositionRefreshDebounceTimer"),
                    &expected_timer
                ),
                "native exact stored timer {scene}"
            );
            for effect in &effects {
                if eq(&get(effect, "0"), &s("timer")) {
                    assert!(
                        eq(&get(effect, "1"), &N::Num(37.)),
                        "native debounce interval {scene}"
                    )
                }
            }
            if scene == "fresh-timer" {
                assert!(
                    eq(&get(&effects[0], "1"), &N::Num(72.)),
                    "native repeated timer getter value"
                )
            }
            if scene == "zero-timer" {
                assert_eq!(names, vec!["timer"], "native zero handle not cancelled")
            }
            if scene == "set-error" {
                assert!(
                    matches!(get(&e, "_hip3PositionRefreshDebounceTimer"), N::Null),
                    "native registration throw preserves old handle"
                )
            }
        }
        let snapshot = if matches!(h.st.borrow().snapshot, N::U) {
            N::U
        } else {
            arr(members(&h.st.borrow().snapshot))
        };
        native_rows.push((
            family,
            scene,
            object(vec![
                ("effects", arr(effects)),
                ("last", get(&e, "_lastPositionRefreshTime")),
                ("pending", arr(members(&pending))),
                ("timer", get(&e, "_hip3PositionRefreshDebounceTimer")),
                ("snapshot", snapshot),
                ("failed", N::B(failed)),
            ]),
        ));
    }
    assert_eq!(native_rows.len(), 22, "native completed HIP3 refresh floor");
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/hip3-position-refresh-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = value["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 22, "independent source case floor");
    assert_eq!(value["last_index"], 21);
    for ((family, scene, native), row) in native_rows.iter().zip(rows) {
        assert_eq!(row["family"], *family);
        assert_eq!(row["scene"], *scene);
        native::assert_value(
            native,
            &row["state"],
            &format!("supplemental source {family}/{scene}"),
        )
    }
}
