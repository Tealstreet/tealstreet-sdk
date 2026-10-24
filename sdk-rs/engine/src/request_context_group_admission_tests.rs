use super::*;
use crate::request_context::{
    Credential, Pacing, RetryPolicy,
    plan::{CallBound, RetryWindow},
};
use crate::{Command, CredentialSlot, Response, SocketId, Venue};

struct Consumer<const NONCE: bool = false>(usize);
fn credential() -> Credential {
    Credential {
        slot: CredentialSlot::Primary,
        reference: crate::signing::KeyRef("fixture".into()),
    }
}
fn call(timeout_ms: u32) -> CallBound<'static> {
    CallBound {
        timeout_ms,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        after_refusal: &[],
    }
}
impl<const NONCE: bool> Venue for Consumer<NONCE> {
    fn http_release() -> crate::HttpRelease<Self> {
        if NONCE {
            crate::HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
        } else {
            crate::HttpRelease::Mutable(|_, _, _| {
                Ok(crate::request_context::Admission::Unscheduled)
            })
        }
    }
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn http_pacing(&self) -> &[Pacing] {
        &[Pacing {
            group: 1,
            minimum_gap_ms: 5.,
        }]
    }
    fn http_pacing_sets(&self) -> &[crate::request_context::PacingSet] {
        &[crate::request_context::PacingSet {
            group: 2,
            constraints: &[Pacing {
                group: 1,
                minimum_gap_ms: 0.,
            }],
        }]
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        if self.0 == 14 {
            custody(cx);
            return;
        }
        let read = self.0 == 5;
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: if read {
                    PreflightPurpose::ReadOnly
                } else {
                    PreflightPurpose::WriteAfterReads
                },
                read_credential: None,
                write_credential: (!read).then(credential),
            })
            .unwrap();
        let fallback_call = [call(11)];
        let fallback = [StepBound {
            wait_ms: 0,
            calls: &fallback_call,
        }];
        let prefix = [CallBound {
            after_refusal: &fallback,
            ..call(13)
        }];
        let mut pair = [call(31), call(37)];
        if self.0 == 3 {
            pair[0].attempts = 2;
        }
        if self.0 == 4 {
            pair[0].after_refusal = &fallback;
        }
        let steps = [
            StepBound {
                wait_ms: 0,
                calls: &prefix,
            },
            StepBound {
                wait_ms: 0,
                calls: &pair,
            },
        ];
        let policy = PlanPolicy {
            independent_calls: false,
            operation: if read {
                Operation::Read
            } else {
                Operation::Mutation
            },
            credential: (!read).then(credential),
            pacing: match self.0 {
                6 => Some(Pacing {
                    group: 1,
                    minimum_gap_ms: 1.,
                }),
                7 => Some(Pacing {
                    group: 1,
                    minimum_gap_ms: 0.,
                }),
                8 => Some(Pacing {
                    group: 99,
                    minimum_gap_ms: 0.,
                }),
                15 => Some(Pacing {
                    group: 2,
                    minimum_gap_ms: 0.,
                }),
                _ => None,
            },
            retry: RetryPolicy::DefinitiveRefusal,
        };
        let declarations = [MutationBarrierBound {
            step: match self.0 {
                0 => 99,
                2 => 0,
                _ => 1,
            },
        }; 2];
        let barriers = &declarations[..if self.0 == 1 { 2 } else { 1 }];
        let before = (
            cx.io.next_id,
            cx.io.actions.len(),
            cx.io.request_contexts.plan_context_storage(),
        );
        let result = match self.0 {
            9 => cx
                .admit_plan_owned(
                    &preflight,
                    AdmissionDeclaration {
                        steps: &steps,
                        policy,
                        terminals: &[],
                        barriers,
                    },
                    true,
                )
                .map(|(plan, _)| plan),
            11 | 17 => cx.admit_plan(&preflight, &steps, policy),
            12 => cx.admit_plan_with_terminals(&preflight, &steps, policy, &[]),
            13 => cx
                .begin_planned_execution(
                    &preflight,
                    &steps,
                    policy,
                    crate::engine::Window::UntilTerminal,
                    "group-default",
                )
                .map(|(plan, _, _)| plan),
            _ => cx.admit_plan_with_barriers(&preflight, &steps, policy, barriers),
        };
        if self.0 < 10 || self.0 == 15 || self.0 == 16 {
            assert_eq!(
                result.err(),
                Some(if self.0 == 1 {
                    Error::Duplicate
                } else {
                    Error::Invalid
                })
            );
            assert_eq!(
                before,
                (
                    cx.io.next_id,
                    cx.io.actions.len(),
                    cx.io.request_contexts.plan_context_storage()
                )
            );
            if self.0 == 16 {
                cx.fail_preflight(&preflight, Error::Invalid.venue_error())
                    .unwrap();
            } else {
                cx.fail_preflight(&preflight, "fixture complete").unwrap();
            }
        } else {
            let plan = result.unwrap();
            let body = cx.io.request_contexts.plan_body(&plan).unwrap();
            if self.0 == 10 {
                assert_eq!(body.barriers, vec![1..3]);
                assert_eq!(body.entries[1].terms.timeout_ms, 31);
                assert_eq!(body.entries[2].terms.timeout_ms, 37);
                assert_eq!(body.entries[3].terms.timeout_ms, 11);
            } else {
                assert!(body.barriers.is_empty());
            }
            cx.fail_plan(&plan, "fixture complete").unwrap();
        }
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn check(mode: usize) {
    check_registration::<false>(mode);
}
fn check_registration<const NONCE: bool>(mode: usize) {
    let mut engine = crate::test_support::seeded_engine(Consumer::<NONCE>(mode));
    engine.start(0.);
    engine.handle(
        1.,
        crate::Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    if mode == 16 {
        assert!(actions.iter().any(|action| matches!(action,
            Action::Notice(crate::Notice::CommandFailed { command: 7, error })
            if error.kind == Some(crate::ErrorKind::InvalidInput)
                && error.outcome == Some(crate::Outcome::NotSent))));
    }
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(
                    crate::Notice::CommandDone { command: 7, .. }
                        | crate::Notice::CommandFailed { command: 7, .. }
                )
            ))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
}
#[test]
fn invalid_barrier_geometry_and_read_policy_spend_no_resources() {
    for mode in 0..6 {
        check(mode);
    }
}
#[test]
fn nonzero_or_unknown_shared_pacing_spends_no_resources() {
    for mode in 6..9 {
        check(mode);
    }
}
#[test]
fn physical_execution_barrier_is_refused_before_resources() {
    check(9);
}
#[test]
fn root_membership_stays_exact_with_an_earlier_fallback() {
    check(10);
}
#[test]
fn existing_three_constructors_keep_empty_barriers() {
    for mode in 11..14 {
        check(mode);
    }
}

fn custody(cx: &mut Cx<'_, Context>) {
    let preflight = cx
        .begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: Some(credential()),
        })
        .unwrap();
    let calls = [call(31), call(37)];
    let steps = [StepBound {
        wait_ms: 0,
        calls: &calls,
    }];
    let plan = cx
        .admit_plan_with_barriers(
            &preflight,
            &steps,
            PlanPolicy {
                independent_calls: false,
                operation: Operation::Mutation,
                credential: Some(credential()),
                pacing: None,
                retry: RetryPolicy::Never,
            },
            &[MutationBarrierBound { step: 0 }],
        )
        .unwrap();
    let slots = cx.plan_slots(&plan).unwrap();
    let first = cx.plan_leg_context(&plan, &slots[0]).unwrap();
    let second = cx.plan_leg_context(&plan, &slots[1]).unwrap();
    let before = (
        cx.io.next_id,
        cx.io.actions.len(),
        cx.io.request_contexts.plan_context_storage(),
    );
    let first_key = cx
        .io
        .request_contexts
        .validate_mutation_group_submission(&first)
        .unwrap()
        .unwrap();
    assert_eq!(first_key.ordinal, 0);
    assert_eq!(first_key.count, 2);
    assert!(matches!(
        cx.io
            .request_contexts
            .validate_mutation_group_submission(&second),
        Err(Error::State)
    ));
    let mut foreign = first.clone();
    foreign.bind_issuer(&std::sync::Arc::new(()));
    assert!(matches!(
        cx.io.request_contexts.mutation_group_key(&foreign),
        Err(Error::Revoked)
    ));
    assert_eq!(
        before,
        (
            cx.io.next_id,
            cx.io.actions.len(),
            cx.io.request_contexts.plan_context_storage()
        )
    );
    let admission = cx.owned_http_admission(&first).unwrap().unwrap();
    cx.io.request_contexts.commit_owned_admission(&admission);
    let second_key = cx
        .io
        .request_contexts
        .validate_mutation_group_submission(&second)
        .unwrap()
        .unwrap();
    assert_eq!(second_key.ordinal, 1);
    assert!(first_key.same_group(second_key));
    cx.fail_plan(&plan, "fixture complete").unwrap();
    assert!(matches!(
        cx.io.request_contexts.mutation_group_key(&first),
        Err(Error::Revoked)
    ));
}

#[test]
fn group_custody_refuses_foreign_stale_and_out_of_order_without_effects() {
    check(14);
}

#[test]
fn expanded_nonzero_set_floor_is_refused_before_group_ids_or_storage() {
    check(15);
}

#[test]
fn nonce_unscheduled_barrier_refuses_before_ids_storage_or_credits() {
    check_registration::<true>(16);
}
#[test]
fn nonce_unscheduled_ordinary_plan_still_admits() {
    check_registration::<true>(17);
}
