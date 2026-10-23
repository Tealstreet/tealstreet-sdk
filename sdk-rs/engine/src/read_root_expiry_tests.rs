//! Original reproductions for engine-owned caller-wait Read root expiry
//! (docs/plans/RUST_READ_ROOT_EXPIRY.md). Only `admit_root` changes at cutover.
use super::*;
use crate::history::{ReadCompletion, ReadCursorHistory};
use crate::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use crate::request_context::{Context, Operation, Policy, RetryPolicy};
use std::cell::Cell;
use std::rc::Rc;

// Original adapter: today only a plain untracked root exists. A missing
// receipt has no engine refusal yet, so the adapter can only take the default.
fn admit_root<P>(cx: &mut Cx<'_, P>) -> Option<Context> {
    let wait = match cx.command_wait() {
        Some(CommandWait::Remaining(ms)) => ms,
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            return None;
        }
        None => crate::DEFAULT_COMMAND_WAIT_MS,
    };
    cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: wait,
        pacing: None,
        retry: RetryPolicy::Never,
    })
    .ok()
}

fn leg_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

struct Walk {
    history: ReadCursorHistory<String>,
    root: Context,
    leg: Option<Context>,
    pages: u32,
}

#[derive(Default)]
struct Probe {
    /// Park new queries instead of issuing their first page.
    park: bool,
    /// Commands at or above this id stay legacy: no root, no reply.
    legacy_from: Option<CommandId>,
    parked: HashMap<CommandId, Walk>,
    roots: HashMap<CommandId, Context>,
    finalized: Rc<Cell<usize>>,
}

impl Probe {
    fn issue(&mut self, cx: &mut Cx<'_, Box<Walk>>, mut walk: Walk) {
        let Ok(leg) = cx.http_leg_context(&walk.root, leg_policy()) else {
            return;
        };
        walk.leg = Some(leg.clone());
        let _ = cx.request_managed(
            leg,
            Method::Get,
            format!("https://root.test/page/{}", walk.pages),
            None,
            Box::new(walk),
        );
    }

    // Native accepted-only recovery on an actual page failure.
    fn fail(&mut self, cx: &mut Cx<'_, Box<Walk>>, walk: Walk, error: &str) {
        let finalized = self.finalized.clone();
        let _ = walk
            .history
            .finish_with(cx, Err(VenueError::new(error)), move |rows| {
                finalized.set(finalized.get() + 1);
                serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
            });
    }
}

fn stall(_: Stall) -> String {
    "stalled".into()
}
fn encode(rows: &[String]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|e| VenueError::new(e.to_string()))
}

impl Venue for Probe {
    type Pending = Box<Walk>;
    fn start(&mut self, _: &mut Cx<'_, Box<Walk>>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Box<Walk>>, command: Command) {
        let id = command.completion_id().unwrap();
        if self.legacy_from.is_some_and(|from| id >= from) {
            return;
        }
        let Some(root) = admit_root(cx) else {
            return;
        };
        let completion = ReadCompletion::from_context(cx, id, root.clone()).unwrap();
        self.roots.insert(id, root.clone());
        let walk = Walk {
            history: ReadCursorHistory::new(completion, CursorWalk::new(10), stall, encode),
            root,
            leg: None,
            pages: 0,
        };
        if self.park {
            self.parked.insert(id, walk);
        } else {
            self.issue(cx, walk);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Box<Walk>>, walk: Box<Walk>, response: Response<'_>) {
        let mut walk = *walk;
        let leg = walk.leg.clone().unwrap();
        walk.pages += 1;
        let step = {
            let Ok(mut page) = walk.history.admit(cx, &leg) else {
                return;
            };
            page.stage(response.body.into());
            page.advance(CursorPage {
                next: Some(format!("p{}", walk.pages)),
                rows: 1,
                fresh: 1,
                want_more: walk.pages < 2,
            })
        };
        if let CursorStep::More(_) = step {
            self.issue(cx, walk);
        } else {
            let finalized = self.finalized.clone();
            let _ = walk.history.finish_with(cx, Ok(()), move |rows| {
                finalized.set(finalized.get() + 1);
                serde_json::to_string(&rows).map_err(|e| VenueError::new(e.to_string()))
            });
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Box<Walk>>, walk: Box<Walk>, error: &str) {
        self.fail(cx, *walk, error);
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Box<Walk>>, _: TimerId) {}
    fn on_open(&mut self, _: &mut Cx<'_, Box<Walk>>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Box<Walk>>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Box<Walk>>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine(probe: Probe) -> Engine<Probe> {
    let mut e = crate::test_support::seeded_engine(probe);
    e.start(0.);
    e.drain_actions().for_each(drop);
    e
}

fn query(command: CommandId) -> Command {
    Command::FetchCandles {
        command,
        symbol: "BTCUSD".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    }
}

fn receive(e: &mut Engine<Probe>, now: f64, command: CommandId, wait: u32) {
    e.handle_command(now, query(command), Some(wait)).unwrap();
}

/// A public engine entry at `now` that neither the venue nor a leg owns.
fn tick(e: &mut Engine<Probe>, now: f64) {
    e.handle(now, Event::PollScaleChanged { scale: 1. });
}

fn released(e: &mut Engine<Probe>, now: f64) -> Vec<HttpRequest> {
    crate::test_support::prepared_actions(e, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn respond(e: &mut Engine<Probe>, now: f64, request: &HttpRequest, body: &str) {
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

/// This command's terminals and INCOMPLETE markers, in order.
fn outcome(actions: &[Action], command: CommandId) -> (Vec<Result<String, VenueError>>, usize) {
    let terminals = actions
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
        .collect();
    let markers = actions
        .iter()
        .filter(|a| {
            matches!(a, Action::Notice(Notice::Event(event))
                if event.code == log_code::HISTORY_INCOMPLETE
                    && event.fields.contains(&("command", Value::Num(f64::from(command)))))
        })
        .count();
    (terminals, markers)
}

fn assert_strict_timeout(actions: &[Action], command: CommandId, finalized: usize, label: &str) {
    let (terminals, markers) = outcome(actions, command);
    assert!(
        matches!(terminals.as_slice(), [Err(error)]
            if error.kind == Some(ErrorKind::Timeout) && error.outcome.is_none()),
        "{label}: one strict Timeout, got {terminals:?}"
    );
    assert_eq!(markers, 0, "{label}: no INCOMPLETE marker");
    assert_eq!(finalized, 0, "{label}: no native finalizer");
}

#[test]
fn a_parked_root_expires_at_its_deadline_without_venue_help() {
    let mut e = engine(Probe {
        park: true,
        ..Default::default()
    });
    receive(&mut e, 0., 7, 1_000);
    e.drain_actions().for_each(drop);
    tick(&mut e, 1_000.);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_strict_timeout(&actions, 7, e.venue.finalized.get(), "parked");
}

#[test]
fn a_fractional_root_deadline_is_kept_exactly() {
    let mut e = engine(Probe {
        park: true,
        ..Default::default()
    });
    receive(&mut e, 0.25, 7, 1_000);
    assert_eq!(e.venue.roots[&7].deadline(), 1_000.25);
    e.drain_actions().for_each(drop);
    tick(&mut e, 999.75);
    let (terminals, _) = outcome(&e.drain_actions().collect::<Vec<_>>(), 7);
    assert!(terminals.is_empty(), "a live root does not expire early");
    tick(&mut e, 1_000.25);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_strict_timeout(&actions, 7, e.venue.finalized.get(), "equality");
}

/// Page 1 accepted at 10 000; page 2's leg and the root share deadline 30 000.
fn second_page_in_flight(release: bool) -> (Engine<Probe>, Option<HttpRequest>) {
    let mut e = engine(Probe::default());
    receive(&mut e, 0., 7, 30_000);
    let page = released(&mut e, 0.).pop().expect("first page");
    respond(&mut e, 10_000., &page, "row-1");
    let second = if release {
        released(&mut e, 10_000.).pop()
    } else {
        e.drain_actions().for_each(drop);
        None
    };
    (e, second)
}

/// The in-flight page's own leg timer, bound to its exact managed request.
fn leg_timer(e: &Engine<Probe>) -> TimerId {
    assert_eq!(e.io.managed_http.len(), 1, "exactly one page in flight");
    e.io.managed_http.values().next().unwrap().timer
}

fn released_page_expiry(leg_first: bool) -> Engine<Probe> {
    let (mut e, second) = second_page_in_flight(true);
    let second = second.expect("second page released");
    let timer = leg_timer(&e);
    if leg_first {
        e.handle(30_000., Event::TimerFired { timer });
        tick(&mut e, 30_000.);
    } else {
        tick(&mut e, 30_000.);
        e.handle(30_000., Event::TimerFired { timer });
    }
    respond(&mut e, 30_001., &second, "late-row");
    e
}

#[test]
fn root_expiry_with_a_released_page_in_flight_is_strict_leg_timer_first() {
    let mut e = released_page_expiry(true);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_strict_timeout(&actions, 7, e.venue.finalized.get(), "leg first");
}

#[test]
fn root_expiry_with_a_released_page_in_flight_is_strict_root_deadline_first() {
    let mut e = released_page_expiry(false);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_strict_timeout(&actions, 7, e.venue.finalized.get(), "root first");
}

#[test]
fn root_expiry_with_a_page_still_preparing_is_strict() {
    let (mut e, _) = second_page_in_flight(false);
    let timer = leg_timer(&e);
    e.handle(30_000., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert_strict_timeout(&actions, 7, e.venue.finalized.get(), "preparing");
}

#[test]
fn a_leg_only_timeout_under_a_live_root_keeps_accepted_rows() {
    let mut e = engine(Probe::default());
    receive(&mut e, 0., 7, 300_000);
    let page = released(&mut e, 0.).pop().expect("first page");
    respond(&mut e, 10_000., &page, "row-1");
    released(&mut e, 10_000.).pop().expect("second page");
    let timer = leg_timer(&e);
    e.handle(30_000., Event::TimerFired { timer });
    let (terminals, markers) = outcome(&e.drain_actions().collect::<Vec<_>>(), 7);
    assert!(matches!(terminals.as_slice(), [Ok(rows)] if rows == r#"["row-1"]"#));
    assert_eq!(markers, 1);
    assert_eq!(e.venue.finalized.get(), 1);
}

fn same_id_arrival(arrival: f64) {
    let mut e = engine(Probe {
        park: true,
        ..Default::default()
    });
    receive(&mut e, 0., 7, 1_000);
    let old = e.venue.roots[&7].clone();
    e.drain_actions().for_each(drop);
    e.venue.park = false;
    receive(&mut e, arrival, 7, 5_000);
    let new = e.venue.roots[&7].clone();
    let actions: Vec<_> = e.drain_actions().collect();
    let (terminals, _) = outcome(&actions, 7);
    let old_ended_once =
        matches!(terminals.as_slice(), [Err(error)] if error.kind == Some(ErrorKind::Timeout));
    assert_eq!(
        (new.deadline(), new.owner() != old.owner(), old_ended_once),
        (arrival + 5_000., true, true),
        "arrival {arrival}: (new root deadline, new generation, old root ended once)"
    );
}

#[test]
fn a_same_id_arrival_at_the_deadline_gets_its_own_receipt() {
    same_id_arrival(1_000.);
}

#[test]
fn a_same_id_arrival_past_the_deadline_gets_its_own_receipt() {
    same_id_arrival(1_500.);
}

#[test]
fn two_roots_due_together_each_end_once_and_a_live_root_is_untouched() {
    let mut e = engine(Probe {
        park: true,
        ..Default::default()
    });
    receive(&mut e, 0., 7, 1_000);
    receive(&mut e, 0., 8, 1_000);
    receive(&mut e, 0., 9, 60_000);
    e.drain_actions().for_each(drop);
    tick(&mut e, 1_000.);
    let actions: Vec<_> = e.drain_actions().collect();
    let strict = |command| {
        matches!(outcome(&actions, command), (terminals, 0) if matches!(terminals.as_slice(),
            [Err(error)] if error.kind == Some(ErrorKind::Timeout) && error.outcome.is_none()))
    };
    let root = e.venue.roots[&9].clone();
    let cx = Cx {
        now: 2_000.,
        store: &mut e.store,
        io: &mut e.io,
    };
    let live = outcome(&actions, 9).0.is_empty() && cx.validate_read_root_context(&root).is_ok();
    assert_eq!(
        (strict(7), strict(8), live),
        (true, true, true),
        "(first due root, second due root, live third root)"
    );
}

fn public_entry(entry: &str) {
    let mut e = engine(Probe {
        park: true,
        ..Default::default()
    });
    receive(&mut e, 0., 7, 1_000);
    e.drain_actions().for_each(drop);
    match entry {
        "prepare_dispatch" => e.prepare_dispatch(1_000.),
        _ => {
            let _ = e.take_frame(1_000.);
        }
    }
    let actions: Vec<_> = e.drain_actions().collect();
    assert_strict_timeout(&actions, 7, 0, entry);
}

#[test]
fn prepare_dispatch_expires_a_due_root_first() {
    public_entry("prepare_dispatch");
}

#[test]
fn take_frame_expires_a_due_root_first() {
    public_entry("take_frame");
}

fn missing_receipt(duplicate: bool) {
    let label = if duplicate {
        "duplicate legacy id"
    } else {
        "exhausted sequence"
    };
    let mut e = engine(Probe {
        legacy_from: Some(50),
        ..Default::default()
    });
    let command = if duplicate {
        receive(&mut e, 0., 50, 300_000);
        e.venue.legacy_from = None;
        50
    } else {
        e.io.command_waits.exhaust_receipts();
        7
    };
    e.drain_actions().for_each(drop);
    receive(&mut e, 1., command, 5_000);
    let minted = e.venue.roots.contains_key(&command);
    let (terminals, _) = outcome(&e.drain_actions().collect::<Vec<_>>(), command);
    let refused = matches!(terminals.as_slice(), [Err(error)] if error.outcome.is_none());
    assert_eq!(
        (minted, refused),
        (false, true),
        "{label}: (default root minted, one typed Read refusal), got {terminals:?}"
    );
}

#[test]
fn a_duplicate_legacy_receipt_is_refused_without_a_default_root() {
    missing_receipt(true);
}

#[test]
fn an_exhausted_receipt_sequence_is_refused_without_a_default_root() {
    missing_receipt(false);
}
