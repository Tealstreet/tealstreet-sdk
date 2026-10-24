//! Public candles preserve their native wire contract under shared Read custody.
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, TimerId,
    VenueError,
};
use safe_cex_venue_cryptocom::{Cryptocom, Options};
type TestEngine = Engine<Cryptocom>;
const NOW: f64 = 1_710_000_000_000.;
const ROWS: &str = r#"{"code":0,"result":{"data":[{"t":120001,"o":"1","h":"2","l":"0.5","c":"1.5","v":"3"},{"t":60001,"o":"bad","h":"2","l":"0.5","c":"1.5","v":"3"}]}}"#;
fn engine(options: Options) -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(Cryptocom::new(options))
}
fn command(e: &mut TestEngine, now: f64, owner: u32, interval: Interval) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "BTC/USD+PERP".into(),
            interval,
            limit: Some(0),
            from_ms: Some(1.25),
            to_ms: Some(180_000.5),
        }),
    );
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
fn no_io_auth(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Sign { .. } | Action::Notice(Notice::AccountAuth { .. })
    )));
    assert!(
        !actions.iter().any(|a| matches!(a,
            Action::ManagedHttp(action) if !matches!(action.as_ref(), HttpAction::Cancel { .. })
        )),
        "only engine cancellation cleanup is allowed after settlement"
    );
}
fn failure(actions: &[Action], owner: u32, kind: ErrorKind, outcome: Option<Outcome>) {
    let result = terminals(actions, owner);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, outcome);
    no_io_auth(actions);
}
fn parked(e: &mut TestEngine, now: f64, owner: u32) -> (Preparation, HttpRequest, TimerId) {
    command(e, now, owner, Interval::M1);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "HTTP must await route preparation"
    );
    let mut pending = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(a) => match a.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                intent,
            } => {
                assert_eq!(*intent, RoutingIntent::Public);
                Some((*preparation, request.clone()))
            }
            _ => None,
        },
        _ => None,
    });
    let (preparation, request) = pending.next().expect("one real managed candle preparation");
    assert!(pending.next().is_none());
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("original twenty-second command bound starts before preparation");
    assert_eq!(request.timeout_ms, Some(20_000));
    assert!(request.headers.is_empty() && request.body.is_none());
    (preparation, request, timer)
}
fn release(e: &mut TestEngine, now: f64, owner: u32) -> HttpRequest {
    command(e, now, owner, Interval::M1);
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn reply(
    e: &mut TestEngine,
    now: f64,
    packet: &HttpRequest,
    status: u16,
    body: &str,
    stream: bool,
) -> Vec<Action> {
    if stream {
        e.handle(
            now,
            Event::HttpStart {
                request: packet.id,
                status,
                headers: "",
            },
        );
        for data in body.as_bytes().chunks(7) {
            e.handle(
                now,
                Event::HttpChunk {
                    request: packet.id,
                    data,
                },
            );
        }
        e.handle(now, Event::HttpEnd { request: packet.id });
    } else {
        e.handle(
            now,
            Event::HttpResponse {
                request: packet.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    e.drain_actions().collect()
}
fn book_peer(e: &mut TestEngine, owner: u32) -> Preparation {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: "BTCUSD-PERP".into(),
            limit: None,
        }),
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}
#[test]
fn original_candle_preparation_binding_and_one_claim_preserve_native_rows() {
    let mut e = engine(Options::default());
    let (preparation, packet, _) = parked(&mut e, NOW, 7);
    assert!(reply(&mut e, NOW + 1., &packet, 200, ROWS, false).is_empty());
    e.http_prepared(
        NOW + 2.,
        preparation,
        Prepared {
            id: packet.id.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let permit = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert_eq!(request.id, packet.id);
                    Some(permit)
                }
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        e.claim_http(NOW + 3., permit),
        Ok(Claim::Released(_))
    ));
    assert!(e.claim_http(NOW + 4., permit).is_err());
    e.drain_actions().for_each(drop);
    let result = terminals(&reply(&mut e, NOW + 5., &packet, 200, ROWS, true), 7);
    let rows: serde_json::Value = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows[0]["timestamp"], 120001.);
    assert_eq!(rows[1]["timestamp"], 60001.);
    assert_eq!(rows[1]["open"], 0.);
    assert!(reply(&mut e, NOW + 6., &packet, 200, ROWS, false).is_empty());
    assert!(e.take_frame(NOW + 6.).is_none());
}
#[test]
fn original_parked_deadline_refuses_late_readiness_once() {
    let mut e = engine(Options::default());
    let (preparation, packet, timer) = parked(&mut e, NOW, 8);
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        8,
        ErrorKind::Timeout,
        Some(Outcome::NotSent),
    );
    assert!(!e.http_preparation_pending(preparation));
    e.http_prepared(
        NOW + 20_001.,
        preparation,
        Prepared {
            id: packet.id.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
    assert!(reply(&mut e, NOW + 20_002., &packet, 200, ROWS, false).is_empty());
}
#[test]
fn original_released_expiry_and_reused_id_fence_stale_callbacks() {
    let mut e = engine(Options::default());
    let first = release(&mut e, NOW, 9);
    failure(
        &reply(&mut e, NOW + 20_000., &first, 200, ROWS, false),
        9,
        ErrorKind::Timeout,
        Some(Outcome::NotSent),
    );
    let second = release(&mut e, NOW + 20_001., 9);
    assert_ne!(first.id, second.id);
    assert!(reply(&mut e, NOW + 20_002., &first, 200, ROWS, false).is_empty());
    let results = terminals(&reply(&mut e, NOW + 20_003., &second, 200, ROWS, false), 9);
    assert_eq!(results.len(), 1);
    assert!(results[0].is_ok());
}
#[test]
fn original_typed_route_cause_survives_without_retry() {
    let mut e = engine(Options::default());
    let (preparation, packet, _) = parked(&mut e, NOW, 10);
    e.http_preparation_failed(
        NOW + 1.,
        preparation,
        Error::State,
        VenueError::new("route unavailable")
            .kind(ErrorKind::Unsupported)
            .venue_code("route"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    failure(&actions, 10, ErrorKind::Unsupported, Some(Outcome::NotSent));
    let result = terminals(&actions, 10);
    assert_eq!(
        result[0].as_ref().unwrap_err().venue_code.as_deref(),
        Some("route")
    );
    assert!(reply(&mut e, NOW + 2., &packet, 200, ROWS, false).is_empty());
    assert_eq!(e.health().auth_failures, 0);
}
#[test]
fn original_typed_transport_cause_survives_without_retry() {
    let mut e = engine(Options::default());
    let packet = release(&mut e, NOW + 3., 11);
    e.handle(
        NOW + 4.,
        Event::HttpFailed {
            request: packet.id,
            error: "wire failed",
        },
    );
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        11,
        ErrorKind::Network,
        Some(Outcome::NotSent),
    );
    assert_eq!(e.health().auth_failures, 0);
}
#[test]
fn original_candle_invalid_clocks_and_finite_reuse_are_genuine() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(Options::default());
        command(&mut e, now, 12, Interval::M1);
        failure(
            &e.drain_actions().collect::<Vec<_>>(),
            12,
            ErrorKind::InvalidInput,
            None,
        );
        let _ = release(&mut e, NOW, 12);
    }
}
#[test]
fn original_candle_capacity_refusal_and_recovery_are_genuine() {
    let mut e = engine(Options::default());
    let peers: Vec<_> = (100..356).map(|owner| book_peer(&mut e, owner)).collect();
    command(&mut e, NOW, 356, Interval::M1);
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        356,
        ErrorKind::Internal,
        None,
    );
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("free owner slot"),
    );
    e.drain_actions().for_each(drop);
    let _ = release(&mut e, NOW + 2., 356);
    assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
}
#[test]
fn finite_native_candle_urls_options_timeframes_and_ignored_limit_stay_unsigned() {
    for (interval, timeframe) in [
        (Interval::M1, "1m"),
        (Interval::M5, "5m"),
        (Interval::M15, "15m"),
        (Interval::M30, "30m"),
        (Interval::H1, "1h"),
        (Interval::H2, "2h"),
        (Interval::H4, "4h"),
        (Interval::H12, "12h"),
        (Interval::D1, "1D"),
        (Interval::W1, "7D"),
    ] {
        for testnet in [false, true] {
            for (primary, secondary) in [(false, false), (true, false), (false, true), (true, true)]
            {
                let mut e = engine(Options {
                    testnet,
                    key: primary.then(|| "fixture-identity".into()),
                    secret: primary.then(|| "fixture-material".into()),
                    read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
                        key: "fixture-secondary".into(),
                        secret: "fixture-material".into(),
                        passphrase: None,
                    }),
                    ..Options::default()
                });
                command(&mut e, NOW, 13, interval);
                let packet = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
                    .into_iter()
                    .find_map(|a| match a {
                        Action::Http(r) => Some(r),
                        _ => None,
                    })
                    .unwrap();
                let root = if testnet {
                    "https://uat-api.3ona.co/exchange/v1"
                } else {
                    "https://api.crypto.com/exchange/v1"
                };
                assert_eq!(
                    packet.url,
                    format!(
                        "{root}/public/get-candlestick?instrument_name=BTC%2FUSD%2BPERP&timeframe={timeframe}&start_ts=1.25&end_ts=180000.5"
                    )
                );
                assert!(packet.headers.is_empty() && packet.body.is_none());
                let results = terminals(&reply(&mut e, NOW + 1., &packet, 200, ROWS, false), 13);
                let rows: serde_json::Value =
                    serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
                assert_eq!(rows.as_array().unwrap().len(), 2);
                assert_eq!(rows[0]["timestamp"], 120001.);
                assert_eq!(rows[1]["open"], 0.);
                assert_eq!(e.health().auth_failures, 0);
            }
        }
    }
}
#[test]
fn finite_native_response_errors_keep_full_fields_auth_health_and_no_retry() {
    for stream in [false, true] {
        for (status, body, message) in [
            (
                401,
                r#"{"code":0,"result":{"data":[]}}"#,
                "Crypto.com HTTP error 401".to_owned(),
            ),
            (
                403,
                r#"{"code":0,"result":{"data":[]}}"#,
                "Crypto.com HTTP error 403".to_owned(),
            ),
            (
                500,
                r#"{"code":40101,"message":"bad key","result":{"data":[]}}"#,
                "Crypto.com API Error 40101: bad key".to_owned(),
            ),
            (
                200,
                r#"{"code":40101,"message":"bad key","result":{"data":[]}}"#,
                "Crypto.com API Error 40101: bad key".to_owned(),
            ),
            (
                200,
                r#"{"code":40101,"message":"bad key"}"#,
                r#"response did not contain the expected list: {"code":40101,"message":"bad key"}"#
                    .to_owned(),
            ),
            (
                200,
                r#"{"code":0,"result":{"data":[{"t":1}]}}"#,
                "invalid candle row".to_owned(),
            ),
            (200, r#"{"code":0,"result":{"data":[]}}"#, String::new()),
        ] {
            let mut e = engine(Options::default());
            let packet = release(&mut e, NOW, 14);
            let actions = reply(&mut e, NOW + 1., &packet, status, body, stream);
            let results = terminals(&actions, 14);
            assert_eq!(results.len(), 1);
            if message.is_empty() {
                assert_eq!(results[0], Ok("[]".into()));
            } else {
                assert_eq!(*results[0].as_ref().unwrap_err(), VenueError::new(message));
            }
            no_io_auth(&actions);
            assert_eq!(e.health().auth_failures, 0);
            assert!(reply(&mut e, NOW + 2., &packet, 200, ROWS, false).is_empty());
        }
    }
}
#[test]
fn unsupported_interval_is_local_and_original_root_bound_still_applies() {
    let mut e = engine(Options::default());
    command(&mut e, NOW, 15, Interval::M3);
    let actions: Vec<_> = e.drain_actions().collect();
    no_io_auth(&actions);
    let results = terminals(&actions, 15);
    assert_eq!(results.len(), 1);
    assert_eq!(
        *results[0].as_ref().unwrap_err(),
        VenueError::new(Interval::M3.unsupported("Crypto.com"))
    );
}

#[test]
fn finite_native_undecodable_stream_body_keeps_string_failure() {
    for status in [200, 500] {
        let mut e = engine(Options::default());
        let packet = release(&mut e, NOW, 16);
        let actions = reply(&mut e, NOW + 1., &packet, status, "unavailable", false);
        let result = terminals(&actions, 16);
        assert_eq!(result.len(), 1);
        assert_eq!(
            *result[0].as_ref().unwrap_err(),
            VenueError::new(if status == 500 {
                "Crypto.com HTTP error 500"
            } else {
                "response did not contain the expected list: unavailable"
            })
        );
        no_io_auth(&actions);
        assert_eq!(e.health().auth_failures, 0);
    }
}

#[test]
fn original_unsupported_interval_requires_valid_clock() {
    for at in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(Options::default());
        command(&mut e, at, 17, Interval::M3);
        let output: Vec<_> = e.drain_actions().collect();
        assert_eq!(output.len(), 1);
        failure(&output, 17, ErrorKind::InvalidInput, None);
        command(&mut e, NOW, 17, Interval::M3);
        let output: Vec<_> = e.drain_actions().collect();
        assert_eq!(output.len(), 1);
        assert_eq!(
            terminals(&output, 17),
            [Err(VenueError::new(Interval::M3.unsupported("Crypto.com")))]
        );
    }
}
#[test]
fn original_unsupported_interval_requires_capacity_and_keeps_peers() {
    let mut e = engine(Options::default());
    let peers: Vec<_> = (100..356).map(|owner| book_peer(&mut e, owner)).collect();
    command(&mut e, NOW, 356, Interval::M3);
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(output.len(), 1);
    failure(&output, 356, ErrorKind::Internal, None);
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("free peer"),
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminals(&output, 100).len(), 1);
    assert!(terminals(&output, 356).is_empty());
    command(&mut e, NOW + 2., 356, Interval::M3);
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(output.len(), 1);
    assert_eq!(
        terminals(&output, 356),
        [Err(VenueError::new(Interval::M3.unsupported("Crypto.com")))]
    );
    assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
}
#[test]
fn original_retired_bind_item_failure_and_timer_do_not_touch_reused_owner() {
    let mut e = engine(Options::default());
    let (preparation, first, timer) = parked(&mut e, NOW, 18);
    e.http_prepared(
        NOW + 1.,
        preparation,
        Prepared {
            id: first.id.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("real unclaimed bind permit");
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    failure(
        &e.drain_actions().collect::<Vec<_>>(),
        18,
        ErrorKind::Timeout,
        Some(Outcome::NotSent),
    );
    let second = release(&mut e, NOW + 20_001., 18);
    assert_ne!(first.id, second.id);
    assert!(e.claim_http(NOW + 20_002., permit).is_err());
    e.http_prepared(
        NOW + 20_003.,
        preparation,
        Prepared {
            id: first.id.0,
            expires_at: None,
        },
    );
    e.handle(
        NOW + 20_004.,
        Event::HttpStart {
            request: first.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 20_004.,
        Event::HttpChunk {
            request: first.id,
            data: ROWS.as_bytes(),
        },
    );
    e.handle(NOW + 20_004., Event::HttpEnd { request: first.id });
    e.handle(
        NOW + 20_005.,
        Event::HttpFailed {
            request: first.id,
            error: "retired failure",
        },
    );
    e.handle(NOW + 20_006., Event::TimerFired { timer });
    assert!(e.drain_actions().next().is_none());
    let fresh = r#"{"code":0,"result":{"data":[{"t":900001,"o":"9","h":"10","l":"8","c":"9.5","v":"11"}]}}"#;
    let results = terminals(&reply(&mut e, NOW + 20_007., &second, 200, fresh, true), 18);
    assert_eq!(results.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
    assert_eq!(
        rows,
        serde_json::json!([{"timestamp":900001.,"open":9.,"high":10.,"low":8.,"close":9.5,"volume":11.}])
    );
    assert!(e.take_frame(NOW + 20_007.).is_none());
    assert_eq!(e.health().auth_failures, 0);
}
