use super::*;
use crate::request_context::{CommandEntryError, Context, Error};
use std::collections::HashMap;

#[derive(Default)]
struct NativeRead {
    roots: HashMap<CommandId, Context>,
    timers: HashMap<TimerId, CommandId>,
    calls: usize,
    terminal_results: Vec<Result<(), Error>>,
    generic_refusal: bool,
    extension_refusal: bool,
    numeric_refusal: bool,
}
impl Venue for NativeRead {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        self.calls += 1;
        let id = command.completion_id().unwrap();
        let mut policy = read_policy();
        policy.timeout_ms = 5000;
        let root = match cx.admit_http_command(policy) {
            Ok(root) => root,
            Err(CommandEntryError::Rejected(_)) => return,
            Err(CommandEntryError::Revoked) => panic!("unexpected command entry revocation"),
        };
        if self.generic_refusal && id == 8 {
            assert_eq!(
                cx.http_context(Scope::Command(7), read_policy()),
                Err(Error::State)
            );
        }
        if self.extension_refusal && id == 8 {
            assert_eq!(
                cx.http_context(Scope::Command(7), read_policy()),
                Err(Error::State)
            );
        }
        if id == 8 {
            cx.done_context(&root, "cached".into()).unwrap();
        } else {
            let timer = cx.set_timer(5000.);
            self.timers.insert(timer, id);
            self.roots.insert(id, root);
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
        if let Some(command) = self.timers.remove(&timer) {
            let root = self.roots.get(&command).unwrap();
            if self.numeric_refusal {
                cx.done(command, "wrong numeric completion".into());
                cx.fail(command, "wrong numeric failure");
                assert_eq!(
                    cx.validate_read_root_context(root),
                    Ok(()),
                    "numeric completion must not erase entry authority"
                );
            }
            assert_eq!(cx.validate_http_context(root), Err(Error::TimedOut));
            self.terminal_results.push(cx.fail_context(
                root,
                VenueError::new("native read deadline").kind(ErrorKind::Timeout),
            ));
        }
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
fn numbered_command(id: CommandId) -> Command {
    Command::FetchCandles {
        command: id,
        symbol: "BTCUSD".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    }
}
fn native_timer(e: &Engine<NativeRead>, command: CommandId) -> TimerId {
    *e.venue
        .timers
        .iter()
        .find(|(_, id)| **id == command)
        .unwrap()
        .0
}
#[test]
fn retained_entry_native_timer_completes_after_unrelated_expired_handle_cleanup() {
    let mut e = crate::test_support::seeded_engine(NativeRead::default());
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(2., Event::Command(numbered_command(7)));
    let timer = native_timer(&e, 7);
    let staged = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(e.venue.roots[&7].deadline(), 5002.);
    assert!(staged.iter().any(
        |a| matches!(a, Action::SetTimer { timer: id, at_ms } if *id == timer && *at_ms == 5002.)
    ));
    assert!(
        !staged
            .iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
    );
    e.handle(5003., Event::Command(numbered_command(8)));
    e.drain_actions().for_each(drop);
    e.handle(5004., Event::TimerFired { timer });
    assert_eq!(
        e.venue.terminal_results,
        vec![Ok(())],
        "unrelated issuance must preserve the original completion generation"
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(terminal_count(&actions), 1);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.kind == Some(ErrorKind::Timeout))));
}
#[test]
fn retained_entry_duplicate_after_expiry_cannot_replace_native_owner() {
    let mut e = crate::test_support::seeded_engine(NativeRead::default());
    e.start(0.);
    e.handle(2., Event::Command(numbered_command(7)));
    let timer = native_timer(&e, 7);
    let original = e.venue.roots[&7].clone();
    e.drain_actions().for_each(drop);
    e.handle(5003., Event::Command(numbered_command(7)));
    assert_eq!(
        e.venue.calls, 1,
        "expired entry remains owned until native terminal cleanup"
    );
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::SetTimer { .. } | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
    e.handle(5004., Event::TimerFired { timer });
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
    e.handle(5005., Event::Command(numbered_command(7)));
    let replacement = e.venue.roots[&7].clone();
    assert_ne!(original.owner(), replacement.owner());
    e.drain_actions().for_each(drop);
    let mut cx = Cx {
        now: 5006.,
        store: &mut e.store,
        io: &mut e.io,
    };
    assert_eq!(
        cx.fail_context(&original, "stale timer"),
        Err(Error::Revoked)
    );
    assert_eq!(cx.validate_read_root_context(&replacement), Ok(()));
    e.handle(5006., Event::TimerFired { timer });
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 0);
}
#[test]
fn retained_entry_generic_same_command_cannot_remint_after_expiry() {
    let mut e = crate::test_support::seeded_engine(NativeRead {
        generic_refusal: true,
        ..Default::default()
    });
    e.start(0.);
    e.handle(2., Event::Command(numbered_command(7)));
    e.drain_actions().for_each(drop);
    e.handle(5003., Event::Command(numbered_command(8)));
    let timer = native_timer(&e, 7);
    e.handle(5004., Event::TimerFired { timer });
    assert_eq!(e.venue.terminal_results, vec![Ok(())]);
}
#[test]
fn retained_entry_capacity_does_not_evict_expired_callers_and_terminal_frees_slot() {
    let mut e = crate::test_support::seeded_engine(NativeRead::default());
    e.start(0.);
    for command in 100..356 {
        e.handle(2., Event::Command(numbered_command(command)));
    }
    assert_eq!(e.venue.roots.len(), 256);
    e.drain_actions().for_each(drop);
    e.handle(5003., Event::Command(numbered_command(400)));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 400, error }) if error.kind == Some(ErrorKind::Internal))));
    assert!(!e.venue.roots.contains_key(&400));
    let timer = native_timer(&e, 100);
    e.handle(5004., Event::TimerFired { timer });
    assert_eq!(e.venue.terminal_results, vec![Ok(())]);
    e.drain_actions().for_each(drop);
    e.handle(5005., Event::Command(numbered_command(401)));
    assert!(e.venue.roots.contains_key(&401));
    let cx = Cx {
        now: 5006.,
        store: &mut e.store,
        io: &mut e.io,
    };
    for command in 101..356 {
        assert_eq!(
            cx.validate_read_root_context(&e.venue.roots[&command]),
            Ok(())
        );
        assert_eq!(
            cx.validate_http_context(&e.venue.roots[&command]),
            Err(Error::TimedOut)
        );
    }
}

#[test]
fn retained_entry_native_timer_denies_numeric_finalization_before_typed_completion() {
    let mut e = crate::test_support::seeded_engine(NativeRead {
        numeric_refusal: true,
        ..Default::default()
    });
    e.start(0.);
    e.handle(2., Event::Command(numbered_command(7)));
    let timer = native_timer(&e, 7);
    e.drain_actions().for_each(drop);
    e.handle(5003., Event::TimerFired { timer });
    assert_eq!(e.venue.terminal_results, vec![Ok(())]);
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn retained_entry_generic_sibling_cannot_extend_original_root() {
    let mut e = crate::test_support::seeded_engine(NativeRead {
        extension_refusal: true,
        ..Default::default()
    });
    e.start(0.);
    e.handle(2., Event::Command(numbered_command(7)));
    let timer = native_timer(&e, 7);
    {
        let mut cx = Cx {
            now: 3.,
            store: &mut e.store,
            io: &mut e.io,
        };
        let mut policy = read_policy();
        policy.timeout_ms = 9000;
        assert_eq!(
            cx.http_context(Scope::Command(7), policy),
            Err(Error::Invalid)
        );
        assert_eq!(cx.validate_read_root_context(&e.venue.roots[&7]), Ok(()));
    }
    e.drain_actions().for_each(drop);
    e.handle(5003., Event::Command(numbered_command(8)));
    e.handle(5004., Event::TimerFired { timer });
    assert_eq!(e.venue.terminal_results, vec![Ok(())]);
}
