//! Declared mutation terminal controls; original RED retained in #1894 history.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::{Intent, WriteFact};
use safe_cex_engine::request_context::{
    Context, Credential, Error, HttpOwnerRetirement, Operation, Plan, PlanPolicy, PreflightPolicy,
    PreflightPurpose, ReconciliationData, RetryPolicy,
    plan::{CallBound, MutationTerminalBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::signing::{
    KeyRef, Operation as SignOperation, SignError, SignFuture, SignReceipt, SignRequest,
    SignResult, Signer,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    Notice, Response, SignStep, SocketId, TimerId, Venue,
};
use std::sync::Arc;

// Local presentation fixture only: this does not prove remote family signing.
struct Present;
impl Signer for Present {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("immediate fixture")
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Ok(SignResult::new(vec![b"value".to_vec()], work.receipt)))
    }
}
struct Recipe;
impl SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        3_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Ok(SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 3_000.),
            },
            context: Default::default(),
            operations: vec![SignOperation::Present {
                key: KeyRef("fixture".into()),
            }],
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.signer_headers.push((
            "x-credential".into(),
            String::from_utf8(result.values[0].clone()).map_err(|_| SignError::InvalidResult)?,
        ));
        Ok(())
    }
}
fn credential() -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: KeyRef("fixture".into()),
    }
}
#[derive(Default)]
struct NativeAmend {
    omit: bool,
    undeclared: bool,
    uncertain_prefix: bool,
    check_refusals: bool,
    check_declarations: bool,
    foreign: Option<Plan>,
    old_context: Option<Context>,
    plan: Option<Plan>,
    slot: Option<Slot>,
    replies: usize,
    retired: usize,
    original_deadline: f64,
    completion: Option<Result<(), Error>>,
}
impl NativeAmend {
    fn issue(&mut self, cx: &mut Cx<'_, Context>) {
        let plan = self.plan.as_ref().unwrap();
        let slot = cx.plan_slots(plan).unwrap().remove(0);
        let placing = if self.uncertain_prefix {
            self.replies == 0 || self.replies >= 3
        } else {
            self.replies != 0
        };
        let context = if placing {
            cx.plan_leg_context_with_intent(plan, &slot, Intent::Place)
                .unwrap()
        } else {
            cx.plan_leg_context(plan, &slot).unwrap()
        };
        if self.replies == 0 {
            assert_eq!(
                cx.done_mutation_plan_after_data(plan, &slot, &context, "[]".into()),
                Err(Error::Revoked)
            );
            self.old_context = Some(context.clone());
        }
        cx.request_signed_managed(
            context.clone(),
            Method::Post,
            format!(
                "https://venue.test/{}",
                if self.replies == 0 {
                    "cancel"
                } else {
                    "replace"
                }
            ),
            None,
            Box::new(Recipe),
            context,
        )
        .unwrap();
        self.slot = Some(slot);
    }
}
impl Venue for NativeAmend {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        assert!(matches!(command, Command::AmendOrder { .. }));
        if let Some(previous) = self.plan.take() {
            self.foreign = Some(previous);
            self.check_refusals = true;
        }
        self.replies = 0;
        self.completion = None;
        self.slot = None;
        self.old_context = None;
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential()),
            })
            .unwrap();
        let calls = [CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        }];
        let steps = (0..if self.uncertain_prefix { 4 } else { 2 })
            .map(|_| StepBound {
                wait_ms: 0,
                calls: &calls,
            })
            .collect::<Vec<_>>();
        let policy = PlanPolicy {
            operation: Operation::Mutation,
            credential: Some(credential()),
            pacing: None,
            retry: RetryPolicy::Never,
        };
        if self.check_declarations {
            assert!(matches!(
                cx.admit_plan_with_terminals(
                    &preflight,
                    &steps,
                    policy.clone(),
                    &[MutationTerminalBound { after_step: 0 }; 257]
                ),
                Err(Error::Capacity)
            ));
            for terminals in [
                vec![MutationTerminalBound { after_step: 2 }],
                vec![
                    MutationTerminalBound { after_step: 0 },
                    MutationTerminalBound { after_step: 0 },
                ],
            ] {
                assert!(matches!(
                    cx.admit_plan_with_terminals(&preflight, &steps, policy.clone(), &terminals),
                    Err(Error::Invalid)
                ));
            }
            let parallel = [
                CallBound {
                    timeout_ms: 20_000,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                    after_refusal: &[],
                },
                CallBound {
                    timeout_ms: 20_000,
                    attempts: 1,
                    retry_window: RetryWindow::Shared,
                    after_refusal: &[],
                },
            ];
            assert!(matches!(
                cx.admit_plan_with_terminals(
                    &preflight,
                    &[StepBound {
                        wait_ms: 0,
                        calls: &parallel
                    }],
                    policy,
                    &[MutationTerminalBound { after_step: 0 }]
                ),
                Err(Error::Invalid)
            ));
        }
        let terminals = [MutationTerminalBound {
            after_step: if self.uncertain_prefix { 2 } else { 0 },
        }];
        let plan = cx
            .admit_plan_with_terminals(
                &preflight,
                &steps,
                PlanPolicy {
                    operation: Operation::Mutation,
                    credential: Some(credential()),
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
                if self.omit && !self.undeclared {
                    &terminals
                } else {
                    &[]
                },
            )
            .unwrap();
        self.original_deadline = plan.deadline();
        self.plan = Some(plan);
        self.issue(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!(response.status, 200);
        let plan = self.plan.as_ref().unwrap().clone();
        let slot = self.slot.as_ref().unwrap().clone();
        assert_eq!(plan.deadline(), self.original_deadline);
        cx.validate_plan_delivery(&plan, &context).unwrap();
        if self.uncertain_prefix && self.replies < 2 {
            if self.replies == 0 {
                assert_eq!(response.body, "prefix-native-id");
                cx.http_write_fact(
                    &context,
                    WriteFact::Accepted {
                        order_ids: vec![response.body.into()],
                    },
                    None,
                )
                .unwrap();
                cx.validate_http_write_effects(&context).unwrap();
            } else {
                use safe_cex_engine::classify::{CodeTable, Key, Native, NativeCode, Verdict};
                const TABLE: CodeTable = CodeTable {
                    rows: &[],
                    success: &[Key::Int(0)],
                    unlisted: Verdict::Unknown(None),
                    status: &[],
                };
                assert_eq!(response.body, "unlisted native failure");
                let native = Native {
                    code: NativeCode::Int(987654),
                    message: response.body,
                };
                let fact = cx
                    .classify_write(&context, &TABLE, Some(&native), response.body)
                    .unwrap();
                cx.http_write_fact(
                    &context,
                    WriteFact::Failed {
                        fact,
                        known_effects: false,
                    },
                    None,
                )
                .unwrap();
            }
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            self.replies += 1;
            self.issue(cx);
            return;
        }
        let cancel = self.replies == 0 || (self.uncertain_prefix && self.replies == 2);
        assert_eq!(
            response.body,
            if cancel {
                "cancelled"
            } else {
                "replacement-native-id"
            }
        );
        if self.check_refusals && self.replies == 0 {
            assert_eq!(
                cx.done_mutation_plan_after_data(&plan, &slot, &context, "[]".into()),
                Err(Error::State),
                "missing native journal fact cannot retire the call"
            );
            let foreign = self.foreign.as_ref().unwrap();
            assert_eq!(
                cx.done_mutation_plan_after_data(foreign, &slot, &context, "[]".into()),
                Err(Error::Revoked)
            );
            let now = cx.now;
            for (invalid_now, expected) in [
                (f64::NAN, Error::Invalid),
                (context.deadline(), Error::TimedOut),
                (plan.deadline(), Error::TimedOut),
            ] {
                cx.now = invalid_now;
                assert_eq!(
                    cx.done_mutation_plan_after_data(&plan, &slot, &context, "[]".into()),
                    Err(expected)
                );
            }
            cx.now = now;
            cx.validate_plan_delivery(&plan, &context).unwrap();
        }
        if self.replies != 0 {
            assert_eq!(
                cx.done_mutation_plan_after_data(
                    &plan,
                    &slot,
                    self.old_context.as_ref().unwrap(),
                    "[]".into()
                ),
                Err(Error::Revoked)
            );
            cx.validate_plan_delivery(&plan, &context).unwrap();
        }
        cx.http_write_fact(
            &context,
            WriteFact::Accepted {
                order_ids: if cancel {
                    vec![]
                } else {
                    vec![response.body.into()]
                },
            },
            None,
        )
        .unwrap();
        cx.validate_http_write_effects(&context).unwrap();
        self.replies += 1;
        if self.omit {
            // Keep the parent's structural prohibition after the new API lands.
            assert_eq!(cx.done_plan(&plan, "[]".into()), Err(Error::State));
            self.completion =
                Some(cx.done_mutation_plan_after_data(&plan, &slot, &context, "[]".into()));
            if self.undeclared {
                assert_eq!(self.completion, Some(Err(Error::Invalid)));
                cx.validate_plan_delivery(&plan, &context).unwrap();
                cx.fail_plan(
                    &plan,
                    safe_cex_engine::VenueError::new("undeclared native terminal")
                        .kind(safe_cex_engine::ErrorKind::Internal),
                )
                .unwrap();
            } else {
                assert_eq!(
                    cx.done_mutation_plan_after_data(&plan, &slot, &context, "[]".into()),
                    Err(Error::Revoked)
                );
            }
        } else {
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            if self.replies == 2 {
                self.completion = Some(cx.done_plan(&plan, "[\"replacement-native-id\"]".into()));
            } else {
                self.issue(cx);
            }
        }
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("conditional-native".into())
    }
    fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) {
        self.retired += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("unexpected fixture failure: {error}");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn start(omit: bool) -> Engine<NativeAmend> {
    start_with(NativeAmend {
        omit,
        ..Default::default()
    })
}
fn start_with(venue: NativeAmend) -> Engine<NativeAmend> {
    let mut e = support::seeded_engine(venue);
    e.set_signer(Some(Arc::new(Present))).unwrap();
    e.start(0.);
    e.handle(
        1.,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "native".into(),
            price: Some(1.),
            amount: None,
        }),
    );
    e
}
fn request(e: &mut Engine<NativeAmend>, now: f64) -> HttpRequest {
    let mut requests = prepared_actions(e, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        });
    let request = requests
        .next()
        .expect("managed request released through final claim");
    assert!(requests.next().is_none());
    request
}
fn reply(e: &mut Engine<NativeAmend>, request: &HttpRequest, at: f64, body: &str) {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn done(actions: &[Action], result: &str) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(action,
        Action::Notice(Notice::CommandDone { command: 7, result: got }) if got == result)
        })
        .count()
}
fn total_terminals(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                Action::Notice(
                    Notice::CommandDone { command: 7, .. }
                        | Notice::CommandFailed { command: 7, .. }
                )
            )
        })
        .count()
}
#[test]
fn original_red_cancel_success_can_retire_unissued_replacement_once() {
    let mut e = start(true);
    let cancel = request(&mut e, 1.);
    assert!(cancel.url.ends_with("/cancel"));
    reply(&mut e, &cancel, 2., "cancelled");
    let actions: Vec<_> = prepared_actions(&mut e, 2.);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "no replacement when native remainder is zero"
    );
    assert_eq!(
        done(&actions, "[]"),
        1,
        "declared native conditional terminal must complete once"
    );
    assert_eq!(total_terminals(&actions), 1);
    assert_eq!(e.venue().completion, Some(Ok(())));
    assert_eq!(e.venue().retired, 1);
    reply(&mut e, &cancel, 3., "cancelled");
    let late = prepared_actions(&mut e, 3.);
    assert_eq!(total_terminals(&late), 0);
    assert!(!late.iter().any(|action| matches!(action, Action::Http(_))));
    assert_eq!(e.venue().replies, 1);
}
#[test]
fn ordinary_full_amend_preserves_native_identity_and_root() {
    let mut e = start(false);
    let cancel = request(&mut e, 1.);
    reply(&mut e, &cancel, 2., "cancelled");
    let replacement = request(&mut e, 2.);
    assert!(replacement.url.ends_with("/replace"));
    reply(&mut e, &replacement, 3., "replacement-native-id");
    let completed = prepared_actions(&mut e, 3.);
    assert_eq!(done(&completed, "[\"replacement-native-id\"]"), 1);
    assert_eq!(total_terminals(&completed), 1);
    assert_eq!(e.venue().completion, Some(Ok(())));
    assert_eq!(e.venue().retired, 1);
}

#[test]
fn refused_conditional_checks_leave_exact_data_available_for_success() {
    let foreign = start(false).venue().plan.clone().unwrap();
    let mut e = start_with(NativeAmend {
        omit: true,
        check_refusals: true,
        foreign: Some(foreign),
        ..Default::default()
    });
    let cancel = request(&mut e, 1.);
    reply(&mut e, &cancel, 2., "cancelled");
    let completed = prepared_actions(&mut e, 2.);
    assert_eq!(done(&completed, "[]"), 1);
    assert_eq!(total_terminals(&completed), 1);
    assert_eq!(e.venue().retired, 1);
}

#[test]
fn undeclared_terminal_refuses_without_consuming_current_delivery() {
    let mut e = start_with(NativeAmend {
        omit: true,
        undeclared: true,
        ..Default::default()
    });
    let cancel = request(&mut e, 1.);
    reply(&mut e, &cancel, 2., "cancelled");
    let completed = prepared_actions(&mut e, 2.);
    assert_eq!(done(&completed, "[]"), 0);
    assert_eq!(total_terminals(&completed), 1);
    assert_eq!(e.venue().retired, 1);
}

#[test]
fn refused_terminal_declarations_do_not_consume_admission_ids() {
    let ordinary = request(&mut start(false), 1.);
    let mut e = start_with(NativeAmend {
        check_declarations: true,
        ..Default::default()
    });
    let after_refusals = request(&mut e, 1.);
    assert_eq!(after_refusals.id, ordinary.id);
    assert_eq!(after_refusals.url, ordinary.url);
    assert_eq!(e.venue().original_deadline, 40_001.);
}

#[test]
fn valid_terminal_projects_prior_unknown_and_preserves_accepted_prefix_ids() {
    let mut e = start_with(NativeAmend {
        omit: true,
        uncertain_prefix: true,
        ..Default::default()
    });
    let prefix = request(&mut e, 1.);
    reply(&mut e, &prefix, 2., "prefix-native-id");
    let uncertain = request(&mut e, 2.);
    reply(&mut e, &uncertain, 3., "unlisted native failure");
    let cancel = request(&mut e, 3.);
    reply(&mut e, &cancel, 4., "cancelled");
    let completed = prepared_actions(&mut e, 4.);
    assert!(
        !completed
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(done(&completed, "[]"), 0);
    assert_eq!(total_terminals(&completed), 1);
    let error = completed
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), Some("987654"));
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["prefix-native-id"]
    );
    assert_eq!(
        e.venue().completion,
        Some(Ok(())),
        "terminal transaction commits its failure projection"
    );
    assert_eq!(e.venue().retired, 1);
}

#[derive(Default)]
struct ReadProbe {
    plan: Option<Plan>,
    slot: Option<Slot>,
    refused: bool,
}
impl Venue for ReadProbe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        let calls = [CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        }];
        let steps = [
            StepBound {
                wait_ms: 0,
                calls: &calls,
            },
            StepBound {
                wait_ms: 0,
                calls: &calls,
            },
        ];
        let policy = PlanPolicy {
            operation: Operation::Read,
            credential: None,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        assert!(matches!(
            cx.admit_plan_with_terminals(
                &preflight,
                &steps,
                policy.clone(),
                &[MutationTerminalBound { after_step: 0 }]
            ),
            Err(Error::Invalid)
        ));
        let plan = cx.admit_plan(&preflight, &steps, policy).unwrap();
        let slot = cx.plan_slots(&plan).unwrap().remove(0);
        let context = cx.plan_leg_context(&plan, &slot).unwrap();
        cx.request_managed(
            context.clone(),
            Method::Get,
            "https://venue.test/read".into(),
            None,
            context,
        )
        .unwrap();
        self.plan = Some(plan);
        self.slot = Some(slot);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        assert_eq!(response.body, "validated-read");
        let plan = self.plan.as_ref().unwrap();
        let slot = self.slot.as_ref().unwrap();
        cx.validate_plan_delivery(plan, &context).unwrap();
        assert_eq!(
            cx.done_mutation_plan_after_data(plan, slot, &context, "read-result".into()),
            Err(Error::Invalid)
        );
        self.refused = true;
        cx.validate_plan_delivery(plan, &context).unwrap();
        cx.done_read_plan_after_data(plan, slot, &context, "read-result".into())
            .unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("{error}")
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn read_plan_cannot_declare_or_consume_a_mutation_terminal() {
    let mut e = support::seeded_engine(ReadProbe::default());
    e.start(0.);
    e.handle(
        1.,
        Event::Command(Command::FetchLeverage {
            command: 7,
            symbol: "native".into(),
        }),
    );
    let request = prepared_actions(&mut e, 1.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    e.handle(
        2.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "validated-read",
            },
        },
    );
    let completed = prepared_actions(&mut e, 2.);
    assert!(e.venue().refused);
    assert_eq!(done(&completed, "read-result"), 1);
    assert_eq!(total_terminals(&completed), 1);
}

#[derive(Default)]
struct OrdinalProbe {
    plan: Option<Plan>,
    slots: std::collections::HashMap<u64, Slot>,
    requests: usize,
    retired: usize,
}
impl OrdinalProbe {
    fn issue(&mut self, cx: &mut Cx<'_, Context>) {
        let plan = self.plan.as_ref().unwrap();
        for slot in cx.plan_slots(plan).unwrap() {
            let context = cx.plan_leg_context(plan, &slot).unwrap();
            self.slots.insert(context.leg(), slot);
            cx.request_signed_managed(
                context.clone(),
                Method::Post,
                format!("https://venue.test/call-{}", self.requests),
                None,
                Box::new(Recipe),
                context,
            )
            .unwrap();
            self.requests += 1;
        }
    }
}
impl Venue for OrdinalProbe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential()),
            })
            .unwrap();
        let ordinary = [CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        }];
        let fallback = [StepBound {
            wait_ms: 0,
            calls: &ordinary,
        }];
        let parallel = [
            CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &fallback,
            },
            CallBound {
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            },
        ];
        let steps = [
            StepBound {
                wait_ms: 0,
                calls: &parallel,
            },
            StepBound {
                wait_ms: 0,
                calls: &ordinary,
            },
            StepBound {
                wait_ms: 0,
                calls: &ordinary,
            },
        ];
        self.plan = Some(
            cx.admit_plan_with_terminals(
                &preflight,
                &steps,
                PlanPolicy {
                    operation: Operation::Mutation,
                    credential: Some(credential()),
                    pacing: None,
                    retry: RetryPolicy::DefinitiveRefusal,
                },
                &[MutationTerminalBound { after_step: 1 }],
            )
            .unwrap(),
        );
        self.issue(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        let plan = self.plan.as_ref().unwrap().clone();
        let slot = self.slots[&context.leg()].clone();
        cx.validate_plan_delivery(&plan, &context).unwrap();
        if response.body == "refused" {
            use safe_cex_engine::classify::{CodeTable, Key, Native, NativeCode, Row, Verdict};
            const TABLE: CodeTable = CodeTable {
                rows: &[Row {
                    key: Key::Int(77),
                    verdict: Verdict::Refused(safe_cex_engine::ErrorKind::InvalidInput),
                    only: None,
                }],
                success: &[Key::Int(0)],
                unlisted: Verdict::Unknown(None),
                status: &[],
            };
            let native = Native {
                code: NativeCode::Int(77),
                message: "native refusal",
            };
            cx.http_write_failure(&context, &TABLE, Some(&native), "native refusal", None)
                .unwrap();
            cx.replace_plan_call(&plan, &slot, &context).unwrap();
            self.issue(cx);
            return;
        }
        cx.http_write_fact(&context, WriteFact::Accepted { order_ids: vec![] }, None)
            .unwrap();
        if response.body == "anchor" {
            let wrong_slot = self
                .slots
                .iter()
                .find(|(leg, _)| **leg != context.leg())
                .unwrap()
                .1;
            assert_eq!(
                cx.done_mutation_plan_after_data(&plan, wrong_slot, &context, "[]".into()),
                Err(Error::Invalid)
            );
            cx.validate_plan_delivery(&plan, &context).unwrap();
            assert_eq!(cx.done_plan(&plan, "[]".into()), Err(Error::State));
            cx.done_mutation_plan_after_data(&plan, &slot, &context, "[]".into())
                .unwrap();
        } else {
            assert!(matches!(response.body, "prefix" | "fallback"));
            // A successful fallback is still not a declared root terminal.
            assert_eq!(
                cx.done_mutation_plan_after_data(&plan, &slot, &context, "[]".into()),
                Err(Error::Invalid)
            );
            cx.validate_plan_delivery(&plan, &context).unwrap();
            cx.finish_plan_call(&plan, &slot, &context).unwrap();
            self.issue(cx);
        }
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("ordinal-native".into())
    }
    fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) {
        self.retired += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("{error}")
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn ordinal_requests(e: &mut Engine<OrdinalProbe>, now: f64) -> Vec<HttpRequest> {
    prepared_actions(e, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
fn ordinal_reply(e: &mut Engine<OrdinalProbe>, request: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn root_anchor_after_parallel_prefix(branch: bool) {
    let mut e = support::seeded_engine(OrdinalProbe::default());
    e.set_signer(Some(Arc::new(Present))).unwrap();
    e.start(0.);
    e.handle(
        1.,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["native".into()],
        }),
    );
    let root = e.venue().plan.as_ref().unwrap().deadline();
    let prefix = ordinal_requests(&mut e, 1.);
    assert_eq!(prefix.len(), 2);
    ordinal_reply(
        &mut e,
        &prefix[0],
        2.,
        if branch { "refused" } else { "prefix" },
    );
    let fallback = ordinal_requests(&mut e, 2.);
    assert_eq!(fallback.len(), usize::from(branch));
    ordinal_reply(&mut e, &prefix[1], 3., "prefix");
    let anchor = if branch {
        assert!(
            ordinal_requests(&mut e, 3.).is_empty(),
            "live branch blocks later root step"
        );
        ordinal_reply(&mut e, &fallback[0], 4., "fallback");
        ordinal_requests(&mut e, 4.)
    } else {
        ordinal_requests(&mut e, 3.)
    };
    assert_eq!(anchor.len(), 1);
    assert_eq!(e.venue().plan.as_ref().unwrap().deadline(), root);
    ordinal_reply(&mut e, &anchor[0], 5., "anchor");
    let completed = prepared_actions(&mut e, 5.);
    assert_eq!(done(&completed, "[]"), 1);
    assert_eq!(total_terminals(&completed), 1);
    assert!(
        !completed
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(e.venue().retired, 1);
}
#[test]
fn later_root_terminal_maps_after_parallel_prefix_and_unselected_fallback() {
    root_anchor_after_parallel_prefix(false);
}
#[test]
fn selected_fallback_is_not_a_terminal_and_does_not_shift_later_root_anchor() {
    root_anchor_after_parallel_prefix(true);
}

#[test]
fn old_terminal_plan_cannot_complete_a_reused_command_generation() {
    let mut e = start(true);
    let old = request(&mut e, 1.);
    let previous = e.venue().plan.as_ref().unwrap().owner();
    reply(&mut e, &old, 2., "cancelled");
    assert_eq!(total_terminals(&prepared_actions(&mut e, 2.)), 1);
    e.handle(
        4.,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "native".into(),
            price: Some(1.),
            amount: None,
        }),
    );
    assert_ne!(e.venue().plan.as_ref().unwrap().owner(), previous);
    let current = request(&mut e, 4.);
    reply(&mut e, &current, 5., "cancelled");
    let completed = prepared_actions(&mut e, 5.);
    assert_eq!(done(&completed, "[]"), 1);
    assert_eq!(total_terminals(&completed), 1);
    assert_eq!(e.venue().retired, 2);
    reply(&mut e, &old, 6., "cancelled");
    let late = prepared_actions(&mut e, 6.);
    assert_eq!(total_terminals(&late), 0);
    assert!(!late.iter().any(|action| matches!(action, Action::Http(_))));
}
