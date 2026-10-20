//! Scripted discovery regressions; no network or account credentials.
use safe_cex_engine::{Action, Engine, Event, Notice, Response};
use safe_cex_venue_orderly::{Options, Orderly};
const CATALOGUE: &str = r###"{"success":true,"data":{"rows":[{"broken":true},{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01},{"symbol":"PERP_ETH_USDC","base_tick":0.001,"quote_tick":0.01,"base_min":0.001,"base_max":100,"base_imr":0.02}]}}"###;
fn public_engine() -> Engine<Orderly> {
    Engine::new(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }))
}
#[test]
fn catalogue_network_failures_back_off_then_keep_valid_rows() {
    let mut engine = public_engine();
    engine.start(0.0);
    let mut request = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/public/info") => Some(request),
            _ => None,
        })
        .expect("catalogue request");
    let mut now = 0.0;
    for delay in [1000.0, 2000.0, 4000.0] {
        engine.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "scripted network failure",
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "no immediate retry"
        );
        let timer = actions
            .into_iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if at_ms == now + delay => Some(timer),
                _ => None,
            })
            .expect("shared exponential retry timer");
        now += delay;
        engine.handle(now, Event::TimerFired { timer });
        request = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("/public/info") => Some(request),
                _ => None,
            })
            .expect("fresh catalogue request");
    }
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOGUE,
            },
        },
    );
    assert!(engine.store().loaded().markets, "valid peers must commit");
    assert!(!engine.store().markets().is_empty());
    assert!(engine.drain_actions().any(|action| matches!(action,
        Action::Notice(Notice::Log { message, .. }) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn authenticated_discovery_starts_private_reads_before_tickers() {
    let mut engine = Engine::new(Orderly::new(Options {
        key: Some("ed25519:11111111111111111111111111111111".into()),
        secret: Some(bs58::encode([7; 32]).into_string()),
        application_id: Some("scripted-account".into()),
        ..Default::default()
    }));
    engine.start(0.0);
    let info = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: info.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"data":{"max_leverage":20}}"#,
            },
        },
    );
    let catalogue = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/public/info") => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: CATALOGUE,
            },
        },
    );
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().tickers);
    assert!(
        engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/client/margin_modes")))
    );
}

#[test]
fn ticker_http_failure_retries_without_reloading_the_catalogue() {
    let mut engine = public_engine();
    engine.start(0.0);
    let catalogue = engine.drain_actions().find_map(|a| match a { Action::Http(r) => Some(r), _ => None }).unwrap();
    engine.handle(0.0, Event::HttpResponse { request: catalogue.id, response: Response { status: 200, headers: "", body: CATALOGUE }});
    let ticker = engine.drain_actions().find_map(|a| match a { Action::Http(r) => Some(r), _ => None }).unwrap();
    engine.handle(0.0, Event::HttpResponse { request: ticker.id, response: Response { status: 503, headers: "", body: "{}" }});
    let timer = engine.drain_actions().find_map(|a| match a {
        Action::SetTimer { timer, at_ms: 1000.0 } => Some(timer), _ => None,
    }).expect("failed public ticker read uses shared retry backoff");
    assert!(engine.store().loaded().markets);
    engine.handle(1000.0, Event::TimerFired { timer });
    let retry = engine.drain_actions().find_map(|a| match a {
        Action::Http(r) if r.url == ticker.url => Some(r), _ => None,
    }).expect("ticker source retried independently");
    assert_ne!(retry.id, ticker.id);
}
