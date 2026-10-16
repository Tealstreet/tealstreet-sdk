use safe_cex_exchange::runtime::ExchangeTask;
use safe_cex_venue_polymarket::ws_timers::{
    clear_private_ping_interval, clear_public_ping_interval,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
    rc::Rc,
};

struct Timer {
    calls: Rc<RefCell<Vec<u64>>>,
    fail: bool,
}
impl ExchangeTask for Timer {
    fn cancel(&self) {
        self.calls.borrow_mut().push(17);
        if self.fail {
            panic!("host cancellation failed");
        }
    }
}

fn check(venue: &str, clear: fn(&mut Option<Box<dyn ExchangeTask>>)) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_timer_clear_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 6, "independent cleanup fixture floor");
    assert_eq!(cases[5]["venue"], "private");
    assert_eq!(
        cases[5]["scenario"], "cancel-throws",
        "last cleanup fixture"
    );
    let selected: Vec<_> = cases.iter().filter(|case| case["venue"] == venue).collect();
    assert_eq!(selected.len(), 3, "independent per-venue cleanup floor");
    for (index, case) in selected.into_iter().enumerate() {
        assert_eq!(
            case["scenario"],
            ["absent", "present", "cancel-throws"][index],
            "independent cleanup scenario order"
        );
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut interval: Option<Box<dyn ExchangeTask>> = if case["scenario"] == "absent" {
            None
        } else {
            Some(Box::new(Timer {
                calls: calls.clone(),
                fail: case["scenario"] == "cancel-throws",
            }))
        };
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2, "independent repeated cleanup floor");
        for (iteration, step) in steps.iter().enumerate() {
            let result = catch_unwind(AssertUnwindSafe(|| clear(&mut interval)));
            let expected_calls = match index {
                0 => 0,
                1 => 1,
                _ => iteration + 1,
            };
            assert_eq!(
                calls.borrow().len(),
                expected_calls,
                "Polymarket native timer cancellation count: {venue} {}",
                case["scenario"]
            );
            for id in calls.borrow().iter() {
                assert_eq!(*id, 17, "Polymarket native canceled timer identity");
            }
            assert_eq!(
                interval.is_some(),
                index == 2,
                "independent native timer retention"
            );
            assert_eq!(
                result.is_err(),
                index == 2,
                "independent native cancellation failure"
            );
            assert_eq!(
                step["calls"].as_array().unwrap().len(),
                expected_calls,
                "independent source cancellation floor"
            );
            assert_eq!(
                interval.is_some(),
                step["retained"].as_bool().unwrap(),
                "Polymarket native timer retention: {venue}"
            );
            assert_eq!(
                result.is_err(),
                step["failed"].as_bool().unwrap(),
                "Polymarket native cancellation failure: {venue}"
            );
            if let Err(error) = result {
                assert_eq!(
                    error.downcast_ref::<&str>().copied(),
                    Some("host cancellation failed"),
                    "Polymarket native cancellation error"
                );
            }
        }
    }
}

#[test]
fn public_ping_timer_cleanup_matches_source() {
    check("public", clear_public_ping_interval);
}

#[test]
fn private_ping_timer_cleanup_matches_source() {
    check("private", clear_private_ping_interval);
}
