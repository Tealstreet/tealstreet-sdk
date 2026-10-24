//! Every exact terminal retires its Store write fence; stale Plans never do.
#![cfg(all(feature = "http-plans", feature = "test-support"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Credential, Error, Operation, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallBound, RetryWindow, StepBound},
};
use safe_cex_engine::signing::KeyRef;
use safe_cex_engine::{Action, Command, Cx, Engine, Event, Notice, Response, SocketId, Venue};

const CALLS: [CallBound<'static>; 1] = [CallBound {
    after_refusal: &[],
    timeout_ms: 100,
    attempts: 1,
    retry_window: RetryWindow::Shared,
}];
const STEPS: [StepBound<'static>; 1] = [StepBound {
    wait_ms: 0,
    calls: &CALLS,
}];
#[derive(Default)]
struct Probe {
    old: Option<Plan>,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        let Command::SetLeverage {
            command, symbol, ..
        } = command
        else {
            panic!("fixture command")
        };
        if symbol == "stale" {
            assert_eq!(
                cx.fail_plan(self.old.as_ref().unwrap(), "stale replay"),
                Err(Error::Revoked)
            );
            return;
        }
        let credential = Credential {
            slot: safe_cex_engine::CredentialSlot::Primary,
            reference: KeyRef("fixture".into()),
        };
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential.clone()),
            })
            .unwrap();
        if symbol == "success" {
            cx.store
                .metadata_write_started(command, Some("BTC"))
                .unwrap();
            assert!(cx.store.metadata_write_pending("BTC"));
            cx.done_empty_plan(&preflight, "[]".into()).unwrap();
            return;
        }
        let plan = cx
            .admit_plan(
                &preflight,
                &STEPS,
                PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Mutation,
                    credential: Some(credential),
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.store
            .metadata_write_started(command, Some(&symbol))
            .unwrap();
        assert!(cx.store.metadata_write_pending(&symbol));
        if self.old.is_none() {
            self.old = Some(plan.clone());
        }
        if symbol == "fail" {
            cx.fail_plan(&plan, "controlled refusal").unwrap();
        }
    }
}
fn engine() -> Engine<Probe> {
    support::seeded_engine(Probe::default())
}
fn issue(e: &mut Engine<Probe>, now: f64, command: u32, symbol: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::SetLeverage {
            command,
            symbol: symbol.into(),
            leverage: 2.,
            side: None,
        }),
    );
    e.drain_actions().collect()
}
fn terminals(actions: &[Action], owner: u32) -> usize {
    actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. }) if *command == owner)).count()
}
#[test]
fn explicit_plan_failure_retires_the_store_write_fence() {
    let mut e = engine();
    let actions = issue(&mut e, 0., 7, "fail");
    assert_eq!(terminals(&actions, 7), 1);
    assert!(!e.store().metadata_write_pending("fail"));
}
#[test]
fn authenticated_local_success_retires_the_store_write_fence() {
    let mut e = engine();
    let actions = issue(&mut e, 0., 7, "success");
    assert_eq!(terminals(&actions, 7), 1);
    assert!(!e.store().metadata_write_pending("BTC"));
}
#[test]
fn expiry_clears_only_its_generation_and_reused_id_survives_stale_seals() {
    let mut e = engine();
    let first = issue(&mut e, 0., 7, "BTC");
    assert!(e.store().metadata_write_pending("BTC"));
    let timer = first
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms: 100. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(100., Event::TimerFired { timer });
    let ended: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&ended, 7), 1);
    assert!(!e.store().metadata_write_pending("BTC"));
    let second = issue(&mut e, 101., 7, "ETH");
    assert!(e.store().metadata_write_pending("ETH"));
    let current = second
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms: 201. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    assert_ne!(timer, current);
    assert_eq!(terminals(&issue(&mut e, 102., 8, "stale"), 7), 0);
    e.handle(103., Event::TimerFired { timer });
    let stale: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&stale, 7), 0);
    assert!(e.store().metadata_write_pending("ETH"));
    e.handle(201., Event::TimerFired { timer: current });
    let ended: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&ended, 7), 1);
    assert!(!e.store().metadata_write_pending("ETH"));
}
