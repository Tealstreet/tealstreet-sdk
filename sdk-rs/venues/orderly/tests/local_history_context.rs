mod support;

use safe_cex_engine::{
    Action, Command, ErrorKind, Event, Method, Notice, Query, Response, VenueError,
};
use safe_cex_venue_orderly::{Options, Orderly};
use support::Driver;

const COMMAND: u32 = 71;

fn engine() -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.);
    let requests: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    let markets = requests
        .iter()
        .find(|r| r.url.ends_with("/v1/public/info"))
        .expect("genuine native catalogue request");
    e.handle(
        0.,
        Event::HttpResponse {
            request: markets.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let _ = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
    assert!(e.store().markets().iter().any(|m| m.symbol == "BTCUSDC"));
    e
}

fn snapshot(e: &Driver) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}",
        e.store().loaded(),
        e.store().markets(),
        e.store().tickers(),
        e.store().orders(),
        e.store().positions(),
        e.store().balance()
    )
}

fn run(e: &mut Driver, now: f64, query: Query) -> Vec<Action> {
    let before = snapshot(e);
    e.handle(
        now,
        Event::Command(Command::Query {
            command: COMMAND,
            query,
        }),
    );
    let out = e.drain_actions().collect();
    assert_eq!(snapshot(e), before, "local refusal cannot mutate Store");
    out
}

fn failure(out: &[Action]) -> &VenueError {
    assert_eq!(
        out.len(),
        1,
        "one terminal, no transport/sign/timer work: {out:?}"
    );
    match &out[0] {
        Action::Notice(Notice::CommandFailed { command, error }) => {
            assert_eq!(*command, COMMAND);
            assert_eq!(error.outcome, None, "a Read has no write outcome");
            error
        }
        _ => panic!("expected one correlated failure: {out:?}"),
    }
}

fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "BTCUSDC".into(),
        client_order_id: "fixture-client".into(),
    }
}
fn income() -> Query {
    Query::Income {
        symbol: None,
        kind: Some("COMMISSION".into()),
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn trades(symbol: &str, limit: u32) -> Query {
    Query::PublicTrades {
        symbol: symbol.into(),
        limit: Some(limit),
    }
}
fn fills(order_id: Option<&str>, from_ms: Option<f64>) -> Query {
    Query::Fills {
        symbol: None,
        order_id: order_id.map(String::from),
        from_ms,
        to_ms: None,
        limit: Some(1),
    }
}
fn native(query: Query, expected: &str) {
    let out = run(&mut engine(), 100., query);
    let error = failure(&out);
    assert_eq!(error.legacy_text(), expected);
    assert_eq!(error.kind, None, "preserve native error metadata");
}
fn unstamped(query: Query) {
    let out = run(&mut engine(), f64::NAN, query);
    assert_eq!(
        failure(&out).kind,
        Some(ErrorKind::InvalidInput),
        "unstamped entry must use the shared typed rejection before native local failure"
    );
}

#[test]
fn unstamped_lookup_uses_shared_entry_rejection() {
    unstamped(lookup());
}
#[test]
fn unstamped_income_uses_shared_entry_rejection() {
    unstamped(income());
}
#[test]
fn unstamped_validation_uses_shared_entry_rejection() {
    unstamped(trades("BTCUSDC", 0));
}

#[test]
fn native_lookup_message_and_no_io_are_preserved() {
    native(lookup(), "Orderly client-order lookup is not supported");
}
#[test]
fn native_income_message_and_no_io_are_preserved() {
    native(
        income(),
        "Orderly income supports settled funding payments only",
    );
}
#[test]
fn native_zero_limit_message_and_no_io_are_preserved() {
    native(
        trades("BTCUSDC", 0),
        "Orderly invalid history range or limit",
    );
}
#[test]
fn native_timestamp_validation_is_preserved() {
    native(fills(None, Some(0.5)), "Orderly invalid history timestamp");
}
#[test]
fn native_canonical_order_id_validation_is_preserved() {
    native(
        fills(Some("01"), None),
        "Orderly history order ID must be a canonical positive uint64",
    );
}
#[test]
fn native_missing_private_credentials_is_preserved() {
    native(
        fills(None, None),
        "Orderly private history requires credentials",
    );
}
#[test]
fn native_unknown_public_symbol_is_preserved() {
    native(trades("UNKNOWN", 1), "Orderly unknown history symbol");
}
#[test]
fn native_reversed_range_precedes_private_credentials() {
    native(
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(2.),
            to_ms: Some(1.),
            limit: Some(1),
        },
        "Orderly invalid history range or limit",
    );
}
#[test]
fn accepted_public_history_remains_unsigned_and_reachable() {
    let mut e = engine();
    e.handle(
        100.,
        Event::Command(Command::Query {
            command: COMMAND,
            query: trades("BTCUSDC", 2),
        }),
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, 100.);
    // One native request; the managed command root adds only its deadline.
    let (http, other): (Vec<_>, Vec<_>) = out.iter().partition(|a| matches!(a, Action::Http(_)));
    assert_eq!(http.len(), 1, "exact native accepted action batch: {out:?}");
    assert!(
        other.iter().all(|a| matches!(a, Action::SetTimer { .. })),
        "{out:?}"
    );
    let Action::Http(request) = http[0] else {
        panic!("native public history request: {out:?}")
    };
    assert_eq!(request.method, Method::Get);
    assert!(
        request
            .url
            .ends_with("/v1/public/market_trades?symbol=PERP_BTC_USDC&limit=2"),
        "{}",
        request.url
    );
    assert!(request.headers.is_empty());
    assert!(request.body.is_none());
}
