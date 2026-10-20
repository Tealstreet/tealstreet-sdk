use safe_cex_engine::{Action, Engine, Event, Notice, RequestId, Response};
use safe_cex_venue_blofin::{Blofin, Options};

const MARKET: &str = r#"{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}"#;
fn reply(e: &mut Engine<Blofin>, request: RequestId, body: &str) {
    e.handle(
        10.,
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
fn retry(e: &mut Engine<Blofin>) -> RequestId {
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1010., Event::TimerFired { timer });
    e.drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap()
}
#[test]
fn malformed_catalogue_neighbor_and_failed_ticker_do_not_block_account() {
    let mut e = Engine::new(Blofin::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    }));
    e.start(0.);
    let request = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        request,
        &format!(r#"{{"code":"0","data":[{{}},{MARKET},{{"instId":3}}]}}"#),
    );
    assert_eq!(e.store().markets().len(), 1);
    let out: Vec<_> = e.drain_actions().collect();
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("account/margin-mode")))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message == "Skipped malformed public discovery row")));
    let ticker = out
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("market/tickers") => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        ticker,
        r#"{"code":"429","msg":"rate limit","data":[]}"#,
    );
    assert!(!e.store().loaded().tickers);
    let request = retry(&mut e);
    reply(
        &mut e,
        request,
        r#"{"code":"0","data":[{"instId":"BTC-USDT","last":"100","open24h":"100","bidPrice":"","askPrice":"","volCurrency24h":"","vol24h":""},{"instId":3}]}"#,
    );
    assert_eq!(e.store().tickers().len(), 1);
    let ticker = &e.store().tickers()[0];
    assert_eq!(
        (ticker.bid, ticker.ask, ticker.volume, ticker.quote_volume),
        (0., 0., 0., 0.)
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("account/margin-mode")))
    );
}
#[test]
fn catalogue_transport_failure_retries_without_partial_rows() {
    let mut e = Engine::new(Blofin::new(Options::default()));
    e.start(0.);
    let request = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    e.handle(
        10.,
        Event::HttpFailed {
            request,
            error: "offline",
        },
    );
    assert!(!e.store().loaded().markets);
    let request = retry(&mut e);
    reply(
        &mut e,
        request,
        &format!(r#"{{"code":"0","data":[{MARKET}]}}"#),
    );
    assert_eq!(e.store().markets().len(), 1);
}
