mod support;
use safe_cex_engine::model::TimeInForce;
use safe_cex_engine::{Action, Event};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 10_000.0;

#[test]
fn private_auth_is_delayed_and_subscriptions_wait_for_ack() {
    let mut h = Harness::new();
    let socket = h.socket(true);
    h.event(Event::WsOpened { socket });
    assert!(!h.actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - h.now, 1000.0) => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 1000.0;
    h.event(Event::TimerFired { timer });
    assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("public/auth") && text.contains("sig"))));
    h.event(Event::WsMessage {
        socket,
        text: r#"{"method":"public/auth","code":0}"#,
    });
    assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("user.advance.order") && text.contains("user.trade") && text.contains("user.balance"))));
}
#[test]
fn position_snapshot_cannot_overwrite_newer_stream_exposure() {
    let mut h = Harness::new();
    let old = h.http("get-positions");
    h.ws("user.positions", r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","open_position_pnl":"3","leverage":"10","liquidation_price":"120","update_timestamp_ms":5}"#);
    h.respond(&old, r#"{"code":0,"result":{"data":[{"instrument_name":"BTCUSD-PERP","quantity":"1","cost":"99","update_timestamp_ms":1}]}}"#);
    let p = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|p| p.side == safe_cex_engine::model::PositionSide::Short)
        .unwrap();
    assert_eq!((p.contracts, p.leverage, p.entry_price), (2.0, 10.0, 100.0));
    assert!(h.engine.store().loaded().positions);
}
#[test]
fn terminal_events_fence_both_snapshot_and_out_of_order_frames() {
    let mut h = Harness::new();
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!("{{\"code\":0,\"result\":{{\"data\":[{ORDER}]}}}}"),
    );
    h.ws(
        "user.order",
        &ORDER
            .replace("ACTIVE", "FILLED")
            .replace("\"update_time\":1", "\"update_time\":2"),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    h.ws("user.order", ORDER);
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(
        h.engine.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    assert_eq!(
        h.engine.store().orders()[0].client_order_id.as_deref(),
        Some("caller")
    );
}
#[test]
fn failed_second_order_snapshot_keeps_the_first_good_state() {
    let mut h = Harness::new();
    h.ws("user.order", ORDER);
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    let r = h.http("advanced/get-open-orders");
    h.event(Event::HttpResponse {
        request: r.id,
        response: safe_cex_engine::Response {
            status: 403,
            headers: "",
            body: "denied",
        },
    });
    assert_eq!(h.engine.store().orders().len(), 1);
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(
            safe_cex_engine::LogEvent { .. }
        ))
    )));
}
#[test]
fn each_execution_emits_once_with_large_ids_intact() {
    let mut h = Harness::new();
    h.load();
    h.engine.take_frame(h.now);
    let row = r#"{"trade_id":9007199254740997,"order_id":9007199254740993,"instrument_name":"BTCUSD-PERP","side":"BUY","traded_price":"100","traded_quantity":"0.1"}"#;
    h.ws("user.trade", row);
    h.ws("user.trade", row);
    let bytes = h.engine.take_frame(h.now).unwrap();
    let mut at = 16;
    while at < bytes.len() {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            assert_eq!(
                u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()),
                1
            );
            return;
        }
        at += 8 + len;
    }
    panic!("missing fills section");
}

#[test]
fn closure_timestamps_fence_late_nonzero_and_zero_frames() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    let zero =
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"0","cost":"0","update_timestamp_ms":20}"#;
    h.ws("user.positions", short);
    assert_eq!(h.engine.store().positions()[0].id, "BTCUSD-PERP:short");
    h.ws("user.positions", &zero.replace(":20", ":5"));
    assert_eq!(h.engine.store().positions().len(), 1);
    h.ws("user.positions", zero);
    h.ws("user.positions", short);
    assert!(h.engine.store().positions().is_empty());
}
#[test]
fn live_long_does_not_drop_untouched_short_in_initial_snapshot() {
    let mut h = Harness::new();
    let old = h.http("get-positions");
    let long =
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"2","cost":"200","update_timestamp_ms":10}"#;
    let short =
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"-1","cost":"100","update_timestamp_ms":1}"#;
    h.ws("user.positions", long);
    h.respond(
        &old,
        &format!(
            "{{\"code\":0,\"result\":{{\"data\":[{},{}]}}}}",
            long.replace(":10", ":1").replace("\"2\"", "\"1\""),
            short
        ),
    );
    let positions = h.engine.store().positions();
    assert_eq!(positions.len(), 2);
    assert!(
        positions
            .iter()
            .any(|p| p.id == "BTCUSD-PERP:long" && p.contracts == 2.0)
    );
    assert!(
        positions
            .iter()
            .any(|p| p.id == "BTCUSD-PERP:short" && p.contracts == 1.0)
    );
}
#[test]
fn signature_rejections_retry_once_even_with_http_error_status() {
    let mut h = Harness::new();
    for attempt in 0..2 {
        let r = h.http("get-positions");
        h.event(Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 401,
                headers: "",
                body: r#"{"code":40101,"message":"bad signature"}"#,
            },
        });
        if attempt == 0 {
            assert!(h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn closed_exposure_stays_closed_against_cached_rest_and_equal_time_frames() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    h.ws("user.positions", short);
    h.ws(
        "user.positions",
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"0","cost":"0","update_timestamp_ms":20}"#,
    );
    h.ws("user.positions", &short.replace(":10", ":20"));
    assert!(h.engine.store().positions().is_empty());
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    assert!(h.engine.store().positions().is_empty());
}
#[test]
fn rest_position_timestamps_fence_older_stream_frames() {
    let mut h = Harness::new();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":30}"#;
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    h.ws(
        "user.positions",
        &short.replace(":30", ":10").replace(r#""-2""#, r#""-1""#),
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}

#[test]
fn cached_rest_order_cannot_rewind_stream_update_before_request_started() {
    let mut h = Harness::new();
    let live = ORDER
        .replace("\"update_time\":1", "\"update_time\":30")
        .replace(
            "\"cumulative_quantity\":\"0\"",
            "\"cumulative_quantity\":\"0.5\"",
        );
    h.ws("user.order", &live);
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    let row = &h.engine.store().orders()[0];
    assert_eq!(row.last_update, 30.0);
    assert_eq!(row.filled, 0.5);
}

#[test]
fn authoritative_empty_snapshot_fences_duplicate_live_position() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    h.ws("user.positions", short);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    h.reply("get-positions", EMPTY);
    assert!(h.engine.store().positions().is_empty());
    h.ws("user.positions", short);
    assert!(
        h.engine.store().positions().is_empty(),
        "duplicate older exposure must not resurrect after authoritative closure"
    );
    h.reply("user-balance", BALANCE);
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    assert!(h.engine.store().positions().is_empty());
    h.ws("user.positions", &short.replace(":10", ":11"));
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}

fn next_tick(h: &mut Harness) {
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 10000.0;
    h.event(Event::TimerFired { timer });
}
#[test]
fn cached_rest_after_reopen_keeps_newer_live_exposure() {
    let mut h = Harness::new();
    h.load();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":10}"#;
    h.ws("user.positions", short);
    next_tick(&mut h);
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.ws("user.positions", &short.replace(":10", ":11"));
    assert_eq!(h.engine.store().positions()[0].last_update, Some(11.0));
    next_tick(&mut h);
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    assert_eq!(
        h.engine.store().positions().len(),
        1,
        "cached closed row must not erase newer reopened exposure"
    );
    assert_eq!(h.engine.store().positions()[0].last_update, Some(11.0));
}

// ETHUSD-PERP is deliberately filtered (not tradable); BADUSD-PERP is malformed.
fn catalogue(bad_leverage: &str) -> String {
    let perp = |symbol: &str, tradable: bool, leverage: &str| {
        format!(
            r#"{{"symbol":"{symbol}","inst_type":"PERPETUAL_SWAP","tradable":{tradable},"base_ccy":"X","quote_ccy":"USD","price_tick_size":"0.1","qty_tick_size":"0.001","quote_decimals":1,"quantity_decimals":3,"max_leverage":{leverage}}}"#
        )
    };
    format!(
        r#"{{"code":0,"result":{{"data":[{},{},{}]}}}}"#,
        perp("BTCUSD-PERP", true, r#""50""#),
        perp("ETHUSD-PERP", false, r#""50""#),
        perp("BADUSD-PERP", true, bad_leverage),
    )
}
fn rows(rows: &[&str]) -> String {
    format!(r#"{{"code":0,"result":{{"data":[{}]}}}}"#, rows.join(","))
}
const BTC_POSITION: &str =
    r#"{"instrument_name":"BTCUSD-PERP","quantity":"1","cost":"100","update_timestamp_ms":1}"#;

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let mut h = Harness::new();
    h.load();
    let mut e = h.engine;
    let now = 1_610_000_001_000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let mut obsolete = stale;
    let mut current = None;
    for interval in [100_000_f64, 1_500.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(0.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending.iter().any(|r| r.url.contains("get-positions")),
        "actual native account request"
    );
    // Pause while REST is in flight. Completion may finish the current read
    // chain, but must not recreate an active recurring host timer.
    e.handle(
        due,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    e.drain_actions().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = if r.url.contains("user-balance") {
                BALANCE
            } else {
                EMPTY
            };
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            pending.extend(e.drain_actions().filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            }));
        }
    }
    assert!(pending.is_empty(), "all fixture account reads complete");
    e.handle(
        due + 2.,
        Event::PollScaleChanged {
            scale: 10_000. / POLL_MS,
        },
    );
    let resumed: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(0.0));
}

#[test]
fn owned_position_and_order_on_filtered_market_are_skipped_and_the_chain_commits() {
    let mut h = Harness::with_instruments(&catalogue("50"));
    let filtered = BTC_POSITION.replace("BTCUSD-PERP", "ETHUSD-PERP");
    h.reply("get-positions", &rows(&[BTC_POSITION, &filtered]));
    h.reply("user-balance", BALANCE);
    let filtered_order = ORDER
        .replace("BTCUSD-PERP", "ETHUSD-PERP")
        .replace("9007199254740993", "2");
    h.reply("private/get-open-orders", &rows(&[ORDER, &filtered_order]));
    h.reply("advanced/get-open-orders", EMPTY);
    let store = h.engine.store();
    let loaded = store.loaded();
    assert!(loaded.positions && loaded.balance && loaded.orders);
    assert_eq!(store.balance().total, 1000.0);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "BTCUSD-PERP");
    assert_eq!(store.orders().len(), 1);
    assert_eq!(store.orders()[0].id, "9007199254740993");
    assert!(warned(&h, "position on uncatalogued market ETHUSD-PERP"));
    assert!(warned(&h, "order on uncatalogued market ETHUSD-PERP"));
}

#[test]
fn owned_position_on_malformed_market_preserves_the_previous_snapshot() {
    let mut h = Harness::with_instruments(&catalogue("50"));
    h.reply("get-positions", &rows(&[BTC_POSITION]));
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    next_tick(&mut h);
    let changed = BTC_POSITION.replace("\"1\"", "\"3\"").replace(":1}", ":2}");
    let malformed = BTC_POSITION.replace("BTCUSD-PERP", "BADUSD-PERP");
    h.reply("get-positions", &rows(&[&changed, &malformed]));
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    assert!(warned(&h, "owned position market unavailable"));
}

#[test]
fn working_order_on_malformed_market_rejects_snapshot_but_other_missing_rows_are_skipped() {
    for (symbol, status, loaded) in [
        ("BADUSD-PERP", "ACTIVE", false),
        ("BADUSD-PERP", "FILLED", true),
        ("ETHUSD-PERP", "ACTIVE", true),
        ("MISSINGUSD-PERP", "ACTIVE", true),
        ("MISSING_USD", "ACTIVE", true),
    ] {
        let mut h = Harness::with_instruments(&catalogue("50"));
        h.reply("get-positions", EMPTY);
        h.reply("user-balance", BALANCE);
        let order = ORDER
            .replace("BTCUSD-PERP", symbol)
            .replace("ACTIVE", status);
        h.reply("private/get-open-orders", &rows(&[&order]));
        if loaded {
            h.reply("advanced/get-open-orders", EMPTY);
        }
        assert_eq!(
            h.engine.store().loaded().orders,
            loaded,
            "{symbol} {status}"
        );
    }
}

#[test]
fn partial_catalogue_commits_then_rereads_without_a_second_private_start() {
    let mut h = Harness::with_instruments(&catalogue("50"));
    assert!(h.engine.store().market("BTCUSD-PERP").is_some());
    assert!(h.engine.store().market("BADUSD-PERP").is_none());
    let retry = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - h.now, 1000.0) => Some(*timer),
            _ => None,
        })
        .expect("partial catalogue retry");
    h.load();
    h.actions.clear();
    h.now += 1000.0;
    h.event(Event::TimerFired { timer: retry });
    h.reply("public/get-instruments", &catalogue(r#""20""#));
    assert!(h.engine.store().market("BADUSD-PERP").is_some());
    assert!(h.engine.store().market("BTCUSD-PERP").is_some());
    assert!(
        !h.actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::WsConnect { .. } | Action::SetTimer { .. }
        )),
        "no ticker reseed, private restart or further retry: {:?}",
        h.actions
    );
    assert!(h.engine.store().loaded().positions);
}

fn warned(h: &Harness, needle: &str) -> bool {
    h.actions.iter().any(|a| {
        matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. }))
            if message.contains(needle))
    })
}
#[test]
fn unmappable_snapshot_rows_are_skipped_and_keep_known_state() {
    let mut h = Harness::new();
    let short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","open_position_pnl":null,"isolation_type":null,"liquidation_price":null,"update_timestamp_ms":1}"#;
    h.reply(
        "get-positions",
        &format!(r#"{{"code":0,"result":{{"data":[{short}]}}}}"#),
    );
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    let bad_short = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":null}"#;
    let foreign = r#"{"instrument_name":"ETHUSD-PERP","quantity":null}"#;
    let mut orders_read = false;
    // Orders are re-read on every fourth reconcile.
    for _ in 0..4 {
        next_tick(&mut h);
        h.reply(
            "get-positions",
            &format!(r#"{{"code":0,"result":{{"data":[{bad_short},{foreign}]}}}}"#),
        );
        assert!(warned(
            &h,
            "Crypto.com skipped unmappable position row BTCUSD-PERP"
        ));
        assert!(!warned(&h, "ETHUSD-PERP"), "foreign rows stay quiet");
        h.reply("user-balance", BALANCE);
        orders_read = h
            .actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("private/get-open-orders")));
        if orders_read {
            break;
        }
    }
    assert!(orders_read, "orders were never re-read");
    let position_skips = h
        .actions
        .iter()
        .filter(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("position row BTCUSD-PERP")))
        .count();
    assert_eq!(position_skips, 1, "a persistent skip is logged once");
    let bad = r#"{"order_id":"9007199254740993","instrument_name":"BTCUSD-PERP","order_type":"LIMIT","side":"BUY","quantity":null}"#;
    let fresh = r#"{"order_id":"77","instrument_name":"BTCUSD-PERP","order_type":"LIMIT","side":"SELL","quantity":"1","limit_price":"110","cumulative_quantity":null,"time_in_force":null,"exec_inst":null}"#;
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{bad},{fresh}]}}}}"#),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    assert!(warned(
        &h,
        "Crypto.com skipped unmappable order row 9007199254740993"
    ));
    let mut ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.as_str())
        .collect();
    ids.sort();
    assert_eq!(ids, ["77", "9007199254740993"]);
    let fresh = h.engine.store().order("77").unwrap();
    assert_eq!(
        (fresh.filled, fresh.remaining, fresh.time_in_force),
        (0.0, 1.0, Some(TimeInForce::GoodTillCancel))
    );
    let positions = h.engine.store().positions();
    assert_eq!((positions.len(), positions[0].contracts), (1, 2.0));
}
#[test]
fn balance_snapshot_ignores_bad_rows_after_the_summary() {
    let mut h = Harness::new();
    h.reply("get-positions", EMPTY);
    h.reply(
        "user-balance",
        r#"{"code":0,"result":{"data":[{"total_margin_balance":"1000","total_available_balance":"950","total_initial_margin":"50","total_session_unrealized_pnl":"10"},{"total_margin_balance":null}]}}"#,
    );
    h.reply("private/get-open-orders", EMPTY);
    assert_eq!(h.engine.store().balance().total, 1000.0);
    assert!(warned(&h, "skipped 1 unmappable balance rows"));
}
#[test]
fn pushed_order_with_null_fill_keeps_the_stored_fill() {
    let mut h = Harness::new();
    h.load();
    h.ws(
        "user.order",
        &ORDER
            .replace(
                r#""cumulative_quantity":"0""#,
                r#""cumulative_quantity":"0.4""#,
            )
            .replace(
                r#""time_in_force":"GOOD_TILL_CANCEL""#,
                r#""time_in_force":"IMMEDIATE_OR_CANCEL""#,
            ),
    );
    h.ws(
        "user.order",
        &ORDER
            .replace(
                r#""cumulative_quantity":"0""#,
                r#""cumulative_quantity":null"#,
            )
            .replace(
                r#""time_in_force":"GOOD_TILL_CANCEL""#,
                r#""time_in_force":null"#,
            )
            .replace(r#""update_time":1"#, r#""update_time":2"#),
    );
    let order = &h.engine.store().orders()[0];
    assert_eq!(
        (order.filled, order.remaining, order.time_in_force),
        (0.4, 0.6, Some(TimeInForce::ImmediateOrCancel))
    );
}
fn push(h: &mut Harness, channel: &str, rows: &str) {
    let socket = h.socket(true);
    let text = format!(r#"{{"code":0,"result":{{"channel":"{channel}","data":[{rows}]}}}}"#);
    h.event(Event::WsMessage {
        socket,
        text: &text,
    });
}
#[test]
fn bad_rows_in_pushed_frames_do_not_drop_their_neighbours() {
    let mut h = Harness::new();
    h.load();
    push(
        &mut h,
        "user.order",
        &format!(
            r#"{{"order_id":"5","instrument_name":"BTCUSD-PERP","quantity":null}},{{"order_id":"6","instrument_name":"BTCUSD-PERP"}},{ORDER}"#
        ),
    );
    assert_eq!(h.engine.store().orders().len(), 1);
    assert!(warned(&h, "skipped 2 unmappable order rows (first: 5)"));

    push(
        &mut h,
        "user.positions",
        r#"{"instrument_name":"BTCUSD-PERP","quantity":"1","cost":null},{"instrument_name":"ETHUSD-PERP","quantity":null},{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200","update_timestamp_ms":5}"#,
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    assert!(warned(
        &h,
        "skipped 1 unmappable position rows (first: BTCUSD-PERP)"
    ));
    assert!(!warned(&h, "ETHUSD-PERP"));

    push(
        &mut h,
        "user.balance",
        r#"{"total_margin_balance":null},{"total_margin_balance":"2000","total_available_balance":"1900","total_initial_margin":"100","total_session_unrealized_pnl":"0"}"#,
    );
    assert_eq!(h.engine.store().balance().total, 2000.0);
    assert!(warned(&h, "skipped 1 unmappable balance rows"));

    h.engine.take_frame(h.now);
    push(
        &mut h,
        "user.trade",
        r#"{"trade_id":9,"order_id":5,"instrument_name":"BTCUSD-PERP","traded_price":null},{"trade_id":10,"order_id":6,"instrument_name":"BTCUSD-PERP","side":"BUY","traded_price":"100","traded_quantity":"0.1"}"#,
    );
    assert!(warned(&h, "skipped 1 unmappable execution rows (first: 9)"));
    assert_eq!(fill_count(h.engine.take_frame(h.now).unwrap()), 1);
}
fn fill_count(bytes: &[u8]) -> u32 {
    let mut at = 16;
    while at < bytes.len() {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + len;
    }
    0
}
const COINS: &str = r#""position_balances":[{"instrument_name":"USD","quantity":"1000","max_withdrawal_balance":"950"},{"instrument_name":"CRO","quantity":"5","max_withdrawal_balance":"7"},{"instrument_name":"BTC","quantity":"1e","max_withdrawal_balance":"1"},{"quantity":"1"}]"#;
fn coins(h: &Harness) -> Vec<(String, f64, f64, f64, f64)> {
    h.engine
        .store()
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
fn balance_snapshot_publishes_per_coin_balances_and_skips_bad_coins() {
    let mut h = Harness::new();
    h.reply("get-positions", EMPTY);
    h.reply(
        "user-balance",
        &BALANCE.replace(r#""10"}"#, &format!(r#""10",{COINS}}}"#)),
    );
    h.reply("private/get-open-orders", EMPTY);
    assert_eq!(
        (
            h.engine.store().balance().total,
            h.engine.store().balance().free
        ),
        (1000.0, 950.0)
    );
    assert_eq!(
        coins(&h),
        [
            ("CRO".into(), 5.0, 7.0, 0.0, 0.0),
            ("USD".into(), 1000.0, 950.0, 50.0, 0.0)
        ]
    );
    assert!(warned(
        &h,
        "skipped 2 unmappable currency balance rows (first: BTC)"
    ));
}
#[test]
fn pushed_balance_publishes_per_coin_balances_with_the_aggregate() {
    let mut h = Harness::new();
    h.load();
    assert_eq!(h.engine.store().currency_balances(), Some(&[][..]));
    h.ws(
        "user.balance",
        &format!(
            r#"{{"total_margin_balance":"2000","total_available_balance":"1900","total_initial_margin":"100","total_session_unrealized_pnl":"0",{COINS}}}"#
        ),
    );
    assert_eq!(h.engine.store().balance().total, 2000.0);
    assert_eq!(coins(&h).len(), 2);
    assert_eq!(coins(&h)[1], ("USD".into(), 1000.0, 950.0, 50.0, 0.0));
    h.ws(
        "user.balance",
        r#"{"total_margin_balance":"2100","total_available_balance":"1900","total_initial_margin":"100","total_session_unrealized_pnl":"0","position_balances":"garbled"}"#,
    );
    assert_eq!(h.engine.store().balance().total, 2100.0);
    assert_eq!(coins(&h).len(), 2);
}

/// Polls until an order read is issued, answering every read; returns the logs it produced.
fn reconcile_with_orders(h: &mut Harness, positions: &str, orders: &str) -> Vec<String> {
    reconcile_with_lists(h, positions, orders, "")
}
/// `reconcile_with_orders`, with `advanced` as the advanced order list.
fn reconcile_with_lists(
    h: &mut Harness,
    positions: &str,
    orders: &str,
    advanced: &str,
) -> Vec<String> {
    let start = h.actions.len();
    loop {
        next_tick(h);
        h.reply(
            "get-positions",
            &format!(r#"{{"code":0,"result":{{"data":[{positions}]}}}}"#),
        );
        h.reply("user-balance", BALANCE);
        if h.actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("private/get-open-orders")))
        {
            h.reply(
                "private/get-open-orders",
                &format!(r#"{{"code":0,"result":{{"data":[{orders}]}}}}"#),
            );
            h.reply(
                "advanced/get-open-orders",
                &format!(r#"{{"code":0,"result":{{"data":[{advanced}]}}}}"#),
            );
            break;
        }
    }
    h.actions[start.min(h.actions.len())..]
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(event)) => Some(event.message.clone()),
            _ => None,
        })
        .collect()
}
#[test]
fn an_unidentified_snapshot_row_protects_stored_rows_while_valid_peers_commit() {
    let instruments = INSTRUMENTS.replace(
        "]}}",
        r#",{"symbol":"ETHUSD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"ETH","quote_ccy":"USD","price_tick_size":"0.1","qty_tick_size":"0.001","quote_decimals":1,"quantity_decimals":3,"max_leverage":"50","underlying_symbol":"ETHUSD-INDEX"}]}}"#,
    );
    let mut h = Harness::with_instruments(&instruments);
    let position = |symbol: &str, qty: &str, time: u32| {
        format!(
            r#"{{"instrument_name":"{symbol}","quantity":"{qty}","cost":"200","update_timestamp_ms":{time}}}"#
        )
    };
    let order = |price: &str, time: u32| {
        format!(
            r#"{{"order_id":"77","instrument_name":"BTCUSD-PERP","order_type":"LIMIT","side":"SELL","quantity":"1","limit_price":"{price}","status":"ACTIVE","update_time":{time}}}"#
        )
    };
    h.reply(
        "get-positions",
        &format!(
            r#"{{"code":0,"result":{{"data":[{},{}]}}}}"#,
            position("BTCUSD-PERP", "1", 1),
            position("ETHUSD-PERP", "-2", 1)
        ),
    );
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(
            r#"{{"code":0,"result":{{"data":[{ORDER},{}]}}}}"#,
            order("110", 1)
        ),
    );
    h.reply("advanced/get-open-orders", EMPTY);
    let eth = h
        .engine
        .store()
        .position("ETHUSD-PERP:short")
        .unwrap()
        .clone();
    let stored_order = h.engine.store().order("9007199254740993").unwrap().clone();

    let unnamed_position = r#"{"quantity":"1","cost":null}"#;
    let unnamed_order = r#"{"instrument_name":"BTCUSD-PERP","order_type":"LIMIT","quantity":null}"#;
    for round in 0..2 {
        let logs = reconcile_with_orders(
            &mut h,
            &format!(
                "{},{unnamed_position}",
                position("BTCUSD-PERP", "3", 2 + round)
            ),
            &format!("{},{unnamed_order}", order("111", 2 + round)),
        );
        let store = h.engine.store();
        assert_eq!(store.position("BTCUSD-PERP:long").unwrap().contracts, 3.0);
        assert_eq!(store.position("ETHUSD-PERP:short"), Some(&eth));
        assert_eq!(store.order("77").unwrap().price, 111.0);
        assert_eq!(store.order("9007199254740993"), Some(&stored_order));
        let skips: Vec<_> = logs.iter().filter(|l| l.contains("skipped")).collect();
        let expected: &[&str] = if round == 0 {
            &[
                "Crypto.com skipped unmappable position row without an instrument",
                "Crypto.com skipped unmappable order row without an id",
            ]
        } else {
            &[]
        };
        assert_eq!(skips, expected, "{logs:?}");
        assert!(!logs.iter().any(|l| l.contains("null") || l.contains('{')));
    }

    reconcile_with_orders(&mut h, &position("BTCUSD-PERP", "3", 9), &order("111", 9));
    let store = h.engine.store();
    let positions: Vec<_> = store.positions().iter().map(|p| p.id.as_str()).collect();
    assert_eq!(positions, ["BTCUSD-PERP:long"]);
    let orders: Vec<_> = store.orders().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(orders, ["77"]);
}

#[test]
fn an_unidentified_row_protects_only_its_own_order_list() {
    let mut h = Harness::new();
    let advanced = r#"{"order_id":"55","instrument_name":"BTCUSD-PERP","order_type":"STOP_LOSS","side":"SELL","quantity":"1","trigger_price":"90","status":"ACTIVE","update_time":1}"#;
    h.reply("get-positions", r#"{"code":0,"result":{"data":[]}}"#);
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
    );
    h.reply(
        "advanced/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{advanced}]}}}}"#),
    );
    let ids = |h: &Harness| {
        let mut ids: Vec<_> = h
            .engine
            .store()
            .orders()
            .iter()
            .map(|o| o.id.clone())
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&h), ["55", "9007199254740993"]);
    let unnamed = r#"{"instrument_name":"BTCUSD-PERP","order_type":"LIMIT","quantity":null}"#;
    // The advanced order left its list; the regular list's id-less row cannot vouch for it.
    reconcile_with_lists(&mut h, "", unnamed, "");
    assert_eq!(ids(&h), ["9007199254740993"]);
    reconcile_with_lists(&mut h, "", ORDER, advanced);
    assert_eq!(ids(&h), ["55", "9007199254740993"]);
    // And the reverse: an id-less advanced row keeps 55 but not the departed regular order.
    reconcile_with_lists(&mut h, "", "", unnamed);
    assert_eq!(ids(&h), ["55"]);
}

#[test]
fn a_market_without_a_positive_step_is_malformed_and_holds_its_exposure() {
    let valid = r#""symbol":"BADUSD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"X","quote_ccy":"USD","price_tick_size":"0.1","qty_tick_size":"0.001","quote_decimals":1,"#;
    let zero_tick = r#""symbol":"BADUSD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"X","quote_ccy":"USD","price_tick_size":"0","qty_tick_size":"0.001","quote_decimals":400,"#;
    let markets = catalogue(r#""50""#).replace(valid, zero_tick);
    assert_ne!(markets, catalogue(r#""50""#));
    let mut h = Harness::with_instruments(&markets);
    assert!(h.engine.store().market("BADUSD-PERP").is_none());
    h.reply("get-positions", &rows(&[BTC_POSITION]));
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    next_tick(&mut h);
    let changed = BTC_POSITION.replace("\"1\"", "\"3\"").replace(":1}", ":2}");
    let malformed = BTC_POSITION.replace("BTCUSD-PERP", "BADUSD-PERP");
    h.reply("get-positions", &rows(&[&changed, &malformed]));
    assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    assert!(warned(&h, "owned position market unavailable"));
}

#[test]
fn decoded_position_rearms_skip_memory_before_the_snapshot_finishes() {
    let mut h = Harness::new();
    h.load();
    let bad = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":null}"#;
    let good = r#"{"instrument_name":"BTCUSD-PERP","quantity":"-2","cost":"200"}"#;
    h.ws("user.positions", good);
    for (rows, expected) in [
        (bad.to_owned(), 1),
        (bad.to_owned(), 1),
        (format!("{good},{bad}"), 2),
        (bad.to_owned(), 2),
    ] {
        next_tick(&mut h);
        h.reply(
            "get-positions",
            &format!(r#"{{"code":0,"result":{{"data":[{rows}]}}}}"#),
        );
        h.reply("user-balance", BALANCE);
        if h.actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("private/get-open-orders")))
        {
            h.reply("private/get-open-orders", EMPTY);
            h.reply("advanced/get-open-orders", EMPTY);
        }
        let reports = h
            .actions
            .iter()
            .filter(|a| {
                matches!(a,
                    Action::Notice(safe_cex_engine::Notice::Event(event))
                        if event.message == "Crypto.com skipped unmappable position row BTCUSD-PERP"
                )
            })
            .count();
        assert_eq!(reports, expected);
        if expected == 1 {
            // The same remembered identity stays silent after its stored copy
            // disappears: Crypto.com uses Once, not Coinbase's Error upgrade.
            h.engine.store_mut().set_positions(vec![]);
        }
    }
    let levels: Vec<_> = h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.message == "Crypto.com skipped unmappable position row BTCUSD-PERP" =>
            {
                Some(event.level)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        levels,
        [safe_cex_engine::Level::Warn, safe_cex_engine::Level::Error]
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}

#[test]
fn snapshot_token_regular_advanced_join_preserves_absent_rows_after_invalidation() {
    let mut h = Harness::new();
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
    );
    h.engine.store_mut().invalidate_order_snapshots();
    h.reply("advanced/get-open-orders", EMPTY);
    assert!(h.engine.store().orders().is_empty());
    assert!(!h.engine.store().loaded().orders);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
}

#[test]
fn snapshot_token_accepted_cancel_ack_fences_absent_rows_and_retry_keeps_authority() {
    for retry in 0..3 {
        let mut h = Harness::new();
        h.reply("get-positions", EMPTY);
        h.reply("user-balance", BALANCE);
        h.reply(
            "private/get-open-orders",
            &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
        );
        h.event(Event::Command(
            safe_cex_engine::Command::CancelSymbolOrders {
                command: 908,
                symbol: "BTCUSD-PERP".into(),
            },
        ));
        h.reply("cancel-all-orders", r#"{"code":0,"result":{}}"#);
        if retry == 1 {
            h.reply(
                "advanced/get-open-orders",
                r#"{"code":40101,"message":"retry auth"}"#,
            );
        } else if retry == 2 {
            let failed = h.http("advanced/get-open-orders");
            h.event(Event::HttpFailed {
                request: failed.id,
                error: "transport loss",
            });
        }
        h.reply("advanced/get-open-orders", EMPTY);
        assert!(h.engine.store().orders().is_empty(), "retry={retry}");
        assert!(!h.engine.store().loaded().orders);
    }
}

#[test]
fn snapshot_token_capture_and_late_exhaustion_preserve_reconciliation_timer() {
    for late in [false, true] {
        let mut h = Harness::new();
        h.reply("get-positions", EMPTY);
        if !late {
            safe_cex_engine::test_support::exhaust_order_snapshots(h.engine.store_mut());
        }
        h.reply("user-balance", BALANCE);
        if late {
            h.reply(
                "private/get-open-orders",
                &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
            );
            safe_cex_engine::test_support::exhaust_order_snapshots(h.engine.store_mut());
            h.reply("advanced/get-open-orders", EMPTY);
        } else {
            assert!(
                !h.actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.contains("get-open-orders")))
            );
        }
        assert!(h.engine.store().orders().is_empty());
        assert!(!h.engine.store().loaded().orders);
        let timer = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms > h.now => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap();
        h.now = timer.1;
        h.event(Event::TimerFired { timer: timer.0 });
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("get-positions")))
        );
    }
}

#[test]
fn snapshot_token_ws_advanced_peer_survives_and_regular_rows_progress() {
    let mut h = Harness::new();
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    let sibling = h.engine.store_mut().begin_orders_snapshot().unwrap();
    h.reply(
        "private/get-open-orders",
        &format!(r#"{{"code":0,"result":{{"data":[{ORDER}]}}}}"#),
    );
    let advanced = ORDER
        .replace("9007199254740993", "8123")
        .replace("\"LIMIT\"", "\"STOP_LOSS\"")
        .replace("\"limit_price\":\"90\"", "\"trigger_price\":\"95\"");
    h.ws("user.advance.order", &advanced);
    h.reply("advanced/get-open-orders", EMPTY);
    assert!(h.engine.store().order("8123").is_some());
    assert!(h.engine.store().order("9007199254740993").is_some());
    assert!(h.engine.store().orders_snapshot_current(&sibling));
}

#[test]
fn snapshot_token_invalid_order_ack_does_not_invalidate_valid_identity_does() {
    for (body, accepted) in [
        (r#"{"code":40001,"message":"refused"}"#, false),
        (r#"{"code":0,"result":{"order_id":"bad"}}"#, false),
        (r#"{"code":0,"result":{"order_id":123}}"#, true),
    ] {
        let mut h = Harness::new();
        h.load();
        let token = h.engine.store_mut().begin_orders_snapshot().unwrap();
        let order = serde_json::from_str(
            r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.0021,"price":90.04}"#,
        )
        .unwrap();
        h.event(Event::Command(safe_cex_engine::Command::PlaceOrders {
            command: 911,
            orders: vec![order],
        }));
        h.reply("private/create-order", body);
        assert_eq!(
            h.engine.store().orders_snapshot_current(&token),
            !accepted,
            "{body}"
        );
    }
}
