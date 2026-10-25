//! Caller command wait carried from the host envelope into a genuine command
//! entry (docs/plans/RUST_COMMAND_TIMEOUT.md).
use super::*;
use crate::command::{HostInput, decode_host_input};
use crate::request_context::{Operation, Policy, RetryPolicy};

use crate::CommandWait as Wait;

fn wait<P>(cx: &Cx<'_, P>) -> Option<Wait> {
    cx.command_wait()
}

/// Decode the envelope's wait: `Ok(None)` when absent.
fn decoded_wait(json: &str) -> Result<Option<u32>, ()> {
    match decode_host_input(json) {
        Ok(HostInput::Command { timeout_ms, .. }) => Ok(timeout_ms),
        _ => Err(()),
    }
}

/// Deliver one host envelope the way the WASM host does.
fn deliver(engine: &mut Engine<Probe>, now: f64, json: &str) {
    match decode_host_input(json).expect("valid envelope") {
        HostInput::Command {
            command,
            timeout_ms,
        } => engine
            .handle_command(now, command, timeout_ms)
            .expect("valid wait"),
        _ => panic!("command envelope"),
    }
}

/// `handle_command` with an explicit wait; `false` when refused undispatched.
fn handle_with(engine: &mut Engine<Probe>, now: f64, command: Command, wait: Option<u32>) -> bool {
    engine.handle_command(now, command, wait).is_ok()
}

/// The command's live caller deadline, keyed by its receipt.
fn deadline(engine: &Engine<Probe>, command: CommandId) -> Option<f64> {
    engine.io.core.command_waits.deadline(command)
}

#[derive(Default)]
struct Probe {
    /// (command, wait) observed in each command entry.
    entries: Vec<(CommandId, Option<Wait>)>,
    /// Waits observed in borrowed callbacks.
    borrowed: Vec<Option<Wait>>,
    /// Admit command 11 with an invalid policy: a typed initial rejection.
    reject: bool,
    /// Keep command 21's admitted root live (no terminal).
    hold_live: bool,
    timer: Option<TimerId>,
    /// Finish an expired entry as a read (no outcome) instead of a mutation.
    expire_as_read: bool,
    /// Leave cancels and placements numbered 1000 and up awaiting a reply.
    ignore_high: bool,
    /// Opt into order locks, so cancel-all defers behind a placement.
    locks: bool,
    /// Cancel 2's entry fails this other command, outside that one's entry.
    fail_from_2: Option<CommandId>,
    /// An expired entry refuses locally through refuse_command instead.
    refuse_expired: bool,
}

impl Venue for Probe {
    type Pending = ();
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }
    fn guard_pending_placements(&self) -> bool {
        self.locks
    }
    fn guard_order_mutations(&self) -> bool {
        self.locks
    }
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.borrowed.push(wait(cx));
        // A late legacy terminal from a callback, outside any command entry.
        if let Some(command) = text.strip_prefix("fail:") {
            cx.fail(command.parse().expect("command id"), "late");
            return;
        }
        let mut order = crate::model::Order {
            id: text.into(),
            parent_id: None,
            status: crate::model::OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: crate::model::OrderType::Limit,
            side: crate::model::OrderSide::Buy,
            price: 1.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        order.last_update = cx.now;
        cx.store.upsert_order(order);
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        self.borrowed.push(wait(cx));
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
        if Some(timer) == self.timer {
            self.borrowed.push(wait(cx));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        let id = command.completion_id().unwrap_or_default();
        let observed = wait(cx);
        self.entries.push((id, observed));
        if observed == Some(Wait::Expired) {
            // Fresh entry, before admission: the shared typed expiry terminal.
            let operation = if self.expire_as_read {
                Operation::Read
            } else {
                Operation::Mutation
            };
            if self.refuse_expired {
                cx.refuse_command(operation, "venue local refusal");
                return;
            }
            assert_eq!(cx.reject_expired_command(operation), Ok(()));
            return;
        }
        assert_eq!(
            cx.reject_expired_command(Operation::Read),
            Err(crate::request_context::CommandEntryError::Revoked),
            "only an expired wait may be rejected as expired"
        );
        if let Command::CancelOrders { command: 2, .. } = &command
            && let Some(other) = self.fail_from_2.take()
        {
            cx.fail(other, "late");
        }
        match command {
            Command::PlaceOrders { command, .. } if self.ignore_high && command >= 1000 => {}
            Command::PlaceOrders { command, .. } => cx.done(command, r#"["7"]"#.into()),
            Command::CancelOrders { command: 11, .. } if self.reject => {
                let _ = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 0,
                    pacing: None,
                    retry: RetryPolicy::Never,
                });
            }
            Command::CancelOrders { command, .. } if self.ignore_high && command >= 1000 => {}
            Command::CancelOrders { command: 21, .. } if self.hold_live => {
                let _ = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 600_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                });
            }
            Command::CancelOrders { command, .. } => {
                if self.timer.is_none() {
                    self.timer = Some(cx.set_timer(1.0));
                }
                cx.done(command, "null".into());
            }
            _ => {}
        }
    }
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine(probe: Probe) -> Engine<Probe> {
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    engine
}

fn cancel(command: CommandId, id: &str, wait: Option<u32>) -> String {
    match wait {
        Some(ms) => format!(
            r#"{{"op":"cancelOrders","command":{command},"ids":["{id}"],"timeoutMs":{ms}}}"#
        ),
        None => format!(r#"{{"op":"cancelOrders","command":{command},"ids":["{id}"]}}"#),
    }
}

fn observed(engine: &Engine<Probe>, command: CommandId) -> Vec<Option<Wait>> {
    engine
        .venue
        .entries
        .iter()
        .filter(|(id, _)| *id == command)
        .map(|(_, wait)| *wait)
        .collect()
}

#[test]
fn the_envelope_wait_reaches_the_fresh_entry_and_absence_is_the_sdk_default() {
    let mut e = engine(Probe::default());
    deliver(&mut e, 0., &cancel(5, "x", Some(300_000)));
    deliver(&mut e, 0., &cancel(6, "y", None));
    assert_eq!(observed(&e, 5), [Some(Wait::Remaining(300_000))]);
    assert_eq!(observed(&e, 6), [Some(Wait::Remaining(120_000))]);
}

#[test]
fn a_deferred_command_sees_the_time_left_from_its_receipt() {
    for (wait, left) in [(Some(300_000), 295_000), (None, 115_000)] {
        let mut e = engine(Probe::default());
        deliver(
            &mut e,
            0.,
            r#"{"op":"placeOrders","command":1,"orders":[]}"#,
        );
        deliver(&mut e, 0., &cancel(2, "7", wait));
        assert!(observed(&e, 2).is_empty(), "held until the order is stored");
        e.handle(
            5_000.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert_eq!(
            observed(&e, 2),
            [Some(Wait::Remaining(left))],
            "wait {wait:?}"
        );
    }
}

#[test]
fn borrowed_callbacks_and_other_entries_see_no_wait() {
    let mut e = engine(Probe::default());
    deliver(&mut e, 0., &cancel(5, "x", Some(300_000)));
    let timer = e.venue.timer.expect("probe timer");
    e.handle(1., Event::TimerFired { timer });
    e.handle(
        2.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "z",
        },
    );
    assert_eq!(e.venue.borrowed, [None, None]);
    assert_eq!(observed(&e, 5), [Some(Wait::Remaining(300_000))]);
}

#[test]
fn an_expired_fresh_entry_finishes_typed_with_zero_effects() {
    for (release_at, label, read) in [
        (1_000., "at the deadline", false),
        (999.5, "under 1 ms before", false),
        (1_000., "a read at the deadline", true),
    ] {
        let mut e = engine(Probe {
            expire_as_read: read,
            ..Default::default()
        });
        deliver(
            &mut e,
            0.,
            r#"{"op":"placeOrders","command":1,"orders":[]}"#,
        );
        deliver(&mut e, 0., &cancel(2, "7", Some(1_000)));
        e.drain_actions().for_each(drop);
        e.handle(
            release_at,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert_eq!(observed(&e, 2), [Some(Wait::Expired)], "{label}");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                Action::Http(_) | Action::ManagedHttp(_) | Action::SetTimer { .. }
            )),
            "{label}"
        );
        let outcome = if read {
            None
        } else {
            Some(crate::Outcome::NotSent)
        };
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a,
                    Action::Notice(Notice::CommandFailed { command: 2, error })
                        if error.kind == Some(crate::ErrorKind::Timeout)
                            && error.outcome == outcome))
                .count(),
            1,
            "{label}"
        );
        assert_eq!(
            deadline(&e, 2),
            None,
            "{label}: the terminal cleared the stamp"
        );
    }
}

#[test]
fn a_duplicate_live_id_neither_replaces_nor_clears_the_first_stamp() {
    let mut e = engine(Probe {
        hold_live: true,
        ..Default::default()
    });
    deliver(&mut e, 0., &cancel(21, "x", Some(300_000)));
    deliver(&mut e, 10., &cancel(21, "x", Some(5_000)));
    assert_eq!(
        observed(&e, 21),
        [Some(Wait::Remaining(300_000))],
        "duplicate refused"
    );
    assert_eq!(
        deadline(&e, 21),
        Some(300_000.),
        "the first receipt keeps its deadline"
    );
}

#[test]
fn a_reused_id_after_either_terminal_gets_a_fresh_stamp() {
    let mut e = engine(Probe {
        reject: true,
        ..Default::default()
    });
    deliver(&mut e, 0., &cancel(11, "x", Some(1_000)));
    deliver(&mut e, 0., &cancel(12, "y", Some(1_000)));
    assert_eq!(deadline(&e, 11), None, "typed rejection cleared it");
    assert_eq!(deadline(&e, 12), None, "terminal cleared it");
    e.venue.reject = false;
    deliver(&mut e, 5_000., &cancel(11, "x", None));
    deliver(&mut e, 5_000., &cancel(12, "y", None));
    assert_eq!(
        observed(&e, 11).last(),
        Some(&Some(Wait::Remaining(120_000)))
    );
    assert_eq!(
        observed(&e, 12).last(),
        Some(&Some(Wait::Remaining(120_000)))
    );
}

#[test]
fn decode_distinguishes_absent_null_and_out_of_range_waits() {
    let base = r#"{"op":"cancelOrders","command":5,"ids":["x"]"#;
    assert_eq!(decoded_wait(&format!("{base}}}")), Ok(None));
    assert_eq!(
        decoded_wait(&format!(r#"{base},"timeoutMs":2147483647}}"#)),
        Ok(Some(2_147_483_647))
    );
    for bad in ["null", "0", "-1", "1.5", "\"1\"", "2147483648"] {
        assert_eq!(
            decoded_wait(&format!(r#"{base},"timeoutMs":{bad}}}"#)),
            Err(()),
            "timeoutMs {bad}"
        );
    }
}

#[test]
fn handle_command_refuses_bad_clocks_and_waits_without_dispatch() {
    for (now, wait) in [
        (f64::NAN, Some(1_000)),
        (0., Some(0)),
        (0., Some(2_147_483_648)),
        (0., Some(u32::MAX)),
        (f64::MAX, Some(1_000)),
    ] {
        let mut e = engine(Probe::default());
        let command = Command::CancelOrders {
            command: 5,
            ids: vec!["x".into()],
        };
        assert!(
            !handle_with(&mut e, now, command, wait),
            "now {now} wait {wait:?}"
        );
        assert!(observed(&e, 5).is_empty(), "now {now} wait {wait:?}");
    }
}

#[test]
fn a_default_stamp_that_cannot_advance_dispatches_unstamped() {
    let mut e = engine(Probe::default());
    // now + 120000 rounds back to now: no finite advancing default deadline.
    e.handle(
        f64::MAX,
        Event::Command(Command::CancelOrders {
            command: 5,
            ids: vec!["x".into()],
        }),
    );
    assert_eq!(observed(&e, 5), [None]);
}

#[test]
fn an_expired_deferred_wait_survives_many_live_waits() {
    let mut e = engine(Probe {
        ignore_high: true,
        ..Default::default()
    });
    deliver(
        &mut e,
        0.,
        r#"{"op":"placeOrders","command":1,"orders":[]}"#,
    );
    deliver(&mut e, 0., &cancel(2, "7", Some(1_000)));
    for command in 1_000..5_200 {
        deliver(&mut e, 0., &cancel(command, "x", Some(1_000)));
    }
    deliver(&mut e, 2_000., &cancel(6_000, "x", Some(1_000)));
    assert_eq!(deadline(&e, 2), Some(1_000.), "only a terminal removes it");
    e.handle(
        3_000.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "7",
        },
    );
    assert_eq!(observed(&e, 2), [Some(Wait::Expired)]);
}

#[test]
fn an_exhausted_receipt_sequence_never_wraps() {
    let mut e = engine(Probe::default());
    e.io.core.command_waits.exhaust_receipts();
    deliver(&mut e, 0., &cancel(5, "x", Some(1_000)));
    assert_eq!(observed(&e, 5), [None], "no receipt left: unstamped");
    assert_eq!(deadline(&e, 5), None);
}

#[test]
fn a_duplicate_refused_before_its_entry_keeps_the_live_stamp() {
    let duplicates = [
        Command::FetchAccount { command: 1_000 },
        Command::AmendOrderWithSymbol {
            command: 1_000,
            id: "x".into(),
            symbol: "BTC".into(),
            price: Some(1.),
            amount: None,
        },
    ];
    for duplicate in duplicates {
        let mut e = engine(Probe {
            ignore_high: true,
            ..Default::default()
        });
        // A legacy command left awaiting its reply, with no Context.
        deliver(&mut e, 0., &cancel(1_000, "x", Some(300_000)));
        assert!(handle_with(&mut e, 10., duplicate.clone(), Some(5_000)));
        assert_eq!(
            deadline(&e, 1_000),
            Some(300_000.),
            "{duplicate:?} refused before its entry"
        );
    }
}

#[test]
fn a_held_command_keeps_its_own_receipt_past_a_same_id_arrival() {
    let mut e = engine(Probe::default());
    deliver(
        &mut e,
        0.,
        r#"{"op":"placeOrders","command":1,"orders":[]}"#,
    );
    deliver(&mut e, 0., &cancel(2, "7", Some(1_000)));
    assert!(observed(&e, 2).is_empty(), "held until the order is stored");
    // The duplicate's arrival releases the original at its hold cap.
    deliver(&mut e, 10_000., &cancel(2, "7", Some(50_000)));
    assert_eq!(
        observed(&e, 2),
        [Some(Wait::Expired), None],
        "the original reads its own receipt; the duplicate has none"
    );
    assert_eq!(deadline(&e, 2), None, "the original's terminal cleared it");
}

#[test]
fn a_held_original_keeps_its_wait_past_a_same_id_late_terminal() {
    let mut e = engine(Probe::default());
    deliver(
        &mut e,
        0.,
        r#"{"op":"placeOrders","command":1,"orders":[]}"#,
    );
    deliver(&mut e, 0., &cancel(2_000, "7", Some(300_000)));
    // A same-id duplicate left unanswered, then failed from a callback.
    assert!(handle_with(
        &mut e,
        1.,
        Command::FetchStpMode { command: 2_000 },
        None
    ));
    e.handle(
        2.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "fail:2000",
        },
    );
    assert_eq!(deadline(&e, 2_000), Some(300_000.), "held receipt kept");
    e.handle(
        3.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "7",
        },
    );
    assert_eq!(
        observed(&e, 2_000),
        [None, Some(Wait::Remaining(299_997))],
        "the duplicate had no wait; the original keeps its own"
    );
}

#[test]
fn waits_a_venue_never_answers_stay_bounded() {
    let mut e = engine(Probe {
        ignore_high: true,
        ..Default::default()
    });
    for command in 1_000..6_000 {
        deliver(&mut e, 0., &cancel(command, "x", Some(1_000)));
    }
    deliver(&mut e, 2_000., &cancel(7_000, "x", Some(1_000)));
    assert_eq!(
        e.io.core.command_waits.len(),
        1,
        "expired waits no command still carries are dropped"
    );
}

#[cfg(feature = "order-locks")]
#[test]
fn a_deferred_cancel_all_keeps_its_own_receipt_through_the_lock() {
    let mut e = engine(Probe {
        ignore_high: true,
        locks: true,
        ..Default::default()
    });
    deliver(
        &mut e,
        0.,
        r#"{"op":"placeOrders","command":1000,"orders":[]}"#,
    );
    deliver(
        &mut e,
        0.,
        r#"{"op":"cancelSymbolOrders","command":2,"symbol":"","timeoutMs":1000}"#,
    );
    assert!(observed(&e, 2).is_empty(), "deferred behind the placement");
    deliver(
        &mut e,
        1.,
        r#"{"op":"fetchAccount","command":2,"timeoutMs":50000}"#,
    );
    assert_eq!(deadline(&e, 2), Some(1_000.), "the duplicate kept no stamp");
    // The placement ends; the deferred cancel-all re-enters past its wait.
    e.handle(
        2_000.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "fail:1000",
        },
    );
    assert_eq!(observed(&e, 2), [Some(Wait::Expired)]);
    assert_eq!(deadline(&e, 2), None, "its own terminal cleared it");
}

#[test]
fn a_released_batch_keeps_each_later_receipt_until_its_entry() {
    let mut e = engine(Probe {
        fail_from_2: Some(3),
        ..Default::default()
    });
    deliver(
        &mut e,
        0.,
        r#"{"op":"placeOrders","command":1,"orders":[]}"#,
    );
    deliver(&mut e, 0., &cancel(2, "7", Some(300_000)));
    deliver(&mut e, 0., &cancel(3, "7", Some(300_000)));
    // One release dispatches both; cancel 2's entry fails 3 outside 3's entry.
    e.handle(
        5_000.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "7",
        },
    );
    assert_eq!(observed(&e, 3), [Some(Wait::Remaining(295_000))]);
}

/// A local refusal in an entry whose caller wait already expired: admission of
/// an expired Fresh entry succeeds, so refuse_command settles it exactly once
/// with the venue's own refusal, the same as the per-site blocks it replaced.
/// No I/O, and no Revoked assertion.
#[test]
fn a_local_refusal_in_an_expired_entry_settles_once_without_io() {
    for read in [false, true] {
        let mut e = engine(Probe {
            expire_as_read: read,
            refuse_expired: true,
            ..Default::default()
        });
        deliver(
            &mut e,
            0.,
            r#"{"op":"placeOrders","command":1,"orders":[]}"#,
        );
        deliver(&mut e, 0., &cancel(2, "7", Some(1_000)));
        e.drain_actions().for_each(drop);
        e.handle(
            1_000.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert_eq!(observed(&e, 2), [Some(Wait::Expired)]);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
        )));
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 2, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "exactly one terminal (read: {read})");
        assert_eq!(failures[0].message, "venue local refusal");
        let outcome = if read {
            None
        } else {
            Some(crate::Outcome::NotSent)
        };
        assert_eq!(failures[0].outcome, outcome, "read: {read}");
    }
}
