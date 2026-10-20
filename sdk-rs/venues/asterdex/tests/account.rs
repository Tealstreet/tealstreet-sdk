//! Official Aster V3 account, positionRisk and user-data payload contracts.
//! https://github.com/asterdex/api-docs/blob/master/V3(Recommended)/EN/aster-finance-futures-api-v3.md
use safe_cex_engine::model::{OrderStatus, PositionSide};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId};
use safe_cex_venue_asterdex::{AsterDex, Options};
use serde_json::{Value, json};

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
}
impl H {
    fn new() -> Self {
        let mut h = Self::pending_mode();
        h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
        h
    }
    fn pending_mode() -> Self {
        let mut h = Self {
            engine: Engine::new(AsterDex::new(Options {
                key: Some("documented-key".into()),
                secret: Some("synthetic-secret".into()),
                ..Default::default()
            })),
            now: 1700000000000.0,
            requests: vec![],
            sockets: vec![],
            timers: vec![],
            done: vec![],
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
        h.reply("/openOrders", "[]");
        h.reply("/listenKey", r#"{"listenKey":"private-contract"}"#);
        let socket = h.socket();
        h.event(Event::WsOpened { socket });
        h.reply("/openOrders", "[]");
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
        for action in self.engine.drain_actions() {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, result))
                }
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
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
fn malformed_position_snapshot_preserves_last_good_state() {
    let mut h = H::new();
    h.fire_after(27000.0);
    h.reply("/positionRisk",r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","entryPrice":"0","unRealizedProfit":"0","liquidationPrice":"0","notional":"0","leverage":"25"}, {"symbol":"BROKEN"}]"#);
    assert_eq!(h.engine.store().positions()[0].contracts, 1.0);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 10.0);
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
fn account_identity_read_preserves_alias_and_primary_key_fallback() {
    let mut h = H::new();
    for (command, body, expected) in [
        (20, r#"{"accountAlias":"owner-alias"}"#, "owner-alias"),
        (21, "{}", "documented-key"),
    ] {
        h.event(Event::Command(safe_cex_engine::Command::FetchAccount {
            command,
        }));
        h.reply("/account", body);
        let output: Value = serde_json::from_str(
            h.done
                .iter()
                .find(|(id, _)| *id == command)
                .unwrap()
                .1
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(output, json!({"userId":expected}));
    }
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
