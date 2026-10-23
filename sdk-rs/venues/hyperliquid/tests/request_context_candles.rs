//! Real perps candles must preserve JSON headers through managed preparation.
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        testnet,
        ..Options::default()
    }));
    e.start(NOW);
    // Discovery is deliberately still legacy: this slice changes candles only.
    let metadata = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.body.as_deref() == Some(r#"{"type":"metaAndAssetCtxs"}"#) => {
                Some(r)
            }
            _ => None,
        })
        .expect("legacy catalog seed");
    reply(&mut e, NOW, &metadata, META);
    assert_eq!(e.store().market("BTC-PERP").unwrap().id, "BTC");
    let dexs = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.body.as_deref() == Some(r#"{"type":"perpDexs"}"#) => Some(r),
            _ => None,
        })
        .expect("legacy dex discovery");
    reply(&mut e, NOW, &dexs, "[null]");
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
    let (permit, request) = actions
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
        .expect("public prepared packet binds without signing");
    wire(&request, testnet);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
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
