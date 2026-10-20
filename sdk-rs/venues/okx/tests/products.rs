#![allow(dead_code)]
mod support;

use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Response, SocketId};
use safe_cex_venue_okx::{Okx, Options};
use support::*;

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
    let mut e = Engine::new(Okx::new(options));
    e.start(0.0);
    let a = actions(&mut e).remove(0);
    assert!(is_get(&a, "instruments?instType=SWAP"));
    if eea {
        assert!(
            matches!(&a,Action::Http(r) if r.url.contains(if private { "/account/instruments?" } else { "/public/instruments?" }))
        );
    }
    reply(&mut e, &a, &rows(&[MARKET, &inverse()]), 1.0);
    if eea {
        assert!(!e.store().loaded().markets);
        let a = actions(&mut e).remove(0);
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
    let a = actions(&mut e).remove(0);
    assert!(is_get(&a, "tickers?instType=SWAP"));
    reply(&mut e, &a, &rows(&[TICKER, &ticker("BTC-USD-SWAP")]), 3.0);
    if eea {
        assert!(!e.store().loaded().tickers);
        let a = actions(&mut e).remove(0);
        assert!(is_get(&a, "tickers?instType=FUTURES"));
        reply(
            &mut e,
            &a,
            &rows(&[&ticker("BTC-USD-261225"), &ticker("BTC-USDT-261225")]),
            4.0,
        );
    }
    let out = actions(&mut e);
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
    let out = actions(&mut e);
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
        pending.extend(actions(e));
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
    let sent = actions(&mut e);
    assert!(sent.iter().any(|a|matches!(a,Action::WsSend{text,..} if text.contains(r#""channel":"index-tickers","instId":"BTC-USD""#))));
    ws(
        &mut e,
        public,
        r#"{"arg":{"channel":"index-tickers","instId":"BTC-USD"},"data":[{"instId":"BTC-USD","idxPx":"41000"}]}"#,
    );
    assert_eq!(e.store().ticker("BTCUSD").unwrap().index, 41000.0);
    assert_eq!(e.store().ticker("BTCUSD261225").unwrap().index, 41000.0);
    assert_eq!(e.store().ticker("BTCUSDT261225").unwrap().index, 40000.0);
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
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    let out = actions(&mut e);
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
    let futures = actions(&mut e)
        .into_iter()
        .find(|a| is_get(a, "positions?instType=FUTURES"))
        .unwrap();
    assert!(!e.store().loaded().positions);
    e.handle(7.0,Event::Command(Command::PlaceOrders {command:99,orders:vec![serde_json::from_str(r#"{"symbol":"BTCUSD261225","type":"limit","side":"buy","amount":2,"price":40000}"#).unwrap()]}));
    assert!(actions(&mut e).iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:99,result:Err(error)}) if error.contains("not ready"))));
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
    let out = actions(&mut e);
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
fn failed_xperp_catalog_never_publishes_only_the_swap_half() {
    let mut e = Engine::new(Okx::new(Options {
        eea: true,
        ..Options::default()
    }));
    e.start(0.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = actions(&mut e).remove(0);
    reply(
        &mut e,
        &a,
        &envelope(
            r#"{"instType":"FUTURES","ruleType":"xperp","ctType":"inverse","instId":"BTC-USD-261225"}"#,
        ),
        2.0,
    );
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::Http(_)))
    );
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
        }),
    );
    let a = actions(&mut e)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    assert!(
        matches!(&a,Action::Http(r) if r.body.as_deref().unwrap().contains(r#""instId":"BTC-USD-261225""#))
    );
    reply(&mut e, &a, &envelope(""), 8.0);
    let out = actions(&mut e);
    let swap = out
        .iter()
        .find(|a| is_get(a, "positions?instType=SWAP"))
        .unwrap();
    reply(&mut e, swap, &envelope(""), 9.0);
    let future = actions(&mut e)
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
    let out = actions(&mut e);
    let private = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    complete(out, &mut e);
    e.handle(7.0, Event::WsOpened { socket: private });
    let out = actions(&mut e);
    assert!(out.iter().any(|a|matches!(a,Action::WsSend{text,..}if text.contains(r#""apiKey":"primary""#)&&text.contains(r#""passphrase":"primary-pass""#))));
    e.handle(
        8.0,
        Event::Command(Command::SetLeverage {
            command: 8,
            symbol: "BTCUSD261225".into(),
            leverage: 5.0,
        }),
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a|matches!(a,Action::Http(r) if r.headers.iter().any(|(k,v)|*k=="OK-ACCESS-KEY"&&v=="primary"))));
}
#[test]
fn collateral_uses_exact_swap_mark_not_xperp_basis_and_unknown_currency_falls_back() {
    let (mut e, _, out) = private_catalog();
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 2005.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    // Keep a second equity currency with no corresponding USD swap.
    let mut pending = out;
    while let Some(a) = pending.pop() {
        if let Action::Http(r) = &a {
            let body = if r.url.contains("account-position-risk") {
                envelope(r#"{"balData":[{"ccy":"BTC","eq":"1"},{"ccy":"UNKNOWN","eq":"7"}]}"#)
            } else {
                envelope("")
            };
            reply(&mut e, &a, &body, 6.0);
            pending.extend(actions(&mut e));
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
    assert_eq!(e.store().balance().total, 42007.0);
    assert_eq!(e.store().balance().free, 42007.0);
}
#[test]
fn blank_untraded_xperp_ticker_fields_do_not_abort_other_seeded_markets() {
    let mut e = Engine::new(Okx::new(Options {
        eea: true,
        ..Options::default()
    }));
    e.start(0.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(&future()), 2.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(TICKER), 3.0);
    let a = actions(&mut e).remove(0);
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
        actions(&mut e)
            .iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
}
#[test]
fn malformed_xperp_seed_remains_atomic_despite_permitted_blank_values() {
    for bad in [
        r#""last":"NaN""#,
        r#""last":"Infinity""#,
        r#""last":"bad""#,
        r#""last":null"#,
    ] {
        let mut e = Engine::new(Okx::new(Options {
            eea: true,
            ..Options::default()
        }));
        e.start(0.0);
        let a = actions(&mut e).remove(0);
        reply(&mut e, &a, &envelope(MARKET), 1.0);
        let a = actions(&mut e).remove(0);
        reply(&mut e, &a, &envelope(&future()), 2.0);
        let a = actions(&mut e).remove(0);
        reply(&mut e, &a, &envelope(TICKER), 3.0);
        let a = actions(&mut e).remove(0);
        reply(
            &mut e,
            &a,
            &envelope(&ticker("BTC-USD-261225").replace(r#""last":"40000""#, bad)),
            4.0,
        );
        assert!(!e.store().loaded().tickers, "{bad}");
        assert!(e.store().tickers().is_empty(), "partial SWAP seed leaked");
        assert!(
            !actions(&mut e)
                .iter()
                .any(|a| matches!(a, Action::WsConnect { .. } | Action::Http(_)))
        );
    }
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
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
