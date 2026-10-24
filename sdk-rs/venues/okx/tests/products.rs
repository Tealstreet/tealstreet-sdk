#![allow(dead_code)]
mod support;

use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Response, SocketId};
use safe_cex_venue_okx::Okx;

// OKX v5 Get instruments / positions / orders schemas. EEA advertises X-Perps
// as FUTURES with ruleType=xperp; other dated futures remain out of scope.
fn inverse() -> String {
    MARKET
        .replace("BTC-USDT-SWAP", "BTC-USD-SWAP")
        .replace("\"linear\"", "\"inverse\"")
        .replace("\"ctVal\":\"0.01\"", "\"ctVal\":\"100\"")
        .replace("\"ctValCcy\":\"BTC\"", "\"ctValCcy\":\"USD\"")
        .replace("\"settleCcy\":\"USDT\"", "\"settleCcy\":\"BTC\"")
}
fn future() -> String {
    inverse().replace("BTC-USD-SWAP", "BTC-USD-261225").replace(
        "\"instType\":\"SWAP\"",
        "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\",\"uly\":\"BTC-USD\"",
    )
}
fn future_linear() -> String {
    MARKET.replace("BTC-USDT-SWAP", "BTC-USDT-261225").replace(
        "\"instType\":\"SWAP\"",
        "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\",\"uly\":\"BTC-USDT\"",
    )
}
fn rows(values: &[&str]) -> String {
    format!(r#"{{"code":"0","data":[{}]}}"#, values.join(","))
}
fn ticker(id: &str) -> String {
    TICKER.replace("BTC-USDT-SWAP", id)
}
fn catalog(options: Options) -> (Engine<Okx>, Vec<Action>) {
    let eea = options.eea;
    let private = options.key.is_some();
    let mut e = crate::support::engine(options);
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    assert!(is_get(&a, "instruments?instType=SWAP"));
    if eea {
        assert!(
            matches!(&a,Action::Http(r) if r.url.contains(if private { "/account/instruments?" } else { "/public/instruments?" }))
        );
    }
    reply(&mut e, &a, &rows(&[MARKET, &inverse()]), 1.0);
    if eea {
        assert!(!e.store().loaded().markets);
        let a = take_http(&mut prepared(&mut e, 1.0));
        assert!(is_get(&a, "instruments?instType=FUTURES"));
        let unsupported = future()
            .replace("\"xperp\"", "\"normal\"")
            .replace("261225", "261231");
        reply(
            &mut e,
            &a,
            &rows(&[&future(), &future_linear(), &unsupported]),
            2.0,
        );
    }
    assert!(e.store().loaded().markets);
    let mut out = prepared(&mut e, if eea { 2.0 } else { 1.0 });
    let a = take_http(&mut out);
    assert!(is_get(&a, "tickers?instType=SWAP"));
    reply(&mut e, &a, &rows(&[TICKER, &ticker("BTC-USD-SWAP")]), 3.0);
    if eea {
        assert!(!e.store().loaded().tickers);
        let a = take_http(&mut prepared(&mut e, 3.0));
        assert!(is_get(&a, "tickers?instType=FUTURES"));
        reply(
            &mut e,
            &a,
            &rows(&[&ticker("BTC-USD-261225"), &ticker("BTC-USDT-261225")]),
            4.0,
        );
    }
    out.extend(prepared(&mut e, if eea { 4.0 } else { 3.0 }));
    (e, out)
}
fn private_catalog() -> (Engine<Okx>, SocketId, Vec<Action>) {
    let (mut e, out) = catalog(Options {
        eea: true,
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    let config = out.iter().find(|a| is_get(a, "/account/config")).unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        5.0,
    );
    let out = prepared(&mut e, 5.0);
    let private = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    (e, private, out)
}
fn complete(mut pending: Vec<Action>, e: &mut Engine<Okx>) -> Vec<String> {
    let mut urls = Vec::new();
    while let Some(a) = pending.pop() {
        let Action::Http(r) = &a else { continue };
        urls.push(r.url.clone());
        let body = if r.url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"BTC","eq":"1"},{"ccy":"USDT","eq":"5000"}]}"#)
        } else if r.url.contains("/positions?") {
            let id = if r.url.ends_with("FUTURES") {
                "BTC-USD-261225"
            } else {
                "BTC-USD-SWAP"
            };
            envelope(&format!(
                r#"{{"instId":"{id}","pos":"5","posSide":"net","avgPx":"39000","markPx":"40000","ccy":"BTC","lever":"10","upl":"0.001","liqPx":"30000","mgnMode":"cross","imr":"0.01","mmr":"0.001"}}"#
            ))
        } else if r.url.contains("/orders-pending?") {
            let (id, order) = if r.url.ends_with("FUTURES") {
                ("BTC-USD-261225", "future-order")
            } else {
                ("BTC-USD-SWAP", "swap-order")
            };
            envelope(&format!(
                r#"{{"instId":"{id}","ordId":"{order}","ordType":"limit","state":"live","side":"buy","sz":"4","accFillSz":"1","px":"40000","ccy":"BTC","lever":"10","reduceOnly":"false"}}"#
            ))
        } else {
            envelope("")
        };
        reply(e, &a, &body, 6.0);
        pending.extend(prepared(e, 6.0));
    }
    urls
}
#[test]
fn mixed_catalog_keeps_inverse_and_strict_eea_xperp_products() {
    let (e, _) = catalog(Options {
        eea: true,
        ..Options::default()
    });
    assert_eq!(e.store().markets().len(), 4);
    let inverse = e.store().market("BTCUSD").unwrap();
    assert_eq!((&*inverse.base, &*inverse.quote), ("BTC", "USD"));
    assert_eq!(inverse.is_inverse, Some(true));
    assert_eq!(inverse.contract_size, Some(100.0));
    let future = e.store().market("BTCUSD261225").unwrap();
    assert_eq!(future.id, "BTC-USD-261225");
    assert_eq!(future.pretty_symbol.as_deref(), Some("BTC/USD XPERP"));
    assert!(e.store().market("BTCUSD261231").is_none());
    assert_eq!(e.store().tickers().len(), 4);
    let (global, _) = catalog(Options::default());
    assert_eq!(global.store().markets().len(), 2);
}
#[test]
fn index_underlying_updates_every_matching_swap_and_xperp() {
    let (mut e, out) = catalog(Options {
        eea: true,
        ..Options::default()
    });
    let public = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(5.0, Event::WsOpened { socket: public });
    let sent = prepared(&mut e, 5.0);
    assert!(sent.iter().any(|a|matches!(a,Action::WsSend{text,..} if text.contains(r#""channel":"index-tickers","instId":"BTC-USD""#))));
    ws(
        &mut e,
        public,
        r#"{"arg":{"channel":"index-tickers","instId":"BTC-USD"},"data":[{"instId":"BTC-USD","idxPx":"41000"}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSD").unwrap().index, 41000.0);
    assert_eq!(e.store().ticker("BTCUSD261225").unwrap().index, 41000.0);
    assert_eq!(e.store().ticker("BTCUSDT261225").unwrap().index, 40000.0);
    ws(
        &mut e,
        public,
        r#"{"arg":{"channel":"index-tickers","instId":"BTC-USDT"},"data":[{"instId":"BTC-USDT","idxPx":"42000"},{"instId":"UNKNOWN-USD","idxPx":"1"}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().index, 42000.0);
    assert_eq!(e.store().ticker("BTCUSDT261225").unwrap().index, 42000.0);
    assert_eq!(e.store().ticker("BTCUSD").unwrap().index, 41000.0);
    assert_eq!(e.store().ticker("BTCUSD261225").unwrap().index, 41000.0);
}
#[test]
fn mixed_private_snapshots_commit_union_and_inverse_risk_in_usd() {
    let (mut e, _, out) = private_catalog();
    let urls = complete(out, &mut e);
    assert_eq!(
        urls.iter()
            .filter(|u| u.contains("account-position-risk"))
            .count(),
        1,
        "account equity must not be counted per product"
    );
    assert!(
        urls.iter()
            .any(|u| u.ends_with("positions?instType=FUTURES"))
    );
    assert_eq!(urls.iter().filter(|u| u.contains("orders-")).count(), 10);
    assert_eq!(e.store().positions().len(), 2);
    for p in e.store().positions() {
        assert_eq!(p.contracts, 0.0125);
        assert_eq!(p.notional, 500.0);
        assert_eq!(p.unrealized_pnl, 40.0);
    }
    assert_eq!(e.store().orders().len(), 2);
    for o in e.store().orders() {
        assert_eq!(o.amount, 4.0);
        assert_eq!(o.filled, 0.0025);
        assert_eq!(o.remaining, 0.0075);
        assert_eq!(o.extra.as_ref().unwrap().remaining_native, Some(3.0));
    }
    assert_eq!(e.store().balance().used, 800.0);
    assert_eq!(e.store().balance().upnl, 80.0);
    assert_eq!(e.store().balance().total, 44920.0);
    assert_eq!(e.store().balance().free, 44134.0);
    let btc = e
        .store()
        .currency_balances()
        .unwrap()
        .iter()
        .find(|c| c.currency == "BTC")
        .unwrap();
    assert!((btc.balance.used - 0.02165).abs() < 1e-12);
    assert_eq!(btc.balance.upnl, 0.002);
}
#[test]
fn product_subscriptions_and_execution_fill_use_actual_inverse_price() {
    let (mut e, private, out) = private_catalog();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    prepared(&mut e, 7.0);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    let out = prepared(&mut e, 4.0);
    assert!(out.iter().any(|a|matches!(a,Action::WsSend{text,..} if text.contains(r#""channel":"positions","instType":"FUTURES""#) && text.contains(r#""channel":"orders","instType":"SWAP""#))));
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USD-261225","ordId":"exec","ordType":"limit","state":"partially_filled","side":"buy","sz":"5","accFillSz":"2","px":"50000","fillPx":"50000","fillSz":"2","tradeId":"trade1"}]}"#,
    );
    let frame = e.take_frame(9.0).unwrap();
    let mut at = 16;
    let mut fill_amount = None;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let data = &frame[at + 8..at + 8 + len];
            assert_eq!(u32::from_le_bytes(data[..4].try_into().unwrap()), 1);
            fill_amount = Some(f64::from_le_bytes(data[24..32].try_into().unwrap()));
        }
        at += 8 + len;
    }
    assert_eq!(fill_amount, Some(0.004));
    assert_eq!(e.store().order("exec").unwrap().remaining, 0.006);
}
#[test]
fn delayed_futures_read_gates_startup_and_failure_preserves_old_union() {
    let (mut e, _, out) = private_catalog();
    let requests: Vec<_> = out
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    let swap = requests
        .iter()
        .find(|a| is_get(a, "positions?instType=SWAP"))
        .unwrap();
    reply(&mut e, swap, &envelope(""), 6.0);
    let futures = prepared(&mut e, 6.0)
        .into_iter()
        .find(|a| is_get(a, "positions?instType=FUTURES"))
        .unwrap();
    assert!(!e.store().loaded().positions);
    e.handle(7.0,Event::Command(Command::PlaceOrders {command:99,orders:vec![serde_json::from_str(r#"{"symbol":"BTCUSD261225","type":"limit","side":"buy","amount":2,"price":40000}"#).unwrap()]}));
    assert!(prepared(&mut e, 7.0).iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed { command: 99, error }) if error.legacy_text().contains("not ready"))));
    let Action::Http(request) = futures else {
        panic!()
    };
    e.handle(
        8.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 500,
                headers: "",
                body: "failed",
            },
        },
    );
    assert!(!e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
}
#[test]
fn inverse_and_xperp_writes_keep_native_lots_and_exact_instrument_identity() {
    let (mut e, _, out) = private_catalog();
    complete(out, &mut e);
    e.handle(8.0,Event::Command(Command::PlaceOrders {command:5,orders:vec![serde_json::from_str(r#"{"symbol":"BTCUSD261225","type":"limit","side":"buy","amount":2,"price":40000,"clientOrderId":"CallerX"}"#).unwrap()]}));
    let out = prepared(&mut e, 8.0);
    let request = out
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(request.method, Method::Post);
    let body = request.body.as_deref().unwrap();
    assert!(body.contains(r#""instId":"BTC-USD-261225""#));
    assert!(body.contains(r#""sz":"2""#));
    assert!(body.contains(r#""clOrdId":"CallerX""#));
}
#[test]
fn malformed_xperp_row_is_skipped_without_withholding_the_catalog() {
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = take_http(&mut prepared(&mut e, 1.0));
    reply(
        &mut e,
        &a,
        &envelope(&format!(
            r#"{{"instType":"FUTURES","ruleType":"xperp","ctType":"inverse","instId":"BTC-USD-261225"}},{}"#,
            future()
        )),
        2.0,
    );
    assert!(e.store().loaded().markets);
    let symbols: Vec<_> = e
        .store()
        .markets()
        .iter()
        .map(|m| m.symbol.clone())
        .collect();
    assert_eq!(symbols.len(), 2, "{symbols:?}");
    assert!(
        prepared(&mut e, 1.0)
            .iter()
            .any(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
    );
}

#[test]
fn failed_xperp_catalog_read_retries_the_whole_union_without_publishing_swap() {
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = take_http(&mut prepared(&mut e, 1.0));
    reply(
        &mut e,
        &a,
        r#"{"code":"50001","msg":"Service temporarily unavailable","data":[]}"#,
        2.0,
    );
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    let out = prepared(&mut e, 1.0);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::Http(_)))
    );
    let timer = retry_timer(&out, 2.0, 1_000.0).expect("startup retry timer");
    e.handle(1002.0, Event::TimerFired { timer });
    let a = take_http(&mut prepared(&mut e, 1002.0));
    assert!(is_get(&a, "/api/v5/public/instruments?instType=SWAP"));
    reply(&mut e, &a, &envelope(MARKET), 1003.0);
    let a = take_http(&mut prepared(&mut e, 1003.0));
    reply(&mut e, &a, &envelope(&future()), 1004.0);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 2);
}

#[test]
fn malformed_xperp_catalogue_row_does_not_discard_valid_swaps() {
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = take_http(&mut prepared(&mut e, 1.0));
    reply(
        &mut e,
        &a,
        &envelope(
            r#"{"instType":"FUTURES","ruleType":"xperp","ctType":"inverse","instId":"BTC-USD-261225"}"#,
        ),
        2.0,
    );
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert!(e.store().market("BTCUSDT").is_some());
    assert!(prepared(&mut e, 1.0).iter().any(|a| matches!(a, Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message == "Skipped malformed public discovery row")));
}

#[test]
fn failed_later_product_position_refresh_keeps_both_previous_products() {
    let (mut e, _, out) = private_catalog();
    complete(out, &mut e);
    let old = e.store().positions().to_vec();
    e.handle(
        7.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSD261225".into(),
            leverage: 8.0,
            side: None,
        }),
    );
    let a = prepared(&mut e, 7.0)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    assert!(
        matches!(&a,Action::Http(r) if r.body.as_deref().unwrap().contains(r#""instId":"BTC-USD-261225""#))
    );
    reply(&mut e, &a, &envelope(""), 8.0);
    let out = prepared(&mut e, 8.0);
    let swap = out
        .iter()
        .find(|a| is_get(a, "positions?instType=SWAP"))
        .unwrap();
    reply(&mut e, swap, &envelope(""), 9.0);
    let future = prepared(&mut e, 9.0)
        .into_iter()
        .find(|a| is_get(a, "positions?instType=FUTURES"))
        .unwrap();
    let Action::Http(request) = future else {
        panic!()
    };
    e.handle(
        10.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 500,
                headers: "",
                body: "failed",
            },
        },
    );
    assert_eq!(e.store().positions(), old);
    assert_eq!(e.store().balance().used, 800.0);
}
#[test]
fn read_keys_override_http_passphrase_but_never_private_socket_login_or_writes() {
    let (mut e, out) = catalog(Options {
        eea: true,
        key: Some("primary".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "secondary".into(),
            secret: "secondary-secret".into(),
            passphrase: Some("secondary-pass".into()),
        }),
        ..Options::default()
    });
    let config = out.iter().find(|a| is_get(a, "/account/config")).unwrap();
    assert!(
        matches!(config,Action::Http(r) if r.headers.iter().any(|(k,v)|*k=="OK-ACCESS-KEY"&&v=="secondary")&&r.headers.iter().any(|(k,v)|*k=="OK-ACCESS-PASSPHRASE"&&v=="secondary-pass"))
    );
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        5.0,
    );
    let out = prepared(&mut e, 5.0);
    let private = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    let out = prepared(&mut e, 7.0);
    assert!(out.iter().any(|a|matches!(a,Action::WsSend{text,..}if text.contains(r#""apiKey":"primary""#)&&text.contains(r#""passphrase":"primary-pass""#))));
    e.handle(
        8.0,
        Event::Command(Command::SetLeverage {
            command: 8,
            symbol: "BTCUSD261225".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let out = prepared(&mut e, 7.0);
    assert!(out.iter().any(|a|matches!(a,Action::Http(r) if r.headers.iter().any(|(k,v)|*k=="OK-ACCESS-KEY"&&v=="primary"))));
}
#[test]
fn collateral_uses_exact_swap_mark_not_xperp_basis() {
    let (mut e, _, out) = private_catalog();
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 2005.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let mut pending = out;
    while let Some(a) = pending.pop() {
        if let Action::Http(r) = &a {
            let body = if r.url.contains("account-position-risk") {
                envelope(r#"{"balData":[{"ccy":"BTC","eq":"1"}]}"#)
            } else {
                envelope("")
            };
            reply(&mut e, &a, &body, 6.0);
            pending.extend(prepared(&mut e, 6.0));
        }
    }
    let mut tickers = e.store().tickers().to_vec();
    tickers.rotate_right(2);
    for t in &mut tickers {
        t.mark = if t.id.ends_with("261225") {
            90000.0
        } else {
            42000.0
        };
    }
    e.store_mut().set_tickers(tickers);
    e.handle(7000.0, Event::TimerFired { timer });
    assert_eq!(e.store().balance().total, 42000.0);
    assert_eq!(e.store().balance().free, 42000.0);
}
#[test]
fn blank_untraded_xperp_ticker_fields_do_not_abort_other_seeded_markets() {
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = take_http(&mut prepared(&mut e, 1.0));
    reply(&mut e, &a, &envelope(&future()), 2.0);
    let a = take_http(&mut prepared(&mut e, 2.0));
    reply(&mut e, &a, &envelope(TICKER), 3.0);
    let a = take_http(&mut prepared(&mut e, 3.0));
    let ticker = ticker("BTC-USD-261225")
        .replace(r#""last":"40000""#, r#""last":"""#)
        .replace(r#""open24h":"40000""#, r#""open24h":"""#);
    reply(&mut e, &a, &envelope(&ticker), 4.0);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 2);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 40000.0);
    let t = e.store().ticker("BTCUSD261225").unwrap();
    assert_eq!(t.last, 0.0);
    assert_eq!(t.mark, 0.0);
    assert_eq!(t.percentage, 0.0);
    assert_eq!(
        prepared(&mut e, 4.0)
            .iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
}
#[test]
fn malformed_xperp_seed_values_start_at_zero_without_withholding_other_tickers() {
    for (good, bad) in [
        (r#""last":"40000""#, r#""last":"NaN""#),
        (r#""last":"40000""#, r#""last":"Infinity""#),
        (r#""last":"40000""#, r#""last":"bad""#),
        (r#""bidPx":"39999.9""#, r#""bidPx":"bad""#),
    ] {
        let mut e = crate::support::engine(Options {
            eea: true,
            ..Options::default()
        });
        e.start(0.0);
        let a = take_http(&mut prepared(&mut e, 0.0));
        reply(&mut e, &a, &envelope(MARKET), 1.0);
        let a = take_http(&mut prepared(&mut e, 1.0));
        reply(&mut e, &a, &envelope(&future()), 2.0);
        let a = take_http(&mut prepared(&mut e, 2.0));
        reply(&mut e, &a, &envelope(TICKER), 3.0);
        let a = take_http(&mut prepared(&mut e, 3.0));
        let row = ticker("BTC-USD-261225").replace(good, bad);
        reply(&mut e, &a, &envelope(&row), 4.0);
        assert!(e.store().loaded().tickers, "{bad}");
        assert_eq!(e.store().tickers().len(), 2, "{bad}");
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 40000.0);
        let seeded = e.store().ticker("BTCUSD261225").unwrap();
        assert_eq!(
            if bad.contains("bidPx") {
                seeded.bid
            } else {
                seeded.last
            },
            0.0,
            "{bad}"
        );
        let out = prepared(&mut e, 4.0);
        assert!(out.iter().any(|a| matches!(a, Action::WsConnect { .. })));
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message == "OKX seeded 1 malformed ticker rows as zero")), "{bad}");
    }
}

#[test]
fn unparseable_xperp_ticker_row_keeps_valid_swap_peers_and_logs_the_row() {
    let mut e = crate::support::engine(Options {
        eea: true,
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = take_http(&mut prepared(&mut e, 1.0));
    reply(&mut e, &a, &envelope(&future()), 2.0);
    let a = take_http(&mut prepared(&mut e, 2.0));
    reply(&mut e, &a, &envelope(TICKER), 3.0);
    let a = take_http(&mut prepared(&mut e, 3.0));
    reply(
        &mut e,
        &a,
        // A null quote now seeds zero; a non-string quote is still undecodable.
        &envelope(&ticker("BTC-USD-261225").replace(r#""last":"40000""#, r#""last":40000"#)),
        4.0,
    );
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 40000.0);
    assert!(prepared(&mut e, 3.0).iter().any(|a| matches!(a, Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message == "Skipped malformed public discovery row")));
}

#[test]
fn account_push_retains_real_usd_and_coin_details_and_live_order_margin() {
    let (mut e, private, out) = private_catalog();
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 2005.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    prepared(&mut e, 7.0);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    prepared(&mut e, 4.0);
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45400","details":[{"ccy":"BTC","eq":"1"},{"ccy":"USD","eq":"100"},{"ccy":"USDT","eq":"5300"}]}]}"#,
    );
    assert_eq!(e.store().balance().used, 800.0);
    assert_eq!(e.store().balance().upnl, 80.0);
    assert_eq!(
        e.store().balance().free,
        44534.0,
        "live order IMR must remain deducted"
    );
    assert_eq!(e.store().balance().total, 45320.0);
    let currencies = e.store().currency_balances().unwrap();
    let usd = currencies
        .iter()
        .find(|c| c.currency == "USD")
        .expect("real USD must not be treated as scalar sentinel");
    assert_eq!(usd.balance.total, 100.0);
    assert_eq!(usd.balance.free, 100.0);
    let btc = currencies.iter().find(|c| c.currency == "BTC").unwrap();
    assert!((btc.balance.used - 0.02165).abs() < 1e-12);
    assert_eq!(btc.balance.total, 0.998);
    // Account details are changed-currency deltas; the new USD row must not wipe BTC/USDT.
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45410","details":[{"ccy":"USD","eq":"110"}]}]}"#,
    );
    assert_eq!(e.store().currency_balances().unwrap().len(), 3);
    assert_eq!(e.store().balance().free, 44544.0);
    e.handle(2005.0, Event::TimerFired { timer });
    assert_eq!(
        e.store().balance().free,
        44544.0,
        "tick must retain scalar equity and order IMR"
    );
    assert_eq!(e.store().balance().total, 45330.0);
}

#[test]
fn malformed_account_details_are_atomic_and_scalar_only_push_keeps_currency_details() {
    let (mut e, private, out) = private_catalog();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    prepared(&mut e, 7.0);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    prepared(&mut e, 4.0);
    let before = e.store().balance().clone();
    let coins = e.store().currency_balances().unwrap().to_vec();
    for details in [
        r#"[{"ccy":"BTC","eq":"2"},{"ccy":"USD","eq":"NaN"}]"#,
        r#"[{"ccy":"BTC","eq":"2"},{"ccy":"BTC","eq":"3"}]"#,
        r#"[{"ccy":"USD"}]"#,
        r#"null"#,
    ] {
        ws(
            &mut e,
            private,
            &format!(
                r#"{{"arg":{{"channel":"account"}},"data":[{{"totalEq":"99999","details":{details}}}]}}"#
            ),
        );
        assert_eq!(e.store().balance(), &before);
        assert_eq!(e.store().currency_balances().unwrap(), coins);
    }
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45300"}]}"#,
    );
    assert_eq!(e.store().balance().free, 44434.0);
    assert_eq!(e.store().balance().total, 45220.0);
    assert_eq!(e.store().currency_balances().unwrap(), coins);
}

#[test]
fn native_usd_margin_uses_decimal_addition_and_subtraction_for_every_row() {
    let (mut e, private, out) = private_catalog();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    prepared(&mut e, 7.0);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    prepared(&mut e, 4.0);
    // Two independent positions accumulate exactly four USD of native IMR.
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USDT-SWAP","pos":"1","avgPx":"40000","ccy":"USD","imr":"1.2","upl":"-0.1"},{"instId":"BTC-USDT-261225","pos":"1","avgPx":"40000","ccy":"USD","imr":"2.8","upl":"-0.2"}]}"#,
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT-261225","ordId":"usd-order-one","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","ccy":"USD","lever":"125","reduceOnly":"false"}]}"#,
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45010.1","details":[{"ccy":"USD","eq":"10.1"}]}]}"#,
    );
    let usd = || {
        e.store()
            .currency_balances()
            .unwrap()
            .iter()
            .find(|c| c.currency == "USD")
            .unwrap()
            .balance
            .clone()
    };
    assert_eq!(usd().used, 7.52, "four position IMR plus 3.52 order IMR");
    assert_eq!(usd().free, 2.58);
    assert_eq!(usd().upnl, -0.3);
    assert_eq!(usd().total, 10.4);
    // A second order must be folded through decimal addition individually too.
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT-SWAP","ordId":"usd-order-two","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","ccy":"USD","lever":"400","reduceOnly":"false"}]}"#,
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45010.1"}]}"#,
    );
    let usd = &e
        .store()
        .currency_balances()
        .unwrap()
        .iter()
        .find(|c| c.currency == "USD")
        .unwrap()
        .balance;
    assert_eq!(usd.used, 8.62);
    assert_eq!(usd.free, 1.48);
    assert_eq!(usd.upnl, -0.3);
    assert_eq!(usd.total, 10.4);
}

#[test]
fn native_coin_margin_keeps_subcent_precision_and_negative_pnl() {
    let (mut e, private, out) = private_catalog();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    prepared(&mut e, 7.0);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    prepared(&mut e, 4.0);
    // Distinct products and live orders share BTC collateral; no USD-cent trim.
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USD-SWAP","pos":"1","avgPx":"40000","ccy":"BTC","imr":"0.0000000012","upl":"-0.0000000011"},{"instId":"BTC-USD-261225","pos":"1","avgPx":"40000","ccy":"BTC","imr":"0.0000000023","upl":"-0.0000000022"}]}"#,
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45000","details":[{"ccy":"BTC","eq":"1.00000001"}]}]}"#,
    );
    let btc = &e
        .store()
        .currency_balances()
        .unwrap()
        .iter()
        .find(|c| c.currency == "BTC")
        .unwrap()
        .balance;
    assert_eq!(btc.used, 0.0016500035);
    assert_eq!(btc.free, 0.9983500065);
    assert_eq!(btc.upnl, -0.0000000033);
    assert_eq!(btc.total, 1.0000000133);
    // At 16 decimals, the scaled sum is an odd integer above 2^52.
    // Math.round leaves that integer unchanged; adding 0.5 before floor does not.
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USD-SWAP","ordId":"swap-order","ordType":"limit","state":"canceled","side":"buy","sz":"4","accFillSz":"1"},{"instId":"BTC-USD-261225","ordId":"future-order","ordType":"limit","state":"canceled","side":"buy","sz":"4","accFillSz":"1"}]}"#,
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USD-SWAP","pos":"1","avgPx":"40000","ccy":"BTC","imr":"0.5","upl":"-0.0000000011"},{"instId":"BTC-USD-261225","pos":"1","avgPx":"40000","ccy":"BTC","imr":"0.0000000000000001","upl":"-0.0000000022"}]}"#,
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45000","details":[{"ccy":"BTC","eq":"1"}]}]}"#,
    );
    let btc = &e
        .store()
        .currency_balances()
        .unwrap()
        .iter()
        .find(|c| c.currency == "BTC")
        .unwrap()
        .balance;
    assert_eq!(btc.used, 0.5000000000000001);
    assert_eq!(btc.free, 0.4999999999999999);
    assert_eq!(btc.upnl, -0.0000000033);
    assert_eq!(btc.total, 1.0000000033);
}

#[test]
fn missing_order_currency_reserves_proven_settlement_on_rest_and_websocket() {
    for (instrument, lots, currency, native_reserve) in [
        ("BTC-USDT-SWAP", 100, "USDT", 4400.0),
        ("BTC-USD-SWAP", 400, "BTC", 0.11),
    ] {
        for currency_field in ["", r#", "ccy":"""#] {
            let (mut e, private, mut pending) = private_catalog();
            let order = |size| {
                format!(
                    r#"{{"instId":"{instrument}","ordId":"reserve-order","ordType":"limit","state":"live","side":"buy","sz":"{size}","accFillSz":"0","px":"40000","lever":"10","reduceOnly":"false"{currency_field}}}"#
                )
            };
            while let Some(request) = pending.pop() {
                let Action::Http(http) = &request else {
                    continue;
                };
                let body = if http.url.contains("account-position-risk") {
                    envelope(r#"{"balData":[{"ccy":"BTC","eq":"1"},{"ccy":"USDT","eq":"5000"}]}"#)
                } else if http.url.ends_with("/orders-pending?instType=SWAP") {
                    envelope(&order(lots))
                } else {
                    envelope("")
                };
                reply(&mut e, &request, &body, 6.0);
                pending.extend(prepared(&mut e, 6.0));
            }
            assert!(e.store().loaded().orders && e.store().loaded().balance);
            assert_eq!(e.store().balance().free, 40600.0);
            let ledger = e
                .store()
                .currency_balances()
                .unwrap()
                .iter()
                .find(|row| row.currency == currency)
                .unwrap();
            let equity = if currency == "BTC" { 1.0 } else { 5000.0 };
            assert_eq!(ledger.balance.used, native_reserve);
            assert_eq!(ledger.balance.free, equity - native_reserve);

            e.handle(7.0, Event::WsOpened { socket: private });
            prepared(&mut e, 7.0);
            ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
            prepared(&mut e, 4.0);
            ws(
                &mut e,
                private,
                &format!(
                    r#"{{"arg":{{"channel":"orders"}},"data":[{}]}}"#,
                    order(lots / 2)
                ),
            );
            ws(
                &mut e,
                private,
                r#"{"arg":{"channel":"account"},"data":[{"totalEq":"45000"}]}"#,
            );
            assert_eq!(e.store().balance().free, 42800.0);
            let ledger = e
                .store()
                .currency_balances()
                .unwrap()
                .iter()
                .find(|row| row.currency == currency)
                .unwrap();
            assert_eq!(ledger.balance.used, native_reserve / 2.0);
            assert_eq!(ledger.balance.free, equity - native_reserve / 2.0);
        }
    }
}

mod product_semantics {
    use crate::support::FixtureOptions as Options;
    use crate::support::*;
    use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, SocketId};
    use safe_cex_venue_okx::Okx;

    // OKX instruments API: inverse ctVal is quote currency; xperp rows use FUTURES.
    // https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments
    const INVERSE: &str = r#"{"instId":"BTC-USD-SWAP","instType":"SWAP","ctType":"inverse","ctVal":"100","ctValCcy":"USD","settleCcy":"BTC","uly":"BTC-USD","state":"live","lotSz":"1","tickSz":"0.1","minSz":"1","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"100"}"#;
    const XPERP: &str = r#"{"instId":"BTC-USD-UM","instType":"FUTURES","ruleType":"xperp","ctType":"linear","ctVal":"0.001","ctValCcy":"BTC","settleCcy":"USD","uly":"BTC-USD","state":"live","lotSz":"1","tickSz":"0.1","minSz":"1","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"10"}"#;
    fn ticker(id: &str) -> String {
        TICKER.replace("BTC-USDT-SWAP", id)
    }
    fn startup(eea: bool, private: bool) -> (Engine<Okx>, Vec<Action>) {
        let mut e = crate::support::engine(Options {
            eea,
            key: private.then(|| "test-key".into()),
            secret: private.then(|| "test-secret".into()),
            passphrase: private.then(|| "test-passphrase".into()),
            ..Options::default()
        });
        e.start(0.0);
        let request = take_http(&mut prepared(&mut e, 0.0));
        assert!(
            matches!(&request, Action::Http(r) if r.url.contains(if eea && private { "/account/instruments?" } else { "/public/instruments?" }))
        );
        reply(
            &mut e,
            &request,
            &envelope(&format!("{MARKET},{INVERSE}")),
            1.0,
        );
        if eea {
            assert!(!e.store().loaded().markets);
            let request = take_http(&mut prepared(&mut e, 1.0));
            assert!(matches!(&request, Action::Http(r) if r.url.ends_with("instType=FUTURES")));
            let dated = XPERP
                .replace("xperp", "normal")
                .replace("BTC-USD-UM", "BTC-USD-261225");
            let pre = XPERP
                .replace("xperp", "pre_market")
                .replace("BTC-USD-UM", "BTC-USD-PRE");
            reply(
                &mut e,
                &request,
                &envelope(&format!("{XPERP},{dated},{pre}")),
                1.5,
            );
        }
        let mut initial = prepared(&mut e, if eea { 1.5 } else { 1.0 });
        let request = take_http(&mut initial);
        reply(
            &mut e,
            &request,
            &envelope(&format!("{},{}", TICKER, ticker("BTC-USD-SWAP"))),
            2.0,
        );
        if eea {
            assert!(!e.store().loaded().tickers);
            let request = take_http(&mut prepared(&mut e, 2.0));
            reply(&mut e, &request, &envelope(&ticker("BTC-USD-UM")), 2.5);
        }
        initial.extend(prepared(&mut e, if eea { 2.5 } else { 2.0 }));
        (e, initial)
    }
    fn socket(initial: &[Action], suffix: &str) -> SocketId {
        initial
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with(suffix) => Some(*socket),
                _ => None,
            })
            .unwrap()
    }
    fn done(e: &mut Engine<Okx>) -> serde_json::Value {
        actions(e)
            .into_iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone { result: s, .. }) => {
                    Some(serde_json::from_str(&s).unwrap())
                }
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn global_inverse_and_eea_xperps_have_native_contract_metadata() {
        for eea in [false, true] {
            let (e, _) = startup(eea, false);
            assert_eq!(e.store().markets().len(), if eea { 3 } else { 2 });
            let m = e.store().market("BTCUSD").unwrap();
            assert_eq!((m.base.as_str(), m.quote.as_str()), ("BTC", "USD"));
            assert_eq!(m.is_inverse, Some(true));
            assert_eq!(m.contract_size, Some(100.0));
            if eea {
                let m = e.store().market("BTCUSDUM").unwrap();
                assert_eq!(m.pretty_symbol.as_deref(), Some("BTC/USD XPERP"));
                assert_eq!(m.is_inverse, Some(false));
            }
        }
    }

    #[test]
    fn eea_indexes_deduplicate_underlying_and_fan_out_to_swap_and_xperp() {
        let (mut e, initial) = startup(true, false);
        let public = socket(&initial, "/public");
        e.handle(3.0, Event::WsOpened { socket: public });
        let subscriptions: Vec<serde_json::Value> = prepared(&mut e, 3.0)
            .into_iter()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } if text != "ping" => {
                    Some(serde_json::from_str(&text).unwrap())
                }
                _ => None,
            })
            .collect();
        let count = subscriptions
            .iter()
            .flat_map(|s| s["args"].as_array().unwrap())
            .filter(|s| s["channel"] == "index-tickers" && s["instId"] == "BTC-USD")
            .count();
        assert_eq!(count, 1);
        ws(
            &mut e,
            public,
            r#"{"arg":{"channel":"index-tickers","instId":"BTC-USD"},"data":[{"instId":"BTC-USD","idxPx":"41000"}]}"#,
        );
        assert_eq!(e.store().ticker("BTCUSD").unwrap().index, 41000.0);
        assert_eq!(e.store().ticker("BTCUSDUM").unwrap().index, 41000.0);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().index, 40000.0);
        ws(
            &mut e,
            public,
            r#"{"arg":{"channel":"index-tickers","instId":"BTC-USD"},"data":[{"instId":"UNKNOWN-USD","idxPx":"1"},{"instId":"BTC-USD","idxPx":"42000"},{"instId":"BTC-USDT","idxPx":"43000"}]}"#,
        );
        assert_eq!(e.store().ticker("BTCUSD").unwrap().index, 42000.0);
        assert_eq!(e.store().ticker("BTCUSDUM").unwrap().index, 42000.0);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().index, 43000.0);
    }

    #[test]
    fn global_index_routes_keep_linear_and_inverse_underlyings_separate() {
        let (mut e, initial) = startup(false, false);
        let public = socket(&initial, "/public");
        e.handle(3.0, Event::WsOpened { socket: public });
        prepared(&mut e, 3.0);
        ws(
            &mut e,
            public,
            r#"{"arg":{"channel":"index-tickers","instId":"BTC-USD"},"data":[{"instId":"BTC-USD","idxPx":"41000"},{"instId":"UNKNOWN-USD","idxPx":"1"},{"instId":"BTC-USDT","idxPx":"42000"}]}"#,
        );
        assert_eq!(e.store().ticker("BTCUSD").unwrap().index, 41000.0);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().index, 42000.0);
    }

    #[test]
    fn failed_second_eea_catalog_page_never_publishes_a_partial_set() {
        let mut e = crate::support::engine(Options {
            eea: true,
            ..Options::default()
        });
        e.start(0.0);
        let request = take_http(&mut prepared(&mut e, 0.0));
        reply(&mut e, &request, &envelope(INVERSE), 1.0);
        let request = take_http(&mut prepared(&mut e, 1.0));
        reply(
            &mut e,
            &request,
            r#"{"code":"50000","msg":"failed","data":[]}"#,
            2.0,
        );
        assert!(!e.store().loaded().markets);
        assert!(e.store().markets().is_empty());
    }

    #[test]
    fn inverse_public_trades_and_fills_use_execution_price_and_quote_contract_value() {
        let (mut e, _) = startup(false, true);
        e.handle(
            4.0,
            Event::Command(Command::Query {
                command: 1,
                query: Query::PublicTrades {
                    symbol: "BTCUSD".into(),
                    limit: Some(1),
                },
            }),
        );
        let request = take_http(&mut prepared(&mut e, 4.0));
        reply(
            &mut e,
            &request,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","tradeId":"7","side":"buy","px":"50000","sz":"5","ts":"1700000000000"}"#,
            ),
            5.0,
        );
        let rows = done(&mut e);
        assert_eq!(rows[0]["qty"], 0.01);
        assert_eq!(rows[0]["quoteQty"], 500.0);
        e.handle(
            6.0,
            Event::Command(Command::Query {
                command: 2,
                query: Query::Fills {
                    symbol: Some("BTCUSD".into()),
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
            }),
        );
        let request = take_http(&mut prepared(&mut e, 6.0));
        reply(
            &mut e,
            &request,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","ordId":"9","tradeId":"8","side":"buy","fillPx":"50000","fillSz":"5","fillTime":"1700000000000","fee":"-0.00001","feeCcy":"BTC","fillPnl":"0.0001"}"#,
            ),
            7.0,
        );
        assert_eq!(done(&mut e)[0]["qty"], 0.01);
    }

    #[test]
    fn eea_scoped_history_uses_futures_and_account_wide_history_visits_both() {
        let (mut e, _) = startup(true, true);
        for symbol in [Some("BTCUSDUM".into()), None] {
            e.handle(
                4.0,
                Event::Command(Command::Query {
                    command: 1,
                    query: Query::OrderHistory {
                        symbol: symbol.clone(),
                        from_ms: None,
                        to_ms: None,
                        limit: None,
                    },
                }),
            );
            let request = take_http(&mut prepared(&mut e, 4.0));
            assert!(
                matches!(&request, Action::Http(r) if r.url.contains(if symbol.is_some() { "instType=FUTURES" } else { "instType=SWAP" }))
            );
            reply(&mut e, &request, &envelope(""), 5.0);
            if symbol.is_none() {
                let request = take_http(&mut prepared(&mut e, 5.0));
                assert!(matches!(&request, Action::Http(r) if r.url.contains("instType=FUTURES")));
                reply(&mut e, &request, &envelope(""), 6.0);
            }
            assert_eq!(done(&mut e), serde_json::json!([]));
        }
    }

    #[test]
    fn eea_private_reads_subscriptions_and_inverse_risk_keep_usd_units() {
        let (mut e, initial) = startup(true, true);
        let config = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(&mut e, config, &envelope(r#"{"posMode":"net_mode"}"#), 3.0);
        let initial = prepared(&mut e, 3.0);
        let private = socket(&initial, "/private");
        let positions = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
            .unwrap();
        let risk = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/account-position-risk?instType=SWAP"))
            .unwrap();
        assert!(
            initial
                .iter()
                .any(|a| is_get(a, "/api/v5/trade/orders-pending?instType=SWAP"))
        );
        reply(
            &mut e,
            risk,
            &envelope(r#"{"balData":[{"ccy":"BTC","eq":"1"}]}"#),
            4.0,
        );
        reply(
            &mut e,
            positions,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","ccy":"BTC","lever":"10","upl":"0.001","liqPx":"30000","mgnMode":"cross","imr":"0.01","mmr":"0.001"}"#,
            ),
            5.0,
        );
        assert!(!e.store().loaded().positions);
        let future = prepared(&mut e, 5.0)
            .into_iter()
            .find(|a| is_get(a, "/api/v5/account/positions?instType=FUTURES"))
            .unwrap();
        reply(&mut e, &future, &envelope(""), 5.5);
        let p = &e.store().positions()[0];
        assert_eq!(p.contracts, 0.0125);
        assert_eq!(p.notional, 500.0);
        assert_eq!(p.unrealized_pnl, 40.0);
        assert_eq!(e.store().balance().used, 400.0);
        prepared(&mut e, 5.5);
        e.handle(6.0, Event::WsOpened { socket: private });
        prepared(&mut e, 6.0);
        ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
        let out = prepared(&mut e, 4.0);
        assert!(out.iter().any(
        |a| matches!(a, Action::WsSend { text, .. } if text.contains("\"instType\":\"FUTURES\""))
    ));
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USD-SWAP","ordId":"1","ordType":"limit","state":"partially_filled","side":"buy","sz":"10","accFillSz":"5","px":"50000","fillPx":"50000","fillSz":"5","tradeId":"1","reduceOnly":false}]}"#,
        );
        let o = e.store().orders().iter().find(|o| o.id == "1").unwrap();
        assert_eq!(o.amount, 10.0);
        assert_eq!(o.filled, 0.01);
        assert_eq!(o.remaining, 0.01);
        assert_eq!(o.extra.as_ref().unwrap().remaining_native, Some(5.0));
        let frame = e.take_frame(10.0).unwrap();
        let mut at = 16;
        let mut found = false;
        while at < frame.len() {
            let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
            let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
            if kind == 11 {
                assert_eq!(
                    u16::from_le_bytes(frame[at + 2..at + 4].try_into().unwrap()),
                    2
                );
                let data = &frame[at + 8..at + 8 + len];
                let count = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
                assert_eq!(count, 1);
                let rows = (4 + 12 * count).next_multiple_of(8);
                assert_eq!(
                    f64::from_le_bytes(data[rows..rows + 8].try_into().unwrap()),
                    50000.0
                );
                assert_eq!(
                    f64::from_le_bytes(data[rows + 8..rows + 16].try_into().unwrap()),
                    0.01
                );
                found = true;
            }
            at += 8 + len;
        }
        assert!(found);
    }

    #[test]
    fn inverse_and_xperp_placements_keep_native_lots_and_caller_ids() {
        let (mut e, initial) = startup(true, true);
        let config = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(&mut e, config, &envelope(r#"{"posMode":"net_mode"}"#), 3.0);
        // Placement admission requires the complete account snapshot, including
        // both EEA products; config alone does not prove account readiness.
        let mut pending = prepared(&mut e, 3.0);
        while let Some(request) = pending.pop() {
            let Action::Http(http) = &request else {
                continue;
            };
            let body = if http.url.contains("account-position-risk") {
                envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
            } else {
                envelope("")
            };
            reply(&mut e, &request, &body, 3.5);
            pending.extend(prepared(&mut e, 3.5));
        }
        assert!(e.store().loaded().balance);
        assert!(e.store().loaded().positions);
        assert!(e.store().loaded().orders);
        let orders = serde_json::from_str(r#"[{"symbol":"BTCUSD","type":"limit","side":"buy","amount":5,"price":50000,"clientOrderId":"Inverse1"},{"symbol":"BTCUSDUM","type":"limit","side":"sell","amount":7,"price":50000,"clientOrderId":"EU1"}]"#).unwrap();
        e.handle(
            4.0,
            Event::Command(Command::PlaceOrders { command: 1, orders }),
        );
        let out = prepared(&mut e, 3.5);
        let request = out
            .iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body[0]["instId"], "BTC-USD-SWAP");
        assert_eq!(body[0]["sz"], "5");
        assert_eq!(body[0]["clOrdId"], "Inverse1");
        assert_eq!(body[1]["instId"], "BTC-USD-UM");
        assert_eq!(body[1]["sz"], "7");
        assert_eq!(body[1]["clOrdId"], "EU1");
    }

    #[test]
    fn inverse_order_history_converts_at_average_price_without_touching_ledger_currency() {
        let (mut e, _) = startup(false, true);
        e.handle(
            4.0,
            Event::Command(Command::Query {
                command: 1,
                query: Query::OrderHistory {
                    symbol: Some("BTCUSD".into()),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let request = take_http(&mut prepared(&mut e, 4.0));
        reply(
            &mut e,
            &request,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","ordId":"9","side":"buy","ordType":"limit","state":"canceled","px":"40000","avgPx":"50000","sz":"10","accFillSz":"5","cTime":"1700000000000","uTime":"1700000000001"}"#,
            ),
            5.0,
        );
        let rows = done(&mut e);
        assert_eq!(rows[0]["amount"], 0.02);
        assert_eq!(rows[0]["filled"], 0.01);
        assert_eq!(rows[0]["status"], "canceled");
        e.handle(
            6.0,
            Event::Command(Command::Query {
                command: 2,
                query: Query::Income {
                    symbol: Some("BTCUSD".into()),
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let request = take_http(&mut prepared(&mut e, 6.0));
        reply(
            &mut e,
            &request,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","billId":"10","type":"8","pnl":"0","fee":"0","balChg":"0.0001","ccy":"BTC","ts":"1700000000000"}"#,
            ),
            7.0,
        );
        let rows = done(&mut e);
        assert_eq!(rows[0]["income"], 0.0001);
        assert_eq!(rows[0]["asset"], "BTC");
    }

    #[test]
    fn eea_income_limit_joins_both_products_then_keeps_newest_whole_bill() {
        let (mut e, _) = startup(true, true);
        e.handle(
            4.0,
            Event::Command(Command::Query {
                command: 1,
                query: Query::Income {
                    symbol: None,
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
            }),
        );
        let request = take_http(&mut prepared(&mut e, 4.0));
        let old: Vec<_> = (1..=100).map(|i| format!(r#"{{"instId":"BTC-USD-SWAP","billId":"{i}","type":"8","pnl":"0","fee":"0","balChg":"0.0001","ccy":"BTC","ts":"1700000000000"}}"#)).collect();
        reply(&mut e, &request, &envelope(&old.join(",")), 5.0);
        let request = take_http(&mut prepared(&mut e, 5.0));
        assert!(matches!(&request, Action::Http(r) if r.url.contains("instType=FUTURES")));
        reply(
            &mut e,
            &request,
            &envelope(
                r#"{"instId":"BTC-USD-UM","billId":"101","type":"2","pnl":"2","fee":"-0.1","balChg":"1.9","ccy":"USD","ts":"1700000000001"}"#,
            ),
            6.0,
        );
        let rows = done(&mut e);
        assert_eq!(
            rows.as_array().unwrap().len(),
            2,
            "preserve both components of newest bill"
        );
        assert_eq!(rows[0]["symbol"], "BTCUSDUM");
        assert_eq!(rows[1]["symbol"], "BTCUSDUM");
    }

    #[test]
    fn eea_composite_history_stall_is_an_explicit_incomplete_failure() {
        let (mut e, _) = startup(true, true);
        e.handle(
            4.0,
            Event::Command(Command::Query {
                command: 1,
                query: Query::OrderHistory {
                    symbol: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let request = take_http(&mut prepared(&mut e, 4.0));
        let row = r#"{"instId":"BTC-USD-SWAP","ordId":"100","side":"buy","ordType":"limit","state":"filled","px":"50000","sz":"1","accFillSz":"1","cTime":"1700000000000"}"#;
        let page = envelope(&vec![row; 100].join(","));
        reply(&mut e, &request, &page, 5.0);
        let timer = prepared(&mut e, 5.0)
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(105.0, Event::TimerFired { timer });
        let request = take_http(&mut prepared(&mut e, 105.0));
        reply(&mut e, &request, &page, 106.0);
        let out = prepared(&mut e, 106.0);
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error: s, .. }) if s.legacy_text().contains("incomplete source"))));
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
        );
    }

    #[test]
    fn inverse_position_uses_native_mark_when_ticker_mark_is_unavailable() {
        let (mut e, initial) = startup(false, true);
        let public = socket(&initial, "/public");
        e.handle(3.0, Event::WsOpened { socket: public });
        prepared(&mut e, 3.0);
        ws(
            &mut e,
            public,
            r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USD-SWAP","markPx":"0"}]}"#,
        );
        let config = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(&mut e, config, &envelope(r#"{"posMode":"net_mode"}"#), 4.0);
        let pending = prepared(&mut e, 4.0);
        let positions = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("/positions?")))
            .unwrap();
        let risk = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("account-position-risk")))
            .unwrap();
        reply(
            &mut e,
            risk,
            &envelope(r#"{"balData":[{"ccy":"USD","eq":"1000"}]}"#),
            5.0,
        );
        reply(
            &mut e,
            positions,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","markPx":"50000","ccy":"BTC","lever":"10","upl":"0.001","mgnMode":"cross","imr":"0.01","mmr":"0.001"}"#,
            ),
            6.0,
        );
        let p = &e.store().positions()[0];
        assert_eq!(p.contracts, 0.01);
        assert_eq!(p.unrealized_pnl, 50.0);
        assert_eq!(p.notional, 500.0);
        assert_eq!(e.store().balance().used, 500.0);
    }

    #[test]
    fn live_preopen_optional_caps_do_not_abort_the_native_catalog() {
        // Recorded public instruments payload, 2026-10-20. The venue omits caps
        // for unavailable iceberg/trigger/TWAP orders while publishing real limits.
        let row = include_str!("fixtures/crml-preopen.json");
        let mut e = crate::support::engine(Options::default());
        e.start(0.0);
        let request = take_http(&mut prepared(&mut e, 0.0));
        reply(
            &mut e,
            &request,
            &envelope(&format!("{INVERSE},{row}")),
            1.0,
        );
        assert!(e.store().loaded().markets);
        assert_eq!(e.store().markets().len(), 2);
        let preopen = e.store().market("CRMLUSDT").unwrap();
        assert!(!preopen.active);
        assert_eq!(preopen.limits.amount.max, 20000.0);
        assert!(
            prepared(&mut e, 1.0)
                .iter()
                .any(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
        );
    }

    #[test]
    fn invalid_caps_skip_only_that_row_and_a_wholly_unusable_catalog_retries() {
        let native: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/crml-preopen.json")).unwrap();
        for value in ["malformed", "-1", "NaN", "inf", "0"] {
            let mut row = native.clone();
            row["maxTriggerSz"] = value.into();
            let mut e = crate::support::engine(Options::default());
            e.start(0.0);
            let request = take_http(&mut prepared(&mut e, 0.0));
            reply(
                &mut e,
                &request,
                &envelope(&format!("{INVERSE},{row}")),
                1.0,
            );
            assert!(e.store().loaded().markets, "{value}");
            assert_eq!(e.store().markets().len(), 1, "{value}");
            assert!(e.store().market("CRMLUSDT").is_none(), "{value}");
            assert_eq!(e.store().markets()[0].id, "BTC-USD-SWAP", "{value}");
        }
        let mut row = native;
        for key in [
            "maxIcebergSz",
            "maxLmtSz",
            "maxMktSz",
            "maxStopSz",
            "maxTriggerSz",
            "maxTwapSz",
        ] {
            row[key] = "".into();
        }
        let mut e = crate::support::engine(Options::default());
        e.start(0.0);
        let request = take_http(&mut prepared(&mut e, 1.0));
        reply(&mut e, &request, &envelope(&row.to_string()), 1.0);
        assert!(!e.store().loaded().markets);
        assert!(retry_timer(&prepared(&mut e, 1.0), 1.0, 1_000.0).is_some());
    }

    #[test]
    fn untraded_xperp_blank_last_is_unavailable_and_partial_stream_keeps_known_trade() {
        // Actual EEA ticker TEST002-USD_UM_XPERP-310822 publishes last/open24h="".
        let mut e = crate::support::engine(Options {
            eea: true,
            ..Options::default()
        });
        e.start(0.0);
        let request = take_http(&mut prepared(&mut e, 0.0));
        reply(&mut e, &request, &envelope(MARKET), 1.0);
        let request = take_http(&mut prepared(&mut e, 1.0));
        reply(
            &mut e,
            &request,
            &envelope(&XPERP.replace("BTC-USD-UM", "TEST002-USD_UM_XPERP-310822")),
            1.5,
        );
        let request = take_http(&mut prepared(&mut e, 1.0));
        reply(&mut e, &request, &envelope(TICKER), 2.0);
        let request = take_http(&mut prepared(&mut e, 2.0));
        reply(
            &mut e,
            &request,
            &envelope(
                r#"{"instId":"TEST002-USD_UM_XPERP-310822","last":"","open24h":"","bidPx":"0","askPx":"0","volCcy24h":"0","vol24h":"0"}"#,
            ),
            2.5,
        );
        assert!(e.store().loaded().tickers);
        assert_eq!(e.store().tickers().len(), 2);
        let t = e.store().ticker("TEST002USD_UM_XPERP310822").unwrap();
        assert_eq!(
            (t.last, t.mark, t.index, t.percentage),
            (0.0, 0.0, 0.0, 0.0)
        );
        let initial = prepared(&mut e, 2.0);
        let public = socket(&initial, "/public");
        e.handle(3.0, Event::WsOpened { socket: public });
        prepared(&mut e, 3.0);
        ws(
            &mut e,
            public,
            r#"{"arg":{"channel":"tickers"},"data":[{"instId":"BTC-USDT-SWAP","last":"","open24h":"","bidPx":"39999","askPx":"40001"}]}"#,
        );
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 40000.0);
        let known = e.store().ticker("BTCUSDT").unwrap().clone();
        for last in ["bad", "NaN", "inf"] {
            ws(
                &mut e,
                public,
                &format!(
                    r#"{{"arg":{{"channel":"tickers"}},"data":[{{"instId":"BTC-USDT-SWAP","last":"{last}","open24h":"40000","bidPx":"1","askPx":"2"}}]}}"#
                ),
            );
            assert_eq!(e.store().ticker("BTCUSDT").unwrap(), &known);
        }
    }

    #[test]
    fn nonempty_malformed_native_last_seeds_zero_and_keeps_the_ticker_row() {
        for last in ["garbage", "NaN", "inf"] {
            let mut e = crate::support::engine(Options::default());
            e.start(0.0);
            let request = take_http(&mut prepared(&mut e, 0.0));
            reply(&mut e, &request, &envelope(MARKET), 1.0);
            let request = take_http(&mut prepared(&mut e, 1.0));
            reply(
                &mut e,
                &request,
                &envelope(&TICKER.replace("\"last\":\"40000\"", &format!("\"last\":\"{last}\""))),
                2.0,
            );
            assert!(e.store().loaded().tickers);
            let t = e.store().ticker("BTCUSDT").unwrap();
            assert_eq!((t.last, t.percentage), (0.0, 0.0), "{last}");
            assert_eq!(t.bid, 39999.9);
        }
    }

    #[test]
    fn partial_position_push_uses_all_native_initial_margin_and_remains_stable_at_next_tick() {
        let (mut e, initial) = startup(false, true);
        let config = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(&mut e, config, &envelope(r#"{"posMode":"net_mode"}"#), 3.0);
        let pending = prepared(&mut e, 3.0);
        let private = socket(&pending, "/private");
        let positions = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("/positions?")))
            .unwrap();
        let risk = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("account-position-risk")))
            .unwrap();
        let tick = pending
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 2003.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        reply(
            &mut e,
            risk,
            &envelope(r#"{"balData":[{"ccy":"USD","eq":"10000"}]}"#),
            4.0,
        );
        reply(
            &mut e,
            positions,
            &envelope(
                r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","mgnMode":"cross","imr":"200","mmr":"20"},{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","ccy":"BTC","lever":"10","upl":"0.001","mgnMode":"cross","imr":"0.01","mmr":"0.001"}"#,
            ),
            5.0,
        );
        let orders = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("orders-pending?")))
            .unwrap();
        reply(
            &mut e,
            orders,
            &envelope(
                r#"{"instId":"BTC-USDT-SWAP","ordId":"77","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","lever":"inf"}"#,
            ),
            6.0,
        );
        let mut reads = prepared(&mut e, 6.0);
        while let Some(request) = reads.pop() {
            if matches!(&request, Action::Http(_)) {
                reply(&mut e, &request, &envelope(""), 6.5);
                reads.extend(prepared(&mut e, 6.5));
            }
        }
        assert_eq!(
            e.store().balance().free,
            8960.0,
            "unproven REST leverage reserves the full440USD instead of zero"
        );
        e.handle(7.0, Event::WsOpened { socket: private });
        prepared(&mut e, 7.0);
        ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
        prepared(&mut e, 4.0);
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT-SWAP","ordId":"77","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","lever":"10"}]}"#,
        );
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","ccy":"BTC","lever":"10","upl":"0.002","mgnMode":"cross","imr":"0.012","mmr":"0.0012"}]}"#,
        );
        let balance = e.store().balance().clone();
        assert_eq!(
            balance.used, 680.0,
            "linear200 + inverse0.012BTC*40000, not MMR"
        );
        assert_eq!(balance.upnl, 130.0);
        assert_eq!(balance.total, 9870.0);
        assert_eq!(balance.free, 9276.0, "retain44USD order reserve");
        assert_eq!(e.store().positions().len(), 2);
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"account"},"data":[{"totalEq":"12000"}]}"#,
        );
        let fresh = e.store().balance().clone();
        assert_eq!(fresh.used, 680.0);
        assert_eq!(
            fresh.free, 11276.0,
            "fresh equity retains the same44USD order reserve"
        );
        assert_eq!(fresh.total, 11870.0);
        let positions_before = e.store().positions().to_vec();
        let setting_before = e.store().setting("BTCUSD");
        for field in ["imr", "upl"] {
            for value in ["bad", "NaN", "inf", "", "1e308"] {
                let mut row = serde_json::json!({"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","ccy":"BTC","lever":"99","upl":"0.002","mgnMode":"cross","imr":"0.012"});
                row[field] = value.into();
                ws(
                    &mut e,
                    private,
                    &format!(r#"{{"arg":{{"channel":"positions"}},"data":[{row}]}}"#),
                );
                assert_eq!(e.store().balance(), &fresh, "{field}={value}");
                assert_eq!(e.store().positions(), positions_before);
                assert_eq!(e.store().setting("BTCUSD"), setting_before);
            }
        }
        for leverage in ["inf", "NaN", "bad"] {
            ws(
                &mut e,
                private,
                &format!(
                    r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT-SWAP","ordId":"77","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","lever":"{leverage}"}}]}}"#
                ),
            );
            ws(
                &mut e,
                private,
                r#"{"arg":{"channel":"account"},"data":[{"totalEq":"12000"}]}"#,
            );
            assert_eq!(
                e.store().balance().free,
                10880.0,
                "{leverage} keeps440USD reserve"
            );
        }
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT-SWAP","ordId":"77","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","lever":"10"}]}"#,
        );
        e.handle(2003.0, Event::TimerFired { timer: tick });
        assert_eq!(e.store().balance(), &fresh);
    }

    #[test]
    fn isolated_reported_margin_supplies_risk_when_native_imr_is_unavailable() {
        let (mut e, initial) = startup(false, true);
        let config = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(&mut e, config, &envelope(r#"{"posMode":"net_mode"}"#), 3.0);
        let pending = prepared(&mut e, 3.0);
        let private = socket(&pending, "/private");
        let positions = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("/positions?")))
            .unwrap();
        let risk = pending
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("account-position-risk")))
            .unwrap();
        reply(
            &mut e,
            risk,
            &envelope(r#"{"balData":[{"ccy":"USD","eq":"1000"}]}"#),
            4.0,
        );
        reply(
            &mut e,
            positions,
            &envelope(
                r#"{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","ccy":"BTC","lever":"10","upl":"0.001","mgnMode":"isolated","imr":"","margin":"0.01"}"#,
            ),
            5.0,
        );
        assert_eq!(e.store().positions().len(), 1);
        assert_eq!(e.store().balance().used, 400.0);
        assert_eq!(e.store().balance().upnl, 40.0);
        assert_eq!(e.store().balance().free, 600.0);
        e.handle(6.0, Event::WsOpened { socket: private });
        prepared(&mut e, 6.0);
        ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
        prepared(&mut e, 4.0);
        let before = e.store().balance().clone();
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","ccy":"BTC","upl":"0.001","mgnMode":"isolated","imr":"","margin":""}]}"#,
        );
        assert_eq!(e.store().balance(), &before);
        assert_eq!(e.store().positions().len(), 1);
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USD-SWAP","pos":"0","posSide":"net","ccy":"BTC","upl":"","mgnMode":"isolated","imr":"","margin":""}]}"#,
        );
        assert!(e.store().positions().is_empty());
        assert_eq!(e.store().balance().used, 0.0);
        assert_eq!(e.store().balance().upnl, 0.0);
        assert_eq!(e.store().balance().free, 1000.0);
    }

    const JOIN_SWAP_POSITION: &str = r#"{"instType":"SWAP","instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","mgnMode":"cross","imr":"200"}"#;
    const JOIN_FUTURE_POSITION: &str = r#"{"instType":"FUTURES","instId":"BTC-USD-UM","pos":"7","posSide":"net","avgPx":"39000","lever":"10","upl":"7","mgnMode":"cross","imr":"28"}"#;
    const JOIN_SWAP_ORDER: &str = r#"{"instType":"SWAP","instId":"BTC-USDT-SWAP","ordId":"301","ordType":"limit","state":"live","side":"buy","sz":"1","accFillSz":"0","px":"40000","lever":"10"}"#;
    const JOIN_FUTURE_ORDER: &str = r#"{"instType":"FUTURES","instId":"BTC-USD-UM","ordId":"302","ordType":"limit","state":"live","side":"buy","sz":"7","accFillSz":"0","px":"40000","lever":"10"}"#;

    fn eea_account() -> (Engine<Okx>, SocketId, Vec<Action>) {
        let (mut e, initial) = startup(true, true);
        let config = initial
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(&mut e, config, &envelope(r#"{"posMode":"net_mode"}"#), 3.0);
        let pending = prepared(&mut e, 3.0);
        let private = socket(&pending, "/private");
        e.handle(3.5, Event::WsOpened { socket: private });
        prepared(&mut e, 3.5);
        ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
        prepared(&mut e, 4.0);
        (
            e,
            private,
            pending
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_)))
                .collect(),
        )
    }

    #[test]
    fn eea_account_sources_are_explicit_and_commit_each_join_only_after_both_products() {
        let (mut e, _, pending) = eea_account();
        let risk = pending
            .iter()
            .find(|a| is_get(a, "/api/v5/account/account-position-risk?instType=SWAP"))
            .unwrap();
        reply(
            &mut e,
            risk,
            &envelope(r#"{"balData":[{"ccy":"USD","eq":"5000"}]}"#),
            4.0,
        );
        let positions = pending
            .iter()
            .find(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
            .unwrap();
        reply(&mut e, positions, &envelope(JOIN_SWAP_POSITION), 4.1);
        assert!(!e.store().loaded().positions && !e.store().loaded().balance);
        let future = prepared(&mut e, 4.1)
            .into_iter()
            .find(|a| is_get(a, "/api/v5/account/positions?instType=FUTURES"))
            .unwrap();
        reply(&mut e, &future, &envelope(JOIN_FUTURE_POSITION), 4.2);
        assert_eq!(e.store().positions().len(), 2);
        assert_eq!(e.store().balance().used, 228.0);
        let first = pending
            .into_iter()
            .find(|a| is_get(a, "/api/v5/trade/orders-pending?instType=SWAP"))
            .unwrap();
        let mut next = vec![first];
        let mut paths = Vec::new();
        while let Some(request) = next.pop() {
            let Action::Http(http) = &request else {
                panic!("request")
            };
            paths.push(
                http.url
                    .strip_prefix("https://eea.okx.com")
                    .unwrap()
                    .to_owned(),
            );
            let row = if http.url.ends_with("orders-pending?instType=SWAP") {
                JOIN_SWAP_ORDER
            } else if http.url.ends_with("orders-pending?instType=FUTURES") {
                JOIN_FUTURE_ORDER
            } else {
                ""
            };
            assert!(
                e.store().orders().is_empty(),
                "no product or order-kind subset is publishable"
            );
            reply(&mut e, &request, &envelope(row), 5.0);
            next.extend(
                prepared(&mut e, 5.0)
                    .into_iter()
                    .filter(|a| matches!(a, Action::Http(_))),
            );
        }
        let expected: Vec<_> = ["SWAP", "FUTURES"]
            .into_iter()
            .flat_map(|product| {
                std::iter::once(format!("/api/v5/trade/orders-pending?instType={product}")).chain(
                    ["oco", "conditional", "trigger", "move_order_stop"].map(move |kind| {
                        format!(
                            "/api/v5/trade/orders-algo-pending?instType={product}&ordType={kind}"
                        )
                    }),
                )
            })
            .collect();
        assert_eq!(paths, expected);
        assert_eq!(e.store().orders().len(), 2);
        assert!(e.store().orders().iter().any(|o| o.id == "301"));
        assert!(e.store().orders().iter().any(|o| o.id == "302"));
    }

    #[test]
    fn eea_failed_later_product_preserves_proven_positions_equity_and_orders() {
        let (mut e, private, pending) = eea_account();
        ws(
            &mut e,
            private,
            &format!(
                r#"{{"arg":{{"channel":"positions"}},"data":[{JOIN_SWAP_POSITION},{JOIN_FUTURE_POSITION}]}}"#
            ),
        );
        ws(
            &mut e,
            private,
            &format!(
                r#"{{"arg":{{"channel":"orders"}},"data":[{}]}}"#,
                JOIN_SWAP_ORDER.replace("301", "900")
            ),
        );
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"account"},"data":[{"totalEq":"6000"}]}"#,
        );
        let positions_before = e.store().positions().to_vec();
        let orders_before = e.store().orders().to_vec();
        let balance_before = e.store().balance().clone();
        let mut next = pending;
        while let Some(request) = next.pop() {
            let Action::Http(http) = &request else {
                continue;
            };
            let body = if http.url.contains("instType=FUTURES") {
                r#"{"code":"50000","msg":"later product failed","data":[]}"#.into()
            } else if http.url.contains("account-position-risk") {
                envelope(r#"{"balData":[{"ccy":"USD","eq":"1234"}]}"#)
            } else if http.url.contains("/positions?") {
                envelope(&JOIN_SWAP_POSITION.replace("\"pos\":\"5\"", "\"pos\":\"1\""))
            } else if http.url.contains("/orders-pending?") {
                envelope(JOIN_SWAP_ORDER)
            } else {
                envelope("")
            };
            reply(&mut e, &request, &body, 5.0);
            next.extend(
                prepared(&mut e, 5.0)
                    .into_iter()
                    .filter(|a| matches!(a, Action::Http(_))),
            );
            assert_eq!(e.store().positions(), positions_before);
            assert_eq!(e.store().orders(), orders_before);
            assert_eq!(e.store().balance(), &balance_before);
        }
    }

    #[test]
    fn eea_late_futures_snapshot_keeps_newer_pushes_from_both_products_and_equity() {
        let (mut e, private, pending) = eea_account();
        let risk = pending
            .iter()
            .find(|a| is_get(a, "/api/v5/account/account-position-risk?instType=SWAP"))
            .unwrap();
        reply(
            &mut e,
            risk,
            &envelope(r#"{"balData":[{"ccy":"USD","eq":"5000"}]}"#),
            4.0,
        );
        let positions = pending
            .iter()
            .find(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
            .unwrap();
        reply(&mut e, positions, &envelope(JOIN_SWAP_POSITION), 4.1);
        let future = prepared(&mut e, 4.1)
            .into_iter()
            .find(|a| is_get(a, "/api/v5/account/positions?instType=FUTURES"))
            .unwrap();
        let pushed_swap = JOIN_SWAP_POSITION
            .replace("\"pos\":\"5\"", "\"pos\":\"6\"")
            .replace("\"imr\":\"200\"", "\"imr\":\"240\"");
        let pushed_future = JOIN_FUTURE_POSITION
            .replace("\"pos\":\"7\"", "\"pos\":\"8\"")
            .replace("\"imr\":\"28\"", "\"imr\":\"32\"");
        ws(
            &mut e,
            private,
            &format!(
                r#"{{"arg":{{"channel":"positions"}},"data":[{pushed_swap},{pushed_future}]}}"#
            ),
        );
        ws(
            &mut e,
            private,
            r#"{"arg":{"channel":"account"},"data":[{"totalEq":"6000"}]}"#,
        );
        let proven = e.store().positions().to_vec();
        reply(&mut e, &future, &envelope(JOIN_FUTURE_POSITION), 6.0);
        assert_eq!(e.store().positions(), proven);
        assert_eq!(e.store().balance().used, 272.0);
        assert_eq!(e.store().balance().total, 5943.0);
    }

    #[test]
    fn eea_wrong_product_or_contradictory_native_type_rejects_joined_snapshots() {
        for row in [
            JOIN_SWAP_POSITION.to_owned(),
            JOIN_FUTURE_POSITION.replace("\"instType\":\"FUTURES\"", "\"instType\":\"SWAP\""),
        ] {
            let (mut e, _, pending) = eea_account();
            let risk = pending
                .iter()
                .find(|a| is_get(a, "/api/v5/account/account-position-risk?instType=SWAP"))
                .unwrap();
            reply(
                &mut e,
                risk,
                &envelope(r#"{"balData":[{"ccy":"USD","eq":"5000"}]}"#),
                4.0,
            );
            let positions = pending
                .iter()
                .find(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
                .unwrap();
            reply(&mut e, positions, &envelope(JOIN_SWAP_POSITION), 4.1);
            let future = prepared(&mut e, 4.1)
                .into_iter()
                .find(|a| is_get(a, "/api/v5/account/positions?instType=FUTURES"))
                .unwrap();
            reply(&mut e, &future, &envelope(&row), 4.2);
            assert!(!e.store().loaded().positions && !e.store().loaded().balance);
            assert!(e.store().positions().is_empty());
        }
        for row in [
            JOIN_SWAP_ORDER.to_owned(),
            JOIN_FUTURE_ORDER.replace("\"instType\":\"FUTURES\"", "\"instType\":\"SWAP\""),
        ] {
            let (mut e, _, pending) = eea_account();
            let mut next: Vec<_> = pending
                .into_iter()
                .filter(|a| is_get(a, "/api/v5/trade/orders-pending?instType=SWAP"))
                .collect();
            while let Some(request) = next.pop() {
                let Action::Http(http) = &request else {
                    continue;
                };
                let data = if http.url.ends_with("orders-pending?instType=SWAP") {
                    JOIN_SWAP_ORDER
                } else if http.url.ends_with("orders-pending?instType=FUTURES") {
                    &row
                } else {
                    ""
                };
                reply(&mut e, &request, &envelope(data), 5.0);
                next.extend(
                    prepared(&mut e, 5.0)
                        .into_iter()
                        .filter(|a| matches!(a, Action::Http(_))),
                );
            }
            assert!(!e.store().loaded().orders);
            assert!(e.store().orders().is_empty());
        }
    }

    #[test]
    fn eea_delayed_futures_order_walk_keeps_newer_pushes_from_both_products() {
        let (mut e, private, pending) = eea_account();
        let mut next: Vec<_> = pending
            .into_iter()
            .filter(|a| is_get(a, "/api/v5/trade/orders-pending?instType=SWAP"))
            .collect();
        while let Some(request) = next.pop() {
            let Action::Http(http) = &request else {
                continue;
            };
            if http.url.ends_with("orders-pending?instType=FUTURES") {
                ws(
                    &mut e,
                    private,
                    &format!(
                        r#"{{"arg":{{"channel":"orders"}},"data":[{JOIN_SWAP_ORDER},{JOIN_FUTURE_ORDER}]}}"#
                    ),
                );
                ws(
                    &mut e,
                    private,
                    &format!(
                        r#"{{"arg":{{"channel":"orders"}},"data":[{}]}}"#,
                        JOIN_SWAP_ORDER.replace("\"state\":\"live\"", "\"state\":\"canceled\"")
                    ),
                );
                ws(
                    &mut e,
                    private,
                    &format!(
                        r#"{{"arg":{{"channel":"orders"}},"data":[{}]}}"#,
                        JOIN_FUTURE_ORDER.replace("\"sz\":\"7\"", "\"sz\":\"8\"")
                    ),
                );
            }
            let row = if http.url.ends_with("orders-pending?instType=SWAP") {
                JOIN_SWAP_ORDER
            } else if http.url.ends_with("orders-pending?instType=FUTURES") {
                JOIN_FUTURE_ORDER
            } else {
                ""
            };
            reply(&mut e, &request, &envelope(row), 6.0);
            next.extend(
                prepared(&mut e, 6.0)
                    .into_iter()
                    .filter(|a| matches!(a, Action::Http(_))),
            );
        }
        assert_eq!(e.store().orders().len(), 2);
        assert_eq!(
            e.store()
                .orders()
                .iter()
                .find(|o| o.id == "301")
                .unwrap()
                .status,
            safe_cex_engine::model::OrderStatus::Canceled
        );
        assert_eq!(
            e.store()
                .orders()
                .iter()
                .find(|o| o.id == "302")
                .unwrap()
                .amount,
            8.0
        );
    }

    #[test]
    fn eea_reconnect_retires_pending_second_product_for_positions_and_orders() {
        let (mut e, private, pending) = eea_account();
        let mut next = pending;
        let mut stale = Vec::new();
        while let Some(request) = next.pop() {
            let Action::Http(http) = &request else {
                continue;
            };
            if http.url.contains("instType=FUTURES") {
                stale.push(request);
                continue;
            }
            let row = if http.url.contains("account-position-risk") {
                r#"{"balData":[{"ccy":"USD","eq":"5000"}]}"#
            } else if http.url.contains("/positions?") {
                JOIN_SWAP_POSITION
            } else if http.url.contains("/orders-pending?") {
                JOIN_SWAP_ORDER
            } else {
                ""
            };
            reply(&mut e, &request, &envelope(row), 5.0);
            next.extend(
                prepared(&mut e, 5.0)
                    .into_iter()
                    .filter(|a| matches!(a, Action::Http(_))),
            );
        }
        assert_eq!(stale.len(), 2);
        assert!(!e.store().loaded().positions && !e.store().loaded().orders);
        e.handle(
            100.0,
            Event::WsClosed {
                socket: private,
                code: 1006,
            },
        );
        let timer = prepared(&mut e, 5.0)
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(2000.0, Event::TimerFired { timer });
        let socket = socket(&prepared(&mut e, 2000.0), "/private");
        e.handle(2001.0, Event::WsOpened { socket });
        prepared(&mut e, 2001.0);
        e.handle(
            2002.0,
            Event::WsMessage {
                socket,
                text: r#"{"event":"login","code":"0"}"#,
            },
        );
        let mut handed_off = prepared(&mut e, 2002.0);
        let wakeups: Vec<_> = handed_off
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 2002.0 => Some(*timer),
                _ => None,
            })
            .collect();
        for timer in wakeups {
            e.handle(2002.0, Event::TimerFired { timer });
            handed_off.extend(prepared(&mut e, 2002.0));
        }
        let mut fresh: Vec<_> = handed_off
            .into_iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .collect();
        while let Some(request) = fresh.pop() {
            let Action::Http(http) = &request else {
                continue;
            };
            let row = if http.url.contains("account-position-risk") {
                r#"{"balData":[{"ccy":"USD","eq":"6000"}]}"#
            } else {
                ""
            };
            reply(&mut e, &request, &envelope(row), 2002.0);
            fresh.extend(
                prepared(&mut e, 2002.0)
                    .into_iter()
                    .filter(|a| matches!(a, Action::Http(_))),
            );
        }
        assert!(e.store().loaded().positions && e.store().loaded().orders);
        assert_eq!(e.store().balance().total, 6000.0);
        for request in stale {
            let Action::Http(http) = &request else {
                continue;
            };
            let row = if http.url.contains("/positions?") {
                JOIN_FUTURE_POSITION
            } else {
                JOIN_FUTURE_ORDER
            };
            reply(&mut e, &request, &envelope(row), 2003.0);
        }
        assert!(e.store().positions().is_empty() && e.store().orders().is_empty());
        assert_eq!(e.store().balance().total, 6000.0);
        assert!(
            !prepared(&mut e, 2003.0)
                .iter()
                .any(|a| matches!(a, Action::Http(_))),
            "retired FUTURES completion must not advance old order kinds"
        );
    }
}

#[test]
fn open_position_on_a_skipped_instrument_is_reported_not_silently_hidden() {
    let mut e = crate::support::engine(Options {
        key: Some("k".into()),
        secret: Some("s".into()),
        passphrase: Some("p".into()),
        ..Options::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    let broken = r#"{"instType":"SWAP","instId":"ETH-USDT-SWAP","ctType":"linear","ctVal":"bad"}"#;
    reply(&mut e, &a, &envelope(&format!("{MARKET},{broken}")), 1.0);
    // Private startup begins with the catalogue, alongside the ticker seed.
    let out = prepared(&mut e, 1.0);
    let tickers = out
        .iter()
        .find(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
        .unwrap();
    reply(&mut e, tickers, &envelope(TICKER), 2.0);
    let a = out
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        &a,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let mut pending: Vec<_> = prepared(&mut e, 3.0)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    let mut logs = Vec::new();
    while let Some(a) = pending.pop() {
        let body = if matches!(&a, Action::Http(r) if r.url.contains("/account/positions")) {
            envelope(
                r#"{"instType":"SWAP","instId":"ETH-USDT-SWAP","pos":"3","posSide":"net","avgPx":"2000","lever":"5","mgnMode":"cross"}"#,
            )
        } else if matches!(&a, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut e, &a, &body, 4.0);
        for out in prepared(&mut e, 4.0) {
            match out {
                Action::Http(_) => pending.push(out),
                Action::Notice(safe_cex_engine::Notice::Event(event)) => logs.push(event.message),
                _ => {}
            }
        }
    }
    assert!(
        logs.iter()
            .any(|m| m.contains("ETH-USDT-SWAP") && m.contains("not shown")),
        "{logs:?}"
    );
}
