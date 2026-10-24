mod support;

use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, Response, TimerId};
use safe_cex_venue_orderly::{Options, Orderly};

type Driver = Engine<Orderly>;
const LIMIT: f64 = 20_000.;
const TICKERS_AT: f64 = 100.;

fn start(testnet: bool) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
        public_data_only: true,
        testnet,
        ..Default::default()
    }));
    e.start(0.);
    e
}
fn actions(e: &mut Driver, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn no_command_or_signing(out: &[Action]) {
    assert!(
        !out.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
                | Action::Sign { .. }
                | Action::SignPayload { .. }
        )),
        "seed cannot borrow command/signing authority: {out:?}"
    );
}
fn request(out: Vec<Action>, testnet: bool, path: &str) -> HttpRequest {
    no_command_or_signing(&out);
    let mut requests = out.into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let r = requests.next().expect("one actual native seed request");
    assert!(requests.next().is_none());
    assert_eq!(r.method, Method::Get);
    let base = if testnet {
        "https://testnet-api-evm.orderly.org"
    } else {
        "https://api-evm.orderly.org"
    };
    assert_eq!(r.url, format!("{base}{path}"));
    assert!(r.headers.is_empty());
    assert!(r.body.is_none());
    r
}
fn answer(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
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
    actions(e, now)
}
fn catalogue(e: &mut Driver, testnet: bool) -> HttpRequest {
    let r = request(actions(e, 0.), testnet, "/v1/public/info");
    let out = answer(e, &r, TICKERS_AT, support::MARKETS);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 2);
    assert!(!e.store().loaded().tickers);
    request(out, testnet, "/v1/public/futures")
}
fn retry(out: &[Action], now: f64) -> TimerId {
    no_command_or_signing(out);
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "native backoff precedes retry release"
    );
    out.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - now)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("native jittered one-second first retry")
}

#[test]
fn markets_waits_for_prepare_before_raw_release() {
    let mut e = start(false);
    let out: Vec<_> = e.drain_actions().collect();
    no_command_or_signing(&out);
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "markets escaped before Prepare/Bind/Claim"
    );
    assert!(out.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { request, .. } if request.url.ends_with("/v1/public/info")))));
}
#[test]
fn tickers_waits_for_prepare_before_raw_release() {
    let mut e = start(false);
    let r = request(actions(&mut e, 0.), false, "/v1/public/info");
    e.handle(
        TICKERS_AT,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(e.store().loaded().markets);
    no_command_or_signing(&out);
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "tickers escaped before Prepare/Bind/Claim"
    );
    assert!(out.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { request, .. } if request.url.ends_with("/v1/public/futures")))));
}
#[test]
fn market_final_at_its_own_deadline_cannot_publish_or_start_tickers() {
    let mut e = start(false);
    let r = request(actions(&mut e, 0.), false, "/v1/public/info");
    let out = answer(&mut e, &r, LIMIT, support::MARKETS);
    assert!(!e.store().loaded().markets, "expired market seed published");
    assert!(e.store().markets().is_empty());
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "expired DATA must not start ticker seed"
    );
    let timer = retry(&out, LIMIT);
    e.handle(LIMIT + 1000., Event::TimerFired { timer });
    let again = request(actions(&mut e, LIMIT + 1000.), false, "/v1/public/info");
    assert_ne!(again.id, r.id);
}
#[test]
fn ticker_final_at_its_own_deadline_cannot_publish_or_open_public_socket() {
    let mut e = start(false);
    let r = catalogue(&mut e, false);
    let before = e.store().markets().to_vec();
    let deadline = TICKERS_AT + LIMIT;
    let out = answer(&mut e, &r, deadline, support::TICKERS);
    assert!(!e.store().loaded().tickers, "expired ticker seed published");
    assert!(e.store().tickers().is_empty());
    assert_eq!(e.store().markets(), before);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::WsSend { .. })),
        "expired tickers released socket startup"
    );
    let timer = retry(&out, deadline);
    e.handle(deadline + 1000., Event::TimerFired { timer });
    let again = request(
        actions(&mut e, deadline + 1000.),
        false,
        "/v1/public/futures",
    );
    assert_ne!(again.id, r.id);
}
#[test]
fn unprepared_markets_has_admission_deadline_and_late_prepare_is_inert() {
    let mut e = start(false);
    let out: Vec<_> = e.drain_actions().collect();
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == LIMIT => Some(*timer),
            _ => None,
        })
        .expect("market bound starts before Prepare");
    let token = out
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("managed market preparation");
    e.handle(LIMIT, Event::TimerFired { timer });
    assert!(!e.http_preparation_pending(token));
    e.http_prepared(
        LIMIT,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!out.iter().any(
        |a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Bind { .. }))
    ));
    assert!(!e.store().loaded().markets);
    let _ = retry(&out, LIMIT);
}

fn native_wire(testnet: bool) {
    let mut e = start(testnet);
    assert!(!e.store().loaded().markets && !e.store().loaded().tickers);
    let ticker = catalogue(&mut e, testnet);
    let m = e.store().market("BTCUSDC").unwrap();
    assert_eq!(m.id, "BTC/USDC:USDC");
    assert_eq!(m.precision.amount, 0.001);
    assert_eq!(m.precision.price, 0.1);
    assert_eq!(m.limits.leverage.max, 100.);
    let out = answer(&mut e, &ticker, TICKERS_AT + 1., support::TICKERS);
    no_command_or_signing(&out);
    assert!(e.store().loaded().tickers);
    let t = e.store().ticker("BTCUSDC").unwrap();
    assert_eq!(t.id, "BTC/USDC:USDC");
    assert_eq!(
        (t.bid, t.ask, t.last, t.mark, t.index),
        (51000., 51000., 51000., 51000., 50999.)
    );
    assert_eq!(t.percentage, 2.);
    assert_eq!((t.volume, t.quote_volume), (10., 500000.));
    assert!(out.iter().any(|a| matches!(a, Action::WsConnect { url, .. } if url.starts_with(if testnet { "wss://testnet-ws-evm.orderly.org/" } else { "wss://ws-evm.orderly.org/" }))));
}
#[test]
fn native_mainnet_wire_and_market_then_ticker_results() {
    native_wire(false);
}
#[test]
fn native_testnet_wire_and_market_then_ticker_results() {
    native_wire(true);
}
#[test]
fn native_market_network_failure_backs_off_then_rebuilds_markets() {
    let mut e = start(false);
    let r = request(actions(&mut e, 0.), false, "/v1/public/info");
    e.handle(
        1.,
        Event::HttpFailed {
            request: r.id,
            error: "fixture network failure",
        },
    );
    let out = actions(&mut e, 1.);
    assert!(!e.store().loaded().markets);
    let timer = retry(&out, 1.);
    e.handle(1001., Event::TimerFired { timer });
    let again = request(actions(&mut e, 1001.), false, "/v1/public/info");
    assert_ne!(again.id, r.id);
    let out = answer(&mut e, &again, 1002., support::MARKETS);
    let _ = request(out, false, "/v1/public/futures");
    assert!(e.store().loaded().markets);
}
#[test]
fn native_ticker_network_failure_backs_off_without_refetching_markets() {
    let mut e = start(false);
    let r = catalogue(&mut e, false);
    let markets = e.store().markets().to_vec();
    e.handle(
        TICKERS_AT + 1.,
        Event::HttpFailed {
            request: r.id,
            error: "fixture network failure",
        },
    );
    let out = actions(&mut e, TICKERS_AT + 1.);
    let timer = retry(&out, TICKERS_AT + 1.);
    e.handle(TICKERS_AT + 1001., Event::TimerFired { timer });
    let again = request(
        actions(&mut e, TICKERS_AT + 1001.),
        false,
        "/v1/public/futures",
    );
    assert_ne!(again.id, r.id);
    answer(&mut e, &again, TICKERS_AT + 1002., support::TICKERS);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().markets(), markets);
}
#[test]
fn native_empty_market_rows_retry_without_markets_or_tickers_publication() {
    let mut e = start(false);
    let r = request(actions(&mut e, 0.), false, "/v1/public/info");
    let out = answer(&mut e, &r, 1., r#"{"success":true,"data":{"rows":[]}}"#);
    assert!(!e.store().loaded().markets && !e.store().loaded().tickers);
    let _ = retry(&out, 1.);
}
#[test]
fn native_unusable_ticker_rows_retry_only_tickers() {
    let mut e = start(false);
    let r = catalogue(&mut e, false);
    let out = answer(
        &mut e,
        &r,
        TICKERS_AT + 1.,
        r#"{"success":true,"data":{"rows":[]}}"#,
    );
    assert!(e.store().loaded().markets);
    assert!(!e.store().loaded().tickers);
    let timer = retry(&out, TICKERS_AT + 1.);
    e.handle(TICKERS_AT + 1001., Event::TimerFired { timer });
    let _ = request(
        actions(&mut e, TICKERS_AT + 1001.),
        false,
        "/v1/public/futures",
    );
}
