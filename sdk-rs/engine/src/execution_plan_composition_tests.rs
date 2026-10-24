use super::*;
use crate::engine::{ExecutionHandle, Window};
use crate::request_context::{
    Plan, PlanPolicy, PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallBound, CallTerms, RetryWindow, StepBound},
};
use crate::{Action, Command, Engine, Event, Notice, Response, SocketId, TimerId, Venue};

fn read_policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn preflight_policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::ReadOnly,
        read_credential: None,
        write_credential: None,
    }
}
fn plan(cx: &mut Cx<'_, Context>, wait_ms: u32) -> Plan {
    let owner = cx.begin_preflight(preflight_policy()).unwrap();
    let calls = [CallBound {
        timeout_ms: 200,
        attempts: 1,
        retry_window: RetryWindow::Shared,
        after_refusal: &[],
    }];
    cx.admit_plan(
        &owner,
        &[StepBound {
            wait_ms,
            calls: &calls,
        }],
        PlanPolicy {
            operation: Operation::Read,
            credential: None,
            pacing: None,
            retry: RetryPolicy::Never,
        },
    )
    .unwrap()
}
#[derive(Default)]
struct Probe {
    execution: Option<ExecutionHandle>,
    observer: Option<Context>,
    plan: Option<Plan>,
    background: Option<crate::RequestId>,
    short: bool,
    inspect_noncontext: bool,
    denials: usize,
    ready: usize,
    preserved_delivery: bool,
}
impl Probe {
    fn refuse_group_membership(&mut self, cx: &mut Cx<'_, Context>, leg: &Context) {
        let roots = cx.io.request_contexts.executions.roots.clone();
        let observers = cx.io.request_contexts.executions.observers.clone();
        let actions = cx.io.actions.len();
        assert_eq!(
            cx.begin_execution(leg, read_policy(200), Window::UntilTerminal, "same"),
            Err(Error::Revoked)
        );
        assert_eq!(
            cx.join_execution(self.execution.as_ref().unwrap(), leg, "same"),
            Err(Error::Revoked)
        );
        assert_eq!(cx.io.request_contexts.executions.roots, roots);
        assert_eq!(cx.io.request_contexts.executions.observers, observers);
        assert_eq!(cx.io.actions.len(), actions);
        self.denials += 2;
    }
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let context = cx
            .http_context(Scope::Background, read_policy(1000))
            .unwrap();
        self.background = Some(
            cx.request_managed(
                context.clone(),
                None,
                crate::Method::Get,
                "https://venue.test/background".into(),
                None,
                context,
            )
            .unwrap(),
        );
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        match command.completion_id().unwrap() {
            1 => {
                let root = cx
                    .admit_http_command(read_policy(if self.short { 50 } else { 1000 }))
                    .unwrap();
                self.execution = Some(
                    cx.begin_execution(&root, read_policy(1000), Window::UntilTerminal, "same")
                        .unwrap(),
                );
                self.observer = Some(root);
            }
            7 if self.inspect_noncontext => {
                let owner = cx.begin_preflight(preflight_policy()).unwrap();
                let leg = cx
                    .preflight_read_context(
                        &owner,
                        read_policy(200),
                        CallTerms {
                            wait_ms: 0,
                            timeout_ms: 200,
                            attempts: 1,
                            retry_window: RetryWindow::Shared,
                        },
                    )
                    .unwrap();
                self.refuse_group_membership(cx, &leg);
                cx.fail_preflight(&owner, "control complete").unwrap();
            }
            99 if self.inspect_noncontext => {
                let plan = plan(cx, 0);
                let slot = cx.plan_slots(&plan).unwrap().remove(0);
                let leg = cx.plan_leg_context(&plan, &slot).unwrap();
                self.refuse_group_membership(cx, &leg);
                cx.fail_plan(&plan, "control complete").unwrap();
            }
            99 => {
                self.plan = Some(plan(cx, 100));
            }
            _ => unreachable!(),
        }
    }
    fn on_http_plan_ready(&mut self, _: &mut Cx<'_, Context>, _: &Plan) {
        self.ready += 1;
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
        let marker = cx.io.http_delivery.clone().unwrap();
        assert_eq!(marker.context, context);
        let depth = cx.io.request_contexts.delivery_depth;
        assert_eq!(depth, 1);
        // Neither successful group DATA nor group abort can borrow this
        // unrelated delivery. An observer's explicit detach can still seal
        // that observer, without erasing the marker held by the callback.
        assert_eq!(
            cx.complete_execution(self.execution.as_ref().unwrap(), |_| Ok("borrowed".into())),
            Err(Error::Revoked)
        );
        assert_eq!(
            cx.abort_execution(self.execution.as_ref().unwrap(), "borrowed"),
            Err(Error::Revoked)
        );
        cx.detach_execution_observer(self.observer.as_ref().unwrap(), "observer detached")
            .unwrap();
        assert_eq!(cx.io.http_delivery, Some(marker));
        assert_eq!(cx.io.request_contexts.delivery_depth, depth);
        self.preserved_delivery = true;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {
        panic!("owned timer escaped");
    }
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
fn engine(probe: Probe) -> Engine<Probe> {
    let mut e = crate::test_support::seeded_engine(probe);
    e.start(0.);
    e.handle(0., command(1));
    e
}
#[test]
fn execution_observer_seals_preserve_an_unrelated_real_delivery_marker() {
    let mut e = engine(Probe::default());
    e.handle(0., command(99));
    let actions = crate::test_support::prepared_actions(&mut e, 1.);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(request) if Some(request.id) == e.venue.background))
    );
    e.handle(
        2.,
        Event::HttpResponse {
            request: e.venue.background.unwrap(),
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert!(e.venue.preserved_delivery);
    assert!(e.io.http_delivery.is_none());
    assert_eq!(e.io.request_contexts.delivery_depth, 0);
    assert!(!e.io.request_contexts.is_observer(1));
    assert!(e.io.request_contexts.command_live(99, 2.));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, error }) if error.message == "observer detached")).count(), 1);
}
#[test]
fn execution_observer_expiry_runs_before_plan_timer_early_return() {
    let mut e = engine(Probe {
        short: true,
        ..Default::default()
    });
    e.handle(0., command(99));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms: 100. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(100., Event::TimerFired { timer });
    assert_eq!(e.venue.ready, 1);
    assert!(!e.io.request_contexts.is_observer(1));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, error }) if error.kind == Some(crate::ErrorKind::Timeout))).count(), 1);
    e.handle(101., Event::TimerFired { timer });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
}
#[test]
fn preflight_and_plan_legs_cannot_replace_context_observer_roots() {
    let mut e = engine(Probe {
        inspect_noncontext: true,
        ..Default::default()
    });
    e.drain_actions().for_each(drop);
    e.handle(1., command(7));
    e.drain_actions().for_each(drop);
    e.handle(2., command(99));
    e.drain_actions().for_each(drop);
    assert_eq!(e.venue.denials, 4);
    assert!(e.io.request_contexts.is_observer(1));
    assert_eq!(e.io.request_contexts.executions.observers.len(), 1);
}

#[test]
fn execution_member_cannot_seal_directly_while_an_unrelated_plan_is_retained() {
    let mut e = engine(Probe::default());
    e.handle(0., command(99));
    e.drain_actions().for_each(drop);
    let root = e.venue.observer.clone().unwrap();
    let handle = e.venue.execution.clone().unwrap();
    {
        let mut cx = Cx {
            now: 1.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let ids = cx.io.next_id;
        assert_eq!(cx.done_context(&root, "direct".into()), Err(Error::Revoked));
        assert_eq!(cx.fail_context(&root, "direct"), Err(Error::Revoked));
        cx.done(1, "numeric".into());
        cx.fail(1, "numeric");
        assert_eq!(cx.io.next_id, ids);
        assert!(cx.io.request_contexts.is_observer(1));
        assert!(cx.io.request_contexts.command_live(99, 1.));
    }
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: 1, .. } | Notice::CommandFailed { command: 1, .. }
        )
    )));
    Cx {
        now: 2.,
        store: &mut e.store,
        io: &mut e.io,
    }
    .abort_execution(&handle, "genuine")
    .unwrap();
    assert!(e.io.request_contexts.command_live(99, 2.));
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 1, error }) if error.message == "genuine"))
            .count(),
        1
    );
}
