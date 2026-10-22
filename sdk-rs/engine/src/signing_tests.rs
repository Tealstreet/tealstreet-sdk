use super::*;
use crate::signing::{
    Algorithm, Context, KeyRef, Operation, PayloadPart, SignError, SignRequest, SignResult,
};
struct Step;
impl crate::SignStep for Step {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let now = context.now_ms;
        if let Action::Http(http) = action {
            http.url = format!("https://venue.test/private?timestamp={now}");
        }
        Ok(SignRequest {
            context: Context::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("account/primary".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: now.to_string().into_bytes(),
                }],
            }],
        })
    }
    fn apply(&mut self, action: &mut Action, _: &SignResult) -> Result<(), SignError> {
        if let Action::Http(http) = action {
            http.headers.push(("signature", "completed".into()));
        }
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}
#[derive(Default)]
struct TestVenue {
    failed: usize,
    last_sign_error: Option<String>,
    repair_on_failure: bool,
    invalid_prepare: bool,
    repair_request: Option<RequestId>,
}
impl Venue for TestVenue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let request = cx.request(
            Method::Post,
            "https://venue.test/private".into(),
            vec![],
            None,
            (),
        );
        cx.sign_http(
            request,
            if self.invalid_prepare {
                Box::new(InvalidStep)
            } else {
                Box::new(Step)
            },
        );
        cx.route_request(request, RequestRoute::Primary);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
        panic!("not a network failure");
    }
    fn on_sign_failed(&mut self, cx: &mut Cx<'_, ()>, _: (), error: &str) {
        self.failed += 1;
        self.last_sign_error = Some(error.to_owned());
        if self.repair_on_failure && self.failed == 1 {
            let request = cx.request(
                Method::Post,
                "https://venue.test/repair".into(),
                vec![],
                None,
                (),
            );
            self.repair_request = Some(request);
            cx.sign_http(request, Box::new(Step));
        }
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {
        panic!("signing timeout leaked to venue timer");
    }
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine() -> Engine<TestVenue> {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine.start(1000.);
    engine
}
fn staged(engine: &mut Engine<TestVenue>) -> (RequestId, TimerId) {
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let attempt = actions
        .iter()
        .find_map(|a| {
            if let Action::Sign { attempt, .. } = a {
                Some(*attempt)
            } else {
                None
            }
        })
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    (attempt, timer)
}
fn result() -> Result<SignResult, SignError> {
    Ok(SignResult(vec![vec![0; 32]]))
}
#[test]
fn delayed_dispatch_stamps_current_time_and_preserves_route() {
    let mut engine = engine();
    engine.prepare_dispatch(4000.);
    let (attempt, _) = staged(&mut engine);
    engine.handle(
        4001.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    let http = engine
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(h) = a {
                Some(h)
            } else {
                None
            }
        })
        .unwrap();
    assert!(http.url.ends_with("timestamp=4000"));
    assert_eq!(http.route, Some(RequestRoute::Primary));
}
#[test]
fn hung_signer_times_out_without_reply_and_late_reply_is_inert() {
    let mut engine = engine();
    let (attempt, timer) = staged(&mut engine);
    engine.handle(21000., Event::TimerFired { timer });
    assert_eq!(engine.venue.failed, 1);
    engine.handle(
        21001.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(engine.venue.failed, 1);
}
#[test]
fn stale_batch_reprepares_with_new_attempt_without_extending_deadline() {
    let mut engine = engine();
    let (attempt, _) = staged(&mut engine);
    engine.handle(
        6000.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    let (next, _) = staged(&mut engine);
    assert_ne!(attempt, next);
    engine.handle(
        6001.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    assert!(engine.drain_actions().next().is_none());
    engine.handle(
        21000.,
        Event::Signed {
            attempt: next,
            result: result(),
        },
    );
    assert_eq!(engine.venue.failed, 1);
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn malformed_and_refused_results_fail_once_without_dispatch() {
    for result in [Ok(SignResult(vec![vec![1; 31]])), Err(SignError::Refused)] {
        let mut engine = engine();
        let (attempt, _) = staged(&mut engine);
        engine.handle(1001., Event::Signed { attempt, result });
        assert_eq!(engine.venue.failed, 1);
        assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
}
#[test]
fn timeout_override_after_staging_bounds_the_signer_too() {
    let mut engine = engine();
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.request_timeout(RequestId(1), 1000);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms: 2000., .. }))
    );
    let attempt = actions
        .iter()
        .find_map(|a| {
            if let Action::Sign {
                attempt,
                timeout_ms: 1000,
                ..
            } = a
            {
                Some(*attempt)
            } else {
                None
            }
        })
        .unwrap();
    engine.handle(
        2000.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    assert_eq!(engine.venue.failed, 1);
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn async_signing_serializes_dependent_dispatch_and_ignores_unsolicited_reply() {
    let mut engine = engine();
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let second = cx.request(
        Method::Post,
        "https://venue.test/second".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(second, Box::new(Step));
    let (first, _) = staged(&mut engine);
    // Only one provider batch may be active; a guessed later reply is inert.
    engine.handle(
        1001.,
        Event::Signed {
            attempt: RequestId(first.0 + 1),
            result: result(),
        },
    );
    assert!(!engine.has_actions());
    engine.handle(
        1002.,
        Event::Signed {
            attempt: first,
            result: result(),
        },
    );
    assert!(engine.has_actions());
    let actions: Vec<_> = engine.drain_actions().collect();
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(h) = a {
                Some(h.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests, [RequestId(1)]);
    let next = actions
        .iter()
        .find_map(|a| {
            if let Action::Sign { attempt, .. } = a {
                Some(*attempt)
            } else {
                None
            }
        })
        .unwrap();
    engine.handle(
        1003.,
        Event::Signed {
            attempt: next,
            result: result(),
        },
    );
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(h) = a {
                Some(h.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests, [second]);
}
struct Ready;
impl crate::signing::Signer for Ready {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, _: SignRequest) -> crate::signing::SignFuture {
        Box::pin(std::future::ready(result()))
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(result())
    }
}
#[test]
fn local_signing_keeps_mixed_signed_and_unsigned_action_order() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine.set_signer(Some(std::sync::Arc::new(Ready))).unwrap();
    engine.start(1000.);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let second = cx.request(
        Method::Get,
        "https://venue.test/public".into(),
        vec![],
        None,
        (),
    );
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(h) = a {
                Some(h.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests, [RequestId(1), second]);
}
#[test]
fn full_pending_queue_rejects_dependent_ws_message_instead_of_dropping_it() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    for _ in 0..256 {
        cx.send_signed(SocketId(42), "auth".into(), Box::new(Step));
    }
    cx.send(SocketId(42), "dependent");
    assert_eq!(engine.io.signing_admission.len(), 1);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::WsClose {
            socket: SocketId(42)
        }
    )));
}
#[test]
fn queued_attempt_own_deadline_expires_behind_a_hung_head() {
    let mut engine = engine();
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let second = cx.request(
        Method::Post,
        "https://venue.test/second".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(second, Box::new(Step));
    cx.request_timeout(second, 1000);
    let actions: Vec<_> = engine.drain_actions().collect();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer {
                timer,
                at_ms: 2000.,
            } = a
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    engine.handle(2000., Event::TimerFired { timer });
    assert_eq!(engine.venue.failed, 1);
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn queued_events_before_drain_cannot_pass_an_earlier_completed_outbound() {
    let mut engine = engine();
    let (first, _) = staged(&mut engine);
    let mut cx = Cx {
        now: 1001.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let second = cx.request(
        Method::Post,
        "https://venue.test/second".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(second, Box::new(Step));
    engine.handle(
        1002.,
        Event::Signed {
            attempt: first,
            result: result(),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let first_http = actions
        .iter()
        .position(|a| matches!(a,Action::Http(h) if h.id==RequestId(1)))
        .unwrap();
    let next_work = actions
        .iter()
        .position(|a| matches!(a, Action::Sign { .. }))
        .unwrap();
    assert!(first_http < next_work);
}
#[test]
fn completed_signature_is_rechecked_at_the_actual_drain() {
    let mut engine = engine();
    let (attempt, _) = staged(&mut engine);
    engine.handle(
        1001.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    engine.prepare_dispatch(8000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Sign { attempt: next, .. } if *next != attempt))
    );
}
#[test]
fn explicit_close_revokes_auth_without_waiting_for_host_callback() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let socket = SocketId(42);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.send_signed(socket, "unsigned".into(), Box::new(Step));
    engine.prepare_dispatch(1000.);
    let (attempt, _) = staged(&mut engine);
    let mut cx = Cx {
        now: 1001.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.close(socket);
    engine.handle(
        1002.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}
#[test]
fn dependent_ws_message_waits_for_authentication_dispatch() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let socket = SocketId(42);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.send_signed(socket, "auth".into(), Box::new(Step));
    cx.send(socket, "dependent");
    engine.prepare_dispatch(1000.);
    let (attempt, _) = staged(&mut engine);
    engine.handle(
        1001.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    let messages: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(messages, ["auth", "dependent"]);
}
#[test]
fn socket_close_revokes_pending_auth_completion() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let socket = SocketId(42);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.send_signed(socket, "unsigned".into(), Box::new(Step));
    engine.prepare_dispatch(1000.);
    let (attempt, _) = staged(&mut engine);
    engine.handle(1001., Event::WsClosed { socket, code: 1006 });
    engine.handle(
        1002.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}

struct InvalidStep;
impl crate::SignStep for InvalidStep {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Err(SignError::InvalidRequest)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        unreachable!("invalid preparation never completes")
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

fn complete_callback_retry(engine: &mut Engine<TestVenue>, now: f64) {
    let actions: Vec<_> = engine.drain_actions().collect();
    let attempts: Vec<_> = actions
        .iter()
        .filter_map(|action| {
            if let Action::Sign {
                attempt,
                work,
                timeout_ms,
            } = action
            {
                assert!(
                    !work.operations.is_empty(),
                    "internal marker escaped dispatch"
                );
                assert!(*timeout_ms > 0);
                Some(*attempt)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(attempts.len(), 1, "exactly one real repair batch");
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now,
        Event::Signed {
            attempt: attempts[0],
            result: result(),
        },
    );
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| {
            if let Action::Http(http) = a {
                Some(http.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests, [engine.venue.repair_request.unwrap()]);
    assert_eq!(engine.venue.failed, 1);
}

#[test]
fn preparation_failure_callback_retry_emits_only_real_signing_work() {
    let mut engine = crate::test_support::seeded_engine(TestVenue {
        repair_on_failure: true,
        invalid_prepare: true,
        ..Default::default()
    });
    engine.start(1000.);
    complete_callback_retry(&mut engine, 1001.);
}

#[test]
fn expired_outbound_callback_retry_emits_only_real_signing_work() {
    let mut engine = crate::test_support::seeded_engine(TestVenue {
        repair_on_failure: true,
        ..Default::default()
    });
    engine.start(1000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let attempt = actions
        .iter()
        .find_map(|a| {
            if let Action::Sign { attempt, .. } = a {
                Some(*attempt)
            } else {
                None
            }
        })
        .unwrap();
    engine.handle(
        1001.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    engine.prepare_dispatch(21000.);
    complete_callback_retry(&mut engine, 21001.);
}

struct ReadyAfterRefusal(std::sync::atomic::AtomicUsize);
impl crate::signing::Signer for ReadyAfterRefusal {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, _: SignRequest) -> crate::signing::SignFuture {
        unreachable!("an immediate provider is never polled")
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        (self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0).then(result)
    }
}
#[test]
fn unavailable_immediate_provider_callback_retry_dispatches_once() {
    let mut engine = crate::test_support::seeded_engine(TestVenue {
        repair_on_failure: true,
        ..Default::default()
    });
    engine
        .set_signer(Some(std::sync::Arc::new(ReadyAfterRefusal(
            std::sync::atomic::AtomicUsize::new(0),
        ))))
        .unwrap();
    engine.start(1000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Sign { .. })),
        "internal marker escaped inline completion"
    );
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(http) = a {
                Some(http.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests, [engine.venue.repair_request.unwrap()]);
    assert_eq!(engine.venue.failed, 1);
}

fn calibrate_protocol_time(engine: &mut Engine<TestVenue>, raw: f64, protocol: f64) {
    let mut cx = Cx {
        now: raw,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert!(cx.observe_protocol_time(protocol));
    assert_eq!(cx.protocol_now(), protocol);
    assert_eq!(cx.now, raw, "calibration must not replace scheduling time");
}

fn protocol_sign_attempt(actions: &[Action], timestamp: f64, timeout: u32) -> RequestId {
    let attempts: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Sign {
                attempt,
                work,
                timeout_ms,
            } => {
                assert_eq!(*timeout_ms, timeout);
                assert!(
                    work.operations
                        == vec![Operation::Sign {
                            algorithm: Algorithm::HmacSha256,
                            key: KeyRef("account/primary".into()),
                            payload: vec![PayloadPart::Bytes {
                                bytes: timestamp.to_string().into_bytes(),
                            }],
                        }],
                    "provider payload must use the latest protocol time",
                );
                Some(*attempt)
            }
            _ => None,
        })
        .collect();
    assert_eq!(attempts.len(), 1, "exactly one real provider batch");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::SignedOutbound { .. }))
    );
    attempts[0]
}

#[test]
fn protocol_dispatch_time_changes_signing_but_not_raw_timers_or_frame_time() {
    let mut engine = engine();
    calibrate_protocol_time(&mut engine, 1000., 61000.);
    let timer = {
        let mut cx = Cx {
            now: 1000.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        cx.set_timer(250.)
    };
    engine.prepare_dispatch(4000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let attempt = protocol_sign_attempt(&actions, 64000., 17000);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { timer: id, at_ms: 1250. } if *id == timer))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms: 21000., .. }))
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        4000.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("timestamp=64000"));
    assert_eq!(requests[0].timeout_ms, Some(17000));
    assert_eq!(requests[0].route, Some(RequestRoute::Primary));
    let frame = engine.snapshot_frame(4000.);
    assert_eq!(f64::from_le_bytes(frame[8..16].try_into().unwrap()), 4000.);
}

#[test]
fn same_raw_time_calibration_invalidates_old_success_and_error_without_losing_fifo() {
    for protocol in [11000., 500.] {
        for refused in [false, true] {
            let mut engine = engine();
            let (old, _) = staged(&mut engine);
            let second = {
                let mut cx = Cx {
                    now: 1000.,
                    store: &mut engine.store,
                    io: &mut engine.io,
                };
                let second = cx.request(
                    Method::Post,
                    "https://venue.test/second".into(),
                    vec![],
                    None,
                    (),
                );
                cx.sign_http(second, Box::new(Step));
                second
            };
            calibrate_protocol_time(&mut engine, 1000., protocol);
            engine.handle(
                1000.,
                Event::Signed {
                    attempt: old,
                    result: if refused {
                        Err(SignError::Refused)
                    } else {
                        result()
                    },
                },
            );
            let actions: Vec<_> = engine.drain_actions().collect();
            let renewed = protocol_sign_attempt(&actions, protocol, 20000);
            assert_ne!(renewed, old);
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a, Action::SetTimer { at_ms: 21000., .. }))
            );
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert_eq!(engine.venue.failed, 0);
            for result in [result(), Err(SignError::Refused)] {
                engine.handle(
                    1000.,
                    Event::Signed {
                        attempt: old,
                        result,
                    },
                );
                assert!(engine.drain_actions().next().is_none());
            }
            engine.handle(
                1000.,
                Event::Signed {
                    attempt: renewed,
                    result: result(),
                },
            );
            let actions: Vec<_> = engine.drain_actions().collect();
            let next = protocol_sign_attempt(&actions, protocol, 20000);
            assert_ne!(next, renewed);
            let first_http = actions
                .iter()
                .position(|a| matches!(a, Action::Http(h) if h.id == RequestId(1)))
                .unwrap();
            let next_sign = actions
                .iter()
                .position(|a| matches!(a, Action::Sign { .. }))
                .unwrap();
            assert!(
                first_http < next_sign,
                "renewed first request retains its FIFO position"
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Http(_)))
                    .count(),
                1
            );
            engine.handle(
                1000.,
                Event::Signed {
                    attempt: next,
                    result: result(),
                },
            );
            let requests: Vec<_> = engine
                .drain_actions()
                .filter_map(|a| match a {
                    Action::Http(http) => Some(http),
                    _ => None,
                })
                .collect();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].id, second);
            assert!(requests[0].url.ends_with(&format!("timestamp={protocol}")));
            assert_eq!(engine.venue.failed, 0);
        }
    }
}

#[test]
fn materialized_outbound_is_reprepared_after_same_raw_time_calibration() {
    let mut engine = engine();
    let (old, _) = staged(&mut engine);
    engine.handle(
        1000.,
        Event::Signed {
            attempt: old,
            result: result(),
        },
    );
    assert!(
        engine
            .io
            .actions
            .iter()
            .any(|a| matches!(a, Action::SignedOutbound { .. }))
    );
    calibrate_protocol_time(&mut engine, 1000., 61000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let renewed = protocol_sign_attempt(&actions, 61000., 20000);
    assert_ne!(old, renewed);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms: 21000., .. }))
    );
    engine.handle(
        1000.,
        Event::Signed {
            attempt: old,
            result: result(),
        },
    );
    assert!(engine.drain_actions().next().is_none());
    engine.handle(
        2000.,
        Event::Signed {
            attempt: renewed,
            result: result(),
        },
    );
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("timestamp=61000"));
    assert_eq!(requests[0].timeout_ms, Some(19000));
    assert_eq!(requests[0].headers, [("signature", "completed".into())]);
    assert_eq!(engine.venue.failed, 0);
}

#[test]
fn raw_timeout_precedes_protocol_revision_for_pending_and_materialized_signatures() {
    for materialized in [false, true] {
        let mut engine = engine();
        let (old, _) = staged(&mut engine);
        if materialized {
            engine.handle(
                1000.,
                Event::Signed {
                    attempt: old,
                    result: result(),
                },
            );
        }
        calibrate_protocol_time(&mut engine, 21000., 1000.);
        if materialized {
            engine.prepare_dispatch(21000.);
        } else {
            engine.handle(
                21000.,
                Event::Signed {
                    attempt: old,
                    result: Err(SignError::Refused),
                },
            );
        }
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Sign { .. } | Action::SignedOutbound { .. }
        )));
        assert_eq!(engine.venue.failed, 1);
        assert_eq!(
            engine.venue.last_sign_error.as_deref(),
            Some(SignError::TimedOut.venue_error().message.as_str())
        );
        engine.handle(
            21001.,
            Event::Signed {
                attempt: old,
                result: result(),
            },
        );
        assert!(engine.drain_actions().next().is_none());
        assert_eq!(engine.venue.failed, 1);
    }
}

#[test]
fn unchanged_or_invalid_protocol_observation_does_not_invalidate_pending_signature() {
    let mut engine = engine();
    calibrate_protocol_time(&mut engine, 1000., 61000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let attempt = protocol_sign_attempt(&actions, 61000., 20000);
    calibrate_protocol_time(&mut engine, 1000., 61000.);
    {
        let mut cx = Cx {
            now: 1000.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        assert!(!cx.observe_protocol_time(f64::NAN));
        assert_eq!(cx.protocol_now(), 61000.);
    }
    engine.handle(
        1000.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert_eq!(engine.venue.failed, 0);
}

#[test]
fn repeated_protocol_calibration_never_extends_the_original_raw_deadline() {
    let mut engine = engine();
    let (mut attempt, _) = staged(&mut engine);
    for (raw, protocol) in [
        (2000., 72000.),
        (4000., 3000.),
        (6000., 116000.),
        (20000., 1000.),
    ] {
        calibrate_protocol_time(&mut engine, raw, protocol);
        engine.handle(
            raw,
            Event::Signed {
                attempt,
                result: result(),
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        let renewed = protocol_sign_attempt(&actions, protocol, (21000. - raw) as u32);
        assert_ne!(renewed, attempt);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms: 21000., .. }))
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(engine.venue.failed, 0);
        engine.handle(
            raw,
            Event::Signed {
                attempt,
                result: Err(SignError::Refused),
            },
        );
        assert!(engine.drain_actions().next().is_none());
        attempt = renewed;
    }
    calibrate_protocol_time(&mut engine, 21000., 121000.);
    engine.handle(
        21000.,
        Event::Signed {
            attempt,
            result: result(),
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    assert_eq!(engine.venue.failed, 1);
    assert_eq!(
        engine.venue.last_sign_error.as_deref(),
        Some(SignError::TimedOut.venue_error().message.as_str())
    );
}

struct NonceStep;
impl crate::SignStep for NonceStep {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let mut nonce = [0; 4];
        context.signing_bytes(&mut nonce)?;
        let work = Step.prepare(action, context)?;
        let suffix = format!("?stamp={}&nonce={nonce:?}", context.now_ms);
        match action {
            Action::Http(http) => http.url.push_str(&suffix),
            Action::WsConnect { url, .. } => url.push_str(&suffix),
            _ => return Err(SignError::InvalidRequest),
        }
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if let Action::WsConnect { headers, .. } = action {
            headers.push(("signed", "yes".into()));
            Ok(())
        } else {
            Step.apply(action, result)
        }
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}
fn upgrade(engine: &mut Engine<TestVenue>, public: bool) -> SocketId {
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.connect_signed(
        SocketSpec::new("wss://venue.test/private")
            .channel(if public {
                SocketChannel::Public
            } else {
                SocketChannel::Private
            })
            .feed("private-recover", true)
            .binary()
            .headers(vec![("static", "kept".into())]),
        Box::new(NonceStep),
    )
}
fn signing_work(actions: &[Action]) -> (RequestId, String) {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => {
                let Operation::Sign { payload, .. } = &work.operations[0] else {
                    panic!("signature operation")
                };
                let PayloadPart::Bytes { bytes } = &payload[0] else {
                    panic!("exact bytes")
                };
                Some((*attempt, String::from_utf8(bytes.clone()).unwrap()))
            }
            _ => None,
        })
        .expect("one sign request")
}
#[test]
fn signing_context_uses_only_signing_stream_and_redraws_after_clock_revision() {
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };
    let calls = Arc::new(AtomicU8::new(0));
    let capture = calls.clone();
    let entropy = crate::entropy::Entropy::new(
        Box::new(|_| panic!("identity stream used for signing")),
        Box::new(move |out| {
            let value = capture.fetch_add(1, Ordering::SeqCst) + 1;
            out.fill(value);
            Ok(())
        }),
    );
    let mut engine = Engine::new(TestVenue::default(), Some(entropy)).unwrap();
    let socket = upgrade(&mut engine, false);
    engine.prepare_dispatch(1000.);
    let first_actions: Vec<_> = engine.drain_actions().collect();
    let (first, stamp) = signing_work(&first_actions);
    assert_eq!(stamp, "1000");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        !first_actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    calibrate_protocol_time(&mut engine, 1000., 61000.);
    engine.handle(
        1000.,
        Event::Signed {
            attempt: first,
            result: result(),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let (second, stamp) = signing_work(&actions);
    assert_ne!(first, second);
    assert_eq!(stamp, "61000");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms: 21000., .. }))
    );
    engine.handle(
        1001.,
        Event::Signed {
            attempt: second,
            result: result(),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let upgrades: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::WsConnect {
                socket: id,
                url,
                headers,
                channel,
                context,
                binary,
            } => Some((id, url, headers, channel, context, binary)),
            _ => None,
        })
        .collect();
    assert_eq!(upgrades.len(), 1);
    let (id, url, headers, channel, context, binary) = upgrades[0];
    assert_eq!(*id, socket);
    assert!(url.ends_with("stamp=61000&nonce=[2, 2, 2, 2]"));
    assert_eq!(
        headers,
        &[("static", "kept".into()), ("signed", "yes".into())]
    );
    assert_eq!(*channel, Some(SocketChannel::Private));
    assert_eq!(context.as_ref().unwrap().feed, "private-recover");
    assert!(context.as_ref().unwrap().recover);
    assert!(*binary);
}
#[test]
fn signed_upgrade_never_connects_after_timeout_or_close_or_public_suppression() {
    for mode in ["timeout", "explicit", "host"] {
        let mut engine = crate::test_support::seeded_engine(TestVenue::default());
        let socket = upgrade(&mut engine, false);
        engine.prepare_dispatch(1000.);
        let (attempt, timer) = staged(&mut engine);
        match mode {
            "timeout" => engine.handle(21000., Event::TimerFired { timer }),
            "host" => engine.handle(1001., Event::WsClosed { socket, code: 1006 }),
            _ => {
                let mut cx = Cx {
                    now: 1001.,
                    store: &mut engine.store,
                    io: &mut engine.io,
                };
                cx.close(socket);
            }
        }
        engine.handle(
            21001.,
            Event::Signed {
                attempt,
                result: result(),
            },
        );
        assert!(
            !engine
                .drain_actions()
                .any(|a| matches!(a, Action::WsConnect { .. }))
        );
    }
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine
        .set_public_feed_policy(PublicFeedPolicy::Disabled)
        .unwrap();
    assert_eq!(upgrade(&mut engine, true), SocketId(0));
    assert!(engine.drain_actions().next().is_none());
}
#[test]
fn signing_entropy_failure_clears_partial_nonce_and_never_reaches_provider_or_socket() {
    let entropy = crate::entropy::Entropy::new(
        Box::new(|_| panic!("identity")),
        Box::new(|out| {
            out.fill(99);
            Err("private provider detail".into())
        }),
    );
    let context = crate::DispatchContext::new(1000., Some(&entropy));
    let mut out = [1; 4];
    assert_eq!(context.signing_bytes(&mut out), Err(SignError::Unavailable));
    assert_eq!(out, [0; 4]);
    let mut engine = Engine::new(TestVenue::default(), Some(entropy)).unwrap();
    upgrade(&mut engine, false);
    engine.prepare_dispatch(1000.);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Sign { .. } | Action::WsConnect { .. }))
    );
}
