#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, SocketId};
use safe_cex_venue_okx::{Okx, Options};
use support::*;

// OKX instruments API: inverse ctVal is quote currency; xperp rows use FUTURES.
// https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments
const INVERSE: &str = r#"{"instId":"BTC-USD-SWAP","instType":"SWAP","ctType":"inverse","ctVal":"100","ctValCcy":"USD","settleCcy":"BTC","uly":"BTC-USD","state":"live","lotSz":"1","tickSz":"0.1","minSz":"1","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"100"}"#;
const XPERP: &str = r#"{"instId":"BTC-USD-UM","instType":"FUTURES","ruleType":"xperp","ctType":"linear","ctVal":"0.001","ctValCcy":"BTC","settleCcy":"USD","uly":"BTC-USD","state":"live","lotSz":"1","tickSz":"0.1","minSz":"1","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"10"}"#;
fn ticker(id: &str) -> String {
    TICKER.replace("BTC-USDT-SWAP", id)
}
fn startup(eea: bool, private: bool) -> (Engine<Okx>, Vec<Action>) {
    let mut e = Engine::new(Okx::new(Options {
        eea,
        key: private.then(|| "test-key".into()),
        secret: private.then(|| "test-secret".into()),
        passphrase: private.then(|| "test-passphrase".into()),
        ..Options::default()
    }));
    e.start(0.0);
    let request = actions(&mut e).remove(0);
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
        let request = actions(&mut e).remove(0);
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
    let request = actions(&mut e).remove(0);
    reply(
        &mut e,
        &request,
        &envelope(&format!("{},{}", TICKER, ticker("BTC-USD-SWAP"))),
        2.0,
    );
    if eea {
        assert!(!e.store().loaded().tickers);
        let request = actions(&mut e).remove(0);
        reply(&mut e, &request, &envelope(&ticker("BTC-USD-UM")), 2.5);
    }
    let initial = actions(&mut e);
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
            Action::Notice(Notice::CommandDone { result: Ok(s), .. }) => {
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
    let subscriptions: Vec<serde_json::Value> = actions(&mut e)
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
    actions(&mut e);
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
    let mut e = Engine::new(Okx::new(Options {
        eea: true,
        ..Options::default()
    }));
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(INVERSE), 1.0);
    let request = actions(&mut e).remove(0);
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
    let request = actions(&mut e).remove(0);
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
    let request = actions(&mut e).remove(0);
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
        let request = actions(&mut e).remove(0);
        assert!(
            matches!(&request, Action::Http(r) if r.url.contains(if symbol.is_some() { "instType=FUTURES" } else { "instType=SWAP" }))
        );
        reply(&mut e, &request, &envelope(""), 5.0);
        if symbol.is_none() {
            let request = actions(&mut e).remove(0);
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
    let initial = actions(&mut e);
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
    let future = actions(&mut e)
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/positions?instType=FUTURES"))
        .unwrap();
    reply(&mut e, &future, &envelope(""), 5.5);
    let p = &e.store().positions()[0];
    assert_eq!(p.contracts, 0.0125);
    assert_eq!(p.notional, 500.0);
    assert_eq!(p.unrealized_pnl, 40.0);
    assert_eq!(e.store().balance().used, 400.0);
    actions(&mut e);
    e.handle(6.0, Event::WsOpened { socket: private });
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    let out = actions(&mut e);
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
                u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap()),
                1
            );
            assert_eq!(
                f64::from_le_bytes(frame[at + 8 + len - 8..at + 8 + len].try_into().unwrap()),
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
    actions(&mut e);
    let orders = serde_json::from_str(r#"[{"symbol":"BTCUSD","type":"limit","side":"buy","amount":5,"price":50000,"clientOrderId":"Inverse1"},{"symbol":"BTCUSDUM","type":"limit","side":"sell","amount":7,"price":50000,"clientOrderId":"EU1"}]"#).unwrap();
    e.handle(
        4.0,
        Event::Command(Command::PlaceOrders { command: 1, orders }),
    );
    let out = actions(&mut e);
    let request = out
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
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
    let request = actions(&mut e).remove(0);
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
    let request = actions(&mut e).remove(0);
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
    let request = actions(&mut e).remove(0);
    let old: Vec<_> = (1..=100).map(|i| format!(r#"{{"instId":"BTC-USD-SWAP","billId":"{i}","type":"8","pnl":"0","fee":"0","balChg":"0.0001","ccy":"BTC","ts":"1700000000000"}}"#)).collect();
    reply(&mut e, &request, &envelope(&old.join(",")), 5.0);
    let request = actions(&mut e).remove(0);
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
    let request = actions(&mut e).remove(0);
    let row = r#"{"instId":"BTC-USD-SWAP","ordId":"100","side":"buy","ordType":"limit","state":"filled","px":"50000","sz":"1","accFillSz":"1","cTime":"1700000000000"}"#;
    let page = envelope(&vec![row; 100].join(","));
    reply(&mut e, &request, &page, 5.0);
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(105.0, Event::TimerFired { timer });
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &page, 106.0);
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Err(s), .. }) if s.contains("incomplete source"))));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { result: Ok(_), .. })))
    );
}

#[test]
fn inverse_position_uses_native_mark_when_ticker_mark_is_unavailable() {
    let (mut e, initial) = startup(false, true);
    let public = socket(&initial, "/public");
    e.handle(3.0, Event::WsOpened { socket: public });
    actions(&mut e);
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
    let pending = actions(&mut e);
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
    let mut e = Engine::new(Okx::new(Options::default()));
    e.start(0.0);
    let request = actions(&mut e).remove(0);
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
        actions(&mut e)
            .iter()
            .any(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
    );
}

#[test]
fn supplied_invalid_or_wholly_absent_caps_never_publish_a_partial_catalog() {
    let native: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/crml-preopen.json")).unwrap();
    for value in ["malformed", "-1", "NaN", "inf", "0"] {
        let mut row = native.clone();
        row["maxTriggerSz"] = value.into();
        let mut e = Engine::new(Okx::new(Options::default()));
        e.start(0.0);
        let request = actions(&mut e).remove(0);
        reply(
            &mut e,
            &request,
            &envelope(&format!("{INVERSE},{row}")),
            1.0,
        );
        assert!(!e.store().loaded().markets, "{value}");
        assert!(e.store().markets().is_empty());
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
    let mut e = Engine::new(Okx::new(Options::default()));
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(&row.to_string()), 1.0);
    assert!(!e.store().loaded().markets);
}

#[test]
fn untraded_xperp_blank_last_is_unavailable_and_partial_stream_keeps_known_trade() {
    // Actual EEA ticker TEST002-USD_UM_XPERP-310822 publishes last/open24h="".
    let mut e = Engine::new(Okx::new(Options {
        eea: true,
        ..Options::default()
    }));
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(
        &mut e,
        &request,
        &envelope(&XPERP.replace("BTC-USD-UM", "TEST002-USD_UM_XPERP-310822")),
        1.5,
    );
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(TICKER), 2.0);
    let request = actions(&mut e).remove(0);
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
    let initial = actions(&mut e);
    let public = socket(&initial, "/public");
    e.handle(3.0, Event::WsOpened { socket: public });
    actions(&mut e);
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
fn nonempty_malformed_native_last_still_rejects_the_entire_ticker_snapshot() {
    for last in ["garbage", "NaN", "inf"] {
        let mut e = Engine::new(Okx::new(Options::default()));
        e.start(0.0);
        let request = actions(&mut e).remove(0);
        reply(&mut e, &request, &envelope(MARKET), 1.0);
        let request = actions(&mut e).remove(0);
        reply(
            &mut e,
            &request,
            &envelope(&TICKER.replace("\"last\":\"40000\"", &format!("\"last\":\"{last}\""))),
            2.0,
        );
        assert!(!e.store().loaded().tickers);
        assert!(e.store().tickers().is_empty());
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
    let pending = actions(&mut e);
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
    let mut reads = actions(&mut e);
    while let Some(request) = reads.pop() {
        if matches!(&request, Action::Http(_)) {
            reply(&mut e, &request, &envelope(""), 6.5);
            reads.extend(actions(&mut e));
        }
    }
    assert_eq!(
        e.store().balance().free,
        8960.0,
        "unproven REST leverage reserves the full440USD instead of zero"
    );
    e.handle(7.0, Event::WsOpened { socket: private });
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
    let pending = actions(&mut e);
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
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
    let pending = actions(&mut e);
    let private = socket(&pending, "/private");
    e.handle(3.5, Event::WsOpened { socket: private });
    actions(&mut e);
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    actions(&mut e);
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
    let future = actions(&mut e)
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
            actions(&mut e)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
    let expected: Vec<_> = ["SWAP", "FUTURES"]
        .into_iter()
        .flat_map(|product| {
            std::iter::once(format!("/api/v5/trade/orders-pending?instType={product}")).chain(
                ["oco", "conditional", "trigger", "move_order_stop"].map(move |kind| {
                    format!("/api/v5/trade/orders-algo-pending?instType={product}&ordType={kind}")
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
            actions(&mut e)
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
    let future = actions(&mut e)
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
        &format!(r#"{{"arg":{{"channel":"positions"}},"data":[{pushed_swap},{pushed_future}]}}"#),
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
        let future = actions(&mut e)
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
                actions(&mut e)
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
            actions(&mut e)
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
            actions(&mut e)
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
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(2000.0, Event::TimerFired { timer });
    let socket = socket(&actions(&mut e), "/private");
    e.handle(2001.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(&mut e, socket, r#"{"event":"login","code":"0"}"#);
    let mut fresh: Vec<_> = actions(&mut e)
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
            actions(&mut e)
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
        !actions(&mut e).iter().any(|a| matches!(a, Action::Http(_))),
        "retired FUTURES completion must not advance old order kinds"
    );
}
