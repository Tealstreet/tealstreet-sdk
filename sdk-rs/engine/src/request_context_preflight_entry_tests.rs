use super::*;
use crate::request_context::{CommandEntryError, Preflight, PreflightPolicy, PreflightPurpose};
use crate::{Action, Command, Event, Notice, Response, SocketId, TimerId, Venue};

fn policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::ReadOnly,
        read_credential: None,
        write_credential: None,
    }
}
fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: crate::request_context::RetryPolicy::Never,
    }
}
fn initial(cx: &mut Cx<'_, ()>, policy: PreflightPolicy) -> Result<Preflight, CommandEntryError> {
    cx.begin_preflight(policy)
}
#[derive(Default)]
struct Probe {
    callback: Option<fn(&mut Cx<'_, ()>)>,
    outside: bool,
    fill: bool,
    checks: usize,
    timer: Option<TimerId>,
}
impl Probe {
    fn outside(&mut self, cx: &mut Cx<'_, ()>) {
        if self.outside {
            let ids = cx.io.next_id;
            assert!(matches!(
                initial(cx, policy()),
                Err(CommandEntryError::Revoked)
            ));
            assert_eq!(cx.io.next_id, ids);
            assert!(cx.io.request_contexts.commands.is_empty());
            self.checks += 1;
        }
    }
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if self.outside {
            self.timer = Some(cx.set_timer(10.));
            self.outside(cx);
        }
        if self.fill {
            for command in 100..356 {
                cx.http_context(Scope::Command(command), read_policy())
                    .unwrap();
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        if let Some(callback) = self.callback {
            callback(cx);
        }
        self.checks += 1;
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: TimerId) {
        self.outside(cx);
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, ()>) {
        self.outside(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, ()>, _: SocketId) {
        self.outside(cx);
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn command() -> Event<'static> {
    Event::Command(Command::FetchCandles {
        command: 7,
        symbol: "BTCUSD".into(),
        interval: crate::model::Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    })
}
fn run(callback: fn(&mut Cx<'_, ()>), fill: bool) -> Vec<Action> {
    let mut engine = crate::test_support::seeded_engine(Probe {
        callback: Some(callback),
        fill,
        ..Default::default()
    });
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(1., command());
    assert_eq!(engine.venue().checks, 1);
    engine.drain_actions().collect()
}
fn terminals(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(
                    Notice::CommandDone { command: 7, .. }
                        | Notice::CommandFailed { command: 7, .. }
                )
            )
        })
        .count()
}
#[test]
fn preflight_has_no_initial_authority_in_start_timer_feed_or_socket_callbacks() {
    let mut engine = crate::test_support::seeded_engine(Probe {
        outside: true,
        ..Default::default()
    });
    engine.start(0.);
    let timer = engine.venue().timer.unwrap();
    engine.drain_actions().for_each(drop);
    engine.handle(10., Event::TimerFired { timer });
    engine.apply_shared_tickers(11.);
    engine.handle(
        12.,
        Event::WsOpened {
            socket: SocketId(9),
        },
    );
    assert_eq!(engine.venue().checks, 4);
    assert_eq!(terminals(&engine.drain_actions().collect::<Vec<_>>()), 0);
}
#[test]
fn preflight_cannot_borrow_entry_after_an_opaque_side_effect() {
    let actions = run(
        |cx| {
            cx.send(SocketId(9), "native activity");
            let ids = cx.io.next_id;
            assert!(matches!(
                initial(cx, policy()),
                Err(CommandEntryError::Revoked)
            ));
            assert_eq!(cx.io.next_id, ids);
            assert!(cx.io.request_contexts.commands.is_empty());
        },
        false,
    );
    assert_eq!(terminals(&actions), 0);
}
#[test]
fn context_initial_owner_excludes_preflight_without_a_second_rejection() {
    let actions = run(
        |cx| {
            let root = cx.admit_http_command(read_policy()).unwrap();
            let ids = cx.io.next_id;
            assert!(matches!(
                initial(cx, policy()),
                Err(CommandEntryError::Revoked)
            ));
            assert_eq!(cx.io.next_id, ids);
            cx.done_context(&root, "cached".into()).unwrap();
        },
        false,
    );
    assert_eq!(terminals(&actions), 1);
}
#[test]
fn preflight_initial_owner_excludes_context_and_completes_once() {
    let actions = run(
        |cx| {
            let owner = initial(cx, policy()).unwrap();
            assert!(cx.io.entry_bound(7));
            assert_eq!(
                cx.admit_http_command(read_policy()),
                Err(CommandEntryError::Revoked)
            );
            cx.done_preflight(&owner, "cached".into()).unwrap();
        },
        false,
    );
    assert_eq!(terminals(&actions), 1);
}
#[test]
fn cached_preflight_terminal_cannot_remint_its_entry() {
    let actions = run(
        |cx| {
            let owner = initial(cx, policy()).unwrap();
            cx.done_preflight(&owner, "cached".into()).unwrap();
            let ids = cx.io.next_id;
            assert!(matches!(
                initial(cx, policy()),
                Err(CommandEntryError::Revoked)
            ));
            assert_eq!(cx.io.next_id, ids);
            cx.done(7, "duplicate".into());
            cx.fail(7, "duplicate");
        },
        false,
    );
    assert_eq!(terminals(&actions), 1);
}
#[test]
fn invalid_preflight_is_one_engine_owned_read_rejection() {
    let actions = run(
        |cx| {
            let mut invalid = policy();
            invalid.write_credential = Some(crate::request_context::Credential {
                slot: crate::CredentialSlot::Primary,
                reference: crate::signing::KeyRef("invalid-read-write".into()),
            });
            assert!(matches!(
                initial(cx, invalid),
                Err(CommandEntryError::Rejected(Error::Invalid))
            ));
        },
        false,
    );
    assert_eq!(
        terminals(&actions),
        1,
        "unissued preflight cannot leave its caller hanging"
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {command: 7, error}) if error.outcome.is_none())));
}
#[test]
fn preflight_owner_capacity_rejection_cannot_remint_or_emit_twice() {
    let actions = run(
        |cx| {
            let ids = cx.io.next_id;
            assert!(matches!(
                initial(cx, policy()),
                Err(CommandEntryError::Rejected(Error::Capacity))
            ));
            assert!(matches!(
                initial(cx, policy()),
                Err(CommandEntryError::Revoked)
            ));
            assert_eq!(cx.io.next_id, ids);
            cx.done(7, "duplicate".into());
            cx.fail(7, "duplicate");
        },
        true,
    );
    assert_eq!(terminals(&actions), 1);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {command: 7, error}) if error.outcome.is_none())));
}
#[test]
fn invalid_write_preflight_rejection_proves_not_sent() {
    let actions = run(
        |cx| {
            let mut invalid = policy();
            invalid.purpose = PreflightPurpose::WriteAfterReads;
            assert!(matches!(
                initial(cx, invalid),
                Err(CommandEntryError::Rejected(Error::Invalid))
            ));
        },
        false,
    );
    assert_eq!(terminals(&actions), 1);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {command: 7, error}) if error.outcome == Some(crate::Outcome::NotSent))));
}
#[test]
fn genuine_preflight_keeps_its_captured_command_intent_through_admission() {
    let actions = run(
        |cx| {
            let owner = initial(cx, policy()).unwrap();
            assert!(
                matches!(owner.owner(), Owner::Command { id: 7, generation } if generation == cx.io.request_contexts.commands[&7].generation)
            );
            assert_eq!(
                cx.io.request_contexts.commands[&7].intent,
                Some(crate::classify::Intent::Read)
            );
            cx.done_preflight(&owner, "cached".into()).unwrap();
        },
        false,
    );
    assert_eq!(terminals(&actions), 1);
}
