use super::*;
use crate::signing::{
    Algorithm, Context, KeyRef, Operation, PayloadPart, SignError, SignRequest, SignResult,
};
struct Step;
impl crate::SignStep for Step {
    fn prepare(&mut self, action: &mut Action, now: f64) -> Result<SignRequest, SignError> {
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
        cx.sign_http(request, Box::new(Step));
        cx.route_request(request, RequestRoute::Primary);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
        panic!("not a network failure");
    }
    fn on_sign_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
        self.failed += 1;
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
