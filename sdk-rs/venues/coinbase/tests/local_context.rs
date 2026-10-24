mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Outcome, Query};
use safe_cex_venue_coinbase::{Coinbase, Options};

fn errors(actions: &[Action]) -> Vec<&safe_cex_engine::VenueError> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}
fn assert_invalid_entry(e: &mut support::Driver, command: Command) {
    e.handle(f64::NAN, Event::Command(command));
    let a: Vec<_> = e.drain_actions().collect();
    let failures = errors(&a);
    assert_eq!(failures.len(), 1, "one genuine-entry refusal: {a:?}");
    assert_eq!(failures[0].kind, Some(ErrorKind::InvalidInput));
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. })
    )));
}
#[test]
fn local_portfolio_refusal_requires_valid_entry() {
    let (mut e, _) = support::ready();
    assert_invalid_entry(&mut e, Command::FetchAccount { command: 7 });
}
#[test]
fn cached_leverage_success_requires_valid_entry() {
    let (mut e, _) = support::ready();
    e.store_mut().set_settings([(
        0,
        safe_cex_engine::store::MarketSetting {
            leverage: 3.,
            ..Default::default()
        },
    )]);
    let symbol = e.store().markets()[0].symbol.clone();
    assert_invalid_entry(&mut e, Command::FetchLeverage { command: 7, symbol });
}
#[test]
fn unsupported_mutation_refusal_requires_valid_entry() {
    let (mut e, _) = support::ready();
    assert_invalid_entry(
        &mut e,
        Command::SetLeverage {
            command: 7,
            symbol: "BTC-PERP-INTX".into(),
            leverage: 2.,
            side: None,
        },
    );
}
#[test]
fn initial_history_validation_requires_valid_entry() {
    let (mut e, _) = support::ready();
    assert_invalid_entry(
        &mut e,
        Command::Query {
            command: 7,
            query: Query::PublicTrades {
                symbol: "BTC-PERP-INTX".into(),
                limit: Some(0),
            },
        },
    );
}
fn full_startup_queue() -> support::Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Default::default()
        })
        .unwrap(),
    );
    for command in 1..=256 {
        e.handle(
            1000.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTC-PERP-INTX".into(),
                interval: Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        assert!(
            e.drain_actions().next().is_none(),
            "native queue retains the pending command"
        );
    }
    e
}
#[test]
fn startup_capacity_refusal_requires_valid_entry() {
    let mut e = full_startup_queue();
    assert_invalid_entry(
        &mut e,
        Command::FetchCandles {
            command: 1000,
            symbol: "BTC-PERP-INTX".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        },
    );
}
#[test]
fn finite_local_results_and_refusals_keep_native_contract() {
    let (mut e, _) = support::ready();
    e.handle(1000., Event::Command(Command::FetchAccount { command: 7 }));
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(&a[..], [Action::Notice(Notice::CommandFailed { error, .. })] if error.message == "Coinbase private portfolio ID is unavailable" && error.outcome.is_none())
    );
    e.store_mut().set_settings([(
        0,
        safe_cex_engine::store::MarketSetting {
            leverage: 3.,
            ..Default::default()
        },
    )]);
    let symbol = e.store().markets()[0].symbol.clone();
    e.handle(
        1001.,
        Event::Command(Command::FetchLeverage {
            command: 7,
            symbol: symbol.clone(),
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(&a[..], [Action::Notice(Notice::CommandDone { result, .. })] if result == r#"{"buyLeverage":3.0,"sellLeverage":3.0}"#)
    );
    e.handle(
        1002.,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol,
            leverage: 2.,
            side: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(&a[..], [Action::Notice(Notice::CommandFailed { error, .. })] if error.kind == Some(ErrorKind::Unsupported) && error.outcome == Some(Outcome::NotSent) && error.message == "Coinbase does not support account leverage, margin or position-mode mutations")
    );
}
#[test]
fn full_startup_queue_keeps_exact_failure_and_warning() {
    let mut e = full_startup_queue();
    e.handle(
        1000.,
        Event::Command(Command::FetchCandles {
            command: 1000,
            symbol: "BTC-PERP-INTX".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(errors(&a).len(), 1);
    assert_eq!(
        errors(&a)[0].message,
        "Coinbase public startup command queue is full"
    );
    assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message == "Coinbase public startup command queue is full")));
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
}
