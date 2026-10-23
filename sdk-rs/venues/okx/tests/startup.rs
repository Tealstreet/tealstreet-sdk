#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Event, Notice};

#[test]
fn catalogue_skips_bad_neighbors_and_starts_account_before_ticker_seed() {
    let mut engine = crate::support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    engine.start(0.);
    let request = prepared(&mut engine, 0.).remove(0);
    reply(
        &mut engine,
        &request,
        &envelope(&format!("{{}},{MARKET},{{\"instId\":3}}")),
        1.,
    );
    assert_eq!(engine.store().markets().len(), 1);
    let out = prepared(&mut engine, 1.);
    assert!(out.iter().any(|a| is_get(a, "/api/v5/account/config")));
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message == "Skipped malformed public discovery row")));
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
    let timer = retry_timer(&prepared(&mut engine, 2.), 2.0, 1_000.0).unwrap();
    engine.handle(1002., Event::TimerFired { timer });
    let retry = prepared(&mut engine, 1002.).remove(0);
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
        !prepared(&mut engine, 1003.)
            .iter()
            .any(|a| is_get(a, "/api/v5/account/config"))
    );
}

#[test]
fn catalogue_transport_failure_retries_with_fresh_accumulation() {
    let mut engine = crate::support::engine(Options::default());
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

#[test]
fn eea_later_source_cannot_hide_all_malformed_rows_behind_an_earlier_product() {
    const FUTURE: &str = r#"{"instId":"BTC-USD-UM","instType":"FUTURES","ruleType":"xperp","ctType":"linear","ctVal":"0.001","ctValCcy":"BTC","settleCcy":"USD","uly":"BTC-USD","state":"live","lotSz":"1","tickSz":"0.1","minSz":"1","maxLmtSz":"1000","lever":"10"}"#;
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope("{}"), 2.0);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    let out = actions(&mut e);
    let timer = retry_timer(&out, 2.0, 1_000.0).unwrap();
    let request = out
        .into_iter()
        .find(|a| is_get(a, "tickers?instType=SWAP"))
        .unwrap();
    reply(&mut e, &request, &envelope(TICKER), 3.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(""), 4.0);
    let out = actions(&mut e);
    let socket = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(5.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USDT-SWAP","markPx":"77"}]}"#,
    );
    actions(&mut e);
    e.handle(1002.0, Event::TimerFired { timer });
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1003.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(FUTURE), 1004.0);
    assert_eq!(e.store().markets().len(), 2);
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::WsConnect { .. })));
    let request = out
        .into_iter()
        .find(|a| is_get(a, "tickers?instType=SWAP"))
        .unwrap();
    reply(&mut e, &request, &envelope(TICKER), 1005.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope("{}"), 1006.0);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().mark, 77.0);
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| matches!(a, Action::WsConnect { .. })));
    let timer = retry_timer(&out, 1006.0, 1_000.0).unwrap();
    e.handle(2006.0, Event::TimerFired { timer });
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(TICKER), 2007.0);
    let request = actions(&mut e).remove(0);
    let future_quote = TICKER.replace("BTC-USDT-SWAP", "BTC-USD-UM");
    reply(&mut e, &request, &envelope(&future_quote), 2008.0);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 2);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().mark, 77.0);
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn nonfinite_or_nonpositive_instrument_rails_are_isolated_from_valid_peers() {
    for field in ["ctVal", "lotSz", "tickSz", "minSz", "lever"] {
        for bad in ["NaN", "inf", "-inf", "0", "-1"] {
            let key = format!("\"{field}\":\"");
            let start = MARKET.find(&key).unwrap() + key.len();
            let end = start + MARKET[start..].find('"').unwrap();
            let bad_market = format!("{}{bad}{}", &MARKET[..start], &MARKET[end..])
                .replace("BTC-USDT-SWAP", "BAD-USDT-SWAP");
            let mut e = crate::support::engine(Options::default());
            e.start(0.0);
            let request = actions(&mut e).remove(0);
            reply(
                &mut e,
                &request,
                &envelope(&format!("{bad_market},{MARKET}")),
                1.0,
            );
            assert_eq!(e.store().markets().len(), 1, "{field}={bad}");
            assert_eq!(e.store().markets()[0].id, "BTC-USDT-SWAP");
            assert!(actions(&mut e).iter().any(|a| matches!(a, Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message == "Skipped malformed public discovery row")));
        }
    }
}

#[test]
fn recovering_an_earlier_product_keeps_existing_market_setting_indices() {
    let future = MARKET.replace("BTC-USDT-SWAP", "BTC-USD-UM").replace(
        "\"instType\":\"SWAP\"",
        "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\"",
    );
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope("{}"), 1.0);
    let request = actions(&mut e)
        .into_iter()
        .find(|a| is_get(a, "instruments?instType=FUTURES"))
        .unwrap();
    reply(&mut e, &request, &envelope(&future), 2.0);
    let symbol = e.store().market_by_id("BTC-USD-UM").unwrap().symbol.clone();
    e.store_mut()
        .update_setting(&symbol, |setting| setting.leverage = 7.0);
    assert_eq!(e.store().market_index_by_id("BTC-USD-UM"), Some(0));
    let timer = retry_timer(&actions(&mut e), 2.0, 1_000.0).unwrap();
    e.handle(1002.0, Event::TimerFired { timer });
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1003.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(&future), 1004.0);
    assert_eq!(e.store().market_index_by_id("BTC-USD-UM"), Some(0));
    assert_eq!(e.store().market_index_by_id("BTC-USDT-SWAP"), Some(1));
    assert_eq!(e.store().setting(&symbol).unwrap().leverage, 7.0);
    assert!(
        !actions(&mut e).iter().any(|a| matches!(a, Action::Http(_))),
        "catalogue recovery must queue behind the existing ticker walk"
    );
}

#[test]
fn overlapping_catalogue_and_ticker_retries_keep_one_walk_and_protect_live_fields_until_the_queue_settles()
 {
    let future = MARKET.replace("BTC-USDT-SWAP", "BTC-USD-UM").replace(
        "\"instType\":\"SWAP\"",
        "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\"",
    );
    let future_quote = TICKER.replace("BTC-USDT-SWAP", "BTC-USD-UM");
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope("{}"), 2.0);
    let out = actions(&mut e);
    let catalogue_timer = retry_timer(&out, 2.0, 1_000.0).unwrap();
    let request = out
        .into_iter()
        .find(|a| is_get(a, "tickers?instType=SWAP"))
        .unwrap();
    reply(&mut e, &request, &envelope(TICKER), 3.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope("{}"), 4.0);
    let out = actions(&mut e);
    let ticker_timer = retry_timer(&out, 4.0, 1_000.0).unwrap();
    let socket = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(5.0, Event::WsOpened { socket });
    actions(&mut e);
    e.handle(
        1002.0,
        Event::TimerFired {
            timer: catalogue_timer,
        },
    );
    let catalogue = actions(&mut e).remove(0);
    e.handle(
        1004.0,
        Event::TimerFired {
            timer: ticker_timer,
        },
    );
    let ticker = actions(&mut e).remove(0);
    reply(&mut e, &catalogue, &envelope(MARKET), 1005.0);
    let catalogue = actions(&mut e).remove(0);
    reply(&mut e, &catalogue, &envelope(&future), 1006.0);
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
    reply(&mut e, &ticker, &envelope(TICKER), 1007.0);
    let ticker = actions(&mut e).remove(0);
    reply(&mut e, &ticker, &envelope(&future_quote), 1008.0);
    let ticker = actions(&mut e)
        .into_iter()
        .find(|a| is_get(a, "tickers?instType=SWAP"))
        .expect("one queued refresh");
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USDT-SWAP","markPx":"77"}]}"#,
    );
    actions(&mut e);
    reply(&mut e, &ticker, &envelope(TICKER), 1010.0);
    let ticker = actions(&mut e).remove(0);
    reply(&mut e, &ticker, &envelope(&future_quote), 1011.0);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().mark, 77.0);
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

#[test]
fn failed_account_config_is_retried_by_the_startup_backoff() {
    let mut engine = crate::support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    engine.start(0.);
    let request = prepared(&mut engine, 0.).remove(0);
    reply(&mut engine, &request, &envelope(MARKET), 1.);
    let config = prepared(&mut engine, 1.)
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut engine,
        &config,
        r#"{"code":"50011","msg":"rate limit","data":[]}"#,
        2.,
    );
    let timer = retry_timer(&prepared(&mut engine, 2.), 2.0, 1_000.0).unwrap();
    engine.handle(1_002., Event::TimerFired { timer });
    let config = prepared(&mut engine, 1_002.)
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .expect("config retried after failure");
    reply(
        &mut engine,
        &config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        5_003.,
    );
    let mut pending: Vec<_> = prepared(&mut engine, 5_003.)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(r) if r.url.contains("/api/v5/account") || r.url.contains("/api/v5/trade")))
        .collect();
    while let Some(a) = pending.pop() {
        let body = if matches!(&a, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut engine, &a, &body, 5_004.);
        pending.extend(prepared(&mut engine, 5_004.).into_iter().filter(|a| {
            matches!(a, Action::Http(r) if r.url.contains("/api/v5/account") || r.url.contains("/api/v5/trade"))
        }));
    }
    let loaded = engine.store().loaded();
    assert!(loaded.balance && loaded.orders && loaded.positions);
}

#[test]
fn catalogue_rate_limit_retry_waits_for_retry_after() {
    let mut engine = crate::support::engine(Options::default());
    engine.start(0.);
    let Action::Http(request) = actions(&mut engine).remove(0) else {
        panic!("catalogue request")
    };
    engine.handle(
        0.,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 429,
                headers: "Retry-After: 120",
                body: "",
            },
        },
    );
    let mut timers = Vec::new();
    let mut retried_at = None;
    let mut now = 0.;
    loop {
        for action in actions(&mut engine) {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| *t != timer),
                Action::Http(r) if r.url.contains("/api/v5/public/instruments") => {
                    retried_at.get_or_insert(now);
                }
                _ => {}
            }
        }
        if retried_at.is_some() || timers.is_empty() {
            break;
        }
        timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (timer, at) = timers.remove(0);
        now = at;
        engine.handle(now, Event::TimerFired { timer });
    }
    assert_eq!(
        retried_at,
        Some(120_000.),
        "catalogue retried only after Retry-After"
    );
}
