//! #1919 composed with #1896: a ready founder's own physical Plan is its same
//! admission; another founder's physical Plan or generation never is.
use super::*;
use crate::engine::*;
use crate::request_context::{
    Context, Error, Operation, Plan, PlanPolicy, Preflight, PreflightPolicy, PreflightPurpose,
    RetryPolicy,
    plan::{CallBound, RetryWindow, StepBound},
};

const FOREIGN: CommandId = 10;
const READY: CommandId = 20;

#[derive(Default)]
struct Probe {
    /// A live physical Plan founded by another command, and its founder owner.
    foreign: Option<(Plan, Owner)>,
    capped: bool,
    cap_ids_unchanged: bool,
    results: Vec<(&'static str, Result<(), Error>)>,
}

fn preflight_policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::ReadOnly,
        read_credential: None,
        write_credential: None,
    }
}

fn plan_policy() -> PlanPolicy {
    PlanPolicy {
        independent_calls: false,
        operation: Operation::Read,
        credential: None,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

const CALLS: [CallBound<'static>; 1] = [CallBound {
    timeout_ms: 100,
    attempts: 1,
    retry_window: RetryWindow::Shared,
    after_refusal: &[],
}];

fn steps() -> [StepBound<'static>; 1] {
    [StepBound {
        wait_ms: 0,
        calls: &CALLS,
    }]
}

impl Probe {
    fn record(&mut self, label: &'static str, result: Result<(), Error>) {
        self.results.push((label, result));
    }
}

impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let id = command.completion_id().unwrap();
        let preflight = if self.capped {
            cx.begin_preflight_with_command_wait(preflight_policy())
                .unwrap()
        } else {
            cx.begin_preflight(preflight_policy()).unwrap()
        };
        if self.capped {
            let ids = cx.io.core.next_id;
            let result = cx.begin_planned_execution(
                &preflight,
                &steps(),
                plan_policy(),
                Window::UntilTerminal,
                "capped",
            );
            self.cap_ids_unchanged = cx.io.core.next_id == ids;
            self.record("capped execution", result.map(|_| ()));
            return;
        }
        if id == FOREIGN {
            let (plan, ..) = cx
                .begin_planned_execution(
                    &preflight,
                    &steps(),
                    plan_policy(),
                    Window::UntilTerminal,
                    "foreign",
                )
                .unwrap();
            self.foreign = Some((plan, preflight.owner()));
            return;
        }
        cx.park_preflight(&preflight).unwrap();
        cx.request_preflight_ready(&preflight).unwrap();
    }
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Context>, preflight: &Preflight) {
        if let Some((foreign, _)) = self.foreign.clone() {
            let leg = cx
                .plan_slots(&foreign)
                .and_then(|slots| cx.plan_leg_context(&foreign, &slots[0]))
                .map(|_| ());
            self.record("foreign leg", leg);
            let seal = cx.fail_plan(&foreign, "foreign");
            self.record("foreign seal", seal);
        }
        let admitted = cx.begin_planned_execution(
            preflight,
            &steps(),
            plan_policy(),
            Window::UntilTerminal,
            "own",
        );
        let Ok((plan, ..)) = admitted else {
            self.record("own admission", admitted.map(|_| ()));
            return;
        };
        let slots = cx.plan_slots(&plan);
        let leg = slots.and_then(|mut slots| cx.plan_leg_context(&plan, &slots.remove(0)));
        let leg = match leg {
            Ok(leg) => {
                self.record("own leg", Ok(()));
                leg
            }
            Err(error) => {
                self.record("own leg", Err(error));
                return;
            }
        };
        let sent = cx
            .request_managed(
                leg.clone(),
                Method::Get,
                "https://ready.test/own".into(),
                None,
                leg,
            )
            .map(|_| ())
            .map_err(|(_, error)| error);
        self.record("own send", sent);
        let failed = cx.fail_plan(&plan, "own failure");
        self.record("own failure", failed);
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
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

fn command(e: &mut Engine<Probe>, id: CommandId, at: f64) {
    e.handle(
        at,
        Event::Command(Command::CancelOrders {
            command: id,
            ids: vec![],
        }),
    );
}

#[test]
fn a_ready_founder_uses_its_own_physical_plan_and_never_a_foreign_one() {
    let mut e = crate::test_support::seeded_engine(Probe::default());
    e.start(0.);
    command(&mut e, FOREIGN, 0.);
    command(&mut e, READY, 1.);
    let actions = crate::test_support::prepared_actions(&mut e, 1.);
    assert_eq!(
        e.venue.results,
        [
            ("foreign leg", Err(Error::Revoked)),
            ("foreign seal", Err(Error::Revoked)),
            ("own leg", Ok(())),
            ("own send", Ok(())),
            ("own failure", Ok(())),
        ]
    );
    let terminal = |id| {
        actions
            .iter()
            .filter(|a| {
                matches!(a, Action::Notice(Notice::CommandDone { command, .. }
                    | Notice::CommandFailed { command, .. }) if *command == id)
            })
            .count()
    };
    assert_eq!(terminal(READY), 1, "the ready founder ends once, as itself");
    assert_eq!(terminal(FOREIGN), 0, "the foreign founder is untouched");
}

#[test]
fn only_the_exact_founder_generation_owns_a_physical_plan() {
    let mut e = crate::test_support::seeded_engine(Probe::default());
    e.start(0.);
    command(&mut e, FOREIGN, 0.);
    let (plan, founder) = e.venue.foreign.clone().expect("foreign Plan");
    let Owner::Command { id, generation } = founder else {
        panic!("command founder")
    };
    let registry = &e.io.core.request_contexts;
    assert!(registry.founded_by(plan.owner(), founder));
    assert!(!registry.founded_by(
        plan.owner(),
        Owner::Command {
            id,
            generation: generation + 1,
        }
    ));
    assert!(!registry.founded_by(
        plan.owner(),
        Owner::Command {
            id: READY,
            generation,
        }
    ));
    assert!(
        !registry.founded_by(founder, founder),
        "not a physical owner"
    );
}

#[test]
fn a_receipt_capped_preflight_cannot_found_a_planned_execution() {
    let mut e = crate::test_support::seeded_engine(Probe {
        capped: true,
        ..Default::default()
    });
    e.start(0.);
    command(&mut e, FOREIGN, 0.);
    assert_eq!(
        (&e.venue.results[..], e.venue.cap_ids_unchanged),
        (&[("capped execution", Err(Error::Invalid))][..], true)
    );
}
