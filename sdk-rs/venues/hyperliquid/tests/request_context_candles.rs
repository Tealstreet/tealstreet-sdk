//! Real perps candles must preserve JSON headers through managed preparation.
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Permit, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Response,
    VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const NOW: f64 = 1000.;
const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100"}]]"#;
const BODY: &str = r#"{"type":"candleSnapshot","req":{"coin":"BTC","interval":"1m","startTime":0,"endTime":60000}}"#;
const CANDLES: &str = r#"[{"t":2000,"o":"2","h":"3","l":"1","c":"2","v":"4"},{"t":1000,"o":"1","h":"2","l":"1","c":"2","v":"3"}]"#;

fn reply(e: &mut Engine<Hyperliquid>, now: f64, request: &HttpRequest, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn boot(testnet: bool) -> Engine<Hyperliquid> {
    boot_modes(testnet, false, false)
}

fn boot_modes(testnet: bool, hip3: bool, merged_spot: bool) -> Engine<Hyperliquid> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        testnet,
        enable_spot_markets: merged_spot,
        ..Options::default()
    }));
    e.start(NOW);
    // Discovery is deliberately still legacy: this slice changes candles only.
    let initial: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    let metadata = initial
        .iter()
        .find(|r| r.body.as_deref() == Some(r#"{"type":"metaAndAssetCtxs"}"#))
        .expect("legacy catalog seed");
    reply(&mut e, NOW, metadata, META);
    assert_eq!(e.store().market("BTC-PERP").unwrap().id, "BTC");
    if merged_spot {
        let spot = initial
            .iter()
            .find(|r| r.body.as_deref() == Some(r#"{"type":"spotMetaAndAssetCtxs"}"#))
            .expect("merged spot discovery");
        reply(
            &mut e,
            NOW,
            spot,
            r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"UBTC","index":1,"szDecimals":5,"fullName":"Unit Bitcoin"}],"universe":[{"name":"@1","tokens":[1,0],"index":1}]},[{"coin":"@1","midPx":"60000","markPx":"60010","prevDayPx":"59000","dayNtlVlm":"1000"}]]"#,
        );
    }
    let dexs = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.body.as_deref() == Some(r#"{"type":"perpDexs"}"#) => Some(r),
            _ => None,
        })
        .expect("legacy dex discovery");
    reply(
        &mut e,
        NOW,
        &dexs,
        if hip3 {
            r#"[null,{"name":"xyz"}]"#
        } else {
            "[null]"
        },
    );
    if hip3 {
        let dex = e
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(r)
                    if r.body.as_deref() == Some(r#"{"type":"metaAndAssetCtxs","dex":"xyz"}"#) =>
                {
                    Some(r)
                }
                _ => None,
            })
            .expect("native HIP-3 catalog");
        reply(&mut e, NOW, &dex, &META.replace("BTC", "xyz:GOLD"));
        assert_eq!(e.store().market("XYZ|GOLD-PERP").unwrap().id, "xyz:GOLD");
    }
    if merged_spot {
        assert_eq!(e.store().market("UBTC/USDC").unwrap().is_spot, Some(true));
    }
    // Anonymous fee seeding and socket setup are outside this candle control.
    e.drain_actions().for_each(drop);
    e.take_frame(NOW);
    e
}

fn ask(e: &mut Engine<Hyperliquid>, now: f64, command: u32) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: "BTC-PERP".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: Some(60_000.),
        }),
    );
    e.drain_actions().collect()
}

fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "perps candle JSON POST must await managed preparation, with no legacy HTTP"
    );
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Sign { .. } | Action::SignPayload { .. } | Action::SignedOutbound { .. }
        )),
        "public candle read requires no signer or fabricated credential"
    );
    let mut prepares = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(action) => match action.as_ref() {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => {
                assert_eq!(*intent, RoutingIntent::Public);
                Some((*preparation, request.clone()))
            }
            _ => None,
        },
        _ => None,
    });
    let prepared = prepares
        .next()
        .expect("actual candle read emits managed Prepare");
    assert!(prepares.next().is_none());
    prepared
}

fn wire(request: &HttpRequest, testnet: bool) {
    assert_eq!(request.method, Method::Post);
    assert_eq!(
        request.url,
        if testnet {
            "https://api.hyperliquid-testnet.xyz/info"
        } else {
            "https://api.hyperliquid.xyz/info"
        }
    );
    assert_eq!(request.body.as_deref(), Some(BODY));
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    assert!(request.signer_headers.is_empty());
}

fn release(
    e: &mut Engine<Hyperliquid>,
    token: Preparation,
    now: f64,
    testnet: bool,
) -> HttpRequest {
    let (permit, request) = bind(e, token, now);
    wire(&request, testnet);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}

fn bind(e: &mut Engine<Hyperliquid>, token: Preparation, now: f64) -> (Permit, HttpRequest) {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Sign { .. } | Action::SignPayload { .. }
    )));
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("public prepared packet binds without signing")
}

fn results(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
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

#[test]
fn perps_json_headers_survive_prepare_bind_and_preclaim_callbacks_cannot_complete() {
    for testnet in [false, true] {
        let mut e = boot(testnet);
        let actions = ask(&mut e, NOW, 500);
        let (token, unsigned) = preparation(&actions);
        wire(&unsigned, testnet);
        assert!(results(&actions, 500).is_empty());
        reply(&mut e, NOW + 1., &unsigned, CANDLES);
        e.handle(
            NOW + 1.,
            Event::HttpStart {
                request: unsigned.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            NOW + 1.,
            Event::HttpChunk {
                request: unsigned.id,
                data: CANDLES.as_bytes(),
            },
        );
        e.handle(
            NOW + 1.,
            Event::HttpEnd {
                request: unsigned.id,
            },
        );
        assert!(results(&e.drain_actions().collect::<Vec<_>>(), 500).is_empty());
        assert!(e.take_frame(NOW + 1.).is_none());
        let request = release(&mut e, token, NOW + 2., testnet);
        reply(&mut e, NOW + 3., &request, CANDLES);
        let done = results(&e.drain_actions().collect::<Vec<_>>(), 500);
        assert_eq!(done.len(), 1);
        let rows: serde_json::Value = serde_json::from_str(done[0].as_ref().unwrap()).unwrap();
        // Native response order survives, with millisecond wire times mapped to seconds.
        assert_eq!(rows[0]["timestamp"], 2.);
        assert_eq!(rows[1]["timestamp"], 1.);
        assert_eq!(rows[0]["volume"], 4.);
        reply(&mut e, NOW + 4., &request, CANDLES);
        assert!(results(&e.drain_actions().collect::<Vec<_>>(), 500).is_empty());
    }
}

#[test]
fn perps_candle_original_deadline_covers_parking_and_release_without_id_reuse_leak() {
    for claimed in [false, true] {
        let mut e = boot(false);
        let actions = ask(&mut e, NOW, 501);
        let (token, unsigned) = preparation(&actions);
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        if claimed {
            release(&mut e, token, NOW + 10_000., false);
        }
        e.handle(NOW + 20_000., Event::TimerFired { timer: timers[0] });
        let terminal = results(&e.drain_actions().collect::<Vec<_>>(), 501);
        assert!(matches!(terminal.as_slice(), [Err(_)]));
        if !claimed {
            assert_eq!(
                terminal[0].as_ref().unwrap_err().kind,
                Some(ErrorKind::Timeout)
            );
        }
        assert_eq!(
            terminal[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::Timeout)
        );
        assert_eq!(
            terminal[0].as_ref().unwrap_err().outcome,
            if claimed {
                None
            } else {
                Some(Outcome::NotSent)
            }
        );
        let (next, _) = preparation(&ask(&mut e, NOW + 20_001., 501));
        reply(&mut e, NOW + 20_002., &unsigned, CANDLES);
        assert!(results(&e.drain_actions().collect::<Vec<_>>(), 501).is_empty());
        let request = release(&mut e, next, NOW + 20_003., false);
        reply(&mut e, NOW + 20_004., &request, CANDLES);
        assert!(matches!(
            results(&e.drain_actions().collect::<Vec<_>>(), 501).as_slice(),
            [Ok(_)]
        ));
    }
}

#[test]
fn perps_candle_typed_preparation_refusal_is_preserved_once() {
    let mut e = boot(false);
    let (token, _) = preparation(&ask(&mut e, NOW, 502));
    e.http_preparation_failed(
        NOW + 1.,
        token,
        Error::State,
        VenueError::new("public route denied").kind(ErrorKind::AuthPermission),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let terminal = results(&actions, 502);
    assert_eq!(terminal.len(), 1);
    let error = terminal[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::AuthPermission));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.message, "public route denied");
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Sign { .. } | Action::SignPayload { .. }
    )));
    e.http_preparation_failed(NOW + 2., token, Error::State, VenueError::new("duplicate"));
    assert!(results(&e.drain_actions().collect::<Vec<_>>(), 502).is_empty());
}

#[test]
fn native_zero_limit_cap_and_ignored_from_geometry_survive() {
    let mut e = boot(false);
    for (index, (limit, from, expected_start)) in [
        (Some(0), None, 120_000_000),
        (Some(2000), Some(1.), 30_000_000),
        (Some(1), Some(f64::NAN), 119_940_000),
        (None, Some(1.9), 1),
        (None, None, 90_000_000),
    ]
    .into_iter()
    .enumerate()
    {
        let command = 600 + index as u32;
        let now = NOW + index as f64 * 10.;
        e.handle(
            now,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTC-PERP".into(),
                interval: Interval::M1,
                limit,
                from_ms: from,
                to_ms: Some(120_000_000.),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let (token, unsigned) = preparation(&actions);
        let expected = serde_json::json!({"type":"candleSnapshot","req":{"coin":"BTC","interval":"1m","startTime":expected_start,"endTime":120_000_000}});
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(unsigned.body.as_deref().unwrap()).unwrap(),
            expected
        );
        let (permit, request) = bind(&mut e, token, now + 1.);
        assert_eq!(request.body, unsigned.body);
        assert_eq!(
            request.headers,
            vec![("Content-Type", "application/json".into())]
        );
        assert!(request.signer_headers.is_empty());
        assert!(matches!(
            e.claim_http(now + 1., permit),
            Ok(Claim::Released(_))
        ));
        reply(&mut e, now + 2., &request, "[]");
        assert_eq!(
            results(&e.drain_actions().collect::<Vec<_>>(), command).as_slice(),
            [Ok("[]".into())]
        );
    }
}

#[test]
fn hip3_raw_coin_spelling_is_preserved_in_public_managed_json() {
    let mut e = boot_modes(false, true, false);
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 610,
            symbol: "XYZ|GOLD-PERP".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: Some(60_000.),
        }),
    );
    let (token, unsigned) = preparation(&e.drain_actions().collect::<Vec<_>>());
    assert_eq!(
        unsigned.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"xyz:GOLD","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
    let (permit, request) = bind(&mut e, token, NOW + 1.);
    assert_eq!(request.body, unsigned.body);
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    assert!(matches!(
        e.claim_http(NOW + 1., permit),
        Ok(Claim::Released(_))
    ));
    reply(&mut e, NOW + 2., &request, CANDLES);
    assert!(matches!(
        results(&e.drain_actions().collect::<Vec<_>>(), 610).as_slice(),
        [Ok(_)]
    ));
}

#[test]
fn streamed_prefix_is_never_published_and_progress_cannot_extend_deadline() {
    for failure in 0..3 {
        let mut e = boot(false);
        let actions = ask(&mut e, NOW, 611);
        let (token, _) = preparation(&actions);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        let request = release(&mut e, token, NOW + 1., false);
        e.handle(
            NOW + 19_000.,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            NOW + 19_001.,
            Event::HttpChunk {
                request: request.id,
                data: br#"[{"t":2000,"o":"2","h":"3","l":"1","c":"2","v":"4"},"#,
            },
        );
        assert!(results(&e.drain_actions().collect::<Vec<_>>(), 611).is_empty());
        match failure {
            0 => e.handle(
                NOW + 19_002.,
                Event::HttpEnd {
                    request: request.id,
                },
            ),
            1 => e.handle(
                NOW + 19_002.,
                Event::HttpFailed {
                    request: request.id,
                    error: "loss after valid prefix",
                },
            ),
            _ => e.handle(NOW + 20_000., Event::TimerFired { timer }),
        }
        let actions: Vec<_> = e.drain_actions().collect();
        let done = results(&actions, 611);
        assert_eq!(done.len(), 1);
        let error = done[0].as_ref().unwrap_err();
        assert_eq!(
            error.kind,
            match failure {
                // A stream that ends mid-array is a malformed reply, not loss.
                0 => Some(ErrorKind::Internal),
                1 => Some(ErrorKind::Network),
                _ => Some(ErrorKind::Timeout),
            }
        );
        assert_eq!(error.outcome, None);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_)) || matches!(a, Action::Notice(Notice::Event(event)) if event.code == "history.incomplete")));
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
        assert!(e.take_frame(NOW + 20_000.).is_none());
    }
}

#[test]
fn capacity_denial_precedes_invalid_input_and_does_not_retire_a_peer() {
    let mut e = boot(false);
    let mut tokens = Vec::new();
    for command in 1..=256 {
        tokens.push(preparation(&ask(&mut e, NOW, command)).0);
    }
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 612,
            symbol: "UNKNOWN".into(),
            interval: Interval::S1,
            limit: None,
            from_ms: Some(-1.),
            to_ms: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let done = results(&actions, 612);
    assert_eq!(done.len(), 1);
    let error = done[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    assert_eq!(error.message, "Engine request capacity reached");
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::ManagedHttp(_)
            | Action::SetTimer { .. }
            | Action::Sign { .. }
            | Action::SignPayload { .. }
    )));
    assert!(e.http_preparation_pending(tokens[1]));
    e.http_preparation_failed(
        NOW + 1.,
        tokens[0],
        Error::State,
        VenueError::new("first only").kind(ErrorKind::AuthPermission),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(matches!(results(&actions, 1).as_slice(), [Err(_)]));
    assert!(results(&actions, 2).is_empty());
    assert!(e.http_preparation_pending(tokens[1]));
    let (next, _) = preparation(&ask(&mut e, NOW + 2., 612));
    let request = release(&mut e, next, NOW + 3., false);
    reply(&mut e, NOW + 4., &request, CANDLES);
    assert!(matches!(
        results(&e.drain_actions().collect::<Vec<_>>(), 612).as_slice(),
        [Ok(_)]
    ));
    assert!(e.http_preparation_pending(tokens[1]));
}

#[test]
fn native_preflight_failures_keep_their_messages_and_unclassified_metadata() {
    for (symbol, interval, from, to, expected) in [
        (
            "BTC-PERP",
            Interval::S1,
            None,
            None,
            Interval::S1.unsupported("Hyperliquid"),
        ),
        (
            "UNKNOWN",
            Interval::M1,
            None,
            None,
            "Market not found".into(),
        ),
        (
            "BTC-PERP",
            Interval::M1,
            Some(-1.),
            None,
            "Invalid candle time range".into(),
        ),
        (
            "BTC-PERP",
            Interval::M1,
            Some(f64::NAN),
            Some(60_000.),
            "Invalid candle time range".into(),
        ),
    ] {
        let mut e = boot(false);
        e.handle(
            NOW,
            Event::Command(Command::FetchCandles {
                command: 613,
                symbol: symbol.into(),
                interval,
                limit: None,
                from_ms: from,
                to_ms: to,
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let done = results(&actions, 613);
        assert_eq!(done.len(), 1);
        let error = done[0].as_ref().unwrap_err();
        assert_eq!(error.message, expected);
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::SetTimer { .. }
        )));
    }
}

#[test]
fn merged_spot_candles_remain_on_the_existing_legacy_path() {
    let mut e = boot_modes(false, false, true);
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 614,
            symbol: "UBTC/USDC".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: Some(60_000.),
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    let request = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("held merged spot path stays legacy");
    assert_eq!(
        request.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"@1","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
    reply(&mut e, NOW + 1., &request, CANDLES);
    assert!(matches!(
        results(&e.drain_actions().collect::<Vec<_>>(), 614).as_slice(),
        [Ok(_)]
    ));
    let (token, request) = preparation(&ask(&mut e, NOW + 2., 615));
    wire(&request, false);
    let released = release(&mut e, token, NOW + 3., false);
    reply(&mut e, NOW + 4., &released, CANDLES);
    assert!(matches!(
        results(&e.drain_actions().collect::<Vec<_>>(), 615).as_slice(),
        [Ok(_)]
    ));
}

#[test]
fn native_response_errors_keep_their_exact_unclassified_metadata() {
    for (status, body, expected) in [
        (503, "unavailable", "Hyperliquid HTTP 503: unavailable"),
        (408, "timeout", "Hyperliquid HTTP 408: timeout"),
        (
            200,
            r#"[{"t":0,"o":"NaN","h":"2","l":"1","c":"2","v":"3"}]"#,
            "Malformed Hyperliquid candles",
        ),
    ] {
        let mut e = boot(false);
        let (token, _) = preparation(&ask(&mut e, NOW, 616));
        let request = release(&mut e, token, NOW + 1., false);
        e.handle(
            NOW + 2.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let failures = results(&actions, 616);
        let [Err(error)] = failures.as_slice() else {
            panic!("one native response failure: {actions:?}");
        };
        assert_eq!(error.message, expected);
        assert_eq!(error.kind, None);
        assert_eq!(error.outcome, None);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
    }
}
