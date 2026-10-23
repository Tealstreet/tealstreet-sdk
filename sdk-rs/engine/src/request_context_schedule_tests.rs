use super::*;

fn call(attempts: u16) -> CallBound<'static> {
    CallBound {
        timeout_ms: 20_000,
        attempts,
        retry_window: RetryWindow::FreshAttempt,
        after_refusal: &[],
    }
}

fn accounting(schedule: &Schedule, used: usize, reserved: usize, released: usize) {
    assert_eq!(schedule.used_attempts(), used);
    assert_eq!(schedule.reserved_attempts(), reserved);
    assert_eq!(schedule.released_attempts(), released);
    assert_eq!(
        used + reserved + released,
        schedule.bound().physical_attempts()
    );
}

#[test]
fn serial_native_plan_cannot_complete_with_unissued_required_calls() {
    let calls = (0..8).map(|_| vec![call(2)]).collect::<Vec<_>>();
    let steps = calls
        .iter()
        .map(|calls| StepBound { wait_ms: 0, calls })
        .collect::<Vec<_>>();
    let mut schedule = Schedule::from_steps(&steps).unwrap();
    assert_eq!(schedule.bound().span_ms(), 320_000);
    accounting(&schedule, 0, 16, 0);
    for step in 0..8 {
        assert!(!schedule.is_complete());
        let slots = schedule.available().collect::<Vec<_>>();
        assert_eq!(slots.len(), 1);
        let slot = &slots[0];
        assert_eq!(schedule.terms(slot).unwrap().timeout_ms, 20_000);
        assert_eq!(
            schedule.terms(slot).unwrap().retry_window,
            RetryWindow::FreshAttempt
        );
        schedule.start(slot).unwrap();
        schedule.retry(slot).unwrap();
        assert_eq!(schedule.retry(slot), Err(Error::Capacity));
        schedule.settle(slot).unwrap();
        accounting(&schedule, (step + 1) * 2, (7 - step) * 2, 0);
        assert_eq!(schedule.settle(slot), Err(Error::State));
        assert_eq!(schedule.start(slot), Err(Error::State));
    }
    assert!(schedule.is_complete());
    assert_eq!(schedule.available().count(), 0);
}

#[test]
fn concurrent_step_is_a_barrier_and_preserves_declared_waits() {
    let first = [call(1), call(2)];
    let second = [call(1)];
    let steps = [
        StepBound {
            wait_ms: 200,
            calls: &first,
        },
        StepBound {
            wait_ms: 500,
            calls: &second,
        },
    ];
    let mut schedule = Schedule::from_steps(&steps).unwrap();
    assert_eq!(schedule.bound().span_ms(), 60_700);
    let slots = schedule.available().collect::<Vec<_>>();
    assert_eq!(slots.len(), 2);
    for slot in &slots {
        assert_eq!(schedule.terms(slot).unwrap().wait_ms, 200);
        schedule.start(slot).unwrap();
    }
    schedule.settle(&slots[1]).unwrap();
    assert_eq!(schedule.available().count(), 0);
    assert!(!schedule.is_complete());
    accounting(&schedule, 2, 1, 1);
    schedule.settle(&slots[0]).unwrap();
    let next = schedule.available().next().unwrap();
    assert_eq!(schedule.terms(&next).unwrap().wait_ms, 500);
    schedule.start(&next).unwrap();
    schedule.settle(&next).unwrap();
    assert!(schedule.is_complete());
    accounting(&schedule, 3, 0, 1);
}

fn branch_plan() -> Schedule {
    let leaves = [call(2), call(1)];
    let branch = [StepBound {
        wait_ms: 100,
        calls: &leaves,
    }];
    let parent = [CallBound {
        after_refusal: &branch,
        ..call(2)
    }];
    let tail = [call(1)];
    Schedule::from_steps(&[
        StepBound {
            wait_ms: 0,
            calls: &parent,
        },
        StepBound {
            wait_ms: 0,
            calls: &tail,
        },
    ])
    .unwrap()
}

#[test]
fn selected_replacement_supersedes_parent_once_and_then_unlocks_serial_tail() {
    let mut schedule = branch_plan();
    let parent = schedule.available().next().unwrap();
    assert_eq!(schedule.replace(&parent), Err(Error::State));
    schedule.start(&parent).unwrap();
    schedule.replace(&parent).unwrap();
    accounting(&schedule, 1, 4, 1);
    assert_eq!(schedule.settle(&parent), Err(Error::State));
    assert_eq!(schedule.retry(&parent), Err(Error::State));
    assert_eq!(schedule.replace(&parent), Err(Error::State));
    let children = schedule.available().collect::<Vec<_>>();
    assert_eq!(children.len(), 2);
    for child in &children {
        assert_eq!(schedule.terms(child).unwrap().wait_ms, 100);
        schedule.start(child).unwrap();
        schedule.settle(child).unwrap();
    }
    assert!(!schedule.is_complete());
    accounting(&schedule, 3, 1, 2);
    let tail = schedule.available().next().unwrap();
    schedule.start(&tail).unwrap();
    assert_eq!(schedule.replace(&tail), Err(Error::Invalid));
    accounting(&schedule, 4, 0, 2);
    schedule.settle(&tail).unwrap();
    assert!(schedule.is_complete());
}

#[test]
fn unused_fallback_and_retry_credits_are_released_without_becoming_effects() {
    let mut schedule = branch_plan();
    let parent = schedule.available().next().unwrap();
    schedule.start(&parent).unwrap();
    schedule.settle(&parent).unwrap();
    accounting(&schedule, 1, 1, 4);
    assert!(!schedule.is_complete());
    let tail = schedule.available().next().unwrap();
    schedule.start(&tail).unwrap();
    schedule.settle(&tail).unwrap();
    accounting(&schedule, 2, 0, 4);
    assert!(schedule.is_complete());
}

#[test]
fn nested_branch_completion_cannot_skip_an_outer_peer_or_tail() {
    let leaf = [call(1)];
    let leaf_steps = [StepBound {
        wait_ms: 0,
        calls: &leaf,
    }];
    let middle = [
        CallBound {
            after_refusal: &leaf_steps,
            ..call(1)
        },
        call(1),
    ];
    let middle_steps = [StepBound {
        wait_ms: 0,
        calls: &middle,
    }];
    let root = [CallBound {
        after_refusal: &middle_steps,
        ..call(1)
    }];
    let mut schedule = Schedule::from_steps(&[StepBound {
        wait_ms: 0,
        calls: &root,
    }])
    .unwrap();
    let root = schedule.available().next().unwrap();
    schedule.start(&root).unwrap();
    schedule.replace(&root).unwrap();
    let middle = schedule.available().collect::<Vec<_>>();
    schedule.start(&middle[0]).unwrap();
    schedule.replace(&middle[0]).unwrap();
    let leaf = schedule
        .available()
        .find(|slot| slot.index != middle[1].index)
        .unwrap();
    schedule.start(&leaf).unwrap();
    schedule.settle(&leaf).unwrap();
    assert!(!schedule.is_complete());
    schedule.start(&middle[1]).unwrap();
    schedule.settle(&middle[1]).unwrap();
    assert!(schedule.is_complete());
    accounting(&schedule, 4, 0, 0);
}

#[test]
fn foreign_duplicate_and_cancelled_slots_cannot_change_accounting() {
    let mut first = branch_plan();
    let mut second = branch_plan();
    let slot = first.available().next().unwrap();
    assert_eq!(second.start(&slot), Err(Error::Revoked));
    assert_eq!(second.terms(&slot), Err(Error::Revoked));
    assert_eq!(second.retry(&slot), Err(Error::Revoked));
    assert_eq!(second.replace(&slot), Err(Error::Revoked));
    assert_eq!(second.settle(&slot), Err(Error::Revoked));
    accounting(&second, 0, 6, 0);
    assert_eq!(first.retry(&slot), Err(Error::State));
    first.start(&slot).unwrap();
    assert_eq!(first.start(&slot), Err(Error::State));
    first.cancel();
    first.cancel();
    assert!(!first.is_complete());
    assert_eq!(first.available().count(), 0);
    assert_eq!(first.start(&slot), Err(Error::State));
    assert_eq!(first.settle(&slot), Err(Error::State));
    accounting(&first, 1, 0, 5);
}

#[test]
fn retained_terms_do_not_borrow_the_mutable_native_declaration() {
    let mut calls = [call(2)];
    let mut schedule = Schedule::from_steps(&[StepBound {
        wait_ms: 450,
        calls: &calls,
    }])
    .unwrap();
    calls[0].timeout_ms = 1;
    calls[0].attempts = 1;
    assert_eq!(calls[0].timeout_ms, 1);
    assert_eq!(calls[0].attempts, 1);
    let slot = schedule.available().next().unwrap();
    assert_eq!(schedule.terms(&slot).unwrap().timeout_ms, 20_000);
    assert_eq!(schedule.terms(&slot).unwrap().wait_ms, 450);
    schedule.start(&slot).unwrap();
    schedule.retry(&slot).unwrap();
    schedule.settle(&slot).unwrap();
    assert!(schedule.is_complete());
}

#[test]
fn capacity_and_empty_declarations_refuse_before_a_schedule_exists() {
    assert!(matches!(Schedule::from_steps(&[]), Err(Error::Invalid)));
    let calls = [call(257)];
    assert!(matches!(
        Schedule::from_steps(&[StepBound {
            wait_ms: 0,
            calls: &calls
        }]),
        Err(Error::Capacity)
    ));
    let calls = [call(256)];
    let mut schedule = Schedule::from_steps(&[StepBound {
        wait_ms: 0,
        calls: &calls,
    }])
    .unwrap();
    let slot = schedule.available().next().unwrap();
    schedule.start(&slot).unwrap();
    for _ in 1..256 {
        schedule.retry(&slot).unwrap();
    }
    assert_eq!(schedule.retry(&slot), Err(Error::Capacity));
    schedule.settle(&slot).unwrap();
    accounting(&schedule, 256, 0, 0);
}

#[test]
fn aliased_concurrent_fallbacks_keep_distinct_slots_and_credit_custody() {
    let leaves = [call(2), call(1)];
    let fallback = [StepBound {
        wait_ms: 0,
        calls: &leaves,
    }];
    let parents = [
        CallBound {
            after_refusal: &fallback,
            ..call(2)
        },
        CallBound {
            after_refusal: &fallback,
            ..call(2)
        },
    ];
    let steps = [StepBound {
        wait_ms: 0,
        calls: &parents,
    }];
    for select_both in [false, true] {
        let mut schedule = Schedule::from_steps(&steps).unwrap();
        let parents = schedule.available().collect::<Vec<_>>();
        schedule.start(&parents[0]).unwrap();
        schedule.start(&parents[1]).unwrap();
        schedule.replace(&parents[0]).unwrap();
        if select_both {
            schedule.replace(&parents[1]).unwrap();
            accounting(&schedule, 2, 6, 2);
        } else {
            schedule.settle(&parents[1]).unwrap();
            accounting(&schedule, 2, 3, 5);
        }
        let children = schedule.available().collect::<Vec<_>>();
        assert_eq!(children.len(), if select_both { 4 } else { 2 });
        let unique = children
            .iter()
            .map(|slot| slot.index)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            unique.len(),
            children.len(),
            "aliased declarations are distinct occurrences"
        );
        for (index, child) in children.iter().enumerate() {
            schedule.start(child).unwrap();
            schedule.settle(child).unwrap();
            assert_eq!(schedule.is_complete(), index + 1 == children.len());
        }
        accounting(
            &schedule,
            if select_both { 6 } else { 4 },
            0,
            if select_both { 4 } else { 6 },
        );
    }
}
