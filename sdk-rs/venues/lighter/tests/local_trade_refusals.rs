//! Synchronous trading refusals settle only a genuine Mutation entry
//! (docs/plans/RUST_LIGHTER_LOCAL_TRADE_REFUSALS.md).
mod support;
use safe_cex_engine::model::{Interval, PlaceOrderOpts};
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Outcome, VenueError};
use safe_cex_venue_lighter::Lighter;
use serde_json::json;
use support::*;

const OWNER: u32 = 900;

fn order(value: serde_json::Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}

/// The six refusals reachable from a ready engine, each with its native text.
fn refusals() -> Vec<(&'static str, Command, Option<&'static str>)> {
    let mut caller_id =
        order(json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}));
    caller_id.client_order_id = Some("caller".into());
    let unknown_market =
        order(json!({"symbol":"NOPE","type":"limit","side":"buy","amount":0.2,"price":90}));
    vec![
        (
            "caller client order ID",
            Command::PlaceOrders {
                command: OWNER,
                orders: vec![caller_id],
            },
            Some("Lighter does not support caller client order IDs"),
        ),
        (
            "placement plan error",
            Command::PlaceOrders {
                command: OWNER,
                orders: vec![unknown_market],
            },
            Some("Market NOPE not found"),
        ),
        (
            "non-numeric cancel",
            Command::CancelOrders {
                command: OWNER,
                ids: vec!["abc".into()],
            },
            Some("Cannot cancel Lighter order: expected a numeric order index"),
        ),
        (
            "unknown cancel",
            Command::CancelOrders {
                command: OWNER,
                ids: vec!["424242".into()],
            },
            Some("Lighter order 424242 not found"),
        ),
        (
            "amend unknown order",
            Command::AmendOrder {
                command: OWNER,
                id: "424242".into(),
                price: Some(91.0),
                amount: None,
            },
            Some("Lighter order not found"),
        ),
        (
            "hedged position mode",
            Command::SetPositionMode {
                command: OWNER,
                hedged: true,
                symbol: None,
            },
            Some("Lighter supports one-way position mode only"),
        ),
    ]
}

fn run(e: &mut Engine<Lighter>, command: Command) -> Vec<Action> {
    e.handle(NOW, Event::Command(command));
    e.drain_actions().collect()
}

fn failure(actions: &[Action]) -> Vec<VenueError> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == OWNER => {
                Some(error.clone())
            }
            _ => None,
        })
        .collect()
}

fn no_io(actions: &[Action], case: &str) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
        "{case}: a local refusal sends nothing"
    );
}

/// Park genuine managed candle owners until admission refuses one.
fn fill_owner_capacity(e: &mut Engine<Lighter>) {
    for command in 1_000..1_400 {
        e.handle(
            NOW,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "ETH".into(),
                interval: Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let parked = e.drain_actions().any(|action| {
            matches!(&action, Action::ManagedHttp(http) if matches!(**http, HttpAction::Prepare { .. }))
        });
        if !parked {
            return;
        }
    }
    panic!("owner capacity was never reached");
}

#[test]
fn free_capacity_keeps_each_native_refusal_exact() {
    for (case, command, message) in refusals() {
        let (mut e, _) = ready();
        let actions = run(&mut e, command);
        no_io(&actions, case);
        let errors = failure(&actions);
        assert_eq!(errors.len(), 1, "{case}: one terminal");
        assert_eq!(errors[0].outcome, Some(Outcome::NotSent), "{case}");
        assert_ne!(
            errors[0].message, "Engine request capacity reached",
            "{case}"
        );
        if let Some(message) = message {
            assert_eq!(errors[0].message, message, "{case}");
        }
    }
}

#[test]
fn full_owner_capacity_refuses_each_at_genuine_admission() {
    let mut native = Vec::new();
    for (case, command, _) in refusals() {
        let (mut e, _) = ready();
        fill_owner_capacity(&mut e);
        let actions = run(&mut e, command);
        no_io(&actions, case);
        let errors = failure(&actions);
        assert_eq!(errors.len(), 1, "{case}: one terminal");
        assert_eq!(errors[0].outcome, Some(Outcome::NotSent), "{case}");
        if errors[0].message != "Engine request capacity reached" {
            native.push(format!("{case}: {}", errors[0].message));
        }
    }
    assert!(
        native.is_empty(),
        "each refusal needs a genuine Mutation entry: {native:#?}"
    );
}
