//! Sorting source callbacks on the actual native host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::{Scope, ScopeShared},
        value::{Js, Thrown, arg},
    },
    native::native_runtime,
};
use std::{cell::Cell, future::Future, rc::Rc, sync::Arc};
fn run<F: Future<Output = ()>>(body: impl FnOnce(Scope<()>) -> F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        safe_cex_exchange::js_runtime::library::install_library(
            &shared.globals,
            Rc::downgrade(&shared),
        );
        body(Scope {
            shared,
            this: Js::Undefined,
        })
        .await;
    });
}

fn input() -> Vec<Js> {
    [
        f64::NAN,
        558.,
        f64::NAN,
        197.,
        572.,
        f64::NAN,
        f64::NAN,
        48.,
        245.,
        415.,
        459.,
        f64::NAN,
        665.,
        f64::NAN,
        625.,
        832.,
        557.,
        440.,
        367.,
        613.,
        f64::NAN,
        f64::NAN,
    ]
    .into_iter()
    .map(Js::Number)
    .collect()
}
fn bits(values: &Js) -> Vec<u64> {
    let mut bits = values
        .array_items()
        .unwrap()
        .into_iter()
        .flatten()
        .map(|v| v.as_f64().unwrap().to_bits())
        .collect::<Vec<_>>();
    bits.sort();
    bits
}
#[test]
fn native_array_sort_accepts_nan_comparisons_without_panicking() {
    run(|_| async {
        let array = Js::array(input());
        let original = bits(&array);
        let compare = Js::function("descending price", |_, a| {
            Ok(Js::Number(
                arg(&a, 1).to_number()? - arg(&a, 0).to_number()?,
            ))
        });
        let result = array
            .call_method("sort", vec![compare])
            .expect("JS permits NaN comparator results");
        assert!(result.same_reference(&array));
        assert_eq!(bits(&array), original);
    });
}
#[test]
fn native_order_by_handles_repeated_nonreflexive_criteria_without_panicking() {
    run(|scope| async move {
        let order_by = scope.global("lodash").unwrap().get("orderBy").unwrap();
        for len in [22, 64, 256] {
            let values = Js::array(
                (0..len)
                    .map(|i| {
                        if i % 3 == 0 {
                            Js::Number(f64::NAN)
                        } else {
                            Js::Number((i * 17 % 41) as f64)
                        }
                    })
                    .collect(),
            );
            let original = bits(&values);
            let result = order_by
                .call(Js::Undefined, vec![values])
                .expect("source orderBy accepts repeated NaN");
            assert_eq!(bits(&result), original);
        }
    });
}
#[test]
fn native_sort_preserves_stability_holes_utf16_and_first_throw() {
    run(|_| async {
        let equal = Js::array(vec![Js::Number(3.), Js::Number(1.), Js::Number(2.)]);
        equal
            .call_method(
                "sort",
                vec![Js::function("NaN", |_, _| Ok(Js::Number(f64::NAN)))],
            )
            .unwrap();
        assert_eq!(equal.get("0").unwrap().as_f64(), Some(3.));
        let marker = Js::object(vec![("cause", Js::str("comparator"))]);
        let thrown = marker.clone();
        let error = equal
            .call_method(
                "sort",
                vec![Js::function("throw", move |_, _| {
                    Err(Thrown::new(thrown.clone()))
                })],
            )
            .unwrap_err();
        assert!(error.value.same_reference(&marker));
        let array = Js::array(vec![
            Js::utf16(&[0xe000]),
            Js::Undefined,
            Js::utf16(&[0xd800]),
        ]);
        let Js::Array(cells) = &array else {
            unreachable!()
        };
        cells.borrow_mut().push(None);
        array.call_method("sort", vec![]).unwrap();
        assert_eq!(&*array.get("0").unwrap().to_js_string().unwrap(), &[0xd800]);
        assert!(matches!(cells.borrow()[2], Some(Js::Undefined)));
        assert!(cells.borrow()[3].is_none());
        let error = Js::array(vec![])
            .call_method("sort", vec![Js::Null])
            .unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    });
}

#[test]
fn native_order_by_stops_on_the_first_comparison_throw() {
    run(|scope| async move {
        let count = Rc::new(Cell::new(0));
        let marker = Js::object(vec![("cause", Js::str("criteria"))]);
        let values = Js::array(
            (0..8)
                .map(|_| {
                    let count = count.clone();
                    let marker = marker.clone();
                    Js::object(vec![(
                        "valueOf",
                        Js::function("throw", move |_, _| {
                            count.set(count.get() + 1);
                            Err(Thrown::new(marker.clone()))
                        }),
                    )])
                })
                .collect(),
        );
        let order_by = scope.global("lodash").unwrap().get("orderBy").unwrap();
        let error = order_by.call(Js::Undefined, vec![values]).unwrap_err();
        assert!(error.value.same_reference(&marker));
        assert_eq!(
            count.get(),
            1,
            "orderBy must stop comparing after the first source throw"
        );
    });
}
#[test]
fn native_sort_rejects_a_present_noncallable_comparator() {
    run(|_| async {
        let error = Js::array(vec![])
            .call_method("sort", vec![Js::Null])
            .unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    });
}

#[test]
fn native_consistent_sort_matches_node_across_merge_boundaries() {
    let output=std::process::Command::new("node").args(["-e",r#"
console.log(JSON.stringify([0,1,2,3,4,7,8,9,21,22,31,32,33,63,64,65,129].map(n=>Array.from({length:n},(_,id)=>({id,key:(id*17)%11})).sort((a,b)=>a.key-b.key).map(x=>x.id))));
"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Vec<Vec<usize>> = serde_json::from_slice(&output.stdout).unwrap();
    run(|_| async move {
        for ids in expected {
            let values = Js::array(
                (0..ids.len())
                    .map(|id| {
                        Js::object(vec![
                            ("id", Js::Number(id as f64)),
                            ("key", Js::Number((id * 17 % 11) as f64)),
                        ])
                    })
                    .collect(),
            );
            values
                .call_method(
                    "sort",
                    vec![Js::function("ascending", |_, a| {
                        Ok(Js::Number(
                            arg(&a, 0).get("key")?.to_number()?
                                - arg(&a, 1).get("key")?.to_number()?,
                        ))
                    })],
                )
                .unwrap();
            let actual: Vec<usize> = values
                .array_items()
                .unwrap()
                .into_iter()
                .flatten()
                .map(|v| v.get("id").unwrap().as_f64().unwrap() as usize)
                .collect();
            assert_eq!(actual, ids, "stable sort at length {}", ids.len());
        }
    });
}

#[test]
fn native_inconsistent_sort_matches_node_comparison_schedule() {
    let output = std::process::Command::new("node").args(["-e", r#"
const lengths = [0,1,2,3,7,8,16,22,31,32,33,63,64,65,95,96,127,128,129,255,256,257,511,512,513,1024,2048];
const results = [];
for (let i = 0; i < 4096; ++i) {
  const seed = Math.imul(i+1, 2654435761) >>> 0;
  const n = lengths[i % lengths.length], mode = i % 6, pattern = Math.floor(i/6) % 5;
  let input = seed;
  const rows = Array.from({length:n}, (_, id) => {
    input = (Math.imul(input, 1664525) + 1013904223) >>> 0;
    const key = pattern === 0 ? (id % 7 === 0 ? NaN : input % 97)
      : pattern === 1 ? Math.floor((n-id)/7)
      : pattern === 2 ? (id*37) % n
      : pattern === 3 ? id % 32 + Math.floor((n-id)/32)*32
      : [NaN, Infinity, -Infinity, -0, 0, 1][id % 6];
    return {id, key};
  });
  let state = (seed ^ 0x9e3779b9) >>> 0, calls = 0, trace = 2166136261;
  rows.sort((a,b) => {
    ++calls;
    trace = Math.imul(trace ^ a.id, 16777619) >>> 0;
    trace = Math.imul(trace ^ b.id, 16777619) >>> 0;
    if (mode === 1) {
      state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
      return [-1,0,1,NaN][state >>> 30];
    }
    if (mode === 2) return -1;
    if (mode === 3) return [0,-1,1][(a.id%3 + 3 - b.id%3)%3];
    if (mode === 4 && a.key === b.key) return 1;
    if (mode === 5 && (a.id+b.id)%5 === 0) return NaN;
    return a.key-b.key;
  });
  results.push({seed,n,mode,pattern,ids:rows.map(x=>x.id),calls,trace,state});
}
console.log(JSON.stringify({engine:process.versions.v8,results}));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = result["results"].as_array().unwrap();
    assert_eq!(cases.len(), 4096);
    for (case_index, case) in cases.iter().enumerate() {
        let seed = case["seed"].as_u64().unwrap() as u32;
        let n = case["n"].as_u64().unwrap() as usize;
        let mode = case["mode"].as_u64().unwrap();
        let pattern = case["pattern"].as_u64().unwrap();
        let mut input = seed;
        let mut rows: Vec<_> = (0..n)
            .map(|id| {
                input = input.wrapping_mul(1664525).wrapping_add(1013904223);
                let key = match pattern {
                    0 if id % 7 == 0 => f64::NAN,
                    0 => (input % 97) as f64,
                    1 => ((n - id) / 7) as f64,
                    2 => ((id * 37) % n) as f64,
                    3 => (id % 32 + ((n - id) / 32) * 32) as f64,
                    _ => [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0., 0., 1.][id % 6],
                };
                (id, key)
            })
            .collect();
        let mut state = seed ^ 0x9e37_79b9;
        let mut calls = 0u64;
        let mut trace = 2166136261u32;
        safe_cex_exchange::js_runtime::sorting::stable_sort(&mut rows, |a, b| {
            calls += 1;
            trace = (trace ^ a.0 as u32).wrapping_mul(16777619);
            trace = (trace ^ b.0 as u32).wrapping_mul(16777619);
            let order = match mode {
                1 => {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    [-1., 0., 1., f64::NAN][(state >> 30) as usize]
                }
                2 => -1.,
                3 => [0., -1., 1.][(a.0 % 3 + 3 - b.0 % 3) % 3],
                4 if a.1 == b.1 => 1.,
                5 if (a.0 + b.0) % 5 == 0 => f64::NAN,
                _ => a.1 - b.1,
            };
            Ok(order.partial_cmp(&0.).unwrap_or(std::cmp::Ordering::Equal))
        })
        .unwrap();
        let observed = serde_json::json!({"ids": rows.iter().map(|row| row.0).collect::<Vec<_>>(), "calls":calls,"trace":trace,"state":state});
        let expected = serde_json::json!({"ids":case["ids"],"calls":case["calls"],"trace":case["trace"],"state":case["state"]});
        assert_eq!(
            observed, expected,
            "V8 {} comparator case {case_index}, n={n}, mode={mode}, pattern={pattern}",
            result["engine"]
        );
    }
}
