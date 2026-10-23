mod independent_plan_calls {
    use super::*;
    use crate::request_context::{
        Plan, PlanPolicy, PreflightPolicy, PreflightPurpose,
        plan::{CallBound, RetryWindow, Slot, StepBound},
    };

    const TABLE: crate::classify::CodeTable = crate::classify::CodeTable {
        rows: &[],
        success: &[],
        unlisted: crate::classify::Verdict::Unknown(None),
        status: &[],
    };

    struct Batch {
        independent: bool,
        settle: bool,
        read: bool,
        plan: Option<Plan>,
        active: Option<Slot>,
        callbacks: Vec<VenueError>,
        responses: usize,
        retired: usize,
    }
    impl Batch {
        fn new(independent: bool, settle: bool) -> Self {
            Self {
                independent,
                settle,
                read: false,
                plan: None,
                active: None,
                callbacks: Vec::new(),
                responses: 0,
                retired: 0,
            }
        }
        fn next(&mut self, cx: &mut Cx<'_, Context>) {
            let plan = self.plan.as_ref().unwrap();
            let Some(slot) = cx.plan_slots(plan).unwrap().into_iter().next() else {
                cx.fail_plan(plan, "batch finished").unwrap();
                return;
            };
            let context = cx.plan_leg_context(plan, &slot).unwrap();
            self.active = Some(slot);
            cx.request_signed_managed(
                context.clone(),
                Method::Post,
                "https://venue.test/independent".into(),
                None,
                Box::new(Recipe),
                context,
            )
            .unwrap();
        }
    }
    impl Venue for Batch {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            let credential = (!self.read).then(|| crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("fixture".into()),
            });
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: if self.read {
                        PreflightPurpose::ReadOnly
                    } else {
                        PreflightPurpose::WriteAfterReads
                    },
                    read_credential: None,
                    write_credential: credential.clone(),
                })
                .unwrap();
            let call = [CallBound {
                timeout_ms: 100,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            }];
            let steps = [
                StepBound {
                    wait_ms: 0,
                    calls: &call,
                },
                StepBound {
                    wait_ms: 0,
                    calls: &call,
                },
            ];
            let ids = cx.io.next_id;
            let storage = cx.io.request_contexts.plan_context_storage();
            let action_count = cx.io.actions.len();
            let admitted = cx.admit_plan(
                &preflight,
                &steps,
                PlanPolicy {
                    independent_calls: self.independent,
                    operation: if self.read {
                        Operation::Read
                    } else {
                        Operation::Mutation
                    },
                    credential,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            );
            if self.read && self.independent {
                assert_eq!(admitted.unwrap_err(), Error::Invalid);
                assert_eq!(cx.io.next_id, ids);
                assert_eq!(cx.io.request_contexts.plan_context_storage(), storage);
                assert_eq!(cx.io.actions.len(), action_count);
                cx.fail_preflight(&preflight, "read refused atomically")
                    .unwrap();
                return;
            }
            self.plan = Some(admitted.unwrap());
            if self.read {
                cx.fail_plan(self.plan.as_ref().unwrap(), "read admission control")
                    .unwrap();
            } else {
                self.next(cx);
            }
        }
        fn on_http_failed(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            error: VenueError,
        ) {
            assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
            assert_eq!(error.partial.as_ref().unwrap().order_ids, ["unproven-id"]);
            assert_eq!(
                cx.http_write_fact(
                    &context,
                    crate::classify::WriteFact::Accepted {
                        order_ids: vec!["forged".into()]
                    },
                    None,
                )
                .unwrap_err(),
                Error::State,
            );
            assert_eq!(
                cx.http_write_failure(&context, &TABLE, None, "no grant", None)
                    .unwrap_err(),
                Error::State,
            );
            self.callbacks.push(error);
            if self.settle {
                cx.finish_plan_failed_call(
                    self.plan.as_ref().unwrap(),
                    self.active.as_ref().unwrap(),
                    &context,
                )
                .unwrap();
                self.next(cx);
            }
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            assert_eq!(response.status, 200, "unproven reply reached DATA codec");
            let plan = self.plan.as_ref().unwrap();
            cx.validate_plan_delivery(plan, &context).unwrap();
            cx.http_write_fact(
                &context,
                crate::classify::WriteFact::Accepted {
                    order_ids: vec!["accepted-id".into()],
                },
                None,
            )
            .unwrap();
            cx.finish_plan_call(plan, self.active.as_ref().unwrap(), &context)
                .unwrap();
            self.responses += 1;
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
            evidence: &mut crate::request_context::UnprovenEvidence,
        ) {
            evidence.order_id("unproven-id");
        }
        fn on_http_owner_retired(&mut self, _: &crate::request_context::HttpOwnerRetirement) {
            self.retired += 1;
        }
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
    fn setup(independent: bool, settle: bool) -> (Engine<Batch>, Vec<Action>) {
        let mut e = crate::test_support::seeded_engine(Batch::new(independent, settle));
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::PlaceOrders {
                command: 7,
                orders: vec![
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
                    )
                    .unwrap(),
                ],
            }),
        );
        let actions = e.drain_actions().collect();
        (e, actions)
    }
    fn release(e: &mut Engine<Batch>, actions: &[Action], now: f64) -> RequestId {
        let preparation = actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("next independent call reaches physical preparation");
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
    fn response(e: &mut Engine<Batch>, request: RequestId, status: u16, now: f64) -> Vec<Action> {
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
    fn failures(actions: &[Action]) -> Vec<&VenueError> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn unproven_independent_leg_settles_then_synchronously_submits_its_sibling() {
        for status in [408, 503] {
            let (mut e, actions) = setup(true, true);
            let first = release(&mut e, &actions, 0.);
            let actions = response(&mut e, first, status, 1.);
            assert_eq!(
                e.venue.callbacks.len(),
                1,
                "unproven leg bypassed typed failure callback"
            );
            assert_eq!(
                e.venue.callbacks[0].kind,
                Some(if status == 408 {
                    ErrorKind::Timeout
                } else {
                    ErrorKind::VenueUnavailable
                })
            );
            assert!(
                failures(&actions).is_empty(),
                "synchronous sibling was auto-sealed"
            );
            assert_eq!(e.venue.retired, 0);
            let second = release(&mut e, &actions, 2.);
            let actions = response(&mut e, second, 200, 3.);
            assert_eq!(e.venue.responses, 1);
            assert_eq!(failures(&actions).len(), 1);
            let error = failures(&actions)[0];
            assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
            assert_eq!(
                error.partial.as_ref().unwrap().order_ids,
                ["unproven-id", "accepted-id"]
            );
            assert_eq!(e.venue.retired, 1);
        }
    }
    #[test]
    fn declined_independent_failure_auto_seals_once_and_retires_unissued_suffix() {
        let (mut e, actions) = setup(true, false);
        let first = release(&mut e, &actions, 0.);
        let actions = response(&mut e, first, 503, 1.);
        assert_eq!(e.venue.callbacks.len(), 1, "declining callback never ran");
        assert_eq!(failures(&actions).len(), 1);
        assert_eq!(failures(&actions)[0].outcome, Some(crate::Outcome::Unknown));
        assert_eq!(e.venue.retired, 1);
        assert!(e.io.managed_http.is_empty());
        assert!(!actions.iter().any(
            |a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{..}))
        ));
    }
    #[test]
    fn default_plan_keeps_direct_abort_without_a_native_failure_callback() {
        let (mut e, actions) = setup(false, true);
        let first = release(&mut e, &actions, 0.);
        let actions = response(&mut e, first, 503, 1.);
        assert!(e.venue.callbacks.is_empty());
        assert_eq!(failures(&actions).len(), 1);
        assert_eq!(failures(&actions)[0].outcome, Some(crate::Outcome::Unknown));
        assert_eq!(e.venue.retired, 1);
    }
    #[test]
    fn duplicate_unproven_delivery_cannot_repeat_the_callback_or_retire_the_sibling() {
        let (mut e, actions) = setup(true, true);
        let first = release(&mut e, &actions, 0.);
        let actions = response(&mut e, first, 503, 1.);
        assert_eq!(e.venue.callbacks.len(), 1);
        let second = release(&mut e, &actions, 2.);
        let duplicate = response(&mut e, first, 503, 3.);
        assert!(failures(&duplicate).is_empty());
        assert_eq!(e.venue.callbacks.len(), 1);
        assert_eq!(e.venue.retired, 0);
        let terminal = response(&mut e, second, 200, 4.);
        assert_eq!(failures(&terminal).len(), 1);
        assert!(response(&mut e, first, 408, 5.).is_empty());
        assert_eq!(e.venue.callbacks.len(), 1);
        assert_eq!(e.venue.retired, 1);
    }
    #[test]
    fn read_plan_refuses_independence_before_mutating_admission_state() {
        let mut e = crate::test_support::seeded_engine(Batch {
            read: true,
            ..Batch::new(true, false)
        });
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "BTCUSD".into(),
                interval: crate::model::Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    #[test]
    fn read_plan_without_independence_still_admits() {
        let mut e = crate::test_support::seeded_engine(Batch {
            read: true,
            ..Batch::new(false, false)
        });
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "BTCUSD".into(),
                interval: crate::model::Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            }),
        );
        assert!(e.venue.plan.is_some());
    }
}
