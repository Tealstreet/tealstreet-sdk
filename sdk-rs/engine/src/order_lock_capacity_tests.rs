//! Order-lock capacity refusals (docs/plans/RUST_ORDER_LOCK_CAPACITY.md).
use super::*;
use crate::model::PlaceOrderOpts;

/// Holds every order command in flight; a FetchCandles finishes command 1.
#[derive(Default)]
struct Held {
    entries: usize,
}

impl Venue for Held {
    type Pending = ();
    fn guard_pending_placements(&self) -> bool {
        true
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        if let Command::FetchCandles { command, .. } = command {
            cx.done(1, "[]".into());
            cx.done(command, "{}".into());
            return;
        }
        self.entries += 1;
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

const CAPACITY_MESSAGE: &str =
    "Order command capacity reached: 64 order commands are in flight; retry after one finishes";
const CONFLICT_MESSAGE: &str = "Order amendment or cancellation is already in flight";

fn order(symbol: &str) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": symbol, "type": "limit", "side": "buy", "amount": 1.0, "price": 1.0,
    }))
    .expect("placement fixture")
}

fn place(command: CommandId, symbol: &str) -> Command {
    Command::PlaceOrders {
        command,
        orders: vec![order(symbol)],
    }
}

fn started() -> Engine<Held> {
    let mut e = crate::test_support::seeded_engine(Held::default());
    e.start(0.);
    e.drain_actions().for_each(drop);
    e
}

/// Hold `count` placements on distinct symbols, commands 1..=count.
fn fill(e: &mut Engine<Held>, count: CommandId) {
    let before = e.venue.entries;
    for command in 1..=count {
        e.handle(1., Event::Command(place(command, &format!("S{command}"))));
    }
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. }))),
        "{actions:?}"
    );
    assert_eq!(e.venue.entries, before + count as usize);
}

fn failures(actions: &[Action], command: CommandId) -> Vec<&VenueError> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect()
}

fn nothing_sent(actions: &[Action]) -> bool {
    !actions.iter().any(|a| {
        matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
        )
    })
}

#[test]
fn original_a_new_placement_at_the_scope_cap_is_not_sent_with_a_capacity_message() {
    let mut e = started();
    fill(&mut e, 64);
    e.handle(2., Event::Command(place(65, "S65")));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let errors = failures(&actions, 65);
    assert_eq!(errors.len(), 1, "{actions:?}");
    assert_eq!(errors[0].legacy_text(), CAPACITY_MESSAGE);
    assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
    assert_eq!(errors[0].outcome, Some(crate::Outcome::NotSent));
    assert!(nothing_sent(&actions));
    assert_eq!(
        e.venue.entries, 64,
        "the venue never sees the refused placement"
    );
    assert!((1..=64).all(|id| e.io.order_locks.contains(id)));
    assert!(!e.io.order_locks.contains(65));
}

#[test]
fn control_capacity_frees_when_a_held_placement_finishes() {
    let mut e = started();
    fill(&mut e, 64);
    e.handle(
        2.,
        Event::Command(Command::FetchCandles {
            command: 900,
            symbol: "S1".into(),
            interval: Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    e.drain_actions().for_each(drop);
    assert!(!e.io.order_locks.contains(1));
    e.handle(3., Event::Command(place(65, "S65")));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(failures(&actions, 65).is_empty(), "{actions:?}");
    assert_eq!(e.venue.entries, 65);
}

#[test]
fn control_a_placement_behind_an_in_flight_cancel_all_keeps_the_conflict_refusal() {
    let mut e = started();
    e.handle(
        1.,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTC".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(2., Event::Command(place(2, "BTC")));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let errors = failures(&actions, 2);
    assert_eq!(errors.len(), 1, "{actions:?}");
    assert_eq!(errors[0].legacy_text(), CONFLICT_MESSAGE);
    assert_eq!(errors[0].outcome, None);
}

#[test]
fn control_a_conflict_at_the_cap_keeps_the_conflict_refusal() {
    let mut e = started();
    e.handle(
        1.,
        Event::Command(Command::CancelSymbolOrders {
            command: 100,
            symbol: "BTC".into(),
        }),
    );
    fill(&mut e, 63);
    e.handle(2., Event::Command(place(65, "BTC")));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let errors = failures(&actions, 65);
    assert_eq!(errors.len(), 1, "{actions:?}");
    assert_eq!(errors[0].legacy_text(), CONFLICT_MESSAGE);
    assert_eq!(errors[0].outcome, None);
}

#[test]
fn control_amend_and_cancel_at_the_cap_keep_the_existing_refusal() {
    for command in [
        Command::CancelOrders {
            command: 65,
            ids: vec!["x".into()],
        },
        Command::AmendOrder {
            command: 65,
            id: "x".into(),
            price: Some(2.),
            amount: None,
        },
    ] {
        let mut e = started();
        fill(&mut e, 64);
        e.handle(2., Event::Command(command));
        let actions = e.drain_actions().collect::<Vec<_>>();
        let errors = failures(&actions, 65);
        assert_eq!(errors.len(), 1, "{actions:?}");
        assert_eq!(errors[0].legacy_text(), CONFLICT_MESSAGE);
        assert_eq!(errors[0].outcome, None);
        assert!(nothing_sent(&actions));
    }
}
