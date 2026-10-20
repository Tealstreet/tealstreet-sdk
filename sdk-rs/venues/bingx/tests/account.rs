//! Scripted private payload oracles: BingX-API/api-ai-skills swap-account,
//! swap-ws-account and swap-trade. No credentials or live writes are used.
use safe_cex_engine::model::{MarginType, OrderStatus, PositionMode, PositionSide};
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, Method, Notice, RequestId, Response, SocketId, TimerId,
    wire,
};
use safe_cex_venue_bingx::{Bingx, Options};

const KEY: &str = "/openApi/user/auth/userDataStream";
const MODE: &str = "/openApi/swap/v1/positionSide/dual";
const BALANCE: &str = "/openApi/swap/v3/user/balance";
const POSITIONS: &str = "/openApi/swap/v2/user/positions";
const ORDERS: &str = "/openApi/swap/v2/trade/openOrders";
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const DAILY: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;
const BALANCES: &str = r#"{"code":0,"data":[{"asset":"USDC","balance":"50","availableMargin":"40","usedMargin":"10","unrealizedProfit":"2"},{"asset":"USDT","balance":"1000","availableMargin":"950","usedMargin":"50","unrealizedProfit":"10"}]}"#;
const POSITION: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"BOTH","avgPrice":"60000","unrealizedProfit":"10","leverage":20,"isolated":false,"liquidationPrice":50000,"initialMargin":"300"}]}"#;
const OPEN: &str = r#"{"code":0,"data":{"orders":[{"orderId":8389765519993909000,"symbol":"BTC-USDT","type":"LIMIT","side":"BUY","status":"NEW","origQty":"0.2","executedQty":"0","price":"59000","positionSide":"BOTH","timeInForce":"GTC","clientOrderId":"client"}]}}"#;
const ORDER_ID: &str = "8389765519993909000";

struct Harness {
    engine: Engine<Bingx>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<(SocketId, String)>,
    timers: Vec<(TimerId, f64)>,
    sent: Vec<(SocketId, String)>,
    closed: Vec<SocketId>,
    logs: Vec<String>,
}
impl Harness {
    fn new() -> Self {
        let mut h = Self {
            engine: Engine::new(Bingx::new(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                ..Options::default()
            })),
            now: 1000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            timers: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            logs: Vec::new(),
        };
        h.engine.start(h.now);
        h.collect();
        h.respond("/quote/contracts", 200, MARKETS);
        h.respond("/quote/ticker", 200, DAILY);
        h.respond("/quote/premiumIndex", 200, PREMIUM);
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::Log { message, .. }) => self.logs.push(message),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| {
                panic!(
                    "missing {path}; have {:?}",
                    self.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
                )
            });
        self.requests.remove(i)
    }
    fn reply(&mut self, id: RequestId, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request: id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn respond(&mut self, path: &str, status: u16, body: &str) {
        let r = self.take(path);
        self.reply(r.id, status, body);
    }
    fn private(&self) -> SocketId {
        self.sockets
            .iter()
            .rev()
            .find(|(_, u)| u.contains("listenKey="))
            .unwrap()
            .0
    }
    fn message(&mut self, text: &str) {
        self.now += 1.0;
        self.event(Event::WsMessage {
            socket: self.private(),
            text,
        });
    }
    fn fire_at(&mut self, at: f64) {
        let i = self
            .timers
            .iter()
            .position(|(_, t)| *t == at)
            .unwrap_or_else(|| panic!("no timer at {at}; have {:?}", self.timers));
        let (timer, time) = self.timers.remove(i);
        self.now = time;
        self.event(Event::TimerFired { timer });
    }
    fn poll_late(&mut self, now: f64) {
        let mut scheduled = 3000.0;
        for offset in [6000.0, 4000.0, 2000.0, 0.0] {
            let i = self
                .timers
                .iter()
                .position(|(_, t)| *t == scheduled)
                .expect("account poll");
            let (timer, _) = self.timers.remove(i);
            self.now = now - offset;
            self.event(Event::TimerFired { timer });
            scheduled = self.now + 2000.0;
        }
    }
    fn ready() -> Self {
        Self::ready_with_orders(OPEN)
    }
    fn ready_with_orders(orders: &str) -> Self {
        let mut h = Self::new();
        h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
        h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
        h.respond(POSITIONS, 200, POSITION);
        h.respond(BALANCE, 200, BALANCES);
        h.respond(ORDERS, 200, orders);
        h.event(Event::WsOpened {
            socket: h.private(),
        });
        h.respond(POSITIONS, 200, POSITION);
        h.respond(BALANCE, 200, BALANCES);
        h.respond(ORDERS, 200, orders);
        h.engine.take_frame(h.now);
        h
    }
    fn fills(&mut self) -> Vec<(f64, f64)> {
        let Some(frame) = self.engine.take_market_frame(self.now) else {
            return Vec::new();
        };
        let Some((_, payload)) =
            wire::sections(frame).find(|(kind, _)| *kind == wire::section::FILLS)
        else {
            return Vec::new();
        };
        let n = u32::from_le_bytes(payload[0..4].try_into().unwrap()) as usize;
        let start = (4 + 12 * n + 7) & !7;
        (0..n)
            .map(|i| {
                let off = start + i * 16;
                (
                    f64::from_le_bytes(payload[off..off + 8].try_into().unwrap()),
                    f64::from_le_bytes(payload[off + 8..off + 16].try_into().unwrap()),
                )
            })
            .collect()
    }
}

#[test]
fn private_startup_preserves_big_ids_unsigned_key_and_primary_balance() {
    let mut h = Harness::new();
    let key = h.take(KEY);
    assert_eq!(key.method, Method::Post);
    assert!(!key.url.contains("signature"));
    assert_eq!(key.body, None);
    assert!(
        key.headers
            .iter()
            .any(|(k, v)| *k == "X-BX-APIKEY" && v == "key")
    );
    let mode = h.take(MODE);
    assert!(mode.url.contains("signature="));
    assert!(mode.url.contains("timestamp=1000"));
    let h = Harness::ready();
    let store = h.engine.store();
    assert!(store.loaded().balance && store.loaded().positions && store.loaded().orders);
    assert_eq!(store.balance().total, 1000.0);
    assert_eq!(store.balance().free, 950.0);
    assert_eq!(store.orders()[0].id, ORDER_ID);
    let p = &store.positions()[0];
    assert_eq!(p.id, "BTCUSDT");
    assert_eq!(p.contracts, 0.1);
    assert_eq!(p.leverage, 20.0);
    assert_eq!(p.margin_type, Some(MarginType::Cross));
    assert_eq!(p.position_mode, Some(PositionMode::Oneway));
    assert_eq!(p.margin_size, Some(300.0));
}

#[test]
fn position_mode_accepts_documented_bool_and_ts_string_variants() {
    let mut h = Harness::new();
    h.respond(
        MODE,
        200,
        r#"{"code":0,"data":{"dualSidePosition":"true"}}"#,
    );
    h.respond(POSITIONS,200,r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"SHORT","avgPrice":"60000","unrealizedProfit":"10","leverage":"20","isolated":true,"liquidationPrice":"70000","initialMargin":"300"}]}"#);
    assert!(h.engine.store().hedged());
    let p = &h.engine.store().positions()[0];
    assert_eq!(p.id, "BTCUSDT:short");
    assert_eq!(p.side, PositionSide::Short);
}

#[test]
fn every_partial_execution_is_emitted_once_including_repeated_terminal_events() {
    let mut h = Harness::ready();
    let partial = r#"{"e":"ORDER_TRADE_UPDATE","E":1001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","z":"0.05","l":"0.05","L":"59001","ap":"59001"}}"#;
    h.message(partial);
    assert_eq!(h.fills(), [(59001.0, 0.05)]);
    h.message(partial);
    assert!(h.fills().is_empty());
    let terminal = r#"{"e":"TRADE_UPDATE","E":1002,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"FILLED","q":"0.2","z":"0.2","l":"0.15","L":"59002","ap":"59001.75"}}"#;
    h.message(terminal);
    let fills = h.fills();
    assert_eq!(fills.len(), 1);
    assert!((fills[0].1 - 0.15).abs() < 1e-12);
    assert_eq!(fills[0].0, 59002.0);
    h.message(terminal);
    assert!(h.fills().is_empty());
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Closed
    );
    h.message(partial);
    assert!(h.fills().is_empty());
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Closed
    );
}

#[test]
fn expired_key_closes_stale_socket_and_recovery_is_single_owner() {
    let mut h = Harness::ready();
    let old = h.private();
    h.message(r#"{"e":"listenKeyExpired"}"#);
    assert_eq!(h.closed, [old]);
    let key = h.take(KEY);
    h.event(Event::WsClosed {
        socket: old,
        code: 1000,
    });
    assert!(!h.requests.iter().any(|r| r.url.contains(KEY)));
    h.reply(key.id, 200, r#"{"code":0,"data":{"listenKey":"key-two"}}"#);
    let new = h.private();
    assert_ne!(old, new);
    h.event(Event::WsMessage {
        socket: old,
        text: r#"{"e":"listenKeyExpired"}"#,
    });
    assert_eq!(h.private(), new);
    assert!(!h.requests.iter().any(|r| r.url.contains(KEY)));
}

#[test]
fn transient_key_failure_retries_and_keepalive_expiry_rebuilds() {
    let mut h = Harness::new();
    h.respond(KEY, 500, r#"{"code":100500,"msg":"busy"}"#);
    h.fire_at(31000.0);
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.event(Event::WsOpened {
        socket: h.private(),
    });
    h.fire_at(1_831_000.0);
    let put = h.take(KEY);
    assert_eq!(put.method, Method::Put);
    assert!(put.url.contains("listenKey=key-one"));
    assert!(!put.url.contains("signature"));
    h.reply(
        put.id,
        404,
        r#"{"code":100001,"msg":"listen key does not exist"}"#,
    );
    assert!(
        h.requests
            .iter()
            .any(|r| r.method == Method::Post && r.url.contains(KEY))
    );
}

#[test]
fn stale_keepalive_response_cannot_install_timer_for_replaced_key() {
    let mut h = Harness::ready();
    h.fire_at(1_801_000.0);
    let put = h.take(KEY);
    h.message(r#"{"e":"listenKeyExpired"}"#);
    let key = h.take(KEY);
    h.reply(put.id, 200, r#"{"code":0}"#);
    assert!(!h.timers.iter().any(|(_, at)| *at == h.now + 1_800_000.0));
    h.reply(key.id, 200, r#"{"listenKey":"replacement"}"#);
    assert!(
        h.sockets
            .last()
            .unwrap()
            .1
            .ends_with("listenKey=replacement")
    );
}

#[test]
fn private_watchdog_recovers_a_silent_connection_and_replies_to_ping() {
    let mut h = Harness::ready();
    h.message("Ping");
    assert!(h.sent.iter().any(|(_, s)| s == "Pong"));
    h.fire_at(16000.0);
    h.fire_at(31000.0);
    h.fire_at(46000.0); // Last receive was at 1001.
    assert!(h.requests.iter().all(|r| !r.url.contains(KEY)));
    h.fire_at(61000.0);
    assert!(h.requests.iter().any(|r| r.url.contains(KEY)));
    assert_eq!(h.closed.len(), 1);
}

#[test]
fn stale_rest_positions_cannot_restore_a_closed_position() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    let positions = h.take(POSITIONS);
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","pa":"0","ep":"60000","up":"0","ps":"BOTH","mt":"cross"}]}}"#);
    assert!(h.engine.store().positions().is_empty());
    h.reply(positions.id, 200, POSITION);
    assert!(h.engine.store().positions().is_empty());
}

#[test]
fn stale_rest_orders_cannot_resurrect_a_terminal_order() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    let orders = h.take(ORDERS);
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"CANCELED","q":"0.2","z":"0"}}"#);
    h.reply(orders.id, 200, OPEN);
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Canceled
    );
    h.respond(ORDERS, 200, OPEN);
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn http_200_error_envelopes_and_malformed_snapshots_preserve_state() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    h.respond(
        POSITIONS,
        200,
        r#"{"data":[],"code":100004,"msg":"denied"}"#,
    );
    h.respond(
        ORDERS,
        200,
        r#"{"code":100004,"msg":"denied","data":{"orders":[]}}"#,
    );
    h.respond(BALANCE, 200, r#"{"code":100004,"msg":"denied","data":[]}"#);
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(h.engine.store().balance().total, 1000.0);
    h.fire_at(28000.0);
    h.respond(
        POSITIONS,
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT"}]}"#,
    );
    assert_eq!(h.engine.store().positions().len(), 1);
}

#[test]
fn position_packets_insert_new_rows_and_config_updates_preserve_side_settings() {
    let mut h = Harness::ready();
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","pa":"-0.3","ep":"62000","up":"7","ps":"BOTH","mt":"isolated"}]}}"#);
    let p = &h.engine.store().positions()[0];
    assert_eq!(p.side, PositionSide::Short);
    assert_eq!(p.contracts, 0.3);
    assert_eq!(p.margin_type, Some(MarginType::Isolated));
    h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":25,"S":15,"mt":"cross"}}"#);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 25.0);
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(25.0)
    );
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().sell_leverage,
        Some(15.0)
    );
    assert_eq!(h.engine.store().positions()[0].leverage, 15.0);
    h.engine.store_mut().set_hedged(true);
    h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":30}}"#);
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(30.0)
    );
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().sell_leverage,
        Some(15.0)
    );
    assert_eq!(h.engine.store().positions()[0].leverage, 15.0);
}

#[test]
fn hedge_position_snapshots_preserve_both_leverage_rails_in_either_order() {
    for rows in [
        r#"[{"symbol":"BTC-USDT","positionAmt":"-0.1","positionSide":"SHORT","avgPrice":"60000","unrealizedProfit":"0","leverage":7,"isolated":true,"liquidationPrice":0},{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"LONG","avgPrice":"60000","unrealizedProfit":"0","leverage":5,"isolated":true,"liquidationPrice":0}]"#,
        r#"[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"LONG","avgPrice":"60000","unrealizedProfit":"0","leverage":5,"isolated":true,"liquidationPrice":0},{"symbol":"BTC-USDT","positionAmt":"-0.1","positionSide":"SHORT","avgPrice":"60000","unrealizedProfit":"0","leverage":7,"isolated":true,"liquidationPrice":0}]"#,
    ] {
        let mut h = Harness::ready();
        h.engine.store_mut().set_hedged(true);
        h.fire_at(3000.0);
        h.respond(POSITIONS, 200, &format!(r#"{{"code":0,"data":{rows}}}"#));
        let setting = h.engine.store().setting("BTCUSDT").unwrap();
        assert_eq!(setting.leverage, 5.0);
        assert_eq!(setting.buy_leverage, Some(5.0));
        assert_eq!(setting.sell_leverage, Some(7.0));
    }
}

#[test]
fn hedge_snapshot_does_not_erase_the_configured_flat_opposite_leg() {
    let mut h = Harness::ready();
    h.engine.store_mut().set_hedged(true);
    h.fire_at(3000.0);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 2.0;
        s.buy_leverage = Some(2.0);
        s.sell_leverage = Some(7.0);
    });
    h.respond(POSITIONS, 200, r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"LONG","avgPrice":"60000","unrealizedProfit":"0","leverage":5,"isolated":true,"liquidationPrice":0}]}"#);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.leverage, 5.0);
    assert_eq!(setting.buy_leverage, Some(5.0));
    assert_eq!(setting.sell_leverage, Some(7.0));
}

#[test]
fn flat_snapshot_is_authoritative_but_missing_data_is_not() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    h.respond(POSITIONS, 200, r#"{"code":0,"data":[]}"#);
    assert!(h.engine.store().positions().is_empty());
    h.fire_at(5000.0);
    h.respond(POSITIONS, 200, r#"{"code":0,"msg":"missing"}"#);
    assert!(h.logs.iter().any(|s| s.contains("expected list")));
}

#[test]
fn startup_partial_snapshot_seeds_fill_baseline() {
    let partial = OPEN
        .replace("\"executedQty\":\"0\"", "\"executedQty\":\"0.05\"")
        .replace("\"status\":\"NEW\"", "\"status\":\"PARTIALLY_FILLED\"");
    let mut h = Harness::ready_with_orders(&partial);
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","z":"0.1","l":"0.05","L":"59002"}}"#);
    let fills = h.fills();
    assert_eq!(fills.len(), 1);
    assert!((fills[0].1 - 0.05).abs() < 1e-12);
}

#[test]
fn reconnect_snapshot_does_not_replay_historical_gap_at_new_execution_price() {
    let partial = OPEN
        .replace("\"executedQty\":\"0\"", "\"executedQty\":\"0.05\"")
        .replace("\"status\":\"NEW\"", "\"status\":\"PARTIALLY_FILLED\"");
    let mut h = Harness::ready_with_orders(&partial);
    h.event(Event::WsClosed {
        socket: h.private(),
        code: 1006,
    });
    h.fire_at(2000.0);
    h.respond(KEY, 200, r#"{"listenKey":"reconnected"}"#);
    h.event(Event::WsOpened {
        socket: h.private(),
    });
    let after_gap = partial.replace("\"executedQty\":\"0.05\"", "\"executedQty\":\"0.15\"");
    h.respond(ORDERS, 200, &after_gap);
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"FILLED","q":"0.2","z":"0.2","l":"0.05","L":"59003"}}"#);
    let fills = h.fills();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].0, 59003.0);
    assert!((fills[0].1 - 0.05).abs() < 1e-12);
}

#[test]
fn reconnect_execution_before_snapshot_uses_only_last_execution_quantity() {
    let partial = OPEN.replace("\"executedQty\":\"0\"", "\"executedQty\":\"0.05\"");
    let mut h = Harness::ready_with_orders(&partial);
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"FILLED","q":"0.2","z":"0.2","l":"0.05","L":"59003"}}"#);
    let fills = h.fills();
    assert_eq!(fills.len(), 1);
    assert!((fills[0].1 - 0.05).abs() < 1e-12);
}

#[test]
fn liquidation_execution_emits_fill_even_without_an_ordinary_order_type() {
    let mut h = Harness::ready();
    let liquidation = r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":8389765519993909001,"s":"BTC-USDT","o":"LIQUIDATION","S":"SELL","X":"FILLED","q":"0.1","z":"0.1","l":"0.1","L":"50000"}}"#;
    h.message(liquidation);
    assert_eq!(h.fills(), [(50000.0, 0.1)]);
    h.message(liquidation);
    assert!(h.fills().is_empty());
}

#[test]
fn position_poll_is_two_seconds_with_orders_and_balance_offset_by_four_seconds() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    assert!(h.requests.iter().any(|r| r.url.contains(POSITIONS)));
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains(ORDERS) || r.url.contains(BALANCE))
    );
    h.respond(POSITIONS, 200, POSITION);
    h.fire_at(5000.0);
    assert!(h.requests.iter().any(|r| r.url.contains(ORDERS)));
    assert!(!h.requests.iter().any(|r| r.url.contains(BALANCE)));
    h.respond(ORDERS, 200, OPEN);
    h.respond(POSITIONS, 200, POSITION);
    h.fire_at(7000.0);
    h.respond(POSITIONS, 200, POSITION);
    h.fire_at(9000.0);
    assert!(h.requests.iter().any(|r| r.url.contains(BALANCE)));
    assert!(!h.requests.iter().any(|r| r.url.contains(ORDERS)));
}

#[test]
fn terminal_without_cumulative_quantity_keeps_replay_protection() {
    let mut h = Harness::ready();
    let terminal = r#"{"e":"ORDER_TRADE_UPDATE","E":1002,"o":{"i":8389765519993909001,"s":"BTC-USDT","o":"MARKET","S":"BUY","X":"FILLED","q":"0.2","l":"0.2","L":"59002"}}"#;
    h.message(terminal);
    assert_eq!(h.fills(), [(59002.0, 0.2)]);
    h.message(terminal);
    assert!(h.fills().is_empty());
}

#[test]
fn cumulative_and_non_cumulative_copies_share_execution_identity() {
    for cumulative_first in [true, false] {
        let mut h = Harness::ready();
        let event = |kind: &str, cumulative: bool| {
            format!(
                r#"{{"e":"{kind}","E":1001,"o":{{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","p":"59000","t":"execution-one","T":1001,"l":"0.05","L":"59001"{}}}}}"#,
                if cumulative { r#", "z":"0.05""# } else { "" }
            )
        };
        h.message(&event("ORDER_TRADE_UPDATE", cumulative_first));
        assert_eq!(h.fills(), [(59001.0, 0.05)]);
        h.message(&event("TRADE_UPDATE", !cumulative_first));
        assert!(h.fills().is_empty());
        h.message(&event("TRADE_UPDATE", false).replace("\"T\":1001", "\"T\":1002"));
        assert!(h.fills().is_empty());
        let order = h.engine.store().order(ORDER_ID).unwrap();
        assert_eq!(order.filled, 0.05);
        assert!((order.remaining - 0.15).abs() < 1e-12);
    }
}

#[test]
fn a_partial_without_cumulative_size_updates_the_live_order_without_amend_proof() {
    let mut h = Harness::ready();
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","E":1001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","p":"59000","t":"partial-one","T":1001,"l":"0.05","L":"59001","ps":"BOTH","f":"GTC"}}"#);
    assert_eq!(h.fills(), [(59001.0, 0.05)]);
    let order = h.engine.store().order(ORDER_ID).unwrap();
    assert_eq!(order.filled, 0.05);
    assert!((order.remaining - 0.15).abs() < 1e-12);
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 42,
        id: ORDER_ID.into(),
        price: None,
        amount: Some(0.3),
    }));
    assert!(!h.requests.iter().any(|r| r.url.contains("/trade/amend")));
}

#[test]
fn mixed_partial_and_terminal_replays_keep_quantities_monotonic() {
    let mut h = Harness::ready();
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","E":1001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","t":"partial-one","T":1001,"l":"0.05","L":"59001"}}"#);
    assert_eq!(h.fills(), [(59001.0, 0.05)]);
    let terminal = r#"{"e":"TRADE_UPDATE","E":1002,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"FILLED","q":"0.2","t":"terminal-one","T":1002,"l":"0.15","L":"59002"}}"#;
    h.message(terminal);
    assert_eq!(h.fills(), [(59002.0, 0.15)]);
    let order = h.engine.store().order(ORDER_ID).unwrap();
    assert_eq!(order.status, OrderStatus::Closed);
    assert_eq!(order.filled, 0.2);
    assert_eq!(order.remaining, 0.0);
    h.message(&terminal.replace("\"l\":\"0.15\"", "\"z\":\"0.2\",\"l\":\"0.15\""));
    assert!(h.fills().is_empty());
    h.message(r#"{"e":"TRADE_UPDATE","E":1003,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","z":"0.05","t":"partial-one","T":1001,"l":"0.05","L":"59001"}}"#);
    assert!(h.fills().is_empty());
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.2);
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Closed
    );
    h.engine.store_mut().set_orders(Vec::new());
    h.message(terminal);
    assert!(h.fills().is_empty());
    h.message(&terminal.replace("\"l\":\"0.15\"", "\"z\":\"0.2\",\"l\":\"0.15\""));
    assert!(h.fills().is_empty());
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn snapshot_started_before_a_non_cumulative_partial_cannot_roll_it_back() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    h.respond(POSITIONS, 200, POSITION);
    h.fire_at(5000.0);
    let snapshot = h.take(ORDERS);
    h.message(r#"{"e":"ORDER_TRADE_UPDATE","E":5001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","t":"partial-one","T":5001,"l":"0.05","L":"59001"}}"#);
    assert_eq!(h.fills(), [(59001.0, 0.05)]);
    h.reply(snapshot.id, 200, OPEN);
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.05);
    assert!(h.requests.iter().any(|r| r.url.contains(ORDERS)));
}

#[test]
fn cumulative_growth_distinguishes_equal_fills_in_the_same_millisecond_without_trade_ids() {
    let mut h = Harness::ready();
    let partial = r#"{"e":"ORDER_TRADE_UPDATE","E":1001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","p":"59000","T":1001,"z":"0.05","l":"0.05","L":"59001"}}"#;
    h.message(partial);
    assert_eq!(h.fills(), [(59001.0, 0.05)]);
    h.message(&partial.replace("\"z\":\"0.05\"", "\"z\":\"0.1\""));
    assert_eq!(h.fills(), [(59001.0, 0.05)]);
    let order = h.engine.store().order(ORDER_ID).unwrap();
    assert_eq!(order.filled, 0.1);
    assert_eq!(order.remaining, 0.1);
    h.message(&partial.replace("\"z\":\"0.05\",", ""));
    assert!(h.fills().is_empty());
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.1);
}

#[test]
fn a_terminal_without_cumulative_size_accepts_a_unique_late_native_execution() {
    let mut h = Harness::ready();
    let terminal = r#"{"e":"ORDER_TRADE_UPDATE","E":1002,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"FILLED","q":"0.2","t":"second","T":1002,"l":"0.1","L":"59002"}}"#;
    h.message(terminal);
    assert_eq!(h.fills(), [(59002.0, 0.1)]);
    let order = h.engine.store().order(ORDER_ID).unwrap();
    assert_eq!(order.status, OrderStatus::Closed);
    assert_eq!(order.filled, 0.2);
    assert_eq!(order.remaining, 0.0);
    let partial = r#"{"e":"TRADE_UPDATE","E":1001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","t":"first","T":1001,"l":"0.1","L":"59001"}}"#;
    h.message(partial);
    assert_eq!(h.fills(), [(59001.0, 0.1)]);
    h.message(partial);
    assert!(h.fills().is_empty());
    h.message(terminal);
    assert!(h.fills().is_empty());
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.2);
}

#[test]
fn terminal_tombstones_suppress_unidentified_and_rest_covered_late_executions() {
    let terminal = r#"{"e":"ORDER_TRADE_UPDATE","E":1002,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"FILLED","q":"0.2","t":"second","T":1002,"l":"0.05","L":"59002"}}"#;
    let partial = r#"{"e":"TRADE_UPDATE","E":1001,"o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","T":1001,"l":"0.05","L":"59001"}}"#;
    let mut h = Harness::ready();
    h.message(terminal);
    assert_eq!(h.fills(), [(59002.0, 0.05)]);
    h.message(partial);
    assert!(h.fills().is_empty());
    let snapshot = OPEN.replace("\"executedQty\":\"0\"", "\"executedQty\":\"0.05\"");
    let mut h = Harness::ready_with_orders(&snapshot);
    h.message(terminal);
    assert_eq!(h.fills(), [(59002.0, 0.05)]);
    h.message(&partial.replace("\"T\":1001", "\"t\":\"historical-first\",\"T\":1001"));
    assert!(h.fills().is_empty());
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.2);
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().remaining, 0.0);
}
