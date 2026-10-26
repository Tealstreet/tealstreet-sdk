use super::*;

fn call(timeout_ms: u32) -> CallBound<'static> {
    CallBound {
        timeout_ms,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        after_refusal: &[],
    }
}

fn one(timeout_ms: u32) -> PlanBound {
    PlanBound::from_steps(&[StepBound {
        wait_ms: 0,
        calls: &[call(timeout_ms)],
    }])
    .unwrap()
}

#[test]
fn bingx_serial_cancel_plan_retains_native_fresh_retry_windows_beyond_120_seconds() {
    let calls: Vec<_> = (0..8)
        .map(|_| CallBound {
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            ..call(20_000)
        })
        .collect();
    let steps: Vec<_> = calls
        .iter()
        .map(|call| StepBound {
            wait_ms: 0,
            calls: std::slice::from_ref(call),
        })
        .collect();
    let bound = PlanBound::from_steps(&steps).unwrap();
    assert_eq!(bound.span_ms(), 320_000);
    assert_eq!(bound.logical_calls(), 8);
    assert_eq!(bound.physical_attempts(), 16);
    assert_eq!(bound.peak_concurrent(), 1);
    assert_eq!(bound.deadline(1_000.), Ok(321_000.));
}

#[test]
fn concurrent_calls_use_the_maximum_window_and_serial_steps_include_each_wait() {
    let calls = [call(20_000), call(30_000)];
    let concurrent = PlanBound::from_steps(&[StepBound {
        wait_ms: 450,
        calls: &calls,
    }])
    .unwrap();
    assert_eq!(concurrent.span_ms(), 30_450);
    assert_eq!(concurrent.peak_concurrent(), 2);
    let serial = PlanBound::from_steps(&[
        StepBound {
            wait_ms: 450,
            calls: &calls[..1],
        },
        StepBound {
            wait_ms: 1_000,
            calls: &calls[1..],
        },
    ])
    .unwrap();
    assert_eq!(serial.span_ms(), 51_450);
    assert_eq!(serial.peak_concurrent(), 1);
    assert_eq!(serial.physical_attempts(), concurrent.physical_attempts());
}

#[test]
fn shared_retry_window_still_reserves_every_physical_attempt() {
    let bound = PlanBound::from_steps(&[StepBound {
        wait_ms: 200,
        calls: &[CallBound {
            attempts: 3,
            ..call(20_000)
        }],
    }])
    .unwrap();
    assert_eq!(bound.span_ms(), 20_200);
    assert_eq!(bound.logical_calls(), 1);
    assert_eq!(bound.physical_attempts(), 3);
}

#[test]
fn fallback_runs_once_after_parent_retries_and_keeps_historical_parent_credits() {
    let children = [call(10_000), call(20_000)];
    let fallback = [StepBound {
        wait_ms: 100,
        calls: &children,
    }];
    let bound = PlanBound::from_steps(&[StepBound {
        wait_ms: 500,
        calls: &[CallBound {
            attempts: 2,
            retry_window: RetryWindow::FreshAttempt,
            after_refusal: &fallback,
            ..call(5_000)
        }],
    }])
    .unwrap();
    assert_eq!(bound.span_ms(), 30_600);
    assert_eq!(bound.logical_calls(), 3);
    assert_eq!(bound.physical_attempts(), 4);
    assert_eq!(bound.peak_concurrent(), 2);
}

#[test]
fn concurrent_fallback_peaks_are_summed_even_when_another_parent_is_still_live() {
    let two_calls = [call(1), call(1)];
    let three_calls = [call(1), call(1), call(1)];
    let two = [StepBound {
        wait_ms: 0,
        calls: &two_calls,
    }];
    let three = [StepBound {
        wait_ms: 0,
        calls: &three_calls,
    }];
    let parents = [
        CallBound {
            after_refusal: &two,
            ..call(1)
        },
        CallBound {
            after_refusal: &three,
            ..call(1)
        },
    ];
    let bound = PlanBound::from_steps(&[StepBound {
        wait_ms: 0,
        calls: &parents,
    }])
    .unwrap();
    assert_eq!(bound.span_ms(), 2);
    assert_eq!(bound.peak_concurrent(), 5);
    assert_eq!(bound.physical_attempts(), 7);
}

#[test]
fn aliased_fallbacks_are_charged_per_occurrence_instead_of_deduplicated() {
    let fallback = [StepBound {
        wait_ms: 0,
        calls: &[call(1)],
    }];
    let parents = [
        CallBound {
            after_refusal: &fallback,
            ..call(1)
        },
        CallBound {
            after_refusal: &fallback,
            ..call(1)
        },
    ];
    let bound = PlanBound::from_steps(&[StepBound {
        wait_ms: 0,
        calls: &parents,
    }])
    .unwrap();
    assert_eq!(bound.logical_calls(), 4);
    assert_eq!(bound.physical_attempts(), 4);
}

#[test]
fn invalid_declarations_refuse_without_creating_an_empty_success() {
    assert_eq!(PlanBound::from_steps(&[]), Err(Error::Invalid));
    assert_eq!(
        PlanBound::from_steps(&[StepBound {
            wait_ms: 1,
            calls: &[],
        }]),
        Err(Error::Invalid)
    );
    for invalid in [
        call(0),
        CallBound {
            attempts: 0,
            ..call(1)
        },
    ] {
        assert_eq!(
            PlanBound::from_steps(&[StepBound {
                wait_ms: 0,
                calls: &[invalid],
            }]),
            Err(Error::Invalid)
        );
    }
}

#[test]
fn capacity_is_the_existing_journal_bound_including_retries_and_nested_fallbacks() {
    let full = PlanBound::from_steps(&[StepBound {
        wait_ms: 0,
        calls: &[CallBound {
            attempts: MAX_ATTEMPTS as u16,
            ..call(1)
        }],
    }])
    .unwrap();
    assert_eq!(full.physical_attempts(), MAX_ATTEMPTS);
    let fallback = [StepBound {
        wait_ms: 0,
        calls: &[call(1)],
    }];
    for attempts in [MAX_ATTEMPTS as u16, u16::MAX] {
        assert_eq!(
            PlanBound::from_steps(&[StepBound {
                wait_ms: 0,
                calls: &[CallBound {
                    attempts,
                    after_refusal: &fallback,
                    ..call(1)
                }],
            }]),
            Err(Error::Capacity)
        );
    }
    let too_many: Vec<_> = (0..=MAX_ATTEMPTS).map(|_| call(1)).collect();
    assert_eq!(
        PlanBound::from_steps(&[StepBound {
            wait_ms: 0,
            calls: &too_many,
        }]),
        Err(Error::Capacity)
    );
}

#[test]
fn cyclic_declarations_stop_at_capacity_without_recursive_stack_growth() {
    static STEPS: [StepBound<'static>; 1] = [StepBound {
        wait_ms: 0,
        calls: &CALLS,
    }];
    static CALLS: [CallBound<'static>; 1] = [CallBound {
        timeout_ms: 1,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        after_refusal: &STEPS,
    }];
    assert_eq!(PlanBound::from_steps(&STEPS), Err(Error::Capacity));
}

#[test]
fn largest_declared_time_and_pacing_use_wide_checked_arithmetic() {
    let calls: Vec<_> = (0..MAX_ATTEMPTS).map(|_| call(u32::MAX)).collect();
    let steps: Vec<_> = calls
        .iter()
        .map(|call| StepBound {
            wait_ms: u32::MAX,
            calls: std::slice::from_ref(call),
        })
        .collect();
    let bound = PlanBound::from_steps(&steps).unwrap();
    assert_eq!(
        bound.span_ms(),
        u64::from(u32::MAX) * MAX_ATTEMPTS as u64 * 2
    );
    assert_eq!(bound.deadline(0.), Ok(bound.span_ms() as f64));
}

#[test]
fn deadline_rounds_up_if_a_later_representable_result_would_be_too_short() {
    let raw = (1u64 << 54) as f64;
    assert_eq!((raw + 5.) - raw, 4.);
    assert_eq!(one(5).deadline(raw), Ok((raw + 5.).next_up()));
    // Subtracting now can hide a short sum through a second rounding operation.
    let raw = -0.2f64;
    let rounded = raw + 320_000.;
    assert_eq!(rounded - raw, 320_000.);
    assert_eq!(one(320_000).deadline(raw), Ok(rounded.next_up()));
    assert_eq!(one(1).deadline(0.), Ok(1.));
}

#[test]
fn nonfinite_clocks_and_unrepresentable_later_deadlines_refuse() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(one(1).deadline(now), Err(Error::Invalid));
    }
    for now in [f64::MAX, -f64::MAX] {
        assert_eq!(one(1).deadline(now), Err(Error::Capacity));
    }
}

fn terms(retry_window: RetryWindow) -> CallTerms {
    CallTerms {
        wait_ms: 0,
        timeout_ms: 20_000,
        attempts: 2,
        retry_window,
    }
}

#[test]
fn fresh_call_window_retains_chain_and_projects_each_native_attempt() {
    let window = terms(RetryWindow::FreshAttempt)
        .window(0., 80_000.)
        .unwrap();
    assert_eq!(window.deadline(), 40_000.);
    assert_eq!(window.physical_deadline(0.), Ok(20_000.));
    assert_eq!(window.physical_deadline(19_000.), Ok(39_000.));
    assert_eq!(window.physical_deadline(39_999.), Ok(40_000.));
    assert_eq!(window.physical_deadline(40_000.), Err(Error::TimedOut));
    assert_eq!(window.physical_deadline(40_001.), Err(Error::TimedOut));
    assert_eq!(window.physical_deadline(-1.), Err(Error::Invalid));
    // The helper is pure; querying another time does not renew a captured cap.
    assert_eq!(window.physical_deadline(0.), Ok(20_000.));
}

#[test]
fn shared_call_retry_never_renews_the_first_physical_deadline() {
    let window = terms(RetryWindow::Shared).window(0., 80_000.).unwrap();
    assert_eq!(window.deadline(), 20_000.);
    assert_eq!(window.physical_deadline(19_000.), Ok(20_000.));
    assert_eq!(window.physical_deadline(20_000.), Err(Error::TimedOut));
    assert_eq!(window.physical_deadline(20_001.), Err(Error::TimedOut));
}

#[test]
fn physical_windows_clip_to_root_and_later_serial_call_has_its_own_chain() {
    let call = terms(RetryWindow::FreshAttempt);
    let first = call.window(0., 80_000.).unwrap();
    let second = call.window(38_000., 80_000.).unwrap();
    assert_eq!(first.deadline(), 40_000.);
    assert_eq!(second.deadline(), 78_000.);
    assert_eq!(second.physical_deadline(38_000.), Ok(58_000.));
    assert_eq!(second.physical_deadline(57_000.), Ok(77_000.));
    let clipped = call.window(38_000., 70_000.).unwrap();
    assert_eq!(clipped.deadline(), 70_000.);
    assert_eq!(clipped.physical_deadline(57_000.), Ok(70_000.));
    assert_eq!(clipped.physical_deadline(70_000.), Err(Error::TimedOut));
    // Concurrent work starts together: neither call borrows a peer's deadline.
    let concurrent = call.window(0., 40_000.).unwrap();
    assert_eq!(concurrent, first);
}

#[test]
fn call_window_rejects_invalid_declarations_and_clocks_without_mutation() {
    let call = terms(RetryWindow::FreshAttempt);
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(call.window(now, 80_000.), Err(Error::Invalid));
    }
    for root in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(call.window(0., root), Err(Error::Invalid));
    }
    for root in [0., -1.] {
        assert_eq!(call.window(0., root), Err(Error::TimedOut));
    }
    let mut invalid = call;
    invalid.timeout_ms = 0;
    assert_eq!(invalid.window(0., 80_000.), Err(Error::Invalid));
    invalid.timeout_ms = 20_000;
    invalid.attempts = 0;
    assert_eq!(invalid.window(0., 80_000.), Err(Error::Invalid));
    invalid.attempts = MAX_ATTEMPTS as u16 + 1;
    assert_eq!(invalid.window(0., 80_000.), Err(Error::Capacity));
    let window = call.window(0., 80_000.).unwrap();
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(window.physical_deadline(now), Err(Error::Invalid));
    }
}

#[test]
fn call_window_uses_the_same_upward_projection_as_whole_plan_bounds() {
    let call = CallTerms {
        timeout_ms: 5,
        attempts: 1,
        ..terms(RetryWindow::FreshAttempt)
    };
    let raw = (1u64 << 54) as f64;
    let window = call.window(raw, raw + 100.).unwrap();
    assert_eq!(window.deadline(), one(5).deadline(raw).unwrap());
    assert_eq!(window.physical_deadline(raw), Ok(window.deadline()));
    assert_eq!(call.window(-f64::MAX, 0.), Err(Error::Capacity));
    let call = CallTerms {
        timeout_ms: 320_000,
        ..call
    };
    let window = call.window(-0.2, 1_000_000.).unwrap();
    assert_eq!(window.deadline(), one(320_000).deadline(-0.2).unwrap());
}
