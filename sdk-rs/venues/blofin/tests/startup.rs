mod trusted;
use safe_cex_engine::{Action, Engine, Event, Notice, RequestId, Response};
use safe_cex_venue_blofin::{Blofin, Options};

const MARKET: &str = r#"{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}"#;
fn reply(e: &mut Engine<Blofin>, request: RequestId, body: &str) {
    reply_at(e, request, 10., body);
}
fn reply_at(e: &mut Engine<Blofin>, request: RequestId, at: f64, body: &str) {
    e.handle(
        at,
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
    safe_cex_engine::test_support::prepared_actions(e, 1010.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap()
}
#[test]
fn malformed_catalogue_neighbor_and_failed_ticker_do_not_block_account() {
    let mut e = trusted::trusted_engine_with(
        Options::default(),
        Some(trusted::Keys::new("key", "secret", "pass")),
    );
    e.start(0.);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
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
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, 10.);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("account/margin-mode")))
    );
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message == "Skipped malformed public discovery row")));
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
    let mut e = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options::default()));
    e.start(0.);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
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
#[test]
fn unknown_ticker_source_retains_previous_quotes_retries_and_accepts_valid_neighbors() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options::default()));
    e.start(0.);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        request,
        &format!(r#"{{"code":"0","data":[{MARKET}]}}"#),
    );
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 10.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("market/tickers") => Some(r.id),
            _ => None,
        })
        .unwrap();
    e.store_mut()
        .set_tickers(vec![safe_cex_engine::model::Ticker {
            id: "BTC-USDT".into(),
            symbol: "BTCUSDT".into(),
            last: 123.,
            ..Default::default()
        }]);
    let previous = e.store().tickers().to_vec();
    reply(
        &mut e,
        request,
        r#"{"code":"0","data":[{"instId":"UNKNOWN","last":"100"}]}"#,
    );
    assert_eq!(e.store().tickers(), previous);
    let request = retry(&mut e);
    reply(
        &mut e,
        request,
        r#"{"code":"0","data":[{"instId":"UNKNOWN"},{"instId":"BTC-USDT","last":"150"}]}"#,
    );
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 150.);
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
}
#[test]
fn late_ticker_seed_preserves_live_fields_missing_peers_and_funding_only_updates() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options::default()));
    e.start(0.);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        request,
        &format!(
            r#"{{"code":"0","data":[{MARKET},{}]}}"#,
            MARKET.replace("BTC", "ETH")
        ),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 10.);
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("market/tickers") => Some(r.id),
            _ => None,
        })
        .unwrap();
    e.handle(10., Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    reply(
        &mut e,
        request,
        r#"{"code":"0","data":[{"instId":"UNKNOWN"}]}"#,
    );
    let request = retry(&mut e);
    e.handle(1020., Event::WsMessage { socket, text: r#"{"arg":{"channel":"funding-rate"},"data":[{"instId":"BTC-USDT","fundingRate":"0.01","fundingTime":"12345"}]}"# });
    e.handle(1021., Event::WsMessage { socket, text: r#"{"arg":{"channel":"tickers"},"data":[{"instId":"ETH-USDT","last":"240","open24h":"200","bidPrice":"239","askPrice":"241","volCurrency24h":"20","vol24h":"4800"}]}"# });
    reply_at(
        &mut e,
        request,
        1030.,
        r#"{"code":"0","data":[{"instId":"BTC-USDT","last":"150","open24h":"100","bidPrice":"149","askPrice":"151","volCurrency24h":"10","vol24h":"1500"}]}"#,
    );
    let btc = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (btc.last, btc.bid, btc.ask, btc.volume),
        (150., 149., 151., 10.)
    );
    assert_eq!((btc.funding_rate, btc.funding_time), (0.01, 12345.));
    let eth = e.store().ticker("ETHUSDT").unwrap();
    assert_eq!(
        (eth.last, eth.bid, eth.ask, eth.mark, eth.index),
        (240., 239., 241., 240., 240.)
    );
    assert_eq!((eth.volume, eth.quote_volume), (20., 4800.));
    assert_eq!(e.store().tickers().len(), 2);
}
#[test]
fn empty_successful_catalogue_is_retried_not_committed() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(&mut e, request, r#"{"code":"0","data":[]}"#);
    assert!(!e.store().loaded().markets);
    let request = retry(&mut e);
    reply(
        &mut e,
        request,
        &format!(r#"{{"code":"0","data":[{MARKET}]}}"#),
    );
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
}
