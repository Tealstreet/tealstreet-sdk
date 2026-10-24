//! Original private-history custody controls; production remains gated on JWT APIs.
mod support;

use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Method, Notice, Query, RequestRoute, Response,
    VenueError,
};
use safe_cex_venue_paradex::Options;
use support::{Harness, MARKETS, TICKERS};

const SYMBOL: &str = "BTC-USD-PERP";
const START: f64 = 1_681_300_000_000.;
const END: f64 = 1_681_400_000_000.;
const FILL: &str = r#"{"id":"fill-a","order_id":"123","market":"BTC-USD-PERP","side":"BUY","price":"30000.12","size":"0.5","remaining_size":"0.5","fee":"-0.02","fee_currency":"USDC","realized_pnl":"7.56","realized_funding":"-0.1","created_at":1681375176910,"liquidity":"MAKER"}"#;
const ORDER: &str = r#"{"id":"123","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","status":"CLOSED","size":"2","remaining_size":"0.5","price":"30000","avg_fill_price":"30001","cancel_reason":"USER_CANCELED","created_at":1681375176910,"last_updated_at":1681375177000,"client_id":"caller","flags":[],"instruction":"GTC"}"#;

#[derive(Clone, Copy, Debug)]
enum Family {
    Fills,
    Orders,
    Income,
    Lookup,
}
const FAMILIES: [Family; 4] = [
    Family::Fills,
    Family::Orders,
    Family::Income,
    Family::Lookup,
];

fn login_pending(testnet: bool) -> Harness {
    let mut h = Harness::empty(Options {
        testnet,
        key: Some("opaque-public-subkey".into()),
        paradex_address: Some("0x123".into()),
        subkey_address: Some("0x456".into()),
        payload_signer: true,
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    let auth = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SignPayload { request, .. } => Some(*request),
            _ => None,
        })
        .expect("existing delegated startup must request its real auth payload");
    h.e.handle(
        h.now,
        Event::PayloadSigned {
            request: auth,
            signature: "fixture signature",
            headers: vec![],
        },
    );
    h.collect();
    assert!(
        h.pending
            .iter()
            .any(|request| request.url.contains("/auth/"))
    );
    h.e.take_frame(h.now);
    h.actions.clear();
    h
}

fn ready(testnet: bool) -> Harness {
    let mut h = login_pending(testnet);
    h.reply("/auth/", r#"{"jwt_token":"test-jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    assert!(
        h.failures.is_empty(),
        "native startup must succeed before custody tests"
    );
    h.e.take_frame(h.now);
    h.actions.clear();
    h
}

fn query(family: Family) -> Query {
    match family {
        Family::Fills => Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(START),
            to_ms: Some(END),
            limit: Some(2),
        },
        Family::Orders => Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: Some(START),
            to_ms: Some(END),
            limit: Some(2),
        },
        Family::Income => Query::Income {
            symbol: Some(SYMBOL.into()),
            kind: None,
            from_ms: Some(START),
            to_ms: Some(END),
            limit: Some(5),
        },
        Family::Lookup => Query::OrderByClientId {
            symbol: SYMBOL.into(),
            client_order_id: "caller".into(),
        },
    }
}
fn path(family: Family) -> &'static str {
    match family {
        Family::Fills | Family::Income => "/fills?",
        Family::Orders => "/orders-history?",
        Family::Lookup => "/orders?",
    }
}
fn raw_command(h: &mut Harness, owner: u32, family: Family) -> Vec<Action> {
    command_wait(h, owner, query(family), 120_000)
}
fn command_wait(h: &mut Harness, owner: u32, query: Query, wait: u32) -> Vec<Action> {
    h.e.handle_command(
        h.now,
        Command::Query {
            command: owner,
            query,
        },
        Some(wait),
    )
    .unwrap();
    h.e.drain_actions().collect()
}

fn terminals(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
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
fn request(actions: &[Action]) -> HttpRequest {
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request.clone()),
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { request, .. } => Some(request.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "exactly one native physical read is selected"
    );
    requests[0].clone()
}
fn no_marker(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}

#[test]
fn native_authenticated_routes_headers_and_rows_are_live_on_both_networks() {
    for testnet in [false, true] {
        for family in FAMILIES {
            let mut h = ready(testnet);
            let balance = h.e.store().balance().clone();
            h.command(Command::Query {
                command: 900,
                query: query(family),
            });
            let request = h.take(path(family));
            let host = if testnet { "testnet" } else { "prod" };
            let page_size = match family {
                Family::Fills => 2,
                Family::Income => 500,
                _ => 100,
            };
            let bounds = if matches!(family, Family::Lookup) {
                String::new()
            } else {
                "&start_at=1681300000000&end_at=1681400000000".into()
            };
            assert_eq!(
                request.url,
                format!(
                    "https://api.{host}.paradex.trade/v1{}market=BTC-USD-PERP&page_size={page_size}{bounds}",
                    path(family)
                )
            );
            assert_eq!(request.method, Method::Get);
            assert!(request.body.is_none());
            assert_eq!(
                request.headers,
                vec![
                    ("Authorization", "Bearer test-jwt".into()),
                    ("Content-Type", "application/json".into()),
                    ("Accept", "application/json".into()),
                ]
            );
            assert_ne!(request.route, Some(RequestRoute::PublicCors));
            let row = if matches!(family, Family::Orders | Family::Lookup) {
                ORDER
            } else {
                FILL
            };
            h.respond(request, 200, &format!(r#"{{"results":[{row}]}}"#));
            let value: serde_json::Value = serde_json::from_str(h.result(900).unwrap()).unwrap();
            match family {
                Family::Fills => {
                    assert_eq!(value[0]["qty"], 0.5);
                    assert_eq!(value[0]["commission"], 0.02);
                }
                Family::Orders => {
                    assert_eq!(value[0]["id"], "123");
                    assert_eq!(value[0]["filled"], 1.5);
                }
                Family::Income => {
                    assert_eq!(value.as_array().unwrap().len(), 3);
                    assert_eq!(value[0]["income"], 7.56);
                }
                Family::Lookup => {
                    assert_eq!(value["found"], true);
                    assert_eq!(value["order"]["id"], "123");
                }
            }
            assert_eq!(terminals(&h.actions, 900).len(), 1);
            assert_eq!(h.e.store().balance(), &balance);
            assert!(h.e.store().orders().is_empty());
            assert!(h.e.store().positions().is_empty());
            assert!(h.e.take_frame(h.now).is_none());
            no_marker(&h.actions);
        }
    }
}

fn requires_preparation(family: Family, testnet: bool) {
    let mut h = ready(testnet);
    let actions = raw_command(&mut h, 901, family);
    assert!(terminals(&actions, 901).is_empty());
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "private history must await managed preparation instead of emitting legacy HTTP"
    );
    let intents: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    request, intent, ..
                } => {
                    assert_eq!(request.method, Method::Get);
                    assert!(request.url.contains(path(family)));
                    assert!(request.body.is_none());
                    Some(*intent)
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(intents.len(), 1);
    assert_ne!(
        intents[0],
        RoutingIntent::Public,
        "Bearer reads cannot use public/CORS intent"
    );
    no_marker(&actions);
}

#[test]
fn fills_mainnet_waits_for_managed_preparation() {
    requires_preparation(Family::Fills, false);
}

#[test]
fn fills_testnet_waits_for_managed_preparation() {
    requires_preparation(Family::Fills, true);
}

#[test]
fn orders_mainnet_waits_for_managed_preparation() {
    requires_preparation(Family::Orders, false);
}

#[test]
fn orders_testnet_waits_for_managed_preparation() {
    requires_preparation(Family::Orders, true);
}

#[test]
fn income_mainnet_waits_for_managed_preparation() {
    requires_preparation(Family::Income, false);
}

#[test]
fn income_testnet_waits_for_managed_preparation() {
    requires_preparation(Family::Income, true);
}

#[test]
fn lookup_mainnet_waits_for_managed_preparation() {
    requires_preparation(Family::Lookup, false);
}

#[test]
fn lookup_testnet_waits_for_managed_preparation() {
    requires_preparation(Family::Lookup, true);
}

#[test]
fn an_unclaimed_private_page_cannot_publish_a_history_result() {
    let mut h = ready(false);
    let actions = raw_command(&mut h, 902, Family::Orders);
    let read = request(&actions);
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: read.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"results":[]}"#,
            },
        },
    );
    let actions: Vec<_> = h.e.drain_actions().collect();
    assert!(
        terminals(&actions, 902).is_empty(),
        "unclaimed bytes cannot authorize an empty private history result"
    );
    no_marker(&actions);
}

#[test]
fn full_genuine_read_capacity_denies_a_private_page_before_dispatch() {
    let mut h = ready(false);
    for owner in 1..=256 {
        h.e.handle_command(
            h.now,
            Command::FetchOrderBook {
                command: owner,
                symbol: SYMBOL.into(),
                limit: Some(1),
            },
            Some(120_000),
        )
        .unwrap();
        let actions: Vec<_> = h.e.drain_actions().collect();
        assert!(terminals(&actions, owner).is_empty());
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))).count(), 1,
            "each peer must be a genuine parked Read owner");
    }
    h.e.handle_command(
        h.now,
        Command::FetchOrderBook {
            command: 904,
            symbol: SYMBOL.into(),
            limit: Some(1),
        },
        Some(120_000),
    )
    .unwrap();
    let control: Vec<_> = h.e.drain_actions().collect();
    let denied = terminals(&control, 904);
    assert_eq!(denied.len(), 1, "the shared pool really is full");
    let canonical = denied[0].as_ref().unwrap_err();
    assert_eq!(canonical.kind, Some(ErrorKind::Internal));
    assert_eq!(canonical.outcome, None);
    assert!(
        !control
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    let actions = raw_command(&mut h, 903, Family::Orders);
    let results = terminals(&actions, 903);
    assert_eq!(
        results.len(),
        1,
        "capacity denies the private walk before dispatch"
    );
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error, canonical);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    no_marker(&actions);
}

#[test]
fn a_private_walk_captures_the_configured_caller_cap_before_dispatch() {
    let mut h = ready(false);
    let actions = command_wait(&mut h, 904, query(Family::Orders), 1_234);
    let read = request(&actions);
    assert_eq!(
        read.timeout_ms,
        Some(1_234),
        "first page cannot exceed the original caller wait"
    );
    assert!(terminals(&actions, 904).is_empty());
}

fn begin_orders(h: &mut Harness, owner: u32, wait: u32) -> HttpRequest {
    let actions = command_wait(h, owner, query(Family::Orders), wait);
    request(&actions)
}
fn accept_first_order(h: &mut Harness, owner: u32, wait: u32) -> HttpRequest {
    let first = begin_orders(h, owner, wait);
    h.respond(
        first,
        200,
        &format!(r#"{{"results":[{ORDER}],"next":"accepted-page"}}"#),
    );
    assert!(
        terminals(&h.actions, owner).is_empty(),
        "first page is accepted without completing the walk"
    );
    let second = h.take("cursor=accepted-page");
    assert!(second.url.contains("/orders-history?"));
    second
}
fn marker_count(actions: &[Action]) -> usize {
    actions.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)).count()
}
fn counts(actions: &[Action], owner: u32) -> (usize, usize, usize) {
    let terminals = terminals(actions, owner);
    (
        terminals.iter().filter(|result| result.is_ok()).count(),
        terminals.iter().filter(|result| result.is_err()).count(),
        marker_count(actions),
    )
}
fn assert_strict_failure(h: &Harness, owner: u32) {
    assert_eq!(
        counts(&h.actions, owner),
        (0, 1, 0),
        "a nontransport failure cannot turn accepted rows into partial success"
    );
}

#[test]
fn original_root_equality_is_strict_after_an_accepted_page_without_firing_a_timer() {
    let mut h = ready(false);
    let second = accept_first_order(&mut h, 905, 100);
    h.now += 100.;
    h.respond(second, 200, r#"{"results":[]}"#);
    assert_strict_failure(&h, 905);
    let results = terminals(&h.actions, 905);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
}

#[test]
fn a_login_wait_expires_under_the_original_caller_cap() {
    let mut h = login_pending(false);
    let actions = command_wait(&mut h, 906, query(Family::Orders), 100);
    assert!(terminals(&actions, 906).is_empty());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    h.now += 100.;
    h.e.handle(h.now, Event::PollScaleChanged { scale: 1. });
    let actions: Vec<_> = h.e.drain_actions().collect();
    assert_eq!(
        terminals(&actions, 906).len(),
        1,
        "auth waiting cannot outlive the caller root"
    );
    let results = terminals(&actions, 906);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    no_marker(&actions);
}

#[test]
fn late_login_cannot_release_a_private_page_for_an_expired_root() {
    let mut h = login_pending(false);
    let actions = command_wait(&mut h, 907, query(Family::Orders), 100);
    assert!(terminals(&actions, 907).is_empty());
    h.now += 100.;
    let auth = h.take("/auth/");
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: auth.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"jwt_token":"late-jwt"}"#,
            },
        },
    );
    let actions: Vec<_> = h.e.drain_actions().collect();
    let history = actions.iter().filter(|a| match a {
        Action::Http(request) => request.url.contains("/orders-history?"),
        Action::ManagedHttp(action) => matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains("/orders-history?")),
        _ => false,
    }).count();
    assert_eq!(
        history, 0,
        "late auth cannot mint a page after the original receipt expires"
    );
    let results = terminals(&actions, 907);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    no_marker(&actions);
}

#[test]
fn native_http_status_after_an_accepted_page_is_strict() {
    let mut h = ready(false);
    let second = accept_first_order(&mut h, 908, 120_000);
    h.respond(second, 503, &format!(r#"{{"results":[{ORDER}]}}"#));
    assert_strict_failure(&h, 908);
}
#[test]
fn native_error_envelope_after_an_accepted_page_is_strict() {
    let mut h = ready(false);
    let second = accept_first_order(&mut h, 909, 120_000);
    h.respond(
        second,
        200,
        &format!(r#"{{"error":"UNAVAILABLE","results":[{ORDER}]}}"#),
    );
    assert_strict_failure(&h, 909);
}
#[test]
fn malformed_stream_after_an_accepted_page_is_strict() {
    let mut h = ready(false);
    let second = accept_first_order(&mut h, 910, 120_000);
    h.respond(second, 200, &format!(r#"{{"results":[{ORDER}],"next":"#));
    assert_strict_failure(&h, 910);
}

#[test]
fn native_transport_failure_keeps_only_the_accepted_prefix() {
    let mut h = ready(false);
    let second = accept_first_order(&mut h, 911, 120_000);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: second.id,
            error: "scripted transport failure",
        },
    );
    h.collect();
    assert_eq!(counts(&h.actions, 911), (1, 0, 1));
    let rows: serde_json::Value = serde_json::from_str(h.result(911).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "123");
    assert_eq!(rows[0]["filled"], 1.5);
}
#[test]
fn an_empty_prefix_transport_failure_is_strict() {
    let mut h = ready(false);
    let first = begin_orders(&mut h, 912, 120_000);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: first.id,
            error: "scripted transport failure",
        },
    );
    h.collect();
    assert_strict_failure(&h, 912);
}
#[test]
fn a_finite_limit_finishes_from_data_without_issuing_a_suffix() {
    let mut h = ready(false);
    let first = begin_orders(&mut h, 913, 120_000);
    let other = ORDER.replace(r#""id":"123""#, r#""id":"124""#);
    h.respond(
        first,
        200,
        &format!(r#"{{"results":[{ORDER},{other}],"next":"unissued-suffix"}}"#),
    );
    assert_eq!(counts(&h.actions, 913), (1, 0, 0));
    let rows: serde_json::Value = serde_json::from_str(h.result(913).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("unissued-suffix"))
    );
}
#[test]
fn successful_empty_discovery_switches_to_one_native_aggregate_page() {
    let mut h = ready(false);
    let actions = raw_command(&mut h, 914, Family::Fills);
    h.respond(request(&actions), 200, r#"{"results":[]}"#);
    let aggregate = h.take("/orders-history?");
    assert!(aggregate.url.contains("limit=2"));
    assert!(!aggregate.url.contains("cursor="));
    h.respond(
        aggregate,
        200,
        &format!(r#"{{"results":[{ORDER}],"next":"ignored"}}"#),
    );
    assert_eq!(counts(&h.actions, 914), (1, 0, 0));
    let rows: serde_json::Value = serde_json::from_str(h.result(914).unwrap()).unwrap();
    assert_eq!(rows[0]["qty"], 1.5);
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("/fills?")
                || request.url.contains("/orders-history?"))
    );
}
#[test]
fn lookup_definitive_absence_requires_complete_open_and_history_data() {
    let mut h = ready(false);
    let actions = raw_command(&mut h, 915, Family::Lookup);
    h.respond(request(&actions), 200, r#"{"results":[]}"#);
    assert!(
        terminals(&h.actions, 915).is_empty(),
        "open miss alone is not definitive"
    );
    let historical = h.take("/orders-history?");
    h.respond(historical, 200, r#"{"results":[]}"#);
    assert_eq!(counts(&h.actions, 915), (1, 0, 0));
    let result: serde_json::Value = serde_json::from_str(h.result(915).unwrap()).unwrap();
    assert_eq!(result["found"], false);
    assert_eq!(result["definitive"], true);
}
#[test]
fn lookup_transport_failure_never_claims_definitive_absence() {
    let mut h = ready(false);
    let actions = raw_command(&mut h, 916, Family::Lookup);
    let first = request(&actions);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: first.id,
            error: "scripted transport failure",
        },
    );
    h.collect();
    assert_strict_failure(&h, 916);
}

#[test]
fn unknown_markets_keep_their_native_local_result_without_a_physical_page() {
    for family in [Family::Orders, Family::Lookup] {
        let mut h = ready(false);
        let query = match query(family) {
            Query::OrderHistory {
                from_ms,
                to_ms,
                limit,
                ..
            } => Query::OrderHistory {
                symbol: Some("UNKNOWN-USD-PERP".into()),
                from_ms,
                to_ms,
                limit,
            },
            Query::OrderByClientId {
                client_order_id, ..
            } => Query::OrderByClientId {
                symbol: "UNKNOWN-USD-PERP".into(),
                client_order_id,
            },
            _ => unreachable!(),
        };
        let actions = command_wait(&mut h, 917, query, 120_000);
        assert_eq!(counts(&actions, 917), (1, 0, 0));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
        );
        let results = terminals(&actions, 917);
        let result: serde_json::Value = serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
        if matches!(family, Family::Lookup) {
            assert_eq!(
                result,
                serde_json::json!({"found": false, "definitive": false})
            );
        } else {
            assert_eq!(result, serde_json::json!([]));
        }
    }
}

#[test]
fn an_empty_page_after_accepted_fills_does_not_start_an_aggregate_stage() {
    let mut h = ready(false);
    let actions = raw_command(&mut h, 918, Family::Fills);
    h.respond(
        request(&actions),
        200,
        &format!(r#"{{"results":[{FILL}],"next":"discovery-page"}}"#),
    );
    assert!(terminals(&h.actions, 918).is_empty());
    h.reply("cursor=discovery-page", r#"{"results":[]}"#);
    assert_eq!(counts(&h.actions, 918), (1, 0, 0));
    let rows: serde_json::Value = serde_json::from_str(h.result(918).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["qty"], 0.5);
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("/fills?")
                || request.url.contains("/orders-history?"))
    );
}
