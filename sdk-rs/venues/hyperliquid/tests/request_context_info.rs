mod support;

use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use std::collections::{HashSet, VecDeque};

const NOW: f64 = 1_000.;
const SPOT: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"HFUN","index":2,"szDecimals":2}],"universe":[{"name":"@1","tokens":[2,0],"index":1}]},[{"coin":"@1","midPx":"1","markPx":"1"}]]"#;

fn reply(engine: &mut Engine<Hyperliquid>, request: &HttpRequest, body: &str) {
    engine.handle(
        NOW,
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

fn release(engine: &mut Engine<Hyperliquid>, testnet: bool) -> Vec<Action> {
    engine.prepare_dispatch(NOW);
    let mut queue: VecDeque<_> = engine.drain_actions().collect();
    let mut result = Vec::new();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::Http(_) | Action::Sign { .. } | Action::SignPayload { .. } => {
                panic!("info read bypassed unsigned managed preparation")
            }
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(intent, RoutingIntent::Public);
                    assert_eq!(request.timeout_ms, Some(20_000));
                    assert_eq!(
                        request.headers,
                        [("Content-Type", "application/json".into())]
                    );
                    assert!(request.signer_headers.is_empty());
                    assert_eq!(
                        request.url,
                        if testnet {
                            "https://api.hyperliquid-testnet.xyz/info"
                        } else {
                            "https://api.hyperliquid.xyz/info"
                        }
                    );
                    engine.http_prepared(
                        NOW,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(
                        engine.claim_http(NOW, permit),
                        Ok(Claim::Released(_))
                    ));
                    result.push(Action::Http(request));
                }
                _ => {}
            },
            action => result.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    result
}

#[test]
fn real_perps_spot_and_testnet_catalogues_require_prepare_bind_and_claim() {
    for (spot, testnet) in [(false, false), (true, false), (false, true), (true, true)] {
        let options = Options {
            public_data_only: true,
            testnet,
            ..Options::default()
        };
        let venue = if spot {
            Hyperliquid::new_spot(options)
        } else {
            Hyperliquid::new(options)
        };
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine.start(NOW);
        let actions = release(&mut engine, testnet);
        let requests: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            support::kind(&requests[0]),
            if spot {
                "spotMetaAndAssetCtxs"
            } else {
                "metaAndAssetCtxs"
            }
        );
        reply(
            &mut engine,
            &requests[0],
            if spot { SPOT } else { support::META },
        );
        assert!(!engine.store().markets().is_empty());
    }
}

#[test]
fn actual_private_bootstrap_info_reads_are_unsigned_managed_calls() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    engine.start(NOW);
    let mut seen = HashSet::new();
    for _ in 0..20 {
        let actions = release(&mut engine, false);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(request) => {
                    let kind = support::kind(&request);
                    seen.insert(kind.clone());
                    let body = match kind.as_str() {
                        "metaAndAssetCtxs" => support::META,
                        "perpDexs" => "[null]",
                        "userAbstraction" => r#""disabled""#,
                        "maxBuilderFee" => "30",
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                        "clearinghouseState" => support::STATE,
                        "frontendOpenOrders" => "[]",
                        other => panic!("unexpected native request {other}"),
                    };
                    reply(&mut engine, &request, body);
                }
                Action::WsConnect { socket, .. } => engine.handle(NOW, Event::WsOpened { socket }),
                _ => {}
            }
        }
    }
    for kind in [
        "metaAndAssetCtxs",
        "perpDexs",
        "userAbstraction",
        "maxBuilderFee",
        "userFees",
        "clearinghouseState",
        "frontendOpenOrders",
    ] {
        assert!(seen.contains(kind), "real bootstrap must issue {kind}");
    }
    assert_eq!(engine.store().positions().len(), 1);
    assert_eq!(engine.store().balance().total, 1_000.);
}

#[test]
fn preparation_at_deadline_does_not_publish_and_native_discovery_can_retry() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(NOW);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (preparation, request) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("real metadata preparation");
    engine.http_prepared(
        NOW + 20_000.,
        preparation,
        Prepared {
            id: 77,
            expires_at: None,
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(!actions.iter().any(|a| match a {
        Action::Http(_) => true,
        Action::ManagedHttp(action) => matches!(action.as_ref(), HttpAction::Bind { .. }),
        _ => false,
    }));
    engine.handle(
        NOW + 20_001.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::META,
            },
        },
    );
    assert!(engine.store().markets().is_empty());
    let (timer, at) = actions
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("native discovery retry");
    engine.handle(at, Event::TimerFired { timer });
    assert!(engine.drain_actions().any(|a| match a {
        Action::ManagedHttp(action) => matches!(action.as_ref(),
            HttpAction::Prepare { request: next, .. } if next.id != request.id),
        _ => false,
    }));
}
