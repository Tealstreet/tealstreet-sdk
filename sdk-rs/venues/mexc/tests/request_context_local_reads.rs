//! Immediate native read errors must use genuine bounded command custody.
mod support;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Query, Response, VenueError,
};
use support::TestEngine;

const NOW: f64 = 1_000.;
const DAY: f64 = 86_400_000.;

#[derive(Clone)]
enum Read {
    History(Query),
    Leverage(&'static str),
}
struct Case {
    private: bool,
    paused: bool,
    read: Read,
    error: &'static str,
}
fn orders(from: Option<f64>, to: Option<f64>, limit: Option<u32>) -> Query {
    Query::OrderHistory {
        symbol: Some("BTCUSDT".into()),
        from_ms: from,
        to_ms: to,
        limit,
    }
}
fn cases() -> Vec<Case> {
    vec![
        Case {
            private: false,
            paused: false,
            read: Read::History(Query::OrderByClientId {
                symbol: "UNKNOWN".into(),
                client_order_id: "caller".into(),
            }),
            error: "MEXC client-order-ID lookup is unsupported",
        },
        Case {
            private: false,
            paused: false,
            read: Read::History(Query::Income {
                symbol: Some("UNKNOWN".into()),
                kind: None,
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            }),
            error: "MEXC income supports only kind FUNDING_FEE",
        },
        Case {
            private: false,
            paused: false,
            read: Read::History(orders(Some(f64::NAN), None, Some(0))),
            error: "MEXC private queries require credentials",
        },
        Case {
            private: true,
            paused: false,
            read: Read::History(orders(Some(NOW + 100.), None, None)),
            error: "Invalid MEXC query time range",
        },
        Case {
            private: true,
            paused: false,
            read: Read::History(orders(Some(0.), Some(90. * DAY * 1000.), None)),
            error: "MEXC query range exceeds 1000 windows",
        },
        Case {
            private: true,
            paused: true,
            read: Read::Leverage("UNKNOWN"),
            error: "MEXC account reads are rate limited",
        },
        Case {
            private: false,
            paused: false,
            read: Read::Leverage("UNKNOWN"),
            error: "Market not found",
        },
        Case {
            private: false,
            paused: false,
            read: Read::Leverage("BTCUSDT"),
            error: "MEXC leverage requires credentials",
        },
    ]
}
fn ready(private: bool) -> TestEngine {
    let mut e = if private {
        let (mut e, _, reads, _) = support::private_started();
        for r in reads {
            support::response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
        }
        e
    } else {
        support::started().0
    };
    e.drain_actions().for_each(drop);
    e.take_frame(NOW);
    e
}
fn dispatch(e: &mut TestEngine, now: f64, owner: u32, read: Read) -> Vec<Action> {
    let command = match read {
        Read::History(query) => Command::Query {
            command: owner,
            query,
        },
        Read::Leverage(symbol) => Command::FetchLeverage {
            command: owner,
            symbol: symbol.into(),
        },
    };
    e.handle(now, Event::Command(command));
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
fn native(actions: &[Action], owner: u32, message: &str) {
    assert_eq!(
        actions.len(),
        1,
        "local error emits no I/O, timer, auth evidence or publication"
    );
    assert_eq!(
        terminal(actions, owner),
        vec![Err(VenueError::new(message))]
    );
}
fn rejected(actions: &[Action], owner: u32, kind: ErrorKind) {
    assert_eq!(
        actions.len(),
        1,
        "one engine initial refusal is the only effect"
    );
    let results = terminal(actions, owner);
    assert_eq!(results.len(), 1);
    let error = results[0]
        .as_ref()
        .expect_err("local error needs bounded custody");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
}
fn legacy_request(actions: Vec<Action>) -> HttpRequest {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
    );
    let mut requests = actions.into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let r = requests.next().expect("valid native read still dispatches");
    assert!(requests.next().is_none());
    r
}
fn fixture(case: &Case) -> (TestEngine, Option<HttpRequest>) {
    let mut e = ready(case.private);
    // Hold a real legacy read before book peers; its later 429 pauses account and depth together.
    let pause = case
        .paused
        .then(|| legacy_request(dispatch(&mut e, NOW, 900, Read::Leverage("BTCUSDT"))));
    (e, pause)
}
fn pause(e: &mut TestEngine, request: Option<HttpRequest>) {
    if let Some(r) = request {
        e.handle(
            NOW,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 429,
                    headers: "retry-after: 10",
                    body: r#"{"success":false,"code":429,"message":"limited"}"#,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(terminal(&actions, 900).len(), 1);
        e.take_frame(NOW);
    }
}
fn book(e: &mut TestEngine, owner: u32) -> Preparation {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(terminal(&actions, owner).is_empty());
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed book occupies one owner slot")
}

fn capacity_case(index: usize) {
    let case = cases().swap_remove(index);
    let (mut e, pending_pause) = fixture(&case);
    let peers: Vec<_> = (1..=256).map(|owner| book(&mut e, owner)).collect();
    pause(&mut e, pending_pause);
    rejected(
        &dispatch(&mut e, NOW, 600, case.read.clone()),
        600,
        ErrorKind::Internal,
    );
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("settled peer"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminal(&actions, 1).len(), 1);
    assert!(terminal(&actions, 600).is_empty());
    native(&dispatch(&mut e, NOW + 2., 600, case.read), 600, case.error);
    assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
    assert_eq!(e.health().auth_failures, 0);
    assert!(e.take_frame(NOW + 2.).is_none());
}
#[test]
fn original_unsupported_lookup_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(0);
}
#[test]
fn original_unsupported_income_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(1);
}
#[test]
fn original_grouped_history_validation_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(2);
}
#[test]
fn original_derived_history_range_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(3);
}
#[test]
fn original_history_window_cap_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(4);
}
#[test]
fn original_paused_leverage_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(5);
}
#[test]
fn original_unknown_leverage_market_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(6);
}
#[test]
fn original_missing_leverage_credentials_requires_capacity_and_recovers_a_peer_slot() {
    capacity_case(7);
}
#[test]
fn original_local_read_invalid_clocks_fail_typed_then_reuse_the_id_at_finite_time() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for case in cases().into_iter().filter(|c| !c.paused && !matches!(&c.read,
            Read::History(Query::OrderHistory { from_ms: Some(from), to_ms: None, .. }) if from.is_finite())) {
            let (mut e, _) = fixture(&case);
            rejected(&dispatch(&mut e, now, 601, case.read.clone()), 601, ErrorKind::InvalidInput);
            native(&dispatch(&mut e, NOW, 601, case.read), 601, case.error);
            assert_eq!(e.health().auth_failures, 0);
            assert!(e.take_frame(NOW).is_none());
        }
    }
}
#[test]
fn finite_local_reads_preserve_full_native_errors_and_all_eight_branch_precedence() {
    for case in cases() {
        let (mut e, pending_pause) = fixture(&case);
        pause(&mut e, pending_pause);
        native(&dispatch(&mut e, NOW, 602, case.read), 602, case.error);
        assert_eq!(e.health().auth_failures, 0);
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn grouped_history_errors_keep_validation_order_and_fields() {
    let reads = [
        (
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            },
            "MEXC fills require a symbol or order ID",
        ),
        (
            Query::Fills {
                symbol: Some("UNKNOWN".into()),
                order_id: Some("bad".into()),
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            },
            "Market not found",
        ),
        (
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: Some("bad".into()),
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            },
            "Invalid MEXC order ID",
        ),
        (
            orders(Some(f64::NAN), None, Some(0)),
            "Invalid MEXC query time range",
        ),
        (
            orders(Some(-1.), None, None),
            "Invalid MEXC query time range",
        ),
        (
            orders(Some(2.), Some(1.), Some(0)),
            "Invalid MEXC query time range",
        ),
        (
            orders(None, None, Some(0)),
            "MEXC query limit must be 1..100000",
        ),
        (
            orders(None, None, Some(100_001)),
            "MEXC query limit must be 1..100000",
        ),
    ];
    for (read, message) in reads {
        let mut e = ready(true);
        native(
            &dispatch(&mut e, NOW, 603, Read::History(read)),
            603,
            message,
        );
        assert!(e.take_frame(NOW).is_none());
        assert_eq!(e.health().auth_failures, 0);
    }
}
#[test]
fn repeated_local_reads_do_not_leak_command_owners() {
    let mut e = ready(false);
    for _ in 0..300 {
        for case in cases().into_iter().filter(|c| !c.private) {
            native(&dispatch(&mut e, NOW, 604, case.read), 604, case.error);
        }
    }
    let peer = book(&mut e, 604);
    assert!(e.http_preparation_pending(peer));
}
#[test]
fn duplicate_numeric_id_does_not_steal_an_existing_managed_owner() {
    for case in cases() {
        let (mut e, pending_pause) = fixture(&case);
        let peer = book(&mut e, 605);
        pause(&mut e, pending_pause);
        let actions = dispatch(&mut e, NOW, 605, case.read);
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
            if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
        );
        assert!(e.http_preparation_pending(peer));
        e.http_preparation_failed(
            NOW + 1.,
            peer,
            Error::State,
            VenueError::new("settled peer"),
        );
        assert_eq!(
            terminal(&e.drain_actions().collect::<Vec<_>>(), 605).len(),
            1
        );
        assert!(!e.http_preparation_pending(peer));
    }
}
#[test]
fn valid_native_history_and_leverage_continuations_keep_dispatch_and_results() {
    for (private, read, path, response) in [
        (
            true,
            Read::History(orders(None, None, Some(1))),
            "/api/v1/private/order/list/history_orders?page_num=1&page_size=1&states=3%2C4%2C5&symbol=BTC_USDT",
            r#"{"success":true,"code":0,"data":[]}"#,
        ),
        (
            true,
            Read::History(Query::Income {
                symbol: Some("BTCUSDT".into()),
                kind: Some("funding_fee".into()),
                from_ms: Some(0.),
                to_ms: Some(90. * DAY * 1000.),
                limit: Some(1),
            }),
            "/api/v1/private/position/funding_records?page_num=1&page_size=1&symbol=BTC_USDT",
            r#"{"success":true,"code":0,"data":{"currentPage":1,"totalPage":1,"resultList":[]}}"#,
        ),
        (
            false,
            Read::History(Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(1),
            }),
            "/api/v1/contract/deals/BTC_USDT?limit=1",
            r#"{"success":true,"code":0,"data":[]}"#,
        ),
        (
            true,
            Read::Leverage("BTCUSDT"),
            "/api/v1/private/position/leverage?symbol=BTC_USDT",
            r#"{"success":true,"code":0,"data":[{"positionType":1,"leverage":10,"openType":1,"maxLeverageView":125,"maxLeverage":500},{"positionType":2,"leverage":15,"openType":1,"maxLeverageView":100,"maxLeverage":500}]}"#,
        ),
    ] {
        let mut e = ready(private);
        let request = legacy_request(dispatch(&mut e, NOW, 606, read.clone()));
        assert_eq!(request.url, format!("https://api.mexc.com{path}"));
        if private {
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(k, v)| *k == "ApiKey" && v == "replay-key")
            );
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(k, v)| *k == "Request-Time" && v == "1000")
            );
        } else {
            assert!(request.headers.is_empty());
        }
        e.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: response,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let expected = if matches!(read, Read::Leverage(_)) {
            r#"{"buyLeverage":10.0,"sellLeverage":15.0,"marginType":"isolated","positionMode":"oneway"}"#
        } else {
            "[]"
        };
        assert_eq!(terminal(&actions, 606), vec![Ok(expected.into())]);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Http(_)))
        );
    }
}
#[test]
fn exactly_one_thousand_windows_and_order_specific_fills_keep_native_exceptions() {
    for read in [
        orders(Some(0.), Some(90. * DAY * 1000. - 1.), Some(1)),
        Query::Fills {
            symbol: None,
            order_id: Some("739113577038255617".into()),
            from_ms: Some(0.),
            to_ms: Some(90. * DAY * 1000.),
            limit: Some(1),
        },
    ] {
        let mut e = ready(true);
        let request = legacy_request(dispatch(&mut e, NOW, 607, Read::History(read)));
        assert!(
            request.url.contains("start_time=")
                || request.url.ends_with("/deal_details/739113577038255617?")
        );
        let peer = book(&mut e, 608);
        assert!(e.http_preparation_pending(peer));
    }
}
