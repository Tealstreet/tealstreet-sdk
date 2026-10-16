use safe_cex_venue_paradex::state_operations::{
    fetch_historical_positions, get_paradex_sdk, is_public_data_only,
    reset_sdk_state_after_init_failure,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, ready},
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug)]
struct Marker(&'static str);
type Host = Rc<Marker>;
struct Held {
    ready: Rc<Cell<bool>>,
    result: Option<Result<Host, Host>>,
}
impl Future for Held {
    type Output = Result<Host, Host>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.get() {
            Poll::Ready(self.result.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
fn units(text: &str) -> Vec<u16> {
    let mut value: Vec<u16> = text.encode_utf16().collect();
    value.push(0xd800);
    value
}
fn outcome(actual: &Result<Host, Host>, source: &Value, modules: &[Host], error: &Host) {
    match actual {
        Ok(value) => {
            assert_eq!(source["ok"].as_bool(), Some(true), "cache_native_success");
            assert_eq!(source["id"].as_str(), Some(value.0), "cache_native_value");
            let original = modules.iter().find(|module| module.0 == value.0).unwrap();
            assert!(Rc::ptr_eq(value, original), "cache_native_identity");
            assert_eq!(source["same"].as_bool(), Some(true));
        }
        Err(value) => {
            assert_eq!(source["ok"].as_bool(), Some(false), "cache_native_error");
            assert!(Rc::ptr_eq(value, error), "cache_native_error_identity");
            assert_eq!(source["same"].as_bool(), Some(true));
        }
    }
}
fn check_cache(cache: &RefCell<Option<Host>>, source: &Value, modules: &[Host]) {
    let value = cache.borrow();
    match value.as_ref() {
        Some(value) => {
            assert_eq!(source.as_str(), Some(value.0), "cache_native_slot");
            let original = modules.iter().find(|module| module.0 == value.0).unwrap();
            assert!(Rc::ptr_eq(value, original), "cache_native_slot_identity");
        }
        None => assert!(source.is_null(), "cache_native_empty"),
    }
}

#[test]
fn actual_typescript_state_operations() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/state_operations_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "state_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["publicCases"].as_array().unwrap();
    assert_eq!(cases.len(), 120, "public_source_count");
    let mut completed = 0;
    for flag in ["absent", "false", "true"] {
        for mask in 0..8 {
            for failure in [-1, 0, 1, 2, 3] {
                let row = &cases[completed];
                assert_eq!(row["flag"], flag, "public_live_flag");
                assert_eq!(row["mask"], mask, "public_live_mask");
                assert_eq!(row["failure"], failure, "public_live_failure");
                let credentials: Vec<Vec<u16>> = (0..3)
                    .map(|i| {
                        if mask & (1 << i) != 0 {
                            units(&format!("credential-{i}-"))
                        } else {
                            vec![]
                        }
                    })
                    .collect();
                assert_eq!(
                    row["credentials"],
                    json!(credentials),
                    "public_live_credentials"
                );
                let error = Rc::new(Marker("error"));
                let trace = RefCell::new(Vec::<i32>::new());
                let read = |index| {
                    trace.borrow_mut().push(index);
                    if failure == index {
                        Err(error.clone())
                    } else {
                        Ok(if index == 0 {
                            flag == "true"
                        } else {
                            !credentials[(index - 1) as usize].is_empty()
                        })
                    }
                };
                let result = is_public_data_only(|| read(0), || read(1), || read(2), || read(3));
                match result {
                    Ok(value) => {
                        assert_eq!(
                            row["outcome"]["ok"].as_bool(),
                            Some(true),
                            "public_native_success"
                        );
                        assert_eq!(
                            row["outcome"]["value"].as_bool(),
                            Some(value),
                            "public_native_value"
                        );
                    }
                    Err(value) => {
                        assert_eq!(
                            row["outcome"]["ok"].as_bool(),
                            Some(false),
                            "public_native_error"
                        );
                        assert!(Rc::ptr_eq(&value, &error), "public_native_error_identity");
                        assert_eq!(row["outcome"]["same"].as_bool(), Some(true));
                    }
                }
                let expected: Vec<i32> = row["trace"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_i64().unwrap() as i32)
                    .collect();
                assert_eq!(*trace.borrow(), expected, "public_native_trace");
                completed += 1;
            }
        }
    }
    assert_eq!(completed, 120, "public_completed_cases");
    let cases = source["resets"].as_array().unwrap();
    assert_eq!(cases.len(), 80, "reset_source_count");
    completed = 0;
    for mask in 0..16 {
        for failure in [-1, 0, 1, 2, 3] {
            let row = &cases[completed];
            assert_eq!(row["mask"], mask, "reset_live_mask");
            assert_eq!(row["failure"], failure, "reset_live_failure");
            let leases: Vec<Host> = ["init", "account", "signer", "jwt"]
                .into_iter()
                .map(|name| Rc::new(Marker(name)))
                .collect();
            let slots: RefCell<Vec<Option<Host>>> = RefCell::new(
                leases
                    .iter()
                    .enumerate()
                    .map(|(i, value)| {
                        if mask & (1 << i) != 0 {
                            Some(value.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
            );
            let trace = RefCell::new(Vec::new());
            let error = Rc::new(Marker("error"));
            let write = |index: usize, value: Option<Host>| {
                assert!(value.is_none(), "reset_native_null_write");
                trace.borrow_mut().push(index);
                if failure == index as i32 {
                    Err(error.clone())
                } else {
                    slots.borrow_mut()[index] = value;
                    Ok(())
                }
            };
            let result = reset_sdk_state_after_init_failure(
                |v| write(0, v),
                |v| write(1, v),
                |v| write(2, v),
                |v| write(3, v),
            );
            for (i, value) in slots.borrow().iter().enumerate() {
                match value {
                    Some(value) => {
                        assert_eq!(row["slots"][i], "original", "reset_native_slot");
                        assert!(Rc::ptr_eq(value, &leases[i]), "reset_native_slot_identity");
                    }
                    None => assert_eq!(row["slots"][i], "null", "reset_native_slot"),
                }
                assert_eq!(
                    leases[i].0,
                    ["init", "account", "signer", "jwt"][i],
                    "reset_native_live_host_lease"
                );
            }
            match result {
                Ok(()) => {
                    assert_eq!(
                        row["outcome"]["ok"].as_bool(),
                        Some(true),
                        "reset_native_success"
                    );
                    assert_eq!(row["outcome"]["undefined"].as_bool(), Some(true));
                }
                Err(value) => {
                    assert_eq!(
                        row["outcome"]["ok"].as_bool(),
                        Some(false),
                        "reset_native_error"
                    );
                    assert!(Rc::ptr_eq(&value, &error), "reset_native_error_identity");
                    assert_eq!(row["outcome"]["same"].as_bool(), Some(true));
                }
            }
            let expected: Vec<usize> = row["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    assert_eq!(entry[1].as_bool(), Some(true));
                    entry[0].as_u64().unwrap() as usize
                })
                .collect();
            assert_eq!(*trace.borrow(), expected, "reset_native_trace");
            completed += 1;
        }
    }
    assert_eq!(completed, 80, "reset_completed_cases");
    let cases = source["histories"].as_array().unwrap();
    assert_eq!(cases.len(), 3, "history_source_count");
    let mut context = Context::from_waker(Waker::noop());
    for (i, mode) in ["ok", "sync-error", "async-error"].into_iter().enumerate() {
        let row = &cases[i];
        assert_eq!(row["mode"], mode, "history_live_mode");
        let opts = Rc::new((units("account-"), -0.0_f64, f64::INFINITY));
        assert_eq!(row["opts"]["account"], json!(opts.0), "history_live_opts");
        assert!(row["opts"]["fromNegativeZero"].as_bool().unwrap());
        assert!(row["opts"]["toInfinity"].as_bool().unwrap());
        let value = Rc::new(Marker("records"));
        let error = Rc::new(Marker("error"));
        let ready = Rc::new(Cell::new(false));
        let calls = Cell::new(0);
        let mut pending = Box::pin(fetch_historical_positions(opts.clone(), |given, scoped| {
            calls.set(calls.get() + 1);
            assert!(Rc::ptr_eq(&given, &opts), "history_native_opts_identity");
            assert_eq!(
                given.1.to_bits(),
                (-0.0_f64).to_bits(),
                "history_native_from"
            );
            assert_eq!(
                given.2.to_bits(),
                f64::INFINITY.to_bits(),
                "history_native_to"
            );
            assert!(scoped, "history_native_symbol_scoped");
            if mode == "sync-error" {
                Err(error.clone())
            } else {
                Ok(Held {
                    ready: ready.clone(),
                    result: Some(if mode == "async-error" {
                        Err(error.clone())
                    } else {
                        Ok(value.clone())
                    }),
                })
            }
        }));
        let mut polled = pending.as_mut().poll(&mut context);
        assert_eq!(
            row["before"]["settled"].as_bool(),
            Some(polled.is_ready()),
            "history_native_before"
        );
        assert_eq!(calls.get(), 1, "history_native_call_count");
        if polled.is_pending() {
            ready.set(true);
            polled = pending.as_mut().poll(&mut context);
        }
        let Poll::Ready(result) = polled else {
            panic!("history_native_settlement")
        };
        match result {
            Ok(actual) => {
                assert_eq!(
                    row["outcome"]["ok"].as_bool(),
                    Some(true),
                    "history_native_success"
                );
                assert!(
                    Rc::ptr_eq(&actual, &value),
                    "history_native_output_identity"
                );
            }
            Err(actual) => {
                assert_eq!(
                    row["outcome"]["ok"].as_bool(),
                    Some(false),
                    "history_native_error"
                );
                assert!(Rc::ptr_eq(&actual, &error), "history_native_error_identity");
            }
        }
        assert_eq!(row["outcome"]["same"].as_bool(), Some(true));
        assert_eq!(calls.get(), 1, "history_native_final_count");
        assert_eq!(
            row["trace"],
            json!([[true, true, ["symbolScoped"], true]]),
            "history_source_trace"
        );
    }
    let cases = source["caches"].as_array().unwrap();
    assert_eq!(cases.len(), 8, "cache_source_count");
    for (i, mode) in [
        "cached",
        "ok",
        "sync-error",
        "async-error",
        "race-ab",
        "race-ba",
        "race-a-error",
        "race-b-error",
    ]
    .into_iter()
    .enumerate()
    {
        let row = &cases[i];
        assert_eq!(row["mode"], mode, "cache_live_mode");
        let modules: Vec<Host> = ["cached", "a", "b", "warm"]
            .into_iter()
            .map(|name| Rc::new(Marker(name)))
            .collect();
        let cache = RefCell::new(if mode == "cached" {
            Some(modules[0].clone())
        } else {
            None
        });
        let error = Rc::new(Marker("error"));
        let a_ready = Rc::new(Cell::new(false));
        let b_ready = Rc::new(Cell::new(false));
        let loads = RefCell::new(Vec::new());
        let loader = |who: usize, held: &Rc<Cell<bool>>| {
            loads.borrow_mut().push(if who == 1 { "a" } else { "b" });
            if mode == "sync-error" {
                return Err(error.clone());
            }
            Ok(Held {
                ready: held.clone(),
                result: Some(
                    if (who == 1 && ["async-error", "race-a-error"].contains(&mode))
                        || (who == 2 && mode == "race-b-error")
                    {
                        Err(error.clone())
                    } else {
                        Ok(modules[who].clone())
                    },
                ),
            })
        };
        let mut first = Box::pin(get_paradex_sdk(&cache, || loader(1, &a_ready)));
        let mut second = Box::pin(get_paradex_sdk(&cache, || loader(2, &b_ready)));
        let mut a_result = first.as_mut().poll(&mut context);
        let mut b_result = if mode.starts_with("race-") {
            second.as_mut().poll(&mut context)
        } else {
            Poll::Pending
        };
        check_cache(&cache, &row["before"]["cache"], &modules);
        assert_eq!(
            row["before"]["settled"].as_u64(),
            Some(u64::from(a_result.is_ready()) + u64::from(b_result.is_ready())),
            "cache_native_before_settled"
        );
        assert_eq!(
            row["before"]["loads"],
            json!(*loads.borrow()),
            "cache_native_before_loads"
        );
        let order = if mode == "race-ba" {
            vec!["b", "a"]
        } else if mode.starts_with("race-") {
            vec!["a", "b"]
        } else {
            vec!["a"]
        };
        assert_eq!(
            row["after"].as_array().unwrap().len(),
            order.len(),
            "cache_independent_settlement_count"
        );
        for (j, who) in order.into_iter().enumerate() {
            let step = &row["after"][j];
            assert_eq!(step["who"], who, "cache_live_order");
            let actual = if who == "a" {
                if a_result.is_pending() {
                    a_ready.set(true);
                    a_result = first.as_mut().poll(&mut context);
                }
                &a_result
            } else {
                if b_result.is_pending() {
                    b_ready.set(true);
                    b_result = second.as_mut().poll(&mut context);
                }
                &b_result
            };
            let Poll::Ready(result) = actual else {
                panic!("cache_native_settlement")
            };
            outcome(result, &step["outcome"], &modules, &error);
            check_cache(&cache, &step["cache"], &modules);
        }
        let mut warm = Box::pin(get_paradex_sdk(&cache, || {
            loads.borrow_mut().push("warm");
            Ok::<_, Host>(ready(Ok(modules[3].clone())))
        }));
        let Poll::Ready(result) = warm.as_mut().poll(&mut context) else {
            panic!("cache_native_warm_settlement")
        };
        outcome(&result, &row["warm"], &modules, &error);
        check_cache(&cache, &row["cache"], &modules);
        assert_eq!(
            row["loads"],
            json!(*loads.borrow()),
            "cache_native_load_count"
        );
    }
}
