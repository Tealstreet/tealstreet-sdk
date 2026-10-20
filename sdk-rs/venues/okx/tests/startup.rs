#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Engine, Event, Notice};
use safe_cex_venue_okx::{Okx, Options};
use support::*;

#[test]
fn catalogue_skips_bad_neighbors_and_starts_account_before_ticker_seed() {
    let mut engine = Engine::new(Okx::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    }));
    engine.start(0.);
    let request = actions(&mut engine).remove(0);
    reply(
        &mut engine,
        &request,
        &envelope(&format!("{{}},{MARKET},{{\"instId\":3}}")),
        1.,
    );
    assert_eq!(engine.store().markets().len(), 1);
    let out = actions(&mut engine);
    assert!(out.iter().any(|a| is_get(a, "/api/v5/account/config")));
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message == "Skipped malformed public discovery row")));
    let ticker = out
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("market/tickers")))
        .unwrap();
    reply(
        &mut engine,
        ticker,
        r#"{"code":"50011","msg":"rate limit","data":[]}"#,
        2.,
    );
    assert!(!engine.store().loaded().tickers);
    let timer = actions(&mut engine)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1002.,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(1002., Event::TimerFired { timer });
    let retry = actions(&mut engine).remove(0);
    reply(
        &mut engine,
        &retry,
        &envelope(
            r#"{"instId":"BTC-USDT-SWAP","last":"","open24h":"","bidPx":"","askPx":"","volCcy24h":"","vol24h":""},{"instId":3}"#,
        ),
        1003.,
    );
    assert!(engine.store().loaded().tickers);
    assert_eq!(engine.store().tickers().len(), 1);
    let ticker = &engine.store().tickers()[0];
    assert_eq!(
        (
            ticker.last,
            ticker.bid,
            ticker.ask,
            ticker.volume,
            ticker.quote_volume
        ),
        (0., 0., 0., 0., 0.)
    );
    assert!(
        !actions(&mut engine)
            .iter()
            .any(|a| is_get(a, "/api/v5/account/config"))
    );
}

#[test]
fn catalogue_transport_failure_retries_with_fresh_accumulation() {
    let mut engine = Engine::new(Okx::new(Options::default()));
    engine.start(0.);
    let Action::Http(request) = actions(&mut engine).remove(0) else {
        panic!()
    };
    engine.handle(
        1.,
        Event::HttpFailed {
            request: request.id,
            error: "offline",
        },
    );
    let timer = actions(&mut engine)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(1001., Event::TimerFired { timer });
    let request = actions(&mut engine).remove(0);
    reply(&mut engine, &request, &envelope(MARKET), 1002.);
    assert_eq!(engine.store().markets().len(), 1);
}
