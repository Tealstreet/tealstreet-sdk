//! Contract V3 official Account_API and private channel payloads; numeric IDs
//! deliberately exceed JavaScript's exact integer range.
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, RequestId, Response, SocketId, TimerId};
use safe_cex_venue_weex::{Options, WeexFutures};
const NOW: f64 = 1_773_295_700_000.0;
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
const NORMAL: &str = r#"[{"orderId":702345678901234600,"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW","origQty":"0.02","executedQty":"0","price":"69500","timeInForce":"POST_ONLY","updateTime":1773295700000}]"#;
const ALGO: &str = r#"[{"algoId":812345678901234500,"symbol":"BTCUSDT","side":"SELL","orderType":"STOP","algoStatus":"UNTRIGGERED","quantity":"0.01","triggerPrice":"68900","price":"68800","tpTriggerPrice":"70500","slTriggerPrice":"68000","updateTime":1773295700000}]"#;
const POSITIONS: &str = r#"[{"id":689987235755328154,"symbol":"BTCUSDT","side":"LONG","marginType":"CROSSED","separatedMode":"COMBINED","leverage":"100","size":"0.02","openValue":"1801.067","marginSize":"17.15","isolatedMargin":"0","updatedTime":1773295700000,"unrealizePnl":"-85.569"}]"#;
fn response(e: &mut Engine<WeexFutures>, id: RequestId, status: u16, body: &str) {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|action| {
            if let Action::Http(r) = action {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}
fn request(actions: &[Action], path: &str) -> RequestId {
    requests(actions)
        .into_iter()
        .find(|r| r.url.contains(path))
        .unwrap()
        .id
}
fn started(ws: bool) -> (Engine<WeexFutures>, Vec<Action>) {
    let mut e = Engine::new(WeexFutures::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        private_ws_headers: ws,
        ..Options::default()
    }));
    e.start(NOW);
    let actions: Vec<_> = e.drain_actions().collect();
    response(&mut e, request(&actions, "exchangeInfo"), 200, INFO);
    let actions: Vec<_> = e.drain_actions().collect();
    response(
        &mut e,
        request(&actions, "24hr"),
        200,
        r#"[{"symbol":"BTCUSDT","lastPrice":"90000"}]"#,
    );
    response(
        &mut e,
        request(&actions, "bookTicker"),
        200,
        r#"[{"symbol":"BTCUSDT","bidPrice":"89999","askPrice":"90001"}]"#,
    );
    let actions = e.drain_actions().collect();
    (e, actions)
}
fn complete(e: &mut Engine<WeexFutures>, actions: &[Action], normal: &str, algo: &str) {
    response(
        e,
        request(actions, "accountConfig"),
        200,
        r#"{"dualSidePosition":false,"canTrade":true}"#,
    );
    response(
        e,
        request(actions, "symbolConfig"),
        200,
        r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","separatedType":"COMBINED","crossLeverage":"20","isolatedLongLeverage":"10","isolatedShortLeverage":"15"}]"#,
    );
    response(
        e,
        request(actions, "/balance"),
        200,
        r#"[{"asset":"USDT","balance":"5696.49","availableBalance":"5413.06","frozen":"81.28","unrealizePnl":"-34.553"}]"#,
    );
    response(e, request(actions, "allPosition"), 200, POSITIONS);
    response(e, request(actions, "/openOrders"), 200, normal);
    response(e, request(actions, "/openAlgoOrders"), 200, algo);
}
fn next_cycle(e: &mut Engine<WeexFutures>) -> Vec<Action> {
    let timer = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 2000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 2000.0, Event::TimerFired { timer });
    e.drain_actions().collect()
}
fn private_socket(actions: &[Action]) -> SocketId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnectWithHeaders { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap()
}
#[test]
fn documented_private_snapshots_and_browser_rest_fallback() {
    let (mut e, actions) = started(false);
    assert_eq!(requests(&actions).len(), 6);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnectWithHeaders { .. }))
    );
    for request in requests(&actions) {
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == "ACCESS-SIGN" && !value.is_empty())
        );
    }
    complete(&mut e, &actions, NORMAL, ALGO);
    assert_eq!(e.store().orders().len(), 4);
    assert_eq!(e.store().orders()[0].id, "702345678901234600");
    assert_eq!(e.store().positions()[0].id, "BTCUSDT");
    assert_eq!(e.store().positions()[0].leverage, 100.0);
    assert_eq!(e.store().balance().free, 5413.06);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert!(
        e.store().loaded().orders && e.store().loaded().balance && e.store().loaded().positions
    );
    assert_eq!(
        next_cycle(&mut e)
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        6
    );
}
#[test]
fn failed_or_malformed_half_never_replaces_order_snapshot() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    let expected = e.store().orders().to_vec();
    let actions = next_cycle(&mut e);
    response(&mut e, request(&actions, "/openOrders"), 503, "unavailable");
    response(&mut e, request(&actions, "/openAlgoOrders"), 200, "[]");
    assert_eq!(e.store().orders(), expected);
    let (mut e, actions) = started(false);
    response(&mut e, request(&actions, "/openOrders"), 200, NORMAL);
    response(
        &mut e,
        request(&actions, "/openAlgoOrders"),
        200,
        r#"[{"algoId":1,"symbol":"BTCUSDT","side":"SELL","orderType":"STOP","algoStatus":"NEW"}]"#,
    );
    assert!(!e.store().loaded().orders);
}
#[test]
fn complete_paging_required_before_order_commit() {
    let (mut e, actions) = started(false);
    let row = &NORMAL[1..NORMAL.len() - 1];
    let page = format!("[{}]", vec![row; 100].join(","));
    response(&mut e, request(&actions, "/openOrders"), 200, &page);
    let pages: Vec<_> = e.drain_actions().collect();
    let follow = requests(&pages)
        .into_iter()
        .find(|r| r.url.contains("/openOrders"))
        .unwrap();
    assert!(follow.url.ends_with("limit=100&page=1"));
    response(&mut e, request(&actions, "/openAlgoOrders"), 200, "[]");
    assert!(!e.store().loaded().orders);
    response(&mut e, follow.id, 200, "[]");
    assert!(e.store().loaded().orders);
}
#[test]
fn websocket_terminal_orders_and_positions_reject_stale_reads_and_events() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"orders","d":[{"id":"702345678901234600","symbol":"BTCUSDT","orderSide":"BUY","type":"LIMIT","status":"CANCELED","size":"0.02","price":"69500","updatedTime":"1773295701000"}]}"#});
    complete(&mut e, &actions, NORMAL, "[]");
    assert_eq!(
        e.store().order("702345678901234600").unwrap().status,
        OrderStatus::Canceled
    );
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"positions","d":[{"symbol":"BTCUSDT","side":"LONG","size":"0","updatedTime":"1773295702000"}]}"#});
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"positions","d":[{"symbol":"BTCUSDT","side":"LONG","size":"1","updatedTime":"1773295701000"}]}"#});
    assert!(e.store().positions().is_empty());
    let actions = next_cycle(&mut e);
    complete(&mut e, &actions, NORMAL, "[]");
    assert!(
        e.store()
            .order("702345678901234600")
            .is_none_or(|order| order.status != OrderStatus::Open)
    );
    assert!(e.store().positions().is_empty());
}
#[test]
fn fills_are_per_execution_and_duplicates_are_suppressed() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    let fills = r#"{"e":"fill","d":[{"id":"1","orderId":"42","symbol":"BTCUSDT","orderSide":"BUY","fillSize":"0.1","fillValue":"100"},{"id":"2","orderId":"42","symbol":"BTCUSDT","orderSide":"BUY","fillSize":"0.2","fillValue":"220"}]}"#;
    e.handle(
        NOW,
        Event::WsMessage {
            socket,
            text: fills,
        },
    );
    e.handle(
        NOW,
        Event::WsMessage {
            socket,
            text: fills,
        },
    );
    let frame = e.take_market_frame(NOW).unwrap();
    let mut at = 16;
    let mut found = false;
    for _ in 0..u16::from_le_bytes([frame[6], frame[7]]) {
        let kind = u16::from_le_bytes([frame[at], frame[at + 1]]);
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let data = &frame[at + 8..at + 8 + len];
            assert_eq!(u32::from_le_bytes(data[..4].try_into().unwrap()), 2);
            // The final columns hold price, amount for each execution.
            assert_eq!(
                f64::from_le_bytes(data[len - 32..len - 24].try_into().unwrap()),
                1000.0
            );
            assert_eq!(
                f64::from_le_bytes(data[len - 16..len - 8].try_into().unwrap()),
                1100.0
            );
            found = true;
        }
        at += 8 + len;
    }
    assert!(found);
    assert!(e.take_market_frame(NOW).is_none());
}
#[test]
fn private_watchdog_reconnect_regenerates_header_timestamp() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    let timer = actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 31_000.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|action| matches!(action,Action::WsClose {socket:s} if *s==socket))
    );
    let timer: TimerId = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 32_000.0, Event::TimerFired { timer });
    assert!(e.drain_actions().any(|action|matches!(action,Action::WsConnectWithHeaders {headers,..} if headers.iter().any(|(key,value)|*key=="ACCESS-TIMESTAMP" && value=="1773295732000"))));
}

#[test]
fn settings_arriving_after_positions_supply_identical_rest_and_socket_fallbacks() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    let sparse = r#"[{"symbol":"BTCUSDT","side":"SHORT","size":"1","openValue":"100","updatedTime":1773295700000}]"#;
    response(&mut e, request(&actions, "allPosition"), 200, sparse);
    assert!(!e.store().loaded().positions);
    response(
        &mut e,
        request(&actions, "accountConfig"),
        200,
        r#"{"dualSidePosition":true,"canTrade":true}"#,
    );
    response(
        &mut e,
        request(&actions, "symbolConfig"),
        200,
        r#"[{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"10","isolatedShortLeverage":"15"}]"#,
    );
    response(&mut e, request(&actions, "/balance"), 200, "[]");
    response(&mut e, request(&actions, "/openOrders"), 200, "[]");
    response(&mut e, request(&actions, "/openAlgoOrders"), 200, "[]");
    let position = e.store().position("BTCUSDT:short").unwrap();
    assert_eq!(position.leverage, 15.0);
    assert_eq!(
        position.margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"positions","d":[{"symbol":"BTCUSDT","side":"SHORT","size":"2","openValue":"200","updatedTime":1773295701000}]}"#});
    let position = e.store().position("BTCUSDT:short").unwrap();
    assert_eq!(position.leverage, 15.0);
    assert_eq!(position.contracts, 2.0);
}

#[test]
fn paginated_account_read_waits_for_global_ban_and_is_resigned_afterward() {
    let (mut e, actions) = started(false);
    e.handle(
        NOW,
        Event::HttpResponse {
            request: request(&actions, "/balance"),
            response: Response {
                status: 429,
                headers: "Retry-After: 60",
                body: "limited",
            },
        },
    );
    let row = &NORMAL[1..NORMAL.len() - 1];
    let page = format!("[{}]", vec![row; 100].join(","));
    response(&mut e, request(&actions, "/openOrders"), 200, &page);
    let deferred: Vec<_> = e.drain_actions().collect();
    assert!(requests(&deferred).is_empty());
    assert!(
        deferred
            .iter()
            .any(|a| matches!(a, Action::SetTimer {at_ms,..} if *at_ms == NOW + 80_000.0)),
        "deadline extends past a newly discovered ban"
    );
    let old_deadline = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    assert!(
        deferred
            .iter()
            .any(|a| matches!(a,Action::ClearTimer {timer} if *timer == old_deadline))
    );
    let timer = deferred
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 60_000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 60_000.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let requests = requests(&actions);
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("limit=100&page=1"));
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(key, value)| *key == "ACCESS-TIMESTAMP" && value == "1773295760000")
    );
}

#[test]
fn truncated_balance_and_missing_position_quantity_preserve_last_good_state() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    let balance = e.store().balance().clone();
    let positions = e.store().positions().to_vec();
    let actions = next_cycle(&mut e);
    let id = request(&actions, "/balance");
    e.handle(
        NOW,
        Event::HttpStart {
            request: id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW,
        Event::HttpChunk {
            request: id,
            data: br#"[{"asset":"USDT","balance":"0","availableBalance":"0"}"#,
        },
    );
    e.handle(NOW, Event::HttpEnd { request: id });
    assert_eq!(e.store().balance(), &balance);
    response(
        &mut e,
        request(&actions, "allPosition"),
        200,
        r#"[{"symbol":"BTCUSDT","side":"LONG"}]"#,
    );
    assert_eq!(e.store().positions(), positions);
}

#[test]
fn conditional_trigger_is_distinct_from_execution_price_for_amendment() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, "[]", ALGO);
    let order = e.store().order("812345678901234500").unwrap();
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::Conditional);
    assert_eq!(order.price, 68900.0);
    assert_eq!(
        order.extra.as_ref().unwrap().conditional_order_price,
        Some(68800.0)
    );
}

#[test]
fn delayed_first_socket_event_cannot_close_newer_rest_position() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    complete(&mut e, &actions, "[]", "[]");
    assert_eq!(e.store().positions()[0].contracts, 0.02);
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"positions","d":[{"symbol":"BTCUSDT","side":"LONG","size":"0","updatedTime":"1773295699000"}]}"#});
    assert_eq!(e.store().positions()[0].contracts, 0.02);
    assert_eq!(e.store().positions()[0].last_update, Some(1773295700000.0));
}

#[test]
fn socket_opening_stop_has_the_same_kind_and_trigger_as_rest_algo() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"orders","d":[{"id":"812345678901234500","symbol":"BTCUSDT","orderSide":"BUY","type":"STOP","status":"UNTRIGGERED","size":"1","price":"68800","triggerPrice":"68900","updatedTime":"1773295700000"}]}"#});
    let order = e.store().order("812345678901234500").unwrap();
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::Conditional);
    assert_eq!(order.price, 68900.0);
    assert_eq!(
        order.extra.as_ref().unwrap().conditional_order_price,
        Some(68800.0)
    );
}

#[test]
fn bounded_terminal_floor_rejects_inflight_replay_but_fresh_snapshot_admits_old_active_ids() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    complete(&mut e, &actions, "[]", "[]");
    let stale_cycle = next_cycle(&mut e);
    for i in 1..=20_001 {
        let text = format!(
            r#"{{"e":"orders","d":[{{"id":"{i}","symbol":"BTCUSDT","orderSide":"BUY","type":"LIMIT","status":"CANCELED","size":"1","updatedTime":{}}}]}}"#,
            1773295700000u64 + i
        );
        e.handle(
            NOW,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        // Store terminal rows are ordinarily retired by the 2-second snapshot.
        // Bound this synthetic burst independently from the tombstone cache.
        if i % 100 == 0 {
            e.store_mut().set_orders(vec![]);
        }
    }
    complete(&mut e, &stale_cycle, NORMAL, "[]");
    assert!(e.store().order("702345678901234600").is_none());
    let fresh_cycle = next_cycle(&mut e);
    complete(&mut e, &fresh_cycle, NORMAL, "[]");
    assert_eq!(
        e.store().order("702345678901234600").unwrap().status,
        OrderStatus::Open
    );
    e.handle(NOW,Event::WsMessage {socket,text:r#"{"e":"orders","d":[{"id":"1","symbol":"BTCUSDT","orderSide":"BUY","type":"LIMIT","status":"NEW","size":"1","updatedTime":"1773295700001"}]}"#});
    assert!(e.store().order("1").is_none());
}

#[test]
fn stalled_cycle_times_out_and_late_responses_cannot_finish_newer_cycle() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    let expected = e.store().orders().to_vec();
    let stale = next_cycle(&mut e);
    let deadline = stale
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 22_000.0 => Some(*timer),
            _ => None,
        })
        .expect("20-second reconciliation deadline");
    // One complete half cannot replace the existing complete order snapshot.
    response(&mut e, request(&stale, "/openOrders"), 200, "[]");
    e.drain_actions().for_each(drop);
    e.handle(NOW + 22_000.0, Event::TimerFired { timer: deadline });
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(e.store().orders(), expected);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Log {message,..}) if message.contains("timed out"))));
    let retry = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 26_000.0 => Some(*timer),
            _ => None,
        })
        .expect("read-only retry after deadline");
    e.handle(NOW + 26_000.0, Event::TimerFired { timer: retry });
    let fresh: Vec<_> = e.drain_actions().collect();
    assert_eq!(requests(&fresh).len(), 6);
    // Old settings and empty order responses arrive while the new cycle owns
    // pending. Neither may mutate state nor consume the new pending counter.
    response(
        &mut e,
        request(&stale, "symbolConfig"),
        200,
        r#"[{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"1","isolatedLongLeverage":"1","isolatedShortLeverage":"1"}]"#,
    );
    response(&mut e, request(&stale, "/openAlgoOrders"), 200, "[]");
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(e.store().orders(), expected);
    response(
        &mut e,
        request(&fresh, "accountConfig"),
        200,
        r#"{"dualSidePosition":false,"canTrade":true}"#,
    );
    response(
        &mut e,
        request(&fresh, "symbolConfig"),
        200,
        r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","separatedType":"COMBINED","crossLeverage":"20","isolatedLongLeverage":"10","isolatedShortLeverage":"15"}]"#,
    );
    response(
        &mut e,
        request(&fresh, "/balance"),
        200,
        r#"[{"asset":"USDT","balance":"5696.49","availableBalance":"5413.06"}]"#,
    );
    response(&mut e, request(&fresh, "allPosition"), 200, POSITIONS);
    response(&mut e, request(&fresh, "/openOrders"), 200, NORMAL);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms == NOW + 2_000.0)),
        "only all six current responses may complete the cycle"
    );
    response(&mut e, request(&fresh, "/openAlgoOrders"), 200, ALGO);
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if at_ms == NOW + 2_000.0))
    );
}

#[test]
fn reconciliation_deadline_starts_after_scheduled_rate_limit_pause() {
    let (mut e, actions) = started(false);
    let req = request(&actions, "/balance");
    e.handle(
        NOW,
        Event::HttpResponse {
            request: req,
            response: Response {
                status: 429,
                headers: "Retry-After: 60",
                body: "rate limited",
            },
        },
    );
    for path in [
        "accountConfig",
        "symbolConfig",
        "allPosition",
        "/openOrders",
        "/openAlgoOrders",
    ] {
        let body = if path == "accountConfig" {
            r#"{"dualSidePosition":false,"canTrade":true}"#
        } else {
            "[]"
        };
        response(&mut e, request(&actions, path), 200, body);
    }
    let actions: Vec<_> = e.drain_actions().collect();
    let refresh = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 60_000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    // A refresh firing early after its timer was delayed is still queued
    // behind the ban; the cycle deadline includes that legitimate wait.
    e.handle(NOW + 30_000.0, Event::TimerFired { timer: refresh });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(requests(&actions).is_empty());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms == NOW + 80_000.0))
    );
}

#[test]
fn documented_trailing_market_algo_order_is_read_and_canceled_as_algo() {
    let (mut e, actions) = started(false);
    complete(
        &mut e,
        &actions,
        "[]",
        r#"[{"algoId":812345678901234700,"symbol":"BTCUSDT","side":"SELL","orderType":"TRAILING_MARKET","algoStatus":"UNTRIGGERED","quantity":"0.01","triggerPrice":"0","price":"0","createTime":1773295700000,"activatePrice":"70000","callbackRate":"0.05"}]"#,
    );
    let order = e.store().order("812345678901234700").unwrap();
    assert_eq!(
        order.kind,
        safe_cex_engine::model::OrderType::TrailingStopLoss
    );
    e.drain_actions().for_each(drop);
    e.handle(
        NOW,
        Event::Command(safe_cex_engine::Command::CancelOrders {
            command: 9,
            ids: vec!["812345678901234700".into()],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let requests = requests(&actions);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, safe_cex_engine::Method::Delete);
    assert!(
        requests[0]
            .url
            .ends_with("/algoOrder?orderId=812345678901234700")
    );
}

#[test]
fn failed_cancel_cannot_restore_status_over_newer_authoritative_snapshot() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, "[]");
    let periodic = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 2000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        NOW,
        Event::Command(safe_cex_engine::Command::CancelOrders {
            command: 9,
            ids: vec!["702345678901234600".into()],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let cancel = request(&actions, "/batchOrders");
    e.handle(NOW + 2000.0, Event::TimerFired { timer: periodic });
    let fresh: Vec<_> = e.drain_actions().collect();
    let canceling = NORMAL
        .replace("\"NEW\"", "\"CANCELING\"")
        .replace("1773295700000", "1773295705000");
    complete(&mut e, &fresh, &canceling, "[]");
    assert_eq!(
        e.store().order("702345678901234600").unwrap().status,
        OrderStatus::Canceling
    );
    e.handle(
        NOW + 6000.0,
        Event::HttpFailed {
            request: cancel,
            error: "ambiguous cancel timeout",
        },
    );
    let current = e.store().order("702345678901234600").unwrap();
    assert_eq!(
        current.status,
        OrderStatus::Canceling,
        "the old command no longer owns a row replaced by fresh REST"
    );
    assert_eq!(current.last_update, NOW + 5000.0);
}

#[test]
fn healthy_continuation_pages_get_a_full_budget_and_stalled_peer_still_expires() {
    for hold_balance in [false, true] {
        let (mut e, actions) = started(false);
        let original_deadline = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        response(
            &mut e,
            request(&actions, "accountConfig"),
            200,
            r#"{"dualSidePosition":false,"canTrade":true}"#,
        );
        response(&mut e, request(&actions, "symbolConfig"), 200, "[]");
        if !hold_balance {
            response(
                &mut e,
                request(&actions, "/balance"),
                200,
                r#"[{"asset":"USDT","balance":"1","availableBalance":"1"}]"#,
            );
        }
        response(&mut e, request(&actions, "allPosition"), 200, "[]");
        response(&mut e, request(&actions, "/openAlgoOrders"), 200, "[]");
        e.drain_actions().for_each(drop);
        let row = &NORMAL[1..NORMAL.len() - 1];
        let full = format!("[{}]", vec![row; 100].join(","));
        e.handle(
            NOW + 12_000.0,
            Event::HttpResponse {
                request: request(&actions, "/openOrders"),
                response: Response {
                    status: 200,
                    headers: "",
                    body: &full,
                },
            },
        );
        let continuation: Vec<_> = e.drain_actions().collect();
        assert!(
            continuation
                .iter()
                .any(|a| matches!(a,Action::ClearTimer {timer} if *timer == original_deadline))
        );
        let extended = continuation
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 32_000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        let second = request(&continuation, "/openOrders");
        e.handle(
            NOW + 20_000.0,
            Event::TimerFired {
                timer: original_deadline,
            },
        );
        assert!(e.drain_actions().next().is_none());
        assert!(!e.store().loaded().orders);
        e.handle(
            NOW + 24_000.0,
            Event::HttpResponse {
                request: second,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[]",
                },
            },
        );
        let complete: Vec<_> = e.drain_actions().collect();
        assert!(e.store().loaded().orders);
        if hold_balance {
            assert!(
                !complete
                    .iter()
                    .any(|a| matches!(a,Action::ClearTimer {timer} if *timer == extended))
            );
            e.handle(NOW + 32_000.0, Event::TimerFired { timer: extended });
            assert!(e.drain_actions().any(|a| matches!(a,Action::Notice(safe_cex_engine::Notice::Log {message,..}) if message.contains("timed out"))));
        } else {
            assert!(e.store().loaded().positions);
            assert!(
                complete
                    .iter()
                    .any(|a| matches!(a,Action::ClearTimer {timer} if *timer == extended))
            );
        }
    }
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
        Event::TickIntervalChanged {
            interval_ms: 60_000.0,
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
    engine.handle(now + 1.0, Event::TickIntervalChanged { interval_ms: 500.0 });
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 501.0);
    engine.handle(
        now + 2.0,
        Event::TickIntervalChanged {
            interval_ms: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::TickIntervalChanged { interval_ms: 500.0 },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 90_500.0);
    engine.handle(
        now + 90_500.0,
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
    let completed_at = now + 90_501.0;
    engine.handle(
        completed_at,
        Event::TickIntervalChanged {
            interval_ms: f64::INFINITY,
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
        Event::TickIntervalChanged {
            interval_ms: 30_000.0,
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
    let (mut e, initial) = started(false);
    complete(&mut e, &initial, NORMAL, ALGO);
    let engine = &mut e;
    let (now, mut reads) = policy_poll_read(engine, NOW, "allPosition");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("accountConfig") {
                r#"{"dualSidePosition":false,"canTrade":true}"#
            } else if r.url.contains("symbolConfig") {
                "[]"
            } else if r.url.contains("/balance") {
                r#"[{"asset":"USDT","balance":"100","availableBalance":"90"}]"#
            } else {
                "[]"
            };
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
fn failure_poll_floor_survives_fast_policy_and_pause_resume() {
    let (mut e, initial) = started(false);
    complete(&mut e, &initial, NORMAL, ALGO);
    let engine = &mut e;
    let now = NOW;
    engine.drain_actions().for_each(drop);
    engine.handle(now, Event::TickIntervalChanged { interval_ms: 100.0 });
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == now + 100.0 => Some(timer),
            _ => None,
        })
        .expect("fast account poll");
    engine.handle(now + 100.0, Event::TimerFired { timer });
    let output: Vec<_> = engine.drain_actions().collect();
    let reads: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert!(!reads.is_empty());
    // Keep any next periodic timer for a loop that rearms before HTTP completes.
    for r in reads {
        engine.handle(
            now + 100.0,
            Event::HttpFailed {
                request: r.id,
                error: "account unavailable",
            },
        );
    }
    engine.drain_actions().for_each(drop);
    for (offset, interval_ms) in [(1.0, 1.0), (2.0, f64::INFINITY), (3.0, 500.0)] {
        engine.handle(
            now + 100.0 + offset,
            Event::TickIntervalChanged { interval_ms },
        );
        let times: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        if interval_ms.is_infinite() {
            assert!(times.is_empty());
        } else {
            assert_eq!(times, vec![now + 4100.0]);
        }
    }
}
