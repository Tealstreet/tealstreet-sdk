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
        .find(|a| is_get(a, "/api/v5/account/positions"))
        .unwrap();
    let risk = initial
        .iter()
        .find(|a| is_get(a, "/api/v5/account/account-position-risk"))
        .unwrap();
    assert!(
        initial
            .iter()
            .any(|a| is_get(a, "/api/v5/trade/orders-pending"))
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
