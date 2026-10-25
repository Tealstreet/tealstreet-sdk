use super::*;
use crate::request_context::{Claim, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope};
use crate::socket::Config;
use crate::socket_lifecycle::{AuthMode, AuthRecovery, Invalidation, TrafficPolicy};

#[derive(Default)]
struct Probe {
    feed: Option<FeedId>,
    calls: Vec<usize>,
    pace: f64,
    managed: bool,
    retire: bool,
    replace: bool,
    fatal: bool,
    auth: bool,
    repeat: bool,
}
impl Venue for Probe {
    type Pending = usize;
    fn start(&mut self, cx: &mut Cx<'_, usize>) {
        let feed = cx
            .register_socket_feed(FeedPolicy {
                role: SocketChannel::Public,
                supervision: Config {
                    ping_interval_ms: None,
                    idle_timeout_ms: 20_000.,
                    auth_timeout_ms: self.auth.then_some(10_000.),
                    ..Config::default()
                },
                auth: if self.auth {
                    AuthMode::Required
                } else {
                    AuthMode::None
                },
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: if self.auth {
                    ControlPolicy::Native
                } else {
                    ControlPolicy::None
                },
                invalidation: Invalidation::None,
                subscription_interval_ms: self.pace,
            })
            .unwrap();
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://fixture.invalid").unwrap();
    }
    fn socket_control(&self, _: &Store, _: FeedId, _: bool, text: &str) -> Control {
        if text == "accepted" {
            Control::Auth(crate::socket_lifecycle::AuthEvidence::Accepted)
        } else {
            Control::Other
        }
    }
    fn on_socket_event(
        &mut self,
        _: &mut Cx<'_, usize>,
        _: LifecycleEvent,
    ) -> Result<(), SocketError> {
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        _: &Store,
        _: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for item in items {
            if item.wire_args.as_ref() == "refuse" {
                return Err(SocketError::InvalidCoverage);
            }
            emit(EncodedBatch {
                text: format!("{subscribe}:{}", item.wire_args),
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, usize>,
        epoch: Epoch,
        pending: usize,
    ) -> Result<(), SocketError> {
        self.calls.push(pending);
        if self.repeat && pending < 2 {
            cx.set_socket_interests_then(
                epoch.feed(),
                vec![interest("book")],
                &["book"],
                pending + 1,
            )?;
        }
        assert!(cx.io.http_delivery.is_none());
        assert!(cx.io.command_entry.is_none());
        #[cfg(feature = "order-locks")]
        assert!(cx.io.order_response_token.is_none());
        #[cfg(feature = "http-plans")]
        assert!(cx.io.http_plan_ready.is_none());
        #[cfg(feature = "http-plans")]
        assert!(cx.io.http_plan_expiring.is_none());
        if self.retire {
            cx.stop_socket_feed(epoch.feed());
        }
        if self.replace {
            cx.set_socket_interests(epoch.feed(), vec![interest("replacement")])?;
        }
        if self.fatal {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cx.io.next_id.invalidate();
            }));
        }
        if self.managed {
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 1000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_managed(
                context,
                None,
                Method::Get,
                "https://fixture.invalid/seed".into(),
                None,
                pending,
            )
            .unwrap();
        }
        Ok(())
    }
    fn on_open(&mut self, _: &mut Cx<'_, usize>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, usize>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, usize>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, usize>, _: usize, _: Response<'_>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, usize>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, usize>, _: Command) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, usize>, _: usize, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn interest(key: &str) -> Interest {
    Interest::from_wire_args(key)
}
fn started(probe: Probe) -> (Engine<Probe>, Epoch) {
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    let epoch = engine
        .io
        .socket_driver
        .state
        .epoch(engine.venue.feed.unwrap())
        .unwrap();
    engine.handle(
        1.,
        Event::WsOpened {
            socket: epoch.socket(),
        },
    );
    engine.drain_actions().for_each(drop);
    (engine, epoch)
}
fn cx(engine: &mut Engine<Probe>) -> Cx<'_, usize> {
    Cx {
        now: engine.io.dispatch_now,
        store: &mut engine.store,
        io: &mut engine.io,
    }
}
fn queue(
    engine: &mut Engine<Probe>,
    values: &[&str],
    required: &[&str],
    payload: usize,
) -> Result<u64, SocketError> {
    let feed = engine.venue.feed.unwrap();
    cx(engine).set_socket_interests_then(
        feed,
        values.iter().map(|key| interest(key)).collect(),
        required,
        payload,
    )
}
fn replace(engine: &mut Engine<Probe>, values: &[&str]) {
    let feed = engine.venue.feed.unwrap();
    cx(engine)
        .set_socket_interests(feed, values.iter().map(|key| interest(key)).collect())
        .unwrap();
}
fn wake(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("sealed handoff wakeup")
}
fn no_http(actions: &[Action]) -> bool {
    !actions
        .iter()
        .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
}

#[test]
fn managed_read_waits_for_returned_subscription_batch_then_prepares_binds_and_claims() {
    let (mut e, _) = started(Probe {
        managed: true,
        ..Default::default()
    });
    queue(&mut e, &["book"], &["book"], 7).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 1.);
    let frame = out
        .iter()
        .position(|a| matches!(a, Action::WsSend { .. }))
        .unwrap();
    let arm = out
        .iter()
        .position(|a| matches!(a, Action::SetTimer { timer: t, .. } if *t == timer))
        .unwrap();
    assert!(frame < arm);
    assert!(no_http(&out));
    assert!(e.venue.calls.is_empty());
    e.handle(1., Event::TimerFired { timer });
    let token = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        2.,
        token,
        Prepared {
            id: 17,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(e.claim_http(3., permit), Ok(Claim::Released(_))));
    e.handle(3., Event::TimerFired { timer });
    assert_eq!(e.venue.calls, [7]);
    assert!(e.io.socket_continuations.is_empty());
    assert_eq!(e.io.socket_continuation_bytes, 0);
}

#[test]
fn retirement_before_final_drain_discards_seed_and_fences_its_frames() {
    let (mut e, epoch) = started(Probe {
        managed: true,
        ..Default::default()
    });
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    e.process_socket_work(1.);
    cx(&mut e).stop_socket_feed(epoch.feed());
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|a| matches!(a, Action::WsSend { .. })));
    assert!(no_http(&out));
    assert!(e.io.socket_continuations.is_empty());
    assert_eq!(e.io.socket_continuation_bytes, 0);
}

#[test]
fn retired_or_expired_epoch_never_wakes_a_native_read() {
    for expired in [false, true] {
        let (mut e, epoch) = started(Probe {
            managed: true,
            ..Default::default()
        });
        queue(&mut e, &["book"], &["book"], 1).unwrap();
        let out: Vec<_> = e.drain_actions().collect();
        let timer = wake(&out, 1.);
        if !expired {
            e.handle(
                2.,
                Event::WsClosed {
                    socket: epoch.socket(),
                    code: 1006,
                },
            );
        }
        e.handle(
            if expired { 20_001. } else { 3. },
            Event::TimerFired { timer },
        );
        assert!(no_http(&e.drain_actions().collect::<Vec<_>>()));
        assert!(e.venue.calls.is_empty());
        assert!(e.io.socket_continuations.is_empty());
    }
}

#[test]
fn refused_replacement_and_repair_preserve_committed_work() {
    for repair in [false, true] {
        let (mut e, epoch) = started(Probe::default());
        queue(&mut e, &["book"], &["book"], 1).unwrap();
        let first: Vec<_> = e.drain_actions().collect();
        let timer = wake(&first, 1.);
        if repair {
            cx(&mut e)
                .repair_socket_interests(epoch.feed(), vec!["missing".into()])
                .unwrap();
        } else {
            replace(&mut e, &["refuse"]);
        }
        e.drain_actions().for_each(drop);
        e.handle(1., Event::TimerFired { timer });
        assert_eq!(e.venue.calls, [1]);
    }
}

#[test]
fn additive_replacement_rearms_with_a_fresh_identity_and_old_wakeup_is_inert() {
    let (mut e, _) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let first: Vec<_> = e.drain_actions().collect();
    let old = wake(&first, 1.);
    replace(&mut e, &["book", "ticker"]);
    let second: Vec<_> = e.drain_actions().collect();
    let new = wake(&second, 1.);
    assert_ne!(old, new);
    e.handle(1., Event::TimerFired { timer: old });
    assert!(e.venue.calls.is_empty());
    e.handle(1., Event::TimerFired { timer: new });
    assert_eq!(e.venue.calls, [1]);
}

#[test]
fn committed_remove_and_readd_cannot_reanimate_cancelled_work() {
    let (mut e, _) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let first: Vec<_> = e.drain_actions().collect();
    let old = wake(&first, 1.);
    replace(&mut e, &[]);
    replace(&mut e, &["book"]);
    e.drain_actions().for_each(drop);
    e.handle(1., Event::TimerFired { timer: old });
    assert!(e.venue.calls.is_empty());
    assert!(e.io.socket_continuations.is_empty());
}

#[test]
fn paced_batches_wait_for_the_last_subscription_frame() {
    let (mut e, _) = started(Probe {
        pace: 100.,
        ..Default::default()
    });
    queue(&mut e, &["a", "b"], &["b"], 1).unwrap();
    let first: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        first
            .iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        1
    );
    assert!(
        !first
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 1.))
    );
    let timer = wake(&first, 101.);
    e.handle(101., Event::TimerFired { timer });
    let last: Vec<_> = e.drain_actions().collect();
    let handoff = wake(&last, 101.);
    assert!(e.venue.calls.is_empty());
    e.handle(101., Event::TimerFired { timer: handoff });
    assert_eq!(e.venue.calls, [1]);
}

#[test]
fn admission_refuses_duplicates_missing_requirements_and_capacity_atomically() {
    let (mut e, _) = started(Probe::default());
    let ids = e.io.next_id;
    assert_eq!(
        queue(&mut e, &["book"], &["missing"], 1),
        Err(SocketError::InvalidInterest)
    );
    assert_eq!(
        queue(&mut e, &["book"], &["book", "book"], 1),
        Err(SocketError::InvalidInterest)
    );
    assert_eq!(e.io.next_id, ids);
    e.io.next_id =
        crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID + 1);
    assert_eq!(
        queue(&mut e, &["book"], &["book"], 1),
        Err(SocketError::Capacity)
    );
    assert!(e.io.socket_proposals.is_empty());
    assert!(e.io.socket_continuations.is_empty());
    assert_eq!(e.io.socket_proposal_bytes, 0);
}

#[test]
fn callback_retirement_or_replacement_cannot_fence_the_prior_returned_batch() {
    for retire in [false, true] {
        let (mut e, _) = started(Probe {
            managed: true,
            retire,
            replace: !retire,
            ..Default::default()
        });
        queue(&mut e, &["book"], &["book"], 1).unwrap();
        let prior: Vec<_> = e.drain_actions().collect();
        let timer = wake(&prior, 1.);
        e.handle(1., Event::TimerFired { timer });
        let next: Vec<_> = e.drain_actions().collect();
        assert!(
            prior
                .iter()
                .any(|a| matches!(a, Action::WsSend { text, .. } if text == "true:book"))
        );
        assert!(next.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. }))));
        assert_eq!(e.venue.calls, [1]);
    }
}

#[test]
fn caught_fatal_in_continuation_refuses_every_later_handle_and_drain() {
    let (mut e, _) = started(Probe {
        fatal: true,
        ..Default::default()
    });
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 1.);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || e.handle(1., Event::TimerFired { timer })
        ))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| e
            .drain_actions()
            .for_each(drop)))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || e.handle(2., Event::TimerFired { timer })
        ))
        .is_err()
    );
    assert_eq!(e.venue.calls, [1]);
}

#[test]
fn no_op_convergence_and_repair_still_require_a_new_engine_ingress() {
    let (mut e, epoch) = started(Probe::default());
    replace(&mut e, &["book"]);
    e.drain_actions().for_each(drop);
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|a| matches!(a, Action::WsSend { .. })));
    let old = wake(&out, 1.);
    cx(&mut e)
        .repair_socket_interests(epoch.feed(), vec!["book".into()])
        .unwrap();
    let repaired: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        repaired
            .iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        2
    );
    let new = wake(&repaired, 1.);
    e.handle(1., Event::TimerFired { timer: old });
    assert!(e.venue.calls.is_empty());
    e.handle(1., Event::TimerFired { timer: new });
    assert_eq!(e.venue.calls, [1]);
}

#[test]
fn changed_canonical_interest_cancels_work_even_when_its_key_survives() {
    let (mut e, epoch) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 1.);
    cx(&mut e)
        .set_socket_interests(
            epoch.feed(),
            vec![Interest {
                key: "book".into(),
                wire_args: "changed".into(),
            }],
        )
        .unwrap();
    e.drain_actions().for_each(drop);
    e.handle(1., Event::TimerFired { timer });
    assert!(e.venue.calls.is_empty());
    assert_eq!(e.io.socket_continuation_bytes, 0);
}

#[test]
fn pending_credit_cap_and_last_safe_timer_refuse_without_aliasing() {
    let (mut e, _) = started(Probe::default());
    for id in 0..MAX_PROPOSALS {
        queue(&mut e, &["book"], &["book"], id).unwrap();
    }
    let ids = e.io.next_id;
    assert_eq!(
        queue(&mut e, &["book"], &["book"], 9),
        Err(SocketError::Capacity)
    );
    assert_eq!(e.io.next_id, ids);
    assert_eq!(e.io.socket_proposals.len(), MAX_PROPOSALS);

    let (mut e, _) = started(Probe::default());
    e.io.next_id =
        crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID);
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let first: Vec<_> = e.drain_actions().collect();
    let old = wake(&first, 1.);
    assert_eq!(old.0, crate::resource_ids::MAX_RESOURCE_ID);
    replace(&mut e, &["book", "ticker"]);
    let out: Vec<_> = e.drain_actions().collect();
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 1.))
    );
    e.handle(1., Event::TimerFired { timer: old });
    assert!(e.venue.calls.is_empty());
    assert!(e.io.socket_continuations.is_empty());
    assert!(!e.io.next_id.failed());
}

#[test]
fn continuation_clears_and_restores_ambient_delivery() {
    use crate::request_context::{Attempt, Context, Owner};
    let (mut e, _) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 1.);
    let context = Context::new(
        Owner::Command {
            id: 7,
            generation: 19,
        },
        3,
        Operation::Read,
        None,
        0.,
        5_000.,
        None,
    )
    .unwrap();
    e.io.http_delivery = Some(request_context_registry::Delivery {
        context,
        attempt: Attempt {
            request: RequestId(42),
            generation: 1,
        },
    });
    assert!(e.handle_socket_continuation(1., timer));
    assert_eq!(e.venue.calls, [1]);
    assert_eq!(
        e.io.http_delivery.as_ref().unwrap().attempt.request,
        RequestId(42)
    );
    assert!(e.io.command_entry.is_none());
}

#[test]
fn active_command_callback_cannot_reenter_the_timer_continuation() {
    let (mut e, _) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 1.);
    e.io.begin_command_entry(Some(7));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || e.handle(1., Event::TimerFired { timer })
        ))
        .is_err()
    );
    assert!(e.venue.calls.is_empty());
}

#[test]
fn authentication_and_callback_registration_cannot_skip_the_next_ingress() {
    let (mut e, epoch) = started(Probe {
        auth: true,
        repeat: true,
        ..Default::default()
    });
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    let waiting: Vec<_> = e.drain_actions().collect();
    assert!(!waiting.iter().any(|a| matches!(a, Action::WsSend { .. })));
    assert!(
        !waiting
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 1.))
    );
    e.handle(
        2.,
        Event::WsMessage {
            socket: epoch.socket(),
            text: "accepted",
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    let first = wake(&out, 2.);
    e.handle(2., Event::TimerFired { timer: first });
    assert_eq!(e.venue.calls, [1]);
    let out: Vec<_> = e.drain_actions().collect();
    let next = wake(&out, 2.);
    assert_ne!(first, next);
    e.handle(2., Event::TimerFired { timer: next });
    assert_eq!(e.venue.calls, [1, 2]);
}

struct ParkedStep;
impl crate::SignStep for ParkedStep {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
        use crate::signing::{Algorithm, Context, KeyRef, Operation, PayloadPart, SignRequest};
        Ok(SignRequest {
            receipt: Default::default(),
            context: Context::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("ref:fixture".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"auth".to_vec(),
                }],
            }],
        })
    }
    fn apply(
        &mut self,
        _: &mut Action,
        _: &crate::signing::SignResult,
    ) -> Result<(), crate::signing::SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

#[test]
fn delayed_signer_blocks_frames_and_wakeup_until_the_real_signing_fifo_releases() {
    let (mut e, epoch) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 1).unwrap();
    cx(&mut e).send_signed(epoch.socket(), "login".into(), Box::new(ParkedStep));
    let out: Vec<_> = e.drain_actions().collect();
    let (attempt, receipt) = out
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.receipt)),
            _ => None,
        })
        .unwrap();
    assert!(!out.iter().any(|a| matches!(a, Action::WsSend { .. })));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 1.))
    );
    e.handle(
        2.,
        Event::Signed {
            attempt,
            result: Ok(crate::signing::SignResult::new(vec![vec![0; 32]], receipt)),
        },
    );
    let released: Vec<_> = e.drain_actions().collect();
    let timer = wake(&released, 2.);
    let auth = released
        .iter()
        .position(|a| matches!(a, Action::WsSend { text, .. } if text == "login"))
        .unwrap();
    let sub = released
        .iter()
        .position(|a| matches!(a, Action::WsSend { text, .. } if text == "true:book"))
        .unwrap();
    assert!(auth < sub);
    e.handle(2., Event::TimerFired { timer });
    assert_eq!(e.venue.calls, [1]);
}

#[test]
fn early_one_shot_wakeup_rearms_without_early_callback_or_retained_credit() {
    let (mut e, _) = started(Probe::default());
    queue(&mut e, &["book"], &["book"], 7).unwrap();
    let out: Vec<_> = e.drain_actions().collect();
    let timer = wake(&out, 1.);
    assert!(e.io.socket_continuation_bytes > 0);
    e.handle(0.5, Event::TimerFired { timer });
    let rearmed: Vec<_> = e.drain_actions().collect();
    assert!(e.venue.calls.is_empty());
    assert_eq!(wake(&rearmed, 1.), timer);
    assert_eq!(e.io.socket_continuations.len(), 1);
    e.handle(1., Event::TimerFired { timer });
    e.drain_actions().for_each(drop);
    assert_eq!(e.venue.calls, [7]);
    assert!(e.io.socket_continuations.is_empty());
    assert_eq!(e.io.socket_continuation_bytes, 0);
    assert!(!e.io.timers.contains_key(&timer));
    e.handle(1., Event::TimerFired { timer });
    e.drain_actions().for_each(drop);
    assert_eq!(e.venue.calls, [7]);
}

#[cfg(feature = "http-plans")]
#[test]
fn expiry_cleanup_barrier_is_never_borrowed_by_a_socket_continuation() {
    use crate::request_context::{Plan, Preflight};
    for already_armed in [false, true] {
        let (mut e, _) = started(Probe::default());
        queue(&mut e, &["book"], &["book"], 7).unwrap();
        let previous = already_armed.then(|| wake(&e.drain_actions().collect::<Vec<_>>(), 1.));
        let barrier = Plan::new(&Preflight::new(&std::sync::Arc::new(()), 99, 1), 100.);
        e.io.http_plan_expiring = Some(barrier.clone());
        if let Some(timer) = previous {
            e.handle(1., Event::TimerFired { timer });
        }
        let blocked: Vec<_> = e.drain_actions().collect();
        assert!(e.venue.calls.is_empty());
        assert!(
            !blocked
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 1.)),
            "expiry cleanup cannot arm a native continuation"
        );
        assert!(
            e.io.http_plan_expiring
                .as_ref()
                .unwrap()
                .same_plan(&barrier)
        );
        assert_eq!(e.io.socket_continuations.len(), 1);
        assert!(e.io.socket_continuation_bytes > 0);
        e.io.http_plan_expiring = None;
        let after: Vec<_> = e.drain_actions().collect();
        let timer = wake(&after, 1.);
        if let Some(previous) = previous {
            assert_ne!(timer, previous);
            e.handle(1., Event::TimerFired { timer: previous });
            assert!(e.venue.calls.is_empty());
        }
        e.handle(1., Event::TimerFired { timer });
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.calls, [7]);
        assert_eq!(e.io.socket_continuation_bytes, 0);
        assert!(e.io.socket_continuations.is_empty());
    }
}
