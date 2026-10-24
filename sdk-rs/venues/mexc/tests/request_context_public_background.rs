//! Original native controls for unsigned discovery and subscription snapshots.
mod support;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation, Prepared, RoutingIntent};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, HttpRequest, Notice, Response, SocketId, TimerId, VenueError,
};
use safe_cex_venue_mexc::{MexcFutures, Options};
use support::{MARKETS, TICKERS, TestEngine, jittered};

const DEPTH: &str = r#"{"success":true,"code":0,"data":{"version":10,"bids":[[60000,10,1]],"asks":[[60001,40,1]]}}"#;

fn new() -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(MexcFutures::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
fn actions(e: &mut TestEngine, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn packet(actions: &[Action], path: &str) -> HttpRequest {
    let mut reads = actions.iter().filter_map(|action| match action {
        Action::Http(r) if r.url.contains(path) => Some(r.clone()),
        _ => None,
    });
    let read = reads.next().expect("one physical public request");
    assert!(reads.next().is_none());
    assert_eq!(read.method, safe_cex_engine::Method::Get);
    assert!(read.signer_headers.is_empty());
    assert!(read.headers.is_empty());
    assert!(read.body.is_none());
    read
}
fn reply(e: &mut TestEngine, now: f64, r: &HttpRequest, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn markets() -> (TestEngine, HttpRequest) {
    let mut e = new();
    e.start(0.);
    let r = packet(&actions(&mut e, 0.), "contract/detail");
    (e, r)
}
fn tickers() -> (TestEngine, HttpRequest) {
    let (mut e, market) = markets();
    reply(&mut e, 100., &market, MARKETS);
    let r = packet(&actions(&mut e, 100.), "contract/ticker");
    (e, r)
}
fn started() -> (TestEngine, SocketId) {
    let (mut e, ticker) = tickers();
    reply(&mut e, 100., &ticker, TICKERS);
    let socket = actions(&mut e, 100.)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("public socket");
    e.handle(100., Event::WsOpened { socket });
    actions(&mut e, 100.);
    (e, socket)
}
fn subscribe(e: &mut TestEngine, now: f64) {
    e.handle(
        now,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
}
fn snapshot() -> (TestEngine, SocketId, HttpRequest) {
    let (mut e, socket) = started();
    subscribe(&mut e, 200.);
    let r = packet(&actions(&mut e, 200.), "contract/depth/");
    (e, socket, r)
}
fn pending_route(e: &mut TestEngine, path: &str, now: f64) -> (Preparation, HttpRequest, TimerId) {
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "unsigned background read must wait for Public preparation and final claim"
    );
    let (preparation, request) = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } if request.url.contains(path) => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("public preparation");
    assert_eq!(request.timeout_ms, Some(20_000));
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("deadline starts before route readiness");
    (preparation, request, timer)
}
fn retry(actions: &[Action], now: f64, nominal: f64) -> (TimerId, f64) {
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - now, nominal) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "exactly one native retry");
    timers[0]
}
fn no_command_or_auth(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { .. } | Notice::CommandFailed { .. } | Notice::AccountAuth { .. }
        ) | Action::Sign { .. }
    )));
}

#[test]
fn catalogue_waits_for_public_route_without_eager_http() {
    let mut e = new();
    e.start(0.);
    let (_, r, _) = pending_route(&mut e, "contract/detail", 0.);
    assert_eq!(r.url, "https://api.mexc.com/api/v1/contract/detail");
}
#[test]
fn tickers_wait_for_public_route_without_eager_http() {
    let (mut e, market) = markets();
    reply(&mut e, 100., &market, MARKETS);
    let (_, r, _) = pending_route(&mut e, "contract/ticker", 100.);
    assert_eq!(r.url, "https://api.mexc.com/api/v1/contract/ticker");
}
#[test]
fn depth_waits_for_public_route_without_eager_http() {
    let (mut e, _) = started();
    subscribe(&mut e, 200.);
    let (_, r, _) = pending_route(&mut e, "contract/depth/", 200.);
    assert_eq!(
        r.url,
        "https://api.mexc.com/api/v1/contract/depth/BTC_USDT?limit=1000"
    );
}
#[test]
fn parked_catalogue_expiry_retries_once_and_late_readiness_cannot_release() {
    let mut e = new();
    e.start(0.);
    let (preparation, r, deadline) = pending_route(&mut e, "contract/detail", 0.);
    e.handle(20_000., Event::TimerFired { timer: deadline });
    let a: Vec<_> = e.drain_actions().collect();
    let _ = retry(&a, 20_000., 1000.);
    no_command_or_auth(&a);
    e.http_prepared(
        20_001.,
        preparation,
        Prepared {
            id: r.id.0,
            expires_at: None,
        },
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    reply(&mut e, 20_002., &r, MARKETS);
    assert!(!e.store().loaded().markets);
}
#[test]
fn preparation_refusal_keeps_native_startup_retry_without_auth_evidence() {
    let mut e = new();
    e.start(0.);
    let (preparation, r, _) = pending_route(&mut e, "contract/detail", 0.);
    e.http_preparation_failed(
        1.,
        preparation,
        Error::State,
        VenueError::new("public route unavailable").kind(ErrorKind::AuthPermission),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let (timer, at) = retry(&a, 1., 1000.);
    no_command_or_auth(&a);
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    e.handle(at, Event::TimerFired { timer });
    let next = packet(&actions(&mut e, at), "contract/detail");
    assert_ne!(r.id, next.id);
    reply(&mut e, at, &r, MARKETS);
    assert!(!e.store().loaded().markets);
    reply(&mut e, at + 1., &next, MARKETS);
    assert!(e.store().loaded().markets);
}
#[test]
fn catalogue_at_deadline_cannot_publish_before_timer_ingress() {
    let (mut e, r) = markets();
    reply(&mut e, 20_000., &r, MARKETS);
    assert!(
        !e.store().loaded().markets,
        "expired catalogue must not publish DATA"
    );
    let _ = retry(&e.drain_actions().collect::<Vec<_>>(), 20_000., 1000.);
}
#[test]
fn ticker_at_deadline_cannot_publish_before_timer_ingress() {
    let (mut e, r) = tickers();
    reply(&mut e, 20_100., &r, TICKERS);
    assert!(
        !e.store().loaded().tickers,
        "expired tickers must not publish DATA"
    );
    let _ = retry(&e.drain_actions().collect::<Vec<_>>(), 20_100., 1000.);
}
#[test]
fn depth_at_deadline_cannot_publish_before_timer_ingress() {
    let (mut e, _, r) = snapshot();
    reply(&mut e, 20_200., &r, DEPTH);
    assert!(
        e.book("BTCUSDT").is_none(),
        "expired snapshot must not publish DATA"
    );
    let _ = depth_retry(&e.drain_actions().collect::<Vec<_>>(), 20_200.);
}
#[test]
fn live_catalogue_rows_do_not_publish_when_stream_end_is_expired() {
    let (mut e, r) = markets();
    e.handle(
        1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        2.,
        Event::HttpChunk {
            request: r.id,
            data: MARKETS.as_bytes(),
        },
    );
    assert!(!e.store().loaded().markets);
    e.handle(20_000., Event::HttpEnd { request: r.id });
    assert!(
        !e.store().loaded().markets,
        "live prefix cannot authorize an expired stream end"
    );
}
#[test]
fn native_unsigned_discovery_keeps_urls_rows_and_volume_conversion() {
    let (e, _) = started();
    let m = e.store().market("BTCUSDT").unwrap();
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(m.contract_size, Some(0.0001));
    assert_eq!((t.last, t.volume, t.percentage), (60000., 5., 1.));
}
#[test]
fn native_live_depth_replays_buffered_versions_and_scaled_amounts() {
    let (mut e, socket, r) = snapshot();
    e.handle(201., Event::WsMessage { socket, text: r#"{"channel":"push.depth","symbol":"BTC_USDT","data":{"version":11,"bids":[[60000,30,1]],"asks":[]}}"# });
    reply(&mut e, 202., &r, DEPTH);
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(60000., 0.003)]);
    assert_eq!(e.book("BTCUSDT").unwrap().asks(), [(60001., 0.004)]);
    no_command_or_auth(&actions(&mut e, 202.));
}
#[test]
fn native_unsubscribe_generation_fences_old_snapshot_and_keeps_new_peer() {
    let (mut e, _, old) = snapshot();
    e.handle(
        201.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    subscribe(&mut e, 202.);
    let new = packet(&actions(&mut e, 202.), "contract/depth/");
    reply(&mut e, 203., &old, DEPTH);
    assert!(e.book("BTCUSDT").is_none());
    reply(&mut e, 204., &new, DEPTH);
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(60000.));
}
#[test]
fn native_catalogue_network_retry_preserves_backoff_and_fresh_operation() {
    let (mut e, mut r) = markets();
    let mut now = 1.;
    for nominal in [1000., 2000., 4000.] {
        e.handle(
            now,
            Event::HttpFailed {
                request: r.id,
                error: "network reset",
            },
        );
        let a: Vec<_> = e.drain_actions().collect();
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
        no_command_or_auth(&a);
        let (timer, at) = retry(&a, now, nominal);
        e.handle(at, Event::TimerFired { timer });
        let new = packet(&actions(&mut e, at), "contract/detail");
        assert_ne!(r.id, new.id);
        r = new;
        now = at + 1.;
    }
    reply(&mut e, now, &r, MARKETS);
    assert!(e.store().loaded().markets);
}
#[test]
fn native_depth_429_keeps_retry_after_outside_generation_data_gate() {
    let (mut e, _, old) = snapshot();
    e.handle(
        201.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    subscribe(&mut e, 202.);
    let new = packet(&actions(&mut e, 202.), "contract/depth/");
    e.handle(
        203.,
        Event::HttpResponse {
            request: old.id,
            response: Response {
                status: 429,
                headers: "retry-after: 5\r\n",
                body: "{}",
            },
        },
    );
    reply(&mut e, 204., &new, DEPTH);
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(60000.));
    e.handle(
        205.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    subscribe(&mut e, 206.);
    let a = actions(&mut e, 206.);
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 5203.))
    );
}

fn depth_retry(actions: &[Action], now: f64) -> (TimerId, f64) {
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 500. => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        timers.len(),
        1,
        "first native depth resync is exactly 500ms"
    );
    timers[0]
}

fn depth_failure_recovery(expire: bool) {
    let mut catalogue: serde_json::Value = serde_json::from_str(MARKETS).unwrap();
    let mut eth = catalogue["data"][0].clone();
    eth["symbol"] = "ETH_USDT".into();
    eth["baseCoin"] = "ETH".into();
    catalogue["data"].as_array_mut().unwrap().push(eth);
    let (mut e, market) = markets();
    reply(&mut e, 100., &market, &catalogue.to_string());
    let tick = packet(&actions(&mut e, 100.), "contract/ticker");
    reply(&mut e, 100., &tick, TICKERS);
    let socket = actions(&mut e, 100.)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(100., Event::WsOpened { socket });
    actions(&mut e, 100.);
    subscribe(&mut e, 200.);
    let a = actions(&mut e, 200.);
    let old = packet(&a, "depth/BTC_USDT");
    let deadline = a.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms == 20_200. => Some(*timer),
        _ => None,
    });
    e.handle(
        201.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETHUSDT".into(),
        }),
    );
    let peer = packet(&actions(&mut e, 201.), "depth/ETH_USDT");
    let failure_now = if expire { 20_200. } else { 20_199. };
    if expire {
        assert!(
            deadline.is_some(),
            "background snapshot needs an immutable engine deadline"
        );
        e.handle(
            failure_now,
            Event::TimerFired {
                timer: deadline.unwrap(),
            },
        );
    } else {
        e.handle(
            failure_now,
            Event::HttpFailed {
                request: old.id,
                error: "depth disconnected",
            },
        );
    }
    let a: Vec<_> = e.drain_actions().collect();
    let (timer, at) = depth_retry(&a, failure_now);
    no_command_or_auth(&a);
    reply(&mut e, failure_now, &peer, DEPTH);
    assert_eq!(e.book("ETHUSDT").unwrap().best_bid(), Some(60000.));
    e.handle(at, Event::TimerFired { timer });
    let fresh = packet(&actions(&mut e, at), "depth/BTC_USDT");
    reply(&mut e, at + 1., &old, DEPTH);
    assert!(e.book("BTCUSDT").is_none());
    assert_eq!(e.book("ETHUSDT").unwrap().best_bid(), Some(60000.));
    reply(&mut e, at + 2., &fresh, DEPTH);
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(60000.));
}

#[test]
fn depth_preparation_refusal_releases_buffering_for_native_retry() {
    let (mut e, _) = started();
    subscribe(&mut e, 200.);
    let (preparation, old, _) = pending_route(&mut e, "contract/depth/", 200.);
    e.http_preparation_failed(
        201.,
        preparation,
        Error::State,
        VenueError::new("public depth route refused").kind(ErrorKind::Unsupported),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let (timer, at) = depth_retry(&a, 201.);
    no_command_or_auth(&a);
    e.handle(at, Event::TimerFired { timer });
    let fresh = packet(&actions(&mut e, at), "contract/depth/");
    assert_ne!(old.id, fresh.id);
    reply(&mut e, at + 1., &old, DEPTH);
    assert!(e.book("BTCUSDT").is_none());
    reply(&mut e, at + 2., &fresh, DEPTH);
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(60000.));
}

#[test]
fn native_invalid_catalogue_envelope_precedes_retry_after_evidence() {
    let (mut e, first) = markets();
    e.handle(
        1.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 429,
                headers: "retry-after: 5\r\n",
                body: r#"{"success":false,"code":510,"data":[]}"#,
            },
        },
    );
    let (timer, at) = retry(&e.drain_actions().collect::<Vec<_>>(), 1., 1000.);
    e.handle(at, Event::TimerFired { timer });
    let next = packet(&actions(&mut e, at), "contract/detail");
    reply(&mut e, at, &next, MARKETS);
    let tick = packet(&actions(&mut e, at), "contract/ticker");
    reply(&mut e, at + 1., &tick, TICKERS);
    let socket = actions(&mut e, at + 1.)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(at + 1., Event::WsOpened { socket });
    actions(&mut e, at + 1.);
    subscribe(&mut e, at + 2.);
    let _ = packet(&actions(&mut e, at + 2.), "contract/depth/");
}

#[test]
fn depth_expiry_cleans_up_then_retries_without_harming_live_peer() {
    depth_failure_recovery(true);
}
#[test]
fn native_depth_transport_failure_retries_without_harming_live_peer() {
    depth_failure_recovery(false);
}
