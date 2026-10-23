//! Local history results still borrow genuine, bounded command-entry custody.
use safe_cex_engine::model::{Interval, Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Query, VenueError,
};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};

type TestEngine = Engine<PhemexPerpetuals>;
const NOW: f64 = 1_710_000_000_000.;
const SYMBOL: &str = "BTCUSDT";
const UNSAFE: &str = "BAD/PAIR";
const CREDENTIALS: &str = "Phemex private history requires credentials";
const HISTORY_CAPACITY: &str = "Too many active Phemex history queries";
const INCOME_CAPACITY: &str = "Too many active Phemex income queries";
const LOOKUP: &str = r#"{"found":false,"definitive":false}"#;

fn engine(private: bool) -> TestEngine {
    let mut e = safe_cex_engine::test_support::seeded_engine(PhemexPerpetuals::new(Options {
        key: private.then(|| "fixture-identity".into()),
        secret: private.then(|| "fixture-material".into()),
        ..Options::default()
    }));
    e.store_mut().set_markets(
        [SYMBOL, UNSAFE]
            .into_iter()
            .map(|symbol| Market {
                id: symbol.into(),
                symbol: symbol.into(),
                pretty_symbol: None,
                base: "BTC".into(),
                quote: "USDT".into(),
                active: true,
                precision: Precision {
                    price: 0.1,
                    amount: 0.001,
                },
                limits: Limits {
                    amount: MinMax {
                        min: 0.001,
                        max: 100.,
                    },
                    leverage: MinMax { min: 1., max: 100. },
                    price: None,
                },
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            })
            .collect(),
    );
    e.take_frame(NOW);
    e
}
fn fills(symbol: Option<&str>, from_ms: Option<f64>) -> Query {
    Query::Fills {
        symbol: symbol.map(Into::into),
        order_id: None,
        from_ms,
        to_ms: None,
        limit: None,
    }
}
fn income(symbol: Option<&str>, limit: Option<u32>) -> Query {
    Query::Income {
        symbol: symbol.map(Into::into),
        kind: None,
        from_ms: Some(NOW - 1_000.),
        to_ms: Some(NOW),
        limit,
    }
}
fn lookup(symbol: &str, id: &str) -> Query {
    Query::OrderByClientId {
        symbol: symbol.into(),
        client_order_id: id.into(),
    }
}
fn public(symbol: &str) -> Query {
    Query::PublicTrades {
        symbol: symbol.into(),
        limit: Some(2),
    }
}
fn query(e: &mut TestEngine, now: f64, command: u32, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command, query }));
    e.drain_actions().collect()
}
fn terminal(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn native(actions: &[Action], owner: u32, expected: Result<&str, &str>) {
    assert_eq!(
        actions.len(),
        1,
        "local terminal has no I/O, timers, auth or publication"
    );
    let result = terminal(actions, owner);
    assert_eq!(result.len(), 1);
    match expected {
        Ok(data) => assert_eq!(result[0].as_ref().unwrap(), data),
        Err(text) => {
            let error = result[0].as_ref().unwrap_err();
            assert_eq!(error.message, text);
            assert_eq!(error.kind, None);
            assert_eq!(error.outcome, None);
        }
    }
}
fn admission_failure(actions: &[Action], owner: u32, kind: ErrorKind, text: &str) {
    assert_eq!(
        actions.len(),
        1,
        "admission refusal must be the only action"
    );
    let result = terminal(actions, owner);
    assert_eq!(result.len(), 1);
    let error = result[0]
        .as_ref()
        .expect_err("local result needs genuine engine admission");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
    assert_eq!(error.message, text);
}
fn candle(e: &mut TestEngine, owner: u32) -> Preparation {
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: SYMBOL.into(),
            interval: Interval::M1,
            limit: Some(2),
            from_ms: Some(0.),
            to_ms: Some(180_000.),
        }),
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("actual unsigned candle holds one managed peer owner")
}
fn capacity(read: Query, expected: Result<&str, &str>) {
    let mut e = engine(false);
    let peers: Vec<_> = (1..=256).map(|owner| candle(&mut e, owner)).collect();
    admission_failure(
        &query(&mut e, NOW, 600, read.clone()),
        600,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    assert!(peers.iter().all(|peer| e.http_preparation_pending(*peer)));
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("settle one peer"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let result = terminal(&actions, 1);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].as_ref().unwrap_err().message, "settle one peer");
    assert!(terminal(&actions, 600).is_empty());
    native(&query(&mut e, NOW + 2., 600, read), 600, expected);
    assert!(
        peers[1..]
            .iter()
            .all(|peer| e.http_preparation_pending(*peer))
    );
    e.http_preparation_failed(
        NOW + 3.,
        peers[1],
        Error::State,
        VenueError::new("independent peer"),
    );
    let result = terminal(&e.drain_actions().collect::<Vec<_>>(), 2);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].as_ref().unwrap_err().message, "independent peer");
    assert!(e.take_frame(NOW + 3.).is_none());
}
#[test]
fn local_history_refusal_requires_capacity_and_recovers_after_peer_settlement() {
    capacity(fills(None, None), Err(CREDENTIALS));
}
#[test]
fn nondefinitive_lookup_requires_capacity_and_recovers_after_peer_settlement() {
    capacity(lookup("UNKNOWN", "native-id"), Ok(LOOKUP));
}
#[test]
fn local_income_refusal_requires_capacity_and_recovers_after_peer_settlement() {
    capacity(income(None, None), Err(CREDENTIALS));
}
#[test]
fn local_history_lookup_and_income_reject_invalid_clock_then_allow_id_reuse() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (read, expected) in [
            (fills(None, None), Err(CREDENTIALS)),
            (lookup("UNKNOWN", "native-id"), Ok(LOOKUP)),
            (income(None, None), Err(CREDENTIALS)),
        ] {
            let mut e = engine(false);
            admission_failure(
                &query(&mut e, now, 601, read.clone()),
                601,
                ErrorKind::InvalidInput,
                "Invalid request context",
            );
            native(&query(&mut e, NOW, 601, read), 601, expected);
            assert!(e.take_frame(NOW).is_none());
        }
    }
}
#[test]
fn finite_local_results_keep_exact_native_payloads_errors_and_precedence() {
    for (private, read, expected) in [
        (
            false,
            lookup("UNKNOWN", ""),
            Err("Invalid Phemex client order ID"),
        ),
        (false, lookup("UNKNOWN", "native-id"), Ok(LOOKUP)),
        (false, fills(None, Some(f64::NAN)), Err(CREDENTIALS)),
        (
            true,
            fills(None, Some(f64::NAN)),
            Err("Phemex history requires a known symbol"),
        ),
        (
            true,
            fills(Some(SYMBOL), Some(f64::NAN)),
            Err("Invalid Phemex history time range"),
        ),
        (
            true,
            fills(Some(UNSAFE), None),
            Err("Unsupported Phemex native history symbol"),
        ),
        (false, income(Some(SYMBOL), Some(0)), Err(CREDENTIALS)),
        (
            true,
            income(Some(SYMBOL), Some(0)),
            Err("Phemex account income cannot be filtered by symbol"),
        ),
        (
            true,
            income(None, Some(0)),
            Err("Invalid Phemex income range or limit"),
        ),
        (
            false,
            public("UNKNOWN"),
            Err("Phemex history requires a known symbol"),
        ),
        (
            false,
            public(UNSAFE),
            Err("Unsupported Phemex native history symbol"),
        ),
    ] {
        let mut e = engine(private);
        native(&query(&mut e, NOW, 602, read), 602, expected);
        assert!(e.take_frame(NOW).is_none());
    }
}
fn duplicate(actions: &[Action], owner: u32) {
    assert!(terminal(actions, owner).is_empty());
    assert!(matches!(actions, [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE));
}
fn dispatch_at(e: &mut TestEngine, now: f64, owner: u32, read: Query) -> HttpRequest {
    let actions = query(e, now, owner, read);
    assert!(terminal(&actions, owner).is_empty());
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("valid native path retains its physical dispatch")
}
fn dispatch(e: &mut TestEngine, owner: u32, read: Query) -> HttpRequest {
    dispatch_at(e, NOW, owner, read)
}
fn fail_native_at(e: &mut TestEngine, now: f64, owner: u32, request: &HttpRequest) {
    e.handle(
        now,
        Event::HttpFailed {
            request: request.id,
            error: "native peer completed",
        },
    );
    let result = terminal(&e.drain_actions().collect::<Vec<_>>(), owner);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].as_ref().unwrap_err().message,
        "native peer completed"
    );
}
#[test]
fn active_history_capacity_duplicate_and_native_symbol_precedence_preserve_owners() {
    let mut e = engine(true);
    let first = dispatch(&mut e, 1, fills(Some(SYMBOL), None));
    duplicate(&query(&mut e, NOW, 1, fills(Some(SYMBOL), None)), 1);
    for owner in 2..=16 {
        dispatch(&mut e, owner, fills(Some(SYMBOL), None));
    }
    native(
        &query(&mut e, NOW, 603, fills(Some(UNSAFE), None)),
        603,
        Err(HISTORY_CAPACITY),
    );
    fail_native_at(&mut e, NOW + 1., 1, &first);
    native(
        &query(&mut e, NOW + 1., 603, fills(Some(UNSAFE), None)),
        603,
        Err("Unsupported Phemex native history symbol"),
    );
    let replacement = dispatch_at(&mut e, NOW + 2., 603, fills(Some(SYMBOL), None));
    fail_native_at(&mut e, NOW + 3., 603, &replacement);
}
#[test]
fn active_income_capacity_and_duplicate_preserve_original_native_owner() {
    let mut e = engine(true);
    let first = dispatch(&mut e, 1, income(None, None));
    duplicate(&query(&mut e, NOW, 1, income(None, None)), 1);
    for owner in 2..=16 {
        dispatch(&mut e, owner, income(None, None));
    }
    native(
        &query(&mut e, NOW, 604, income(None, None)),
        604,
        Err(INCOME_CAPACITY),
    );
    fail_native_at(&mut e, NOW + 1., 1, &first);
    let replacement = dispatch_at(&mut e, NOW + 2., 604, income(None, None));
    fail_native_at(&mut e, NOW + 3., 604, &replacement);
}
#[test]
fn repeated_local_admission_does_not_accumulate_owners_or_block_native_paths() {
    let mut e = engine(false);
    for _ in 0..300 {
        native(
            &query(&mut e, NOW, 605, lookup("UNKNOWN", "native-id")),
            605,
            Ok(LOOKUP),
        );
    }
    let peer = candle(&mut e, 605);
    let actions = query(&mut e, NOW, 605, lookup("UNKNOWN", "native-id"));
    assert!(terminal(&actions, 605).is_empty());
    assert!(
        matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    assert!(e.http_preparation_pending(peer));
}
#[test]
fn valid_signed_and_public_native_dispatch_keep_original_route_and_continuations() {
    for (private, read, path) in [
        (
            true,
            fills(Some(SYMBOL), None),
            "/api-data/g-futures/trades?symbol=BTCUSDT&offset=0&limit=100",
        ),
        (
            true,
            income(None, None),
            "/api-data/futures/v2/tradeAccountDetail?currency=USDT&start=1709999999000&end=1710000000000&offset=0&limit=200",
        ),
        (false, public(SYMBOL), "/md/v2/trade?symbol=BTCUSDT"),
    ] {
        let mut e = engine(private);
        let request = dispatch(&mut e, 606, read);
        assert!(request.url.ends_with(path), "{}", request.url);
        assert_eq!(
            request
                .headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case("x-phemex-request-signature")),
            private
        );
        fail_native_at(&mut e, NOW + 1., 606, &request);
    }
}
