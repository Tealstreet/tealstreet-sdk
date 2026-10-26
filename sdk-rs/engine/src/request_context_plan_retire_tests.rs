mod retire_unissued_plan_calls {
    //! Originals for `retire_unissued_plan_call`: see
    //! docs/plans/RUST_PLAN_RETIRE_UNISSUED.md. Genuine admission, issue and
    //! finish; the venue retires chosen steps instead of sending them.
    use super::*;
    use crate::request_context::{
        Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
        plan::{CallBound, RetryWindow, Slot, StepBound},
    };

    #[derive(Default)]
    struct Skipper {
        steps: usize,
        dependent: bool,
        /// Step ordinals the venue retires instead of sending.
        skip: Vec<usize>,
        /// Retire the issued call too (it must be refused).
        retire_issued: bool,
        /// Retire the same slot twice (the second must be refused).
        retire_twice: bool,
        /// Calls per step (0 or 1: one call per step).
        width: usize,
        /// End with fail_plan even without a failed call.
        force_fail: bool,
        /// Every target is already closed at planning: complete before admission.
        all_closed: bool,
        /// Admit a serial Plan and try to retire its first call.
        #[cfg(feature = "serial-plans")]
        serial: bool,
        /// Admit one two-call barrier step and try to retire a member.
        #[cfg(feature = "http-mutation-barriers")]
        barrier: bool,
        plan: Option<Plan>,
        active: Vec<(Context, Slot)>,
        handled: usize,
        failed: bool,
        sent: usize,
        retires: Vec<Result<(), Error>>,
        /// Reserved contexts before and after each successful retirement.
        reserved: Vec<(usize, usize)>,
        /// (available slots, reserved contexts) before and after each refusal.
        refused_state: Vec<((usize, usize), (usize, usize))>,
    }
    impl Skipper {
        fn new(steps: usize, skip: &[usize]) -> Self {
            Self {
                steps,
                skip: skip.to_vec(),
                ..Default::default()
            }
        }
        fn snapshot(cx: &Cx<'_, Context>, plan: &Plan) -> (usize, usize) {
            (
                cx.plan_slots(plan).map_or(0, |slots| slots.len()),
                cx.io.core.request_contexts.plan_context_storage().2,
            )
        }
        fn next(&mut self, cx: &mut Cx<'_, Context>) {
            let plan = self.plan.clone().unwrap();
            loop {
                let slots = cx.plan_slots(&plan).unwrap();
                if slots.is_empty() {
                    if !self.active.is_empty() {
                        return;
                    }
                    if self.failed || self.force_fail {
                        cx.fail_plan(&plan, "batch had a failure").unwrap();
                    } else {
                        cx.done_plan(&plan, "[]".into()).unwrap();
                    }
                    return;
                }
                let mut issued = false;
                for slot in slots {
                    let step = self.handled;
                    self.handled += 1;
                    if !self.skip.contains(&step) {
                        self.issue(cx, &plan, slot);
                        issued = true;
                        continue;
                    }
                    let before = Self::snapshot(cx, &plan);
                    let result = cx.retire_unissued_plan_call(&plan, &slot);
                    self.retires.push(result);
                    if result.is_err() {
                        self.refused_state.push((before, Self::snapshot(cx, &plan)));
                        cx.fail_plan(&plan, "retirement refused").unwrap();
                        return;
                    }
                    self.reserved.push((before.1, Self::snapshot(cx, &plan).1));
                    if self.retire_twice {
                        let before = Self::snapshot(cx, &plan);
                        self.retires.push(cx.retire_unissued_plan_call(&plan, &slot));
                        self.refused_state.push((before, Self::snapshot(cx, &plan)));
                    }
                }
                if issued {
                    return;
                }
            }
        }
        fn issue(&mut self, cx: &mut Cx<'_, Context>, plan: &Plan, slot: Slot) {
            let context = cx.plan_leg_context(plan, &slot).unwrap();
            if self.retire_issued {
                let before = Self::snapshot(cx, plan);
                self.retires.push(cx.retire_unissued_plan_call(plan, &slot));
                self.refused_state.push((before, Self::snapshot(cx, plan)));
            }
            self.active.push((context.clone(), slot));
            self.sent += 1;
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Delete,
                "https://venue.test/cancel".into(),
                None,
                Box::new(Recipe),
                context,
            )
            .unwrap();
        }
        fn settled(&mut self, context: &Context) -> Slot {
            let index = self
                .active
                .iter()
                .position(|(active, _)| active == context)
                .expect("an active call");
            self.active.remove(index).1
        }
    }
    impl Venue for Skipper {
        fn http_mutation_barriers() -> bool {
            true
        }
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            let credential = Some(crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("fixture".into()),
            });
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: credential.clone(),
                })
                .unwrap();
            if self.all_closed {
                cx.done_empty_plan(&preflight, "[]".into()).unwrap();
                return;
            }
            #[cfg(feature = "serial-plans")]
            if self.serial {
                let (plan, slot) = cx
                    .admit_serial_plan(
                        &preflight,
                        crate::request_context::plan::SerialCall {
                            operation: Operation::Mutation,
                            timeout_ms: 100,
                            attempts: 1,
                        },
                        crate::request_context::plan::SerialPolicy {
                            credential: Some(credential.clone().unwrap()),
                            pacing: None,
                            refusals_complete: false,
                        },
                    )
                    .unwrap();
                self.retires.push(cx.retire_unissued_plan_call(&plan, &slot));
                cx.fail_plan(&plan, "serial refusal control").unwrap();
                return;
            }
            let call: Vec<CallBound<'_>> = (0..self.width.max(1))
                .map(|_| CallBound {
                    timeout_ms: 100,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                    after_refusal: &[],
                })
                .collect();
            let steps: Vec<StepBound<'_>> = (0..self.steps)
                .map(|_| StepBound {
                    wait_ms: 0,
                    calls: &call,
                })
                .collect();
            let policy = PlanPolicy {
                independent_calls: !self.dependent,
                operation: Operation::Mutation,
                credential,
                pacing: None,
                retry: RetryPolicy::Never,
            };
            #[cfg(feature = "http-mutation-barriers")]
            if self.barrier {
                let plan = cx
                    .admit_plan_with_barriers(
                        &preflight,
                        &steps,
                        policy,
                        &[crate::request_context::plan::MutationBarrierBound { step: 0 }],
                    )
                    .unwrap();
                let slot = cx.plan_slots(&plan).unwrap().remove(1);
                self.retires.push(cx.retire_unissued_plan_call(&plan, &slot));
                cx.fail_plan(&plan, "barrier refusal control").unwrap();
                return;
            }
            let plan = cx.admit_plan(&preflight, &steps, policy).unwrap();
            self.plan = Some(plan);
            self.next(cx);
        }
        fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: VenueError) {
            self.failed = true;
            let plan = self.plan.clone().unwrap();
            let slot = self.settled(&context);
            cx.finish_plan_failed_call(&plan, &slot, &context).unwrap();
            self.next(cx);
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
            let plan = self.plan.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            cx.http_write_fact(
                &context,
                crate::classify::WriteFact::Accepted {
                    order_ids: vec![format!("canceled-{}", self.sent)],
                },
                None,
            )
            .unwrap();
            let slot = self.settled(&context);
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            self.next(cx);
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("native".into())
        }
        fn unproven_http_evidence(
            &self,
            _: &Context,
            _: Response<'_>,
            _: &mut crate::request_context::UnprovenEvidence,
        ) {
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {}
        fn reconcile_uncertain(&mut self, _: &mut Cx<'_, Context>, _: Option<&[String]>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
            panic!("untyped failure");
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {
            panic!("internal timer escaped");
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn start(venue: Skipper) -> (Engine<Skipper>, Vec<Action>) {
        let mut e = crate::test_support::seeded_engine(venue);
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["a".into(), "b".into(), "c".into()],
            }),
        );
        let actions = e.drain_actions().collect();
        (e, actions)
    }
    fn release(e: &mut Engine<Skipper>, actions: &[Action], now: f64) -> RequestId {
        let preparation = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("the next unskipped call reaches physical preparation");
        e.http_prepared(
            now,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        let (attempt, work) = work(&e.drain_actions().collect::<Vec<_>>());
        e.handle(
            now,
            Event::Signed {
                attempt,
                result: Ok(crate::signing::SignResult {
                    values: vec![b"value".to_vec()],
                    receipt: work.receipt,
                    headers: vec![],
                }),
            },
        );
        let permit = bound(&e.drain_actions().collect::<Vec<_>>());
        e.claim_http(now, permit).unwrap();
        e.drain_actions().for_each(drop);
        preparation.attempt().request
    }
    fn answer(e: &mut Engine<Skipper>, request: RequestId, status: u16, now: f64) -> Vec<Action> {
        e.handle(
            now,
            Event::HttpResponse {
                request,
                response: Response {
                    status,
                    headers: "",
                    body: "native",
                },
            },
        );
        e.drain_actions().collect()
    }
    fn terminals(actions: &[Action]) -> Vec<Result<String, VenueError>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: 7, result }) => {
                    Some(Ok(result.clone()))
                }
                Action::Notice(Notice::CommandFailed { command: 7, error }) => {
                    Some(Err(error.clone()))
                }
                _ => None,
            })
            .collect()
    }
    fn prepares(actions: &[Action]) -> usize {
        actions
            .iter()
            .filter(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. })))
            .count()
    }

    #[test]
    fn a_retired_middle_step_lets_the_next_step_run_and_the_plan_complete() {
        let (mut e, actions) = start(Skipper::new(3, &[1]));
        let baseline = e.io.core.request_contexts.plan_context_storage().2;
        let first = release(&mut e, &actions, 0.);
        let actions = answer(&mut e, first, 200, 1.);
        assert_eq!(e.venue.retires, vec![Ok(())], "the skipped step retires");
        assert_eq!(prepares(&actions), 1, "step 2 is issued right after the retirement");
        let third = release(&mut e, &actions, 2.);
        let actions = answer(&mut e, third, 200, 3.);
        assert_eq!(terminals(&actions), vec![Ok("[]".into())], "one successful terminal");
        assert_eq!(e.venue.sent, 2, "exactly two physical calls");
        assert!(baseline >= 1);
        assert_eq!(
            e.io.core.request_contexts.plan_context_storage().2,
            0,
            "every reservation returned after the seal"
        );
    }

    #[test]
    fn retiring_the_last_step_completes_the_schedule() {
        let (mut e, actions) = start(Skipper::new(2, &[1]));
        let first = release(&mut e, &actions, 0.);
        let actions = answer(&mut e, first, 200, 1.);
        assert_eq!(e.venue.retires, vec![Ok(())]);
        assert_eq!(terminals(&actions), vec![Ok("[]".into())], "done_plan succeeds");
        assert_eq!(e.venue.sent, 1);
    }

    #[test]
    fn a_retirement_after_a_sibling_failure_adds_no_journal_fact() {
        let run = |skip: &[usize], steps: usize| {
            let (mut e, actions) = start(Skipper::new(steps, skip));
            let first = release(&mut e, &actions, 0.);
            let actions = answer(&mut e, first, 503, 1.);
            assert_eq!(e.venue.retires, vec![Ok(()); skip.len()], "retired after the failure");
            let last = release(&mut e, &actions, 2.);
            let actions = answer(&mut e, last, 200, 3.);
            (e.venue.retires.clone(), terminals(&actions))
        };
        let (retires, retired) = run(&[1], 3);
        let (_, control) = run(&[], 2);
        assert_eq!(retires, vec![Ok(())]);
        let (Err(retired), Err(control)) = (&retired[0], &control[0]) else {
            panic!("both runs fail once: {retired:?} vs {control:?}");
        };
        assert_eq!(retired.outcome, control.outcome, "the retired step adds no fact");
        assert_eq!(retired.kind, control.kind);
        assert_eq!(
            retired.partial.as_ref().map(|p| &p.order_ids),
            control.partial.as_ref().map(|p| &p.order_ids)
        );
    }

    #[test]
    fn a_retirement_frees_exactly_one_reserved_context() {
        let (mut e, actions) = start(Skipper::new(3, &[1]));
        let first = release(&mut e, &actions, 0.);
        answer(&mut e, first, 200, 1.);
        assert_eq!(e.venue.retires, vec![Ok(())]);
        let (before, after) = e.venue.reserved[0];
        assert_eq!(after + 1, before, "one reserved context returned");
    }

    #[test]
    fn a_dependent_plan_refuses_retirement_unchanged() {
        let (mut e, actions) = start(Skipper {
            dependent: true,
            ..Skipper::new(3, &[1])
        });
        let first = release(&mut e, &actions, 0.);
        answer(&mut e, first, 200, 1.);
        assert_eq!(e.venue.retires, vec![Err(Error::Invalid)]);
        let (before, after) = e.venue.refused_state[0];
        assert_eq!(before, after, "nothing changed");
    }

    #[test]
    fn an_issued_or_twice_retired_slot_is_revoked_unchanged() {
        let (e, _) = start(Skipper {
            retire_issued: true,
            ..Skipper::new(2, &[])
        });
        assert_eq!(e.venue.retires, vec![Err(Error::Revoked)], "an issued call");
        assert_eq!(e.venue.refused_state[0].0, e.venue.refused_state[0].1);
        let (mut e, actions) = start(Skipper {
            retire_twice: true,
            ..Skipper::new(3, &[1])
        });
        let first = release(&mut e, &actions, 0.);
        answer(&mut e, first, 200, 1.);
        assert_eq!(e.venue.retires, vec![Ok(()), Err(Error::Revoked)], "a retired call");
        assert_eq!(e.venue.refused_state[0].0, e.venue.refused_state[0].1);
    }

    #[test]
    fn retirement_outside_issue_custody_or_after_expiry_is_refused() {
        let (mut e, _) = start(Skipper::new(2, &[]));
        let plan = e.venue.plan.clone().unwrap();
        let mut cx = Cx {
            now: 0.5,
            store: &mut e.store,
            io: &mut e.io,
        };
        // Step 0 is issued and in flight; step 1 is not yet available, so name
        // the only slot a venue could hold: the active one is refused too.
        let slot = e.venue.active[0].1.clone();
        assert_eq!(
            cx.retire_unissued_plan_call(&plan, &slot),
            Err(Error::Revoked),
            "outside custody"
        );
        let mut cx = Cx {
            now: 1_000.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert_eq!(
            cx.retire_unissued_plan_call(&plan, &slot),
            Err(Error::TimedOut),
            "an expired owner refuses with validate_plan's TimedOut"
        );
    }

    #[test]
    fn every_step_retired_at_entry_cannot_succeed() {
        // 4yjlw0 (a)(b), dazzy3 P1 as amended: consecutive retirements at
        // admission custody are allowed, but a Mutation Plan with no confirmed
        // accepted write cannot succeed; done_plan projects NotSent.
        let (e, actions) = start(Skipper::new(3, &[0, 1, 2]));
        assert_eq!(e.venue.retires, vec![Ok(()), Ok(()), Ok(())]);
        assert_eq!(e.venue.sent, 0);
        assert_eq!(prepares(&actions), 0);
        let [Err(error)] = &terminals(&actions)[..] else {
            panic!("one failure terminal: {:?}", terminals(&actions));
        };
        assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
        assert_eq!(e.io.core.request_contexts.plan_context_storage().2, 0);
    }

    #[test]
    fn all_closed_targets_complete_natively_before_admission() {
        // Native []: a venue whose targets are all closed never admits a Plan.
        let (e, actions) = start(Skipper {
            all_closed: true,
            ..Skipper::new(3, &[])
        });
        assert_eq!(terminals(&actions), vec![Ok("[]".into())]);
        assert_eq!(prepares(&actions), 0);
        assert!(e.venue.plan.is_none());
    }

    #[test]
    fn every_step_retired_then_failed_projects_not_sent() {
        let (e, actions) = start(Skipper {
            force_fail: true,
            ..Skipper::new(2, &[0, 1])
        });
        assert_eq!(e.venue.retires, vec![Ok(()), Ok(())]);
        let [Err(error)] = &terminals(&actions)[..] else {
            panic!("one failure terminal: {:?}", terminals(&actions));
        };
        assert_eq!(error.outcome, Some(crate::Outcome::NotSent), "nothing was issued");
    }

    #[test]
    fn a_second_available_call_retires_while_the_first_is_active() {
        // dazzy3 P2: one step of two calls exposes both as Available.
        let (mut e, actions) = start(Skipper {
            width: 2,
            ..Skipper::new(1, &[1])
        });
        assert_eq!(e.venue.retires, vec![Ok(())], "the non-first Available call retires");
        let first = release(&mut e, &actions, 0.);
        let actions = answer(&mut e, first, 200, 1.);
        assert_eq!(terminals(&actions), vec![Ok("[]".into())]);
        assert_eq!(e.venue.sent, 1);
    }

    #[cfg(feature = "serial-plans")]
    #[test]
    fn a_serial_plan_refuses_retirement() {
        let (e, _) = start(Skipper {
            serial: true,
            ..Skipper::new(1, &[])
        });
        assert_eq!(e.venue.retires, vec![Err(Error::State)]);
    }

    #[cfg(feature = "http-mutation-barriers")]
    #[test]
    fn a_barrier_group_member_refuses_retirement() {
        let (e, _) = start(Skipper {
            barrier: true,
            width: 2,
            ..Skipper::new(1, &[])
        });
        assert_eq!(e.venue.retires, vec![Err(Error::Invalid)]);
    }
}
