//! Caller command wait carried from the host envelope into a genuine command
//! entry (docs/plans/RUST_COMMAND_TIMEOUT.md).
use super::*;
use crate::command::{HostInput, decode_host_input};
use crate::request_context::{Operation, Policy, RetryPolicy};

/// Parent: no wait reaches the engine, so these mirror the planned API and
/// every adapter reports the missing capability.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Wait {
    Remaining(u32),
    Expired,
}

fn wait<P>(cx: &Cx<'_, P>) -> Option<Wait> {
    let _ = cx;
    None
}

/// Decode the envelope's wait: `Ok(None)` when absent.
fn decoded_wait(json: &str) -> Result<Option<u32>, ()> {
    decode_host_input(json).map(|_| None).map_err(|_| ())
}

/// Deliver one host envelope the way the WASM host does.
fn deliver(engine: &mut Engine<Probe>, now: f64, json: &str) {
    match decode_host_input(json).expect("valid envelope") {
        HostInput::Command(command) => engine.handle(now, Event::Command(command)),
        _ => panic!("command envelope"),
    }
}

/// `handle_command` with an explicit wait; `false` when refused undispatched.
fn handle_with(engine: &mut Engine<Probe>, now: f64, command: Command, wait: Option<u32>) -> bool {
    let _ = wait;
    engine.handle(now, Event::Command(command));
    true
}

/// The command's live caller deadline, keyed by its receipt.
fn deadline(engine: &Engine<Probe>, command: CommandId) -> Option<f64> {
    let _ = (engine, command);
    None
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
}

impl Venue for Probe {
    type Pending = ();
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.borrowed.push(wait(cx));
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
            // Fresh entry, before admission: typed Timeout with zero effects.
            cx.fail(
                id,
                VenueError::new("Command wait expired")
                    .kind(crate::ErrorKind::Timeout)
                    .outcome(crate::Outcome::NotSent),
            );
            return;
        }
        match command {
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
        deliver(&mut e, 0., r#"{"op":"placeOrders","command":1,"orders":[]}"#);
        deliver(&mut e, 0., &cancel(2, "7", wait));
        assert!(observed(&e, 2).is_empty(), "held until the order is stored");
        e.handle(
            5_000.,
            Event::WsMessage {
                socket: SocketId(1),
                text: "7",
            },
        );
        assert_eq!(observed(&e, 2), [Some(Wait::Remaining(left))], "wait {wait:?}");
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
    for (release_at, label) in [(1_000., "at the deadline"), (999.5, "under 1 ms before")] {
        let mut e = engine(Probe::default());
        deliver(&mut e, 0., r#"{"op":"placeOrders","command":1,"orders":[]}"#);
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
        assert!(actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: 2, error })
                if error.kind == Some(crate::ErrorKind::Timeout))));
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
    assert_eq!(observed(&e, 21), [Some(Wait::Remaining(300_000))], "duplicate refused");
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
    assert_eq!(observed(&e, 11).last(), Some(&Some(Wait::Remaining(120_000))));
    assert_eq!(observed(&e, 12).last(), Some(&Some(Wait::Remaining(120_000))));
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
        (f64::MAX, Some(1_000)),
    ] {
        let mut e = engine(Probe::default());
        let command = Command::CancelOrders {
            command: 5,
            ids: vec!["x".into()],
        };
        assert!(!handle_with(&mut e, now, command, wait), "now {now} wait {wait:?}");
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
