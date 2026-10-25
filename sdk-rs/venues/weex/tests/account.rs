//! Contract V3 official Account_API and private channel payloads; numeric IDs
//! deliberately exceed JavaScript's exact integer range.
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, Level, Notice, RequestId, Response, SocketId, TimerId,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::PreparedActions;
use support::setup::Options;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

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
    let mut e = support::setup::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        private_ws_headers: ws,
        ..Options::default()
    });
    e.start(NOW);
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
    response(&mut e, request(&actions, "exchangeInfo"), 200, INFO);
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
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
    let actions = e.prepared_actions(NOW).collect();
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
        .prepared_actions(NOW)
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 2000.0 => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 2000.0, Event::TimerFired { timer });
    e.prepared_actions(NOW + 2000.0).collect()
}
fn private_socket(actions: &[Action]) -> SocketId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket, headers, ..
            } if !headers.is_empty() => Some(*socket),
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
            .any(|a| matches!(a, Action::WsConnect { headers, .. } if !headers.is_empty()))
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
fn account_and_position_events_during_a_read_cannot_starve_the_balance() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    for text in [r#"{"e":"account","d":{}}"#, r#"{"e":"positions","d":[]}"#] {
        e.handle(NOW, Event::WsMessage { socket, text });
    }
    complete(&mut e, &actions, NORMAL, ALGO);
    assert_eq!(e.store().balance().free, 5413.06);
    assert!(e.store().loaded().balance);
}
#[test]
fn duplicate_config_rows_keep_the_last_valid_native_configuration() {
    let (mut e, actions) = started(false);
    response(
        &mut e,
        request(&actions, "symbolConfig"),
        200,
        r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","separatedType":"COMBINED","crossLeverage":"20","isolatedLongLeverage":"10","isolatedShortLeverage":"15"},{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"30","isolatedLongLeverage":"7","isolatedShortLeverage":"9"}]"#,
    );
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(7.0));
    assert_eq!(setting.sell_leverage, Some(9.0));
    assert_eq!(
        setting.margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    response(
        &mut e,
        request(&actions, "accountConfig"),
        200,
        r#"{"dualSidePosition":false,"canTrade":true}"#,
    );
    for path in ["/balance", "/openOrders", "/openAlgoOrders"] {
        response(&mut e, request(&actions, path), 200, "[]");
    }
    let position = POSITIONS
        .replace("\"marginType\":\"CROSSED\",", "")
        .replace("\"separatedMode\":\"COMBINED\",", "")
        .replace("\"leverage\":\"100\",", "");
    response(&mut e, request(&actions, "allPosition"), 200, &position);
    // The native configuration used by position resolution must agree with
    // Store, including hedge mode and the side-specific isolated leverage.
    assert_eq!(e.store().positions()[0].leverage, 7.0);
    assert_eq!(
        e.store().positions()[0].side,
        safe_cex_engine::model::PositionSide::Long
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
    // The malformed row is skipped; the rest of the snapshot commits.
    assert!(e.store().loaded().orders);
    assert!(e.store().order("1").is_none());
    assert_eq!(e.store().orders().len(), 1);
}
#[test]
fn complete_paging_required_before_order_commit() {
    let (mut e, actions) = started(false);
    let row = &NORMAL[1..NORMAL.len() - 1];
    let page = format!("[{}]", vec![row; 100].join(","));
    response(&mut e, request(&actions, "/openOrders"), 200, &page);
    let pages: Vec<_> = e.prepared_actions(NOW).collect();
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
    // Fills travel with their orders: the book-cadence flush carries both.
    let frame = e.take_market_frame(NOW).unwrap();
    let mut at = 16;
    let mut found = false;
    for _ in 0..u16::from_le_bytes([frame[6], frame[7]]) {
        let kind = u16::from_le_bytes([frame[at], frame[at + 1]]);
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            assert_eq!(
                u16::from_le_bytes(frame[at + 2..at + 4].try_into().unwrap()),
                2
            );
            let data = &frame[at + 8..at + 8 + len];
            let count = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            assert_eq!(count, 2);
            let rows = (4 + 12 * count).next_multiple_of(8);
            assert_eq!(
                f64::from_le_bytes(data[rows..rows + 8].try_into().unwrap()),
                1000.0
            );
            assert_eq!(
                f64::from_le_bytes(data[rows + 8..rows + 16].try_into().unwrap()),
                0.1
            );
            assert_eq!(
                f64::from_le_bytes(data[rows + 16..rows + 24].try_into().unwrap()),
                1100.0
            );
            assert_eq!(
                f64::from_le_bytes(data[rows + 24..rows + 32].try_into().unwrap()),
                0.2
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
        .windows(2)
        .find_map(|pair| match (&pair[0], &pair[1]) {
            (
                Action::WsConnect {
                    socket: private,
                    headers,
                    ..
                },
                Action::SetTimer { timer, at_ms },
            ) if !headers.is_empty() && *private == socket && *at_ms == NOW + 5000.0 => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("private socket watchdog");
    e.handle(NOW + 31_000.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.prepared_actions(NOW + 31_000.0).collect();
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
    assert!(e.prepared_actions(NOW + 32_000.0).any(|action|matches!(action,Action::WsConnect {headers,..} if headers.iter().any(|(key,value)|*key=="ACCESS-TIMESTAMP" && value=="1773295732000"))));
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
    let deferred: Vec<_> = e.prepared_actions(NOW).collect();
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
    let actions: Vec<_> = e.prepared_actions(NOW + 60_000.0).collect();
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
    e.prepared_actions(NOW).for_each(drop);
    e.handle(NOW + 22_000.0, Event::TimerFired { timer: deadline });
    let actions: Vec<_> = e.prepared_actions(NOW + 22_000.0).collect();
    assert_eq!(e.store().orders(), expected);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("timed out"))));
    let retry = actions
        .iter()
        .find_map(|a| match a {
            // The first failure's 4 s backoff, jittered into [3 s, 4 s].
            Action::SetTimer { timer, at_ms }
                if (NOW + 25_000.0..=NOW + 26_000.0).contains(at_ms) =>
            {
                Some(*timer)
            }
            _ => None,
        })
        .expect("read-only retry after deadline");
    e.handle(NOW + 26_000.0, Event::TimerFired { timer: retry });
    let fresh: Vec<_> = e.prepared_actions(NOW + 26_000.0).collect();
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
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms == NOW + 2_000.0)),
        "only all six current responses may complete the cycle"
    );
    response(&mut e, request(&fresh, "/openAlgoOrders"), 200, ALGO);
    assert!(
        e.prepared_actions(NOW)
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
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
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
    let actions: Vec<_> = e.prepared_actions(NOW + 30_000.0).collect();
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
    e.prepared_actions(NOW + 30_000.0).for_each(drop);
    e.handle(
        NOW,
        Event::Command(safe_cex_engine::Command::CancelOrders {
            command: 9,
            ids: vec!["812345678901234700".into()],
        }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
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
        .prepared_actions(NOW)
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
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let cancel = request(&actions, "/batchOrders");
    e.handle(NOW + 2000.0, Event::TimerFired { timer: periodic });
    let fresh: Vec<_> = e.prepared_actions(NOW + 2000.0).collect();
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
        e.prepared_actions(NOW).for_each(drop);
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
        let continuation: Vec<_> = e.prepared_actions(NOW + 12_000.0).collect();
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
        assert!(e.prepared_actions(NOW + 20_000.0).next().is_none());
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
        let complete: Vec<_> = e.prepared_actions(NOW + 24_000.0).collect();
        assert!(e.store().loaded().orders);
        if hold_balance {
            assert!(
                !complete
                    .iter()
                    .any(|a| matches!(a,Action::ClearTimer {timer} if *timer == extended))
            );
            e.handle(NOW + 32_000.0, Event::TimerFired { timer: extended });
            assert!(e.prepared_actions(NOW + 32_000.0).any(|a| matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("timed out"))));
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
    engine.prepared_actions(NOW + 32_000.0).for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .prepared_actions(now)
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
        .prepared_actions(now + 1.0)
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
    let paused: Vec<_> = engine.prepared_actions(now + 2.0).collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(
        !engine
            .prepared_actions(now + 1_501.0)
            .any(|a| matches!(a, Action::Http(_)))
    );
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .prepared_actions(now + 90_000.0)
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
        .prepared_actions(now + 91_500.0)
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
            .prepared_actions(completed_at)
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
    engine.prepared_actions(now).for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .prepared_actions(now)
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
            .prepared_actions(now)
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
    engine.prepared_actions(now).for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let timer = engine
        .prepared_actions(now)
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == now + 1_500.0 => Some(timer),
            _ => None,
        })
        .expect("fast account poll");
    engine.handle(now + 1_500.0, Event::TimerFired { timer });
    let output: Vec<_> = engine.prepared_actions(now + 1_500.0).collect();
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
            now + 1_500.0,
            Event::HttpFailed {
                request: r.id,
                error: "account unavailable",
            },
        );
    }
    engine.prepared_actions(now + 1_500.0).for_each(drop);
    // The jittered failure floor is fixed once and must survive every policy change.
    let mut floor = None;
    for (offset, interval_ms) in [(1.0, 1_000.0), (2.0, f64::INFINITY), (3.0, 1_200.0)] {
        engine.handle(
            now + 1_500.0 + offset,
            Event::PollScaleChanged {
                scale: interval_ms / POLL_MS,
            },
        );
        let times: Vec<_> = engine
            .prepared_actions(now + 1_500.0 + offset)
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        if interval_ms.is_infinite() {
            assert!(times.is_empty());
        } else {
            let at = *times.first().expect("one rearmed poll");
            assert_eq!(times, vec![at]);
            let delay = at - (now + 1_500.0);
            assert!(
                (0.75 * 4_000.0..=4_000.0).contains(&delay),
                "jittered failure backoff {delay}"
            );
            assert_eq!(*floor.get_or_insert(at), at);
        }
    }
}

#[test]
fn live_fill_financials_preserve_optional_zero_and_absolute_fee_without_changing_dedup() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    let text = r#"{"e":"fill","d":[{"id":"f1","orderId":"42","symbol":"BTCUSDT","orderSide":"BUY","fillSize":"0.1","fillValue":"100"},{"id":"f2","orderId":"42","symbol":"BTCUSDT","orderSide":"BUY","fillSize":"0.1","fillValue":"100","fillFee":"-2","realizePnl":"0"},{"id":"f3","orderId":"42","symbol":"BTCUSDT","orderSide":"BUY","fillSize":"0.1","fillValue":"100","fillFee":0,"realizePnl":-3},{"id":"f4","orderId":"42","symbol":"BTCUSDT","orderSide":"BUY","fillSize":"0.1","fillValue":"100","fillFee":{},"realizePnl":"NaN"}]}"#;
    for _ in 0..2 {
        e.handle(NOW, Event::WsMessage { socket, text });
    }
    // A pending fill promotes the market flush to a full frame (MARKETS first).
    let frame = e
        .take_market_frame(NOW)
        .expect("a pending fill promotes the flush to a full frame");
    let payload = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
        .unwrap()
        .1;
    assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 4);
    assert_eq!(payload.len(), 216);
    let symbol_refs: Vec<_> = payload[120..136]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|value| u32::from_le_bytes(*value))
        .collect();
    assert_eq!(symbol_refs.len(), 4);
    assert!(
        symbol_refs
            .iter()
            .all(|value| *value != u32::MAX && *value == symbol_refs[0])
    );
    let value = |at| f64::from_le_bytes(payload[at..at + 8].try_into().unwrap());
    for row in 0..4 {
        assert_eq!([value(56 + row * 16), value(64 + row * 16)], [1000., 0.1]);
    }
    assert!(value(152).is_nan());
    assert_eq!([value(160), value(168)], [0., -3.]);
    assert!(
        value(176).is_nan(),
        "malformed metadata cannot discard a valid fill"
    );
    assert!(value(184).is_nan());
    assert_eq!([value(192), value(200)], [2., 0.]);
    assert!(value(208).is_nan());
    assert!(e.take_market_frame(NOW).is_none());
}
fn logs(actions: &[Action], level: Level) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.level == level => {
                Some(event.message.clone())
            }
            _ => None,
        })
        .collect()
}
#[test]
fn malformed_order_rows_are_skipped_and_their_known_copies_retained() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    assert_eq!(e.store().orders().len(), 4);
    let actions = next_cycle(&mut e);
    let normal = r#"[{"orderId":702345678901234601,"symbol":"BTCUSDT","side":"SELL","type":"LIMIT","status":"NEW","origQty":"0.01","executedQty":"0","price":"71000","updateTime":1773295700000},{"orderId":702345678901234600,"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW","origQty":null,"price":"69500","updateTime":1773295700000}]"#;
    let algo = r#"[{"algoId":812345678901234500,"symbol":"BTCUSDT","side":"SELL","orderType":"STOP","algoStatus":"UNTRIGGERED","quantity":null}]"#;
    complete(&mut e, &actions, normal, algo);
    let ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
    for id in [
        "702345678901234601",
        "702345678901234600",
        "812345678901234500",
        "812345678901234500__tp",
        "812345678901234500__sl",
    ] {
        assert!(ids.contains(&id), "{id} missing from {ids:?}");
    }
    assert_eq!(ids.len(), 5);
    let warnings = logs(&e.prepared_actions(NOW).collect::<Vec<_>>(), Level::Warn);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("malformed order") && w.contains("702345678901234600")),
        "{warnings:?}"
    );
}
#[test]
fn malformed_row_without_identity_preserves_the_whole_order_snapshot() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    let expected = e.store().orders().to_vec();
    let actions = next_cycle(&mut e);
    complete(
        &mut e,
        &actions,
        r#"[{"symbol":"BTCUSDT","side":"BUY"}]"#,
        "[]",
    );
    assert_eq!(e.store().orders(), expected);
}
#[test]
fn algo_row_with_only_plan_type_is_imported() {
    let (mut e, actions) = started(false);
    complete(
        &mut e,
        &actions,
        "[]",
        r#"[{"algoId":812345678901234501,"symbol":"BTCUSDT","side":"SELL","planType":"STOP_LOSS","reduceOnly":true,"algoStatus":"UNTRIGGERED","quantity":"0.01","triggerPrice":"68000","updateTime":1773295700000}]"#,
    );
    let order = e.store().order("812345678901234501").unwrap();
    assert_eq!(order.kind, OrderType::StopLoss);
    assert_eq!(order.price, 68000.0);
}
#[test]
fn malformed_position_row_keeps_the_known_position() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    let before = e.store().position("BTCUSDT").cloned().unwrap();
    let actions = next_cycle(&mut e);
    let positions = POSITIONS.replace(r#""size":"0.02""#, r#""size":null"#);
    response(&mut e, request(&actions, "allPosition"), 200, &positions);
    complete_except_positions(&mut e, &actions);
    assert_eq!(e.store().position("BTCUSDT"), Some(&before));
    assert_eq!(e.store().balance().free, 5413.06);
}
fn complete_except_positions(e: &mut Engine<WeexFutures>, actions: &[Action]) {
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
    response(e, request(actions, "/openOrders"), 200, NORMAL);
    response(e, request(actions, "/openAlgoOrders"), 200, ALGO);
}
#[test]
fn failed_settings_read_backs_off_only_settings() {
    let (mut e, actions) = started(false);
    response(
        &mut e,
        request(&actions, "accountConfig"),
        200,
        r#"{"dualSidePosition":false,"canTrade":true}"#,
    );
    response(
        &mut e,
        request(&actions, "symbolConfig"),
        503,
        "unavailable",
    );
    response(
        &mut e,
        request(&actions, "/balance"),
        200,
        r#"[{"asset":"USDT","balance":"5696.49","availableBalance":"5413.06","frozen":"81.28","unrealizePnl":"-34.553"}]"#,
    );
    response(&mut e, request(&actions, "allPosition"), 200, POSITIONS);
    response(&mut e, request(&actions, "/openOrders"), 200, NORMAL);
    response(&mut e, request(&actions, "/openAlgoOrders"), 200, ALGO);
    // Account reads keep the healthy 2 s cadence; settings sit this cycle out.
    let urls: Vec<_> = requests(&next_cycle(&mut e))
        .into_iter()
        .map(|r| r.url)
        .collect();
    assert_eq!(urls.len(), 4, "{urls:?}");
    assert!(!urls.iter().any(|u| u.contains("Config")));
}
#[test]
fn missing_passphrase_fails_visibly_instead_of_an_empty_account() {
    // Trusted setup refuses it; a provider-supplied identity without a passphrase
    // ref still deactivates rather than publishing an empty account.
    assert!(
        Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }
        .try_build()
        .is_err()
    );
    let mut e =
        safe_cex_engine::test_support::seeded_engine(WeexFutures::new(support::NativeOptions {
            credentials: Some(safe_cex_engine::signing::Identity {
                extra: Default::default(),
                credential: safe_cex_engine::signing::KeyRef("primary.credential".into()),
                signing: safe_cex_engine::signing::KeyRef("primary.signing".into()),
                passphrase: None,
                stable_id: None,
            }),
            ..Default::default()
        }));
    e.start(NOW);
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
    response(&mut e, request(&actions, "exchangeInfo"), 200, INFO);
    // Private startup begins at markets, so its failure may surface before tickers.
    let mut actions: Vec<_> = e.prepared_actions(NOW).collect();
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
    actions.extend(e.prepared_actions(NOW));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    assert!(logs(&actions, Level::Error)[0].contains("passphrase"));
    let loaded = e.store().loaded();
    assert!(!loaded.balance && !loaded.orders && !loaded.positions);
}
#[test]
fn ticker_seed_failure_does_not_block_private_start() {
    let mut e = support::setup::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        ..Options::default()
    });
    e.start(NOW);
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
    response(&mut e, request(&actions, "exchangeInfo"), 200, INFO);
    let seed: Vec<_> = e.prepared_actions(NOW).collect();
    let is_private = |r: &HttpRequest| !r.url.contains("24hr") && !r.url.contains("bookTicker");
    // A successful seed is awaited so account rows map against seeded prices.
    assert!(
        !requests(&seed).iter().any(is_private),
        "{:?}",
        requests(&seed)
    );
    // The first seed failure starts the account; a still-failing seed never gates it.
    response(&mut e, request(&seed, "24hr"), 503, "unavailable");
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
    let private: Vec<_> = requests(&actions).into_iter().filter(is_private).collect();
    assert_eq!(private.len(), 6, "{private:?}");
    response(&mut e, request(&seed, "bookTicker"), 503, "unavailable");
    let retry: Vec<_> = e.prepared_actions(NOW).collect();
    assert!(
        !requests(&retry).iter().any(is_private),
        "account started once"
    );
    complete(&mut e, &actions, NORMAL, ALGO);
    assert!(e.store().loaded().orders && e.store().loaded().balance);
}
#[test]
fn account_starts_after_a_successful_ticker_seed() {
    let (_, actions) = started(false);
    assert!(
        requests(&actions)
            .iter()
            .any(|r| r.url.contains("allPosition"))
    );
}
#[test]
fn malformed_hedged_position_row_keeps_only_its_own_side() {
    let (mut e, actions) = started(false);
    let hedged = r#"[{"symbol":"BTCUSDT","side":"LONG","separatedMode":"SEPARATED","size":"0.02","openValue":"1800","updatedTime":1773295700000},{"symbol":"BTCUSDT","side":"SHORT","separatedMode":"SEPARATED","size":"0.01","openValue":"900","updatedTime":1773295700000}]"#;
    complete(&mut e, &actions, NORMAL, ALGO);
    let actions = next_cycle(&mut e);
    response(&mut e, request(&actions, "allPosition"), 200, hedged);
    complete_except_positions(&mut e, &actions);
    assert!(e.store().position("BTCUSDT:long").is_some());
    assert!(e.store().position("BTCUSDT:short").is_some());
    let actions = next_cycle(&mut e);
    // The short closed; the long row is malformed.
    response(
        &mut e,
        request(&actions, "allPosition"),
        200,
        r#"[{"symbol":"BTCUSDT","side":"LONG","separatedMode":"SEPARATED","size":null}]"#,
    );
    complete_except_positions(&mut e, &actions);
    assert!(e.store().position("BTCUSDT:long").is_some());
    assert!(e.store().position("BTCUSDT:short").is_none());
}
#[test]
fn malformed_row_cannot_resurrect_a_terminal_order() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    e.store_mut()
        .update_order("702345678901234600", |o| o.status = OrderStatus::Canceled);
    let actions = next_cycle(&mut e);
    complete(
        &mut e,
        &actions,
        r#"[{"orderId":702345678901234600,"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW","origQty":null}]"#,
        ALGO,
    );
    assert!(e.store().order("702345678901234600").is_none());
    assert_eq!(e.store().orders().len(), 3);
}
/// Takes this cycle's warnings, then starts the next account cycle.
fn warnings_then_next_cycle(e: &mut Engine<WeexFutures>) -> (Vec<String>, Vec<Action>) {
    let drained: Vec<_> = e.prepared_actions(NOW).collect();
    let timer = drained
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 2000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 2000.0, Event::TimerFired { timer });
    (
        logs(&drained, Level::Warn),
        e.prepared_actions(NOW + 2000.0).collect(),
    )
}
#[test]
fn order_row_without_identity_keeps_stored_orders_while_valid_peers_commit() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    let stored = ["812345678901234500", "812345678901234500__tp"]
        .map(|id| e.store().order(id).unwrap().clone());
    let actions = next_cycle(&mut e);
    let moved = NORMAL.replace(r#""price":"69500""#, r#""price":"69600""#);
    let normal = moved.replace(
        "}]",
        r#"},{"symbol":"BTCUSDT","side":"BUY","price":"12345"}]"#,
    );
    complete(&mut e, &actions, &normal, "[]");
    let store = e.store();
    assert_eq!(store.order("702345678901234600").unwrap().price, 69600.0);
    for row in &stored {
        assert_eq!(store.order(&row.id), Some(row));
    }
    let (warnings, actions) = warnings_then_next_cycle(&mut e);
    assert!(
        warnings.contains(&"WEEX skipped malformed order row without an ID".to_owned()),
        "{warnings:?}"
    );
    assert!(!warnings.iter().any(|w| w.contains("12345")));

    complete(&mut e, &actions, &moved, "[]");
    let ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["702345678901234600"]);
}
#[test]
fn position_row_without_identity_keeps_stored_positions_while_valid_peers_commit() {
    let leg = |side: &str, size: &str| {
        POSITIONS
            .trim_matches(['[', ']'])
            .replace(r#""side":"LONG""#, &format!(r#""side":"{side}""#))
            .replace(
                r#""separatedMode":"COMBINED""#,
                r#""separatedMode":"SEPARATED""#,
            )
            .replace(r#""size":"0.02""#, &format!(r#""size":"{size}""#))
    };
    let (mut e, actions) = started(false);
    let both = format!("[{},{}]", leg("LONG", "0.02"), leg("SHORT", "0.03"));
    response(&mut e, request(&actions, "allPosition"), 200, &both);
    complete_except_positions(&mut e, &actions);
    let short = e.store().position("BTCUSDT:short").cloned().unwrap();
    let (_, actions) = warnings_then_next_cycle(&mut e);

    let partial = format!(r#"[{},{{"side":"LONG","size":"7"}}]"#, leg("LONG", "0.05"));
    response(&mut e, request(&actions, "allPosition"), 200, &partial);
    complete_except_positions(&mut e, &actions);
    assert_eq!(e.store().position("BTCUSDT:long").unwrap().contracts, 0.05);
    assert_eq!(e.store().position("BTCUSDT:short"), Some(&short));
    let (warnings, actions) = warnings_then_next_cycle(&mut e);
    assert!(
        warnings.contains(&"WEEX skipped malformed position row without an ID".to_owned()),
        "{warnings:?}"
    );

    let long = format!("[{}]", leg("LONG", "0.05"));
    response(&mut e, request(&actions, "allPosition"), 200, &long);
    complete_except_positions(&mut e, &actions);
    assert!(e.store().position("BTCUSDT:short").is_none());
    assert_eq!(e.store().positions().len(), 1);
}
#[test]
fn plain_limit_order_events_carry_no_trigger_type() {
    let (mut e, actions) = started(true);
    let socket = private_socket(&actions);
    complete(&mut e, &actions, "[]", "[]");
    // WEEX order events put CONTRACT_PRICE with triggerPrice 0 on plain limit rows.
    e.handle(NOW, Event::WsMessage { socket, text: r#"{"e":"orders","d":[{"id":"702345678901234601","symbol":"BTCUSDT","orderSide":"BUY","type":"LIMIT","status":"NEW","size":"0.02","price":"69500","triggerPrice":"0","triggerPriceType":"CONTRACT_PRICE","updatedTime":"1773295701000"}]}"# });
    let order = e.store().order("702345678901234601").unwrap();
    assert_eq!(order.trigger_type, None);
}
#[test]
fn protective_algo_row_typed_market_keeps_its_working_type() {
    let (mut e, actions) = started(false);
    complete(
        &mut e,
        &actions,
        "[]",
        r#"[{"algoId":812345678901234502,"symbol":"BTCUSDT","side":"SELL","type":"MARKET","planType":"STOP_LOSS","reduceOnly":true,"algoStatus":"UNTRIGGERED","quantity":"0.01","triggerPrice":"68000","workingType":"MARK_PRICE","updateTime":1773295700000}]"#,
    );
    let order = e.store().order("812345678901234502").unwrap();
    assert_eq!(order.kind, OrderType::StopLoss);
    assert_eq!(
        order.trigger_type,
        Some(safe_cex_engine::model::StopTriggerType::MarkPrice)
    );
}
/// Takes this cycle's actions, then starts the next account cycle.
fn drain_then_next_cycle(e: &mut Engine<WeexFutures>) -> (Vec<Action>, Vec<Action>) {
    let drained: Vec<_> = e.prepared_actions(NOW).collect();
    let timer = drained
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 2000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 2000.0, Event::TimerFired { timer });
    (drained, e.prepared_actions(NOW + 2000.0).collect())
}
#[test]
fn startup_position_row_without_identity_is_an_error_logged_once_until_a_clean_snapshot() {
    let message = "WEEX skipped malformed position row without an ID";
    let idless = r#"[{"side":"LONG","size":"7"}]"#;
    let read = |e: &mut Engine<WeexFutures>, actions: &[Action], rows: &str| {
        response(e, request(actions, "allPosition"), 200, rows);
        complete_except_positions(e, actions);
        let (drained, next) = drain_then_next_cycle(e);
        let errors = logs(&drained, Level::Error);
        let warnings = logs(&drained, Level::Warn);
        (
            errors,
            warnings.iter().filter(|l| *l == message).count(),
            next,
        )
    };
    let (mut e, actions) = started(false);
    let (errors, _, actions) = read(&mut e, &actions, idless);
    assert!(e.store().loaded().positions);
    assert_eq!(errors, [message]);
    // The same skip on the next cycle is not reported again.
    let (errors, warnings, actions) = read(&mut e, &actions, idless);
    assert!(errors.is_empty());
    assert_eq!(warnings, 0);

    let (_, _, actions) = read(&mut e, &actions, "[]");
    let (errors, _, _) = read(&mut e, &actions, idless);
    assert_eq!(errors, [message]);
}
#[test]
fn startup_order_row_without_identity_is_an_error_with_no_stored_orders() {
    let (mut e, actions) = started(false);
    let idless = r#"[{"symbol":"BTCUSDT","side":"BUY","price":"12345"}]"#;
    complete(&mut e, &actions, idless, "[]");
    assert!(e.store().loaded().orders);
    let (drained, _) = drain_then_next_cycle(&mut e);
    assert_eq!(
        logs(&drained, Level::Error),
        ["WEEX skipped malformed order row without an ID"]
    );
}

#[test]
fn a_skipped_order_with_an_id_is_an_error_only_when_no_stored_copy_is_kept() {
    let (mut e, actions) = started(false);
    complete(&mut e, &actions, NORMAL, ALGO);
    assert!(e.store().order("702345678901234600").is_some());
    let actions = next_cycle(&mut e);
    let malformed = |id: &str| {
        format!(
            r#"{{"orderId":{id},"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW","origQty":null}}"#
        )
    };
    // 600 keeps its stored copy; 699 has none and drops out.
    let normal = format!(
        "[{},{}]",
        malformed("702345678901234600"),
        malformed("702345678901234699")
    );
    complete(&mut e, &actions, &normal, ALGO);
    assert!(e.store().order("702345678901234600").is_some());
    assert!(e.store().order("702345678901234699").is_none());
    let (drained, actions) = drain_then_next_cycle(&mut e);
    assert_eq!(
        logs(&drained, Level::Error),
        ["WEEX skipped malformed order row 702345678901234699"]
    );
    assert!(
        logs(&drained, Level::Warn)
            .contains(&"WEEX skipped malformed order row 702345678901234600".to_owned())
    );
    assert!(
        drained
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event))
                    if event
                        .message
                        .starts_with("WEEX skipped malformed order row") =>
                    Some(event.code),
                _ => None,
            })
            .all(|code| code == safe_cex_engine::log::code::ROW_SKIPPED)
    );
    complete(&mut e, &actions, &normal, ALGO);
    let (drained, actions) = drain_then_next_cycle(&mut e);
    assert!(logs(&drained, Level::Error).is_empty());
    e.store_mut()
        .update_order("702345678901234600", |o| o.status = OrderStatus::Canceled);
    complete(&mut e, &actions, &normal, ALGO);
    let (drained, actions) = drain_then_next_cycle(&mut e);
    assert_eq!(
        logs(&drained, Level::Error),
        ["WEEX skipped malformed order row 702345678901234600"]
    );
    complete(&mut e, &actions, &normal, ALGO);
    let (drained, _) = drain_then_next_cycle(&mut e);
    assert!(logs(&drained, Level::Error).is_empty());
}
