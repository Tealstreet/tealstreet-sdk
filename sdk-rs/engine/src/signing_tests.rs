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
            receipt: Default::default(),
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
/// `Step` with a chosen ref, so a test can name either credential slot.
struct KeyStep(&'static str);
impl crate::SignStep for KeyStep {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let mut work = Step.prepare(action, context)?;
        if let Some(Operation::Sign { key, .. }) = work.operations.first_mut() {
            *key = KeyRef(self.0.into());
        }
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        Step.apply(action, result)
    }
    fn freshness_ms(&self) -> f64 {
        Step.freshness_ms()
    }
}
#[derive(Default)]
struct TestVenue {
    failed: usize,
    receipts: Vec<(String, crate::signing::SignReceipt)>,
    last_sign_error: Option<String>,
    repair_on_failure: bool,
    invalid_prepare: bool,
    repair_request: Option<RequestId>,
    /// Sign the first request with this ref instead of `Step`'s.
    key: Option<&'static str>,
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
            } else if let Some(key) = self.key {
                Box::new(KeyStep(key))
            } else {
                Box::new(Step)
            },
        );
        cx.route_request(request, RequestRoute::Primary);
    }
    fn on_signing_receipt(
        &mut self,
        target: SigningTarget<'_, ()>,
        receipt: crate::signing::SignReceipt,
    ) {
        let target = match target {
            SigningTarget::Http { request, pending } => {
                *pending = ();
                format!("http:{}", request.0)
            }
            SigningTarget::Socket { socket } => format!("socket:{}", socket.0),
        };
        self.receipts.push((target, receipt));
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
    Ok(SignResult::new(vec![vec![0; 32]], Default::default()))
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
    for result in [
        Ok(SignResult::new(vec![vec![1; 31]], Default::default())),
        Err(SignError::Refused),
    ] {
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
    assert_eq!(engine.io.core.signing_admission.len(), 1);
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

/// A recipe whose identity lacks the ref it needs (`signing_ref()` failing).
struct MissingKeyStep;
impl crate::SignStep for MissingKeyStep {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Err(SignError::MissingKey)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        unreachable!("a missing key never completes")
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
            .core
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

struct ReceiptStep {
    expiry_only: bool,
}
impl crate::SignStep for ReceiptStep {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let mut request = Step.prepare(action, context)?;
        request.receipt = crate::signing::SignReceipt {
            nonce: None,
            stamp_ms: (!self.expiry_only).then_some(context.now_ms.floor()),
            expiry_ms: Some(context.now_ms.floor() + 20.),
        };
        Ok(request)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        Step.apply(action, result)
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}
fn receipt_engine(expiry_only: bool) -> Engine<TestVenue> {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let request = cx.request(
        Method::Post,
        "https://venue.test/private".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(request, Box::new(ReceiptStep { expiry_only }));
    engine.prepare_dispatch(1000.);
    engine
}
fn receipt_attempt(actions: &[Action]) -> (RequestId, crate::signing::SignReceipt) {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.receipt)),
            _ => None,
        })
        .unwrap()
}
fn with_receipt(receipt: crate::signing::SignReceipt) -> Result<SignResult, SignError> {
    Ok(SignResult::new(vec![vec![0; 32]], receipt))
}
#[test]
fn wrong_missing_or_invalid_receipt_is_refused_before_apply_or_publication() {
    use crate::signing::SignReceipt;
    for receipt in [
        SignReceipt::default(),
        SignReceipt {
            nonce: None,
            stamp_ms: Some(999.),
            expiry_ms: Some(1020.),
        },
        SignReceipt {
            nonce: None,
            stamp_ms: Some(1000.),
            expiry_ms: Some(1021.),
        },
        SignReceipt {
            nonce: None,
            stamp_ms: Some(f64::NAN),
            expiry_ms: Some(1020.),
        },
    ] {
        let mut engine = receipt_engine(false);
        let actions: Vec<_> = engine.drain_actions().collect();
        let (attempt, _) = receipt_attempt(&actions);
        engine.handle(
            1001.,
            Event::Signed {
                attempt,
                result: with_receipt(receipt),
            },
        );
        assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(engine.venue.failed, 1);
        assert!(engine.venue.receipts.is_empty());
    }
}
#[test]
fn expired_receipts_reprepare_at_completion_and_release_and_publish_only_final_attempt_once() {
    for materialized in [false, true] {
        let mut engine = receipt_engine(false);
        let actions: Vec<_> = engine.drain_actions().collect();
        let (first, receipt) = receipt_attempt(&actions);
        if materialized {
            engine.handle(
                1001.,
                Event::Signed {
                    attempt: first,
                    result: with_receipt(receipt),
                },
            );
            assert!(
                engine.venue.receipts.is_empty(),
                "completion alone is not release"
            );
            engine.prepare_dispatch(1020.);
        } else {
            engine.handle(
                1020.,
                Event::Signed {
                    attempt: first,
                    result: with_receipt(receipt),
                },
            );
        }
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(engine.venue.receipts.is_empty());
        let (second, receipt) = receipt_attempt(&actions);
        assert_ne!(first, second);
        assert_eq!(receipt.stamp_ms, Some(1020.));
        assert_eq!(receipt.expiry_ms, Some(1040.));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms: 21000., .. }))
        );
        engine.handle(
            1021.,
            Event::Signed {
                attempt: second,
                result: with_receipt(receipt),
            },
        );
        assert!(engine.venue.receipts.is_empty());
        let actions: Vec<_> = engine.drain_actions().collect();
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(http) => Some(http),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.ends_with("timestamp=1020"));
        assert_eq!(requests[0].timeout_ms, Some(19979));
        assert_eq!(
            engine.venue.receipts,
            [(format!("http:{}", requests[0].id.0), receipt)]
        );
        for attempt in [first, second] {
            engine.handle(
                1022.,
                Event::Signed {
                    attempt,
                    result: with_receipt(receipt),
                },
            );
        }
        assert!(engine.drain_actions().next().is_none());
        assert_eq!(engine.venue.receipts.len(), 1);
    }
}
#[test]
fn expiry_only_and_untimed_socket_receipts_do_not_invent_stamp_and_never_publish_after_close() {
    let mut engine = receipt_engine(true);
    let actions: Vec<_> = engine.drain_actions().collect();
    let (attempt, receipt) = receipt_attempt(&actions);
    assert_eq!(receipt.stamp_ms, None);
    assert_eq!(receipt.expiry_ms, Some(1020.));
    engine.handle(
        1001.,
        Event::Signed {
            attempt,
            result: with_receipt(receipt),
        },
    );
    engine.drain_actions().for_each(drop);
    assert_eq!(engine.venue.receipts[0].1, receipt);
    for close in [false, true] {
        let mut engine = crate::test_support::seeded_engine(TestVenue::default());
        let socket = upgrade(&mut engine, false);
        engine.prepare_dispatch(1000.);
        let actions: Vec<_> = engine.drain_actions().collect();
        let (attempt, receipt) = receipt_attempt(&actions);
        assert_eq!(receipt, Default::default());
        engine.handle(
            1001.,
            Event::Signed {
                attempt,
                result: with_receipt(receipt),
            },
        );
        assert!(engine.venue.receipts.is_empty());
        if close {
            let mut cx = Cx {
                now: 1001.,
                store: &mut engine.store,
                io: &mut engine.io,
            };
            cx.close(socket);
        }
        engine.drain_actions().for_each(drop);
        assert_eq!(engine.venue.receipts.len(), usize::from(!close));
        if !close {
            assert_eq!(
                engine.venue.receipts[0],
                (format!("socket:{}", socket.0), receipt)
            );
        }
    }
}
#[test]
fn receipt_bounds_and_provider_json_fail_closed_without_timing_unit_guessing() {
    use crate::signing::SignReceipt;
    for receipt in [
        SignReceipt {
            nonce: None,
            stamp_ms: Some(-1.),
            expiry_ms: None,
        },
        SignReceipt {
            nonce: None,
            stamp_ms: None,
            expiry_ms: Some(1.5),
        },
        SignReceipt {
            nonce: None,
            stamp_ms: None,
            expiry_ms: Some(f64::INFINITY),
        },
        SignReceipt {
            nonce: None,
            stamp_ms: Some(9_007_199_254_740_992.),
            expiry_ms: None,
        },
        SignReceipt {
            nonce: None,
            stamp_ms: Some(1000.),
            expiry_ms: Some(1000.),
        },
    ] {
        assert_eq!(receipt.validate(), Err(SignError::InvalidRequest));
    }
    assert!(serde_json::from_str::<SignResult>(r#"{"values":[[1]]}"#).is_err());
    assert!(
        serde_json::from_str::<SignResult>(r#"{"values":[[1]],"receipt":{"secret":"ignored?"}}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<SignResult>("[[1]]").is_err());
    assert_eq!(
        SignReceipt {
            nonce: None,
            stamp_ms: None,
            expiry_ms: Some(1_000_000.)
        }
        .validate(),
        Ok(())
    );
}

struct ExpiredStep;
impl crate::SignStep for ExpiredStep {
    fn prepare(
        &mut self,
        action: &mut Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let mut work = Step.prepare(action, context)?;
        work.receipt.expiry_ms = Some(context.now_ms);
        Ok(work)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        panic!("expired work materialized")
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}
struct NeverCalled;
impl crate::signing::Signer for NeverCalled {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        panic!("expired work reached immediate provider")
    }
    fn sign(&self, _: SignRequest) -> crate::signing::SignFuture {
        panic!("expired work reached async provider")
    }
}
#[test]
fn already_expired_preparation_fails_before_immediate_provider_instead_of_repreparing_forever() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine
        .set_signer(Some(std::sync::Arc::new(NeverCalled)))
        .unwrap();
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let request = cx.request(
        Method::Post,
        "https://venue.test/private".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(request, Box::new(ExpiredStep));
    engine.prepare_dispatch(1000.);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Sign { .. } | Action::Http(_)))
    );
    assert_eq!(engine.venue.failed, 1);
    assert!(engine.venue.receipts.is_empty());
    assert_eq!(
        engine.venue.last_sign_error.as_deref(),
        Some(SignError::InvalidRequest.venue_error().message.as_str())
    );
}

#[test]
fn wire_stamp_floors_after_fractional_calibration_without_changing_clock_domains() {
    let mut engine = engine();
    let mut cx = Cx {
        now: 1000.875,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert!(cx.observe_protocol_time(2001.125));
    assert_eq!(cx.protocol_now(), 2001.125);
    assert_eq!(cx.protocol_stamp_ms(), 2001.0);
    assert_eq!(cx.now, 1000.875);
    cx.now += 0.875;
    assert_eq!(cx.protocol_now(), 2002.0);
    assert_eq!(cx.protocol_stamp_ms(), 2002.0);
    let context = crate::DispatchContext::new(cx.protocol_now() + 0.875, None);
    assert_eq!(context.now_ms, 2002.875);
    assert_eq!(context.stamp_ms(), 2002.0);
    assert!(
        crate::signing::SignReceipt {
            nonce: None,
            stamp_ms: Some(context.stamp_ms()),
            expiry_ms: None
        }
        .validate()
        .is_ok()
    );
}

/// Answers per ref from a fixed table, like a provider reporting installed state.
struct Declares(Vec<(&'static str, crate::signing::Algorithm)>);
impl crate::signing::Signer for Declares {
    fn sign(&self, _: SignRequest) -> crate::signing::SignFuture {
        panic!("capability admission must never sign")
    }
    fn can_sign(
        &self,
        key: &crate::signing::KeyRef,
        algorithm: crate::signing::Algorithm,
    ) -> crate::signing::Capability {
        if self.0.contains(&(key.0.as_str(), algorithm)) {
            crate::signing::Capability::Supported
        } else {
            crate::signing::Capability::Unsupported
        }
    }
}
fn capability_identities() -> crate::signing::Identities {
    let key = |name: &str| crate::signing::KeyRef(name.into());
    crate::signing::Identities {
        primary: Some(
            crate::signing::Identity::new(key("api"), Some(key("hmac")))
                .with_extra([(crate::signing::ExtraKey::ZklinkSeed, key("seed"))].into()),
        ),
        secondary: None,
    }
}
fn can_sign_with(
    engine: &mut Engine<TestVenue>,
    slot: crate::CredentialSlot,
    needs: &[crate::signing::Need],
) -> Result<(), SignError> {
    let cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.can_sign(slot, needs)
}
const AMEND_NEEDS: [crate::signing::Need; 2] = [
    crate::signing::Need {
        key: crate::signing::KeyUse::Signing,
        algorithm: crate::signing::Algorithm::HmacSha256,
    },
    crate::signing::Need {
        key: crate::signing::KeyUse::Extra(crate::signing::ExtraKey::ZklinkSeed),
        algorithm: crate::signing::Algorithm::ZkLinkContract,
    },
];

#[test]
fn capability_admission_requires_every_need_supported_for_the_registered_slot() {
    use crate::CredentialSlot::{Primary, Secondary};
    use crate::signing::Algorithm::{HmacSha256, ZkLinkContract};
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine
        .set_signer(Some(std::sync::Arc::new(Declares(vec![
            ("hmac", HmacSha256),
            ("seed", ZkLinkContract),
        ]))))
        .unwrap();
    engine.set_identities(capability_identities()).unwrap();
    assert_eq!(can_sign_with(&mut engine, Primary, &AMEND_NEEDS), Ok(()));
    // An unregistered slot is refused; the venue's claim is never trusted.
    assert_eq!(
        can_sign_with(&mut engine, Secondary, &AMEND_NEEDS[..1]),
        Err(SignError::InvalidRequest)
    );

    let mut hmac_only = crate::test_support::seeded_engine(TestVenue::default());
    hmac_only
        .set_signer(Some(std::sync::Arc::new(Declares(vec![(
            "hmac", HmacSha256,
        )]))))
        .unwrap();
    hmac_only.set_identities(capability_identities()).unwrap();
    assert_eq!(
        can_sign_with(&mut hmac_only, Primary, &AMEND_NEEDS[..1]),
        Ok(())
    );
    assert_eq!(
        can_sign_with(&mut hmac_only, Primary, &AMEND_NEEDS),
        Err(SignError::Unsupported)
    );
    assert_eq!(
        SignError::Unsupported.venue_error().outcome,
        Some(crate::Outcome::NotSent)
    );
}

#[test]
fn capability_admission_fails_closed_for_missing_refs_and_undeclared_hosts() {
    use crate::CredentialSlot::Primary;
    use crate::signing::Algorithm::HmacSha256;
    let mut identities = capability_identities();
    identities.primary.as_mut().unwrap().extra.clear();
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine.set_identities(identities).unwrap();
    // No Rust provider: the host fulfils Action::Sign and declared nothing.
    assert_eq!(
        can_sign_with(&mut engine, Primary, &AMEND_NEEDS[..1]),
        Err(SignError::Unsupported)
    );
    assert_eq!(
        can_sign_with(&mut engine, Primary, &AMEND_NEEDS),
        Err(SignError::InvalidRequest)
    );
    engine.set_host_algorithms(Some(vec![HmacSha256])).unwrap();
    assert_eq!(
        can_sign_with(&mut engine, Primary, &AMEND_NEEDS[..1]),
        Ok(())
    );
    // A later undeclared install clears the earlier declaration.
    engine.set_host_algorithms(None).unwrap();
    assert_eq!(
        can_sign_with(&mut engine, Primary, &AMEND_NEEDS[..1]),
        Err(SignError::Unsupported)
    );

    let mut provider_default = crate::test_support::seeded_engine(TestVenue::default());
    provider_default
        .set_signer(Some(std::sync::Arc::new(Ready)))
        .unwrap();
    provider_default
        .set_identities(capability_identities())
        .unwrap();
    assert_eq!(
        can_sign_with(&mut provider_default, Primary, &AMEND_NEEDS[..1]),
        Err(SignError::Unsupported)
    );

    let mut started = crate::test_support::seeded_engine(TestVenue::default());
    started.start(1000.);
    assert!(started.set_identities(capability_identities()).is_err());
    assert!(started.set_host_algorithms(Some(vec![HmacSha256])).is_err());
}

/// D4: an absent signing ref refuses admission as MissingKey; a missing extra
/// ref stays a malformed InvalidRequest in either need order.
#[test]
fn capability_admission_refuses_an_absent_signing_ref_as_missing_key() {
    use crate::CredentialSlot::Primary;
    let key = |name: &str| crate::signing::KeyRef(name.into());
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine
        .set_identities(crate::signing::Identities {
            primary: Some(crate::signing::Identity::new(key("api"), None)),
            secondary: None,
        })
        .unwrap();
    assert_eq!(
        can_sign_with(&mut engine, Primary, &AMEND_NEEDS[..1]),
        Err(SignError::MissingKey)
    );
    let reversed = [AMEND_NEEDS[1], AMEND_NEEDS[0]];
    for needs in [&AMEND_NEEDS[..], &reversed[..]] {
        assert_eq!(
            can_sign_with(&mut engine, Primary, needs),
            Err(SignError::InvalidRequest)
        );
    }
}

/// Admitted everything, then lost capability after its first signature.
#[derive(Default)]
struct CapabilityLostAfterFirst(std::sync::atomic::AtomicUsize);
impl crate::signing::Signer for CapabilityLostAfterFirst {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, request: SignRequest) -> crate::signing::SignFuture {
        let result = self.sign_ready(&request).unwrap();
        Box::pin(std::future::ready(result))
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        let signed = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Some(if signed == 0 {
            result()
        } else {
            Err(SignError::Refused)
        })
    }
    fn can_sign(
        &self,
        _: &crate::signing::KeyRef,
        _: crate::signing::Algorithm,
    ) -> crate::signing::Capability {
        crate::signing::Capability::Supported
    }
}

#[test]
fn admission_is_evidence_only_release_still_validates_and_earlier_effects_stand() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine
        .set_signer(Some(std::sync::Arc::new(
            CapabilityLostAfterFirst::default(),
        )))
        .unwrap();
    engine.set_identities(capability_identities()).unwrap();
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert_eq!(
        cx.can_sign(crate::CredentialSlot::Primary, &AMEND_NEEDS),
        Ok(())
    );
    let cancel = cx.request(
        Method::Post,
        "https://venue.test/cancel".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(cancel, Box::new(Step));
    let replace = cx.request(
        Method::Post,
        "https://venue.test/replace".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(replace, Box::new(Step));
    let released: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(http) => Some(http.id),
            _ => None,
        })
        .collect();
    // The first release went out; the later refusal fails only its own request.
    assert_eq!(released, [cancel]);
    assert_eq!(engine.venue.failed, 1);
}

#[test]
fn public_operations_validate_refs_algorithms_and_exact_result_widths() {
    let key = || KeyRef("primary.signing".into());
    let request = |operations| SignRequest {
        receipt: Default::default(),
        operations,
        context: Default::default(),
    };
    let public = request(vec![
        Operation::PublicAddress { key: key() },
        Operation::PublicKey {
            algorithm: Algorithm::Ed25519,
            key: key(),
        },
        Operation::PublicKey {
            algorithm: Algorithm::StarkDigest,
            key: key(),
        },
    ]);
    assert_eq!(public.validate(), Ok(()));
    let answer = |widths: [usize; 3]| {
        SignResult::new(
            widths.map(|width| vec![1; width]).to_vec(),
            Default::default(),
        )
    };
    assert_eq!(public.validate_result(&answer([20, 32, 32])), Ok(()));
    for widths in [[32, 32, 32], [20, 33, 32], [20, 32, 31]] {
        assert_eq!(
            public.validate_result(&answer(widths)),
            Err(SignError::InvalidResult)
        );
    }
    for operation in [
        Operation::PublicAddress {
            key: KeyRef(String::new()),
        },
        Operation::PublicKey {
            algorithm: Algorithm::Ed25519,
            key: KeyRef(String::new()),
        },
        Operation::PublicKey {
            algorithm: Algorithm::Es256,
            key: key(),
        },
        Operation::PublicKey {
            algorithm: Algorithm::Secp256k1Digest,
            key: key(),
        },
        Operation::PublicKey {
            algorithm: Algorithm::HmacSha256,
            key: key(),
        },
    ] {
        assert_eq!(
            request(vec![operation]).validate(),
            Err(SignError::InvalidRequest)
        );
    }
}

#[test]
fn l1_wallet_role_is_primary_only_for_injected_refs_and_policies() {
    use crate::signing::{
        CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy, Identities, Identity,
        IdentitySetupPolicy,
    };
    let wallet = Some(IdentitySetupPolicy {
        extra: Some(ExtraKeyPolicy {
            key: ExtraKey::L1Wallet,
            field: ExtraFieldPolicy::RequiredForPrivate,
            encoding: None,
        }),
        ..Default::default()
    });
    let identity = |extra: bool| {
        Identity::new(KeyRef("credential".into()), Some(KeyRef("signing".into()))).with_extra(
            extra
                .then(|| (ExtraKey::L1Wallet, KeyRef("wallet".into())))
                .into_iter()
                .collect(),
        )
    };
    // The role's one algorithm is the L1 digest signature.
    assert_eq!(ExtraKey::L1Wallet.algorithm(), Algorithm::Secp256k1Digest);
    assert_eq!(ExtraKey::ZklinkSeed.algorithm(), Algorithm::ZkLinkContract);
    let primary = CredentialSetupPolicy {
        primary: wallet,
        secondary: None,
    };
    let mut ok = Identities {
        primary: Some(identity(true)),
        secondary: None,
    };
    assert_eq!(primary.apply_to_identities(&mut ok, false), Ok(()));
    // A secondary declaration is invalid policy, even with no secondary identity.
    let secondary = CredentialSetupPolicy {
        primary: None,
        secondary: wallet,
    };
    for secondary_identity in [None, Some(identity(true))] {
        let mut ids = Identities {
            primary: Some(identity(false)),
            secondary: secondary_identity,
        };
        assert_eq!(
            secondary.apply_to_identities(&mut ids, false),
            Err(SignError::InvalidRequest)
        );
    }
    // An injected secondary wallet ref without any declaration is refused too.
    let mut injected = Identities {
        primary: Some(identity(true)),
        secondary: Some(identity(true)),
    };
    assert_eq!(
        primary.apply_to_identities(&mut injected, false),
        Err(SignError::InvalidRequest)
    );
}

/// The local signer, which reports every ref as missing.
struct MissingKeyLocal;
impl crate::signing::Signer for MissingKeyLocal {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, _: SignRequest) -> crate::signing::SignFuture {
        Box::pin(std::future::ready(Err(SignError::MissingKey)))
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Err(SignError::MissingKey))
    }
}

fn latched(actions: impl Iterator<Item = Action>) -> bool {
    actions.into_iter().any(|action| {
        matches!(action, Action::Notice(Notice::Event(LogEvent { code, .. }))
            if code == log_code::CONFIG_INVALID)
    })
}

/// A secondary read slot's missing key fails that request only; the same
/// refusal on a primary ref latches signing for the account.
#[test]
fn only_a_primary_ref_refusal_latches_while_a_secondary_exists() {
    let identity = |credential: &str, signing: &str| {
        crate::signing::Identity::new(KeyRef(credential.into()), Some(KeyRef(signing.into())))
    };
    let refused = |key: &'static str| {
        let mut engine = crate::test_support::seeded_engine(TestVenue {
            key: Some(key),
            ..Default::default()
        });
        engine
            .set_identities(crate::signing::Identities {
                primary: Some(identity("account/credential", "account/primary")),
                secondary: Some(identity("account/read-credential", "account/read")),
            })
            .unwrap();
        engine
            .set_signer(Some(std::sync::Arc::new(MissingKeyLocal)))
            .unwrap();
        engine.start(1000.);
        let latched = latched(engine.drain_actions().collect::<Vec<_>>().into_iter());
        assert_eq!(
            engine.venue.failed, 1,
            "the request itself fails either way"
        );
        latched
    };
    assert!(
        !refused("account/read"),
        "a secondary refusal never latches"
    );
    assert!(refused("account/primary"), "a primary refusal latches");
}

/// No host reports `MissingKey`, but if one did asynchronously, only that
/// request fails: nothing latches, and the next signed request still stages.
#[test]
fn an_asynchronous_missing_key_fails_its_request_without_latching() {
    let mut engine = engine();
    let (attempt, _) = staged(&mut engine);
    engine.handle(
        1001.,
        Event::Signed {
            attempt,
            result: Err(SignError::MissingKey),
        },
    );
    assert_eq!(engine.venue.failed, 1, "the request fails, NotSent");
    assert!(!latched(
        engine.drain_actions().collect::<Vec<_>>().into_iter()
    ));
    let mut cx = Cx {
        now: 1002.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let request = cx.request(
        Method::Post,
        "https://venue.test/private".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(request, Box::new(Step));
    cx.route_request(request, RequestRoute::Primary);
    engine.prepare_dispatch(1002.);
    assert!(
        engine
            .drain_actions()
            .any(|action| matches!(action, Action::Sign { .. })),
        "signing is not latched"
    );
}

/// A socket login whose key is missing fails identically on every reconnect,
/// so it latches and the private role is never reconnected. Any other
/// preparation refusal stays a per-request failure.
#[test]
fn a_socket_login_missing_its_key_latches_and_blocks_its_role() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let private =
        || crate::SocketSpec::new("wss://venue.test/private").channel(SocketChannel::Private);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let socket = cx.connect(private());
    cx.send_signed(socket, "auth".into(), Box::new(MissingKeyStep));
    engine.prepare_dispatch(1000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|action| {
        matches!(action, Action::Notice(Notice::Event(LogEvent { code, .. }))
            if *code == log_code::CONFIG_INVALID)
    }));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    let mut cx = Cx {
        now: 1001.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert_eq!(
        cx.connect(private()),
        SocketId(0),
        "the role never reconnects"
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

/// C4s: a credential-only identity's socket login takes its key through
/// `signing_ref()`; the absent ref latches like any missing key: one visible
/// failure, `start` fails, and the private role never reconnects.
#[test]
fn c4s_a_credential_only_socket_login_latches_through_signing_ref() {
    struct IdentityStep(crate::signing::Identity);
    impl crate::SignStep for IdentityStep {
        fn prepare(
            &mut self,
            _: &mut Action,
            cx: &crate::DispatchContext<'_>,
        ) -> Result<SignRequest, SignError> {
            let key = self.0.signing_ref()?.clone();
            Ok(SignRequest {
                receipt: crate::signing::SignReceipt {
                    stamp_ms: Some(cx.stamp_ms()),
                    expiry_ms: Some(cx.stamp_ms() + 5000.),
                    ..Default::default()
                },
                context: Default::default(),
                operations: vec![crate::signing::Operation::Sign {
                    algorithm: crate::signing::Algorithm::HmacSha256,
                    key,
                    payload: vec![crate::signing::PayloadPart::Bytes {
                        bytes: b"login".to_vec(),
                    }],
                }],
            })
        }
        fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
            unreachable!("an absent signing ref never completes")
        }
        fn freshness_ms(&self) -> f64 {
            5000.
        }
    }
    let identity = crate::signing::Identity::new(KeyRef("primary.credential".into()), None);
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    engine
        .set_identities(crate::signing::Identities {
            primary: Some(identity.clone()),
            secondary: None,
        })
        .unwrap();
    engine.startup_watch_at = Some(2000.);
    let private =
        || crate::SocketSpec::new("wss://venue.test/private").channel(SocketChannel::Private);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let socket = cx.connect(private());
    cx.send_signed(socket, "auth".into(), Box::new(IdentityStep(identity)));
    engine.prepare_dispatch(1000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    let config_invalid = actions
        .iter()
        .filter(|action| {
            matches!(action, Action::Notice(Notice::Event(LogEvent { code, .. }))
                if *code == log_code::CONFIG_INVALID)
        })
        .count();
    assert_eq!(config_invalid, 1, "{actions:?}");
    let startup_failed: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::StartupFailed { error }) => Some(error.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(startup_failed, vec![SignError::MissingKey.venue_error()]);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::WsSend { .. })),
        "refused before provider work or send: {actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    let mut cx = Cx {
        now: 1001.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert_eq!(cx.connect(private()), SocketId(0), "no second connection");
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

/// A signed upgrade missing its key latches before the socket ever opens, and
/// its role is still never reconnected.
#[test]
fn a_signed_connect_missing_its_key_blocks_its_role() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let private =
        || crate::SocketSpec::new("wss://venue.test/private").channel(SocketChannel::Private);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.connect_signed(private(), Box::new(MissingKeyStep));
    engine.prepare_dispatch(1000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|action| {
        matches!(action, Action::Notice(Notice::Event(LogEvent { code, .. }))
            if *code == log_code::CONFIG_INVALID)
    }));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let mut cx = Cx {
        now: 1001.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert_eq!(cx.connect_signed(private(), Box::new(Step)), SocketId(0));
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::Sign { .. }))
    );
}

/// An invalid request shape fails that request only, on a socket login too.
#[test]
fn a_socket_login_with_an_invalid_shape_never_latches() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let private =
        || crate::SocketSpec::new("wss://venue.test/private").channel(SocketChannel::Private);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let socket = cx.connect(private());
    cx.send_signed(socket, "auth".into(), Box::new(InvalidStep));
    engine.prepare_dispatch(1000.);
    assert!(!engine.drain_actions().any(|action| {
        matches!(action, Action::Notice(Notice::Event(LogEvent { code, .. }))
            if code == log_code::CONFIG_INVALID)
    }));
    let mut cx = Cx {
        now: 1001.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert_ne!(
        cx.connect(private()),
        SocketId(0),
        "the role reconnects as before"
    );
}

/// An unsigned frame held behind a socket's queued login is never refused by
/// a latch that fired elsewhere; the socket stays open.
#[test]
fn a_latch_never_refuses_an_unsigned_frame_held_behind_a_login() {
    let mut engine = crate::test_support::seeded_engine(TestVenue::default());
    let private =
        || crate::SocketSpec::new("wss://venue.test/private").channel(SocketChannel::Private);
    let mut cx = Cx {
        now: 1000.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    // Signing is serialized: the request is the head, the login waits.
    let request = cx.request(
        Method::Post,
        "https://venue.test/private".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(request, Box::new(MissingKeyStep));
    cx.route_request(request, RequestRoute::Primary);
    let socket = cx.connect(private());
    cx.send_signed(socket, "auth".into(), Box::new(Step));
    engine.prepare_dispatch(1000.);
    assert!(latched(
        engine.drain_actions().collect::<Vec<_>>().into_iter()
    ));
    let mut cx = Cx {
        now: 1002.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.send(socket, "subscribe");
    engine.prepare_dispatch(1002.);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsClose { socket: s } if s == socket)),
        "the held frame does not tear the socket down"
    );
}

/// The latch notice is the text the engine formatted before it became static.
#[test]
fn the_missing_key_notice_is_the_formatted_text() {
    assert_eq!(
        MISSING_KEY_NOTICE,
        format!(
            "Signing cannot succeed with the installed credentials: {}",
            SignError::MissingKey
        )
    );
}

/// SchnorrDigest has a public form: a lone PublicKey validates and its answer
/// is exactly 40 bytes.
#[test]
fn schnorr_public_key_requests_validate_with_a_forty_byte_answer() {
    let request = SignRequest {
        receipt: Default::default(),
        operations: vec![Operation::PublicKey {
            algorithm: Algorithm::SchnorrDigest,
            key: KeyRef("primary.signing".into()),
        }],
        context: Default::default(),
    };
    assert_eq!(request.validate(), Ok(()));
    let answer = |width| SignResult::new(vec![vec![1; width]], Default::default());
    assert_eq!(request.validate_result(&answer(40)), Ok(()));
    for width in [32, 39, 41, 80] {
        assert_eq!(
            request.validate_result(&answer(width)),
            Err(SignError::InvalidResult)
        );
    }
}
