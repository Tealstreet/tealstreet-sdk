use super::*;
use crate::engine::{ExecutionHandle, Window};
use crate::request_context::{
    Plan, PlanPolicy, PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallBound, RetryWindow, StepBound},
};
use crate::{Action, Command, Engine, Event, Response, SocketId, TimerId, Venue};

fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Default)]
struct CreditProbe {
    filler: usize,
    plan: Option<Plan>,
    refused: bool,
    admitted: bool,
}

impl Venue for CreditProbe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for _ in 0..self.filler {
            cx.http_context(Scope::Background, read_policy()).unwrap();
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        if command.completion_id() == Some(99) {
            let preflight = cx
                .begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::ReadOnly,
                    read_credential: None,
                    write_credential: None,
                })
                .unwrap();
            let calls = (0..250)
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
            self.plan = Some(
                cx.admit_plan(
                    &preflight,
                    &steps,
                    PlanPolicy {
                        operation: Operation::Read,
                        credential: None,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap(),
            );
            assert_eq!(cx.io.request_contexts.reserved_contexts, 250);
            return;
        }
        let root = cx.admit_http_command(read_policy()).unwrap();
        let contexts = cx.io.request_contexts.contexts.clone();
        let roots = cx.io.request_contexts.executions.roots.clone();
        let observers = cx.io.request_contexts.executions.observers.clone();
        let deadline = cx.io.request_contexts.next_execution_deadline();
        let actions = cx.io.actions.len();
        let mut refused_ids = cx.io.next_id;
        refused_ids.reserve(1).unwrap();
        let result = cx.begin_execution(&root, read_policy(), Window::UntilTerminal, "credit");
        if self.filler == 5 {
            assert_eq!(
                result,
                Err(Error::Capacity),
                "execution stole promised Plan credit"
            );
            assert_eq!(cx.io.request_contexts.contexts, contexts);
            assert_eq!(cx.io.request_contexts.executions.roots, roots);
            assert_eq!(cx.io.request_contexts.executions.observers, observers);
            assert_eq!(cx.io.request_contexts.next_execution_deadline(), deadline);
            assert_eq!(cx.io.actions.len(), actions);
            // Existing begin reserves one monotonic identity before registry refusal.
            assert_eq!(cx.io.next_id, refused_ids);
            assert_eq!(cx.validate_read_root_context(&root), Ok(()));
            self.refused = true;
            cx.fail_context(&root, VenueError::new("execution capacity"))
                .unwrap();
        } else {
            let execution: ExecutionHandle = result.unwrap();
            assert_eq!(cx.io.request_contexts.contexts.len(), contexts.len() + 1);
            assert_eq!(
                cx.io.request_contexts.executions.roots.len(),
                roots.len() + 1
            );
            assert!(cx.io.request_contexts.is_observer(7));
            assert_eq!(cx.io.request_contexts.contexts.len() + 250, CAPACITY);
            self.admitted = true;
            cx.abort_execution(&execution, VenueError::new("control complete"))
                .unwrap();
        }
        assert_eq!(cx.io.request_contexts.reserved_contexts, 250);
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn command(command: crate::CommandId) -> Event<'static> {
    Event::Command(Command::FetchCandles {
        command,
        symbol: "BTCUSD".into(),
        interval: crate::model::Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    })
}

fn run(filler: usize) -> Engine<CreditProbe> {
    let mut engine = crate::test_support::seeded_engine(CreditProbe {
        filler,
        ..Default::default()
    });
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(0., command(99));
    engine.drain_actions().for_each(drop);
    engine.handle(1., command(7));
    assert_eq!(engine.io.request_contexts.reserved_contexts, 250);
    assert!(!engine.drain_actions().any(|action| matches!(
        action,
        Action::ManagedHttp(_) | Action::Http(_) | Action::Sign { .. }
    )));
    engine
}

#[test]
fn execution_root_refuses_to_steal_reserved_plan_context_credit() {
    assert!(run(5).venue().refused);
}

#[test]
fn execution_root_uses_one_remaining_context_without_spending_plan_credit() {
    assert!(run(4).venue().admitted);
}
