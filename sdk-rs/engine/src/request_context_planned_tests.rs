// Probe pendings are whole Contexts, returned by refused managed requests.
#[allow(clippy::result_large_err)]
mod planned_calls {
    use super::*;
    use crate::request_context::{
        Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
        plan::{CallBound, RetryWindow, Slot, StepBound},
    };

    #[derive(Default)]
    struct Plans {
        plan: Option<Plan>,
        preflight: Option<crate::request_context::Preflight>,
        slot: Option<Slot>,
        long: bool,
        write: bool,
        leg_intent: Option<crate::classify::Intent>,
        check_intent_refusals: bool,
        expected_intents: std::collections::VecDeque<crate::classify::Intent>,
        continue_failure: bool,
        readiness: usize,
        panic_ready: bool,
        bad_retry: bool,
        check_generic_resend: bool,
        wrong_resend_slot: Option<Slot>,
        other_plan: Option<Plan>,
        cross_owner_denied: usize,
        wait: u32,
        timeout: u32,
        fallback: bool,
        idle: bool,
        early_read: bool,
        early_checks: bool,
        settle_before_early: bool,
        parallel: bool,
        peer_queued: bool,
        early_replayed: Option<Result<(), Error>>,
        early_result: Option<Result<(), Error>>,
        responses: usize,
        timers: usize,
        retired: usize,
        /// Issue the first leg in admission custody but never queue it.
        issue_only: bool,
        unqueued: Option<Context>,
        /// Park the first Slot on this Session in admission custody.
        park_on: Option<crate::request_context::Session>,
        park_token: Option<Context>,
        park_result: Option<Result<(), Error>>,
        repark_on: Option<crate::request_context::Session>,
        /// Park the second parallel Slot on this Session too.
        park_second: Option<crate::request_context::Session>,
        retire_in_ended: Option<crate::request_context::Session>,
        retire_in_expired: Option<crate::request_context::Session>,
        fail_in_ended: bool,
        /// Park a failed call's retry on this Session in its failure callback.
        park_retry_on: Option<crate::request_context::Session>,
        retry_parks: Vec<Result<(), Error>>,
        repark_retry_on: Option<crate::request_context::Session>,
        wait_ended: Vec<(Context, VenueError)>,
        wait_expired: Vec<(Context, VenueError)>,
        /// Whether the Plan was still registered when a pending came back.
        expired_while_live: Vec<bool>,
        inherited: bool,
        // A delivered call reports itself as a complete order read.
        #[cfg(feature = "order-locks")]
        reconcile_reads: bool,
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
    fn context_with_intent(cx: &mut Cx<'_, Context>, plan: &Plan, slot: &Slot,
        intent: crate::classify::Intent) -> Result<Context, Error> {
        cx.plan_leg_context_with_intent(plan, slot, intent)
    }
    impl Plans {
        fn queue(&mut self, cx: &mut Cx<'_, Context>) {
            let plan = self.plan.as_ref().unwrap();
            let slots = cx.plan_slots(plan).unwrap();
            if let Some(slot) = slots.into_iter().next() {
                if self.check_intent_refusals {
                    let ids = cx.io.next_id;
                    let actions = cx.io.actions.len();
                    let storage = cx.io.request_contexts.plan_context_storage();
                    let available = cx.plan_slots(plan).unwrap().len();
                    for invalid in [crate::classify::Intent::Read, crate::classify::Intent::Amend] {
                        assert_eq!(cx.plan_leg_context_with_intent(plan, &slot, invalid), Err(Error::Invalid));
                        assert_eq!(cx.io.next_id, ids);
                        assert_eq!(cx.io.actions.len(), actions);
                        assert_eq!(cx.io.request_contexts.plan_context_storage(), storage);
                        assert_eq!(cx.plan_slots(plan).unwrap().len(), available);
                    }
                }

                let context = if let Some(intent) = self.leg_intent.take() {
                    let result = context_with_intent(cx, plan, &slot, intent);
                    assert!(result.is_ok(), "declared Plan intent must precede journal registration: {result:?}");
                    let context = result.unwrap();
                    // After selection the existing shared public method still
                    // refuses any change to an already-registered logical leg.
                    assert_eq!(cx.http_write_leg_intent(&context, intent), Err(Error::Duplicate));
                    context
                } else { cx.plan_leg_context(plan, &slot).unwrap() };
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
            self.preflight = Some(owner.clone());
            if self.inherited {
                cx.inherit_preflight_uncertainty(&owner).unwrap();
            }
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
            let parallel_calls = std::array::from_fn::<_, 2, _>(|_| CallBound {
                timeout_ms: timeout, attempts: 2,
                retry_window: RetryWindow::FreshAttempt, after_refusal: &[],
            });
            let steps = (0..if self.parallel { 1 } else if self.long { 8 } else { 2 })
                .map(|_| StepBound {
                    wait_ms: self.wait,
                    calls: if self.parallel { &parallel_calls } else { &calls },
                })
                .collect::<Vec<_>>();
            self.plan = Some(
                cx.admit_plan(
                    &owner,
                    &steps,
                    PlanPolicy {
                        independent_calls: false,
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
            if let Some(session) = self.park_on.clone() {
                let plan = self.plan.clone().unwrap();
                let slot = cx.plan_slots(&plan).unwrap().remove(0);
                let token = cx
                    .http_context(crate::request_context::Scope::Background, wait_token_policy())
                    .unwrap();
                self.park_token = Some(token.clone());
                self.slot = Some(slot.clone());
                self.park_result = Some(
                    cx.plan_session_wait(&plan, &slot, None, &session, token)
                        .map_err(|(_, error)| error),
                );
                if let Some(second) = self.park_second.clone() {
                    let slot = cx.plan_slots(&plan).unwrap().remove(0);
                    let token = cx
                        .http_context(crate::request_context::Scope::Background, wait_token_policy())
                        .unwrap();
                    cx.plan_session_wait(&plan, &slot, None, &second, token)
                        .map_err(|(_, error)| error)
                        .unwrap();
                }
            }
            if self.issue_only {
                let plan = self.plan.as_ref().unwrap();
                let slot = cx.plan_slots(plan).unwrap().remove(0);
                self.unqueued = Some(cx.plan_leg_context(plan, &slot).unwrap());
            } else if !self.idle {
                self.queue(cx);
                if self.parallel {
                    let plan = self.plan.as_ref().unwrap();
                    let peer = cx.plan_slots(plan).unwrap().remove(0);
                    let context = cx.plan_leg_context(plan, &peer).unwrap();
                    if self.peer_queued { queue(cx, context, self.write).unwrap(); }
                }
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
            if let Some(expected) = self.expected_intents.pop_front() {
                use crate::classify::{CodeTable, Key, Native, NativeCode, Row, SatisfiedIntent, Verdict};
                const TABLE: CodeTable = CodeTable {
                    rows: &[Row { key: Key::Int(777), verdict: Verdict::Satisfied(SatisfiedIntent::Cancel), only: None }],
                    success: &[Key::Int(0)], unlisted: Verdict::Unknown(None), status: &[],
                };
                let native = Native { code: NativeCode::Int(777), message: "already absent" };
                let fact = cx.classify_write(&context, &TABLE, Some(&native), "already absent").unwrap();
                assert_eq!(fact.verdict(), if expected == crate::classify::Intent::Cancel {
                    Verdict::Satisfied(SatisfiedIntent::Cancel)
                } else { Verdict::Unknown(None) });
            }
            if response.body == "reject" {
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Rejected(
                        VenueError::new("order refused").outcome(crate::Outcome::Rejected),
                    ),
                )
                .unwrap();
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.fail_plan(&plan, "order refused").unwrap();
                return;
            }
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
                let grant = if self.write {
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
                let previous_attempt = cx.io.http_delivery.as_ref().unwrap().attempt.request;
                if self.check_generic_resend {
                    let ids = cx.io.next_id;
                    let actions = cx.io.actions.len();
                    assert_eq!(cx.plan_resend_context(&plan, &slot, &context,
                        crate::request_context::ResendGrant { attempt: RequestId(u64::MAX) }), Err(Error::Revoked));
                    if let Some(foreign) = &self.wrong_resend_slot {
                        assert_eq!(cx.plan_resend_context(&plan, foreign, &context,
                            crate::request_context::ResendGrant { attempt: previous_attempt }), Err(Error::Revoked));
                    }
                    assert_eq!(cx.io.next_id, ids);
                    assert_eq!(cx.io.actions.len(), actions);
                    cx.validate_plan_delivery(&plan, &context).unwrap();
                }
                let context = if let Some(grant) = grant {
                    cx.plan_resend_context(&plan, &slot, &context, grant).unwrap()
                } else {
                    cx.plan_retry_context(&plan, &slot, &context,
                        crate::request_context::RetryEvidence::ReadFailure).unwrap()
                };
                if self.check_generic_resend {
                    // Replaying the prior opaque grant cannot select another
                    // retry or consume additional reserved attempt identities.
                    let ids = cx.io.next_id;
                    let actions = cx.io.actions.len();
                    assert_eq!(cx.plan_resend_context(&plan, &slot, &context,
                        crate::request_context::ResendGrant { attempt: previous_attempt }), Err(Error::Revoked));
                    assert_eq!(cx.io.next_id, ids);
                    assert_eq!(cx.io.actions.len(), actions);
                }
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
            if self.early_read {
                if self.early_checks {
                    let snapshot = (cx.io.next_id, cx.io.actions.len(), cx.io.request_contexts.plan_context_storage());
                    let delivery = cx.io.http_delivery.take();
                    assert_eq!(complete_native_early_read(cx, &plan, &slot, &context), Err(Error::Revoked));
                    cx.io.http_delivery = delivery;
                    let now = cx.now;
                    for expired in [20_000., plan.deadline()] {
                        cx.now = expired;
                        assert_eq!(complete_native_early_read(cx, &plan, &slot, &context), Err(Error::TimedOut));
                    }
                    cx.now = now;
                    let other = self.other_plan.as_ref().unwrap();
                    assert_eq!(complete_native_early_read(cx, other, &slot, &context), Err(Error::Revoked));
                    let other_slot = self.wrong_resend_slot.as_ref().unwrap();
                    assert_eq!(complete_native_early_read(cx, &plan, other_slot, &context), Err(Error::Revoked));
                    assert_eq!(snapshot, (cx.io.next_id, cx.io.actions.len(), cx.io.request_contexts.plan_context_storage()));
                    cx.validate_plan_delivery(&plan, &context).unwrap();
                }
                if self.settle_before_early { cx.finish_plan_call(&plan, &slot, &context).unwrap(); }
                let snapshot = (cx.io.next_id, cx.io.actions.len(), cx.io.request_contexts.plan_context_storage());
                let result = complete_native_early_read(cx, &plan, &slot, &context);
                if result.is_err() {
                    assert_eq!(snapshot, (cx.io.next_id, cx.io.actions.len(), cx.io.request_contexts.plan_context_storage()));
                    if !self.settle_before_early { cx.validate_plan_delivery(&plan, &context).unwrap(); }
                } else {
                    self.early_replayed = Some(complete_native_early_read(cx, &plan, &slot, &context));
                }
                self.early_result = Some(result);
                return;
            }
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
            #[cfg(feature = "order-locks")]
            if self.reconcile_reads {
                cx.orders_reconciled(None);
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
        fn on_plan_session_wait_ended(
            &mut self,
            cx: &mut Cx<'_, Context>,
            plan: &Plan,
            pending: Context,
            error: VenueError,
        ) {
            self.wait_ended.push((pending.clone(), error));
            if std::mem::take(&mut self.fail_in_ended) {
                cx.fail_plan(plan, "venue gave up after login failed").unwrap();
                return;
            }
            if let Some(session) = self.repark_retry_on.take() {
                let slot = self.slot.clone().unwrap();
                cx.plan_retry_session_wait(
                    plan,
                    &slot,
                    &pending,
                    crate::request_context::RetryEvidence::ReadFailure,
                    &session,
                    pending.clone(),
                )
                .map_err(|(_, error)| error)
                .unwrap();
                return;
            }
            if let Some(session) = self.retire_in_ended.take() {
                assert!(cx.retire_session(&session).unwrap().is_empty());
            }
            if let Some(session) = self.repark_on.take() {
                let slot = cx.plan_slots(plan).unwrap().remove(0);
                cx.plan_session_wait(plan, &slot, None, &session, pending)
                    .map_err(|(_, error)| error)
                    .unwrap();
            }
        }
        fn on_session_wait_expired(&mut self, cx: &mut Cx<'_, Context>, pending: Context, error: VenueError) {
            let live = self
                .plan
                .as_ref()
                .is_some_and(|plan| cx.io.request_contexts.plan_registered(plan));
            self.expired_while_live.push(live);
            self.wait_expired.push((pending, error));
            if let Some(session) = self.retire_in_expired.take() {
                assert!(cx.retire_session(&session).unwrap().is_empty());
            }
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {
            self.retired += 1
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            if let Some(session) = self.park_retry_on.clone() {
                let plan = self.plan.clone().unwrap();
                let slot = self.slot.clone().unwrap();
                self.retry_parks.push(
                    cx.plan_retry_session_wait(
                        &plan,
                        &slot,
                        &context,
                        crate::request_context::RetryEvidence::ReadFailure,
                        &session,
                        context.clone(),
                    )
                    .map_err(|(_, error)| error),
                );
                return;
            }
            if self.early_read {
                let plan = self.plan.as_ref().unwrap();
                let slot = self.slot.as_ref().unwrap();
                let before = (cx.io.next_id, cx.io.actions.len(), cx.io.request_contexts.plan_context_storage());
                self.early_result = Some(complete_native_early_read(cx, plan, slot, &context));
                assert_eq!(before, (cx.io.next_id, cx.io.actions.len(), cx.io.request_contexts.plan_context_storage()));
            }
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
    // The native current-settings branch consumes its DATA directly and never
    // settles separately or issues the remaining read.
    fn complete_native_early_read(cx: &mut Cx<'_, Context>, plan: &Plan, slot: &Slot,
        context: &Context) -> Result<(), Error> {
        cx.done_read_plan_after_data(plan, slot, context, "current-settings".into())
    }
    #[test]
    fn validated_first_read_can_return_current_settings_without_issuing_second_read() {
        let mut e = crate::test_support::seeded_engine(Plans {
            early_read: true, ..Default::default()
        });
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        let reserved = e.io.next_id;
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "current-settings");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.early_result, Some(Ok(())), "native early Read success must retire unissued work");
        assert_eq!(e.venue.responses, 1);
        assert_eq!(e.venue.early_replayed, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.retired, 1);
        assert_eq!(e.io.next_id, reserved);
        assert_eq!(e.io.request_contexts.plan_context_storage().0, 0);
        assert_eq!(e.io.request_contexts.plan_context_storage().2, 0);
        assert!(e.io.request_contexts.plan_timer(7).is_none());
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "current-settings")).count(), 1);
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
        response(&mut e, first, 19_001., "late");
        assert!(e.drain_actions().next().is_none());
        assert_eq!(e.venue.responses, 1);
    }
    #[test]
    fn early_read_refusals_preserve_current_data_and_reserved_work() {
        let mut foreign = crate::test_support::seeded_engine(Plans { idle: true, ..Default::default() });
        start(&mut foreign, 0.);
        let other = foreign.venue.plan.clone().unwrap();
        let foreign_slot = Cx { now: 0., store: &mut foreign.store, io: &mut foreign.io }
            .plan_slots(&other).unwrap().remove(0);
        let mut e = crate::test_support::seeded_engine(Plans {
            early_read: true, early_checks: true, other_plan: Some(other),
            wrong_resend_slot: Some(foreign_slot), ..Default::default()
        });
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "ok");
        assert_eq!(e.venue.early_result, Some(Ok(())));
        assert_eq!(e.venue.retired, 1);
    }
    #[test]
    fn early_read_rejects_mutations_settled_delivery_and_selected_peers() {
        for (write, settled, parallel, peer_queued, expected) in [
            (true, false, false, false, Error::Invalid),
            (false, true, false, false, Error::Revoked),
            (false, false, true, false, Error::State),
            (false, false, true, true, Error::State),
        ] {
            let mut e = crate::test_support::seeded_engine(Plans {
                early_read: true, write, settle_before_early: settled,
                parallel, peer_queued, ..Default::default()
            });
            start(&mut e, 0.);
            let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, first, 0.);
            response(&mut e, first, 19_000., "ok");
            assert_eq!(e.venue.early_result, Some(Err(expected)));
            assert_eq!(e.venue.retired, 0);
            assert!(e.io.request_contexts.plan_timer(7).is_some());
            assert!(!e.drain_actions().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))));
        }
    }
    #[test]
    fn early_read_never_converts_transport_failure_into_data_success() {
        let mut e = crate::test_support::seeded_engine(Plans { early_read: true, ..Default::default() });
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        e.handle(19_000., Event::HttpFailed { request: first.attempt().request, error: "network" });
        assert_eq!(e.venue.early_result, Some(Err(Error::Revoked)));
        assert_eq!(e.venue.responses, 0);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))));
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. }))).count(), 1);
        assert_eq!(e.venue.retired, 1);
        assert_eq!(e.io.request_contexts.plan_context_storage().2, 0);
    }
    #[test]
    fn early_read_after_fresh_read_retry_retains_original_plan_bound() {
        let mut e = crate::test_support::seeded_engine(Plans { early_read: true, ..Default::default() });
        start(&mut e, 0.);
        let plan = e.venue.plan.clone().unwrap();
        assert_eq!(plan.deadline(), 80_000.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "retry");
        let (second, packet) = prepared(&e.drain_actions().collect::<Vec<_>>());
        assert_eq!(packet.timeout_ms, Some(20_000));
        release(&mut e, second, 19_000.);
        response(&mut e, second, 38_000., "current-settings");
        assert_eq!(e.venue.early_result, Some(Ok(())));
        assert_eq!(e.venue.retired, 1);
        assert_eq!(plan.deadline(), 80_000.);
        assert_eq!(e.io.request_contexts.plan_context_storage().2, 0);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))).count(), 1);
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
    }
    #[test]
    fn indexed_plan_retires_before_reused_command_generation() {
        let mut e = crate::test_support::seeded_engine(Plans { idle: true, ..Default::default() });
        start(&mut e, 0.);
        let old = e.venue.plan.clone().unwrap();
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        assert!(e.io.request_contexts.has_plans());
        Cx { now: 0., store: &mut e.store, io: &mut e.io }.fail_plan(&old, "cancelled").unwrap();
        assert!(!e.io.request_contexts.has_plans());
        e.drain_actions().for_each(drop);
        let before = e.io.request_contexts.plan_scan_visits();
        e.handle(1., Event::TimerFired { timer });
        e.drain_actions().for_each(drop);
        assert_eq!(e.io.request_contexts.plan_scan_visits(), before);
        start(&mut e, 2.);
        let new = e.venue.plan.clone().unwrap();
        assert_ne!(new.owner(), old.owner());
        assert!(e.io.request_contexts.has_plans());
        e.drain_actions().for_each(drop);
        e.handle(3., Event::TimerFired { timer });
        assert_eq!(e.venue.retired, 1, "stale Plan timer cannot retire the new generation");
        assert!(e.io.request_contexts.has_plans());
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
    /// A serial Read Plan is one logical read: while it waits for its next
    /// step its settled call has left the context map, and an unrelated
    /// admission's prune must not reset its earliest read mark.
    #[cfg(feature = "order-locks")]
    #[test]
    fn a_serial_plans_read_mark_survives_an_unrelated_prune_between_steps() {
        let mut e = crate::test_support::seeded_engine(Plans {
            wait: 1_000,
            reconcile_reads: true,
            ..Default::default()
        });
        start(&mut e, 0.);
        let timer = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms: 1_000. } => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(1_000., Event::TimerFired { timer });
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 1_000.);
        response(&mut e, first, 1_500., "ok");
        e.drain_actions().for_each(drop);
        assert_eq!(e.venue.responses, 1);
        e.io.order_locks.fence(Some(&["BTCUSD".into()]));
        Cx { now: 1_600., store: &mut e.store, io: &mut e.io }
            .http_context(
                crate::request_context::Scope::Background,
                crate::request_context::Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        e.handle(2_500., Event::TimerFired { timer });
        let (second, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, second, 2_500.);
        response(&mut e, second, 3_000., "ok");
        assert_eq!(e.venue.responses, 2);
        assert!(
            e.io.order_locks.uncertain("BTCUSD"),
            "the Plan's first call predates the fence"
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
                independent_calls: false,
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
            assert!(!cx.io.request_contexts.has_plans(), "refused admission cannot enter the index");
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
                    independent_calls: false,
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
                    independent_calls: false,
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
    fn invalid_plan_intents_leave_declared_slots_and_context_credits_available() {
        for write in [false, true] {
            let mut e = crate::test_support::seeded_engine(Plans {
                write, check_intent_refusals: true, ..Default::default()
            });
            start(&mut e, 0.);
            let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, first, 0.);
            response(&mut e, first, 1., "ok");
            let (second, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, second, 1.);
            response(&mut e, second, 2., "ok");
            assert_eq!(e.venue.responses, 2);
            assert_eq!(e.venue.retired, 1);
        }
    }
    #[test]
    fn plan_narrows_a_cancel_leg_before_registration_and_keeps_parent_place_intent() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true, leg_intent: Some(crate::classify::Intent::Cancel),
            expected_intents: [crate::classify::Intent::Cancel, crate::classify::Intent::Place].into(),
            ..Default::default()
        });
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        response(&mut e, first, 1., "ok");
        let (second, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, second, 1.);
        response(&mut e, second, 2., "ok");
        assert!(e.venue.expected_intents.is_empty());
        assert_eq!(e.venue.responses, 2);
        assert_eq!(e.venue.retired, 1);
        assert!(e.drain_actions().any(|action| matches!(action,
            Action::Notice(Notice::CommandDone { command: 7, .. }))));
    }
    #[test]
    fn generic_plan_retry_refuses_a_real_classified_write_grant() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true, check_generic_resend: true, ..Default::default()
        });
        let mut foreign = crate::test_support::seeded_engine(Plans::default());
        start(&mut foreign, 0.);
        e.venue.wrong_resend_slot = foreign.venue.slot.clone();
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

    fn plan_wait_fixture(issue_only: bool) -> (Engine<Plans>, Plan, crate::request_context::Session) {
        let mut e = crate::test_support::seeded_engine(Plans {
            idle: true,
            issue_only,
            ..Default::default()
        });
        start(&mut e, 0.);
        let plan = e.venue.plan.clone().unwrap();
        let session = Cx { now: 0., store: &mut e.store, io: &mut e.io }
            .open_session(crate::request_context::SessionPolicy {
                credential: None,
                repeat_safe: true,
                private_egress: None,
            })
            .unwrap();
        e.drain_actions().for_each(drop);
        (e, plan, session)
    }
    /// Control (RUST_PLAN_SESSION_WAIT C1): a Plan leg cannot park on a Session.
    #[test]
    fn a_plan_leg_cannot_park_on_a_session_today() {
        let (mut e, _, session) = plan_wait_fixture(true);
        let leg = e.venue.unqueued.clone().unwrap();
        let mut cx = Cx { now: 1., store: &mut e.store, io: &mut e.io };
        let (ids, timers) = (cx.io.next_id, cx.io.timers.len());
        let refused = cx.session_wait(&session, &leg, leg.clone());
        assert_eq!(refused.map_err(|(_, error)| error), Err(Error::State));
        assert_eq!((cx.io.next_id, cx.io.timers.len()), (ids, timers));
        assert!(cx.session_ready(&session).is_empty(), "no wait was held");
    }
    /// Control (C3): a session_ready drain outside Plan custody cannot issue
    /// the eligible Slot itself.
    #[test]
    fn a_session_drain_outside_plan_custody_cannot_issue_a_leg() {
        let (mut e, plan, session) = plan_wait_fixture(false);
        let mut cx = Cx { now: 1., store: &mut e.store, io: &mut e.io };
        assert!(cx.session_ready(&session).is_empty());
        let slot = cx.plan_slots(&plan).unwrap().remove(0);
        assert_eq!(cx.plan_leg_context(&plan, &slot), Err(Error::Revoked));
    }
    /// Control (C4): a leg issued before a login wait longer than its logical
    /// window cannot be queued after it, so the wait must precede issue.
    #[test]
    fn a_plan_leg_issued_before_a_long_wait_has_expired_when_login_returns() {
        let (e, plan, _) = plan_wait_fixture(true);
        let leg = e.venue.unqueued.clone().unwrap();
        let after_login = leg.deadline() + 1.;
        assert!(after_login < plan.deadline(), "the Plan root outlives the leg window");
        // The issued leg's own logical window has lapsed; no custody can revive it.
        assert_eq!(e.io.request_contexts.validate(&leg, after_login), Err(Error::TimedOut));
    }
    fn wait_token_policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 1_000_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    fn open(e: &mut Engine<Plans>, now: f64) -> crate::request_context::Session {
        Cx { now, store: &mut e.store, io: &mut e.io }
            .open_session(crate::request_context::SessionPolicy {
                credential: None,
                repeat_safe: true,
                private_egress: None,
            })
            .unwrap()
    }
    fn parked(now: f64) -> (Engine<Plans>, Plan, crate::request_context::Session) {
        let mut e = crate::test_support::seeded_engine(Plans { idle: true, ..Default::default() });
        e.start(0.);
        let session = open(&mut e, 0.);
        e.venue.park_on = Some(session.clone());
        start(&mut e, now);
        assert_eq!(e.venue.park_result, Some(Ok(())), "the eligible Slot parks in admission custody");
        let plan = e.venue.plan.clone().unwrap();
        (e, plan, session)
    }
    fn prepares(actions: &[Action]) -> usize {
        actions.iter().filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))).count()
    }
    fn flush(e: &mut Engine<Plans>, now: f64) -> Vec<Action> {
        e.handle(now, Event::TimerFired { timer: TimerId(u64::MAX - 1) });
        e.drain_actions().collect()
    }

    /// N1: a parked Slot spends nothing; session_ready issues one fresh leg under
    /// the same Plan root, queued in the drain, with one HTTP.
    #[test]
    fn a_parked_slot_resumes_as_a_fresh_leg_under_its_original_root() {
        let (mut e, plan, session) = parked(0.);
        let token = e.venue.park_token.clone().unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(prepares(&actions), 0, "nothing is issued while parked");
        let mut cx = Cx { now: 30_000., store: &mut e.store, io: &mut e.io };
        assert!(cx.plan_slots(&plan).unwrap().is_empty(), "a parked Slot is reserved");
        let waiters = cx.session_ready(&session);
        assert_eq!(waiters.len(), 1);
        let crate::request_context::Waiter::Ready(leg, pending) = waiters.into_iter().next().unwrap() else {
            panic!("a live Plan resumes Ready");
        };
        assert_eq!(pending, token);
        assert_eq!(leg.owner(), plan.owner(), "the original Plan root");
        assert!(leg.deadline() > 30_000. + 20_000. - 1., "the window starts at resume");
        assert!(leg.deadline() <= plan.deadline());
        queue(&mut cx, leg, false).unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(prepares(&actions), 1, "exactly one HTTP");
    }

    /// N3: Plan root expiry while parked: one Timeout terminal, the pending comes
    /// back once through on_session_wait_expired, and nothing is sent.
    #[test]
    fn a_parked_slot_expires_with_its_plan_and_returns_its_pending_once() {
        let (mut e, plan, _) = parked(0.);
        let token = e.venue.park_token.clone().unwrap();
        e.drain_actions().for_each(drop);
        let timer = e.io.request_contexts.plan_timer(7).unwrap();
        e.handle(plan.deadline(), Event::TimerFired { timer });
        // The Plan timer event alone returns the pending: no follow-up event.
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(prepares(&actions), 0);
        let failed = actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. }))).count();
        assert_eq!(failed, 1, "one Plan terminal");
        assert_eq!(e.venue.wait_expired.len(), 1, "the pending is returned once");
        assert_eq!(e.venue.wait_expired[0].0, token);
        assert_eq!(e.venue.expired_while_live, [false], "the Plan terminal comes first");
        let error = &e.venue.wait_expired[0].1;
        assert_eq!((error.kind, error.outcome), (Some(ErrorKind::Timeout), Some(crate::Outcome::NotSent)));
        assert!(e.venue.wait_ended.is_empty());
    }

    /// N4b: retire the old Session and open its replacement outside any readiness
    /// scope; the ended callback re-parks on the new Session in ready custody,
    /// whose login then resumes the leg.
    #[test]
    fn a_retired_session_ends_the_wait_in_ready_custody_for_a_repark() {
        let (mut e, plan, old) = parked(0.);
        e.drain_actions().for_each(drop);
        let replacement = open(&mut e, 1.);
        e.venue.repark_on = Some(replacement.clone());
        let returned = Cx { now: 1., store: &mut e.store, io: &mut e.io }
            .retire_session(&old)
            .unwrap();
        assert!(returned.is_empty(), "a parked Slot is not a plain owner waiter");
        let actions = flush(&mut e, 2.);
        assert_eq!(prepares(&actions), 0);
        assert_eq!(e.venue.wait_ended.len(), 1, "ended once, in ready custody");
        assert_eq!(e.venue.wait_ended[0].1.outcome, Some(crate::Outcome::NotSent));
        assert!(e.venue.wait_expired.is_empty());
        let mut cx = Cx { now: 3., store: &mut e.store, io: &mut e.io };
        let waiters = cx.session_ready(&replacement);
        let Some(crate::request_context::Waiter::Ready(leg, _)) = waiters.into_iter().next() else {
            panic!("the re-parked Slot resumes on the new Session");
        };
        assert_eq!(leg.owner(), plan.owner());
    }

    /// #1896 readiness owns the callback like Plan readiness does: the flush
    /// holds an ended wait until that custody exits, then delivers it once.
    #[test]
    fn an_ended_wait_waits_out_preflight_readiness_custody() {
        let (mut e, _, old) = parked(0.);
        e.drain_actions().for_each(drop);
        let _ = Cx { now: 1., store: &mut e.store, io: &mut e.io }.retire_session(&old).unwrap();
        e.io.http_preflight_ready = e.venue.preflight.clone();
        assert!(e.io.http_preflight_ready.is_some());
        e.flush_plan_session_waits(2.);
        assert!(e.venue.wait_ended.is_empty() && e.venue.wait_expired.is_empty());
        e.io.http_preflight_ready = None;
        e.flush_plan_session_waits(3.);
        assert_eq!(e.venue.wait_ended.len(), 1);
    }

    /// N6: parking needs Plan custody, an unparked Slot and a live Session, and
    /// every refusal spends nothing.
    #[test]
    fn plan_session_wait_refusals_spend_nothing() {
        let (mut e, plan, session) = parked(0.);
        e.drain_actions().for_each(drop);
        let token = e.venue.park_token.clone().unwrap();
        let slot = e.venue.slot.clone().unwrap();
        let mut cx = Cx { now: 1., store: &mut e.store, io: &mut e.io };
        let retired = cx.open_session(crate::request_context::SessionPolicy { credential: None, repeat_safe: true, private_egress: None }).unwrap();
        let _ = cx.retire_session(&retired).unwrap();
        let before = (cx.io.next_id, cx.io.timers.len());
        let again = cx.plan_session_wait(&plan, &slot, None, &session, token.clone());
        assert_eq!(again.map_err(|(_, error)| error), Err(Error::Revoked), "already parked, and no custody");
        let stale = cx.plan_session_wait(&plan, &slot, None, &retired, token);
        assert_eq!(stale.map_err(|(_, error)| error), Err(Error::Revoked));
        assert!(cx.io.next_id == before.0 && cx.io.timers.len() == before.1, "refusals spend nothing");
    }

    /// N8: the venue fails the Plan while a Slot is parked; one terminal, no
    /// ended callback, and the pending comes back once.
    #[test]
    fn a_sealed_plan_returns_its_parked_pending_once_without_custody() {
        let (mut e, plan, _) = parked(0.);
        let token = e.venue.park_token.clone().unwrap();
        e.drain_actions().for_each(drop);
        Cx { now: 1., store: &mut e.store, io: &mut e.io }
            .fail_plan(&plan, "venue gave up")
            .unwrap();
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        actions.extend(flush(&mut e, 2.));
        assert_eq!(prepares(&actions), 0);
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. }))).count(), 1);
        assert!(e.venue.wait_ended.is_empty());
        assert_eq!(e.venue.wait_expired.len(), 1);
        assert_eq!(e.venue.wait_expired[0].0, token);
        let error = &e.venue.wait_expired[0].1;
        assert_eq!((error.kind, error.outcome), (Some(ErrorKind::InvalidInput), Some(crate::Outcome::NotSent)));
    }

    /// Storage reserved at park survives a flush: park, retire, flush, re-park,
    /// retire again, and the ended queue never needs to grow.
    #[test]
    fn ended_wait_storage_survives_a_flush() {
        let (mut e, _, old) = parked(0.);
        e.drain_actions().for_each(drop);
        let replacement = open(&mut e, 1.);
        e.venue.repark_on = Some(replacement.clone());
        let _ = Cx { now: 1., store: &mut e.store, io: &mut e.io }.retire_session(&old).unwrap();
        flush(&mut e, 2.);
        assert_eq!(e.venue.wait_ended.len(), 1);
        let reserved = e.io.session_waits.ended_capacity();
        assert!(reserved >= 1, "the flush kept the reserved buffer");
        let _ = Cx { now: 3., store: &mut e.store, io: &mut e.io }.retire_session(&replacement).unwrap();
        assert_eq!(e.io.session_waits.ended_capacity(), reserved, "no growth in the drain path");
        flush(&mut e, 4.);
        assert_eq!(e.venue.wait_ended.len(), 2);
    }

    /// A no-custody callback inside the flush that retires another Session is
    /// served in the same flush, not stranded until an unrelated event.
    #[test]
    fn a_wait_ended_inside_the_flush_is_delivered_in_the_same_flush() {
        let mut e = crate::test_support::seeded_engine(Plans { idle: true, parallel: true, ..Default::default() });
        e.start(0.);
        let first = open(&mut e, 0.);
        let second = open(&mut e, 0.);
        e.venue.park_on = Some(first);
        e.venue.park_second = Some(second.clone());
        e.venue.retire_in_expired = Some(second);
        start(&mut e, 0.);
        assert_eq!(e.venue.park_result, Some(Ok(())));
        e.drain_actions().for_each(drop);
        let plan = e.venue.plan.clone().unwrap();
        Cx { now: 1., store: &mut e.store, io: &mut e.io }.fail_plan(&plan, "venue gave up").unwrap();
        flush(&mut e, 2.);
        assert_eq!(e.venue.wait_expired.len(), 2, "both pendings returned in one flush");
        assert!(e.venue.wait_ended.is_empty());
    }
    #[test]
    fn a_sealed_plan_is_not_resumed_by_a_later_session_ready() {
        let (mut e, plan, session) = parked(0.);
        e.drain_actions().for_each(drop);
        let mut cx = Cx { now: 1., store: &mut e.store, io: &mut e.io };
        cx.fail_plan(&plan, "venue gave up").unwrap();
        assert!(cx.session_ready(&session).is_empty(), "a sealed Plan issues nothing");
        let actions = flush(&mut e, 2.);
        assert_eq!(prepares(&actions), 0);
        assert_eq!(e.venue.wait_expired.len(), 1, "returned once");
        assert!(e.venue.wait_ended.is_empty(), "no custody callback for a sealed Plan");
    }

    /// A Slot whose Session ended stays held until its ended disposition, so
    /// ordinary readiness in the same flush cannot issue it first.
    #[test]
    fn ordinary_readiness_cannot_issue_a_slot_held_for_its_ended_hook() {
        let mut e = crate::test_support::seeded_engine(Plans::default());
        e.start(0.);
        let old = open(&mut e, 0.);
        e.venue.park_on = Some(old.clone());
        e.venue.idle = false;
        start(&mut e, 0.);
        assert_eq!(e.venue.park_result, Some(Ok(())));
        assert_eq!(prepares(&e.drain_actions().collect::<Vec<_>>()), 0);
        let replacement = open(&mut e, 1.);
        e.venue.repark_on = Some(replacement);
        let _ = Cx { now: 1., store: &mut e.store, io: &mut e.io }.retire_session(&old).unwrap();
        let actions = flush(&mut e, 2.);
        assert_eq!(prepares(&actions), 0, "readiness did not issue the held Slot");
        assert_eq!(e.venue.wait_ended.len(), 1, "the ended hook decided it, and re-parked");
    }

    /// The ended hook seals the Plan while a sibling Slot is still parked; the
    /// sibling's pending comes back in the same flush, with no later event.
    #[test]
    fn a_sibling_parked_slot_returns_in_the_same_flush_when_the_hook_seals() {
        let mut e = crate::test_support::seeded_engine(Plans { idle: true, parallel: true, ..Default::default() });
        e.start(0.);
        let session = open(&mut e, 0.);
        e.venue.park_on = Some(session.clone());
        e.venue.park_second = Some(session.clone());
        e.venue.fail_in_ended = true;
        start(&mut e, 0.);
        assert_eq!(e.venue.park_result, Some(Ok(())));
        e.drain_actions().for_each(drop);
        let _ = Cx { now: 1., store: &mut e.store, io: &mut e.io }.retire_session(&session).unwrap();
        let actions = flush(&mut e, 2.);
        assert_eq!(prepares(&actions), 0);
        assert_eq!(e.venue.wait_ended.len(), 1, "the first ended in ready custody");
        assert_eq!(e.venue.wait_expired.len(), 1, "the sibling returned in the same flush");
        assert_eq!(e.venue.expired_while_live, [false]);
    }

    /// N2: a failed Read call's retry parks on a login Session in the failure
    /// callback, keeps the Plan alive with nothing sent, and resumes as exactly
    /// one more attempt on the same leg; the budget is never exceeded.
    #[test]
    fn a_failed_reads_retry_parks_and_resumes_as_one_more_attempt() {
        let mut e = crate::test_support::seeded_engine(Plans::default());
        e.start(0.);
        let session = open(&mut e, 0.);
        e.venue.park_retry_on = Some(session.clone());
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        e.handle(1., Event::HttpFailed { request: first.attempt().request, error: "login expired" });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(e.venue.retry_parks, [Ok(())], "the retry parks in the failure callback");
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. }))), "the Plan stays alive");
        assert_eq!(prepares(&actions), 0, "nothing is selected while parked");
        let mut cx = Cx { now: 2., store: &mut e.store, io: &mut e.io };
        let waiters = cx.session_ready(&session);
        let Some(crate::request_context::Waiter::Ready(leg, pending)) = waiters.into_iter().next() else {
            panic!("the parked retry resumes");
        };
        assert_eq!(leg, pending, "the same leg resumes");
        queue(&mut cx, leg, false).unwrap();
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(prepares(&actions), 1, "exactly one more attempt");
        let (second, _) = prepared(&actions);
        release(&mut e, second, 3.);
        e.handle(4., Event::HttpFailed { request: second.attempt().request, error: "login expired again" });
        assert_eq!(e.venue.retry_parks[1], Err(Error::RetryForbidden), "the declared budget is spent");
    }

    /// The sibling's Session is still live when the ended hook seals the Plan:
    /// the still-parked entry returns in the same flush through the sealed sweep.
    #[test]
    fn a_still_parked_sibling_on_another_session_returns_when_the_hook_seals() {
        let mut e = crate::test_support::seeded_engine(Plans { idle: true, parallel: true, ..Default::default() });
        e.start(0.);
        let first = open(&mut e, 0.);
        let second = open(&mut e, 0.);
        e.venue.park_on = Some(first.clone());
        e.venue.park_second = Some(second);
        e.venue.fail_in_ended = true;
        start(&mut e, 0.);
        assert_eq!(e.venue.park_result, Some(Ok(())));
        e.drain_actions().for_each(drop);
        let _ = Cx { now: 1., store: &mut e.store, io: &mut e.io }.retire_session(&first).unwrap();
        let actions = flush(&mut e, 2.);
        assert_eq!(prepares(&actions), 0);
        assert_eq!(e.venue.wait_ended.len(), 1);
        assert_eq!(e.venue.wait_expired.len(), 1, "the still-parked sibling returned in the same flush");
        assert_eq!(e.venue.expired_while_live, [false]);
    }

    /// A parked retry whose Session retires re-parks from its Plan's ready
    /// custody on the replacement, then resumes as one attempt.
    #[test]
    fn a_parked_retry_reparks_after_its_session_retires() {
        let mut e = crate::test_support::seeded_engine(Plans::default());
        e.start(0.);
        let old = open(&mut e, 0.);
        e.venue.park_retry_on = Some(old.clone());
        start(&mut e, 0.);
        let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
        release(&mut e, first, 0.);
        e.handle(1., Event::HttpFailed { request: first.attempt().request, error: "login expired" });
        assert_eq!(e.venue.retry_parks, [Ok(())]);
        e.drain_actions().for_each(drop);
        let replacement = open(&mut e, 2.);
        e.venue.repark_retry_on = Some(replacement.clone());
        let _ = Cx { now: 2., store: &mut e.store, io: &mut e.io }.retire_session(&old).unwrap();
        let actions = flush(&mut e, 3.);
        assert_eq!(prepares(&actions), 0);
        assert_eq!(e.venue.wait_ended.len(), 1, "ended once, re-parked in ready custody");
        let mut cx = Cx { now: 4., store: &mut e.store, io: &mut e.io };
        let Some(crate::request_context::Waiter::Ready(leg, _)) = cx.session_ready(&replacement).into_iter().next() else {
            panic!("the re-parked retry resumes on the replacement");
        };
        queue(&mut cx, leg, false).unwrap();
        assert_eq!(prepares(&e.drain_actions().collect::<Vec<_>>()), 1);
    }

    /// A no-delivery resume is issue custody for its own event only: a leg
    /// resumed but not queued is refused in the next engine event.
    #[test]
    fn a_resumed_but_unqueued_leg_is_refused_in_the_next_event() {
        let (mut e, _, session) = parked(0.);
        e.drain_actions().for_each(drop);
        assert!(e.io.session_resumed.capacity() >= 1, "reserved at park");
        let mut cx = Cx { now: 30_000., store: &mut e.store, io: &mut e.io };
        let Some(crate::request_context::Waiter::Ready(leg, _)) = cx.session_ready(&session).into_iter().next() else {
            panic!("resumes");
        };
        e.handle(30_001., Event::TimerFired { timer: TimerId(u64::MAX - 1) });
        e.drain_actions().for_each(drop);
        let mut cx = Cx { now: 30_002., store: &mut e.store, io: &mut e.io };
        let refused = queue(&mut cx, leg, false);
        assert_eq!(refused.map(|_| ()).map_err(|(_, error)| error), Err(Error::Revoked));
        assert_eq!(prepares(&e.drain_actions().collect::<Vec<_>>()), 0);
    }

    fn plan_outcome(actions: &[Action]) -> Option<crate::Outcome> {
        actions.iter().find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error.outcome),
            _ => None,
        }).expect("command failure")
    }
    #[test]
    fn inherited_uncertainty_survives_plan_admission_without_a_released_attempt() {
        for inherited in [false, true] {
            let mut e = crate::test_support::seeded_engine(Plans {
                write: true, idle: true, inherited, ..Default::default()
            });
            start(&mut e, 0.);
            let plan = e.venue.plan.clone().unwrap();
            let mut cx = Cx { now: 1., store: &mut e.store, io: &mut e.io };
            cx.fail_plan(&plan, "re-read left nothing to send").unwrap();
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
            assert_eq!(plan_outcome(&actions), Some(if inherited {
                crate::Outcome::Unknown } else { crate::Outcome::NotSent }));
        }
    }
    #[test]
    fn inherited_uncertainty_is_not_masked_by_an_effects_free_rejection() {
        for inherited in [false, true] {
            let mut e = crate::test_support::seeded_engine(Plans {
                write: true, inherited, ..Default::default()
            });
            start(&mut e, 0.);
            let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, first, 0.);
            response(&mut e, first, 1., "reject");
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(plan_outcome(&actions), Some(if inherited {
                crate::Outcome::Unknown } else { crate::Outcome::Rejected }));
        }
    }
    #[test]
    fn inherited_uncertainty_leaves_a_proven_plan_success_successful() {
        let mut e = crate::test_support::seeded_engine(Plans {
            write: true, inherited: true, ..Default::default()
        });
        start(&mut e, 0.);
        for now in [1., 2.] {
            let (token, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, token, now);
            response(&mut e, token, now, "ok");
        }
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. }))));
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. }))));
    }
}
