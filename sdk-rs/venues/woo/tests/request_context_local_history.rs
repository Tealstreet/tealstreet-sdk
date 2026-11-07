//! Synchronous native refusals borrow genuine initial Read custody.
mod support;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Query, VenueError};
use support::TestEngine;

const NOW: f64 = 1_710_000_000_000.;
const LOOKUP: &str = "WOO remote client order lookup is unsupported";
const INPUT: &str = "Invalid WOO history symbol, range or limit";
const RANGE: &str = "Invalid WOO history range";

fn engine() -> TestEngine {
    let (mut e, _) = support::started();
    e.drain_actions().for_each(drop);
    e.take_frame(NOW);
    e
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "UNKNOWN".into(),
        client_order_id: "bad id".into(),
    }
}
fn fills(from_ms: Option<f64>, to_ms: Option<f64>, limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms,
        to_ms,
        limit,
    }
}
fn query(e: &mut TestEngine, now: f64, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command: 77, query }));
    e.drain_actions().collect()
}
fn failure(actions: &[Action]) -> &VenueError {
    assert_eq!(actions.len(), 1, "only a terminal, without I/O or timers");
    let Action::Notice(Notice::CommandFailed { command: 77, error }) = &actions[0] else {
        panic!("expected correlated failure, got {actions:?}")
    };
    error
}
fn native(actions: &[Action], text: &str) {
    assert_eq!(failure(actions), &VenueError::new(text));
}
fn typed_clock(actions: &[Action]) {
    let error = failure(actions);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, None);
}

#[test]
fn original_unsupported_lookup_invalid_clock_requires_typed_entry_then_reuses_id() {
    let mut e = engine();
    for clock in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        typed_clock(&query(&mut e, clock, lookup()));
        native(&query(&mut e, NOW, lookup()), LOOKUP);
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn original_grouped_validation_invalid_clock_requires_typed_entry_then_reuses_id() {
    let mut e = engine();
    for clock in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        typed_clock(&query(&mut e, clock, fills(None, None, Some(0))));
        native(&query(&mut e, NOW, fills(None, None, Some(0))), INPUT);
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn original_derived_range_invalid_clock_requires_typed_entry_then_reuses_id() {
    let mut e = engine();
    typed_clock(&query(
        &mut e,
        f64::NEG_INFINITY,
        fills(Some(NOW + 1.), None, None),
    ));
    native(
        &query(&mut e, NOW, fills(Some(NOW + 1.), None, None)),
        RANGE,
    );
    assert!(e.take_frame(NOW).is_none());
}
#[test]
fn finite_native_refusals_keep_precedence_metadata_and_no_side_effects() {
    let mut e = engine();
    native(&query(&mut e, NOW, lookup()), LOOKUP);
    for q in [
        fills(Some(f64::NAN), None, None),
        fills(None, Some(f64::INFINITY), None),
        fills(Some(-1.), None, None),
        fills(Some(NOW), Some(NOW - 1.), None),
        fills(None, None, Some(0)),
        fills(None, None, Some(100_001)),
        Query::PublicTrades {
            symbol: "MISSING".into(),
            limit: Some(1),
        },
        Query::OrderHistory {
            symbol: Some("woo:BTCUSDT.P".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    ] {
        native(&query(&mut e, NOW, q), INPUT);
        assert!(e.take_frame(NOW).is_none());
    }
    native(
        &query(&mut e, NOW, fills(Some(NOW + 1.), None, None)),
        RANGE,
    );
}
#[test]
fn repeated_local_settlement_does_not_leak_custody_or_change_the_store() {
    let mut e = engine();
    for _ in 0..300 {
        native(&query(&mut e, NOW, lookup()), LOOKUP);
    }
    assert!(e.take_frame(NOW).is_none());
}
#[test]
fn valid_private_history_aliases_keep_native_credential_refusal() {
    let mut e = engine();
    for alias in ["woo:btcusdt.p", "BTC/USDT", "BTC_USDT_PERP"] {
        let q = Query::Fills {
            symbol: Some(alias.into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        };
        native(
            &query(&mut e, NOW, q),
            "WOO private requests require credentials",
        );
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn valid_public_history_keeps_native_unsigned_get_without_managed_preparation() {
    let mut e = engine();
    let actions = query(
        &mut e,
        NOW,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(2),
        },
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_)
            | Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
    let request = actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    assert!(
        request
            .url
            .contains("/v1/public/market_trades?symbol=PERP_BTC_USDT&limit=2"),
        "{}",
        request.url
    );
    assert!(e.take_frame(NOW).is_none());
}
