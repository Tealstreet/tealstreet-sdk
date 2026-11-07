//! Original existing-API adapter; no new-method compile failures count as REDs.
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Command, CommandId, Cx, Engine, ErrorKind, Event, Notice, Outcome, Response, SocketId,
    Store, TimerId, Venue, VenueError,
};
use std::sync::{Arc, Mutex};

fn mutation() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

// Only this adapter changes at cutover. The current truthful Mutation path
// projects the local setting but cannot seal its empty journal as success.
fn local_success<P>(cx: &mut Cx<'_, P>, project: impl FnOnce(&mut Store)) -> Result<(), String> {
    cx.done_local_setting("[]".into(), project)
        .map_err(|e| format!("{e:?}"))
}

#[derive(Default)]
struct Observed {
    projections: usize,
    results: Vec<Result<(), String>>,
}

struct Probe {
    legacy_success: bool,
    roots: Vec<Context>,
    observed: Arc<Mutex<Observed>>,
}

impl Probe {
    fn local(&mut self, cx: &mut Cx<'_, ()>) {
        let observed = self.observed.clone();
        let result = local_success(cx, |store| {
            observed.lock().unwrap().projections += 1;
            store.set_hedged(false);
        });
        self.observed.lock().unwrap().results.push(result);
    }
}

impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.store.set_hedged(true);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        match command {
            Command::SetPositionMode {
                command, symbol, ..
            } => match symbol.as_deref() {
                Some("hold") => self.roots.push(cx.admit_http_command(mutation()).unwrap()),
                Some("legacy-hold") => {}
                Some("refuse") => {
                    if let Ok(root) = cx.admit_http_command(mutation()) {
                        cx.fail_context(
                            &root,
                            VenueError::new("native unsupported setting")
                                .kind(ErrorKind::Unsupported),
                        )
                        .unwrap();
                    }
                }
                Some("twice") => {
                    self.local(cx);
                    self.local(cx);
                }
                Some("bound") => {
                    self.roots.push(cx.admit_http_command(mutation()).unwrap());
                    self.local(cx);
                }
                Some("timer") => {
                    cx.set_timer(10.);
                }
                Some("panic") => {
                    let observed = self.observed.clone();
                    let _ = local_success(cx, |store| {
                        observed.lock().unwrap().projections += 1;
                        store.set_hedged(false);
                        panic!("native local projection failed");
                    });
                }
                _ if self.legacy_success => {
                    self.observed.lock().unwrap().projections += 1;
                    cx.store.set_hedged(false);
                    cx.done(command, "[]".into());
                }
                _ => self.local(cx),
            },
            Command::FetchStpMode { .. } => self.local(cx),
            _ => self.local(cx),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: TimerId) {
        self.local(cx);
    }
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
}

fn engine(legacy_success: bool) -> (Engine<Probe>, Arc<Mutex<Observed>>) {
    let observed = Arc::new(Mutex::new(Observed::default()));
    let mut e = support::seeded_engine(Probe {
        legacy_success,
        roots: Vec::new(),
        observed: observed.clone(),
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    (e, observed)
}

fn setting(command: CommandId, symbol: &str) -> Command {
    Command::SetPositionMode {
        command,
        hedged: false,
        symbol: Some(symbol.into()),
    }
}

fn assert_no_io(actions: &[Action]) {
    assert!(actions.iter().all(|a| matches!(a, Action::Notice(_))));
}

fn successes(actions: &[Action], id: CommandId) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(a, Action::Notice(Notice::CommandDone { command, result })
            if *command == id && result == "[]")
        })
        .count()
}

#[test]
fn a_genuine_keyless_local_mutation_projects_and_finishes_once_without_io() {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(setting(7, "local")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(!e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 1);
    assert_eq!(
        successes(&actions, 7),
        1,
        "empty Mutation journal currently refuses success"
    );
    assert_eq!(observed.lock().unwrap().results, [Ok(())]);
}

#[test]
fn the_original_native_numeric_local_success_is_a_working_control() {
    let (mut e, observed) = engine(true);
    e.handle(1., Event::Command(setting(7, "local")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(!e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 1);
    assert_eq!(successes(&actions, 7), 1);
}

#[test]
fn a_duplicate_of_a_live_legacy_id_cannot_project_a_new_local_setting() {
    let (mut e, observed) = engine(false);
    e.handle(0., Event::Command(setting(7, "legacy-hold")));
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(setting(7, "local")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert_eq!(successes(&actions, 7), 0);
    assert!(
        e.store().hedged(),
        "duplicate receipt must refuse before Store projection"
    );
    assert_eq!(observed.lock().unwrap().projections, 0);
}

#[test]
fn a_read_entry_cannot_borrow_local_setting_success() {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(Command::FetchStpMode { command: 7 }));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert_eq!(successes(&actions, 7), 0);
    assert!(
        e.store().hedged(),
        "Read dispatch is not a local Setting entry"
    );
    assert_eq!(observed.lock().unwrap().projections, 0);
}

#[test]
fn existing_managed_generation_denies_duplicate_before_projection() {
    let (mut e, observed) = engine(false);
    e.handle(0., Event::Command(setting(7, "hold")));
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(setting(7, "local")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 0);
}

#[test]
fn capacity_refusal_preserves_store_and_native_typed_initial_failure() {
    let (mut e, observed) = engine(false);
    for id in 100..356 {
        e.handle(0., Event::Command(setting(id, "hold")));
        e.drain_actions().for_each(drop);
    }
    e.handle(1., Event::Command(setting(7, "local")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 0);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 7, error })
        if error == &Error::Capacity.venue_error()))
            .count(),
        1
    );
}

#[test]
fn native_keyless_mutation_failure_keeps_its_existing_authority_path() {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(setting(7, "refuse")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 0);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 7, error })
        if error.kind == Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent)))
            .count(),
        1
    );
}

#[test]
fn timer_callback_cannot_project_or_finish_a_local_setting() {
    let (mut e, observed) = engine(false);
    e.handle(0., Event::Command(setting(7, "timer")));
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms: 10. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(10., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 0);
    assert_eq!(successes(&actions, 7), 0);
}

#[test]
fn a_panicking_local_projection_poison_fences_partial_store_publication() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let (mut e, observed) = engine(false);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            e.handle(1., Event::Command(setting(7, "panic")));
        }))
        .is_err()
    );
    assert!(!e.store().hedged(), "the projection really mutated Store");
    let frame_denied = catch_unwind(AssertUnwindSafe(|| {
        let _ = e.take_frame(2.);
    }))
    .is_err();
    let drain_denied = catch_unwind(AssertUnwindSafe(|| {
        e.drain_actions().for_each(drop);
    }))
    .is_err();
    let failed = e.failed();
    assert!(
        failed && frame_denied && drain_denied,
        "failed={failed}, frame_denied={frame_denied}, drain_denied={drain_denied}"
    );
    assert_eq!(observed.lock().unwrap().projections, 1);
}

fn assert_setting_success(command: Command) {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(command));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert_eq!(successes(&actions, 7), 1);
    assert_eq!(observed.lock().unwrap().projections, 1);
}

#[test]
fn leverage_dispatch_has_local_setting_authority() {
    assert_setting_success(Command::SetLeverage {
        command: 7,
        symbol: "X".into(),
        leverage: 2.,
        side: None,
    });
}
#[test]
fn margin_dispatch_has_local_setting_authority() {
    assert_setting_success(Command::SetMarginType {
        command: 7,
        symbol: "X".into(),
        isolated: false,
    });
}
#[test]
fn stp_dispatch_has_local_setting_authority() {
    assert_setting_success(Command::SetStpMode {
        command: 7,
        mode: safe_cex_engine::model::StpMode::ExpireBoth,
    });
}
fn assert_wrong_dispatch(command: Command) {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(command));
    let actions: Vec<_> = e.drain_actions().collect();
    assert_no_io(&actions);
    assert!(actions.is_empty());
    assert!(e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 0);
}
#[test]
fn place_dispatch_cannot_borrow_local_setting_authority() {
    assert_wrong_dispatch(Command::PlaceOrders {
        command: 7,
        orders: vec![],
    });
}
#[test]
fn cancel_dispatch_cannot_borrow_local_setting_authority() {
    assert_wrong_dispatch(Command::CancelOrders {
        command: 7,
        ids: vec![],
    });
}
#[test]
fn amend_dispatch_cannot_borrow_local_setting_authority() {
    assert_wrong_dispatch(Command::AmendOrder {
        command: 7,
        id: "old".into(),
        price: None,
        amount: None,
    });
}
#[test]
fn completion_is_single_and_settled_id_can_be_reused() {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(setting(7, "twice")));
    let first: Vec<_> = e.drain_actions().collect();
    assert_eq!(successes(&first, 7), 1);
    assert_eq!(observed.lock().unwrap().projections, 1);
    assert_eq!(
        observed.lock().unwrap().results,
        [Ok(()), Err("Revoked".into())]
    );
    e.handle(2., Event::Command(setting(7, "local")));
    let second: Vec<_> = e.drain_actions().collect();
    assert_eq!(successes(&second, 7), 1);
    assert_eq!(observed.lock().unwrap().projections, 2);
}
#[test]
fn bound_context_cannot_be_relabelled_as_local_success() {
    let (mut e, observed) = engine(false);
    e.handle(1., Event::Command(setting(7, "bound")));
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.is_empty());
    assert!(e.store().hedged());
    assert_eq!(observed.lock().unwrap().projections, 0);
}
