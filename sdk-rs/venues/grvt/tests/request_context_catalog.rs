//! Actual GRVT catalog boundary: public POSTs park until the host's final claim.
//! Draft target: sdk-rs/venues/grvt/tests/request_context_catalog.rs.
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Response, VenueError,
};
use safe_cex_venue_grvt::{Grvt, Options};

const NOW: f64 = 1_000.0;
// Native lite instrument fixture already exercised by startup_resilience.rs.
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;

fn engine(testnet: bool) -> Engine<Grvt> {
    safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        testnet,
        public_data_only: true,
        ..Options::default()
    }))
}

fn assert_wire(request: &HttpRequest, testnet: bool) {
    let root = if testnet {
        "https://market-data.testnet.grvt.io"
    } else {
        "https://market-data.grvt.io"
    };
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, format!("{root}/lite/v1/all_instruments"));
    assert_eq!(request.body.as_deref(), Some("{}"));
    assert_eq!(
        request.headers,
        [("Content-Type", "application/json".into())]
    );
    assert!(request.signer_headers.is_empty());
}

fn assert_no_transport_or_socket(actions: &[Action]) {
    assert!(
        !actions.iter().any(|action| matches!(
            action,
            Action::Http(_)
                | Action::WsConnect { .. }
                | Action::WsSend { .. }
                | Action::Sign { .. }
                | Action::SignPayload { .. }
        )),
        "catalog preparation cannot send HTTP, sign, or start subscriptions"
    );
}

fn assert_no_command_or_auth(actions: &[Action]) {
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(
            Notice::CommandDone { .. } | Notice::CommandFailed { .. } | Notice::AccountAuth { .. }
        )
    )));
}

fn assert_unpublished(engine: &Engine<Grvt>) {
    assert!(!engine.store().loaded().markets);
    assert!(engine.store().markets().is_empty());
    assert!(engine.store().tickers().is_empty());
}

fn preparation(actions: &[Action], testnet: bool) -> (Preparation, HttpRequest) {
    // Current production fails here: it emits Action::Http directly.
    assert_no_transport_or_socket(actions);
    assert_no_command_or_auth(actions);
    let mut prepares = actions.iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match action.as_ref() {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => Some((*preparation, *intent, request.clone())),
            _ => None,
        },
        _ => None,
    });
    let (token, intent, request) = prepares.next().expect("catalog awaits managed preparation");
    assert!(prepares.next().is_none(), "one catalog attempt is parked");
    assert_eq!(intent, RoutingIntent::Public);
    assert_eq!(request.id, token.attempt().request);
    assert_wire(&request, testnet);
    (token, request)
}

fn release(engine: &mut Engine<Grvt>, token: Preparation, testnet: bool, now: f64) -> HttpRequest {
    engine.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_no_transport_or_socket(&actions);
    assert_no_command_or_auth(&actions);
    let mut binds = actions.into_iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Bind {
                preparation,
                permit,
                request,
            } => Some((preparation, permit, request)),
            _ => None,
        },
        _ => None,
    });
    let (bound, permit, request) = binds.next().expect("prepared catalog binds before claim");
    assert!(binds.next().is_none(), "one route binds this attempt");
    assert_eq!(bound, token);
    assert_eq!(request.id, token.attempt().request);
    assert_wire(&request, testnet);
    assert_unpublished(engine);
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    request
}

fn reply(engine: &mut Engine<Grvt>, request: &HttpRequest, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOG,
            },
        },
    );
}

fn assert_native_success(engine: &Engine<Grvt>, actions: &[Action], testnet: bool) {
    assert!(engine.store().loaded().markets);
    assert_eq!(engine.store().markets().len(), 1);
    let market = engine.store().market("BTC_USDT_PERP").unwrap();
    assert_eq!(market.id, "BTC_USDT_Perp");
    assert_eq!(market.precision.price, 0.1);
    assert_eq!(market.precision.amount, 0.001);
    assert_eq!(engine.store().tickers().len(), 1);
    assert_no_command_or_auth(actions);
    let expected = if testnet {
        "wss://market-data.testnet.grvt.io/ws/full"
    } else {
        "wss://market-data.grvt.io/ws/full"
    };
    let sockets: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::WsConnect { url, .. } => Some(url.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sockets, [expected]);
}

#[test]
fn public_catalog_post_keeps_json_headers_and_cannot_publish_before_claim() {
    for testnet in [false, true] {
        let mut engine = engine(testnet);
        engine.start(NOW);
        let actions: Vec<_> = engine.drain_actions().collect();
        let (token, unsigned) = preparation(&actions, testnet);
        assert_unpublished(&engine);
        reply(&mut engine, &unsigned, NOW + 1.0);
        engine.handle(
            NOW + 1.0,
            Event::HttpStart {
                request: unsigned.id,
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            NOW + 1.0,
            Event::HttpChunk {
                request: unsigned.id,
                data: CATALOG.as_bytes(),
            },
        );
        engine.handle(
            NOW + 1.0,
            Event::HttpEnd {
                request: unsigned.id,
            },
        );
        assert_unpublished(&engine);
        let early: Vec<_> = engine.drain_actions().collect();
        assert_no_transport_or_socket(&early);
        assert_no_command_or_auth(&early);
        assert!(engine.http_preparation_pending(token));

        let request = release(&mut engine, token, testnet, NOW + 2.0);
        assert_unpublished(&engine);
        let claimed: Vec<_> = engine.drain_actions().collect();
        assert_no_transport_or_socket(&claimed);
        reply(&mut engine, &request, NOW + 3.0);
        let completed: Vec<_> = engine.drain_actions().collect();
        assert_native_success(&engine, &completed, testnet);
    }
}

#[test]
fn denied_public_catalog_retires_and_recovers_through_existing_startup_backoff() {
    let testnet = true;
    let mut engine = engine(testnet);
    engine.start(NOW);
    let (old, unsigned) = preparation(&engine.drain_actions().collect::<Vec<_>>(), testnet);
    let denied_at = NOW + 1.0;
    engine.http_preparation_failed(
        denied_at,
        old,
        Error::State,
        VenueError::new("public route unavailable").kind(ErrorKind::VenueUnavailable),
    );
    let denied: Vec<_> = engine.drain_actions().collect();
    assert_unpublished(&engine);
    assert_no_transport_or_socket(&denied);
    assert_no_command_or_auth(&denied);
    assert!(!engine.http_preparation_pending(old));
    assert!(
        !denied.iter().any(|action| matches!(
            action,
            Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
        )),
        "catalog recovery waits for the existing Backoff timer"
    );
    let retries: Vec<_> = denied
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms }
                if (750.0..=1000.0).contains(&(*at_ms - denied_at)) =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect();
    assert_eq!(retries.len(), 1, "one jittered first startup-backoff timer");
    let (timer, retry_at) = retries[0];
    engine.handle(retry_at, Event::TimerFired { timer });
    let (next, _) = preparation(&engine.drain_actions().collect::<Vec<_>>(), testnet);
    assert_ne!(old, next);
    assert_unpublished(&engine);

    // A late readiness or body from the denied attempt cannot consume its successor.
    engine.http_prepared(
        retry_at + 1.0,
        old,
        Prepared {
            id: old.attempt().request.0,
            expires_at: None,
        },
    );
    reply(&mut engine, &unsigned, retry_at + 1.0);
    let late: Vec<_> = engine.drain_actions().collect();
    assert_no_transport_or_socket(&late);
    assert_no_command_or_auth(&late);
    assert_unpublished(&engine);
    assert!(engine.http_preparation_pending(next));

    let request = release(&mut engine, next, testnet, retry_at + 2.0);
    reply(&mut engine, &request, retry_at + 3.0);
    let recovered: Vec<_> = engine.drain_actions().collect();
    assert_native_success(&engine, &recovered, testnet);
}
