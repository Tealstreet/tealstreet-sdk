// Row-attributed planned executions (AGT-145): parked followers adopted into
// one serial write, each sealed with only its own rows.
mod row_execution {
    use super::*;
    use crate::request_context::{
        Plan, Preflight, PreflightPolicy, PreflightPurpose,
        plan::{SerialCall, SerialPolicy, Slot},
    };
    use crate::write_plan::{Element, ElementOutcome, ElementResult};

    const FOUNDER: CommandId = 7;
    const FIRST: CommandId = 8;
    const SECOND: CommandId = 9;
    const FOREIGN: CommandId = 10;

    fn credential(reference: &str) -> crate::request_context::Credential {
        crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef(reference.into()),
        }
    }

    fn call() -> SerialCall {
        SerialCall {
            auth: Default::default(),
            operation: Operation::Mutation,
            timeout_ms: 20_000,
            attempts: 1,
            pacing: None,
        }
    }

    #[derive(Clone, Copy, Default, PartialEq)]
    enum Answer {
        #[default]
        Placed,
        /// The venue names the second row refused; the others placed.
        OneRefused,
    }

    #[derive(Default)]
    struct Rows {
        answer: Answer,
        /// Adopt with a gap after the founder's rows.
        gap: bool,
        /// Try one more adoption after the write is issued.
        late: bool,
        /// Try `done_plan` before `done_row_plan`.
        plain_done: bool,
        parked: Vec<Preflight>,
        adopted: Vec<Result<(), Error>>,
        refusals: Vec<Error>,
        plan: Option<Plan>,
        slot: Option<Slot>,
    }

    fn placed(id: &str) -> ElementOutcome {
        ElementOutcome {
            element: Element {
                index: 0,
                client_order_id: None,
            },
            result: ElementResult::Placed {
                order_ids: vec![id.into()],
            },
        }
    }

    impl Rows {
        fn park(&mut self, cx: &mut Cx<'_, Context>, reference: &str) {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential(reference)),
                })
                .unwrap();
            cx.park_preflight_for(&preflight, None).unwrap();
            self.parked.push(preflight);
        }

        fn found(&mut self, cx: &mut Cx<'_, Context>) {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::WriteAfterReads,
                    read_credential: None,
                    write_credential: Some(credential("fixture")),
                })
                .unwrap();
            let (plan, slot, _, _) = cx
                .begin_serial_row_execution(
                    &preflight,
                    call(),
                    SerialPolicy {
                        credential: Some(credential("fixture")),
                        pacing: None,
                        refusals_complete: false,
                    },
                    "7",
                    1,
                )
                .unwrap();
            let mut next = 1;
            for follower in self.parked.clone() {
                let start = next + usize::from(self.gap);
                let ids = cx.io.core.next_id;
                let adopted = cx
                    .adopt_planned_observer(&plan, &follower, "follower", start..start + 1)
                    .map(drop);
                if adopted.is_err() {
                    assert_eq!(cx.io.core.next_id, ids, "a refusal spends no identity");
                }
                if adopted.is_ok() {
                    next = start + 1;
                }
                self.adopted.push(adopted);
            }
            let leg = cx.plan_leg_context(&plan, &slot).unwrap();
            cx.request_signed_managed(
                leg.clone(),
                None,
                Method::Post,
                "https://venue.test/write".into(),
                None,
                Box::new(Recipe),
                leg,
            )
            .unwrap();
            if self.late
                && let Some(follower) = self.parked.last()
            {
                self.refusals.push(
                    cx.adopt_planned_observer(&plan, follower, "late", next..next + 1)
                        .unwrap_err(),
                );
            }
            self.plan = Some(plan);
            self.slot = Some(slot);
        }
    }

    impl Venue for Rows {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, _: &mut Cx<'_, Context>, _: Command) {}
        fn on_venue_action(&mut self, cx: &mut Cx<'_, Context>, command: CommandId, _: VenueAction) {
            match command {
                FOUNDER => self.found(cx),
                FOREIGN => self.park(cx, "other"),
                _ => self.park(cx, "fixture"),
            }
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
            let plan = self.plan.clone().unwrap();
            let slot = self.slot.clone().unwrap();
            cx.validate_plan_delivery(&plan, &context).unwrap();
            let rows = (1..=self.adopted.iter().filter(|a| a.is_ok()).count() + 1)
                .map(|n| placed(&format!("native-{n}")))
                .collect::<Vec<_>>();
            if self.answer == Answer::OneRefused {
                let refusal = VenueError::new("Insufficient margin to place order.")
                    .kind(crate::ErrorKind::Rejected)
                    .outcome(crate::Outcome::Rejected);
                cx.http_write_result(
                    &context,
                    crate::request_context::WriteResult::Rejected(refusal.clone()),
                )
                .unwrap();
                cx.finish_plan_call(&plan, &slot, &context).unwrap();
                let mut rows = rows;
                rows[1].result = ElementResult::NotSent {
                    error: Some(Box::new(refusal.clone())),
                };
                let mut error = refusal.outcome(crate::Outcome::Unknown);
                error.elements = Some(Box::new(crate::write_plan::ElementOutcomes(rows)));
                cx.fail_plan(&plan, error).unwrap();
                return;
            }
            cx.http_write_result(
                &context,
                crate::request_context::WriteResult::Accepted {
                    order_ids: rows
                        .iter()
                        .map(|row| match &row.result {
                            ElementResult::Placed { order_ids } => order_ids[0].clone(),
                            _ => unreachable!(),
                        })
                        .collect(),
                },
            )
            .unwrap();
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            if self.plain_done {
                self.refusals
                    .push(cx.done_plan(&plan, "[]".into()).unwrap_err());
            }
            cx.done_row_plan(&plan, rows).unwrap();
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, _: Context, error: &str) {
            let plan = self.plan.clone().unwrap();
            cx.fail_plan(&plan, VenueError::new(error)).unwrap();
        }
        fn capture_http_reconciliation(
            &self,
            _: &Context,
        ) -> Result<crate::request_context::ReconciliationData, Error> {
            crate::request_context::ReconciliationData::new("row-execution".into())
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

    fn command(engine: &mut Engine<Rows>, command: CommandId) {
        engine.handle(
            0.,
            Event::Command(Command::Venue {
                command,
                action: VenueAction::FetchBalanceInfo,
            }),
        );
    }

    fn prepared(actions: &[Action]) -> Vec<Preparation> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    fn release(engine: &mut Engine<Rows>, token: Preparation) {
        engine.http_prepared(
            0.,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        let (attempt, work) = work(&actions);
        engine.handle(
            0.,
            Event::Signed {
                attempt,
                result: Ok(crate::signing::SignResult::new(
                    vec![b"value".to_vec()],
                    work.receipt,
                )),
            },
        );
        let permit = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("row execution binding");
        assert!(matches!(engine.claim_http(0., permit), Ok(Claim::Released(_))));
    }

    fn terminals(actions: &[Action], command: CommandId) -> Vec<Result<String, VenueError>> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
                    Some(Ok(result.clone()))
                }
                Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                    Some(Err(error.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Followers park first, the founder adopts them, and its one write is released.
    fn rig(venue: Rows, followers: &[CommandId]) -> (Engine<Rows>, Preparation) {
        let mut e = crate::test_support::seeded_engine(venue);
        e.start(0.);
        for &follower in followers {
            command(&mut e, follower);
        }
        command(&mut e, FOUNDER);
        let tokens = prepared(&e.drain_actions().collect::<Vec<_>>());
        assert_eq!(tokens.len(), 1, "adopted followers never prepare a write");
        release(&mut e, tokens[0]);
        (e, tokens[0])
    }

    fn respond(e: &mut Engine<Rows>, token: Preparation) -> Vec<Action> {
        e.handle(
            10.,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        e.drain_actions().collect()
    }

    #[test]
    fn each_adopted_command_receives_only_its_own_order_ids() {
        let (mut e, token) = rig(
            Rows {
                plain_done: true,
                ..Default::default()
            },
            &[FIRST, SECOND],
        );
        assert_eq!(e.venue.adopted, [Ok(()), Ok(())]);
        let actions = respond(&mut e, token);
        assert_eq!(e.venue.refusals, [Error::Invalid], "a row success names its rows");
        assert_eq!(terminals(&actions, FOUNDER), [Ok("[\"native-1\"]".into())]);
        assert_eq!(terminals(&actions, FIRST), [Ok("[\"native-2\"]".into())]);
        assert_eq!(terminals(&actions, SECOND), [Ok("[\"native-3\"]".into())]);
    }

    #[test]
    fn a_refused_row_fails_only_the_command_that_owns_it() {
        let (mut e, token) = rig(
            Rows {
                answer: Answer::OneRefused,
                ..Default::default()
            },
            &[FIRST, SECOND],
        );
        let actions = respond(&mut e, token);
        assert_eq!(terminals(&actions, FOUNDER), [Ok("[\"native-1\"]".into())]);
        assert_eq!(terminals(&actions, SECOND), [Ok("[\"native-3\"]".into())]);
        let first = terminals(&actions, FIRST);
        let error = first[0].as_ref().unwrap_err();
        assert_eq!(error.outcome, Some(crate::Outcome::Rejected));
        assert_eq!(error.elements.as_ref().map(|rows| rows.len()), Some(1));
    }

    #[test]
    fn a_lost_reply_is_unknown_for_every_adopted_command() {
        let (mut e, token) = rig(Rows::default(), &[FIRST, SECOND]);
        e.handle(
            10.,
            Event::HttpFailed {
                request: token.attempt().request,
                error: "network",
            },
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        for owner in [FOUNDER, FIRST, SECOND] {
            let terminal = terminals(&actions, owner);
            assert_eq!(terminal.len(), 1, "{owner}");
            assert_eq!(
                terminal[0].as_ref().unwrap_err().outcome,
                Some(crate::Outcome::Unknown),
                "{owner}"
            );
        }
        assert!(prepared(&actions).is_empty(), "never resent");
    }

    #[test]
    fn refused_adoptions_leave_the_follower_parked_and_the_write_unchanged() {
        // A different credential, and rows that do not follow the last range.
        let (e, _) = rig(Rows::default(), &[FOREIGN]);
        assert_eq!(e.venue.adopted, [Err(Error::Invalid)]);
        assert!(!e.io.core.request_contexts.is_observer(FOREIGN));
        let (e, _) = rig(
            Rows {
                gap: true,
                ..Default::default()
            },
            &[FIRST],
        );
        assert_eq!(e.venue.adopted, [Err(Error::Invalid)]);
        assert!(!e.io.core.request_contexts.is_observer(FIRST));
    }

    #[test]
    fn nothing_joins_once_the_write_is_issued() {
        let (e, _) = rig(
            Rows {
                late: true,
                ..Default::default()
            },
            &[FIRST, SECOND],
        );
        assert_eq!(e.venue.refusals, [Error::State]);
    }
}
