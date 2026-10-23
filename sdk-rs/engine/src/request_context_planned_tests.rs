mod planned_calls {
    use super::*;
    use crate::request_context::{
        Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
        plan::{CallBound, RetryWindow, Slot, StepBound},
    };

    #[derive(Default)]
    struct Plans {
        plan: Option<Plan>,
        slot: Option<Slot>,
        long: bool,
        write: bool,
        continue_failure: bool,
        readiness: usize,
        panic_ready: bool,
        bad_retry: bool,
        check_generic_resend: bool,
        other_plan: Option<Plan>,
        cross_owner_denied: usize,
        wait: u32,
        timeout: u32,
        fallback: bool,
        idle: bool,
        responses: usize,
        timers: usize,
        retired: usize,
    }
    fn credential() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        }
    }
    fn queue(
        cx: &mut Cx<'_, Context>,
        context: Context,
        signed: bool,
    ) -> Result<RequestId, (Context, Error)> {
        if signed {
            cx.request_signed_managed(
                context.clone(),
                Method::Post,
                "https://venue.test/write".into(),
                None,
                Box::new(Recipe),
                context,
            )
        } else {
            cx.request_managed(
                context.clone(),
                Method::Get,
                "https://venue.test/read".into(),
                None,
                context,
            )
        }
    }
    struct BadHeaders;
    impl crate::SignStep for BadHeaders {
        fn freshness_ms(&self) -> f64 {
            20_000.
        }
        fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
            &[("bad\nheader", "value")]
        }
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            panic!("bad metadata reached provider")
        }
        fn apply(
            &mut self,
            _: &mut Action,
            _: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            panic!("bad metadata reached provider result")
        }
    }
    impl Plans {
        fn queue(&mut self, cx: &mut Cx<'_, Context>) {
            let plan = self.plan.as_ref().unwrap();
            let slots = cx.plan_slots(plan).unwrap();
            if let Some(slot) = slots.into_iter().next() {
                let context = cx.plan_leg_context(plan, &slot).unwrap();
                queue(cx, context, self.write).unwrap();
                self.slot = Some(slot);
            }
        }
    }
    impl Plans {
        fn admit(&mut self, cx: &mut Cx<'_, Context>) {
            let owner = cx
                .begin_preflight(
                    PreflightPolicy {
                        purpose: if self.write {
                            PreflightPurpose::WriteAfterReads
                        } else {
                            PreflightPurpose::ReadOnly
                        },
                        read_credential: None,
                        write_credential: self.write.then(credential),
                    },
                )
                .unwrap();
            let timeout = if self.timeout == 0 {
                20_000
            } else {
                self.timeout
            };
            let fallback_calls = [CallBound {
                timeout_ms: timeout,
                attempts: 2,
                retry_window: RetryWindow::FreshAttempt,
                after_refusal: &[],
            }];
            let fallback_steps = [
                StepBound {
                    wait_ms: 0,
                    calls: &fallback_calls,
                },
                StepBound {
                    wait_ms: 0,
                    calls: &fallback_calls,
                },
            ];
            let calls = [CallBound {
                timeout_ms: timeout,
                attempts: 2,
                retry_window: RetryWindow::FreshAttempt,
                after_refusal: if self.fallback { &fallback_steps } else { &[] },
            }];
            let steps = (0..if self.long { 8 } else { 2 })
                .map(|_| StepBound {
                    wait_ms: self.wait,
                    calls: &calls,
                })
                .collect::<Vec<_>>();
            self.plan = Some(
                cx.admit_plan(
                    &owner,
                    &steps,
                    PlanPolicy {
                        operation: if self.write {
                            Operation::Mutation
                        } else {
                            Operation::Read
                        },
                        credential: self.write.then(credential),
                        pacing: None,
                        retry: if self.write {
                            RetryPolicy::DefinitiveRefusal
                        } else {
                            RetryPolicy::ReadFailure
                        },
                    },
                )
                .unwrap(),
            );
            if !self.idle {
                self.queue(cx)
            }
        }
    }
    impl Venue for Plans {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            let plan = self.plan.as_ref().unwrap().clone();
            let slot = self.slot.as_ref().unwrap().clone();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            if response.body == "replace" {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Rejected(
                        VenueError::new("batch refused").outcome(crate::Outcome::Rejected),
                    ),
                )
                .unwrap();
                cx.replace_plan_call(&plan, &slot, &context).unwrap();
                self.queue(cx);
                return;
            }
            if response.body == "retry" {
                let _grant = if self.write {
                    use crate::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
                    const TABLE: CodeTable = CodeTable {
                        rows: &[Row { key: Key::Int(10002), verdict: Verdict::RefusedResend(ErrorKind::AuthInvalid), only: None }],
                        success: &[Key::Int(0)], unlisted: Verdict::Unknown(None), status: &[],
                    };
                    let native = Native { code: NativeCode::Int(10002), message: "request timestamp refused" };
                    let (verdict, grant) = cx.http_write_failure(&context, &TABLE, Some(&native), "refused", None).unwrap();
                    assert_eq!(verdict, Verdict::RefusedResend(ErrorKind::AuthInvalid));
                    Some(grant.expect("first classified effects-free refusal"))
                } else { None };
                if self.check_generic_resend {
                    let ids = cx.io.next_id;
                    let actions = cx.io.actions.len();
                    assert_eq!(cx.plan_retry_context(&plan, &slot, &context,
                        crate::request_context::RetryEvidence::DefinitiveRefusal), Err(Error::RetryForbidden));
                    assert_eq!(cx.io.next_id, ids);
                    assert_eq!(cx.io.actions.len(), actions);
                    cx.validate_plan_delivery(&plan, &context).unwrap();
                }
                let context = cx
                    .plan_retry_context(
                        &plan,
                        &slot,
                        &context,
                        if self.write {
                            crate::request_context::RetryEvidence::DefinitiveRefusal
                        } else {
                            crate::request_context::RetryEvidence::ReadFailure
                        },
                    )
                    .unwrap();
                if self.bad_retry {
                    let result = cx.request_signed_managed(
                        context.clone(),
                        Method::Post,
                        "https://venue.test/write".into(),
                        None,
                        Box::new(BadHeaders),
                        context,
                    );
                    assert_eq!(result.unwrap_err().1, Error::Invalid);
                } else {
                    queue(cx, context, self.write).unwrap();
                }
                return;
            }
            self.responses += 1;
            if self.write {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Accepted {
                        order_ids: vec![format!("native-{}", self.responses)],
                    },
                )
                .unwrap();
                cx.validate_http_write_effects(&context).unwrap();
            }
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            if self.responses
                == if self.long {
                    8
                } else if self.fallback {
                    3
                } else {
                    2
                }
            {
                cx.done_plan(&plan, "validated".into()).unwrap()
            } else {
                self.queue(cx)
            }
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {
            self.retired += 1
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            if self.continue_failure {
                let plan = self.plan.as_ref().unwrap();
                cx.finish_plan_failed_call(plan, self.slot.as_ref().unwrap(), &context)
                    .unwrap();
                self.responses += 1;
                self.queue(cx);
            }
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("plan-native".into())
        }
        fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, Context>, plan: &Plan) {
            assert!(cx.io.http_delivery.is_none());
            assert!(cx.io.request_contexts.plan_scope_clear());
            assert!(self.plan.as_ref().unwrap().same_plan(plan));
            self.readiness += 1;
            assert!(!self.panic_ready, "native readiness panic control");
            if let Some(other) = &self.other_plan {
                for slot in cx.plan_slots(other).unwrap() {
                    assert!(matches!(
                        cx.plan_leg_context(other, &slot),
                        Err(Error::Revoked)
                    ));
                    self.cross_owner_denied += 1;
                }
            }
            if !self.idle {
                self.queue(cx);
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {
            self.timers += 1
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) { self.admit(cx); }
        fn on_venue_action(&mut self, cx: &mut Cx<'_, Context>, _: CommandId, _: VenueAction) { self.admit(cx); }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn start(engine: &mut Engine<Plans>, now: f64) {
        engine.start(now);
        let command = if engine.venue.write {
            Command::Venue { command: 7, action: VenueAction::FetchBalanceInfo }
        } else {
            Command::FetchCandles { command: 7, symbol: "BTCUSD".into(),
                interval: Interval::M1, limit: Some(2), from_ms: None, to_ms: None }
        };
        engine.handle(now, Event::Command(command));
    }
    fn prepared(actions: &[Action]) -> (Preparation, HttpRequest) {
        actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => Some((*preparation, request.clone())),
                    _ => None,
                },
                _ => None,
            })
            .expect("plan preparation")
    }
    fn release(engine: &mut Engine<Plans>, token: Preparation, now: f64) {
        engine.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
        if engine.venue.write {
            let (attempt, work) = work(&actions);
            engine.handle(
                now,
                Event::Signed {
                    attempt,
                    result: Ok(crate::signing::SignResult::new(
                        vec![b"value".to_vec()],
                        work.receipt,
                    )),
                },
            );
            actions = engine.drain_actions().collect();
        }
        let permit = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Bind { permit, .. } => Some(*permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("plan binding");
        assert!(matches!(
            engine.claim_http(now, permit),
            Ok(Claim::Released(_))
        ));
    }
    fn response(engine: &mut Engine<Plans>, token: Preparation, now: f64, body: &str) {
        engine.handle(
            now,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    #[test]
    fn whole_plan_read_uses_all_declared_320_seconds_without_a_120_second_cap() {
        let mut e = crate::test_support::seeded_engine(Plans {
            long: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let plan = e.venue.plan.clone().unwrap();
        assert_eq!(plan.deadline(), 320_000.);
        let reserved = e.io.next_id;
        for index in 0..8 {
            let now = f64::from(index) * 38_000.;
            let (first, packet) = prepared(&actions);
            assert_eq!(packet.timeout_ms, Some(20_000));
            release(&mut e, first, now);
            response(&mut e, first, now + 19_000., "retry");
            actions = e.drain_actions().collect();
            let (second, packet) = prepared(&actions);
            assert_eq!(packet.timeout_ms, Some(20_000));
            release(&mut e, second, now + 19_000.);
            response(&mut e, second, now + 38_000., "ok");
            actions = e.drain_actions().collect();
            assert_eq!(
                e.io.next_id, reserved,
                "all IDs were admitted before the first physical call"
            );
        }
        assert_eq!(e.venue.responses, 8);
        assert_eq!(e.venue.retired, 1);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        );
        assert!(e.io.request_contexts.plan_timer(7).is_none());
    }
    #[test]
    fn plan_root_timer_seals_unissued_calls_without_leaking_a_native_timer() {
        let mut e = crate::test_support::seeded_engine(Plans {
            idle: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer {
                    timer,
                    at_ms: 80_000.,
                } => Some(*timer),
                _ => None,
            })
            .unwrap();
        e.handle(80_000., Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.timers, 0, "root timer is engine owned");
        assert_eq!(e.venue.retired, 1);
        assert!(e.io.request_contexts.plan_timer(7).is_none());
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.kind == Some(ErrorKind::Timeout))));
    }
    #[test]
    fn selected_write_retry_uses_the_shared_journal_and_fresh_reserved_window() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (first, _) = prepared(&actions);
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "retry");
        let (retry, packet) = prepared(&e.drain_actions().collect::<Vec<_>>());
        assert_eq!(packet.timeout_ms, Some(20_000));
        release(&mut e, retry, 19_000.);
        response(&mut e, retry, 38_000., "ok");
        let (next, packet) = prepared(&e.drain_actions().collect::<Vec<_>>());
        assert_eq!(packet.timeout_ms, Some(20_000));
        release(&mut e, next, 38_000.);
        response(&mut e, next, 57_000., "ok");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.retired, 1);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        );
    }
    #[test]
    fn failed_physical_cap_can_continue_independent_work_but_cannot_become_success() {
        for continue_failure in [false, true] {
            let mut e = crate::test_support::seeded_engine(Plans {
                write: true,
                continue_failure,
                ..Default::default()
            });
            start(&mut e, 0.);
            let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            let timer = e.io.managed_http[&first.attempt().request].timer;
            release(&mut e, first, 0.);
            e.handle(20_000., Event::TimerFired { timer });
            let mut actions = e.drain_actions().collect::<Vec<_>>();
            if continue_failure {
                assert!(!actions.iter().any(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 7, .. })
                )));
                let (next, packet) = prepared(&actions);
                assert_eq!(packet.timeout_ms, Some(20_000));
                release(&mut e, next, 20_000.);
                response(&mut e, next, 39_000., "ok");
                actions = e.drain_actions().collect();
            }
            assert_eq!(e.venue.retired, 1);
            assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::Unknown))));
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            );
        }
    }
    #[test]
    fn positive_wait_is_anchored_once_and_wakes_only_after_the_actual_http_scope() {
        let mut e = crate::test_support::seeded_engine(Plans {
            wait: 1_000,
            ..Default::default()
        });
        start(&mut e, 0.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer {
                    timer,
                    at_ms: 1_000.,
                } => Some(*timer),
                _ => None,
            })
            .unwrap();
        e.handle(1_000., Event::TimerFired { timer });
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 1_000.);
        response(&mut e, first, 1_500., "ok");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.readiness, 1);
        assert!(
            actions.iter().any(
                |a| matches!(a, Action::SetTimer { timer: id, at_ms: 2_500. } if *id == timer)
            )
        );
        e.handle(2_000., Event::TimerFired { timer });
        assert_eq!(e.venue.readiness, 1);
        e.drain_actions().for_each(drop);
        e.handle(2_500., Event::TimerFired { timer });
        let (second, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        assert_eq!(e.venue.readiness, 2);
        release(&mut e, second, 2_500.);
        response(&mut e, second, 3_000., "ok");
        assert_eq!(e.venue.retired, 1);
        e.drain_actions().for_each(drop);
    }
    #[test]
    fn temporary_delivery_marker_removal_does_not_grant_readiness() {
        let mut e = crate::test_support::seeded_engine(Plans {
            wait: 1_000,
            ..Default::default()
        });
        start(&mut e, 0.);
        e.drain_actions().for_each(drop);
        let context = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        }
        .http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 10_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .unwrap();
        let delivery = request_context_registry::Delivery {
            context,
            attempt: Attempt {
                request: RequestId(999),
                generation: 999,
            },
        };
        e.io.request_contexts.enter_delivery(&delivery);
        assert!(
            e.io.http_delivery.is_none(),
            "temporary marker removal during deferred work"
        );
        e.flush_http_plans(1_000.);
        assert_eq!(e.venue.readiness, 0);
        e.io.request_contexts.exit_delivery(&delivery);
        e.flush_http_plans(1_000.);
        assert_eq!(e.venue.readiness, 1);
        prepared(&e.drain_actions().collect::<Vec<_>>());
    }
    #[test]
    fn wide_finite_clock_wait_projection_cannot_fail_after_settlement() {
        let raw = 1e17;
        let mut e = crate::test_support::seeded_engine(Plans {
            wait: 1,
            timeout: 32,
            ..Default::default()
        });
        start(&mut e, raw);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == raw + 16. => Some(*timer),
                _ => None,
            })
            .unwrap();
        e.handle(raw + 16., Event::TimerFired { timer });
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, raw + 16.);
        response(&mut e, first, raw + 32., "ok");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { timer: id, at_ms } if *id == timer && *at_ms == raw + 48.)));
        e.handle(raw + 48., Event::TimerFired { timer });
        let (second, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, second, raw + 48.);
        response(&mut e, second, raw + 64., "ok");
        assert_eq!(e.venue.retired, 1);
    }
    #[test]
    fn whole_plan_reservation_refuses_atomically_in_either_safe_id_namespace() {
        for io_short in [true, false] {
            let mut e = crate::test_support::seeded_engine(Plans::default());
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            let preflight = cx
                .issue_preflight(
                    7,
                    PreflightPolicy {
                        purpose: PreflightPurpose::WriteAfterReads,
                        read_credential: None,
                        write_credential: Some(credential()),
                    },
                )
                .unwrap();
            if io_short {
                cx.io.next_id = crate::resource_ids::ResourceIds::starting_at(
                    crate::resource_ids::MAX_RESOURCE_ID - 6,
                );
            } else {
                cx.io
                    .signing
                    .set_test_ids(crate::resource_ids::ResourceIds::with_ceiling(1));
            }
            let before = cx.io.next_id;
            let calls = [CallBound {
                timeout_ms: 20_000,
                attempts: 2,
                retry_window: RetryWindow::FreshAttempt,
                after_refusal: &[],
            }];
            let steps = [StepBound {
                wait_ms: 0,
                calls: &calls,
            }];
            let policy = PlanPolicy {
                operation: Operation::Mutation,
                credential: Some(credential()),
                pacing: None,
                retry: RetryPolicy::DefinitiveRefusal,
            };
            assert!(matches!(
                cx.admit_plan(&preflight, &steps, policy.clone()),
                Err(Error::Capacity)
            ));
            assert_eq!(cx.io.next_id, before);
            assert_eq!(cx.io.request_contexts.plan_context_storage().2, 0);
            assert!(cx.io.actions.is_empty());
            let calls = [CallBound {
                attempts: 1,
                ..calls[0]
            }];
            let steps = [StepBound {
                wait_ms: 0,
                calls: &calls,
            }];
            let plan = cx.admit_plan(&preflight, &steps, policy).unwrap();
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            let context = cx.plan_leg_context(&plan, &slot).unwrap();
            queue(&mut cx, context, true).unwrap();
            assert_eq!(cx.io.request_contexts.plan_context_storage().2, 0);
        }
    }
    #[test]
    fn unrelated_contexts_cannot_steal_backing_promised_to_concurrent_plan_slots() {
        let mut e = crate::test_support::seeded_engine(Plans::default());
        let mut cx = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let preflight = cx
            .issue_preflight(
                7,
                PreflightPolicy {
                    purpose: PreflightPurpose::ReadOnly,
                    read_credential: None,
                    write_credential: None,
                },
            )
            .unwrap();
        let calls = (0..80)
            .map(|_| CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            })
            .collect::<Vec<_>>();
        let steps = [StepBound {
            wait_ms: 0,
            calls: &calls,
        }];
        let plan = cx
            .admit_plan(
                &preflight,
                &steps,
                PlanPolicy {
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let policy = Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        for _ in 0..176 {
            cx.http_context(Scope::Background, policy.clone()).unwrap();
        }
        assert!(matches!(
            cx.http_context(Scope::Background, policy.clone()),
            Err(Error::Capacity)
        ));
        let (_, capacity, credits) = cx.io.request_contexts.plan_context_storage();
        assert_eq!(credits, 80);
        let ids = cx.io.next_id;
        for slot in cx.plan_slots(&plan).unwrap() {
            cx.plan_leg_context(&plan, &slot).unwrap();
            assert_eq!(cx.io.request_contexts.plan_context_storage().1, capacity);
        }
        assert_eq!(cx.io.next_id, ids);
        assert_eq!(
            cx.io.request_contexts.plan_context_storage(),
            (256, capacity, 0)
        );
        cx.fail_plan(&plan, "cancelled").unwrap();
        cx.http_context(Scope::Background, policy).unwrap();
    }
    #[test]
    fn declared_refusal_branch_supersedes_parent_and_preserves_serial_children() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true,
            fallback: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let (parent, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, parent, 0.);
        response(&mut e, parent, 1., "replace");
        for index in 0..3 {
            let (child, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, child, f64::from(index) + 1.);
            response(&mut e, child, f64::from(index) + 2., "ok");
        }
        assert_eq!(e.venue.responses, 3);
        assert_eq!(e.venue.retired, 1);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        );
    }
    #[test]
    fn explicit_empty_write_plan_creates_no_root_context_or_attempt() {
        let mut e = crate::test_support::seeded_engine(Plans::default());
        let mut cx = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let preflight = cx
            .issue_preflight(
                7,
                PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential()),
                },
            )
            .unwrap();
        let ids = cx.io.next_id;
        cx.done_empty_plan(&preflight, "[]".into()).unwrap();
        assert_eq!(cx.io.next_id, ids);
        assert!(cx.io.managed_http.is_empty());
        assert_eq!(cx.io.request_contexts.plan_context_storage().0, 0);
        assert!(
            !cx.io
                .actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. } | Action::ManagedHttp(_)))
        );
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.retired, 1);
    }
    #[test]
    fn readiness_is_bound_to_its_plan_and_delayed_initial_custody_cannot_be_reused() {
        let mut e = crate::test_support::seeded_engine(Plans {
            wait: 1_000,
            ..Default::default()
        });
        start(&mut e, 0.);
        e.drain_actions().for_each(drop);
        let mut cx = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let owner = cx
            .issue_preflight(
                8,
                PreflightPolicy {
                    purpose: PreflightPurpose::ReadOnly,
                    read_credential: None,
                    write_credential: None,
                },
            )
            .unwrap();
        let calls = [CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        }];
        let steps = [StepBound {
            wait_ms: 1_000,
            calls: &calls,
        }];
        let other = cx
            .admit_plan(
                &owner,
                &steps,
                PlanPolicy {
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.io.request_contexts.end_plan_admission();
        let mut cx = Cx {
            now: 1_000.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let slot = cx.plan_slots(&other).unwrap().remove(0);
        assert!(
            matches!(cx.plan_leg_context(&other, &slot), Err(Error::Revoked)),
            "unrelated callback cannot reuse initial None handoff"
        );
        cx.io.http_plan_ready = Some(e.venue.plan.as_ref().unwrap().clone());
        assert!(
            matches!(cx.plan_leg_context(&other, &slot), Err(Error::Revoked)),
            "different Plan readiness grants no custody"
        );
        cx.io.http_plan_ready = Some(other.clone());
        let context = cx.plan_leg_context(&other, &slot).unwrap();
        queue(&mut cx, context, false).unwrap();
        cx.io.http_plan_ready = None;
    }
    #[test]
    fn caught_readiness_panic_fences_venue_queries_signers_permits_and_drain() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let mut e = crate::test_support::seeded_engine(Plans {
            wait: 1_000,
            panic_ready: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer {
                    timer,
                    at_ms: 1_000.,
                } => Some(*timer),
                _ => None,
            })
            .unwrap();
        assert!(
            catch_unwind(AssertUnwindSafe(
                || e.handle(1_000., Event::TimerFired { timer })
            ))
            .is_err()
        );
        assert!(e.failed());
        assert!(catch_unwind(AssertUnwindSafe(|| e.venue())).is_err());
        for query in 0..6 {
            assert!(
                catch_unwind(AssertUnwindSafe(|| match query {
                    0 => {
                        e.capabilities();
                    }
                    1 => {
                        e.book("BTCUSD");
                    }
                    2 => {
                        #[cfg(feature = "book-deltas")]
                        e.set_book_deltas(true);
                        #[cfg(not(feature = "book-deltas"))]
                        e.book("BTCUSD");
                    }
                    3 => {
                        e.drain_actions().for_each(drop);
                    }
                    4 => {
                        e.handle(1_001., Event::TimerFired { timer });
                    }
                    _ => {
                        e.prepare_dispatch(1_001.);
                    }
                }))
                .is_err(),
                "caught native panic leaked a Venue entry/query"
            );
        }
        assert!(!e.signing_pending(RequestId(1)));
        assert_eq!(e.venue.readiness, 1);
    }
    #[test]
    fn generic_plan_retry_refuses_a_real_classified_write_grant() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true, check_generic_resend: true, ..Default::default()
        });
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "retry");
        let (second, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        assert_ne!(first.attempt(), second.attempt());
        assert_eq!(e.venue.responses, 0);
        assert_eq!(e.venue.retired, 0);
    }
    #[test]
    fn selected_retry_queue_refusal_keeps_prior_send_unknown() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true,
            bad_retry: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "retry");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.retired, 1);
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::Unknown))));
    }
    #[test]
    fn plan_authority_rejects_foreign_slots_issuers_and_reused_command_generations() {
        let mut first = crate::test_support::seeded_engine(Plans {
            idle: true,
            ..Default::default()
        });
        let mut second = crate::test_support::seeded_engine(Plans {
            idle: true,
            ..Default::default()
        });
        start(&mut first, 0.);
        start(&mut second, 0.);
        let a = first.venue.plan.clone().unwrap();
        let b = second.venue.plan.clone().unwrap();
        assert_eq!(a.owner(), b.owner());
        let a_slot = Cx {
            now: 0.,
            store: &mut first.store,
            io: &mut first.io,
        }
        .plan_slots(&a)
        .unwrap()
        .remove(0);
        let mut cx = Cx {
            now: 0.,
            store: &mut second.store,
            io: &mut second.io,
        };
        let before = cx.io.next_id;
        assert!(matches!(cx.plan_slots(&a), Err(Error::Revoked)));
        cx.io.http_plan_ready = Some(b.clone());
        assert!(matches!(
            cx.plan_leg_context(&b, &a_slot),
            Err(Error::Revoked)
        ));
        cx.io.http_plan_ready = None;
        assert_eq!(cx.io.next_id, before);
        cx.fail_plan(&b, "aborted").unwrap();
        second.drain_actions().for_each(drop);
        let mut cx = Cx {
            now: 0.,
            store: &mut second.store,
            io: &mut second.io,
        };
        let next = cx
            .issue_preflight(
                7,
                PreflightPolicy {
                    purpose: PreflightPurpose::ReadOnly,
                    read_credential: None,
                    write_credential: None,
                },
            )
            .unwrap();
        assert_ne!(next.owner(), b.owner());
        assert!(matches!(cx.plan_slots(&b), Err(Error::Revoked)));
        assert!(matches!(cx.fail_plan(&b, "stale"), Err(Error::Revoked)));
        cx.done_preflight(&next, "cached".into()).unwrap();
    }
    #[test]
    fn unfinished_plan_cannot_seal_success_or_consume_reserved_resources() {
        let mut e = crate::test_support::seeded_engine(Plans {
            idle: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let plan = e.venue.plan.clone().unwrap();
        let mut cx = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let ids = cx.io.next_id;
        let storage = cx.io.request_contexts.plan_context_storage();
        let actions = cx.io.actions.len();
        assert_eq!(cx.done_plan(&plan, "optimistic".into()), Err(Error::State));
        assert_eq!(cx.io.next_id, ids);
        assert_eq!(cx.io.request_contexts.plan_context_storage(), storage);
        assert_eq!(cx.io.actions.len(), actions);
        cx.fail_plan(&plan, "explicit failure").unwrap();
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.retired, 1);
    }
}
