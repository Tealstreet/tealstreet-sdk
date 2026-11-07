use super::*;
use crate::request_context::{
    Credential, RetryPolicy,
    plan::{CallBound, MutationBarrierBound, RetryWindow},
};
use crate::{Command, CredentialSlot, Response, SocketId, Venue};

#[derive(Default)]
struct Consumer(bool);
impl Venue for Consumer {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let credential = Credential {
            slot: CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        };
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential.clone()),
            })
            .unwrap();
        let call = || CallBound {
            timeout_ms: 100,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        };
        let pair = [call(), call()];
        let steps = [StepBound {
            wait_ms: 0,
            calls: &pair,
        }];
        let policy = PlanPolicy {
            independent_calls: false,
            operation: Operation::Mutation,
            credential: Some(credential),
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let before = (
            cx.io.core.next_id,
            cx.io.core.actions.len(),
            cx.io.core.request_contexts.plan_context_storage(),
        );
        for physical in [false, true] {
            let result = cx.admit_plan_owned(
                &preflight,
                AdmissionDeclaration {
                    steps: &steps,
                    policy: policy.clone(),
                    terminals: &[],
                    barriers: &[MutationBarrierBound { step: 0 }],
                },
                physical,
            );
            assert!(matches!(result, Err(Error::Invalid)));
            assert_eq!(
                (
                    cx.io.core.next_id,
                    cx.io.core.actions.len(),
                    cx.io.core.request_contexts.plan_context_storage()
                ),
                before
            );
        }
        let plan = cx.admit_plan(&preflight, &steps, policy).unwrap();
        assert_eq!(cx.plan_slots(&plan).unwrap().len(), 2);
        cx.fail_plan(&plan, VenueError::new("ordinary Plan remains usable"))
            .unwrap();
        self.0 = true;
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        panic!("no I/O");
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("no I/O");
    }
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
#[test]
fn disabled_barrier_refusal_keeps_ids_credits_and_preflight_for_ordinary_plan() {
    let mut engine = crate::test_support::seeded_engine(Consumer::default());
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
    assert!(engine.venue().0);
    let actions: Vec<_> = engine.drain_actions().collect();
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(crate::Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(crate::Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
}
