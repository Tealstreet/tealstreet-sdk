mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Response, SocketId};
use support::*;
fn row(symbol: &str, id: &str, qty: &str, side: &str, mode: &str) -> String {
    format!(
        r#"{{"symbol":"{symbol}","positionId":"{id}","qty":"{qty}","side":"{side}","avgOpenPrice":"100","entryValue":"100","leverage":20,"marginMode":"ISOLATION","positionMode":"{mode}","realizedPNL":"-3.5"}}"#
    )
}
fn body(rows: &[String]) -> String {
    format!(
        r#"{{"code":0,"data":{{"positionList":[{}]}}}}"#,
        rows.join(",")
    )
}
fn request(e: &mut TestEngine, socket: SocketId, now: f64) -> HttpRequest {
    e.handle(
        now,
        Event::WsMessage {
            socket,
            text: r#"{"ch":"position","data":{}}"#,
        },
    );
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    requests(e)
        .into_iter()
        .find(|r| r.url.contains("get_pending_positions"))
        .unwrap()
}
fn answer(e: &mut TestEngine, r: safe_cex_engine::RequestId, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn seeded() -> (TestEngine, SocketId) {
    let (mut e, socket) = startup();
    let template = e.store().markets()[0].clone();
    let mut markets = e.store().markets().to_vec();
    for base in ["ETH", "XRP"] {
        let mut m = template.clone();
        m.symbol = format!("{base}USDT");
        m.base = base.into();
        m.id = format!("{base}/USDT");
        markets.push(m);
    }
    e.store_mut().set_markets(markets);
    let r = request(&mut e, socket, 1000.0);
    answer(
        &mut e,
        r.id,
        2100.0,
        &body(&[
            row("BTCUSDT", "btc-route", "1", "SELL", "HEDGE"),
            row("BTCUSDT", "btc-long-route", "2", "BUY", "HEDGE"),
            row("ETHUSDT", "eth-route", "3", "SELL", "HEDGE"),
            row("XRPUSDT", "xrp-route", "4", "SELL", "HEDGE"),
        ]),
    );
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().positions().len(), 4);
    (e, socket)
}
#[test]
fn identified_bad_symbol_keeps_both_hedge_legs_while_peers_and_absence_progress() {
    for alias in ["symbol", "instId", "contract", "instrument"] {
        let (mut e, socket) = seeded();
        let r = request(&mut e, socket, 3000.0);
        let bad = row("BTCUSDT", "bad-route", "invalid", "SELL", "HEDGE")
            .replace("\"qty\":\"invalid\"", "\"qty\":{}")
            .replace("\"symbol\"", &format!("\"{alias}\""));
        answer(
            &mut e,
            r.id,
            4100.0,
            &body(&[bad, row("ETHUSDT", "eth-new", "5", "SELL", "HEDGE")]),
        );
        assert_eq!(
            e.store().positions().len(),
            3,
            "{alias}: safely absent XRP removed"
        );
        assert_eq!(
            e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == "ETHUSDT")
                .unwrap()
                .contracts,
            5.0
        );
        let btc: Vec<_> = e
            .store()
            .positions()
            .iter()
            .filter(|p| p.symbol == "BTCUSDT")
            .collect();
        assert_eq!(btc.len(), 2);
        assert!(btc.iter().all(|p| p.leverage == 20.0));
        assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
        assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code == safe_cex_engine::log::code::ROW_SKIPPED)));
    }
}
#[test]
fn unidentified_bad_neighbor_holds_absence_but_cannot_resurrect_a_valid_zero_position() {
    let (mut e, socket) = seeded();
    let r = request(&mut e, socket, 3000.0);
    answer(
        &mut e,
        r.id,
        4100.0,
        &body(&[
            "{}".into(),
            row("BTCUSDT", "", "0", "SELL", "HEDGE"),
            row("ETHUSDT", "eth-new", "5", "SELL", "HEDGE"),
        ]),
    );
    assert_eq!(e.store().positions().len(), 3);
    assert!(!e.store().positions().iter().any(|p| p.symbol == "BTCUSDT" && p.side == safe_cex_engine::model::PositionSide::Short));
    assert!(
        e.store()
            .positions()
            .iter()
            .any(|p| p.symbol == "BTCUSDT" && p.side == safe_cex_engine::model::PositionSide::Long)
    );
    assert!(e.store().positions().iter().any(|p| p.symbol == "XRPUSDT"));
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == "ETHUSDT")
            .unwrap()
            .contracts,
        5.0
    );
    e.drain_actions().for_each(drop);
    let r = request(&mut e, socket, 5000.0);
    answer(&mut e, r.id, 6100.0, &body(&[]));
    assert!(e.store().positions().is_empty());
}
#[test]
fn known_rejection_quarantines_valid_duplicates_in_either_input_order() {
    for bad_first in [false, true] {
        let (mut e, socket) = seeded();
        let r = request(&mut e, socket, 3000.0);
        let bad = row("BTCUSDT", "bad", "invalid", "SELL", "HEDGE")
            .replace("\"qty\":\"invalid\"", "\"qty\":{}");
        let good = row("BTCUSDT", "should-not-win", "99", "SELL", "HEDGE");
        let mut rows = if bad_first {
            vec![bad, good]
        } else {
            vec![good, bad]
        };
        rows.push(row("ETHUSDT", "eth-new", "5", "SELL", "HEDGE"));
        answer(&mut e, r.id, 4100.0, &body(&rows));
        assert_eq!(
            e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == "BTCUSDT"
                    && p.side == safe_cex_engine::model::PositionSide::Short)
                .unwrap()
                .contracts,
            1.0
        );
        assert_eq!(
            e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == "ETHUSDT")
                .unwrap()
                .contracts,
            5.0
        );
    }
}

#[test]
fn retained_position_ids_and_settings_still_route_reductions_to_original_native_identity() {
    use safe_cex_engine::Command;
    let (mut e, socket) = seeded();
    let r = request(&mut e, socket, 3000.0);
    let bad = row("BTCUSDT", "bad-route", "invalid", "SELL", "HEDGE")
        .replace("\"qty\":\"invalid\"", "\"qty\":{}");
    answer(
        &mut e,
        r.id,
        4100.0,
        &body(&[bad, row("ETHUSDT", "eth-new", "5", "SELL", "HEDGE")]),
    );
    e.drain_actions().for_each(drop);
    let order = serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90000,"amount":0.1,"reduceOnly":true}"#).unwrap();
    e.handle(
        4200.0,
        Event::Command(Command::PlaceOrders {
            command: 81,
            orders: vec![order],
        }),
    );
    let write = requests(&mut e)
        .into_iter()
        .find(|r| r.method == safe_cex_engine::Method::Post)
        .unwrap();
    assert!(write.body.unwrap().contains(r#""positionId":"btc-route""#));
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}

#[test]
fn partial_mode_transition_cannot_mix_retained_exposure_with_new_or_zero_modes() {
    for qty in ["0", "9"] {
        let (mut e, socket) = seeded();
        let previous = e.store().positions().to_vec();
        let r = request(&mut e, socket, 3000.0);
        let bad = row("BTCUSDT", "bad", "invalid", "SELL", "HEDGE")
            .replace("\"qty\":\"invalid\"", "\"qty\":{}");
        answer(
            &mut e,
            r.id,
            4100.0,
            &body(&[bad, row("ETHUSDT", "eth-new", qty, "SELL", "ONE_WAY")]),
        );
        assert_eq!(e.store().positions(), previous.as_slice());
        assert_eq!(e.store().setting("ETHUSDT").unwrap().leverage, 20.0);
    }
}

#[test]
fn rejected_symbol_coverage_overflow_keeps_every_cached_position_conservatively() {
    let (mut e, socket) = seeded();
    let previous = e.store().positions().to_vec();
    let r = request(&mut e, socket, 3000.0);
    let mut rows: Vec<_> = (0..4097)
        .map(|i| format!(r#"{{"symbol":"BAD{i}USDT","qty":{{}}}}"#))
        .collect();
    rows.push(row("ETHUSDT", "eth-new", "99", "SELL", "HEDGE"));
    answer(&mut e, r.id, 4100.0, &body(&rows));
    assert_eq!(e.store().positions(), previous.as_slice());
}

#[test]
fn partial_position_read_updates_ui_peers_but_cannot_prove_a_trading_prerequisite() {
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, _) = seeded();
    let order = serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
    e.handle(
        3000.0,
        Event::Command(Command::PlaceOrders {
            command: 82,
            orders: vec![order],
        }),
    );
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("get_pending_positions"))
        .unwrap();
    answer(
        &mut e,
        read.id,
        4100.0,
        &body(&[
            row("BTCUSDT", "btc-new", "9", "SELL", "HEDGE"),
            r#"{"symbol":"ETHUSDT","qty":{}}"#.into(),
        ]),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 82, .. })))
    );
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.symbol == "BTCUSDT")
            .unwrap()
            .contracts,
        9.0
    );
}

#[test]
fn partial_initial_positions_do_not_claim_readiness_and_clean_retry_can_complete() {
    use safe_cex_venue_bitunix::{BitunixFutures, Options};
    let mut e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    }));
    e.start(0.0);
    let mut position = None;
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::WsConnect { socket, .. } => e.handle(0.0, Event::WsOpened { socket }),
                Action::Http(r) if r.url.contains("get_pending_positions") => {
                    position = Some(r.id);
                }
                Action::Http(r) => {
                    let value = if r.url.ends_with("/trading_pairs") {
                        PAIRS
                    } else if r.url.ends_with("/tickers") {
                        TICKERS
                    } else if r.url.contains("/tpsl/") {
                        TPSL
                    } else if r.url.contains("get_pending_orders") {
                        ORDERS
                    } else {
                        ACCOUNT
                    };
                    answer(&mut e, r.id, 0.0, value);
                }
                _ => {}
            }
        }
    }
    assert!(!e.store().loaded().positions);
    answer(
        &mut e,
        position.unwrap(),
        100.0,
        &body(&[row("BTCUSDT", "btc-new", "9", "SELL", "HEDGE"), "{}".into()]),
    );
    assert!(!e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let r = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("get_pending_positions"))
        .unwrap();
    answer(&mut e, r.id, at + 1.0, &body(&[]));
    assert!(e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
}
