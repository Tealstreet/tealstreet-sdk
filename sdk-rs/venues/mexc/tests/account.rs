mod support;
use safe_cex_engine::error::ErrorKind;
use safe_cex_engine::{Action, Command, Engine, Event, LogLevel, Notice};
use safe_cex_venue_mexc::{MexcFutures, Options};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

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

fn currency_totals(e: &TestEngine) -> Vec<(String, f64, f64, f64, f64)> {
    e.store()
        .currency_balances()
        .unwrap()
        .iter()
        .map(|c| {
            let b = &c.balance;
            (c.currency.clone(), b.total, b.free, b.used, b.upnl)
        })
        .collect()
}

#[test]
fn asset_snapshot_and_asset_push_publish_every_currency_balance() {
    let (mut e, socket, reads, _) = private_started();
    let r = reads.iter().find(|r| r.url.contains("assets")).unwrap();
    response(
        &mut e,
        r,
        r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":964,"equity":1000,"unrealized":6,"frozenBalance":0},{"currency":"BTC","availableBalance":0.5,"cashBalance":0.75,"unrealized":0,"frozenBalance":0.3},{"currency":"","availableBalance":1,"equity":1}]}"#,
    );
    assert_eq!(e.store().balance().total, 1000.0);
    assert_eq!(
        currency_totals(&e),
        vec![
            ("BTC".into(), 0.75, 0.5, 0.3, 0.0),
            ("USDT".into(), 1000.0, 964.0, 36.0, 6.0),
        ]
    );

    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.asset","data":{"currency":"USDT","availableBalance":900}}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1200.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1200.0, Event::TimerFired { timer });
    let r = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("assets"))
        .unwrap();
    response(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":900,"equity":950,"unrealized":-4,"frozenBalance":0},{"currency":"USDC","availableBalance":20,"equity":20}]}"#,
    );
    assert_eq!(e.store().balance().total, 950.0);
    assert_eq!(
        currency_totals(&e),
        vec![
            ("USDC".into(), 20.0, 20.0, 0.0, 0.0),
            ("USDT".into(), 950.0, 900.0, 50.0, -4.0),
        ]
    );
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
            recv_window: None,
            read_credentials: None,
            key: Some("key".into()),
            secret: Some("secret".into()),
            public_data_only: false,
        },
    ));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, MARKETS);
    let early = requests(&mut e);
    let ticker = early
        .iter()
        .find(|r| r.url.contains("contract/ticker"))
        .unwrap();
    let mode = early
        .iter()
        .find(|r| r.url.contains("position_mode"))
        .unwrap()
        .clone();
    response(&mut e, ticker, TICKERS);
    let r = mode;
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
            side: None,
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

#[test]
fn returned_regular_rows_keep_terminal_stream_state_through_regular_and_full_snapshots() {
    use safe_cex_engine::model::OrderStatus;
    for full in [false, true] {
        for (state, status) in [(4, OrderStatus::Canceled), (3, OrderStatus::Closed)] {
            for returned in [0, 10, 20] {
                let (mut e, socket, reads, timer) = private_started();
                for r in reads {
                    response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
                }
                e.drain_actions().for_each(drop);
                // Start the read before the authoritative terminal stream update.
                if full {
                    ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
                } else {
                    e.handle(2100.0, Event::TimerFired { timer });
                }
                let reads = requests(&mut e);
                let text = format!(
                    r#"{{"channel":"push.personal.order","data":{}}}"#,
                    ORDER.replace("\"state\":2", &format!("\"state\":{state}"))
                );
                ws(&mut e, socket, &text);
                for r in reads {
                    let body = if r.url.contains("open_orders") {
                        format!(
                            r#"{{"success":true,"code":0,"data":[{}]}}"#,
                            ORDER.replace("\"dealVol\":10", &format!("\"dealVol\":{returned}"))
                        )
                    } else {
                        r#"{"success":true,"code":0,"data":[]}"#.into()
                    };
                    response(&mut e, &r, &body);
                }
                let order = e.store().order("739113577038255617").unwrap();
                assert_eq!(order.status, status);
                assert_eq!(order.filled, f64::from(returned.max(10)) * 0.0001);
                assert_eq!(
                    order.extra.as_ref().unwrap().filled_in_placement_units,
                    Some(f64::from(returned.max(10)))
                );
            }
        }
    }
}

#[test]
fn absent_terminal_regular_rows_are_removed_and_tombstones_cannot_be_restored_by_lagging_rest() {
    let (mut e, socket, reads, timer) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    let text = format!(
        r#"{{"channel":"push.personal.order","data":{}}}"#,
        ORDER.replace("\"state\":2", "\"state\":4")
    );
    ws(&mut e, socket, &text);
    e.drain_actions().for_each(drop);
    e.handle(2100.0, Event::TimerFired { timer });
    for r in requests(&mut e) {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    assert!(e.store().order("739113577038255617").is_none());
    ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
    for r in requests(&mut e) {
        let body = if r.url.contains("open_orders") {
            format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#)
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.into()
        };
        response(&mut e, &r, &body);
    }
    assert!(e.store().order("739113577038255617").is_none());
}

// BTC_USDT is valid, HIDE_USDT deliberately hidden, BAD_USDT malformed (zero contract size).
fn catalogue_with(bad_contract_size: &str) -> String {
    let contract = |symbol: &str, extra: &str| {
        format!(
            r#"{{"symbol":"{symbol}","baseCoin":"X","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true{extra},"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500}}"#
        )
    };
    format!(
        r#"{{"success":true,"code":0,"data":[{},{},{}]}}"#,
        contract("BTC_USDT", r#","contractSize":0.0001"#),
        contract("HIDE_USDT", r#","isHidden":true,"contractSize":1"#),
        contract(
            "BAD_USDT",
            &format!(r#","contractSize":{bad_contract_size}"#)
        ),
    )
}

fn answer_snapshot(e: &mut TestEngine, reads: Vec<safe_cex_engine::HttpRequest>, positions: &str) {
    for request in reads {
        let body = if request.url.contains("assets") {
            r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":964,"equity":1000,"unrealized":6,"frozenBalance":0}]}"#.to_owned()
        } else if request.url.contains("open_positions") {
            format!(r#"{{"success":true,"code":0,"data":[{positions}]}}"#)
        } else if request.url.contains("open_orders") {
            format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#)
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.to_owned()
        };
        response(e, &request, &body);
    }
}

#[test]
fn owned_position_on_filtered_market_is_skipped_and_the_snapshot_commits() {
    let (mut e, _, reads, _) = private_started_with_markets(&catalogue_with("0"));
    let hidden = POSITION
        .replace("BTC_USDT", "HIDE_USDT")
        .replace("739113577038255616", "1");
    answer_snapshot(&mut e, reads, &format!("{POSITION},{hidden}"));
    let loaded = e.store().loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
    assert_eq!(e.store().balance().total, 1000.0);
    let positions = e.store().positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert!(e.store().order("739113577038255617").is_some());
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::Notice(Notice::Log { message, .. }) | Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("uncatalogued market HIDEUSDT"))));
}

#[test]
fn owned_position_on_malformed_market_preserves_the_previous_snapshot() {
    let (mut e, socket, reads, _) = private_started_with_markets(&catalogue_with("0"));
    answer_snapshot(&mut e, reads, POSITION);
    assert!(e.store().loaded().positions);
    assert_eq!(e.store().positions()[0].contracts, 0.006);
    ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
    let reads = requests(&mut e);
    let bad = POSITION
        .replace("BTC_USDT", "BAD_USDT")
        .replace("739113577038255616", "1");
    let changed = POSITION.replace("\"holdVol\":60", "\"holdVol\":90");
    answer_snapshot(&mut e, reads, &format!("{changed},{bad}"));
    let positions = e.store().positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].contracts, 0.006, "previous snapshot kept");
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::Notice(Notice::Log { message, .. }) | Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("owned position market unavailable"))));
}

#[test]
fn partial_catalogue_commits_then_rereads_without_a_second_account_start() {
    let mut e = Engine::new(MexcFutures::new(Options {
        read_credentials: None,
        public_data_only: false,
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        recv_window: None,
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, &catalogue_with("0"));
    assert!(e.store().market("BTCUSDT").is_some());
    assert!(e.store().market("BADUSDT").is_none());
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("position_mode")))
    );
    let retry = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1100.0 => Some(*timer),
            _ => None,
        })
        .expect("partial catalogue retry");
    e.handle(1100.0, Event::TimerFired { timer: retry });
    let reread = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("contract/detail"))
        .expect("catalogue re-read");
    response(&mut e, &reread, &catalogue_with("1"));
    assert!(
        e.store().market("BADUSDT").is_some(),
        "repaired market loaded"
    );
    assert!(e.store().market("BTCUSDT").is_some());
    let after: Vec<_> = e.drain_actions().collect();
    assert!(
        !after
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
    assert!(
        !after.iter().any(|a| matches!(a, Action::SetTimer { .. })),
        "a complete re-read schedules no further retry"
    );
}

#[test]
fn failed_leverage_read_diagnostic_precedes_completion_without_duplicate_log() {
    use safe_cex_engine::Notice;
    let (mut e, _, _, _) = private_started();
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::FetchLeverage {
            command: 91,
            symbol: "BTCUSDT".into(),
        }),
    );
    let request = requests(&mut e).remove(0);
    e.handle(
        201.0,
        Event::HttpFailed {
            request: request.id,
            error: "failed leverage transport",
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    let error=out.iter().position(|a| matches!(a,Action::Notice(Notice::CommandError{command:91,message}) if message=="failed leverage transport")).unwrap();
    let done=out.iter().position(|a| matches!(a,Action::Notice(Notice::CommandDone{command:91,result:Err(message)}) if message=="failed leverage transport")).unwrap();
    assert!(error < done);
    assert!(!out.iter().any(|a| matches!(a,Action::Notice(Notice::Log{message,..}) | Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message=="failed leverage transport")));
    // Correlation does not initiate another request.
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) => panic!("policy updates must not issue account reads"),
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1_501.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_500.0);
    engine.handle(
        now + 91_500.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    let reads: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_501.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let (mut e, _, initial, _) = private_started();
    for r in initial {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    let engine = &mut e;
    let (now, mut reads) = policy_poll_read(engine, 100.0, "open_positions");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = r#"{"success":true,"code":0,"data":[]}"#;
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        reads = engine
            .drain_actions()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}
#[test]
fn unloaded_market_order_does_not_block_any_order_surface_or_loaded_store() {
    for surface in ["open_orders", "planorder", "stoporder"] {
        let (mut e, _, reads, _) = private_started();
        let valid = ORDER.replace("739113577038255617", "\"valid\"");
        let unknown = valid
            .replace("valid", "unloaded")
            .replace("BTC_USDT", "UNLOADED_USDT");
        let mut warnings = Vec::new();
        for request in reads {
            let body = if request.url.contains(surface) {
                format!(r#"{{"success":true,"code":0,"data":[{valid},{unknown}]}}"#)
            } else {
                r#"{"success":true,"code":0,"data":[]}"#.into()
            };
            response(&mut e, &request, &body);
            warnings.extend(e.drain_actions());
        }
        assert!(e.store().loaded().orders, "{surface}");
        assert!(e.store().order("valid").is_some(), "{surface}");
        assert!(e.store().order("unloaded").is_none());
        assert!(warnings.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Log{message,..}) | Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("unloaded") && message.contains("UNLOADED_USDT"))));
    }
}

#[test]
fn unmappable_order_id_preserves_known_copy_but_empty_full_snapshot_clears_it() {
    let (mut e, socket, reads, _) = private_started();
    for request in reads {
        response(&mut e, &request, r#"{"success":true,"code":0,"data":[]}"#);
    }
    ws(
        &mut e,
        socket,
        &format!(r#"{{"channel":"push.personal.order","data":{ORDER}}}"#),
    );
    e.drain_actions().for_each(drop);
    ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 5);
    for request in reads {
        let body = if request.url.contains("open_orders") {
            format!(
                r#"{{"success":true,"code":0,"data":[{}]}}"#,
                ORDER.replace("BTC_USDT", "UNLOADED_USDT")
            )
        } else {
            r#"{"success":true,"code":0,"data":[]}"#.into()
        };
        response(&mut e, &request, &body);
    }
    assert_eq!(
        e.store().order("739113577038255617").unwrap().symbol,
        "BTCUSDT"
    );
    e.drain_actions().for_each(drop);
    e.handle(300.0, Event::WsClosed { socket, code: 1006 });
    let (timer, at) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("private reconnect timer");
    e.handle(at, Event::TimerFired { timer });
    let replacement = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("replacement private socket");
    assert_ne!(replacement, socket);
    e.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(
        at + 1.0,
        Event::WsMessage {
            socket: replacement,
            text: r#"{"channel":"rs.login","data":"success"}"#,
        },
    );
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 5, "reconnect must request the full snapshot");
    for request in reads {
        e.handle(
            at + 2.0,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: r#"{"success":true,"code":0,"data":[]}"#,
                },
            },
        );
    }
    assert!(e.store().loaded().orders);
    assert!(e.store().orders().is_empty());
}

#[test]
fn missing_order_identity_preserves_known_snapshot_and_cannot_finish_initial_load() {
    for initialized in [false, true] {
        let (mut e, socket, mut reads, _) = private_started();
        if initialized {
            for request in reads {
                response(&mut e, &request, r#"{"success":true,"code":0,"data":[]}"#);
            }
            ws(
                &mut e,
                socket,
                &format!(r#"{{"channel":"push.personal.order","data":{ORDER}}}"#),
            );
            e.drain_actions().for_each(drop);
            ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
            reads = requests(&mut e);
        }
        let missing_id = ORDER.replace("\"orderId\":739113577038255617,", "");
        for request in reads {
            let body = if request.url.contains("open_orders") {
                format!(r#"{{"success":true,"code":0,"data":[{missing_id}]}}"#)
            } else {
                r#"{"success":true,"code":0,"data":[]}"#.into()
            };
            response(&mut e, &request, &body);
        }
        assert_eq!(e.store().loaded().orders, initialized);
        if initialized {
            assert_eq!(e.store().order("739113577038255617").unwrap().amount, 0.003);
        } else {
            assert!(e.store().orders().is_empty());
        }
        assert!(e.drain_actions().any(|action| matches!(action,
            Action::Notice(safe_cex_engine::Notice::Log {message,..}) | Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Malformed MEXC snapshot row"))));
    }
}

fn mode_read(body: &str, status: u16) -> (support::TestEngine, Vec<Action>) {
    let mut e = Engine::new(MexcFutures::new(Options {
        read_credentials: None,
        public_data_only: false,
        recv_window: None,
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
    }));
    e.start(0.0);
    let r = support::requests(&mut e).remove(0);
    support::response(&mut e, &r, support::MARKETS);
    // Private startup no longer waits for tickers, so the mode read may come with them.
    let mut pending = support::requests(&mut e);
    let tickers = pending
        .iter()
        .position(|r| r.url.contains("contract/ticker"))
        .unwrap();
    let tickers = pending.remove(tickers);
    support::response(&mut e, &tickers, support::TICKERS);
    pending.extend(support::requests(&mut e));
    let mode = pending
        .into_iter()
        .find(|r| r.url.contains("position_mode"))
        .unwrap();
    e.handle(
        100.0,
        Event::HttpResponse {
            request: mode.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    );
    let actions = e.drain_actions().collect();
    (e, actions)
}

fn startup_failure(actions: &[Action]) -> Option<&safe_cex_engine::error::VenueError> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::StartupFailed { error }) => Some(error),
        _ => None,
    })
}

/// Fires every timer due by `until`, returning whether any fired timer issued a mode read.
fn mode_retried(e: &mut support::TestEngine, mut actions: Vec<Action>, until: f64) -> bool {
    let mut retried = false;
    while let Some((timer, at)) = actions.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms <= until => Some((*timer, *at_ms)),
        _ => None,
    }) {
        actions.retain(|a| !matches!(a, Action::SetTimer { timer: t, .. } if *t == timer));
        e.handle(at, Event::TimerFired { timer });
        let next: Vec<_> = e.drain_actions().collect();
        retried |= next
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("position_mode")));
        actions.extend(next);
    }
    retried
}

#[test]
fn refused_key_fails_startup_instead_of_retrying_the_mode_read_forever() {
    let (mut e, actions) = mode_read(
        r#"{"success":false,"code":602,"message":"Signature verification failed!"}"#,
        200,
    );
    let error = startup_failure(&actions).expect("startup fails");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.venue_code.as_deref(), Some("602"));
    // The SDK's health classifier matches this wording as a rejected key.
    assert!(error.message.to_lowercase().contains("api key rejected"));
    assert!(error.message.contains("Signature verification failed!"));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    assert!(!mode_retried(&mut e, actions, 30_000.0));
    let (_, actions) = mode_read(
        r#"{"success":false,"code":402,"message":"Api_key expired"}"#,
        401,
    );
    assert_eq!(
        startup_failure(&actions).and_then(|e| e.kind),
        Some(ErrorKind::AuthExpired)
    );
}

#[test]
fn refusal_after_a_transient_failure_stops_the_scheduled_retries() {
    let (mut e, actions) = mode_read(r#"{"success":false,"code":510,"message":"busy"}"#, 200);
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let mode = support::requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("position_mode"))
        .unwrap();
    e.handle(
        at + 1.0,
        Event::HttpResponse {
            request: mode.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"success":false,"code":602,"message":"Signature verification failed!"}"#,
            },
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(startup_failure(&actions).is_some());
    assert!(!mode_retried(&mut e, actions, 60_000.0));
}

#[test]
fn transient_mode_failures_keep_retrying_without_failing_startup() {
    for (body, status) in [
        (
            r#"{"success":false,"code":510,"message":"Requests are too frequent"}"#,
            200,
        ),
        ("", 502),
        ("<html>403 Forbidden</html>", 403),
        ("", 401),
    ] {
        let (mut e, actions) = mode_read(body, status);
        assert!(startup_failure(&actions).is_none(), "{status} {body}");
        assert!(mode_retried(&mut e, actions, 30_000.0), "{status} {body}");
    }
}

#[test]
fn refresh_that_filters_a_held_market_keeps_it_with_its_position_and_order() {
    let (mut e, socket, reads, _, retry) = private_started_with_retry(&catalogue_with("0"));
    answer_snapshot(&mut e, reads, POSITION);
    assert_eq!(e.store().positions().len(), 1);
    let listed = e.store().market("BTCUSDT").cloned().unwrap();
    e.handle(
        1100.0,
        Event::TimerFired {
            timer: retry.expect("partial catalogue retry"),
        },
    );
    let reread = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("contract/detail"))
        .expect("catalogue re-read");
    // BTC_USDT is now paused (state 1), which the catalogue filters out.
    let paused = catalogue_with("0").replacen(r#""state":0"#, r#""state":1"#, 1);
    response(&mut e, &reread, &paused);
    assert_eq!(e.store().market("BTCUSDT"), Some(&listed));
    ws(&mut e, socket, r#"{"channel":"rs.login","data":"success"}"#);
    let reads = requests(&mut e);
    answer_snapshot(&mut e, reads, POSITION);
    let loaded = e.store().loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "BTCUSDT");
    assert!(e.store().order("739113577038255617").is_some());
}

#[test]
fn persistently_malformed_catalogue_stops_rereading_after_five_refreshes() {
    let (mut e, _, _, _, mut retry) = private_started_with_retry(&catalogue_with("0"));
    let (mut rereads, mut warned) = (0, false);
    while let Some(timer) = retry.take() {
        e.handle(1100.0, Event::TimerFired { timer });
        let reread = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("contract/detail"))
            .expect("catalogue re-read");
        response(&mut e, &reread, &catalogue_with("0"));
        rereads += 1;
        for action in e.drain_actions() {
            match action {
                Action::SetTimer { timer, .. } => retry = Some(timer),
                Action::Notice(Notice::Log {
                    level: LogLevel::Warning,
                    message,
                })
                | Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    level: safe_cex_engine::Level::Warn,
                    message,
                    ..
                })) if message.contains("stops re-reading") => warned = true,
                _ => {}
            }
        }
        assert!(rereads <= 5, "re-read is bounded");
    }
    assert_eq!(rereads, 5);
    assert!(warned);
    assert!(e.store().market("BTCUSDT").is_some());
}
