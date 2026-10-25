//! Official Aster V3 account, positionRisk and user-data payload contracts.
//! https://github.com/asterdex/api-docs/blob/master/V3(Recommended)/EN/aster-finance-futures-api-v3.md
use safe_cex_engine::model::{OrderStatus, PositionSide};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId};
mod support;
use safe_cex_venue_asterdex::AsterDex;
use serde_json::{Value, json};
use support::Options;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 3_000.0;

const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000"}]}]}"#;
const BALANCE: &str = r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"}"#;
const POSITION: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"1","entryPrice":"100","unRealizedProfit":"5","liquidationPrice":"20","notional":"100","leverage":"10","marginType":"cross"}]"#;
struct H {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<(SocketId, String)>,
    timers: Vec<(TimerId, f64)>,
    done: Vec<(u32, Result<String, String>)>,
    logs: Vec<String>,
    errors: Vec<String>,
    closed: Vec<SocketId>,
}
impl H {
    fn new() -> Self {
        let mut h = Self::pending_mode();
        h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
        h
    }
    fn pending_mode() -> Self {
        Self::pending_mode_with_orders("[]")
    }
    fn pending_mode_with_orders(open_orders: &str) -> Self {
        let mut h = Self {
            engine: support::engine(Options {
                key: Some("documented-key".into()),
                secret: Some("synthetic-secret".into()),
                ..Default::default()
            }),
            now: 1700000000000.0,
            requests: vec![],
            sockets: vec![],
            timers: vec![],
            done: vec![],
            logs: vec![],
            errors: vec![],
            closed: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        h.reply("/time", r#"{"serverTime":1700000000000}"#);
        h.reply("/exchangeInfo", MARKETS);
        h.reply("/leverageBracket", "[]");
        h.reply("/ticker/24hr",r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#);
        h.reply("/openInterest", "[]");
        h.reply("/account", BALANCE);
        h.reply("/positionRisk", POSITION);
        h.reply("/openOrders", open_orders);
        h.reply("/listenKey", r#"{"listenKey":"private-contract"}"#);
        let socket = h.socket();
        h.event(Event::WsOpened { socket });
        h.reply("/openOrders", open_orders);
        h
    }
    fn socket(&self) -> SocketId {
        self.sockets
            .iter()
            .find(|(_, u)| u.contains("/ws/private-contract"))
            .unwrap()
            .0
    }
    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now).into_iter() {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())))
                }
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                    level, message, ..
                })) => {
                    if level == safe_cex_engine::Level::Error {
                        self.errors.push(message.clone());
                    }
                    self.logs.push(message)
                }
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn take(&mut self, suffix: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.split('?').next().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("missing{suffix}: {:?}", self.requests));
        self.requests.remove(i)
    }
    fn respond(&mut self, r: HttpRequest, body: &str) {
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    fn reply(&mut self, path: &str, body: &str) {
        let r = self.take(path);
        self.respond(r, body);
    }
    fn ws(&mut self, value: Value) {
        let text = value.to_string();
        self.event(Event::WsMessage {
            socket: self.socket(),
            text: &text,
        });
    }
    fn fire_after(&mut self, delay: f64) {
        self.now += delay;
        self.ws(json!({"id":42,"result":[]}));
        let due = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= self.now)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for timer in due {
            self.timers.retain(|(id, _)| *id != timer);
            self.event(Event::TimerFired { timer });
        }
    }
}
fn account(time: Value, amount: &str, side: &str) -> Value {
    json!({"e":"ACCOUNT_UPDATE","E":time,"a":{"B":[{"a":"USDT","wb":"1010","cw":"900"}],"P":[{"s":"BTCUSDT","pa":amount,"ep":"101","up":"7","ps":side,"cr":"3"}]}})
}
fn order(time: f64, status: &str, filled: &str, trade: u64) -> Value {
    json!({"e":"ORDER_TRADE_UPDATE","E":time,"o":{"i":123,"s":"BTCUSDT","S":"BUY","ot":"LIMIT","X":status,"p":"100","q":"2","z":filled,"l":"0.5","L":"102","ap":"101","t":trade,"f":"GTC","c":"native-only"}})
}

#[test]
fn startup_uses_v4_account_and_preserves_zero_position_settings() {
    let mut h = H::new();
    assert_eq!(h.engine.store().balance().used, 120.0);
    assert_eq!(h.engine.store().positions()[0].side, PositionSide::Long);
    h.fire_after(27000.0);
    h.reply("/positionRisk",r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","entryPrice":"0","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"25","marginType":"isolated"}]"#);
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 25.0);
}
#[test]
fn account_event_requires_numeric_strictly_increasing_time_atomically() {
    let mut h = H::new();
    h.ws(account(json!(10), "1", "BOTH"));
    let balance = h.engine.store().balance().clone();
    for bad in [
        account(json!("11"), "2", "BOTH"),
        account(json!(10), "2", "BOTH"),
        account(json!(9), "2", "BOTH"),
        account(json!(10.5), "2", "BOTH"),
    ] {
        h.ws(bad);
        assert_eq!(*h.engine.store().balance(), balance);
        assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    }
    let mut malformed = account(json!(11), "2", "BOTH");
    malformed["a"]["B"][0]["cw"] = json!(true);
    h.ws(malformed);
    h.ws(account(json!(11), "2", "BOTH"));
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}
#[test]
fn one_way_flip_updates_side_and_hedge_close_preserves_other_leg() {
    let mut h = H::new();
    h.ws(account(json!(10), "-2", "BOTH"));
    assert_eq!(h.engine.store().positions()[0].side, PositionSide::Short);
    h.ws(account(json!(11), "1", "LONG"));
    h.ws(account(json!(12), "-1", "SHORT"));
    h.ws(account(json!(13), "0", "LONG"));
    assert!(
        h.engine
            .store()
            .positions()
            .iter()
            .any(|p| p.id == "BTCUSDT:short")
    );
    assert!(
        !h.engine
            .store()
            .positions()
            .iter()
            .any(|p| p.id == "BTCUSDT:long")
    );
}
#[test]
fn stale_rest_cannot_overwrite_newer_positions_or_balance() {
    let mut h = H::new();
    h.fire_after(27000.0);
    let balance = h.take("/account");
    let positions = h.take("/positionRisk");
    h.ws(account(json!(10), "-2", "BOTH"));
    h.respond(balance, BALANCE);
    h.respond(positions, POSITION);
    assert_eq!(h.engine.store().balance().total, 1010.0);
    assert_eq!(h.engine.store().positions()[0].side, PositionSide::Short);
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
}
#[test]
fn authoritative_balance_between_events_does_not_double_count_usdt() {
    let mut h = H::new();
    h.ws(account(json!(10), "1", "BOTH"));
    h.fire_after(27000.0);
    h.reply("/account",r#"{"totalWalletBalance":"1200","availableBalance":"1000","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"7"}"#);
    h.ws(account(json!(11), "1", "BOTH"));
    assert_eq!(h.engine.store().balance().total, 1010.0);
    assert_eq!(h.engine.store().balance().free, 900.0);
}
#[test]
fn non_usdt_and_known_multiasset_preserve_root_until_authoritative_read() {
    let mut h = H::new();
    let mut frame = account(json!(10), "1", "BOTH");
    frame["a"]["B"] = json!([{"a":"BTC","wb":"0.1","cw":"0.08"}]);
    h.ws(frame);
    assert_eq!(h.engine.store().balance().total, 1000.0);
    h.ws(account(json!(11), "1", "BOTH"));
    assert_eq!(h.engine.store().balance().total, 1000.0);
    h.fire_after(1000.0);
    h.reply("/account",r#"{"totalWalletBalance":"5000","availableBalance":"4000","totalInitialMargin":"500","totalOpenOrderInitialMargin":"100","totalUnrealizedProfit":"15"}"#);
    assert_eq!(h.engine.store().balance().total, 5000.0);
}
#[test]
fn order_terminal_and_cumulative_guards_survive_stale_ws_and_rest() {
    let mut h = H::new();
    h.ws(order(10.0, "PARTIALLY_FILLED", "0.5", 1));
    assert_eq!(h.engine.store().order("123").unwrap().filled, 0.5);
    h.ws(order(9.0, "NEW", "0", 2));
    assert_eq!(h.engine.store().order("123").unwrap().filled, 0.5);
    h.ws(order(11.0, "FILLED", "2", 3));
    h.ws(order(12.0, "CANCELED", "2", 4));
    assert_eq!(
        h.engine.store().order("123").unwrap().status,
        OrderStatus::Closed
    );
}
#[test]
fn unidentifiable_position_row_keeps_stored_positions_while_valid_rows_commit() {
    let mut h = H::new();
    h.fire_after(27000.0);
    h.reply("/positionRisk",r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","entryPrice":"0","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"25"}, {"positionAmt":"1"}]"#);
    assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 25.0);
}
#[test]
fn legacy_execution_and_balance_remain_supported() {
    let mut h = H::new();
    h.ws(json!({"e":"executionReport","E":10,"i":123,"s":"BTCUSDT","S":"BUY","o":"LIMIT","X":"NEW","p":"100","q":"2","z":"0","f":"GTC"}));
    assert!(h.engine.store().order("123").is_some());
    h.ws(json!({"e":"outboundAccountPosition","B":[{"a":"USDT","f":"950","l":"50"}]}));
    assert_eq!(h.engine.store().balance().free, 950.0);
    assert_eq!(h.engine.store().balance().used, 50.0);
}

fn fill_rows(h: &mut H) -> Vec<(f64, f64)> {
    let Some(frame) = h.engine.take_frame(h.now) else {
        return vec![];
    };
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        let payload = at + 8;
        if kind == 11 {
            let n = u32::from_le_bytes(frame[payload..payload + 4].try_into().unwrap()) as usize;
            let start = (payload + 4 + 12 * n + 7) & !7;
            return (0..n)
                .map(|i| {
                    let a = start + i * 16;
                    (
                        f64::from_le_bytes(frame[a..a + 8].try_into().unwrap()),
                        f64::from_le_bytes(frame[a + 8..a + 16].try_into().unwrap()),
                    )
                })
                .collect();
        }
        at = payload + len;
    }
    vec![]
}
#[test]
fn fills_are_per_execution_with_last_price_and_duplicate_suppression() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    h.ws(order(10.0, "PARTIALLY_FILLED", "0.5", 1));
    h.ws(order(10.0, "PARTIALLY_FILLED", "0.5", 1));
    h.ws(order(11.0, "PARTIALLY_FILLED", "1", 2));
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5), (102.0, 0.5)]);
}

#[test]
fn unique_native_executions_survive_reordered_state_and_invalid_price_retries() {
    for last_status in ["PARTIALLY_FILLED", "FILLED"] {
        let mut h = H::new();
        let _ = fill_rows(&mut h);
        for (id, status) in [(901, "NEW"), (902, "CANCELED")] {
            let mut non_execution = order(10.0, status, "0.5", 101);
            non_execution["o"]["i"] = json!(id);
            h.ws(non_execution);
        }
        assert!(fill_rows(&mut h).is_empty());
        let latest = order(12.0, last_status, "1", 102);
        let earlier = order(11.0, "PARTIALLY_FILLED", "0.5", 101);
        h.ws(latest.clone());
        let mut bad_price = earlier.clone();
        bad_price["o"]["L"] = json!("0");
        bad_price["o"]["ap"] = json!("0");
        h.ws(bad_price);
        h.ws(earlier.clone());
        h.ws(latest);
        h.ws(earlier);
        assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5), (102.0, 0.5)]);
        let final_order = h.engine.store().order("123").unwrap();
        assert_eq!(final_order.filled, 1.0);
        assert_eq!(final_order.last_update, 12.0);
        assert_eq!(
            final_order.status,
            if last_status == "FILLED" {
                OrderStatus::Closed
            } else {
                OrderStatus::Open
            }
        );
    }
}

#[test]
fn delayed_native_executions_survive_rest_or_cancel_ack_progress_without_reopening() {
    for canceled in [false, true] {
        let mut h = H::new();
        let _ = fill_rows(&mut h);
        h.ws(order(10.0, "NEW", "0", 0));
        if canceled {
            h.event(Event::Command(safe_cex_engine::Command::CancelOrders {
                command: 10,
                ids: vec!["123".into()],
            }));
            h.reply(
                "/order",
                r#"{"orderId":123,"status":"CANCELED","executedQty":"1"}"#,
            );
        } else {
            h.fire_after(10_000.0);
            h.reply("/openOrders", r#"[{"symbol":"BTCUSDT","orderId":123,"price":"100","origQty":"2","executedQty":"1","status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","time":20}]"#);
        }
        for event in [
            order(11.0, "PARTIALLY_FILLED", "0.5", 101),
            order(12.0, "PARTIALLY_FILLED", "1", 102),
        ] {
            h.ws(event.clone());
            h.ws(event);
        }
        assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5), (102.0, 0.5)]);
        let final_order = h.engine.store().order("123").unwrap();
        assert_eq!(final_order.filled, 1.0);
        assert_eq!(
            final_order.status,
            if canceled {
                OrderStatus::Canceled
            } else {
                OrderStatus::Open
            }
        );
    }
}

#[test]
fn successful_mode_write_fences_startup_read_and_controls_subsequent_order_policy() {
    for hedged in [true, false] {
        let mut h = H::pending_mode();
        let stale_mode = h.take("/positionSide/dual");
        h.engine.store_mut().set_hedged(!hedged);
        h.event(Event::Command(safe_cex_engine::Command::SetPositionMode {
            command: 20,
            hedged,
            symbol: None,
        }));
        h.reply("/positionSide/dual", "{}");
        assert_eq!(h.engine.store().hedged(), hedged);
        h.respond(
            stale_mode,
            if hedged {
                r#"{"dualSidePosition":false}"#
            } else {
                r#"{"dualSidePosition":true}"#
            },
        );
        assert_eq!(h.engine.store().hedged(), hedged);
        h.event(Event::Command(safe_cex_engine::Command::PlaceOrders {
            command: 21,
            orders: vec![
                serde_json::from_value(json!({
                    "symbol":"BTCUSDT", "type":"market", "side":"buy", "amount":1,
                    "reduceOnly":true
                }))
                .unwrap(),
            ],
        }));
        let order = h.take("/order");
        assert_eq!(order.url.contains("positionSide=SHORT"), hedged);
        assert_eq!(order.url.contains("reduceOnly=true"), !hedged);
    }
}
#[test]
fn fill_without_execution_id_uses_cumulative_progress() {
    let mut h = H::new();
    let mut a = order(10.0, "PARTIALLY_FILLED", "0.5", 1);
    a["o"].as_object_mut().unwrap().remove("t");
    h.ws(a.clone());
    h.ws(a);
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
}

fn cumulative_adoption_event(time: f64, cumulative: &str, last: &str) -> Value {
    let mut event = order(time, "PARTIALLY_FILLED", cumulative, 1);
    event["o"].as_object_mut().unwrap().remove("t");
    event["o"]["l"] = json!(last);
    event
}

#[test]
fn cumulative_adoption_caps_last_and_preserves_full_blind_gap_observations() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    for (time, cumulative, last) in [
        (10.0, "0.25", "1"),
        (11.0, "1.25", "0.125"),
        (12.0, "1.25", "1"),
        (13.0, "0.5", "1"),
        (14.0, "1.5", "1"),
    ] {
        h.ws(cumulative_adoption_event(time, cumulative, last));
    }
    assert_eq!(
        fill_rows(&mut h),
        vec![(102.0, 0.25), (102.0, 0.125), (102.0, 0.25)]
    );
}

#[test]
fn cumulative_adoption_initial_rest_seed_suppresses_fallback_but_not_native_ids() {
    let mut h = H::pending_mode_with_orders(
        r#"[{"symbol":"BTCUSDT","orderId":123,"price":"100","origQty":"2","executedQty":"0.75","status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","time":1}]"#,
    );
    h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
    let _ = fill_rows(&mut h);
    h.ws(cumulative_adoption_event(10.0, "0.75", "0.5"));
    assert!(fill_rows(&mut h).is_empty());
    h.ws(order(11.0, "PARTIALLY_FILLED", "0.75", 7));
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    h.ws(cumulative_adoption_event(12.0, "1", "0.5"));
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.25)]);
}

#[test]
fn cumulative_adoption_later_rest_read_does_not_replace_live_observation() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    h.ws(cumulative_adoption_event(10.0, "0.5", "0.5"));
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    h.fire_after(10_000.0);
    h.reply(
        "/openOrders",
        r#"[{"symbol":"BTCUSDT","orderId":123,"price":"100","origQty":"2","executedQty":"1.5","status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","time":20}]"#,
    );
    h.ws(cumulative_adoption_event(11.0, "1", "1"));
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
}

#[test]
fn cumulative_adoption_direct_ids_and_duplicates_advance_only_observed_progress() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    h.ws(order(10.0, "PARTIALLY_FILLED", "0.25", 7));
    h.ws(order(11.0, "PARTIALLY_FILLED", "1.25", 7));
    h.ws(cumulative_adoption_event(12.0, "1.5", "1"));
    let mut late = order(9.0, "PARTIALLY_FILLED", "0.5", 8);
    late["o"]["l"] = json!("1");
    h.ws(late);
    h.ws(cumulative_adoption_event(13.0, "1.75", "1"));
    assert_eq!(
        fill_rows(&mut h),
        vec![(102.0, 0.5), (102.0, 0.25), (102.0, 1.0), (102.0, 0.25)]
    );
}

#[test]
fn cumulative_adoption_unusable_cumulative_does_not_gate_authoritative_direct_ids() {
    for cumulative in ["invalid", "-1"] {
        let mut h = H::new();
        let _ = fill_rows(&mut h);
        h.ws(cumulative_adoption_event(10.0, cumulative, "0.5"));
        assert!(fill_rows(&mut h).is_empty());
        h.ws(order(11.0, "PARTIALLY_FILLED", cumulative, 7));
        assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
        h.ws(cumulative_adoption_event(12.0, "0.5", "0.5"));
        assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    }
}

#[test]
fn cumulative_adoption_missing_native_last_or_cumulative_cannot_synthesize_a_fill() {
    for missing in ["l", "z"] {
        let mut h = H::new();
        let _ = fill_rows(&mut h);
        let mut event = cumulative_adoption_event(10.0, "0.5", "0.5");
        event["o"].as_object_mut().unwrap().remove(missing);
        h.ws(event);
        assert!(fill_rows(&mut h).is_empty());
        h.ws(cumulative_adoption_event(11.0, "0.5", "0.5"));
        assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    }
}

#[test]
fn cumulative_adoption_invalid_selected_price_can_retry_without_consuming_progress() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    let mut fallback = cumulative_adoption_event(10.0, "0.5", "0.5");
    fallback["o"]["L"] = json!("-1");
    h.ws(fallback);
    assert_eq!(fill_rows(&mut h), vec![(101.0, 0.5)]);
    let mut invalid = cumulative_adoption_event(11.0, "1", "0.5");
    invalid["o"]["L"] = json!("0");
    invalid["o"]["ap"] = json!("0");
    h.ws(invalid);
    assert!(fill_rows(&mut h).is_empty());
    h.ws(cumulative_adoption_event(11.0, "1", "0.5"));
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
}

#[test]
fn cumulative_adoption_legacy_flat_execution_keeps_native_last_requirement() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    for (time, cumulative, last) in [
        (10.0, "0.75", "0.5"),
        (11.0, "0.75", "0.5"),
        (12.0, "1.5", "0.25"),
    ] {
        h.ws(json!({"e":"executionReport","E":time,"i":900,"s":"BTCUSDT","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","p":"100","q":"2","z":cumulative,"l":last,"L":"102","f":"GTC"}));
    }
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5), (102.0, 0.25)]);
}

#[test]
fn cumulative_adoption_unknown_order_kind_does_not_hide_native_executions() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    for (time, cumulative) in [(10.0, "0.5"), (11.0, "0.5"), (12.0, "1")] {
        let mut event = cumulative_adoption_event(time, cumulative, "0.5");
        event["o"]["i"] = json!(900);
        event["o"]["ot"] = json!("UNKNOWN");
        h.ws(event);
    }
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5), (102.0, 0.5)]);
    assert!(h.engine.store().order("900").is_none());
}

#[test]
fn cumulative_adoption_negative_observations_do_not_evict_a_valid_fill_anchor() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    let anchor = cumulative_adoption_event(10.0, "0.5", "0.5");
    h.ws(anchor.clone());
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    // Exercise the real native ledger bound, not a timing/load measurement.
    for id in 100_000..110_000 {
        let mut invalid = cumulative_adoption_event(11.0, "-1", "0.5");
        invalid["o"]["i"] = json!(id);
        invalid["o"]["ot"] = json!("UNKNOWN");
        h.ws(invalid);
    }
    assert!(fill_rows(&mut h).is_empty());
    h.ws(anchor);
    assert_eq!(
        fill_rows(&mut h),
        vec![],
        "invalid cumulative rows must not make an already delivered fill eligible again"
    );
}

#[test]
fn cumulative_adoption_accepted_quiet_observations_still_apply_native_retention() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    let anchor = cumulative_adoption_event(10.0, "0.5", "0.5");
    h.ws(anchor.clone());
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    for id in 100_000..110_000 {
        let mut quiet = cumulative_adoption_event(11.0, "0", "0.5");
        quiet["o"]["i"] = json!(id);
        quiet["o"]["ot"] = json!("UNKNOWN");
        h.ws(quiet);
    }
    assert!(fill_rows(&mut h).is_empty());
    h.ws(anchor);
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
}

#[test]
fn native_fill_delivered_after_local_cancel_ack_is_not_lost() {
    let mut h = H::new();
    h.ws(order(10.0, "NEW", "0", 0));
    h.event(Event::Command(safe_cex_engine::Command::CancelOrders {
        command: 10,
        ids: vec!["123".into()],
    }));
    h.reply("/order", r#"{"orderId":123,"status":"CANCELED"}"#);
    h.ws(order(11.0, "FILLED", "2", 3));
    assert_eq!(
        h.engine.store().order("123").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
}

#[test]
fn unknown_cancel_fences_snapshots_and_open_events_without_hiding_late_fills() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    h.fire_after(75_000.0);
    let stale_orders = h.take("/openOrders");
    assert!(h.engine.store().order("9001").is_none());
    h.event(Event::Command(
        safe_cex_engine::Command::CancelOrdersWithSymbols {
            command: 30,
            orders: vec![safe_cex_engine::model::CancelOrderRef {
                id: "9001".into(),
                symbol: "BTCUSDT".into(),
            }],
        },
    ));
    h.reply("/order", r#"{"orderId":9001,"status":"CANCELED"}"#);
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 30 && result.is_ok())
    );
    assert!(h.engine.store().order("9001").is_none());
    let snapshot = r#"[{"symbol":"BTCUSDT","orderId":9001,"price":"100","origQty":"2","executedQty":"0","status":"NEW","side":"BUY","type":"LIMIT","time":10}]"#;
    h.respond(stale_orders, snapshot);
    assert!(h.engine.store().order("9001").is_none());
    let mut delayed_new = order(10.0, "NEW", "0", 0);
    delayed_new["o"]["i"] = json!(9001);
    h.ws(delayed_new);
    assert!(h.engine.store().order("9001").is_none());
    h.fire_after(1_000.0);
    h.reply("/openOrders", snapshot);
    assert!(h.engine.store().order("9001").is_none());
    let mut delayed_fill = order(11.0, "FILLED", "2", 501);
    delayed_fill["o"]["i"] = json!(9001);
    h.ws(delayed_fill.clone());
    h.ws(delayed_fill);
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    let final_order = h.engine.store().order("9001").unwrap();
    assert_eq!(final_order.status, OrderStatus::Closed);
    assert_eq!(final_order.filled, 2.0);
}

#[test]
fn negative_used_balance_is_rejected_before_advancing_event_time() {
    let mut h = H::new();
    let mut invalid = account(json!(10), "2", "BOTH");
    invalid["a"]["B"][0]["cw"] = json!("1011");
    h.ws(invalid);
    assert_eq!(h.engine.store().balance().total, 1000.0);
    h.ws(account(json!(10), "2", "BOTH"));
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    let currencies = h.engine.store().currency_balances().unwrap();
    let usdt = currencies.iter().find(|c| c.currency == "USDT").unwrap();
    assert_eq!(usdt.balance.free, 900.0);
    assert_eq!(usdt.balance.used, 110.0);
    h.ws(json!({"e":"outboundAccountPosition","B":[{"a":"USDT","f":"999","l":"-1"}]}));
    assert_eq!(h.engine.store().balance().free, 900.0);
}
#[test]
fn rest_nonzero_collateral_flags_multiasset_but_zero_buckets_do_not() {
    for (amount, expected) in [("1", 5000.0), ("0", 1010.0)] {
        let mut h = H::new();
        h.fire_after(27000.0);
        let mut response: Value = serde_json::from_str(BALANCE).unwrap();
        response["totalWalletBalance"] = json!("5000");
        response["assets"] =
            json!([{"asset":"BTC","walletBalance":amount},{"asset":"USDT","walletBalance":"1000"}]);
        h.reply("/account", &response.to_string());
        h.ws(account(json!(10), "1", "BOTH"));
        assert_eq!(h.engine.store().balance().total, expected);
    }
}

#[test]
fn account_identity_read_preserves_alias_and_never_falls_back_to_the_key() {
    let mut h = H::new();
    let result = |h: &mut H, command, body| {
        h.event(Event::Command(safe_cex_engine::Command::FetchAccount {
            command,
        }));
        h.reply("/account", body);
        h.done
            .iter()
            .find(|(id, _)| *id == command)
            .unwrap()
            .1
            .clone()
    };
    let output: Value =
        serde_json::from_str(&result(&mut h, 20, r#"{"accountAlias":"owner-alias"}"#).unwrap())
            .unwrap();
    assert_eq!(output, json!({"userId":"owner-alias"}));
    // V1 without an alias or stableId fails typed; tests/admission.rs covers the fallbacks.
    let error = result(&mut h, 21, "{}").unwrap_err();
    assert!(!error.contains("documented-key"), "{error}");
}
#[test]
fn leverage_metadata_command_completes_from_native_settings() {
    let mut h = H::new();
    h.event(Event::Command(safe_cex_engine::Command::FetchLeverage {
        command: 22,
        symbol: "BTCUSDT".into(),
    }));
    let output: Value = serde_json::from_str(
        h.done
            .iter()
            .find(|(id, _)| *id == 22)
            .unwrap()
            .1
            .as_ref()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(output, json!({"buyLeverage":10,"sellLeverage":10}));
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    safe_cex_engine::test_support::prepared_actions(engine, now)
        .into_iter()
        .for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now)
        .into_iter()
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
    let fast: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 1.0)
        .into_iter()
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
    let paused: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 2.0)
        .into_iter()
        .collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(
        !safe_cex_engine::test_support::prepared_actions(engine, now + 1_501.0)
            .into_iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 90_000.0)
        .into_iter()
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
    let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now + 91_500.0)
        .into_iter()
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
        !safe_cex_engine::test_support::prepared_actions(engine, completed_at)
            .into_iter()
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
    safe_cex_engine::test_support::prepared_actions(engine, now)
        .into_iter()
        .for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = safe_cex_engine::test_support::prepared_actions(engine, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let mut h = H::new();
    let now = h.now;
    let engine = &mut h.engine;
    let (now, mut reads) = policy_poll_read(engine, now, "/account");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("/account") {
                BALANCE
            } else if r.url.contains("positionRisk") {
                POSITION
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
        reads = safe_cex_engine::test_support::prepared_actions(engine, now)
            .into_iter()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}
fn place(h: &mut H, command: u32, order: Value) {
    h.event(Event::Command(safe_cex_engine::Command::PlaceOrders {
        command,
        orders: vec![serde_json::from_value(order).unwrap()],
    }));
}
fn result(h: &H, command: u32) -> &Result<String, String> {
    &h.done.iter().find(|(c, _)| *c == command).unwrap().1
}
#[test]
fn failed_position_mode_read_retries_and_holds_placement_until_loaded() {
    let mut h = H::pending_mode();
    h.reply("/positionSide/dual", r#"{"code":-1000,"msg":"busy"}"#);
    assert!(
        h.logs
            .iter()
            .any(|l| l.contains("position mode read failed"))
    );
    let reduce =
        json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":1,"reduceOnly":true});
    place(&mut h, 30, reduce.clone());
    assert!(!h.requests.iter().any(|r| r.url.contains("/order?")));
    assert!(
        result(&h, 30)
            .as_ref()
            .unwrap_err()
            .contains("position mode has not loaded")
    );
    h.fire_after(1_000.0);
    h.reply("/positionSide/dual", r#"{"dualSidePosition":true}"#);
    assert!(h.engine.store().hedged());
    place(&mut h, 31, reduce);
    assert!(h.take("/order").url.contains("positionSide=SHORT"));
}
#[test]
fn position_mode_read_retries_are_jittered_below_their_nominal_backoff() {
    let mut h = H::pending_mode();
    let mut delays = vec![];
    for nominal in [1_000.0, 2_000.0, 4_000.0, 8_000.0] {
        // A managed read clears its own deadline timer, so select new timers by id.
        let before: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
        h.reply("/positionSide/dual", r#"{"code":-1000,"msg":"busy"}"#);
        let added: Vec<_> = h
            .timers
            .iter()
            .filter(|(id, _)| !before.contains(id))
            .copied()
            .collect();
        assert_eq!(added.len(), 1, "one mode retry timer: {added:?}");
        let (timer, at) = added[0];
        let delay = at - h.now;
        assert!(
            (0.75 * nominal..=nominal).contains(&delay),
            "{delay} outside jittered {nominal}"
        );
        delays.push((delay, nominal));
        h.timers.retain(|(id, _)| *id != timer);
        h.now = at;
        h.event(Event::TimerFired { timer });
    }
    assert!(delays.iter().any(|(delay, nominal)| delay < nominal));
}
#[test]
fn unmapped_liquidation_order_type_still_delivers_its_fill() {
    let mut h = H::new();
    let _ = fill_rows(&mut h);
    let mut liquidation = order(10.0, "FILLED", "2", 77);
    liquidation["o"]["i"] = json!(555);
    liquidation["o"]["ot"] = json!("LIQUIDATION");
    liquidation["o"]["o"] = json!("LIQUIDATION");
    h.ws(liquidation);
    assert_eq!(fill_rows(&mut h), vec![(102.0, 0.5)]);
    assert!(h.engine.store().order("555").is_none());
}
#[test]
fn malformed_open_order_row_is_skipped_and_keeps_known_copy() {
    let mut h = H::new();
    h.ws(order(10.0, "NEW", "0", 1));
    assert!(h.engine.store().order("123").is_some());
    h.fire_after(10_000.0);
    h.reply(
        "/openOrders",
        r#"[{"orderId":200,"symbol":"BTCUSDT","type":"LIMIT","side":"SELL","price":"110","origQty":"1","executedQty":"0","status":"NEW","timeInForce":"GTC","reduceOnly":false,"closePosition":false},
            {"orderId":123,"symbol":"BTCUSDT","type":"LIMIT","side":"BUY","price":"100","origQty":"bad","executedQty":"0","status":"NEW","reduceOnly":false,"closePosition":false}]"#,
    );
    assert!(h.engine.store().order("200").is_some());
    assert_eq!(
        h.engine.store().order("123").unwrap().status,
        OrderStatus::Open
    );
    assert!(
        h.logs
            .iter()
            .any(|l| l == "Aster skipped malformed open order row 123")
    );
}
#[test]
fn malformed_position_row_is_skipped_and_keeps_known_symbol() {
    let mut h = H::new();
    h.fire_after(27000.0);
    h.reply("/positionRisk",r#"[{"symbol":"ETHUSDT","positionSide":"BOTH","positionAmt":"2","entryPrice":"10","unRealizedProfit":"0","liquidationPrice":"0","notional":"20","leverage":"5"},
        {"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"3","entryPrice":"bad","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"25"}]"#);
    let store = h.engine.store();
    assert_eq!(store.position("ETHUSDT").unwrap().contracts, 2.0);
    assert_eq!(store.position("BTCUSDT").unwrap().contracts, 1.0);
}
#[test]
fn flat_position_row_with_a_bad_field_still_closes_the_position() {
    let mut h = H::new();
    h.fire_after(27000.0);
    h.reply("/positionRisk",r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","entryPrice":"bad","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"25"}]"#);
    assert!(h.engine.store().position("BTCUSDT").is_none());
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 25.0);
}
#[test]
fn hedge_malformed_leg_keeps_only_that_leg_without_duplicating_or_reviving_others() {
    let leg = |side: &str, amount: &str, entry: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","positionSide":"{side}","positionAmt":"{amount}","entryPrice":"{entry}","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"5"}}"#
        )
    };
    for (long_amount, expected_long) in [("3", Some(3.0)), ("0", None)] {
        let mut h = H::pending_mode();
        h.reply("/positionSide/dual", r#"{"dualSidePosition":true}"#);
        h.ws(account(json!(10), "1", "LONG"));
        h.ws(account(json!(11), "-2", "SHORT"));
        h.fire_after(27000.0);
        h.reply(
            "/positionRisk",
            &format!(
                "[{},{}]",
                leg("LONG", long_amount, "100"),
                leg("SHORT", "-5", "bad")
            ),
        );
        let positions = h.engine.store().positions();
        assert_eq!(
            positions.iter().filter(|p| p.id == "BTCUSDT:long").count(),
            usize::from(expected_long.is_some())
        );
        assert_eq!(
            positions
                .iter()
                .find(|p| p.id == "BTCUSDT:long")
                .map(|p| p.contracts),
            expected_long
        );
        assert_eq!(
            h.engine
                .store()
                .position("BTCUSDT:short")
                .unwrap()
                .contracts,
            2.0
        );
        assert!(h.engine.store().position("BTCUSDT").is_none());
    }
}
#[test]
fn malformed_row_cannot_reopen_a_locally_canceled_order() {
    let mut h = H::new();
    h.ws(order(10.0, "NEW", "0", 1));
    h.event(Event::Command(safe_cex_engine::Command::CancelOrders {
        command: 50,
        ids: vec!["123".into()],
    }));
    h.reply(
        "/order",
        r#"{"orderId":123,"status":"CANCELED","executedQty":"0"}"#,
    );
    h.fire_after(10_000.0);
    h.reply(
        "/openOrders",
        r#"[{"orderId":123,"symbol":"BTCUSDT","type":"LIMIT","side":"BUY","price":"100","origQty":"bad","executedQty":"0","status":"NEW","reduceOnly":false,"closePosition":false}]"#,
    );
    assert_eq!(
        h.engine.store().order("123").unwrap().status,
        OrderStatus::Canceled
    );
}
#[test]
fn hedge_amend_right_after_placement_uses_the_acknowledged_leg() {
    let mut h = H::pending_mode();
    h.reply("/positionSide/dual", r#"{"dualSidePosition":true}"#);
    place(
        &mut h,
        60,
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}),
    );
    let placed = h.take("/order");
    assert!(placed.url.contains("positionSide=LONG"));
    h.respond(placed, r#"{"orderId":123}"#);
    // The private NEW row carries no positionSide here.
    h.ws(order(10.0, "NEW", "0", 1));
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 61,
        id: "123".into(),
        price: Some(99.0),
        amount: None,
    }));
    let cancel = h.take("/order");
    h.respond(
        cancel,
        r#"{"orderId":123,"status":"CANCELED","executedQty":"0"}"#,
    );
    assert!(h.take("/order").url.contains("positionSide=LONG"));
}
#[test]
fn hedge_amend_keeps_the_order_rows_position_side() {
    let mut h = H::pending_mode();
    h.reply("/positionSide/dual", r#"{"dualSidePosition":true}"#);
    // A hedge close-long limit: SELL on the LONG leg, reported with reduceOnly=false.
    let mut close_long = order(10.0, "NEW", "0", 1);
    close_long["o"]["S"] = json!("SELL");
    close_long["o"]["ps"] = json!("LONG");
    h.ws(close_long);
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 40,
        id: "123".into(),
        price: Some(105.0),
        amount: None,
    }));
    let cancel = h.take("/order");
    h.respond(
        cancel,
        r#"{"orderId":123,"status":"CANCELED","executedQty":"0"}"#,
    );
    let replacement = h.take("/order");
    assert!(replacement.url.contains("side=SELL"));
    assert!(replacement.url.contains("positionSide=LONG"));

    let mut unknown = order(11.0, "NEW", "0", 2);
    unknown["o"]["i"] = json!(124);
    h.ws(unknown);
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 41,
        id: "124".into(),
        price: Some(105.0),
        amount: None,
    }));
    assert!(
        result(&h, 41)
            .as_ref()
            .unwrap_err()
            .contains("no known hedge position side")
    );
}

#[test]
fn rest_ahead_of_stream_does_not_consume_no_id_execution() {
    let mut h = H::new();
    h.event(Event::WsOpened { socket: h.socket() });
    h.reply("/openOrders", r#"[{"symbol":"BTCUSDT","orderId":123,"price":"100","origQty":"2","executedQty":"0.5","status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","time":10}]"#);
    assert_eq!(h.engine.store().order("123").unwrap().filled, 0.5);
    let mut event = order(11.0, "PARTIALLY_FILLED", "0.5", 1);
    event["o"].as_object_mut().unwrap().remove("t");
    h.ws(event.clone());
    h.ws(event);
    assert_eq!(fill_rows(&mut h), [(102.0, 0.5)]);
    assert_eq!(h.engine.store().order("123").unwrap().filled, 0.5);
}

#[test]
fn retired_keepalive_failure_cannot_close_the_replacement_private_socket() {
    let mut h = H::new();
    let old = h.socket();
    let (timer, at) = *h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 1_800_000.0)
        .unwrap();
    h.now = at;
    h.event(Event::TimerFired { timer });
    if h.requests.iter().any(|r| r.url.ends_with("/time")) {
        h.reply("/time", &format!(r#"{{"serverTime":{}}}"#, h.now));
    }
    let keepalive = h.take("/listenKey");
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    let (timer, at) = *h.timers.last().unwrap();
    assert!((750.0..=1000.0).contains(&(at - h.now)));
    h.now = at;
    h.event(Event::TimerFired { timer });
    h.reply("/listenKey", r#"{"listenKey":"replacement"}"#);
    let replacement = h.sockets.last().unwrap().0;
    assert_ne!(replacement, old);
    h.event(Event::WsOpened {
        socket: replacement,
    });
    h.respond(
        keepalive,
        r#"{"code":-1125,"msg":"old listen key expired"}"#,
    );
    assert!(!h.closed.contains(&replacement));
    assert!(!h.requests.iter().any(|r| r.url.ends_with("/listenKey")));
}

#[test]
fn late_listen_key_response_after_connect_deadline_does_not_replace_the_new_session() {
    let mut h = H::new();
    h.event(Event::WsClosed {
        socket: h.socket(),
        code: 1006,
    });
    let (timer, at) = *h.timers.last().unwrap();
    h.now = at;
    let armed: Vec<_> = h.timers.iter().map(|(id, _)| *id).collect();
    h.event(Event::TimerFired { timer });
    let old = h.take("/listenKey");
    // The reconnect arms the supervisor's connect watchdog first; the
    // create's engine session bound (20 s) is due later.
    let (timer, _) = *h
        .timers
        .iter()
        .filter(|(id, _)| !armed.contains(id))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    h.now += 10_000.0;
    h.event(Event::TimerFired { timer });
    let count = h.sockets.len();
    h.respond(old, r#"{"listenKey":"late-retired"}"#);
    assert_eq!(h.sockets.len(), count);
    let (timer, at) = *h.timers.last().unwrap();
    assert!((1500.0..=2000.0).contains(&(at - h.now)));
    h.now = at;
    h.event(Event::TimerFired { timer });
    assert_eq!(h.take("/listenKey").method, safe_cex_engine::Method::Post);
}
#[test]
fn one_malformed_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let leg = |side: &str, amount: &str, entry: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","positionSide":"{side}","positionAmt":"{amount}","entryPrice":"{entry}","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"5"}}"#
        )
    };
    let order = |id: u64, price: &str, qty: &str| {
        format!(
            r#"{{"orderId":{id},"symbol":"BTCUSDT","type":"LIMIT","side":"SELL","price":"{price}","origQty":"{qty}","executedQty":"0","status":"NEW","timeInForce":"GTC","reduceOnly":false,"closePosition":false}}"#
        )
    };
    let read = |h: &mut H, positions: &[String], orders: &[String]| {
        h.fire_after(75_000.0);
        h.reply("/positionRisk", &format!("[{}]", positions.join(",")));
        h.reply("/openOrders", &format!("[{}]", orders.join(",")));
    };
    let mut h = H::pending_mode();
    h.reply("/positionSide/dual", r#"{"dualSidePosition":true}"#);
    read(
        &mut h,
        &[leg("LONG", "1", "100"), leg("SHORT", "-2", "100")],
        &[order(123, "100", "1"), order(200, "110", "1")],
    );
    assert_eq!(h.engine.store().positions().len(), 2);
    assert_eq!(h.engine.store().orders().len(), 2);
    h.logs.clear();

    // The identity-less row protects every stored row but no longer withholds valid peers.
    read(
        &mut h,
        &[
            leg("LONG", "3", "100"),
            leg("SHORT", "-5", "bad"),
            r#"{"positionAmt":"1"}"#.into(),
        ],
        &[order(123, "100", "bad"), order(200, "111", "1")],
    );
    let store = h.engine.store();
    assert_eq!(store.position("BTCUSDT:long").unwrap().contracts, 3.0);
    assert_eq!(store.position("BTCUSDT:short").unwrap().contracts, 2.0);
    assert_eq!(store.order("200").unwrap().price, 111.0);
    assert_eq!(store.order("123").unwrap().amount, 1.0);
    for message in [
        "Aster skipped malformed position row BTCUSDT:short",
        "Aster skipped malformed position row without a symbol",
        "Aster skipped malformed open order row 123",
    ] {
        assert!(h.logs.iter().any(|l| l == message), "{message}");
    }
    assert!(!h.logs.iter().any(|l| l.contains("bad")));

    read(
        &mut h,
        &[leg("LONG", "3", "100")],
        &[order(200, "111", "1")],
    );
    let store = h.engine.store();
    assert!(store.position("BTCUSDT:short").is_none());
    assert_eq!(store.positions().len(), 1);
    assert!(store.order("123").is_none());
    assert_eq!(store.orders().len(), 1);
}
#[test]
fn rejected_position_row_with_no_stored_copy_is_reported_as_an_error() {
    let read = |h: &mut H, positions: &str| {
        h.fire_after(75_000.0);
        h.reply("/positionRisk", positions);
        h.reply("/openOrders", "[]");
    };
    let mut h = H::new();
    // With a stored copy to keep, a skipped row is only a warning.
    read(
        &mut h,
        r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"bad"}]"#,
    );
    assert_eq!(h.engine.store().positions().len(), 1);
    assert!(h.errors.is_empty());
    read(&mut h, "[]");
    assert!(h.engine.store().positions().is_empty());

    // With nothing stored, the snapshot commits as flat, so each skip is an error.
    read(
        &mut h,
        r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"bad"},{"positionAmt":"1"}]"#,
    );
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(
        h.errors,
        [
            "Aster skipped malformed position row BTCUSDT",
            "Aster skipped malformed position row without a symbol",
        ]
    );
}
#[test]
fn a_repeated_position_skip_logs_once_until_the_row_maps_again() {
    let read = |h: &mut H, positions: &str| {
        h.fire_after(75_000.0);
        // The periodic clock re-sync gates signed reads.
        if h.requests.iter().any(|r| r.url.contains("/time")) {
            let time = format!(r#"{{"serverTime":{}}}"#, h.now);
            h.reply("/time", &time);
        }
        h.reply("/positionRisk", positions);
        h.reply("/openOrders", "[]");
    };
    let bad = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"bad"}]"#;
    let mut h = H::new();
    read(&mut h, "[]");
    read(&mut h, bad);
    assert_eq!(h.errors, ["Aster skipped malformed position row BTCUSDT"]);
    // The same skip on the next poll is not reported again.
    read(&mut h, bad);
    assert_eq!(h.errors.len(), 1);

    // Once the row maps, a recurrence is reported again.
    read(&mut h, POSITION);
    assert_eq!(h.engine.store().positions().len(), 1);
    read(&mut h, "[]");
    read(&mut h, bad);
    assert_eq!(h.errors.len(), 2);
}

#[test]
fn explicit_flat_event_without_entry_or_cumulative_pnl_closes_only_reported_leg() {
    let mut h = H::new();
    h.engine.store_mut().set_positions(vec![]);
    h.ws(account(json!(10), "1", "LONG"));
    h.ws(account(json!(11), "-1", "SHORT"));
    let mut close = account(json!(12), "0", "LONG");
    close["a"]["P"][0].as_object_mut().unwrap().remove("ep");
    close["a"]["P"][0].as_object_mut().unwrap().remove("cr");
    h.ws(close);
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].side, PositionSide::Short);
    assert_eq!(h.engine.store().balance().total, 1010.0);
}

#[test]
fn missing_or_malformed_entry_never_flattens_unproven_exposure() {
    for amount in [Some("1"), Some("bad"), None, Some("0")] {
        let mut h = H::new();
        let before = h.engine.store().positions().to_vec();
        let mut event = account(json!(10), amount.unwrap_or("0"), "BOTH");
        if amount.is_none() {
            event["a"]["P"][0].as_object_mut().unwrap().remove("pa");
        }
        if amount == Some("0") {
            event["a"]["P"][0]["ep"] = json!("bad");
        } else {
            event["a"]["P"][0].as_object_mut().unwrap().remove("ep");
        }
        h.ws(event);
        assert_eq!(h.engine.store().positions(), before);
        // Rejected payloads cannot consume the event timestamp.
        h.ws(account(json!(10), "2", "BOTH"));
        assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    }
}

#[test]
fn present_null_or_non_string_flat_entry_is_rejected() {
    for entry in [Value::Null, json!(0), json!(true), json!(""), json!("bad")] {
        let mut h = H::new();
        let before = h.engine.store().positions().to_vec();
        let mut event = account(json!(10), "0", "BOTH");
        event["a"]["P"][0]["ep"] = entry;
        h.ws(event);
        assert_eq!(h.engine.store().positions(), before);
        h.ws(account(json!(10), "2", "BOTH"));
        assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    }
}

// Drive the native poll, including its protocol-clock gate, without changing
// the mapping/publication ordering under test.
fn reporter_begin_positions(h: &mut H) {
    h.fire_after(75_000.0);
    if h.requests.iter().any(|r| r.url.contains("/time")) {
        h.reply("/time", &format!(r#"{{"serverTime":{}}}"#, h.now));
    }
}
fn reporter_positions(h: &mut H, rows: &str) {
    reporter_begin_positions(h);
    h.reply("/positionRisk", rows);
    h.reply("/openOrders", "[]");
}
#[test]
fn reporter_zero_quantity_does_not_rearm_a_key_in_a_rejected_read() {
    let mut h = H::new();
    let bad = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"bad"},{}]"#;
    let message = "Aster skipped malformed position row BTCUSDT";
    reporter_positions(&mut h, bad);
    assert_eq!(h.logs.iter().filter(|m| *m == message).count(), 1);
    let zero = POSITION.replace(r#""positionAmt":"1""#, r#""positionAmt":"0""#);
    reporter_positions(&mut h, &format!("[{},{{}}]", &zero[1..zero.len() - 1]));
    reporter_positions(&mut h, bad);
    assert_eq!(h.logs.iter().filter(|m| *m == message).count(), 1);
    assert_eq!(
        h.logs
            .iter()
            .filter(|m| m.contains("without a symbol"))
            .count(),
        1
    );
    reporter_positions(&mut h, "[]");
    reporter_positions(&mut h, bad);
    assert_eq!(h.logs.iter().filter(|m| *m == message).count(), 2);
    assert_eq!(
        h.logs
            .iter()
            .filter(|m| m.contains("without a symbol"))
            .count(),
        2
    );
}
#[test]
fn reporter_mapped_key_rearms_before_stale_read_rejection_but_anonymous_key_does_not() {
    let mut h = H::new();
    let bad = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"bad"},{}]"#;
    reporter_positions(&mut h, bad);
    reporter_begin_positions(&mut h);
    h.ws(account(json!(10), "3", "BOTH"));
    h.reply(
        "/positionRisk",
        &format!("[{},{{}}]", &POSITION[1..POSITION.len() - 1]),
    );
    h.reply("/openOrders", "[]");
    assert_eq!(h.engine.store().positions()[0].contracts, 3.0);
    reporter_positions(&mut h, bad);
    assert_eq!(
        h.logs
            .iter()
            .filter(|m| m.as_str() == "Aster skipped malformed position row BTCUSDT")
            .count(),
        2
    );
    assert_eq!(
        h.logs
            .iter()
            .filter(|m| m.contains("without a symbol"))
            .count(),
        1
    );
}
#[test]
fn reporter_once_does_not_upgrade_a_remembered_warning_after_its_stored_copy_disappears() {
    let mut h = H::new();
    let bad = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"bad"}]"#;
    reporter_positions(&mut h, bad);
    assert!(h.errors.is_empty());
    // Independent store mutation is not a clean native read and must not rearm.
    h.engine.store_mut().set_positions(vec![]);
    reporter_positions(&mut h, bad);
    assert!(h.errors.is_empty());
    reporter_positions(
        &mut h,
        r#"[{"symbol":"BTCUSDT","positionSide":"SHORT","positionAmt":"bad"}]"#,
    );
    assert_eq!(
        h.errors,
        ["Aster skipped malformed position row BTCUSDT:short"]
    );
}
