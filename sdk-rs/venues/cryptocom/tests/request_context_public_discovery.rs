//! Original real-Engine controls for unsigned discovery Read custody.
mod support;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, TimerId, VenueError,
};
use safe_cex_venue_cryptocom::{Cryptocom, Options};
use support::{INSTRUMENTS, TICKERS, jittered};

type TestEngine = Engine<Cryptocom>;
const SYMBOL: &str = "BTCUSD-PERP";

fn new() -> TestEngine {
    safe_cex_engine::test_support::seeded_engine(Cryptocom::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
fn actions(e: &mut TestEngine, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn packet(output: &[Action], path: &str) -> HttpRequest {
    let mut reads = output.iter().filter_map(|a| match a {
        Action::Http(r) if r.url.ends_with(path) => Some(r.clone()),
        _ => None,
    });
    let read = reads.next().expect("one physical public request");
    assert!(reads.next().is_none());
    assert_eq!(read.method, Method::Get);
    assert!(read.headers.is_empty());
    assert!(read.signer_headers.is_empty());
    assert!(read.body.is_none());
    read
}
fn reply(e: &mut TestEngine, at: f64, read: &HttpRequest, status: u16, body: &str) {
    e.handle(
        at,
        Event::HttpResponse {
            request: read.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn markets() -> (TestEngine, HttpRequest) {
    let mut e = new();
    e.start(0.);
    let read = packet(&actions(&mut e, 0.), "/public/get-instruments");
    (e, read)
}
fn tickers() -> (TestEngine, HttpRequest) {
    let (mut e, read) = markets();
    reply(&mut e, 1., &read, 200, INSTRUMENTS);
    let read = packet(&actions(&mut e, 1.), "/public/get-tickers");
    (e, read)
}
fn pending(e: &mut TestEngine, path: &str, now: f64) -> (Preparation, HttpRequest, TimerId) {
    let output: Vec<_> = e.drain_actions().collect();
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "public discovery waits for real route preparation"
    );
    let (preparation, read) = output
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } if request.url.ends_with(path) => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("one genuine Public preparation");
    assert_eq!(read.timeout_ms, Some(20_000));
    let deadline = output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("deadline starts at issue, before route readiness");
    (preparation, read, deadline)
}
fn retry(output: &[Action], now: f64, delay: f64) -> (TimerId, f64) {
    let timers: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - now, delay) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "one native discovery backoff");
    timers[0]
}
fn release(e: &mut TestEngine, at: f64, preparation: Preparation) -> HttpRequest {
    e.http_prepared(
        at,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, read) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared route binds before physical release");
    assert!(matches!(e.claim_http(at, permit), Ok(Claim::Released(_))));
    e.drain_actions().for_each(drop);
    read
}
fn no_command_or_auth(output: &[Action]) {
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Sign { .. }
            | Action::Notice(
                Notice::CommandDone { .. }
                    | Notice::CommandFailed { .. }
                    | Notice::AccountAuth { .. }
            )
    )));
}
fn peer(e: &mut TestEngine, now: f64, owner: u32) -> Preparation {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
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
        .expect("actual managed candle read owns one slot")
}

#[test]
fn catalogue_waits_for_public_preparation() {
    let mut e = new();
    e.start(0.);
    let (_, read, _) = pending(&mut e, "/public/get-instruments", 0.);
    assert_eq!(
        read.url,
        "https://api.crypto.com/exchange/v1/public/get-instruments"
    );
}
#[test]
fn ticker_waits_for_public_preparation() {
    let (mut e, read) = markets();
    reply(&mut e, 1., &read, 200, INSTRUMENTS);
    let (_, read, _) = pending(&mut e, "/public/get-tickers", 1.);
    assert_eq!(
        read.url,
        "https://api.crypto.com/exchange/v1/public/get-tickers"
    );
}
#[test]
fn parked_catalogue_expires_and_late_route_cannot_release() {
    let mut e = new();
    e.start(0.);
    let (preparation, read, deadline) = pending(&mut e, "/public/get-instruments", 0.);
    e.handle(20_000., Event::TimerFired { timer: deadline });
    let output: Vec<_> = e.drain_actions().collect();
    retry(&output, 20_000., 1000.);
    no_command_or_auth(&output);
    e.http_prepared(
        20_001.,
        preparation,
        Prepared {
            id: read.id.0,
            expires_at: None,
        },
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    reply(&mut e, 20_002., &read, 200, INSTRUMENTS);
    assert!(!e.store().loaded().markets);
}
#[test]
fn preparation_refusal_preserves_native_retry_without_auth_evidence() {
    let mut e = new();
    e.start(0.);
    let (preparation, read, _) = pending(&mut e, "/public/get-instruments", 0.);
    e.http_preparation_failed(
        1.,
        preparation,
        Error::State,
        VenueError::new("route denied"),
    );
    let output: Vec<_> = e.drain_actions().collect();
    no_command_or_auth(&output);
    let (timer, at) = retry(&output, 1., 1000.);
    e.handle(at, Event::TimerFired { timer });
    let fresh = packet(&actions(&mut e, at), "/public/get-instruments");
    assert_ne!(fresh.id, read.id);
    reply(&mut e, at, &read, 200, INSTRUMENTS);
    assert!(!e.store().loaded().markets);
}
#[test]
fn catalogue_at_deadline_cannot_publish_without_timer_ingress() {
    let (mut e, read) = markets();
    reply(&mut e, 20_000., &read, 200, INSTRUMENTS);
    assert!(!e.store().loaded().markets, "DATA expires at equality");
    let output: Vec<_> = e.drain_actions().collect();
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::WsConnect { .. }
    )));
    no_command_or_auth(&output);
}
#[test]
fn catalogue_deadline_timer_then_response_cannot_publish() {
    let mut e = new();
    e.start(0.);
    let (preparation, _, deadline) = pending(&mut e, "/public/get-instruments", 0.);
    let read = release(&mut e, 0., preparation);
    e.handle(20_000., Event::TimerFired { timer: deadline });
    let output: Vec<_> = e.drain_actions().collect();
    retry(&output, 20_000., 1000.);
    reply(&mut e, 20_000., &read, 200, INSTRUMENTS);
    assert!(!e.store().loaded().markets);
}
#[test]
fn expired_ticker_retains_cached_quotes_and_cannot_open_socket() {
    let (mut e, read) = tickers();
    e.store_mut()
        .set_tickers(vec![safe_cex_engine::model::Ticker {
            id: SYMBOL.into(),
            symbol: SYMBOL.into(),
            last: 333.,
            ..Default::default()
        }]);
    reply(&mut e, 20_001., &read, 200, TICKERS);
    assert_eq!(e.store().tickers()[0].last, 333.);
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}
#[test]
fn full_owner_capacity_denies_discovery_and_recovers_without_replacing_peers() {
    let mut e = new();
    let peers: Vec<_> = (1..=256).map(|id| peer(&mut e, 0., id)).collect();
    e.start(0.);
    let output: Vec<_> = e.drain_actions().collect();
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
        "capacity denies public issue"
    );
    no_command_or_auth(&output);
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    let (timer, at) = retry(&output, 0., 1000.);
    e.http_preparation_failed(1., peers[0], Error::State, VenueError::new("free one peer"));
    e.drain_actions().for_each(drop);
    e.handle(at, Event::TimerFired { timer });
    packet(&actions(&mut e, at), "/public/get-instruments");
    assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
}
#[test]
fn catalogue_handoff_releases_slot_before_ticker_at_capacity() {
    let (mut e, read) = markets();
    let peers: Vec<_> = (1..=255).map(|id| peer(&mut e, 0., id)).collect();
    reply(&mut e, 1., &read, 200, INSTRUMENTS);
    packet(&actions(&mut e, 1.), "/public/get-tickers");
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
}
#[test]
fn native_public_wire_and_values_are_unchanged() {
    let (mut e, read) = tickers();
    reply(&mut e, 2., &read, 200, TICKERS);
    let output = actions(&mut e, 2.);
    assert_eq!(e.store().markets()[0].base, "BTC");
    assert_eq!(e.store().markets()[0].precision.amount, 0.001);
    assert_eq!(e.store().tickers()[0].last, 100.);
    assert!(output.iter().any(|a| matches!(a, Action::WsConnect { .. })));
    no_command_or_auth(&output);
}
#[test]
fn native_testnet_public_wire_stays_unsigned() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Cryptocom::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.);
    let read = packet(&actions(&mut e, 0.), "/public/get-instruments");
    assert_eq!(
        read.url,
        "https://uat-api.3ona.co/exchange/v1/public/get-instruments"
    );
}
#[test]
fn native_catalogue_transport_retries_keep_backoff_and_old_callbacks_inert() {
    let (mut e, mut read) = markets();
    let mut now = 0.;
    for delay in [1000., 2000., 4000.] {
        e.handle(
            now,
            Event::HttpFailed {
                request: read.id,
                error: "scripted failure",
            },
        );
        let output: Vec<_> = e.drain_actions().collect();
        no_command_or_auth(&output);
        let (timer, at) = retry(&output, now, delay);
        e.handle(at, Event::TimerFired { timer });
        let fresh = packet(&actions(&mut e, at), "/public/get-instruments");
        assert_ne!(fresh.id, read.id);
        reply(&mut e, at, &read, 200, INSTRUMENTS);
        assert!(!e.store().loaded().markets);
        read = fresh;
        now = at;
    }
}
#[test]
fn sibling_issuance_cannot_suppress_expired_ticker_failure_recovery() {
    let (mut e, read) = tickers();
    let sibling = peer(&mut e, 20_002., 900);
    e.handle(
        20_003.,
        Event::HttpFailed {
            request: read.id,
            error: "late host failure after sibling issuance",
        },
    );
    let output: Vec<_> = e.drain_actions().collect();
    no_command_or_auth(&output);
    let (timer, at) = retry(&output, 20_003., 1000.);
    assert!(e.http_preparation_pending(sibling));
    e.handle(at, Event::TimerFired { timer });
    let fresh = packet(&actions(&mut e, at), "/public/get-tickers");
    assert_ne!(fresh.id, read.id);
    assert!(e.http_preparation_pending(sibling));
}
#[test]
fn native_ticker_failure_is_independent_and_retains_cached_quote() {
    let (mut e, read) = tickers();
    e.store_mut()
        .set_tickers(vec![safe_cex_engine::model::Ticker {
            id: SYMBOL.into(),
            symbol: SYMBOL.into(),
            last: 333.,
            ..Default::default()
        }]);
    reply(&mut e, 2., &read, 503, "{}");
    assert_eq!(e.store().tickers()[0].last, 333.);
    let output: Vec<_> = e.drain_actions().collect();
    let (timer, at) = retry(&output, 2., 1000.);
    e.handle(at, Event::TimerFired { timer });
    packet(&actions(&mut e, at), "/public/get-tickers");
    assert!(e.store().loaded().markets);
}
#[test]
fn native_usable_catalogue_peers_commit_and_private_reads_start_before_tickers() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Cryptocom::new(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    }));
    e.start(0.);
    let read = packet(&actions(&mut e, 0.), "/public/get-instruments");
    let partial = INSTRUMENTS.replace("\"data\":[", "\"data\":[{\"symbol\":\"BROKENUSD-PERP\",\"inst_type\":\"PERPETUAL_SWAP\",\"tradable\":true},");
    reply(&mut e, 1., &read, 200, &partial);
    let output = actions(&mut e, 1.);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    packet(&output, "/public/get-tickers");
    assert!(
        output
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/private/")))
    );
    assert!(!e.store().loaded().tickers);
}
