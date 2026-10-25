use super::*;
use crate::book::Book;
use crate::request_context::CommandEntryError;
use crate::{
    Action, Command, Engine, Event, Method, Notice, Query, Response, SocketId, TimerId, Venue,
};

#[derive(Clone, Copy)]
enum Mode {
    Spent,
    Borrowed,
    Exhausted,
    LegacyDuplicate,
    Reuse,
}

#[derive(Debug, PartialEq)]
struct Counts {
    ids: crate::resource_ids::ResourceIds,
    contexts: usize,
    commands: usize,
    credits: usize,
    plans: usize,
    timers: usize,
    pending: usize,
}
fn counts(cx: &Cx<'_, ()>) -> Counts {
    Counts {
        ids: cx.io.core.next_id,
        contexts: cx.io.core.request_contexts.contexts.len(),
        commands: cx.io.core.request_contexts.commands.len(),
        credits: cx.io.core.request_contexts.reserved_contexts,
        plans: cx.io.core.request_contexts.plan_index.len(),
        timers: cx.io.core.timers.len(),
        pending: cx.io.pending.len(),
    }
}
fn policy(purpose: PreflightPurpose) -> PreflightPolicy {
    PreflightPolicy {
        purpose,
        read_credential: None,
        write_credential: None,
    }
}
struct Probe {
    mode: Mode,
    calls: usize,
    timer: Option<TimerId>,
    old: Option<Preflight>,
    checked: usize,
}
impl Probe {
    fn refuse(
        &mut self,
        cx: &mut Cx<'_, ()>,
        purpose: PreflightPurpose,
        expected: CommandEntryError,
    ) {
        let before = counts(cx);
        assert_eq!(
            cx.begin_preflight_with_command_wait(policy(purpose))
                .map(|_| ()),
            Err(expected)
        );
        assert_eq!(counts(cx), before);
        self.checked += 1;
    }
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if matches!(self.mode, Mode::Borrowed) {
            self.timer = Some(cx.set_timer(1.));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        self.calls += 1;
        match self.mode {
            Mode::Spent => {
                cx.set_timer(100.);
                self.refuse(cx, PreflightPurpose::ReadOnly, CommandEntryError::Revoked);
            }
            Mode::Exhausted => {
                assert_eq!(cx.command_wait(), None);
                self.refuse(
                    cx,
                    PreflightPurpose::ReadOnly,
                    CommandEntryError::Rejected(Error::Capacity),
                );
            }
            Mode::LegacyDuplicate if self.calls == 1 => {
                cx.request(
                    Method::Get,
                    "https://receipt.test/legacy".into(),
                    vec![],
                    None,
                    (),
                );
            }
            Mode::LegacyDuplicate => {
                assert_eq!(cx.command_wait(), None);
                self.refuse(
                    cx,
                    PreflightPurpose::ReadOnly,
                    CommandEntryError::Rejected(Error::Revoked),
                );
            }
            Mode::Reuse => {
                let owner = cx
                    .begin_preflight_with_command_wait(policy(PreflightPurpose::ReadOnly))
                    .unwrap();
                let cap = cx
                    .io
                    .core
                    .request_contexts
                    .preflight_receipt_cap(&owner)
                    .unwrap()
                    .unwrap();
                assert_eq!(cap.1, cx.now + 100.);
                if let Some(old) = &self.old {
                    let before = counts(cx);
                    assert_eq!(
                        cx.io.core.request_contexts.preflight_receipt_cap(old),
                        Err(Error::Revoked)
                    );
                    assert_eq!(counts(cx), before);
                    self.checked += 1;
                }
                self.old = Some(owner.clone());
                cx.done_preflight(&owner, "cached".into()).unwrap();
            }
            Mode::Borrowed => unreachable!(),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: TimerId) {
        self.refuse(cx, PreflightPurpose::ReadOnly, CommandEntryError::Revoked);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        assert_eq!(cx.command_wait(), None);
        self.refuse(cx, PreflightPurpose::ReadOnly, CommandEntryError::Revoked);
        cx.done(7, "original legacy owner".into());
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(mode: Mode) -> Engine<Probe> {
    let mut e = crate::test_support::seeded_engine(Probe {
        mode,
        calls: 0,
        timer: None,
        old: None,
        checked: 0,
    });
    e.start(0.);
    e
}

fn query() -> Command {
    Command::Query {
        command: 7,
        query: Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }
}
fn failures(e: &mut Engine<Probe>) -> Vec<VenueError> {
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect()
}
#[test]
// A WriteAfterReads capture now binds its cap (docs/plans/RUST_WRITE_PREFLIGHT_WAIT.md;
// write_preflight_wait_tests); only a spent entry still refuses with no effects.
fn resource_spent_capture_leaves_every_pool_unchanged() {
    let mut e = engine(Mode::Spent);
    e.drain_actions().for_each(drop);
    e.handle_command(0., query(), Some(100)).unwrap();
    assert_eq!(e.venue.checked, 1);
    assert!(failures(&mut e).is_empty());
}
#[test]
fn borrowed_timer_cannot_capture_or_emit_a_terminal() {
    let mut e = engine(Mode::Borrowed);
    let timer = e.venue.timer.unwrap();
    e.drain_actions().for_each(drop);
    e.handle(1., Event::TimerFired { timer });
    assert_eq!(e.venue.checked, 1);
    assert!(failures(&mut e).is_empty());
}
#[test]
fn exhausted_sequence_uses_one_read_capacity_refusal_without_allocating() {
    let mut e = engine(Mode::Exhausted);
    e.drain_actions().for_each(drop);
    e.io.core.command_waits.exhaust_receipts();
    e.handle_command(0., query(), Some(100)).unwrap();
    assert_eq!(e.venue.checked, 1);
    assert_eq!(failures(&mut e), vec![Error::Capacity.read_venue_error()]);
}
#[test]
fn duplicate_legacy_receipt_cannot_retire_original_stamp_or_http_owner() {
    let mut e = engine(Mode::LegacyDuplicate);
    e.drain_actions().for_each(drop);
    e.handle_command(0., query(), Some(100)).unwrap();
    let packet = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(p) = a {
                Some(p)
            } else {
                None
            }
        })
        .unwrap();
    e.io.core.command_waits.exhaust_receipts();
    e.handle_command(5., query(), Some(1_000)).unwrap();
    assert_eq!(e.io.core.command_waits.deadline(7), Some(100.));
    assert_eq!(e.io.pending.len(), 1);
    assert_eq!(failures(&mut e), vec![Error::Revoked.read_venue_error()]);
    e.handle(
        6.,
        Event::HttpResponse {
            request: packet.id,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
    assert_eq!(e.venue.checked, 2);
    assert_eq!(e.io.core.command_waits.deadline(7), None);
    let done: Vec<_> = e.drain_actions().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "original legacy owner")).collect();
    assert_eq!(done.len(), 1);
}
#[test]
fn reused_id_cannot_attach_a_new_receipt_to_a_retired_preflight() {
    let mut e = engine(Mode::Reuse);
    e.drain_actions().for_each(drop);
    e.handle_command(0., query(), Some(100)).unwrap();
    e.drain_actions().for_each(drop);
    e.handle_command(10., query(), Some(100)).unwrap();
    assert_eq!(e.venue.checked, 1);
    assert!(failures(&mut e).is_empty());
}
