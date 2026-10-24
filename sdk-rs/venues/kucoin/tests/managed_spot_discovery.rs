#![cfg(feature = "spot")]
mod support;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation, RoutingIntent};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, VenueError};
use safe_cex_venue_kucoin::spot::KucoinSpot;
use support::credentials::Options;

const MARKETS: &str = r#"{"code":"200000","data":[{"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.001","priceIncrement":"0.1","baseMinSize":"0.001","baseMaxSize":"100"}]}"#;
const TICKERS: &str = r#"{"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","last":"100","buy":"99","sell":"101"}]}}"#;

fn engine(keyed: bool) -> Engine<KucoinSpot> {
    support::credentials::engine(
        if keyed {
            Options {
                key: Some("fixture-key".into()),
                secret: Some("fixture-secret".into()),
                passphrase: Some("fixture-passphrase".into()),
                ..Options::default()
            }
        } else {
            Options {
                public_data_only: true,
                ..Options::default()
            }
        },
        KucoinSpot::new,
    )
}
fn reply(e: &mut Engine<KucoinSpot>, now: f64, request: safe_cex_engine::RequestId, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn prepare(actions: &[Action]) -> Preparation {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "discovery must wait for transport preparation"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    headers(request);
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("public discovery preparation")
}
fn headers(request: &HttpRequest) {
    assert_eq!(
        request.headers,
        vec![
            ("X-CHANNEL-API-CODE", "3zexp".into()),
            ("Content-Type", "application/json".into())
        ]
    );
    assert!(request.signer_headers.is_empty());
    assert!(request.body.is_none());
}
fn released(e: &mut Engine<KucoinSpot>, now: f64, suffix: &str) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(request) if request.url.ends_with(suffix) => Some(request),
            _ => None,
        })
        .expect("released discovery request")
}

#[test]
fn keyless_and_keyed_catalogue_wait_for_unsigned_preparation() {
    for keyed in [false, true] {
        let mut e = engine(keyed);
        e.start(100.);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = prepare(&actions);
        reply(&mut e, 101., token.attempt().request, MARKETS);
        assert!(!e.store().loaded().markets);
        assert!(e.store().markets().is_empty());
        assert!(e.http_preparation_pending(token));
    }
}
#[test]
fn an_unreleased_catalogue_identity_cannot_publish_rows() {
    let mut e = engine(false);
    e.start(100.);
    let request = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation.attempt().request),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    reply(&mut e, 101., request, MARKETS);
    assert!(
        e.store().markets().is_empty(),
        "request identity alone cannot authorize publication"
    );
    assert!(!e.store().loaded().markets);
}
#[test]
fn preparation_refusal_retries_without_loading_or_publishing() {
    let mut e = engine(false);
    e.start(100.);
    let token = prepare(&e.drain_actions().collect::<Vec<_>>());
    e.http_preparation_failed(
        101.,
        token,
        Error::Invalid,
        VenueError::new("fixture preparation refusal"),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!e.http_preparation_pending(token));
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (timer, at) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms > 101. && at_ms < 5101. => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("native discovery retry");
    e.handle(at, Event::TimerFired { timer });
    prepare(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn expired_released_catalogue_cannot_set_markets() {
    let mut e = engine(false);
    e.start(100.);
    let request = released(&mut e, 100., "/api/v2/symbols");
    reply(&mut e, 20_101., request.id, MARKETS);
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
}
#[test]
fn released_discovery_keeps_native_headers_rows_and_ticker_envelope() {
    let mut e = engine(false);
    e.start(100.);
    let request = released(&mut e, 100., "/api/v2/symbols");
    headers(&request);
    reply(&mut e, 101., request.id, MARKETS);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets()[0].symbol, "BTC-USDT");
    let tickers = released(&mut e, 101., "/api/v1/market/allTickers");
    headers(&tickers);
    reply(&mut e, 102., tickers.id, TICKERS);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers()[0].last, 100.);
    e.drain_actions().for_each(drop);
    reply(
        &mut e,
        103.,
        tickers.id,
        r#"{"code":"200000","data":{"ticker":[{"symbol":"BTC-USDT","last":"999"}]}}"#,
    );
    assert_eq!(e.store().tickers()[0].last, 100.);
}

#[test]
fn parked_ticker_expiry_retries_only_tickers_and_keeps_catalogue() {
    let mut e = engine(false);
    e.start(100.);
    let markets = released(&mut e, 100., "/api/v2/symbols");
    reply(&mut e, 101., markets.id, MARKETS);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let token = prepare(&actions);
    let deadline = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_101. => Some(*timer),
            _ => None,
        })
        .expect("ticker preparation retains its original deadline");
    e.handle(20_101., Event::TimerFired { timer: deadline });
    assert!(e.store().loaded().markets);
    assert!(!e.store().loaded().tickers);
    assert_eq!(e.store().markets().len(), 1);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let (timer, at) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms > 20_101. && at_ms < 25_101. => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("ticker-only native recovery");
    reply(&mut e, 20_102., token.attempt().request, TICKERS);
    assert!(!e.store().loaded().tickers);
    e.handle(at, Event::TimerFired { timer });
    let tickers = released(&mut e, at, "/api/v1/market/allTickers");
    reply(&mut e, at + 1., tickers.id, TICKERS);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().markets().len(), 1);
}

#[test]
fn discovery_terminal_paths_release_capacity_before_native_retry() {
    use safe_cex_engine::request_context::{Claim, Prepared};
    // Fire each recorded retry at one fixed clock only in this capacity probe:
    // expiry must not hide a missing terminal retirement. Timing has separate tests.
    for mode in 0..6 {
        let mut e = engine(false);
        let now = 100.;
        e.start(now);
        for _ in 0..270 {
            let actions = e.drain_actions().collect::<Vec<_>>();
            let token = prepare(&actions);
            if mode == 0 {
                e.http_preparation_failed(
                    now,
                    token,
                    Error::Invalid,
                    VenueError::new("fixture refusal"),
                );
            } else {
                e.http_prepared(
                    now,
                    token,
                    Prepared {
                        id: token.attempt().request.0,
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
                    .unwrap();
                assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
                e.drain_actions().for_each(drop);
                match mode {
                    1 => reply(
                        &mut e,
                        now,
                        token.attempt().request,
                        r#"{"code":"200000","data":[]}"#,
                    ),
                    2 => reply(
                        &mut e,
                        now,
                        token.attempt().request,
                        r#"{"code":"200000","data":{}}"#,
                    ),
                    3 => reply(
                        &mut e,
                        now,
                        token.attempt().request,
                        r#"{"code":"200000","data":["#,
                    ),
                    4 => reply(
                        &mut e,
                        now,
                        token.attempt().request,
                        r#"{"code":"400100","msg":"fixture rejection"}"#,
                    ),
                    _ => e.handle(
                        now,
                        Event::HttpFailed {
                            request: token.attempt().request,
                            error: "fixture transport failure",
                        },
                    ),
                }
            }
            assert!(!e.store().loaded().markets);
            let (timer, at) = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if at_ms > now && at_ms < now + 40_000. => {
                        Some((timer, at_ms))
                    }
                    _ => None,
                })
                .expect("every terminal callback preserves native discovery recovery");
            assert!(at > now);
            e.handle(now, Event::TimerFired { timer });
        }
        let markets = released(&mut e, now, "/api/v2/symbols");
        reply(&mut e, now, markets.id, MARKETS);
        let tickers = released(&mut e, now, "/api/v1/market/allTickers");
        reply(&mut e, now, tickers.id, TICKERS);
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
    }
}
