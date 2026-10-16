use safe_cex_exchange::runtime::ExchangeTask;
use safe_cex_venue_polymarket::disposal::{
    dispose_exchange, dispose_private_websocket, dispose_public_websocket,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};

fn oracle() -> &'static [Value] {
    static CAPTURE: OnceLock<Vec<Value>> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/disposal_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(cases.len(), 10, "independent disposal fixture floor");
        assert_eq!(cases[9]["venue"], "public");
        assert_eq!(
            cases[9]["scenario"], "cancel-fails",
            "last disposal fixture"
        );
        cases
    })
}

#[derive(Debug)]
struct ParentError {
    code: &'static str,
    marker: u64,
}
struct Timer {
    trace: Rc<RefCell<Vec<&'static str>>>,
    fail: bool,
}
impl ExchangeTask for Timer {
    fn cancel(&self) {
        self.trace.borrow_mut().push("cancel");
        if self.fail {
            panic!("cancel failure");
        }
    }
}

fn check_socket(venue: &str) {
    let cases: Vec<_> = oracle()
        .iter()
        .filter(|case| case["venue"] == venue)
        .collect();
    assert_eq!(cases.len(), 4, "independent per-socket disposal floor");
    for (index, case) in cases.into_iter().enumerate() {
        let scenario = ["empty", "live", "base-fails", "cancel-fails"][index];
        assert_eq!(
            case["scenario"], scenario,
            "independent disposal scenario order"
        );
        let trace = Rc::new(RefCell::new(Vec::new()));
        let interval: RefCell<Option<Box<dyn ExchangeTask>>> = RefCell::new(if index == 0 {
            None
        } else {
            Some(Box::new(Timer {
                trace: trace.clone(),
                fail: index == 3,
            }))
        });
        let assets: RefCell<HashSet<u64>> = RefCell::new(if index == 0 {
            HashSet::new()
        } else {
            HashSet::from([101, 102])
        });
        let callbacks: RefCell<BTreeMap<u64, Vec<u64>>> = RefCell::new(if index == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(101, vec![1, 2]), (102, vec![3])])
        });
        let failure = Rc::new(ParentError {
            code: "PARENT",
            marker: 73,
        });
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2, "independent repeated disposal floor");
        for (iteration, step) in steps.iter().enumerate() {
            let parent = || {
                trace.borrow_mut().push("base");
                assert!(
                    !interval.borrow().is_some(),
                    "Polymarket native timer cleared before parent: {venue}"
                );
                if venue == "public" {
                    assert_eq!(
                        assets.borrow().len(),
                        0,
                        "Polymarket native assets cleared before parent"
                    );
                    assert_eq!(
                        callbacks.borrow().len(),
                        0,
                        "Polymarket native callbacks cleared before parent"
                    );
                }
                if index == 2 {
                    Err(failure.clone())
                } else {
                    Ok(())
                }
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                if venue == "public" {
                    dispose_public_websocket(&interval, &assets, &callbacks, parent)
                } else {
                    dispose_private_websocket(&interval, parent)
                }
            }));
            let expected_trace: Vec<&str> = if index == 0 {
                vec!["base"; iteration + 1]
            } else if index == 3 {
                vec!["cancel"; iteration + 1]
            } else {
                std::iter::once("cancel")
                    .chain(std::iter::repeat_n("base", iteration + 1))
                    .collect()
            };
            assert_eq!(
                *trace.borrow(),
                expected_trace,
                "Polymarket native disposal call order: {venue} {scenario}"
            );
            assert_eq!(
                interval.borrow().is_some(),
                index == 3,
                "Polymarket native disposal timer retention"
            );
            let expected_collection_count = if index != 0 && (venue == "private" || index == 3) {
                2
            } else {
                0
            };
            assert_eq!(
                assets.borrow().len(),
                expected_collection_count,
                "Polymarket native disposal assets"
            );
            assert_eq!(
                callbacks.borrow().len(),
                expected_collection_count,
                "Polymarket native disposal callbacks"
            );
            if expected_collection_count == 2 {
                assert!(
                    assets.borrow().contains(&101) && assets.borrow().contains(&102),
                    "Polymarket native retained assets"
                );
                assert_eq!(
                    callbacks.borrow()[&101],
                    vec![1, 2],
                    "Polymarket native retained first callbacks"
                );
                assert_eq!(
                    callbacks.borrow()[&102],
                    vec![3],
                    "Polymarket native retained second callbacks"
                );
            }
            match result {
                Ok(Ok(())) => assert!(index < 2, "Polymarket native disposal success"),
                Ok(Err(error)) => {
                    assert_eq!(index, 2, "Polymarket native parent failure scenario");
                    assert!(
                        Rc::ptr_eq(&error, &failure),
                        "Polymarket native parent error identity"
                    );
                    assert_eq!(error.code, "PARENT");
                    assert_eq!(error.marker, 73);
                }
                Err(error) => {
                    assert_eq!(index, 3, "Polymarket native cancellation failure scenario");
                    assert_eq!(
                        error.downcast_ref::<&str>().copied(),
                        Some("cancel failure"),
                        "Polymarket native cancellation error"
                    );
                }
            }
            // Source observations follow all native state/error assertions.
            let source_trace: Vec<_> = step["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(
                source_trace, expected_trace,
                "independent source disposal trace"
            );
            assert_eq!(step["timerPresent"], index == 3, "source timer retention");
            assert_eq!(
                step["assets"].as_array().unwrap().len(),
                expected_collection_count,
                "independent source assets floor"
            );
            assert_eq!(
                step["callbacks"].as_array().unwrap().len(),
                expected_collection_count,
                "independent source callback floor"
            );
            if index == 2 {
                assert_eq!(step["error"]["code"], "PARENT");
                assert_eq!(step["error"]["marker"], 73);
            }
        }
    }
}

#[test]
fn public_disposal_matches_source() {
    check_socket("public");
}
#[test]
fn private_disposal_matches_source() {
    check_socket("private");
}

#[test]
fn exchange_disposal_matches_source() {
    let cases: Vec<_> = oracle()
        .iter()
        .filter(|case| case["venue"] == "exchange")
        .collect();
    assert_eq!(cases.len(), 2, "independent exchange disposal floor");
    for (index, case) in cases.into_iter().enumerate() {
        assert_eq!(case["scenario"], ["base-ok", "base-fails"][index]);
        let calls = RefCell::new(0);
        let failure = Rc::new(ParentError {
            code: "PARENT",
            marker: 73,
        });
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2);
        for (iteration, step) in steps.iter().enumerate() {
            let result = dispose_exchange(|| {
                *calls.borrow_mut() += 1;
                if index == 1 {
                    Err(failure.clone())
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                *calls.borrow(),
                iteration + 1,
                "Polymarket native exchange parent dispose count"
            );
            match result {
                Ok(()) => assert_eq!(index, 0),
                Err(error) => {
                    assert_eq!(index, 1);
                    assert!(
                        Rc::ptr_eq(&error, &failure),
                        "Polymarket native exchange error identity"
                    );
                    assert_eq!(error.code, "PARENT");
                    assert_eq!(error.marker, 73);
                }
            }
            assert_eq!(
                step["trace"].as_array().unwrap(),
                &vec![Value::String("base".into()); iteration + 1],
                "independent exchange source trace"
            );
            assert_eq!(
                step["error"].is_null(),
                index == 0,
                "source exchange failure"
            );
        }
    }
}
