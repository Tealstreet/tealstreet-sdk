//! BingX's per-IP ban (100410, "in the disabled period") and HTTP 429: discovery waits
//! out the ban instead of retrying inside it, and every list waits behind it.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response};
use safe_cex_venue_bingx::Bingx;
mod support;
use support::Options;

const NOW: f64 = 1_800_000_000_000.0;

fn drain(engine: &mut Engine<Bingx>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}

fn catalogue_request(engine: &mut Engine<Bingx>) -> HttpRequest {
    engine.start(NOW);
    drain(engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("/contracts") => Some(request),
            _ => None,
        })
        .expect("catalogue request")
}

/// The delay of the retry timer the failure scheduled.
fn retry_delay(actions: &[Action], now: f64) -> f64 {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "no immediate retry: {actions:?}"
    );
    actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { at_ms, .. } => Some(at_ms - now),
            _ => None,
        })
        .fold(0.0, f64::max)
}

fn answer(
    engine: &mut Engine<Bingx>,
    request: &HttpRequest,
    status: u16,
    headers: &str,
    body: &str,
) -> Vec<Action> {
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
    drain(engine, NOW)
}

fn public_engine() -> Engine<Bingx> {
    support::engine(Options {
        public_data_only: true,
        ..Default::default()
    })
}

fn keyed_engine() -> Engine<Bingx> {
    support::engine(Options {
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    })
}

#[test]
fn a_ban_without_an_unblock_time_waits_five_minutes_public_and_keyed() {
    for mut engine in [public_engine(), keyed_engine()] {
        let request = catalogue_request(&mut engine);
        let actions = answer(
            &mut engine,
            &request,
            200,
            "",
            r#"{"code":100410,"msg":"api is in the disabled period","timestamp":1800000000500}"#,
        );
        assert_eq!(retry_delay(&actions, NOW), 300_000.0, "{actions:?}");
        assert!(
            actions
                .iter()
                .any(|a| format!("{a:?}").contains("BingX rate-limited this IP"))
        );
    }
}

#[test]
fn a_ban_naming_its_unblock_time_waits_until_then() {
    let mut engine = public_engine();
    let request = catalogue_request(&mut engine);
    let actions = answer(
        &mut engine,
        &request,
        200,
        "",
        r#"{"code":100410,"msg":"in the disabled period, unblocked after 1800000090000","timestamp":1800000000500}"#,
    );
    assert_eq!(retry_delay(&actions, NOW), 90_000.0);
}

#[test]
fn http_429_waits_its_retry_after_or_the_floor() {
    let mut engine = public_engine();
    let request = catalogue_request(&mut engine);
    let actions = answer(
        &mut engine,
        &request,
        429,
        "Retry-After: 120",
        "Too Many Requests",
    );
    assert_eq!(retry_delay(&actions, NOW), 120_000.0);

    let mut engine = public_engine();
    let request = catalogue_request(&mut engine);
    let actions = answer(&mut engine, &request, 429, "", "Too Many Requests");
    assert_eq!(retry_delay(&actions, NOW), 300_000.0);
}

#[test]
fn an_ordinary_failure_keeps_the_short_backoff() {
    let mut engine = public_engine();
    let request = catalogue_request(&mut engine);
    let actions = answer(
        &mut engine,
        &request,
        200,
        "",
        r#"{"code":80012,"msg":"busy"}"#,
    );
    let delay = retry_delay(&actions, NOW);
    assert!((750.0..=1000.0).contains(&delay), "{delay}");
}
