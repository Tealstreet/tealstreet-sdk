mod reconciled_writes {
    use super::*;
    use crate::request_context::{
        Operation, Plan, PlanPolicy, Preflight, PreflightPolicy, PreflightPurpose, ResendGrant,
        plan::{CallBound, RetryWindow, Slot, StepBound},
    };

    const COMMAND: CommandId = 7;

    /// Parent: no declaration or proof type exists, so these mirror the
    /// proposed shapes and every adapter reports the missing capability.
    #[allow(dead_code)]
    mod api {
        pub struct ReconcileBound {
            pub anchor: usize,
            pub reads: u16,
            pub read_timeout_ms: u32,
            pub rounds: u8,
        }
        pub enum Proof {
            Accepted { order_ids: Vec<String> },
            Absent,
        }
    }

    fn credential() -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        }
    }

    // Adapters for the API under test, so each control names one operation.
    fn admit(cx: &mut Cx<'_, Context>, preflight: &Preflight, steps: &[StepBound<'_>]) -> Plan {
        let _ = [api::ReconcileBound {
            anchor: 0,
            reads: 2,
            read_timeout_ms: 5_000,
            rounds: 2,
        }];
        cx.admit_plan(
            preflight,
            steps,
            PlanPolicy {
                independent_calls: false,
                operation: Operation::Mutation,
                credential: Some(credential()),
                pacing: None,
                retry: RetryPolicy::DefinitiveRefusal,
            },
        )
        .unwrap()
    }

    fn reconcile_read(
        cx: &mut Cx<'_, Context>,
        plan: &Plan,
        slot: &Slot,
        anchor: &Context,
    ) -> Result<Context, Error> {
        let _ = (cx, plan, slot, anchor);
        Err(Error::Invalid)
    }

    fn prove(
        cx: &mut Cx<'_, Context>,
        plan: &Plan,
        slot: &Slot,
        read: &Context,
        proof: api::Proof,
    ) -> Result<Option<ResendGrant>, Error> {
        let _ = (cx, plan, slot, read, proof);
        Err::<Option<ResendGrant>, _>(Error::Invalid)
    }

    #[derive(Default)]
    struct Recon {
        plan: Option<Plan>,
        slot: Option<Slot>,
        anchor: Option<Context>,
        reads: Vec<Context>,
        issued: Vec<Result<(), Error>>,
        proofs: Vec<Result<bool, Error>>,
        resent: usize,
        fail_after_proof_refusal: bool,
        /// Venue session/snapshot continuity as the engine samples it.
        continuity: std::cell::Cell<Option<u64>>,
        /// Sign the anchor without a protocol expiry receipt.
        no_expiry: bool,
        /// The anchor's DATA reports a partial, unproven outcome.
        partial: bool,
    }

    /// Recipe without an expiry receipt: no absence fence can be computed.
    struct NoExpiry;
    impl crate::SignStep for NoExpiry {
        fn prepare(
            &mut self,
            outbound: &mut Action,
            context: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            let mut request = Recipe.prepare(outbound, context)?;
            request.receipt.expiry_ms = None;
            Ok(request)
        }
        fn apply(
            &mut self,
            outbound: &mut Action,
            result: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            Recipe.apply(outbound, result)
        }
        fn freshness_ms(&self) -> f64 {
            3_000.
        }
    }

    impl Recon {
        #[allow(dead_code)] // Parent has no continuity hook to implement.
        fn http_continuity(&self, _: &Context) -> Option<u64> {
            self.continuity.get()
        }
    }

    fn queue_write(cx: &mut Cx<'_, Context>, context: Context, no_expiry: bool) {
        let recipe: Box<dyn crate::SignStep> = if no_expiry {
            Box::new(NoExpiry)
        } else {
            Box::new(Recipe)
        };
        cx.request_signed_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/write".into(),
            None,
            recipe,
            context,
        )
        .unwrap();
    }

    impl Recon {
        fn read(&mut self, cx: &mut Cx<'_, Context>) {
            let (plan, slot) = (self.plan.clone().unwrap(), self.slot.clone().unwrap());
            let anchor = self.anchor.clone().unwrap();
            match reconcile_read(cx, &plan, &slot, &anchor) {
                Ok(read) => {
                    cx.request_signed_managed(
                        read.clone(),
                        Method::Get,
                        "https://venue.test/orders".into(),
                        None,
                        Box::new(Recipe),
                        read.clone(),
                    )
                    .unwrap();
                    self.reads.push(read);
                    self.issued.push(Ok(()));
                }
                Err(error) => {
                    self.issued.push(Err(error));
                    cx.fail_plan(&plan, "reconciliation unavailable").unwrap();
                }
            }
        }

        fn proof(&mut self, cx: &mut Cx<'_, Context>, read: &Context, proof: api::Proof) {
            let (plan, slot) = (self.plan.clone().unwrap(), self.slot.clone().unwrap());
            match prove(cx, &plan, &slot, read, proof) {
                Ok(Some(grant)) => {
                    self.proofs.push(Ok(true));
                    let anchor = self.anchor.clone().unwrap();
                    let resend = cx
                        .plan_resend_context(&plan, &slot, &anchor, grant)
                        .unwrap();
                    queue_write(cx, resend, false);
                    self.resent += 1;
                }
                Ok(None) => self.proofs.push(Ok(false)),
                Err(error) => {
                    self.proofs.push(Err(error));
                    if self.fail_after_proof_refusal {
                        cx.fail_plan(&plan, "reconciliation inconclusive").unwrap();
                    }
                }
            }
        }
    }

    impl Venue for Recon {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, _: &mut Cx<'_, Context>, _: Command) {}
        fn on_venue_action(&mut self, cx: &mut Cx<'_, Context>, _: CommandId, _: VenueAction) {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential()),
                })
                .unwrap();
            let calls = [CallBound {
                timeout_ms: 5_000,
                attempts: 2,
                retry_window: RetryWindow::FreshAttempt,
                after_refusal: &[],
            }];
            let plan = admit(
                cx,
                &preflight,
                &[StepBound {
                    wait_ms: 0,
                    calls: &calls,
                }],
            );
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            let anchor = cx.plan_leg_context(&plan, &slot).unwrap();
            queue_write(cx, anchor.clone(), self.no_expiry);
            self.plan = Some(plan);
            self.slot = Some(slot);
            self.anchor = Some(anchor);
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Context>,
            context: Context,
            response: Response<'_>,
        ) {
            let (plan, slot) = (self.plan.clone().unwrap(), self.slot.clone().unwrap());
            if Some(&context) == self.anchor.as_ref() && self.partial {
                cx.validate_plan_delivery(&plan, &context).unwrap();
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::PartialUnknown(
                        VenueError::new("batch partly unknown").partial_ids(&["native-1".into()]),
                    ),
                )
                .unwrap();
                self.read(cx);
                return;
            }
            if Some(&context) == self.anchor.as_ref() {
                cx.validate_plan_delivery(&plan, &context).unwrap();
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Accepted {
                        order_ids: vec!["native-1".into()],
                    },
                )
                .unwrap();
                cx.validate_http_write_effects(&context).unwrap();
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                cx.done_plan(&plan, "placed".into()).unwrap();
                return;
            }
            match response.body {
                "found" => self.proof(
                    cx,
                    &context,
                    api::Proof::Accepted {
                        order_ids: vec!["native-1".into()],
                    },
                ),
                "empty" if self.reads.len() % 2 == 1 => self.read(cx),
                "empty" => self.proof(cx, &context, api::Proof::Absent),
                _ => unreachable!(),
            }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) {
            if Some(&context) == self.anchor.as_ref() {
                self.read(cx);
            }
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("reconciled".into())
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn prepared(actions: &[Action]) -> Vec<(Preparation, HttpRequest)> {
        actions
            .iter()
            .filter_map(|a| match a {
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
            .collect()
    }

    /// Prepare, sign and release one attempt; `sign` false refuses the signer.
    fn release(engine: &mut Engine<Recon>, token: Preparation, now: f64, sign: bool) {
        engine.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        let (attempt, work) = work(&actions);
        let result = if sign {
            Ok(crate::signing::SignResult::new(
                vec![b"value".to_vec()],
                work.receipt,
            ))
        } else {
            Err(crate::signing::SignError::InvalidResult)
        };
        engine.handle(now, Event::Signed { attempt, result });
        if !sign {
            return;
        }
        let permit = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("binding");
        assert!(matches!(
            engine.claim_http(now, permit),
            Ok(Claim::Released(_))
        ));
    }

    fn reply(engine: &mut Engine<Recon>, token: Preparation, now: f64, body: &str) {
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

    fn lose(engine: &mut Engine<Recon>, token: Preparation, now: f64) {
        engine.handle(
            now,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "connection reset",
            },
        );
    }

    fn terminal(actions: &[Action]) -> Vec<Result<String, VenueError>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: COMMAND,
                    result,
                }) => Some(Ok(result.clone())),
                Action::Notice(Notice::CommandFailed {
                    command: COMMAND,
                    error,
                }) => Some(Err(error.clone())),
                _ => None,
            })
            .collect()
    }

    fn writes(actions: &[(Preparation, HttpRequest)]) -> usize {
        actions
            .iter()
            .filter(|(_, request)| request.method == Method::Post)
            .count()
    }

    fn probe() -> Recon {
        Recon {
            continuity: std::cell::Cell::new(Some(1)),
            ..Default::default()
        }
    }

    /// One write lost after release, then its first reconciliation read.
    fn lost_write(probe: Recon, read_at: f64) -> (Engine<Recon>, Vec<Action>, Preparation) {
        let mut e = crate::test_support::seeded_engine(probe);
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::Venue {
                command: COMMAND,
                action: VenueAction::FetchBalanceInfo,
            }),
        );
        let mut log = e.drain_actions().collect::<Vec<_>>();
        let write = prepared(&log)[0].0;
        release(&mut e, write, 0., true);
        lose(&mut e, write, 10.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        assert_eq!(
            e.venue.issued,
            vec![Ok(())],
            "reconciliation read after a lost write"
        );
        let read = prepared(&actions)[0].0;
        release(&mut e, read, read_at, true);
        (e, log, read)
    }

    /// Both reads of one round come back empty.
    fn absent_round(e: &mut Engine<Recon>, log: &mut Vec<Action>, first: Preparation, at: f64) {
        reply(e, first, at, "empty");
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        let second = prepared(&actions)[0].0;
        release(e, second, at + 10., true);
        reply(e, second, at + 20., "empty");
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn a_lost_write_found_by_reconciliation_completes_accepted() {
        let (mut e, mut log, read) = lost_write(probe(), 20.);
        reply(&mut e, read, 30., "found");
        log.extend(e.drain_actions());
        assert_eq!(e.venue.proofs, vec![Ok(false)]);
        assert_eq!(terminal(&log), vec![Ok("placed".into())]);
        assert_eq!(writes(&prepared(&log)), 1, "a found order is never resent");
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn complete_absence_after_signature_expiry_grants_one_same_id_resend() {
        let (mut e, mut log, read) = lost_write(probe(), 10_000.);
        absent_round(&mut e, &mut log, read, 10_100.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        assert_eq!(e.venue.proofs, vec![Ok(true)]);
        let resend = prepared(&actions)[0].0;
        release(&mut e, resend, 10_200., true);
        reply(&mut e, resend, 10_300., "{}");
        log.extend(e.drain_actions());
        assert_eq!(terminal(&log), vec![Ok("placed".into())]);
        assert_eq!(writes(&prepared(&log)), 2);
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn absence_inside_the_signature_window_mints_no_resend() {
        let probe = Recon {
            fail_after_proof_refusal: true,
            ..probe()
        };
        let (mut e, mut log, read) = lost_write(probe, 20.);
        absent_round(&mut e, &mut log, read, 30.);
        log.extend(e.drain_actions());
        assert!(
            matches!(e.venue.proofs.as_slice(), [Err(_)]),
            "{:?}",
            e.venue.proofs
        );
        assert_eq!(e.venue.resent, 0);
        let terminal = terminal(&log);
        assert_eq!(terminal.len(), 1);
        assert_eq!(
            terminal[0].clone().unwrap_err().outcome,
            Some(crate::Outcome::Unknown)
        );
        assert_eq!(writes(&prepared(&log)), 1);
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn a_refused_resend_after_proven_absence_stays_unknown() {
        let (mut e, mut log, read) = lost_write(probe(), 10_000.);
        absent_round(&mut e, &mut log, read, 10_100.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        let resend = prepared(&actions)[0].0;
        release(&mut e, resend, 10_200., false);
        log.extend(e.drain_actions());
        let terminal = terminal(&log);
        assert_eq!(terminal.len(), 1);
        let error = terminal[0].clone().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown), "{error:?}");
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn a_failed_reconciliation_read_leaves_the_write_unknown() {
        let (mut e, mut log, read) = lost_write(probe(), 20.);
        lose(&mut e, read, 30.);
        log.extend(e.drain_actions());
        assert_eq!(e.venue.resent, 0);
        let terminal = terminal(&log);
        assert_eq!(terminal.len(), 1);
        assert_eq!(
            terminal[0].clone().unwrap_err().outcome,
            Some(crate::Outcome::Unknown)
        );
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn the_declared_attempts_cap_same_id_sends_across_rounds() {
        let probe = Recon {
            fail_after_proof_refusal: true,
            ..probe()
        };
        let (mut e, mut log, read) = lost_write(probe, 10_000.);
        absent_round(&mut e, &mut log, read, 10_100.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        let resend = prepared(&actions)[0].0;
        release(&mut e, resend, 10_200., true);
        lose(&mut e, resend, 10_300.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        let read = prepared(&actions)[0].0;
        release(&mut e, read, 20_000., true);
        absent_round(&mut e, &mut log, read, 20_100.);
        log.extend(e.drain_actions());
        assert_eq!(e.venue.proofs.len(), 2);
        assert!(e.venue.proofs[1].is_err(), "no third same-ID send");
        assert_eq!(writes(&prepared(&log)), 2);
        let terminal = terminal(&log);
        assert_eq!(terminal.len(), 1);
        assert_eq!(
            terminal[0].clone().unwrap_err().outcome,
            Some(crate::Outcome::Unknown)
        );
    }

    fn refused_absence(probe: Recon, between: impl FnOnce(&mut Engine<Recon>)) {
        let probe = Recon {
            fail_after_proof_refusal: true,
            ..probe
        };
        let (mut e, mut log, read) = lost_write(probe, 10_000.);
        between(&mut e);
        absent_round(&mut e, &mut log, read, 10_100.);
        log.extend(e.drain_actions());
        assert!(
            matches!(e.venue.proofs.as_slice(), [Err(_)]),
            "{:?}",
            e.venue.proofs
        );
        assert_eq!(e.venue.resent, 0);
        assert_eq!(writes(&prepared(&log)), 1);
        let terminal = terminal(&log);
        assert_eq!(terminal.len(), 1);
        assert_eq!(
            terminal[0].clone().unwrap_err().outcome,
            Some(crate::Outcome::Unknown)
        );
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn a_missing_signature_expiry_never_grants_absence() {
        refused_absence(
            Recon {
                no_expiry: true,
                ..probe()
            },
            |_| {},
        );
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn changed_session_continuity_refuses_the_absence_proof() {
        refused_absence(probe(), |e| e.venue.continuity.set(Some(2)));
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn unknown_continuity_refuses_the_absence_proof() {
        refused_absence(
            Recon {
                continuity: std::cell::Cell::new(None),
                ..Default::default()
            },
            |_| {},
        );
    }

    #[test]
    fn a_partial_unknown_write_never_grants_a_whole_leg_resend() {
        let probe = Recon {
            fail_after_proof_refusal: true,
            partial: true,
            ..probe()
        };
        let mut e = crate::test_support::seeded_engine(probe);
        e.start(0.);
        e.handle(
            0.,
            Event::Command(Command::Venue {
                command: COMMAND,
                action: VenueAction::FetchBalanceInfo,
            }),
        );
        let mut log = e.drain_actions().collect::<Vec<_>>();
        let write = prepared(&log)[0].0;
        release(&mut e, write, 0., true);
        reply(&mut e, write, 10., "partial");
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        if let [(read, _)] = prepared(&actions).as_slice() {
            release(&mut e, *read, 10_000., true);
            absent_round(&mut e, &mut log, *read, 10_100.);
        }
        log.extend(e.drain_actions());
        assert_eq!(e.venue.resent, 0);
        assert_eq!(writes(&prepared(&log)), 1);
        let terminal = terminal(&log);
        assert_eq!(terminal.len(), 1);
        let error = terminal[0].clone().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Unknown));
        assert_eq!(
            error.partial.map(|p| p.order_ids),
            Some(vec!["native-1".to_owned()])
        );
    }

    #[test]
    #[ignore = "held until the reconciliation runtime clears API fit"]
    fn a_prior_round_read_cannot_prove_the_current_attempt() {
        let (mut e, mut log, read) = lost_write(probe(), 10_000.);
        absent_round(&mut e, &mut log, read, 10_100.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        log.extend(actions.iter().cloned());
        let resend = prepared(&actions)[0].0;
        release(&mut e, resend, 10_200., true);
        let stale = e.venue.reads.last().unwrap().clone();
        let (plan, slot) = (e.venue.plan.clone().unwrap(), e.venue.slot.clone().unwrap());
        let mut cx = Cx {
            now: 10_250.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert!(prove(&mut cx, &plan, &slot, &stale, api::Proof::Absent).is_err());
        assert!(reconcile_read(&mut cx, &plan, &slot, &stale).is_err());
    }
}
