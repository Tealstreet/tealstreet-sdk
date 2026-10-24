//! Unsigned public-trades custody and unchanged signed-read compatibility.
use safe_cex_engine::model::{Interval, Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Error, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Query, RequestRoute, Response,
    VenueError,
};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};

type TestEngine = Engine<PhemexPerpetuals>;
const NOW: f64 = 1_710_000_000_000.;
const OWNER: u32 = 700;
const SYMBOL: &str = "BTCUSDT";
const TRADE: &str = r#"{"error":null,"result":{"symbol":"BTCUSDT","trades_p":[[1666860389799499999,"Buy","20702.1","0.949"],[1666860389799499999,"Buy","20702.1","0.949"]]}}"#;
const FILL: &str = r#"{"transactTimeNs":1669407633999999999,"symbol":"BTCUSDT","currency":"USDT","side":"Sell","tradeType":"Trade","execQtyRq":"0.001","execPriceRp":"16600","execFeeRv":"0.00166","execID":"execution-a","orderID":"order-a"}"#;

fn engine(primary: bool, secondary: bool) -> TestEngine {
    engine_with_mode(primary, secondary, false)
}
fn engine_with_mode(primary: bool, secondary: bool, public_data_only: bool) -> TestEngine {
    let mut e = safe_cex_engine::test_support::seeded_engine(PhemexPerpetuals::new(Options {
        public_data_only,
        key: primary.then(|| "fixture-primary".into()),
        secret: primary.then(|| "fixture-material".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "fixture-secondary".into(),
            secret: "fixture-read-material".into(),
            passphrase: None,
        }),
        ..Options::default()
    }));
    e.store_mut().set_markets(vec![Market {
        id: "BTC/USDT:USDT".into(),
        symbol: SYMBOL.into(),
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
    }]);
    e.take_frame(NOW);
    e
}
fn public() -> Query {
    Query::PublicTrades {
        symbol: SYMBOL.into(),
        limit: Some(2),
    }
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some(SYMBOL.into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(2),
    }
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: SYMBOL.into(),
        client_order_id: "caller-a".into(),
    }
}
fn ask(e: &mut TestEngine, at: f64, command: u32, query: Query) {
    e.handle(at, Event::Command(Command::Query { command, query }));
}
fn released(e: &mut TestEngine, at: f64, command: u32, query: Query) -> HttpRequest {
    ask(e, at, command, query);
    physical(safe_cex_engine::test_support::prepared_actions(e, at))
}
fn physical(actions: Vec<Action>) -> HttpRequest {
    let mut requests = actions.into_iter().filter_map(|action| match action {
        Action::Http(request) => Some(request),
        _ => None,
    });
    let request = requests.next().expect("one physical history read");
    assert!(requests.next().is_none());
    request
}
fn reply(
    e: &mut TestEngine,
    at: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn done(actions: &[Action], owner: u32) -> Option<&str> {
    actions.iter().find_map(|action| match action {
        Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
            Some(result.as_str())
        }
        _ => None,
    })
}
fn failure(actions: &[Action], owner: u32) -> &VenueError {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "one correlated history failure");
    assert!(
        done(actions, owner).is_none(),
        "a failure is not history data or lookup absence"
    );
    errors[0]
}
fn prepared(actions: &[Action], endpoint: &str, intent: RoutingIntent) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "physical history must await managed preparation"
    );
    let prepares: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    intent, request, ..
                } => Some((intent, request)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(prepares.len(), 1);
    assert_eq!(*prepares[0].0, intent);
    assert!(prepares[0].1.url.contains(endpoint));
}

#[test]
fn unsigned_public_history_requires_managed_preparation() {
    let mut e = engine(false, false);
    ask(&mut e, NOW, OWNER, public());
    prepared(
        &e.drain_actions().collect::<Vec<_>>(),
        "/md/v2/trade?symbol=BTCUSDT",
        RoutingIntent::Public,
    );
}

#[test]
fn unclaimed_response_cannot_publish_history_before_route_binding() {
    let mut e = engine(false, false);
    ask(&mut e, NOW, OWNER, public());
    let (request, preparation) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some((request, None)),
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    request,
                    preparation,
                    ..
                } => Some((request, Some(preparation))),
                _ => None,
            },
            _ => None,
        })
        .expect("native or managed original request");
    let output = reply(&mut e, NOW + 1., &request, 200, TRADE);
    assert!(
        !output.iter().any(|action| matches!(
            action,
            Action::Notice(
                Notice::CommandDone { command: OWNER, .. }
                    | Notice::CommandFailed { command: OWNER, .. }
            )
        )),
        "unclaimed response has no DATA or terminal authority"
    );
    if let Some(preparation) = preparation {
        assert!(
            e.http_preparation_pending(preparation),
            "early input cannot retire the live owner"
        );
    }
}

#[test]
fn parked_preparation_keeps_the_original_deadline_and_rejects_late_readiness() {
    let mut e = engine(false, false);
    ask(&mut e, NOW, OWNER, public());
    let actions: Vec<_> = e.drain_actions().collect();
    let (preparation, request) = actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request.clone())),
                _ => None,
            },
            _ => None,
        })
        .expect("history must retain managed preparation custody");
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("20s includes route preparation");
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    assert_eq!(
        failure(&e.drain_actions().collect::<Vec<_>>(), OWNER).kind,
        Some(ErrorKind::Timeout)
    );
    e.http_prepared(
        NOW + 20_001.,
        preparation,
        Prepared {
            id: request.id.0,
            expires_at: None,
        },
    );
    assert!(
        !e.drain_actions()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
    );
    assert!(reply(&mut e, NOW + 20_002., &request, 200, TRADE).is_empty());
}

#[test]
fn network_failure_keeps_its_typed_kind_without_message_inference() {
    let mut e = engine(false, false);
    let request = released(&mut e, NOW, OWNER, public());
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: request.id,
            error: "fixture timed out wording",
        },
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(failure(&output, OWNER).kind, Some(ErrorKind::Network));
}

#[test]
fn shared_context_capacity_refuses_history_before_native_active_slot_or_io() {
    let mut e = engine(false, false);
    let mut peers = Vec::new();
    for command in 1..=256 {
        e.handle(
            NOW,
            Event::Command(Command::FetchCandles {
                command,
                symbol: SYMBOL.into(),
                interval: Interval::M1,
                limit: Some(2),
                from_ms: Some(0.),
                to_ms: Some(180_000.),
            }),
        );
        peers.push(
            e.drain_actions()
                .find_map(|action| match action {
                    Action::ManagedHttp(action) => match *action {
                        HttpAction::Prepare { preparation, .. } => Some(preparation),
                        _ => None,
                    },
                    _ => None,
                })
                .expect("a real existing candle owns a shared Context"),
        );
    }
    assert!(peers.iter().all(|peer| e.http_preparation_pending(*peer)));
    ask(&mut e, NOW, OWNER, public());
    let output: Vec<_> = e.drain_actions().collect();
    assert!(
        !output
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_))),
        "exhausted shared authority refuses before dispatch"
    );
    let error = failure(&output, OWNER);
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.message, "Engine request capacity reached");
    assert!(error.outcome.is_none());
    assert!(
        peers.iter().all(|peer| e.http_preparation_pending(*peer)),
        "refusal must preserve every unrelated owner"
    );
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("release fixture peer"),
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(failure(&output, 1).message, "release fixture peer");
    assert!(!e.http_preparation_pending(peers[0]));
    assert!(
        peers[1..]
            .iter()
            .all(|peer| e.http_preparation_pending(*peer))
    );
    let request = released(&mut e, NOW + 2., OWNER, public());
    let output = reply(&mut e, NOW + 3., &request, 200, TRADE);
    assert!(
        done(&output, OWNER).is_some(),
        "refusal did not consume native active slot"
    );
    assert!(
        peers[1..]
            .iter()
            .all(|peer| e.http_preparation_pending(*peer)),
        "successful recovery must preserve all 255 unrelated peers"
    );
}

#[test]
fn pause_wake_cannot_renew_a_history_root_past_twenty_seconds() {
    let mut e = engine(false, false);
    let first = released(&mut e, NOW, OWNER, public());
    let output = reply(&mut e, NOW + 1., &first, 429, "busy");
    assert!(failure(&output, OWNER).message.contains("429"));
    ask(&mut e, NOW + 2., OWNER + 1, public());
    let output: Vec<_> = e.drain_actions().collect();
    let timer = output
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 60_001. => Some(*timer),
            _ => None,
        })
        .expect("actual native pause defers the read");
    e.handle(NOW + 60_001., Event::TimerFired { timer });
    let output: Vec<_> = e.drain_actions().collect();
    assert!(
        !output
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_))),
        "pause wake must validate the original root before issuing I/O"
    );
    assert_eq!(failure(&output, OWNER + 1).kind, Some(ErrorKind::Timeout));
    let fresh = released(&mut e, NOW + 60_002., OWNER + 1, public());
    let output = reply(&mut e, NOW + 60_003., &fresh, 200, TRADE);
    assert!(
        done(&output, OWNER + 1).is_some(),
        "expired pause owner released its native slot"
    );
}

#[test]
fn stale_rate_limit_response_cannot_pause_the_next_owner() {
    let mut e = engine(false, false);
    let first = released(&mut e, NOW, OWNER, public());
    let _ = reply(&mut e, NOW + 20_000., &first, 429, "late stale response");
    let fresh = released(&mut e, NOW + 20_001., OWNER + 1, public());
    let output = reply(&mut e, NOW + 20_002., &fresh, 200, TRADE);
    assert!(
        done(&output, OWNER + 1).is_some(),
        "expired prior payload cannot extend paused_until"
    );
}

#[test]
fn native_public_duplicates_keep_exact_values_without_live_account_mutation() {
    let mut e = engine(false, false);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let request = released(&mut e, NOW, OWNER, public());
    assert_eq!(
        request.url,
        "https://api.phemex.com/md/v2/trade?symbol=BTCUSDT"
    );
    assert!(request.headers.is_empty());
    let output = reply(&mut e, NOW + 1., &request, 200, TRADE);
    let rows: Vec<serde_json::Value> = serde_json::from_str(done(&output, OWNER).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0]["id"], rows[1]["id"]);
    assert_eq!(rows[0]["timestamp"], 1_666_860_389_799.);
    assert_eq!(rows[0]["price"], 20702.1);
    assert_eq!(rows[0]["qty"], 0.949);
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(reply(&mut e, NOW + 2., &request, 200, TRADE).is_empty());
}

#[test]
fn native_signed_read_selection_and_lookup_primary_are_preserved() {
    let mut e = engine(true, true);
    for (offset, query, key, route) in [
        (0., fills(), "fixture-secondary", None),
        (2., public(), "fixture-secondary", None),
        (4., lookup(), "fixture-primary", Some(RequestRoute::Primary)),
    ] {
        let request = released(&mut e, NOW + offset, OWNER, query);
        assert_eq!(request.route, route);
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == key)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, _)| *name == "X-PHEMEX-REQUEST-SIGNATURE")
        );
        let body = if offset == 0. {
            "[]"
        } else if offset == 2. {
            TRADE
        } else {
            "[]"
        };
        let output = reply(&mut e, NOW + offset + 1., &request, 200, body);
        assert!(done(&output, OWNER).is_some());
        if offset == 4. {
            assert_eq!(
                done(&output, OWNER),
                Some(r#"{"found":false,"definitive":true}"#),
            );
        }
    }
}

#[test]
fn native_optional_auth_survives_public_mode_and_single_credential_containers() {
    for (primary, secondary, public_data_only, expected_key) in [
        (true, false, false, "fixture-primary"),
        (false, true, false, "fixture-secondary"),
        (true, true, true, "fixture-secondary"),
        (false, true, true, "fixture-secondary"),
    ] {
        let mut e = engine_with_mode(primary, secondary, public_data_only);
        let request = released(&mut e, NOW, OWNER, public());
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == expected_key)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, _)| *name == "X-PHEMEX-REQUEST-SIGNATURE")
        );
        let output = reply(&mut e, NOW + 1., &request, 200, TRADE);
        assert!(done(&output, OWNER).is_some());
    }
}

#[test]
fn native_streamed_valid_prefix_is_discarded_after_a_bad_row() {
    let mut e = engine(true, false);
    let request = released(&mut e, NOW, OWNER, fills());
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    let chunk = format!("[{FILL},");
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: request.id,
            data: chunk.as_bytes(),
        },
    );
    assert!(
        e.drain_actions()
            .all(|action| !matches!(action, Action::Notice(Notice::CommandDone { .. })))
    );
    e.handle(
        NOW + 3.,
        Event::HttpChunk {
            request: request.id,
            data: b"{\"bad\":true}]",
        },
    );
    e.handle(
        NOW + 4.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert!(failure(&output, OWNER).message.contains("Malformed"));
    let second = released(&mut e, NOW + 5., OWNER, fills());
    let output = reply(&mut e, NOW + 6., &second, 200, &format!("[{FILL}]"));
    let rows: Vec<serde_json::Value> = serde_json::from_str(done(&output, OWNER).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["qty"], 0.001);
    assert_eq!(rows[0]["commission"], 0.00166);
}

#[test]
fn native_history_capacity_recovers_after_an_owned_terminal() {
    let mut e = engine(false, false);
    let reads: Vec<_> = (0..16)
        .map(|index| released(&mut e, NOW, OWNER + index, public()))
        .collect();
    ask(&mut e, NOW, OWNER + 16, public());
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        failure(&output, OWNER + 16).message,
        "Too many active Phemex history queries"
    );
    assert!(
        !output
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
    );
    let output = reply(&mut e, NOW + 1., &reads[0], 200, TRADE);
    assert!(done(&output, OWNER).is_some());
    let next = released(&mut e, NOW + 2., OWNER + 16, public());
    let output = reply(&mut e, NOW + 3., &next, 200, TRADE);
    assert!(done(&output, OWNER + 16).is_some());
}

#[test]
fn expired_public_read_cannot_finish_a_reused_owner() {
    let mut e = engine(false, false);
    let first = released(&mut e, NOW, OWNER, public());
    let output = reply(&mut e, NOW + 20_000., &first, 200, TRADE);
    assert_eq!(failure(&output, OWNER).kind, Some(ErrorKind::Timeout));
    let second = released(&mut e, NOW + 20_001., OWNER, public());
    assert_ne!(first.id, second.id);
    assert!(reply(&mut e, NOW + 20_002., &first, 200, TRADE).is_empty());
    let output = reply(&mut e, NOW + 20_003., &second, 200, TRADE);
    assert!(done(&output, OWNER).is_some());
}

#[test]
fn unsigned_route_refusals_preserve_metadata_without_auth_evidence() {
    let mut e = engine(false, false);
    for (index, kind) in [
        ErrorKind::AuthInvalid,
        ErrorKind::AuthExpired,
        ErrorKind::AuthPermission,
    ]
    .into_iter()
    .enumerate()
    {
        let at = NOW + index as f64 * 2.;
        ask(&mut e, at, OWNER, public());
        let preparation = e
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("unsigned route preparation");
        e.http_preparation_failed(
            at + 1.,
            preparation,
            Error::State,
            VenueError::new("route refusal")
                .kind(kind)
                .venue_code("route-code")
                .venue_message("native route detail"),
        );
        let output: Vec<_> = e.drain_actions().collect();
        let error = failure(&output, OWNER);
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.message, "route refusal");
        assert_eq!(error.venue_code.as_deref(), Some("route-code"));
        assert_eq!(error.venue_message.as_deref(), Some("native route detail"));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(!output.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::AccountAuth { .. }) | Action::Http(_)
        )));
    }
}

#[test]
fn invalid_public_entry_time_refuses_before_native_or_transport_effects() {
    let mut e = engine(false, false);
    ask(&mut e, f64::NAN, OWNER, public());
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(output.len(), 1);
    let error = failure(&output, OWNER);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert!(error.outcome.is_none());
    let fresh = released(&mut e, NOW, OWNER, public());
    let output = reply(&mut e, NOW + 1., &fresh, 200, TRADE);
    assert!(done(&output, OWNER).is_some());
}

#[test]
fn non_success_history_status_rejects_valid_rows_and_releases_its_slot() {
    for status in [100, 199, 300, 302, 399] {
        let mut e = engine(false, false);
        let request = released(&mut e, NOW, OWNER, public());
        let output = reply(&mut e, NOW + 1., &request, status, TRADE);
        let error = failure(&output, OWNER);
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(error.message, format!("Phemex history HTTP {status}"));
        assert!(!output.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
        let fresh = released(&mut e, NOW + 2., OWNER, public());
        let output = reply(&mut e, NOW + 3., &fresh, 200, TRADE);
        assert!(done(&output, OWNER).is_some());
    }
}
