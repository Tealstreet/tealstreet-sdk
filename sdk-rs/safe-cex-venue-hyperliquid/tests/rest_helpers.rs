#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/rest_host.rs"]
mod support;
use cache::{N, eq, object, s};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperObserver, income::*, json_boundaries::JsPayloadHost, market_startup::*,
};
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
use support::{Host, assert_value, decode};
fn oracle() -> &'static Vec<Value> {
    static DATA: OnceLock<Vec<Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/rest_helpers_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["sourceHashes"].as_object().unwrap().len(),
            5,
            "REST independent source file floor"
        );
        let rows = v["cases"].as_array().unwrap().clone();
        assert_eq!(rows.len(), 64, "REST independent case floor through63");
        rows
    })
}
fn complete(h: &Host, mut f: HelperObserver<N, N>) -> Result<N, N> {
    h.drain();
    let mut cx = Context::from_waker(Waker::noop());
    match Pin::new(&mut f).poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("REST observer unresolved"),
    }
}
fn execute(row: &Value) -> (Result<N, N>, Host) {
    let input = decode(&row["input"]);
    let mut h = Host::new(input.clone());
    let get = |h: &mut Host, k: &str| h.get(&input, k).unwrap();
    let resolve = N::Builtin("resolve".into());
    let family = row["family"].as_str().unwrap();
    let mut params = h.clone();
    let result = match family {
        "number" => to_number(&mut h, &get(&mut params, "value")).map(N::Num),
        "read" => read_funding_row(&mut h, &get(&mut params, "row")),
        "fill" => {
            map_hyperliquid_fill_to_income_records(&mut h, &get(&mut params, "row"), &resolve)
        }
        "funding" => {
            map_hyperliquid_funding_to_income_record(&mut h, &get(&mut params, "row"), &resolve)
        }
        "derive" => derive_hyperliquid_closed_positions(
            &mut h,
            &get(&mut params, "rows"),
            &resolve,
            &get(&mut params, "account"),
        ),
        "paginate" => {
            let start = get(&mut h, "start");
            let start = h.number(&start).unwrap();
            let max = get(&mut h, "max");
            let max = if matches!(max, N::U) {
                None
            } else {
                Some(h.number(&max).unwrap())
            };
            let end = get(&mut h, "end");
            let observer = paginate_hyperliquid_by_time(
                h.clone(),
                N::Builtin("post-direct".into()),
                s("userFills"),
                start,
                end,
                N::Builtin("key".into()),
                N::Builtin("cap".into()),
                max,
            );
            assert_value(
                &h.effects(),
                &row["prefix"],
                &format!("REST eager prefix {}", row["name"]),
            );
            let result = complete(&h, observer);
            if let Ok(N::A(result)) = &result {
                let pages = get(&mut h, "pages");
                let N::A(pages) = pages else { panic!("pages") };
                let originals = pages
                    .borrow()
                    .iter()
                    .flat_map(|v| {
                        let N::A(v) = v else { panic!("page") };
                        v.borrow().clone()
                    })
                    .collect::<Vec<_>>();
                for row in result.borrow().iter() {
                    assert!(
                        originals.iter().any(|v| eq(v, row)),
                        "REST original collected row identity"
                    );
                }
            }
            result
        }
        "core" | "hip3" => {
            let source = if family == "core" {
                object(vec![
                    (
                        "infoXhr",
                        object(vec![("post", N::Builtin("post-primary".into()))]),
                    ),
                    (
                        "xhr",
                        object(vec![("post", N::Builtin("post-fallback".into()))]),
                    ),
                    ("log", N::Builtin("log".into())),
                ])
            } else {
                object(vec![
                    ("xhr", object(vec![("post", N::Builtin("post-dex".into()))])),
                    ("dexName", get(&mut h, "dexName")),
                    ("log", N::Builtin("log".into())),
                ])
            };
            let observer = if family == "core" {
                fetch_core_meta_and_asset_ctxs_with_fallback(h.clone(), source)
            } else {
                fetch_hip3_meta_and_asset_ctxs_with_retry(h.clone(), source)
            };
            assert_value(
                &h.effects(),
                &row["prefix"],
                &format!("REST eager prefix {}", row["name"]),
            );
            let result = complete(&h, observer);
            if let Ok(N::A(tuple)) = &result {
                let plan = get(&mut h, "plan");
                let N::A(plan) = plan else { panic!("plan") };
                let mut found = false;
                for item in plan.borrow().iter() {
                    let data = h.get(item, "data").unwrap();
                    if let N::A(original) = &data {
                        let pair = original.borrow();
                        if pair.len() >= 2
                            && eq(&pair[0], &tuple.borrow()[0])
                            && eq(&pair[1], &tuple.borrow()[1])
                        {
                            assert!(
                                !eq(&data, result.as_ref().unwrap()),
                                "REST fresh parsed tuple identity"
                            );
                            found = true;
                        }
                    }
                }
                assert!(found, "REST original meta/context aliases");
            }
            result
        }
        _ => panic!("family"),
    };
    if matches!(family, "fill" | "funding" | "derive") {
        let expected = h.get(&input, "resolved").unwrap();
        if matches!(expected, N::O(_)) {
            if let Ok(value) = &result {
                let records = if let N::A(a) = value {
                    a.borrow().clone()
                } else {
                    vec![value.clone()]
                };
                for record in records {
                    let symbol = h.get(&record, "symbol").unwrap();
                    assert!(
                        eq(&symbol, &expected),
                        "REST original resolved-symbol identity"
                    );
                }
            }
        }
    }
    (result, h)
}
fn check(family: &str, count: usize) {
    let rows = oracle()
        .iter()
        .filter(|r| r["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count, "REST independent family floor");
    for row in rows {
        let label = format!("REST {} {}", family, row["name"]);
        let (result, h) = execute(row);
        assert_eq!(
            result.is_err(),
            row["rejected"].as_bool().unwrap(),
            "REST native outcome {label}"
        );
        match result {
            Ok(v) => {
                assert_value(&v, &row["result"], &label);
                assert_eq!(row["error"][0], "u", "REST expected success {label}")
            }
            Err(e) => assert_value(&e, &row["error"], &label),
        }
        assert_eq!(
            h.closes(),
            if row["name"] == "key-error" { 1 } else { 0 },
            "REST body-throw iterator close"
        );
        assert_value(
            &h.effects(),
            &row["effects"],
            &format!("REST effects {}", row["name"]),
        );
    }
}
#[test]
fn finite_number_prefix() {
    check("number", 12)
}
#[test]
fn nested_funding_rows() {
    check("read", 6)
}
#[test]
fn fill_income_signs() {
    check("fill", 8)
}
#[test]
fn funding_income_ids() {
    check("funding", 6)
}
#[test]
fn derived_close_records() {
    check("derive", 9)
}
#[test]
fn inclusive_history_pages() {
    check("paginate", 9)
}
#[test]
fn core_metadata_fallback() {
    check("core", 7)
}
#[test]
fn hip3_metadata_retry() {
    check("hip3", 7)
}
#[test]
fn dropped_observers_retain_work() {
    for family in ["paginate", "core", "hip3"] {
        let row = oracle()
            .iter()
            .find(|r| {
                r["family"] == family
                    && r["name"]
                        == if family == "paginate" {
                            "overlap"
                        } else if family == "core" {
                            "malformed-fallback"
                        } else {
                            "malformed-retry"
                        }
            })
            .unwrap();
        let input = decode(&row["input"]);
        let mut h = Host::new(input.clone());
        let observer = if family == "paginate" {
            paginate_hyperliquid_by_time(
                h.clone(),
                N::Builtin("post-direct".into()),
                s("userFills"),
                1000.,
                N::Num(2000.),
                N::Builtin("key".into()),
                N::Builtin("cap".into()),
                None,
            )
        } else {
            let source = if family == "core" {
                object(vec![
                    (
                        "infoXhr",
                        object(vec![("post", N::Builtin("post-primary".into()))]),
                    ),
                    (
                        "xhr",
                        object(vec![("post", N::Builtin("post-fallback".into()))]),
                    ),
                    ("log", N::Builtin("log".into())),
                ])
            } else {
                object(vec![
                    ("xhr", object(vec![("post", N::Builtin("post-dex".into()))])),
                    ("dexName", h.get(&input, "dexName").unwrap()),
                    ("log", N::Builtin("log".into())),
                ])
            };
            if family == "core" {
                fetch_core_meta_and_asset_ctxs_with_fallback(h.clone(), source)
            } else {
                fetch_hip3_meta_and_asset_ctxs_with_retry(h.clone(), source)
            }
        };
        assert_value(&h.effects(), &row["prefix"], "REST dropped eager prefix");
        drop(observer);
        h.drain();
        assert_value(
            &h.effects(),
            &row["effects"],
            "REST dropped observer continues",
        );
    }
}
