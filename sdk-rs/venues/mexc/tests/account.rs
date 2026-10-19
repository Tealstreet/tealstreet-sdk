mod support;
use safe_cex_engine::{Action, Command, Event};
use support::*;

const POSITION: &str = r#"{"positionId":739113577038255616,"symbol":"BTC_USDT","positionType":1,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1,"positionMode":2,"unrealized":6,"liquidatePrice":50000,"im":36,"updateTime":1700000000000}"#;
const ORDER: &str = r#"{"orderId":739113577038255617,"symbol":"BTC_USDT","side":4,"orderType":1,"vol":30,"dealVol":10,"price":65000,"state":2,"updateTime":1700000000000}"#;

#[test]
fn startup_signs_reads_and_login_filters_all_order_surfaces() {
    let (mut e, socket, reads, _) = private_started();
    assert_eq!(reads.len(), 5);
    assert!(!e.store().hedged());
    for r in &reads {
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "ApiKey" && v == "replay-key")
        );
    }
    let sent: Vec<_> = e.drain_actions().collect();
    assert!(sent.iter().any(
        |a| matches!(a,Action::WsSend{text,..}if text.contains("signature")&&text.contains("login"))
    ));
    ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
    assert!(e.drain_actions().any(|a|matches!(a,Action::WsSend{text,..}if text.contains("stop.planorder")&&text.contains("order.deal"))));
    for r in &reads {
        let body = if r.url.contains("assets") {
            r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":964,"equity":1000,"unrealized":6,"frozenBalance":0}]}"#.to_owned()
        } else if r.url.contains("open_positions") {
            format!(r#"{{"success":true,"code":0,"data":[{POSITION}]}}"#)
        } else if r.url.contains("open_orders") {
            format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#)
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.to_owned()
        };
        response(&mut e, r, &body);
    }
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(e.store().positions()[0].id, "BTCUSDT");
    assert_eq!(e.store().positions()[0].contracts, 0.006);
    assert_eq!(
        e.store().orders()[0]
            .extra
            .as_ref()
            .unwrap()
            .filled_in_placement_units,
        Some(10.0)
    );
    assert!(e.store().loaded().orders);
}

#[test]
fn terminal_stream_update_wins_over_older_full_snapshot() {
    let (mut e, socket, reads, _) = private_started();
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"channel":"push.personal.order","data":{}}}"#,
            ORDER.replace("\"state\":2", "\"state\":4")
        ),
    );
    for r in &reads {
        let body = if r.url.contains("open_orders") {
            format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#)
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.into()
        };
        response(&mut e, r, &body);
    }
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.id == "739113577038255617"
                && o.status == safe_cex_engine::model::OrderStatus::Open)
    );
}

#[test]
fn failed_full_snapshot_never_commits_successful_subset_as_complete() {
    let (mut e, _, reads, timer) = private_started();
    for r in &reads {
        let body = if r.url.contains("planorder") {
            r#"{"success":false,"code":510,"data":[]}"#
        } else {
            r#"{"success":true,"code":0,"data":[]}"#
        };
        response(&mut e, r, body);
    }
    assert!(!e.store().loaded().orders);
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(requests(&mut e).len(), 5);
}

#[test]
fn fills_emit_once_and_never_increment_cumulative_order_fills() {
    let (mut e, socket, _, _) = private_started();
    ws(
        &mut e,
        socket,
        &format!(r#"{{"channel":"push.personal.order","data":{ORDER}}}"#),
    );
    let deal = r#"{"channel":"push.personal.order.deal","data":{"tradeId":987654321123456789,"orderId":739113577038255617,"symbol":"BTC_USDT","side":4,"price":65000,"vol":10}}"#;
    ws(&mut e, socket, deal);
    ws(&mut e, socket, deal);
    assert_eq!(e.store().order("739113577038255617").unwrap().filled, 0.001);
    let frame = e.take_frame(200.0).unwrap();
    let sections = u16::from_le_bytes(frame[6..8].try_into().unwrap());
    let mut cursor = 16;
    let mut fill_count = 0;
    for _ in 0..sections {
        let kind = u16::from_le_bytes(frame[cursor..cursor + 2].try_into().unwrap());
        let bytes = u32::from_le_bytes(frame[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            fill_count = u32::from_le_bytes(frame[cursor + 8..cursor + 12].try_into().unwrap());
        }
        cursor += 8 + bytes;
    }
    assert_eq!(fill_count, 1);
    assert!(!e.has_actions() || e.drain_actions().all(|a| !matches!(a, Action::Http(_))));
}

#[test]
fn trigger_state_two_is_canceled_and_stale_open_frame_is_shielded() {
    let (mut e, socket, _, _) = private_started();
    let plan = r#"{"channel":"push.personal.plan.order","data":{"id":"p1","symbol":"BTC_USDT","side":4,"vol":10,"triggerPrice":55000,"triggerType":2,"state":2}}"#;
    ws(&mut e, socket, plan);
    ws(&mut e, socket, &plan.replace("\"state\":2", "\"state\":1"));
    assert_eq!(
        e.store().order("p1").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}

#[test]
fn zero_equity_is_a_value_and_does_not_fall_back_to_cash_balance() {
    let (mut e, _, reads, _) = private_started();
    let r = reads.iter().find(|r| r.url.contains("assets")).unwrap();
    response(
        &mut e,
        r,
        r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":0,"equity":0,"cashBalance":100}]}"#,
    );
    assert_eq!(e.store().balance().total, 0.0);
}

#[test]
fn private_429_pauses_periodic_reads() {
    let (mut e, _, reads, timer) = private_started();
    for r in &reads {
        e.handle(
            100.0,
            Event::HttpResponse {
                request: r.id,
                response: safe_cex_engine::Response {
                    status: 429,
                    headers: "retry-after: 10",
                    body: r#"{"success":false,"code":429,"message":"limited"}"#,
                },
            },
        );
    }
    e.drain_actions().for_each(drop);
    e.handle(2100.0, Event::TimerFired { timer });
    assert!(requests(&mut e).is_empty());
}

#[test]
fn flat_symbol_metadata_fetches_both_rails_and_authenticated_cap() {
    let (mut e, _, _, _) = private_started();
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(safe_cex_engine::Command::FetchLeverage {
            command: 41,
            symbol: "BTCUSDT".into(),
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("/position/leverage?symbol=BTC_USDT"));
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":[{"positionType":1,"leverage":10,"openType":1,"maxLeverageView":125,"maxLeverage":500},{"positionType":2,"leverage":15,"openType":1,"maxLeverageView":100,"maxLeverage":500}]}"#,
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(10.0)
    );
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().sell_leverage,
        Some(15.0)
    );
    assert_eq!(
        e.store().market("BTCUSDT").unwrap().limits.leverage.max,
        100.0
    );
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::CommandDone{command:41,result:Ok(s)}) if s.contains("\"sellLeverage\":15.0")&&s.contains("\"buyLeverage\":10.0"))));
}

#[test]
fn unrelated_order_update_does_not_skip_the_second_hedge_rail() {
    let (mut e, socket, reads, _) = private_started();
    ws(
        &mut e,
        socket,
        &format!(r#"{{"channel":"push.personal.order","data":{ORDER}}}"#),
    );
    let long = POSITION.replace("\"positionMode\":2", "\"positionMode\":1");
    let short = long
        .replace("739113577038255616", "739113577038255618")
        .replace("\"positionType\":1", "\"positionType\":2")
        .replace("\"leverage\":10", "\"leverage\":15");
    let r = reads
        .iter()
        .find(|r| r.url.contains("open_positions"))
        .unwrap();
    response(
        &mut e,
        r,
        &format!(r#"{{"success":true,"code":0,"data":[{long},{short}]}}"#),
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 15.0);
}

#[test]
fn startup_mode_and_metadata_reads_respect_the_rate_limit_pause() {
    let mut e = safe_cex_engine::Engine::new(safe_cex_venue_mexc::MexcFutures::new(
        safe_cex_venue_mexc::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            public_data_only: false,
        },
    ));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, MARKETS);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, TICKERS);
    let r = requests(&mut e).remove(0);
    e.handle(
        100.0,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 429,
                headers: "retry-after: 10",
                body: "limited",
            },
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2100.0, Event::TimerFired { timer });
    assert!(requests(&mut e).is_empty());
    e.handle(
        2200.0,
        Event::Command(safe_cex_engine::Command::FetchLeverage {
            command: 41,
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(e.drain_actions().all(|a| !matches!(a, Action::Http(_))));
}

#[test]
fn metadata_read_cannot_be_overwritten_by_older_mutation_ack() {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
        }),
    );
    let mutation = requests(&mut e).remove(0);
    e.handle(
        210.0,
        Event::Command(Command::FetchLeverage {
            command: 2,
            symbol: "BTCUSDT".into(),
        }),
    );
    let read = requests(&mut e).remove(0);
    response(
        &mut e,
        &read,
        r#"{"success":true,"code":0,"data":[{"positionType":1,"leverage":40,"openType":2},{"positionType":2,"leverage":45,"openType":2}]}"#,
    );
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(40.0)
    );
    response(&mut e, &mutation, r#"{"success":true,"code":0}"#);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(40.0)
    );
}

#[test]
fn later_regular_and_full_rest_snapshots_cannot_regress_cumulative_fills() {
    for full in [false, true] {
        let (mut e, socket, reads, timer) = private_started();
        for r in reads {
            response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
        }
        ws(
            &mut e,
            socket,
            &format!(r#"{{"channel":"push.personal.order","data":{ORDER}}}"#),
        );
        e.drain_actions().for_each(drop);
        if full {
            ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
        } else {
            e.handle(2100.0, Event::TimerFired { timer });
        }
        for r in requests(&mut e) {
            let body = if r.url.contains("open_orders") {
                format!(
                    r#"{{"success":true,"code":0,"data":[{}]}}"#,
                    ORDER.replace("\"dealVol\":10", "\"dealVol\":0")
                )
            } else {
                r#"{"success":true,"code":0,"data":[]}"#.into()
            };
            response(&mut e, &r, &body);
        }
        let order = e.store().order("739113577038255617").unwrap();
        assert_eq!(order.filled, 0.001);
        assert_eq!(
            order.extra.as_ref().unwrap().filled_in_placement_units,
            Some(10.0)
        );
        assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(20.0));
    }
}

fn scoped_ready() -> (TestEngine, safe_cex_engine::SocketId) {
    let mut markets: serde_json::Value = serde_json::from_str(MARKETS).unwrap();
    let mut eth = markets["data"][0].clone();
    eth["symbol"] = "ETH_USDT".into();
    eth["baseCoin"] = "ETH".into();
    markets["data"].as_array_mut().unwrap().push(eth);
    let (mut e, socket, reads, _) = private_started_with_markets(&markets.to_string());
    for r in reads {
        let body = if r.url.contains("open_orders") {
            format!(
                r#"{{"success":true,"code":0,"data":[{ORDER},{}]}}"#,
                ORDER
                    .replace("BTC_USDT", "ETH_USDT")
                    .replace("739113577038255617", "739113577038255618")
            )
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.into()
        };
        response(&mut e, &r, &body);
    }
    e.drain_actions().for_each(drop);
    (e, socket)
}
fn place_scoped_plan(e: &mut TestEngine, symbol: &str, now: f64) -> safe_cex_engine::TimerId {
    let opts = serde_json::from_value(serde_json::json!({
        "symbol": symbol, "type": "stop_market", "side": "sell", "amount": 10, "price": 55000,
    }))
    .unwrap();
    e.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command: now as u32,
            orders: vec![opts],
        }),
    );
    let request = requests(e).remove(0);
    e.handle(
        now + 10.0,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: &format!(r#"{{"success":true,"code":0,"data":"placed-{symbol}-{now}"}}"#),
            },
        },
    );
    e.drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == now + 1510.0 => Some(timer),
            _ => None,
        })
        .unwrap()
}

#[test]
fn scoped_stop_reconciliation_preserves_other_symbols_and_newer_stream_updates() {
    let (mut e, socket) = scoped_ready();
    let timer = place_scoped_plan(&mut e, "BTCUSDT", 300.0);
    e.handle(1810.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 3);
    assert!(reads.iter().any(|r| {
        r.url
            .contains("/open_orders/BTC_USDT?page_num=1&page_size=100")
    }));
    for r in reads.iter().filter(|r| !r.url.contains("open_orders")) {
        assert!(r.url.contains("symbol=BTC_USDT"));
        assert!(r.url.contains("start_time=-7689598190"));
    }
    let text = format!(
        r#"{{"channel":"push.personal.order","data":{}}}"#,
        ORDER.replace("\"state\":2", "\"state\":4")
    );
    e.handle(
        1820.0,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    for r in reads {
        let body = if r.url.contains("open_orders") {
            format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#)
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.into()
        };
        response(&mut e, &r, &body);
    }
    assert!(e.store().order("739113577038255618").is_some());
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.id == "739113577038255617"
                && o.status == safe_cex_engine::model::OrderStatus::Open)
    );
    assert!(e.store().loaded().orders);
}

#[test]
fn failed_scoped_stop_snapshot_never_commits_successful_subset() {
    let (mut e, _) = scoped_ready();
    let timer = place_scoped_plan(&mut e, "BTCUSDT", 300.0);
    e.handle(1810.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 3);
    for r in reads {
        response(
            &mut e,
            &r,
            if r.url.contains("planorder") {
                r#"{"success":false,"code":510,"data":[]}"#
            } else {
                r#"{"success":true,"code":0,"data":[]}"#
            },
        );
    }
    assert!(e.store().order("739113577038255617").is_some());
    assert!(e.store().order("739113577038255618").is_some());
    assert!(
        e.store()
            .orders()
            .iter()
            .any(|order| order.id.starts_with("placed-"))
    );
}

#[test]
fn scoped_refreshes_debounce_per_symbol_and_wait_for_other_order_batches() {
    let (mut e, _) = scoped_ready();
    let btc_timer = place_scoped_plan(&mut e, "BTCUSDT", 300.0);
    let eth_timer = place_scoped_plan(&mut e, "ETHUSDT", 500.0);
    e.handle(1810.0, Event::TimerFired { timer: btc_timer });
    let btc_reads = requests(&mut e);
    assert_eq!(btc_reads.len(), 3);
    e.handle(2010.0, Event::TimerFired { timer: eth_timer });
    assert!(requests(&mut e).is_empty());
    for r in btc_reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    let eth_reads = requests(&mut e);
    assert_eq!(eth_reads.len(), 3);
    assert!(eth_reads.iter().all(|r| r.url.contains("ETH_USDT")));
    for r in eth_reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    // Scoped reconciliation cannot retire an existing regular row merely
    // because the venue's immediate post-placement snapshot is lagging.
    assert!(e.store().order("739113577038255617").is_some());
    assert!(e.store().order("739113577038255618").is_some());
    assert!(e.store().loaded().orders);
}

#[test]
fn stream_order_before_placement_ack_still_schedules_scoped_reconciliation() {
    let (mut e, socket) = scoped_ready();
    let opts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":10,"price":55000}"#,
    )
    .unwrap();
    e.handle(
        300.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let request = requests(&mut e).remove(0);
    e.handle(305.0, Event::WsMessage { socket, text: r#"{"channel":"push.personal.plan.order","data":{"id":"stream-first","symbol":"BTC_USDT","side":4,"vol":10,"triggerPrice":55000,"triggerType":2,"state":1}}"# });
    e.handle(
        310.0,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"code":0,"data":"stream-first"}"#,
            },
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer {
                timer,
                at_ms: 1810.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1810.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 3);
    assert!(reads.iter().all(|r| r.url.contains("BTC_USDT")));
}
