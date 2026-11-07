//! Initial local history refusals need genuine shared Read ownership.
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Query, Response, VenueError,
};
use safe_cex_venue_backpack::{Backpack, Options};
type Driver = Engine<Backpack>;

#[derive(Clone, Copy)]
enum Refusal {
    Lookup,
    IncomeKind,
    InvalidTime,
    MissingMarket,
}
const CASES: [Refusal; 4] = [
    Refusal::Lookup,
    Refusal::IncomeKind,
    Refusal::InvalidTime,
    Refusal::MissingMarket,
];
impl Refusal {
    fn query(self) -> Query {
        match self {
            Self::Lookup => Query::OrderByClientId {
                symbol: "NOT_LISTED".into(),
                client_order_id: "1".into(),
            },
            Self::IncomeKind => Query::Income {
                symbol: Some("NOT_LISTED".into()),
                kind: Some("COMMISSION".into()),
                from_ms: Some(20.),
                to_ms: Some(10.),
                limit: Some(0),
            },
            Self::InvalidTime => Query::Fills {
                symbol: Some("NOT_LISTED".into()),
                order_id: None,
                from_ms: Some(20.),
                to_ms: Some(10.),
                limit: Some(0),
            },
            Self::MissingMarket => Query::PublicTrades {
                symbol: "NOT_LISTED".into(),
                limit: Some(0),
            },
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::Lookup => "Backpack client-order-ID lookup is unsupported",
            Self::IncomeKind => "Backpack income query supports funding payments only",
            Self::InvalidTime => "Backpack invalid history time range",
            Self::MissingMarket => "Backpack history market not found",
        }
    }
}
fn engine(public_data_only: bool) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only,
        ..Default::default()
    }));
    e.store_mut().set_hedged(true);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 90.,
        used: 33.,
        upnl: 7.,
    });
    e
}
fn unchanged(e: &Driver) {
    assert!(e.store().hedged());
    let b = e.store().balance();
    assert_eq!((b.total, b.free, b.used, b.upnl), (123., 90., 33., 7.));
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}
fn invoke(e: &mut Driver, command: u32, case: Refusal, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::Query {
            command,
            query: case.query(),
        }),
    );
    e.drain_actions().collect()
}
fn failure(actions: &[Action], command: u32) -> &VenueError {
    assert!(
        actions.iter().all(|a| matches!(a, Action::Notice(_))),
        "local history refusal creates no physical work, timer or signing"
    );
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "one terminal for this initial entry");
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)
    ));
    errors[0]
}
fn park(e: &mut Driver, command: u32) -> Preparation {
    e.handle(
        10.,
        Event::Command(Command::FetchOrderBook {
            command,
            symbol: "SOL_USDC_PERP".into(),
            limit: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("working depth read parks under an engine owner")
}
fn complete_parked(e: &mut Driver, token: Preparation, command: u32) {
    e.http_prepared(
        12.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prior owner still binds after the refused setting");
    assert!(matches!(e.claim_http(12., permit), Ok(Claim::Released(_))));
    e.handle(
        13.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"bids":[],"asks":[]}"#,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: id, result }) if *id == command && result == r#"{"bids":[],"asks":[]}"#)).count(), 1);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command)
    ));
}

#[test]
fn native_local_refusals_remain_keyless_and_preserve_store_without_io() {
    for public in [false, true] {
        for case in CASES {
            let mut e = engine(public);
            let actions = invoke(&mut e, 7, case, 10.);
            let error = failure(&actions, 7);
            assert_eq!(error.message, case.message());
            assert_eq!(error.kind, None);
            assert_eq!(error.outcome, None);
            unchanged(&e);
        }
    }
}
#[test]
fn native_validation_precedence_is_unchanged() {
    let mut e = engine(false);
    // Bad income kind wins over its reversed bounds and missing market.
    let actions = invoke(&mut e, 7, Refusal::IncomeKind, 10.);
    assert_eq!(failure(&actions, 7).message, Refusal::IncomeKind.message());
    // Invalid bounds win over a missing market, before credentials are inspected.
    let actions = invoke(&mut e, 8, Refusal::InvalidTime, 10.);
    assert_eq!(failure(&actions, 8).message, Refusal::InvalidTime.message());
    unchanged(&e);
}
fn invalid_clock(case: Refusal) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(false);
        let actions = invoke(&mut e, 7, case, now);
        let error = failure(&actions, 7);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        unchanged(&e);
    }
}
#[test]
fn unsupported_lookup_obeys_shared_clock() {
    invalid_clock(Refusal::Lookup);
}
#[test]
fn unsupported_income_kind_obeys_shared_clock() {
    invalid_clock(Refusal::IncomeKind);
}
#[test]
fn invalid_time_refusal_obeys_shared_clock() {
    invalid_clock(Refusal::InvalidTime);
}
#[test]
fn missing_market_refusal_obeys_shared_clock() {
    invalid_clock(Refusal::MissingMarket);
}
fn capacity(case: Refusal) {
    let mut e = engine(false);
    let first = park(&mut e, 1);
    for id in 2..=256 {
        park(&mut e, id);
    }
    let actions = invoke(&mut e, 257, case, 11.);
    let error = failure(&actions, 257);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    complete_parked(&mut e, first, 1);
    unchanged(&e);
}
#[test]
fn unsupported_lookup_obeys_shared_capacity() {
    capacity(Refusal::Lookup);
}
#[test]
fn unsupported_income_kind_obeys_shared_capacity() {
    capacity(Refusal::IncomeKind);
}
#[test]
fn invalid_time_refusal_obeys_shared_capacity() {
    capacity(Refusal::InvalidTime);
}
#[test]
fn missing_market_refusal_obeys_shared_capacity() {
    capacity(Refusal::MissingMarket);
}
#[test]
fn duplicate_local_query_preserves_real_prior_owner_delivery() {
    for case in CASES {
        let mut e = engine(false);
        let first = park(&mut e, 7);
        let actions = invoke(&mut e, 7, case, 11.);
        assert!(actions.iter().all(|a| matches!(a, Action::Notice(_))));
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "command.duplicate")));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 7, .. })
                | Action::Notice(Notice::CommandDone { command: 7, .. })
        )));
        complete_parked(&mut e, first, 7);
        unchanged(&e);
    }
}
#[test]
fn completed_local_query_refusals_allow_immediate_id_reuse() {
    let mut e = engine(false);
    for case in CASES {
        let actions = invoke(&mut e, 7, case, 10.);
        let error = failure(&actions, 7);
        assert_eq!(error.message, case.message());
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
        unchanged(&e);
    }
}
