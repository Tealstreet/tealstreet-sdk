//! Genuine native ticker seeds; the production sender is unchanged at this checkpoint.
mod support;
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, RequestRoute, Response};
use safe_cex_venue_txflow::{Options, TxFlow};
use support::{META, TICKER};

type Driver = Engine<TxFlow>;
const NOW: f64 = 1000.;

fn response(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) {
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
fn metadata_ready(metadata: &str) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(NOW);
    let meta = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.body.as_deref().is_some_and(|b| b.contains("perpMeta")) => Some(r),
            _ => None,
        })
        .expect("native metadata request");
    response(&mut e, &meta, NOW, metadata);
    assert!(e.store().loaded().markets);
    e
}
fn ticker(e: &mut Driver, now: f64) -> HttpRequest {
    let requests = safe_cex_engine::test_support::prepared_actions(e, now);
    let mut tickers = requests.into_iter().filter_map(|a| match a {
        Action::Http(r)
            if r.body
                .as_deref()
                .is_some_and(|b| b.contains("activeAssetCtx")) =>
        {
            Some(r)
        }
        _ => None,
    });
    let r = tickers.next().expect("one native ticker seed");
    assert!(tickers.next().is_none());
    r
}
fn advance_25ms(e: &mut Driver, now: f64) {
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == now + 25. => Some(timer),
            _ => None,
        })
        .expect("native 25ms next-ticker cadence");
    e.handle(now + 25., Event::TimerFired { timer });
}

#[test]
fn ticker_seed_prepares_before_any_physical_http() {
    let mut e = metadata_ready(META);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "ticker seeds must wait for managed preparation"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })))
            .count(),
        1
    );
}

#[test]
fn ticker_seed_arms_its_bound_before_host_preparation() {
    let mut e = metadata_ready(META);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        actions.iter().any(|a| matches!(a,
        Action::SetTimer { at_ms, .. } if *at_ms == NOW + 20_000.)),
        "unprepared ticker seed needs its real admission expiry timer"
    );
}

#[test]
fn exact_deadline_ticker_data_cannot_publish_a_seed() {
    let mut e = metadata_ready(META);
    let r = ticker(&mut e, NOW);
    response(&mut e, &r, NOW + 20_000., TICKER);
    advance_25ms(&mut e, NOW + 20_000.);
    assert!(
        !e.store().loaded().tickers,
        "DATA at the new physical deadline cannot become a ticker seed"
    );
    assert!(e.store().tickers().is_empty());
}

#[test]
fn unsigned_ticker_wire_and_route_are_native() {
    let mut e = metadata_ready(META);
    let r = ticker(&mut e, NOW);
    assert_eq!(r.method, Method::Post);
    assert_eq!(r.url, "https://rpc.txflow-testnet.xyz/info");
    assert_eq!(r.route, Some(RequestRoute::PublicCors));
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"type":"activeAssetCtx","coin":"1"}"#)
    );
    assert_eq!(
        r.headers,
        vec![
            ("Content-Type", "application/json".into()),
            ("x-api-version", "2026-08-16".into()),
        ]
    );
}

#[test]
fn valid_ticker_keeps_projection_and_native_next_timer() {
    let mut e = metadata_ready(META);
    let r = ticker(&mut e, NOW);
    response(&mut e, &r, NOW, TICKER);
    assert!(!e.store().loaded().tickers);
    advance_25ms(&mut e, NOW);
    assert!(e.store().loaded().tickers);
    let t = e.store().ticker("BTC-USDC").unwrap();
    assert_eq!(t.mark, 65001.);
    assert_eq!(t.funding_rate, 0.0001);
    assert_eq!(t.funding_time, 7200000.);
    assert_eq!(t.open_interest, 130002.);
}

#[test]
fn wrong_asset_cannot_publish_a_seed() {
    let mut e = metadata_ready(META);
    let r = ticker(&mut e, NOW);
    response(
        &mut e,
        &r,
        NOW,
        &TICKER.replace("\"instrumentId\":1", "\"instrumentId\":2"),
    );
    advance_25ms(&mut e, NOW);
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
}

#[test]
fn transport_failure_keeps_independent_cadence_and_keyed_backoff_recovery() {
    let mut e = metadata_ready(META);
    let r = ticker(&mut e, NOW);
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "offline",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let next = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 25. => Some(*timer),
            _ => None,
        })
        .expect("failure preserves next-ticker cadence");
    let (retry, due) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if support::jittered(*at_ms - NOW, 1000.) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("failed asset retains its own native backoff");
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    e.handle(NOW + 25., Event::TimerFired { timer: next });
    assert!(!e.store().loaded().tickers);
    assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    e.handle(due, Event::TimerFired { timer: retry });
    let retry = ticker(&mut e, due);
    assert_eq!(retry.body, r.body);
    response(&mut e, &retry, due, TICKER);
    advance_25ms(&mut e, due);
    assert!(e.store().loaded().tickers);
}

#[test]
fn separate_physical_bounds_do_not_cap_the_whole_seed_walk() {
    let second = r#",{"index":2,"name":"ETH-USDC","baseCurrency":"ETH","quoteCurrency":"USDC","currencyType":"usdc","basePrecision":"0.0001","priceTick":"0.1","maxLeverage":50,"haltTrading":false,"delisted":false,"useNotionalMargin":false}"#;
    let metadata = META.replace("}]}}", &format!("}}{second}]}}}}"));
    let mut e = metadata_ready(&metadata);
    let r = ticker(&mut e, NOW);
    response(&mut e, &r, 19_000., TICKER);
    advance_25ms(&mut e, 19_000.);
    let r = ticker(&mut e, 19_025.);
    assert_eq!(
        r.body.as_deref(),
        Some(r#"{"type":"activeAssetCtx","coin":"2"}"#)
    );
    response(
        &mut e,
        &r,
        38_000.,
        &TICKER.replace("\"instrumentId\":1", "\"instrumentId\":2"),
    );
    advance_25ms(&mut e, 38_000.);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 2);
}

#[test]
fn deadline_data_progresses_the_next_asset_once_and_retries_its_own_key() {
    let second = r#",{"index":2,"name":"ETH-USDC","baseCurrency":"ETH","quoteCurrency":"USDC","currencyType":"usdc","basePrecision":"0.0001","priceTick":"0.1","maxLeverage":50,"haltTrading":false,"delisted":false,"useNotionalMargin":false}"#;
    let metadata = META.replace("}]}}", &format!("}}{second}]}}}}"));
    let mut e = metadata_ready(&metadata);
    let old = ticker(&mut e, NOW);
    response(&mut e, &old, NOW + 20_000., TICKER);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let next = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_025. => Some(*timer),
            _ => None,
        })
        .collect::<Vec<_>>();
    let retry = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if support::jittered(*at_ms - NOW - 20_000., 1000.) =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(next.len(), 1, "exactly one native continuation");
    assert_eq!(retry.len(), 1, "exactly one retry for the expired asset");
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
    response(&mut e, &old, NOW + 20_001., TICKER);
    e.handle(
        NOW + 20_001.,
        Event::HttpFailed {
            request: old.id,
            error: "late duplicate",
        },
    );
    let duplicate = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !duplicate.iter().any(|a| matches!(
            a,
            Action::SetTimer { .. } | Action::Http(_) | Action::ManagedHttp(_)
        )),
        "no duplicate progression"
    );
    e.handle(NOW + 20_025., Event::TimerFired { timer: next[0] });
    let second = ticker(&mut e, NOW + 20_025.);
    assert_eq!(
        second.body.as_deref(),
        Some(r#"{"type":"activeAssetCtx","coin":"2"}"#)
    );
    response(
        &mut e,
        &second,
        NOW + 20_026.,
        &TICKER.replace("\"instrumentId\":1", "\"instrumentId\":2"),
    );
    advance_25ms(&mut e, NOW + 20_026.);
    assert!(!e.store().loaded().tickers);
    e.drain_actions().for_each(drop);
    e.handle(retry[0].1, Event::TimerFired { timer: retry[0].0 });
    let recovered = ticker(&mut e, retry[0].1);
    assert_eq!(
        recovered.body, old.body,
        "backoff retains the failed asset key"
    );
    response(&mut e, &recovered, retry[0].1, TICKER);
    advance_25ms(&mut e, retry[0].1);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 2);
}
