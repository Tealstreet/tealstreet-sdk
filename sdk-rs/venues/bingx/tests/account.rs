//! Scripted private payload oracles: BingX-API/api-ai-skills swap-account,
//! swap-ws-account and swap-trade. No credentials or live writes are used.
use safe_cex_engine::model::{MarginType, OrderStatus, PositionMode, PositionSide};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Level, Method, Notice, RequestId, Response,
    SocketId, TimerId, wire,
};
use safe_cex_venue_bingx::Bingx;
mod support;
use support::Options;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

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
    errors: Vec<String>,
    done: Vec<(u32, Result<String, String>)>,
    command_errors: Vec<(u32, String)>,
    failures: Vec<(u32, safe_cex_engine::VenueError)>,
}
impl Harness {
    fn new() -> Self {
        let mut h = Self::unstarted();
        h.respond("/quote/contracts", 200, MARKETS);
        h.respond("/quote/ticker", 200, DAILY);
        h.respond("/quote/premiumIndex", 200, PREMIUM);
        h
    }
    fn unstarted() -> Self {
        Self::unstarted_with_provider(|provider| provider)
    }
    fn unstarted_with_provider(
        wrap: impl FnOnce(safe_cex_engine::signing::Provider) -> safe_cex_engine::signing::Provider,
    ) -> Self {
        let (venue, provider) = Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }
        .build();
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine.set_signer(Some(wrap(provider))).unwrap();
        let mut h = Self {
            engine,
            now: 1000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            timers: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            logs: Vec::new(),
            errors: Vec::new(),
            done: Vec::new(),
            command_errors: Vec::new(),
            failures: Vec::new(),
        };
        h.engine.start(h.now);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::Event(event)) => {
                    if event.level == Level::Error {
                        self.errors.push(event.message.clone());
                    }
                    self.logs.push(event.message);
                }
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())));
                    self.failures.push((command, error));
                }
                Action::Notice(Notice::CommandError { command, message }) => {
                    self.command_errors.push((command, message))
                }
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
            // Drive the freshly rearmed native timer when an older managed
            // response deadline shares its timestamp. This harness deliberately
            // leaves unrelated physical deadlines parked; dedicated tests drive them.
            .rposition(|(_, t)| *t == at)
            .unwrap_or_else(|| panic!("no timer at {at}; have {:?}", self.timers));
        let (timer, _) = self.timers.remove(i);
        self.now = at;
        self.event(Event::TimerFired { timer });
    }
    /// Whether a failed-read retry is armed at `sent` plus the engine's
    /// jittered `[0.75, 1.0] * nominal`.
    fn retry_index(&self, sent: f64, nominal: f64) -> Option<usize> {
        self.timers
            .iter()
            .position(|(_, at)| (0.75 * nominal..=nominal).contains(&(at - sent)))
    }
    /// Fires that retry at its nominal due time, never before its jittered one.
    fn fire_retry(&mut self, sent: f64, nominal: f64) {
        let i = self.retry_index(sent, nominal).unwrap_or_else(|| {
            panic!("no {nominal} ms retry after {sent}; have {:?}", self.timers)
        });
        let (timer, _) = self.timers.remove(i);
        self.now = sent + nominal;
        self.event(Event::TimerFired { timer });
    }
    fn fire_latest_due(&mut self, at: f64) {
        let i = self
            .timers
            .iter()
            .rposition(|(_, time)| *time <= at)
            .expect("due private watchdog");
        let (timer, _) = self.timers.remove(i);
        self.now = at;
        self.event(Event::TimerFired { timer });
    }
    fn fire_recovery(&mut self) {
        let (timer, at) = *self.timers.last().expect("recovery timer");
        assert!(at > self.now);
        self.timers.pop();
        self.now = at;
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
        Self::finish_ready(Self::new(), orders)
    }
    fn finish_ready(mut h: Self, orders: &str) -> Self {
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
fn an_unknown_placement_requires_a_fresh_order_read_before_retrying() {
    let mut h = Harness::ready_with_orders(r#"{"code":0,"data":[]}"#);
    for at in [3000., 5000., 7000.] {
        h.fire_at(at);
        if h.requests.iter().any(|r| r.url.contains(POSITIONS)) {
            h.respond(POSITIONS, 200, POSITION);
        }
        if h.requests.iter().any(|r| r.url.contains(BALANCE)) {
            h.respond(BALANCE, 200, BALANCES);
        }
    }
    let stale = h.take(ORDERS);
    let place = |command| Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_value(serde_json::json!({
                "symbol":"BTCUSDT", "type":"limit", "side":"buy",
                "amount":0.01, "price":60000
            }))
            .unwrap(),
        ],
    };
    h.now += 1.;
    h.event(Event::Command(place(700)));
    let first = h.take("/openApi/swap/v2/trade/order");
    h.reply(first.id, 503, "Service Unavailable");
    assert!(
        h.failures
            .iter()
            .any(|(id, error)| *id == 700
                && error.outcome == Some(safe_cex_engine::Outcome::Unknown))
    );
    let refused = |h: &mut Harness, id| {
        h.now += 1.;
        h.event(Event::Command(place(id)));
        assert!(
            h.requests.iter().all(
                |r| r.method != Method::Post || !r.url.contains("/openApi/swap/v2/trade/order")
            ),
            "no retry POST while uncertain"
        );
        assert!(h.failures.iter().any(|(command, error)| *command == id
            && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            && error.message.contains("uncertain")));
    };
    refused(&mut h, 701);
    let fresh = h.take(ORDERS);
    assert_ne!(fresh.id, stale.id);
    for at in [7001. + 1000., 7001. + 3000.] {
        h.fire_at(at);
        assert!(
            h.requests.iter().all(|r| !r.url.contains(ORDERS)),
            "engine re-asks join the held fresh Primary read"
        );
    }
    h.reply(stale.id, 200, r#"{"code":0,"data":[]}"#);
    refused(&mut h, 702);
    h.reply(fresh.id, 200, r#"{"code":0,"data":[]}"#);
    h.now += 1.;
    h.event(Event::Command(place(703)));
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.method == Method::Post && r.url.contains("/openApi/swap/v2/trade/order"))
            .count(),
        1
    );
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
    h.fire_recovery();
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
        !h.requests
            .iter()
            .any(|r| r.method == Method::Post && r.url.contains(KEY))
    );
    h.fire_recovery();
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
    h.fire_recovery();
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
    h.fire_latest_due(16000.0);
    h.fire_latest_due(31000.0);
    h.fire_latest_due(46000.0); // Last receive was at 1001.
    assert!(h.requests.iter().all(|r| !r.url.contains(KEY)));
    h.fire_latest_due(61000.0);
    h.fire_recovery();
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
fn orders_snapshot_crossing_the_store_write_fence_does_not_publish() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    let read = h.take(ORDERS);
    h.engine.store_mut().invalidate_order_snapshots();
    h.reply(read.id, 200, r#"{"code":0,"data":[]}"#);
    assert!(
        h.engine.store().order(ORDER_ID).is_some(),
        "a read issued before the Store write fence must not clear current orders"
    );
    let fresh = h.take(ORDERS);
    h.reply(fresh.id, 200, r#"{"code":0,"data":[]}"#);
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn accepted_cancel_keeps_its_order_state_when_an_older_read_arrives() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    let read = h.take(ORDERS);
    h.event(Event::Command(Command::CancelOrders {
        command: 991,
        ids: vec![ORDER_ID.into()],
    }));
    h.respond("/trade/order", 200, r#"{"code":0,"data":{}}"#);
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Canceled
    );
    h.reply(read.id, 200, OPEN);
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        h.done
            .iter()
            .filter(|(id, result)| *id == 991 && result.is_ok())
            .count(),
        1
    );
}

#[test]
fn an_uncrossed_orders_snapshot_still_publishes() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    let read = h.take(ORDERS);
    h.reply(read.id, 200, r#"{"code":0,"data":[]}"#);
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn sibling_store_publication_does_not_invalidate_an_orders_read() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    let read = h.take(ORDERS);
    h.engine.store_mut().set_orders(Vec::new());
    h.reply(read.id, 200, OPEN);
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn an_incomplete_new_read_keeps_the_clean_witness_for_an_older_stale_response() {
    let mut h = Harness::ready();
    h.poll_late(9000.0);
    let old = h.take(ORDERS);
    h.now += 1.;
    h.event(Event::Command(Command::CancelOrders {
        command: 992,
        ids: vec![ORDER_ID.into()],
    }));
    h.respond(
        "/trade/order",
        200,
        r#"{"code":80016,"msg":"order not exist"}"#,
    );
    h.respond(ORDERS, 200, OPEN);
    for at in [11000., 13000., 15000., 17000.] {
        h.fire_at(at);
    }
    let incomplete = h.take(ORDERS);
    h.reply(
        incomplete.id,
        200,
        r#"{"code":0,"data":{"orders":[{"orderId":8389765519993909000}]}}"#,
    );
    assert!(h.engine.store().order(ORDER_ID).is_some());
    h.reply(old.id, 200, r#"{"code":0,"data":[]}"#);
    assert!(h.engine.store().order(ORDER_ID).is_some());
    assert!(
        h.requests
            .iter()
            .all(|request| !request.url.contains(ORDERS)),
        "a clean current snapshot still suppresses a redundant read after an incomplete successor"
    );
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
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 2.0;
        s.buy_leverage = Some(2.0);
        s.sell_leverage = Some(7.0);
    });
    h.fire_at(3000.0);
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
    h.fire_recovery();
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
    let mut h = Harness::ready();
    let now = h.now;
    let engine = &mut h.engine;
    let (now, mut reads) = policy_poll_read(engine, now, "positions");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("balance") {
                BALANCES
            } else if r.url.contains("positions") {
                POSITION
            } else {
                r#"{"code":0,"data":{"orders":[]}}"#
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
// ETH-USDT is a malformed contract row (identity only); MISSING-USDT is not listed at all.
const PARTIAL_MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125},{"symbol":"ETH-USDT","asset":"ETH"}]}"#;
const BOTH_MARKETS: &str = r#"{"code":0,"data":[{"symbol":"ETH-USDT","asset":"ETH","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":2,"tradeMinLimit":"0.001","maxLongLeverage":100},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;

fn with_markets(markets: &str) -> Harness {
    let mut h = Harness::unstarted();
    h.respond("/quote/contracts", 200, markets);
    h.respond("/quote/ticker", 200, DAILY);
    h.respond("/quote/premiumIndex", 200, PREMIUM);
    h
}
// Prepends a copy of the body's only row on another market.
fn two_rows(body: &str, symbol: &str, id: &str) -> String {
    let open = body.find('[').unwrap() + 1;
    let close = body.rfind(']').unwrap();
    let extra = body[open..close]
        .replace("BTC-USDT", symbol)
        .replace(ORDER_ID, id)
        .replace("\"client\"", "\"other\"");
    body.replacen('[', &format!("[{extra},"), 1)
}

#[test]
fn owned_position_and_order_on_uncatalogued_market_are_skipped_and_the_snapshot_commits() {
    let mut h = Harness::new();
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.respond(
        POSITIONS,
        200,
        &two_rows(POSITION, "MISSING-USDT", ORDER_ID),
    );
    h.respond(BALANCE, 200, BALANCES);
    h.respond(ORDERS, 200, &two_rows(OPEN, "MISSING-USDT", "77"));
    let store = h.engine.store();
    let loaded = store.loaded();
    assert!(loaded.positions && loaded.balance && loaded.orders);
    assert_eq!(store.balance().total, 1000.0);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "BTCUSDT");
    assert_eq!(store.orders().len(), 1);
    assert_eq!(store.orders()[0].id, ORDER_ID);
    for kind in ["position", "order"] {
        let needle = format!("Skipping BingX {kind} on uncatalogued market MISSINGUSDT");
        assert!(h.logs.iter().any(|log| log.contains(&needle)), "{kind}");
    }
}

#[test]
fn owned_position_on_malformed_market_preserves_the_previous_snapshot() {
    let mut h = with_markets(PARTIAL_MARKETS);
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.respond(POSITIONS, 200, POSITION);
    h.respond(BALANCE, 200, BALANCES);
    h.respond(ORDERS, 200, OPEN);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.1);
    h.fire_at(3000.0);
    let changed = POSITION.replace("\"0.1\"", "\"0.4\"");
    h.respond(POSITIONS, 200, &two_rows(&changed, "ETH-USDT", ORDER_ID));
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.1);
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("owned position market unavailable"))
    );
}

#[test]
fn working_order_on_malformed_market_rejects_snapshot_but_other_missing_rows_do_not() {
    for (symbol, status, loaded) in [
        ("ETH-USDT", "NEW", false),
        ("ETH-USDT", "FILLED", true),
        ("MISSING-USDT", "NEW", true),
    ] {
        let mut h = with_markets(PARTIAL_MARKETS);
        h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
        h.respond(
            ORDERS,
            200,
            &OPEN.replace("BTC-USDT", symbol).replace("NEW", status),
        );
        assert_eq!(
            h.engine.store().loaded().orders,
            loaded,
            "{symbol} {status}"
        );
    }
}

#[test]
fn partial_catalogue_commits_then_rereads_without_a_second_account_start() {
    let mut h = with_markets(PARTIAL_MARKETS);
    assert!(h.engine.store().market("BTCUSDT").is_some());
    assert!(h.engine.store().market("ETHUSDT").is_none());
    h.take(KEY);
    h.take(MODE);
    h.requests.clear();
    let sockets = h.sockets.len();
    h.fire_retry(1000.0, 1000.0);
    h.respond("/quote/contracts", 200, BOTH_MARKETS);
    assert!(
        h.engine.store().market("ETHUSDT").is_some(),
        "repaired market"
    );
    assert!(h.engine.store().market("BTCUSDT").is_some());
    assert!(
        h.requests.is_empty(),
        "no ticker reseed or account restart: {:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    assert_eq!(h.sockets.len(), sockets);
    // BTC moved from market row 0 to 1; live ticks must still reach its seeded ticker.
    let public = h
        .sockets
        .iter()
        .find(|(_, url)| !url.contains("listenKey="))
        .unwrap()
        .0;
    h.event(Event::WsOpened { socket: public });
    h.event(Event::WsMessage {
        socket: public,
        text: r#"{"dataType":"market.contracts","data":{"contracts":[{"symbol":"BTC-USDT","tradePrice":"61000","fairPrice":"61001","indexPrice":"61002","fundingRate":"0.01"}]}}"#,
    });
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 61000.0);
}

#[test]
fn polling_ahead_of_stream_preserves_each_delayed_execution_once() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    h.fire_at(5000.0);
    h.respond(
        ORDERS,
        200,
        &OPEN.replace("\"executedQty\":\"0\"", "\"executedQty\":\"0.05\""),
    );
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.05);
    let event = r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":8389765519993909000,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"PARTIALLY_FILLED","q":"0.2","z":"0.05","l":"0.05","L":"59002","t":"late-1"}}"#;
    h.message(event);
    h.message(event);
    assert_eq!(h.fills(), [(59002.0, 0.05)]);
    assert_eq!(h.engine.store().order(ORDER_ID).unwrap().filled, 0.05);
}

#[test]
fn malformed_market_ticker_and_funding_rows_are_skipped_and_private_start_proceeds() {
    let mut h = Harness::unstarted();
    h.respond(
        "/quote/contracts",
        200,
        r#"{"code":0,"data":[{"symbol":"ETH-USDT","asset":"ETH"},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#,
    );
    assert_eq!(h.engine.store().markets().len(), 1);
    assert!(
        h.logs
            .iter()
            .any(|l| l.contains("skipped 1 malformed market row(s): ETH-USDT"))
    );
    h.respond(
        "/quote/ticker",
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"},{"symbol":"BTC-USDT","lastPrice":null}]}"#,
    );
    h.respond(
        "/quote/premiumIndex",
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"bad"}]}"#,
    );
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.symbol.as_str(), ticker.last), ("BTCUSDT", 60000.0));
    assert!(
        h.logs
            .iter()
            .any(|l| l.contains("malformed funding row(s): BTC-USDT"))
    );
    h.take(KEY);
    h.take(MODE);
}

#[test]
fn a_failed_funding_read_starts_the_private_side_and_its_retry_fills_funding() {
    let mut h = Harness::unstarted();
    h.respond("/quote/contracts", 200, MARKETS);
    h.respond("/quote/ticker", 200, DAILY);
    h.respond(
        "/quote/premiumIndex",
        200,
        r#"{"code":100500,"msg":"busy"}"#,
    );
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.last, ticker.funding_rate), (60000.0, 0.0));
    h.take(KEY);
    h.fire_retry(1000.0, 1000.0);
    h.respond("/quote/premiumIndex", 200, PREMIUM);
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!((ticker.mark, ticker.funding_rate), (60001.0, 0.0001));
    assert_eq!(h.engine.store().tickers().len(), 1);
}

#[test]
fn one_malformed_position_row_is_skipped_and_never_closes_its_known_position() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    h.respond(
        POSITIONS,
        200,
        r#"{"code":0,"data":[{"symbol":"ETH-USDT"},{"symbol":"BTC-USDT","positionAmt":"0.2","positionSide":"BOTH","avgPrice":"60000","unrealizedProfit":"10","leverage":20,"isolated":false,"liquidationPrice":50000}]}"#,
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 0.2);
    assert!(
        h.logs
            .iter()
            .any(|l| l.contains("skipped malformed position row ETHUSDT"))
    );
    for snapshot in [
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.3","leverage":0}]}"#,
        r#"{"code":0,"data":[42]}"#,
    ] {
        h.fire_at(h.now + 2000.0);
        h.respond(POSITIONS, 200, snapshot);
        assert_eq!(h.engine.store().positions().len(), 1);
        assert_eq!(h.engine.store().positions()[0].contracts, 0.2);
    }
}

#[test]
fn one_malformed_order_row_is_skipped_and_keeps_only_the_order_it_names() {
    let mut h = Harness::ready();
    h.poll_late(26000.0);
    h.respond(
        ORDERS,
        200,
        r#"{"code":0,"data":{"orders":[{"orderId":8389765519993909000,"symbol":"BTC-USDT","type":"LIMIT"},{"orderId":77,"symbol":"BTC-USDT","type":"LIMIT","side":"SELL","status":"NEW","origQty":"0.1","executedQty":"0","price":"61000","positionSide":"BOTH","timeInForce":"GTC"}]}}"#,
    );
    assert!(h.engine.store().order(ORDER_ID).is_some());
    assert_eq!(h.engine.store().order("77").unwrap().price, 61000.0);
    assert!(
        h.logs
            .iter()
            .any(|l| l.contains(&format!("skipped malformed open order row {ORDER_ID}")))
    );
    h.fire_at(28000.0);
    h.fire_at(30000.0);
    h.respond(
        ORDERS,
        200,
        r#"{"code":0,"data":{"orders":[{"orderId":77,"symbol":"BTC-USDT"}]}}"#,
    );
    assert!(h.engine.store().order(ORDER_ID).is_none());
    assert!(h.engine.store().order("77").is_some());
}

#[test]
fn signature_rejection_is_re_signed_and_resent_exactly_once() {
    let mut h = Harness::new();
    let rejected = r#"{"code":100001,"msg":"Signature verification failed"}"#;
    let first = h.take(MODE);
    h.now += 5.0;
    h.reply(first.id, 200, rejected);
    let second = h.take(MODE);
    assert_ne!(first.url, second.url);
    assert!(second.url.contains("timestamp=1005"));
    h.reply(second.id, 200, rejected);
    assert!(!h.requests.iter().any(|r| r.url.contains(MODE)));
    assert!(h.logs.iter().any(|l| l.contains("private request failed")));

    // Streamed reads and writes take the same single resend.
    h.respond(ORDERS, 200, rejected);
    h.respond(ORDERS, 200, OPEN);
    assert!(h.engine.store().order(ORDER_ID).is_some());
    h.event(Event::Command(Command::CancelOrders {
        command: 9,
        ids: vec![ORDER_ID.into()],
    }));
    h.respond("/trade/order", 200, rejected);
    h.respond("/trade/order", 200, r#"{"code":0,"data":{}}"#);
    assert_eq!(
        h.engine.store().order(ORDER_ID).unwrap().status,
        OrderStatus::Canceled
    );

    // Other venue errors are never resent.
    h.respond(BALANCE, 200, r#"{"code":100004,"msg":"denied","data":[]}"#);
    assert!(!h.requests.iter().any(|r| r.url.contains(BALANCE)));
}

#[test]
fn account_rows_on_a_market_skipped_at_startup_are_reported_once_as_errors() {
    let mut h = Harness::unstarted();
    h.respond(
        "/quote/contracts",
        200,
        r#"{"code":0,"data":[{"symbol":"ETH-USDT","asset":"ETH"},{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#,
    );
    h.respond("/quote/ticker", 200, DAILY);
    h.respond("/quote/premiumIndex", 200, PREMIUM);
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    let eth = r#"{"code":0,"data":[{"symbol":"ETH-USDT","positionAmt":"1","positionSide":"BOTH","avgPrice":"3000","unrealizedProfit":"0","leverage":5,"isolated":false,"liquidationPrice":0}]}"#;
    h.respond(POSITIONS, 200, eth);
    h.respond(
        ORDERS,
        200,
        r#"{"code":0,"data":{"orders":[{"orderId":5,"symbol":"ETH-USDT","type":"LIMIT","side":"BUY","status":"NEW","origQty":"1","executedQty":"0","price":"2900","positionSide":"BOTH","timeInForce":"GTC"}]}}"#,
    );
    h.fire_at(3000.0);
    h.respond(POSITIONS, 200, eth);
    let errors: Vec<_> = h
        .logs
        .iter()
        .filter(|l| l.contains("ETHUSDT is hidden"))
        .collect();
    assert_eq!(errors.len(), 1, "{:?}", h.logs);
}

#[test]
fn a_malformed_daily_row_still_gets_a_ticker_slot_for_funding_and_stream_updates() {
    let mut h = Harness::unstarted();
    h.respond("/quote/contracts", 200, MARKETS);
    h.respond(
        "/quote/ticker",
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":null}]}"#,
    );
    h.respond("/quote/premiumIndex", 200, PREMIUM);
    h.take(KEY);
    let ticker = &h.engine.store().tickers()[0];
    assert_eq!(
        (ticker.symbol.as_str(), ticker.mark, ticker.funding_rate),
        ("BTCUSDT", 60001.0, 0.0001)
    );
}

#[test]
fn flat_rows_close_even_with_zero_leverage_and_hedge_skips_keep_only_their_side() {
    let mut h = Harness::ready();
    h.fire_at(3000.0);
    h.respond(
        POSITIONS,
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0","positionSide":"BOTH","avgPrice":"0","unrealizedProfit":"0","leverage":0,"isolated":false,"liquidationPrice":0}]}"#,
    );
    assert!(h.engine.store().positions().is_empty());

    h.engine.store_mut().set_hedged(true);
    h.fire_at(5000.0);
    h.respond(
        POSITIONS,
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"LONG","avgPrice":"60000","unrealizedProfit":"0","leverage":5,"isolated":false,"liquidationPrice":0},{"symbol":"BTC-USDT","positionAmt":"-0.2","positionSide":"SHORT","avgPrice":"60000","unrealizedProfit":"0","leverage":5,"isolated":false,"liquidationPrice":0}]}"#,
    );
    assert_eq!(h.engine.store().positions().len(), 2);
    // The malformed long row keeps the long; the short, absent, closes.
    h.fire_at(7000.0);
    h.respond(
        POSITIONS,
        200,
        r#"{"code":0,"data":[{"symbol":"BTC-USDT","positionSide":"LONG","positionAmt":"bad"}]}"#,
    );
    let positions = h.engine.store().positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].side, PositionSide::Long);
}

#[test]
fn a_persistent_unidentifiable_order_row_keeps_known_orders_and_logs_once() {
    let mut h = Harness::ready();
    let mut at = 26000.0;
    h.poll_late(at);
    let unidentified = "skipped malformed open order row without an id";
    for _ in 0..3 {
        h.respond(ORDERS, 200, r#"{"code":0,"data":{"orders":[42]}}"#);
        assert!(h.engine.store().order(ORDER_ID).is_some());
        for _ in 0..4 {
            at += 2000.0;
            h.fire_at(at);
        }
    }
    let count = |logs: &[String]| logs.iter().filter(|l| l.contains(unidentified)).count();
    assert_eq!(count(&h.logs), 1, "{:?}", h.logs);
    assert!(h.errors.is_empty(), "{:?}", h.errors);
    // A clean snapshot re-arms it; with nothing stored to keep it is an error.
    h.respond(ORDERS, 200, r#"{"code":0,"data":{"orders":[]}}"#);
    assert!(h.engine.store().order(ORDER_ID).is_none());
    for _ in 0..4 {
        at += 2000.0;
        h.fire_at(at);
    }
    h.respond(ORDERS, 200, r#"{"code":0,"data":{"orders":[42]}}"#);
    assert_eq!(count(&h.logs), 2, "{:?}", h.logs);
    assert_eq!(count(&h.errors), 1, "{:?}", h.errors);
}

#[test]
fn a_rejected_position_row_logs_once_until_it_maps_or_a_snapshot_is_clean() {
    let mut h = Harness::ready();
    let bad_btc = r#"{"symbol":"BTC-USDT"}"#;
    let bad_eth = r#"{"symbol":"ETH-USDT"}"#;
    let good_btc = r#"{"symbol":"BTC-USDT","positionAmt":"0.1","positionSide":"BOTH","avgPrice":"60000","unrealizedProfit":"10","leverage":20,"isolated":false,"liquidationPrice":50000}"#;
    let mut at = 1000.0;
    let mut poll = |h: &mut Harness, rows: &[&str]| {
        at += 2000.0;
        h.fire_at(at);
        h.respond(
            POSITIONS,
            200,
            &format!(r#"{{"code":0,"data":[{}]}}"#, rows.join(",")),
        );
    };
    let count = |logs: &[String], id: &str| {
        let line = format!("skipped malformed position row {id}");
        logs.iter().filter(|l| l.contains(&line)).count()
    };
    poll(&mut h, &[bad_btc, bad_eth]);
    poll(&mut h, &[bad_btc, bad_eth]);
    assert_eq!(count(&h.logs, "BTCUSDT"), 1, "{:?}", h.logs);
    assert_eq!(count(&h.logs, "ETHUSDT"), 1, "{:?}", h.logs);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.1);
    // The stored BTC copy is kept, so only the ETH row is an error.
    assert_eq!(count(&h.errors, "BTCUSDT"), 0, "{:?}", h.errors);
    assert_eq!(count(&h.errors, "ETHUSDT"), 1, "{:?}", h.errors);

    // BTC maps again, re-arming only its own log.
    poll(&mut h, &[good_btc, bad_eth]);
    poll(&mut h, &[bad_btc, bad_eth]);
    assert_eq!(count(&h.logs, "BTCUSDT"), 2, "{:?}", h.logs);
    assert_eq!(count(&h.logs, "ETHUSDT"), 1, "{:?}", h.logs);

    // A clean snapshot re-arms every position log.
    poll(&mut h, &[good_btc]);
    poll(&mut h, &[good_btc, bad_eth]);
    assert_eq!(count(&h.logs, "ETHUSDT"), 2, "{:?}", h.logs);
    assert_eq!(count(&h.errors, "ETHUSDT"), 2, "{:?}", h.errors);
    assert_eq!(count(&h.errors, "BTCUSDT"), 0, "{:?}", h.errors);

    // BTC drops out while ETH stays rejected; its malformed return has no copy left.
    poll(&mut h, &[bad_btc, bad_eth]);
    poll(&mut h, &[bad_eth]);
    assert!(h.engine.store().positions().is_empty());
    poll(&mut h, &[bad_btc, bad_eth]);
    assert_eq!(count(&h.errors, "BTCUSDT"), 1, "{:?}", h.errors);
}

#[test]
fn a_skip_warned_on_a_stale_snapshot_still_errors_once_its_copy_is_gone() {
    let mut h = Harness::ready();
    let bad_btc = r#"{"code":0,"data":[{"symbol":"BTC-USDT"}]}"#;
    h.fire_at(3000.0);
    let positions = h.take(POSITIONS);
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","pa":"0","ep":"60000","up":"0","ps":"BOTH","mt":"cross"}]}}"#);
    h.reply(positions.id, 200, bad_btc);
    let skipped = |logs: &[String]| {
        logs.iter()
            .filter(|l| l.contains("skipped malformed position row BTCUSDT"))
            .count()
    };
    assert_eq!(skipped(&h.logs), 1, "{:?}", h.logs);
    assert_eq!(skipped(&h.errors), 0, "{:?}", h.errors);
    // The repair read commits with no BTC copy left.
    h.fire_recovery();
    h.respond(POSITIONS, 200, bad_btc);
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(skipped(&h.errors), 1, "{:?}", h.errors);
}

#[test]
fn timed_out_key_acquisition_is_fenced_and_retried_after_backoff() {
    let mut h = Harness::new();
    let old = h.take(KEY);
    // Resolve peer requests so the remaining 20s deadline belongs to the key.
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.respond(POSITIONS, 200, POSITION);
    h.respond(BALANCE, 200, BALANCES);
    h.respond(ORDERS, 200, OPEN);
    let deadlines: Vec<_> = h
        .timers
        .iter()
        .copied()
        .filter(|(_, at)| *at == h.now + 20_000.)
        .collect();
    assert_eq!(deadlines.len(), 1, "one actual key HTTP deadline");
    let (timer, at) = deadlines[0];
    h.now = at;
    h.event(Event::TimerFired { timer });
    let sockets = h.sockets.len();
    h.reply(old.id, 200, r#"{"listenKey":"late-retired"}"#);
    assert_eq!(h.sockets.len(), sockets);
    assert!(!h.requests.iter().any(|r| r.url.contains(KEY)));
    h.fire_recovery();
    assert_eq!(h.take(KEY).method, Method::Post);
}

#[test]
fn refresh_that_omits_a_held_market_keeps_it_with_its_position_and_order() {
    let mut h = with_markets(PARTIAL_MARKETS);
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.respond(POSITIONS, 200, POSITION);
    h.respond(BALANCE, 200, BALANCES);
    h.respond(ORDERS, 200, OPEN);
    let listed = h.engine.store().market("BTCUSDT").cloned().unwrap();
    h.fire_retry(1000.0, 1000.0);
    // BTC-USDT drops out of the list while ETH-USDT stays malformed.
    h.respond(
        "/quote/contracts",
        200,
        r#"{"code":0,"data":[{"symbol":"SOL-USDT","asset":"SOL","currency":"USDT","status":1,"quantityPrecision":0,"pricePrecision":2,"tradeMinLimit":"1","maxLongLeverage":50},{"symbol":"ETH-USDT","asset":"ETH"}]}"#,
    );
    assert_eq!(h.engine.store().market("BTCUSDT"), Some(&listed));
    assert!(h.engine.store().market("SOLUSDT").is_some());
    h.fire_at(3000.0);
    h.respond(POSITIONS, 200, POSITION);
    if h.requests.iter().any(|r| r.url.contains(ORDERS)) {
        h.respond(ORDERS, 200, OPEN);
    }
    let store = h.engine.store();
    assert!(store.loaded().positions && store.loaded().orders);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "BTCUSDT");
    assert!(store.order(ORDER_ID).is_some());
}

#[test]
fn persistently_malformed_catalogue_stops_rereading_after_five_refreshes() {
    let mut h = with_markets(PARTIAL_MARKETS);
    let (mut sent, mut delay) = (1000.0, 1000.0);
    for _ in 0..5 {
        h.fire_retry(sent, delay);
        h.respond("/quote/contracts", 200, PARTIAL_MARKETS);
        sent += delay;
        delay *= 2.0;
    }
    assert!(h.retry_index(sent, 30_000.0).is_none(), "no sixth re-read");
    assert!(h.logs.iter().any(|log| log.contains("stops re-reading")));
    assert!(h.engine.store().market("BTCUSDT").is_some());
}

#[test]
fn balance_updates_faster_than_the_balance_read_cannot_starve_the_balance() {
    let mut h = Harness::ready();
    let update = r#"{"e":"ACCOUNT_UPDATE","a":{"B":[{"a":"USDT","wb":"1"}]}}"#;
    h.message(update);
    let mut read = h.take(BALANCE);
    for step in 1..=3u32 {
        h.message(update);
        let body = BALANCES.replace(r#""1000""#, &format!(r#""{}""#, 2000 + step));
        h.reply(read.id, 200, &body);
        let expected = if step <= 2 { 1000.0 } else { 2003.0 };
        assert_eq!(h.engine.store().balance().total, expected, "step {step}");
        read = h.take(BALANCE);
    }
}

#[test]
fn position_events_faster_than_the_mode_read_cannot_starve_mode_and_positions() {
    let mut h = Harness::new();
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.event(Event::WsOpened {
        socket: h.private(),
    });
    let mut read = h.take(MODE);
    for step in 1..=3 {
        h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":10}}"#);
        h.reply(
            read.id,
            200,
            r#"{"code":0,"data":{"dualSidePosition":true}}"#,
        );
        let positions = h.requests.iter().any(|r| r.url.contains(POSITIONS));
        assert_eq!(h.engine.store().hedged(), step == 3, "step {step}");
        assert_eq!(positions, step == 3, "step {step}");
        read = h.take(MODE);
    }
    // Once known (for example just set by a command), only a current read may change the mode.
    for step in 1..=3 {
        h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":10}}"#);
        h.reply(
            read.id,
            200,
            r#"{"code":0,"data":{"dualSidePosition":false}}"#,
        );
        assert!(h.engine.store().hedged(), "stale step {step}");
        read = h.take(MODE);
    }
}

const LEVERAGE: &str = "/openApi/swap/v2/trade/leverage";
const FRESH_RAILS: &str = r#"{"code":0,"msg":"","data":{"longLeverage":8,"shortLeverage":"12","maxLongLeverage":125,"maxShortLeverage":125}}"#;

fn assert_fresh_rails(h: &Harness, command: u32, buy: f64, sell: f64) {
    assert_eq!(h.done.len(), 1);
    let (id, result) = &h.done[0];
    assert_eq!(*id, command);
    let json: serde_json::Value =
        serde_json::from_str(result.as_ref().expect("successful read")).unwrap();
    assert_eq!(json["buyLeverage"].as_f64(), Some(buy));
    assert_eq!(json["sellLeverage"].as_f64(), Some(sell));
}

fn fetch_leverage(h: &mut Harness, command: u32, symbol: &str) {
    h.event(Event::Command(Command::FetchLeverage {
        command,
        symbol: symbol.into(),
    }));
}

#[test]
fn leverage_read_is_a_fresh_signed_get_returning_both_rails() {
    let mut h = Harness::ready();
    fetch_leverage(&mut h, 7, "BTCUSDT");
    let r = h.take(LEVERAGE);
    assert_eq!(r.method, Method::Get);
    assert!(r.url.contains("symbol=BTC-USDT"), "{}", r.url);
    assert!(r.url.contains("signature="), "{}", r.url);
    h.reply(r.id, 200, FRESH_RAILS);
    assert!(
        h.done.is_empty(),
        "rails alone cannot commit partial metadata"
    );
    let margin = h.take("/trade/marginType");
    assert_eq!(margin.method, Method::Get);
    h.reply(margin.id, 200, ISOLATED);
    assert_fresh_rails(&h, 7, 8.0, 12.0);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(8.0));
    assert_eq!(setting.sell_leverage, Some(12.0));
    assert!(h.command_errors.is_empty());
}

#[test]
fn leverage_read_failures_report_once_and_keep_settings() {
    let mut h = Harness::ready();
    let before = h.engine.store().setting("BTCUSDT");
    for (command, status, body) in [
        (8, 200, r#"{"code":109400,"msg":"symbol not exist"}"#),
        (
            9,
            200,
            r#"{"code":0,"data":{"longLeverage":0,"shortLeverage":5}}"#,
        ),
        (10, 200, r#"{"code":0,"data":{}}"#),
        (11, 503, "unavailable"),
    ] {
        fetch_leverage(&mut h, command, "BTCUSDT");
        let r = h.take(LEVERAGE);
        h.reply(r.id, status, body);
        let (done, result) = h.done.pop().expect("completed");
        assert_eq!(done, command);
        let error = result.expect_err(body);
        assert!(!error.is_empty());
        assert!(h.command_errors.is_empty());
    }
    assert!(h.done.is_empty() && h.command_errors.is_empty());
    assert_eq!(h.engine.store().setting("BTCUSDT"), before);
}

#[test]
fn a_config_update_during_the_read_wins_over_its_snapshot() {
    let mut h = Harness::ready();
    fetch_leverage(&mut h, 12, "BTCUSDT");
    let r = h.take(LEVERAGE);
    h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":30,"S":25}}"#);
    h.reply(r.id, 200, FRESH_RAILS);
    // The read answers newer cached rails without publishing its stale snapshot.
    assert_eq!(metadata_rails(&h, 12), (30.0, 25.0));
    assert!(
        !h.requests
            .iter()
            .any(|request| request.url.contains("/trade/marginType"))
    );
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(30.0));
    assert_eq!(setting.sell_leverage, Some(25.0));
}

#[test]
fn leverage_read_needs_a_known_market() {
    let mut h = Harness::ready();
    fetch_leverage(&mut h, 13, "ETHUSDT");
    assert_eq!(h.done, [(13, Err("Unknown BingX market".into()))]);
    assert!(!h.requests.iter().any(|r| r.url.contains(LEVERAGE)));
}

#[test]
fn unrelated_position_activity_during_the_read_does_not_drop_it() {
    let mut h = Harness::ready();
    fetch_leverage(&mut h, 14, "BTCUSDT");
    let r = h.take(LEVERAGE);
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","pa":"0.2","ep":"60000","up":"1","ps":"BOTH","mt":"cross"}]}}"#);
    h.reply(r.id, 200, FRESH_RAILS);
    let margin = h.take("/trade/marginType");
    h.reply(margin.id, 200, ISOLATED);
    assert_fresh_rails(&h, 14, 8.0, 12.0);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(8.0));
    assert_eq!(setting.sell_leverage, Some(12.0));
}

#[test]
fn repeated_explicit_leverage_reads_request_a_new_atomic_pair_despite_cached_rails() {
    let mut h = Harness::ready();
    for command in [21, 22] {
        fetch_leverage(&mut h, command, "BTCUSDT");
        let leverage = h.take(LEVERAGE);
        assert_eq!(leverage.method, Method::Get);
        h.reply(leverage.id, 200, FRESH_RAILS);
        let margin = h.take("/trade/marginType");
        h.reply(margin.id, 200, ISOLATED);
        assert_fresh_rails(&h, command, 8.0, 12.0);
        h.done.clear();
    }
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
}

#[test]
fn a_market_without_a_positive_step_is_malformed_and_holds_its_exposure() {
    let markets = PARTIAL_MARKETS.replace(
        r#"{"symbol":"ETH-USDT","asset":"ETH"}"#,
        r#"{"symbol":"ETH-USDT","asset":"ETH","currency":"USDT","status":1,"quantityPrecision":400,"pricePrecision":2,"tradeMinLimit":"0.001","maxLongLeverage":100}"#,
    );
    let mut h = with_markets(&markets);
    assert!(h.engine.store().market("ETHUSDT").is_none());
    h.respond(KEY, 200, r#"{"listenKey":"key-one"}"#);
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    h.respond(POSITIONS, 200, POSITION);
    h.respond(BALANCE, 200, BALANCES);
    h.respond(ORDERS, 200, OPEN);
    h.fire_at(3000.0);
    let changed = POSITION.replace("\"0.1\"", "\"0.4\"");
    h.respond(POSITIONS, 200, &two_rows(&changed, "ETH-USDT", ORDER_ID));
    assert_eq!(h.engine.store().positions()[0].contracts, 0.1);
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("owned position market unavailable"))
    );
}

#[test]
fn metadata_reads_flat_symbol_leverage_and_margin_without_a_position() {
    let mut h = Harness::ready();
    h.engine.store_mut().set_positions(vec![]);
    h.event(Event::Command(Command::FetchLeverage {
        command: 901,
        symbol: "BTCUSDT".into(),
    }));
    let leverage = h.take("/trade/leverage");
    assert_eq!(leverage.method, Method::Get);
    assert!(leverage.url.contains("symbol=BTC-USDT"));
    assert!(leverage.url.contains("signature="));
    h.reply(
        leverage.id,
        200,
        r#"{"code":0,"data":{"longLeverage":12,"shortLeverage":"34"}}"#,
    );
    let margin = h.take("/trade/marginType");
    assert_eq!(margin.method, Method::Get);
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: margin.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":{"marginType":"ISOLATED"}}"#,
            },
        },
    );
    let out = h.engine.drain_actions().collect::<Vec<_>>();
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command:901, result:json }) if serde_json::from_str::<serde_json::Value>(json).unwrap() == serde_json::json!({"buyLeverage":12,"sellLeverage":34}))));
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(12.0));
    assert_eq!(setting.sell_leverage, Some(34.0));
    assert_eq!(setting.margin_type, Some(MarginType::Isolated));
    assert!(h.engine.store().positions().is_empty());
}

#[test]
fn malformed_position_update_keeps_valid_peers_and_requests_repair() {
    let mut h = Harness::ready();
    let skipped = h.engine.health().rows_skipped;
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","ps":"BOTH","pa":"broken","ep":"60000"},{"s":"BTC-USDT","ps":"BOTH","pa":"0.2","ep":"60010","up":"12"}]}}"#);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.2);
    assert_eq!(h.engine.store().positions()[0].entry_price, 60010.0);
    assert_eq!(h.engine.health().rows_skipped, skipped + 1);
    let at = h
        .timers
        .iter()
        .filter(|(_, at)| *at > h.now && *at < h.now + 750.0)
        .map(|(_, at)| *at)
        .min_by(f64::total_cmp)
        .expect("repair timer");
    h.fire_at(at);
    assert!(h.requests.iter().any(|r| r.url.contains(POSITIONS)));
}

const RAILS: &str = r#"{"code":0,"data":{"longLeverage":12,"shortLeverage":34}}"#;
const ISOLATED: &str = r#"{"code":0,"data":{"marginType":"ISOLATED"}}"#;

fn joined_metadata(h: &mut Harness, first: u32, second: u32) -> HttpRequest {
    fetch_leverage(h, first, "BTCUSDT");
    fetch_leverage(h, second, "BTCUSDT");
    assert_eq!(
        h.requests
            .iter()
            .filter(|request| request.url.contains(LEVERAGE))
            .count(),
        1,
        "same-revision metadata callers share one leverage leg"
    );
    h.take(LEVERAGE)
}

#[test]
fn metadata_owner_coalesces_each_native_leg_and_completes_both_callers() {
    let mut h = Harness::ready();
    let read = joined_metadata(&mut h, 950, 951);
    h.reply(read.id, 200, FRESH_RAILS);
    assert!(
        h.done.is_empty(),
        "leverage alone cannot finish either owner"
    );
    assert_eq!(
        h.requests
            .iter()
            .filter(|request| request.url.contains("/trade/marginType"))
            .count(),
        1,
        "the joined group shares one margin leg too"
    );
    let margin = h.take("/trade/marginType");
    h.reply(margin.id, 200, ISOLATED);
    for command in [950, 951] {
        assert_eq!(metadata_rails(&h, command), (8.0, 12.0));
    }
    assert_eq!(h.done.len(), 2);
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
}

#[test]
fn metadata_owner_fails_both_joined_callers_and_keeps_good_settings() {
    let mut h = Harness::ready();
    let before = h.engine.store().setting("BTCUSDT");
    let read = joined_metadata(&mut h, 952, 953);
    h.reply(read.id, 503, "unavailable");
    for command in [952, 953] {
        failed_metadata(&h, command);
    }
    assert_eq!(h.done.len(), 2);
    assert_eq!(h.engine.store().setting("BTCUSDT"), before);
    assert!(!h.requests.iter().any(|request| {
        request.url.contains(LEVERAGE) || request.url.contains("/trade/marginType")
    }));
}

fn metadata(h: &mut Harness, command: u32) -> HttpRequest {
    h.event(Event::Command(Command::FetchLeverage {
        command,
        symbol: "BTCUSDT".into(),
    }));
    h.take("/trade/leverage")
}
fn margin_stage(h: &mut Harness, command: u32) -> HttpRequest {
    let read = metadata(h, command);
    h.reply(read.id, 200, RAILS);
    h.take("/trade/marginType")
}
fn failed_metadata(h: &Harness, command: u32) {
    assert_eq!(h.done.iter().filter(|(id, _)| *id == command).count(), 1);
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == command && result.is_err())
    );
}

fn metadata_rails(h: &Harness, command: u32) -> (f64, f64) {
    let results = h
        .done
        .iter()
        .filter(|(id, _)| *id == command)
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 1, "read settles once");
    let value: serde_json::Value = serde_json::from_str(
        results[0]
            .1
            .as_ref()
            .expect("settings overlap still returns leverage"),
    )
    .unwrap();
    assert!(!h.command_errors.iter().any(|(id, _)| *id == command));
    (
        value["buyLeverage"].as_f64().unwrap(),
        value["sellLeverage"].as_f64().unwrap(),
    )
}

#[test]
fn metadata_race_with_newer_settings_returns_current_rails_without_publication() {
    for margin in [false, true] {
        let mut h = Harness::ready();
        let read = if margin {
            margin_stage(&mut h, 920)
        } else {
            metadata(&mut h, 920)
        };
        h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":25}}"#);
        let newer = h.engine.store().setting("BTCUSDT").unwrap();
        h.reply(
            read.id,
            200,
            if margin {
                r#"{"code":0,"data":{"marginType":"UNKNOWN"}}"#
            } else {
                RAILS
            },
        );
        assert_eq!(metadata_rails(&h, 920), (25.0, 25.0));
        assert_eq!(h.engine.store().setting("BTCUSDT"), Some(newer));
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("/trade/marginType"))
        );
    }
}

#[test]
fn metadata_read_started_during_any_pending_write_never_publishes() {
    for kind in 0..3 {
        for finish_before_data in [false, true] {
            let mut h = Harness::ready();
            if kind == 2 {
                h.engine.store_mut().set_positions(Vec::new());
                h.engine.store_mut().set_orders(Vec::new());
            }
            let before = h.engine.store().setting("BTCUSDT").unwrap();
            let (command, path) = match kind {
                0 => (
                    Command::SetLeverage {
                        command: 947,
                        symbol: "BTCUSDT".into(),
                        leverage: 25.,
                        side: None,
                    },
                    "/trade/leverage",
                ),
                1 => (
                    Command::SetMarginType {
                        command: 947,
                        symbol: "BTCUSDT".into(),
                        isolated: true,
                    },
                    "/trade/marginType",
                ),
                _ => (
                    Command::SetPositionMode {
                        command: 947,
                        symbol: None,
                        hedged: true,
                    },
                    "/positionSide/dual",
                ),
            };
            h.event(Event::Command(command));
            let write = h.take(path);
            assert!(h.engine.store().metadata_write_pending("BTCUSDT"));
            let read = metadata(&mut h, 948);
            assert_eq!(read.method, Method::Get);
            if finish_before_data {
                h.event(Event::HttpFailed {
                    request: write.id,
                    error: "scripted write failure",
                });
                assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
            }
            h.reply(read.id, 200, RAILS);
            if h.requests
                .iter()
                .any(|request| request.url.contains("/trade/marginType"))
            {
                let margin = h.take("/trade/marginType");
                h.reply(margin.id, 200, ISOLATED);
            }
            if !finish_before_data {
                h.event(Event::HttpFailed {
                    request: write.id,
                    error: "scripted write failure",
                });
            }
            assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
            assert_eq!(
                h.engine.store().setting("BTCUSDT"),
                Some(before),
                "a read issued during pending write {kind} must stay nonpublishing after write retirement"
            );
            assert_eq!(
                metadata_rails(&h, 948),
                (before.buy_leverage.unwrap(), before.sell_leverage.unwrap())
            );
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 948).count(), 1);
            assert!(
                !h.requests
                    .iter()
                    .any(|r| r.url.contains("/trade/marginType"))
            );
        }
    }
}

#[test]
fn metadata_race_with_pending_write_returns_cached_rails_or_read_fallback() {
    for (buy, sell, expected) in [
        (Some(20.0), Some(20.0), (20.0, 20.0)),
        (None, None, (12.0, 34.0)),
        (Some(47.0), None, (47.0, 34.0)),
        (None, Some(53.0), (12.0, 53.0)),
    ] {
        for failed_write in [false, true] {
            let mut h = Harness::ready();
            h.engine.store_mut().update_setting("BTCUSDT", |setting| {
                setting.buy_leverage = buy;
                setting.sell_leverage = sell;
            });
            h.event(Event::Command(Command::SetLeverage {
                command: 921,
                symbol: "BTCUSDT".into(),
                leverage: 25.0,
                side: None,
            }));
            let write = h.take("/trade/leverage");
            assert_eq!(write.method, Method::Post);
            let before = h.engine.store().setting("BTCUSDT").unwrap();
            let read = metadata(&mut h, 922);
            assert_eq!(read.method, Method::Get);
            if failed_write {
                h.event(Event::HttpFailed {
                    request: write.id,
                    error: "scripted write failure",
                });
            }
            h.reply(read.id, 200, RAILS);
            assert_eq!(metadata_rails(&h, 922), expected);
            assert_eq!(h.engine.store().setting("BTCUSDT"), Some(before));
            assert!(
                !h.requests
                    .iter()
                    .any(|r| r.url.contains("/trade/marginType"))
            );
            assert!(!h.requests.iter().any(|r| r.method != Method::Get));
        }
    }
}

#[test]
fn superseded_metadata_still_rejects_invalid_native_leverage_responses() {
    for (status, body) in [
        (503, "{}"),
        (200, r#"{"code":109400,"msg":"invalid symbol"}"#),
        (200, r#"{"code":0,"data":{}}"#),
        (
            200,
            r#"{"code":0,"data":{"longLeverage":0,"shortLeverage":34}}"#,
        ),
    ] {
        let mut h = Harness::ready();
        let read = metadata(&mut h, 923);
        h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":25}}"#);
        let newer = h.engine.store().setting("BTCUSDT");
        h.reply(read.id, status, body);
        failed_metadata(&h, 923);
        assert_eq!(h.engine.store().setting("BTCUSDT"), newer);
    }
}
#[test]
fn metadata_failures_never_publish_partial_or_guessed_settings() {
    for (status, body) in [
        (503, "{}"),
        (200, r#"{"code":0,"data":{"marginType":"UNKNOWN"}}"#),
        (200, r#"{"code":0,"data":{}}"#),
    ] {
        let mut h = Harness::ready();
        let old = h.engine.store().setting("BTCUSDT").unwrap();
        let read = margin_stage(&mut h, 902);
        assert_eq!(
            h.engine.store().setting("BTCUSDT").unwrap().leverage,
            old.leverage
        );
        h.reply(read.id, status, body);
        failed_metadata(&h, 902);
        let setting = h.engine.store().setting("BTCUSDT").unwrap();
        assert_eq!(setting.buy_leverage, old.buy_leverage);
        assert_eq!(setting.sell_leverage, old.sell_leverage);
        assert_eq!(setting.margin_type, old.margin_type);
    }
    for body in [
        r#"{"code":0,"data":{"longLeverage":0,"shortLeverage":34}}"#,
        r#"{"code":0,"data":{"longLeverage":"NaN","shortLeverage":34}}"#,
        r#"{"code":0,"data":{"longLeverage":12}}"#,
    ] {
        let mut h = Harness::ready();
        let read = metadata(&mut h, 903);
        h.reply(read.id, 200, body);
        failed_metadata(&h, 903);
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("/trade/marginType"))
        );
    }
    let mut h = Harness::ready();
    let read = metadata(&mut h, 904);
    h.event(Event::HttpFailed {
        request: read.id,
        error: "scripted failure",
    });
    failed_metadata(&h, 904);
}
#[test]
fn newer_settings_writes_and_snapshots_fence_metadata() {
    for change in 0..4 {
        let mut h = Harness::ready();
        let read = margin_stage(&mut h, 905);
        match change {
            0 => h.event(Event::Command(Command::SetLeverage {
                command: 906,
                symbol: "BTCUSDT".into(),
                leverage: 25.0,
                side: None,
            })),
            1 => h.event(Event::Command(Command::SetMarginType {
                command: 906,
                symbol: "BTCUSDT".into(),
                isolated: true,
            })),
            2 => h.event(Event::Command(Command::SetPositionMode {
                command: 906,
                symbol: None,
                hedged: true,
            })),
            _ => {
                h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":25}}"#);
            }
        }
        let newer = h.engine.store().setting("BTCUSDT").unwrap();
        h.reply(read.id, 200, ISOLATED);
        assert_eq!(
            metadata_rails(&h, 905),
            (newer.buy_leverage.unwrap(), newer.sell_leverage.unwrap())
        );
        assert_eq!(h.engine.store().setting("BTCUSDT"), Some(newer));
        assert_ne!(
            h.engine.store().setting("BTCUSDT").unwrap().buy_leverage,
            Some(12.0)
        );
    }
    let mut h = Harness::ready();
    h.now += POLL_MS;
    let poll = h.timers.iter().find(|(_, at)| *at == 3000.0).unwrap().0;
    h.event(Event::TimerFired { timer: poll });
    let snapshot = h.take(POSITIONS);
    let read = margin_stage(&mut h, 907);
    h.reply(snapshot.id, 200, POSITION);
    h.reply(read.id, 200, ISOLATED);
    assert_eq!(metadata_rails(&h, 907), (20.0, 20.0));
}
#[test]
fn metadata_pending_write_is_sticky_and_eligible_queries_coalesce() {
    let mut h = Harness::ready();
    h.event(Event::Command(Command::SetLeverage {
        command: 908,
        symbol: "BTCUSDT".into(),
        leverage: 25.0,
        side: None,
    }));
    let write = h.take("/trade/leverage");
    h.event(Event::Command(Command::FetchLeverage {
        command: 909,
        symbol: "BTCUSDT".into(),
    }));
    let read = h.take("/trade/leverage");
    h.reply(read.id, 200, RAILS);
    assert_eq!(metadata_rails(&h, 909), (20.0, 20.0));
    assert!(!h.requests.iter().any(|r| r.url.contains("/trade/leverage")));
    h.reply(write.id, 200, r#"{"code":0,"data":{}}"#);
    let first = margin_stage(&mut h, 910);
    fetch_leverage(&mut h, 911, "BTCUSDT");
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/trade/leverage") || r.url.contains("/trade/marginType"))
    );
    h.reply(first.id, 200, ISOLATED);
    assert_eq!(metadata_rails(&h, 910), (12.0, 34.0));
    assert!(h.done.iter().any(|(id, r)| *id == 911 && r.is_ok()));
}
#[test]
fn malformed_only_updates_repair_and_balance_only_updates_stay_quiet() {
    for source in ["P", "p", "B"] {
        let mut h = Harness::ready();
        let skipped = h.engine.health().rows_skipped;
        h.message(&format!(r#"{{"e":"ACCOUNT_UPDATE","a":{{"{source}":[{{"s":"BTC-USDT","ps":"BOTH","pa":"bad"}}]}}}}"#));
        assert_eq!(h.engine.health().rows_skipped, skipped + 1);
        assert_eq!(h.engine.store().positions()[0].contracts, 0.1);
        assert!(
            h.timers
                .iter()
                .any(|(_, at)| *at > h.now && *at < h.now + 750.0)
        );
    }
    let mut h = Harness::ready();
    let skipped = h.engine.health().rows_skipped;
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"B":[{"a":"USDT","wb":"bad"}]}}"#);
    assert_eq!(h.engine.health().rows_skipped, skipped);
    assert!(
        !h.timers
            .iter()
            .any(|(_, at)| *at > h.now && *at < h.now + 750.0)
    );
}

#[test]
fn metadata_commit_fences_older_positions_and_signature_retries_keep_the_fence() {
    let mut h = Harness::ready();
    let poll = h.timers.iter().find(|(_, at)| *at == 3000.0).unwrap().0;
    h.now = 3000.0;
    h.event(Event::TimerFired { timer: poll });
    let snapshot = h.take(POSITIONS);
    let read = margin_stage(&mut h, 912);
    h.reply(read.id, 200, ISOLATED);
    assert!(h.done.iter().any(|(id, r)| *id == 912 && r.is_ok()));
    h.reply(snapshot.id, 200, POSITION);
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(12.0)
    );
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );

    for stage in [0, 1] {
        let mut h = Harness::ready();
        let read = if stage == 0 {
            metadata(&mut h, 913)
        } else {
            margin_stage(&mut h, 913)
        };
        h.reply(
            read.id,
            200,
            r#"{"code":100001,"msg":"signature mismatch"}"#,
        );
        let path = if stage == 0 {
            "/trade/leverage"
        } else {
            "/trade/marginType"
        };
        let retry = h.take(path);
        h.message(r#"{"e":"ACCOUNT_CONFIG_UPDATE","ac":{"s":"BTC-USDT","l":50}}"#);
        h.reply(retry.id, 200, if stage == 0 { RAILS } else { ISOLATED });
        assert_eq!(metadata_rails(&h, 913), (50.0, 50.0));
        assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 50.0);
        assert!(!h.requests.iter().any(|r| r.url.contains(path)));
    }
}
#[test]
fn position_source_precedence_and_mixed_balance_fallback_apply_once() {
    let mut h = Harness::ready();
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","pa":"0.2","ep":60000,"ps":"BOTH"}],"B":[{"s":"BTC-USDT","pa":"0.3","ps":"BOTH"}],"p":[{"s":"BTC-USDT","pa":"0.4","ps":"BOTH"}]}}"#);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.2);
    let skipped = h.engine.health().rows_skipped;
    h.message(r#"{"e":"ACCOUNT_UPDATE","a":{"B":[{"a":"USDT","wb":"bad"},{"s":"BTC-USDT","pa":"0.5","ps":"BOTH"}]}}"#);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.5);
    assert_eq!(h.engine.health().rows_skipped, skipped);
}

#[test]
fn independent_symbol_metadata_and_empty_poll_do_not_invalidate_each_other() {
    let mut h = Harness::ready();
    let mut other = h.engine.store().market("BTCUSDT").unwrap().clone();
    other.id = "ETH-USDT".into();
    other.symbol = "ETHUSDT".into();
    other.base = "ETH".into();
    let first = h.engine.store().market("BTCUSDT").unwrap().clone();
    h.engine.store_mut().set_markets(vec![first, other]);
    let one = margin_stage(&mut h, 914);
    h.event(Event::Command(Command::FetchLeverage {
        command: 915,
        symbol: "ETHUSDT".into(),
    }));
    let two = h.take("/trade/leverage");
    assert!(two.url.contains("symbol=ETH-USDT"));
    h.reply(two.id, 200, RAILS);
    let two = h.take("/trade/marginType");
    assert!(two.url.contains("symbol=ETH-USDT"));
    h.reply(one.id, 200, ISOLATED);
    h.reply(two.id, 200, ISOLATED);
    for id in [914, 915] {
        assert!(h.done.iter().any(|(c, r)| *c == id && r.is_ok()));
    }

    let mut h = Harness::ready();
    let poll = h.timers.iter().find(|(_, at)| *at == 3000.0).unwrap().0;
    h.now = 3000.0;
    h.event(Event::TimerFired { timer: poll });
    let snapshot = h.take(POSITIONS);
    let read = margin_stage(&mut h, 916);
    h.reply(snapshot.id, 200, r#"{"code":0,"data":[]}"#);
    h.reply(read.id, 200, ISOLATED);
    assert!(h.done.iter().any(|(id, r)| *id == 916 && r.is_ok()));
}

#[test]
fn metadata_preflight_emits_one_not_sent_failure_on_the_new_completion_api() {
    let mut h = Harness::ready();
    h.engine.handle(
        h.now,
        Event::Command(Command::FetchLeverage {
            command: 917,
            symbol: "MISSING".into(),
        }),
    );
    let out = h.engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(out.len(), 1);
    assert!(
        matches!(&out[0],Action::Notice(Notice::CommandFailed {command:917,error}) if error.outcome==Some(safe_cex_engine::Outcome::NotSent))
    );
}

const SIGNATURE_MISMATCH: &str = r#"{"code":100001,"msg":"signature mismatch"}"#;
/// Signed query bytes, without the per-attempt timestamp and signature.
fn native_query(r: &HttpRequest) -> String {
    let (path, query) = r.url.split_once('?').unwrap_or((&r.url, ""));
    let kept = query
        .split('&')
        .filter(|p| !p.starts_with("timestamp=") && !p.starts_with("signature="))
        .collect::<Vec<_>>()
        .join("&");
    format!("{path}?{kept}")
}

#[test]
fn plan_metadata_keeps_one_fresh_signature_retry_per_read_and_native_bytes() {
    let mut h = Harness::ready();
    let first = metadata(&mut h, 930);
    h.reply(first.id, 200, SIGNATURE_MISMATCH);
    let retry = h.take("/trade/leverage");
    assert_eq!(native_query(&retry), native_query(&first));
    assert_eq!((retry.method, &retry.body), (first.method, &first.body));
    assert_ne!(retry.id, first.id, "a fresh physical attempt");
    h.reply(retry.id, 200, RAILS);
    let margin = h.take("/trade/marginType");
    h.reply(margin.id, 200, SIGNATURE_MISMATCH);
    let margin_retry = h.take("/trade/marginType");
    assert_eq!(native_query(&margin_retry), native_query(&margin));
    h.reply(margin_retry.id, 200, ISOLATED);
    assert_eq!(metadata_rails(&h, 930), (12.0, 34.0));
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (
            setting.buy_leverage,
            setting.sell_leverage,
            setting.margin_type
        ),
        (Some(12.0), Some(34.0), Some(MarginType::Isolated))
    );
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/trade/leverage") || r.url.contains("/trade/marginType"))
    );
}

#[test]
fn a_second_signature_rejection_fails_the_plan_without_a_third_attempt() {
    for margin in [false, true] {
        let mut h = Harness::ready();
        let before = h.engine.store().setting("BTCUSDT");
        let (read, path) = if margin {
            (margin_stage(&mut h, 931), "/trade/marginType")
        } else {
            (metadata(&mut h, 931), "/trade/leverage")
        };
        h.reply(read.id, 200, SIGNATURE_MISMATCH);
        let retry = h.take(path);
        h.reply(retry.id, 200, SIGNATURE_MISMATCH);
        failed_metadata(&h, 931);
        assert!(!h.requests.iter().any(|r| r.url.contains(path)));
        assert_eq!(h.engine.store().setting("BTCUSDT"), before);
    }
}

#[test]
fn a_signature_retry_gets_a_fresh_native_window_after_the_first_attempt_deadline() {
    let mut h = Harness::ready();
    let start = h.now;
    let first = metadata(&mut h, 932);
    h.now = start + 15_000.0;
    h.reply(first.id, 200, SIGNATURE_MISMATCH);
    let retry = h.take("/trade/leverage");
    // Past the first attempt's 20s window, inside the retry's own fresh one.
    h.now = start + 25_000.0;
    h.reply(retry.id, 200, RAILS);
    let margin = h.take("/trade/marginType");
    h.reply(margin.id, 200, ISOLATED);
    assert_eq!(metadata_rails(&h, 932), (12.0, 34.0));
}

#[test]
fn a_silent_metadata_read_times_out_the_plan_once() {
    let mut h = Harness::ready();
    let before = h.engine.store().setting("BTCUSDT");
    let start = h.now;
    let _read = metadata(&mut h, 933);
    let due: Vec<_> = h
        .timers
        .iter()
        .filter(|(_, at)| *at <= start + 20_000.0)
        .cloned()
        .collect();
    h.now = start + 20_000.0;
    for (timer, _) in due {
        h.event(Event::TimerFired { timer });
    }
    failed_metadata(&h, 933);
    assert_eq!(h.engine.store().setting("BTCUSDT"), before);
}

#[test]
fn metadata_plan_entry_duplicate_unknown_market_cannot_fail_an_active_owner() {
    let mut h = Harness::ready();
    let _first = metadata(&mut h, 940);
    h.engine.handle(
        h.now,
        Event::Command(Command::FetchLeverage {
            command: 940,
            symbol: "MISSING".into(),
        }),
    );
    let out = h.engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !out.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 940, .. })
                | Action::Notice(Notice::CommandDone { command: 940, .. })
        )),
        "a duplicate entry cannot numerically terminate the active metadata owner"
    );
}

#[test]
fn metadata_plan_entry_requires_prepare_before_native_http_release() {
    let mut h = Harness::ready();
    h.engine.handle(
        h.now,
        Event::Command(Command::FetchLeverage {
            command: 941,
            symbol: "BTCUSDT".into(),
        }),
    );
    let out = h.engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !out.iter().any(|action| matches!(action, Action::Http(_))),
        "metadata cannot release legacy HTTP before managed preparation"
    );
    assert!(
        out.iter().any(|action| matches!(
            action,
            Action::ManagedHttp(action)
                if matches!(action.as_ref(), safe_cex_engine::request_context::HttpAction::Prepare { .. })
        )),
        "the metadata owner must issue a real managed preparation"
    );
}

#[test]
fn metadata_plan_entry_distinct_unknown_market_keeps_native_refusal() {
    let mut h = Harness::ready();
    h.engine.handle(
        h.now,
        Event::Command(Command::FetchLeverage {
            command: 942,
            symbol: "MISSING".into(),
        }),
    );
    let out = h.engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        out.iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 942, error })
                    if error.legacy_text() == "Unknown BingX market"
            ))
            .count(),
        1
    );
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
}

#[test]
fn metadata_plan_entry_native_pair_keeps_the_rails_result() {
    let mut h = Harness::ready();
    let margin = margin_stage(&mut h, 943);
    h.reply(margin.id, 200, ISOLATED);
    assert_eq!(metadata_rails(&h, 943), (12.0, 34.0));
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 943).count(), 1);
}

#[test]
fn metadata_write_provider_refusal_releases_pending_owner_before_next_read() {
    struct Refusing {
        inner: safe_cex_engine::signing::Provider,
        refuse: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }
    impl safe_cex_engine::signing::Signer for Refusing {
        fn immediate(&self) -> bool {
            true
        }
        fn sign(
            &self,
            request: safe_cex_engine::signing::SignRequest,
        ) -> safe_cex_engine::signing::SignFuture {
            self.inner.sign(request)
        }
        fn sign_ready(
            &self,
            request: &safe_cex_engine::signing::SignRequest,
        ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
        {
            if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
                Some(Err(safe_cex_engine::signing::SignError::Refused))
            } else {
                self.inner.sign_ready(request)
            }
        }
    }
    for margin in [false, true] {
        let refuse = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = refuse.clone();
        let mut h = Harness::unstarted_with_provider(|inner| {
            std::sync::Arc::new(Refusing {
                inner,
                refuse: flag,
            })
        });
        h.respond("/quote/contracts", 200, MARKETS);
        h.respond("/quote/ticker", 200, DAILY);
        h.respond("/quote/premiumIndex", 200, PREMIUM);
        let mut h = Harness::finish_ready(h, OPEN);
        refuse.store(true, std::sync::atomic::Ordering::SeqCst);
        let requests = h.requests.len();
        let auth_failures = h.engine.health().auth_failures;
        h.event(Event::Command(if margin {
            Command::SetMarginType {
                command: 944,
                symbol: "BTCUSDT".into(),
                isolated: true,
            }
        } else {
            Command::SetLeverage {
                command: 944,
                symbol: "BTCUSDT".into(),
                leverage: 25.0,
                side: None,
            }
        }));
        assert_eq!(
            h.done
                .iter()
                .filter(|(id, result)| *id == 944 && result.is_err())
                .count(),
            1
        );
        assert_eq!(
            h.requests.len(),
            requests,
            "provider refusal releases no HTTP"
        );
        assert_eq!(
            h.failures
                .iter()
                .filter(|(id, _)| *id == 944)
                .collect::<Vec<_>>(),
            vec![&(
                944,
                safe_cex_engine::signing::SignError::Refused.venue_error()
            )]
        );
        assert_eq!(
            h.engine.health().auth_failures,
            auth_failures,
            "provider refusal emits neither auth.rejected nor deactivation"
        );
        assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
        refuse.store(false, std::sync::atomic::Ordering::SeqCst);
        let read = margin_stage(&mut h, 945);
        let mut normal = Harness::ready();
        let expected = margin_stage(&mut normal, 946);
        assert_eq!(
            (&read.url, &read.headers, &read.body),
            (&expected.url, &expected.headers, &expected.body),
            "refusal-off delegates to the unchanged native signing recipe"
        );
        h.reply(read.id, 200, ISOLATED);
        assert_eq!(metadata_rails(&h, 945), (12.0, 34.0));
        let setting = h.engine.store().setting("BTCUSDT").unwrap();
        assert_eq!(
            (setting.buy_leverage, setting.sell_leverage),
            (Some(12.0), Some(34.0))
        );
        assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
        assert_eq!(h.done.iter().filter(|(id, _)| *id == 944).count(), 1);
    }
}

mod metadata_entry_capacity {
    use super::*;
    use safe_cex_engine::Venue;
    use safe_cex_engine::request_context::{
        CommandEntryError, Context, Error, Operation, Policy, PreflightPolicy, PreflightPurpose,
        RetryPolicy,
    };

    struct Probe {
        native: Bingx,
        markets: Vec<safe_cex_engine::model::Market>,
        roots: Vec<Context>,
        release: Option<TimerId>,
        admission: Option<Result<(), CommandEntryError>>,
    }
    fn policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    impl Venue for Probe {
        type Pending = safe_cex_venue_bingx::Pending;
        fn capabilities(&self) -> safe_cex_engine::Capabilities {
            self.native.capabilities()
        }

        fn start(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>) {
            // Only pre-admission metadata refusals run here; native startup I/O
            // is outside this fixture, which supplies the ready market catalogue.
            cx.store.set_markets(self.markets.clone());
            self.release = Some(cx.set_timer(1001.));
        }
        fn on_command(
            &mut self,
            cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
            command: Command,
        ) {
            if let Command::FetchAccount { command: id } = command {
                if (1000..1256).contains(&id) {
                    self.roots.push(cx.admit_http_command(policy()).unwrap());
                    return;
                }
                for root in &self.roots {
                    cx.validate_read_root_context(root).unwrap();
                    cx.validate_http_context(root).unwrap();
                }
                let result = cx.begin_preflight(PreflightPolicy {
                    purpose: PreflightPurpose::ReadOnly,
                    read_credential: None,
                    write_credential: None,
                });
                self.admission = Some(result.as_ref().map(|_| ()).map_err(|e| *e));
                if let Ok(preflight) = result {
                    cx.done_preflight(&preflight, "admission-control".into())
                        .unwrap();
                }
            } else {
                self.native.on_command(cx, command);
            }
        }
        fn on_timer(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, timer: TimerId) {
            assert_eq!(self.release.take(), Some(timer));
            cx.done_context(&self.roots.pop().unwrap(), "released-one-owner".into())
                .unwrap();
        }
        fn on_response(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
            _: Self::Pending,
            _: Response<'_>,
        ) {
            panic!("entry proof releases no HTTP");
        }
        fn on_request_failed(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
            _: Self::Pending,
            _: &str,
        ) {
            panic!("entry proof has no transport");
        }
        fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, Self::Pending>, _: SocketId) {}
        fn on_close(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
            _: SocketId,
            _: u16,
        ) {
        }
        fn on_message(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
            _: SocketId,
            _: &str,
        ) {
        }
        fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn failed(actions: &[Action], id: u32) -> safe_cex_engine::VenueError {
        let terminals = actions.iter().filter(|a| matches!(a,
            Action::Notice(Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. }) if *command == id
        )).collect::<Vec<_>>();
        assert_eq!(terminals.len(), 1, "one correlated terminal");
        match terminals[0] {
            Action::Notice(Notice::CommandFailed { error, .. }) => error.clone(),
            other => panic!("expected failure, got {other:?}"),
        }
    }
    fn no_native_effects(actions: &[Action]) {
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Http(_) | Action::Sign { .. } | Action::ManagedHttp(_) | Action::Notice(Notice::Deactivate)
        ) || matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::AUTH_REJECTED
        )));
    }
    fn check(missing_key: bool) {
        let markets = Harness::ready().engine.store().markets().to_vec();
        let (native, provider) = Options {
            key: (!missing_key).then(|| "key".into()),
            secret: (!missing_key).then(|| "secret".into()),
            ..Options::default()
        }
        .build();
        let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
            native,
            markets,
            roots: Vec::new(),
            release: None,
            admission: None,
        });
        engine.set_signer(Some(provider)).unwrap();
        engine.start(1000.);
        for id in 1000..1256 {
            engine.handle(1000., Event::Command(Command::FetchAccount { command: id }));
            no_native_effects(&engine.drain_actions().collect::<Vec<_>>());
        }
        assert_eq!(engine.venue().roots.len(), 256);
        no_native_effects(&engine.drain_actions().collect::<Vec<_>>());
        engine.handle(
            1000.,
            Event::Command(Command::FetchAccount { command: 976 }),
        );
        assert_eq!(
            engine.venue().admission,
            Some(Err(CommandEntryError::Rejected(Error::Capacity)))
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        no_native_effects(&actions);
        let capacity = failed(&actions, 976);
        assert_eq!(
            capacity.outcome, None,
            "Read admission does not invent a write outcome"
        );
        let symbol = if missing_key { "BTCUSDT" } else { "MISSING" };
        engine.handle(
            1000.,
            Event::Command(Command::FetchLeverage {
                command: 977,
                symbol: symbol.into(),
            }),
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        no_native_effects(&actions);
        let exhausted = failed(&actions, 977);
        let timer = engine.venue().release.unwrap();
        engine.handle(1001., Event::TimerFired { timer });
        assert_eq!(engine.venue().roots.len(), 255);
        no_native_effects(&engine.drain_actions().collect::<Vec<_>>());
        engine.handle(
            1001.,
            Event::Command(Command::FetchAccount { command: 978 }),
        );
        assert_eq!(engine.venue().admission, Some(Ok(())));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        no_native_effects(&actions);
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 978, result }) if result == "admission-control")).count(), 1);
        engine.handle(
            1001.,
            Event::Command(Command::FetchLeverage {
                command: 979,
                symbol: symbol.into(),
            }),
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        no_native_effects(&actions);
        let native = failed(&actions, 979);
        assert_eq!(
            native.legacy_text(),
            if missing_key {
                "BingX metadata requires credentials"
            } else {
                "Unknown BingX market"
            }
        );
        assert_eq!(engine.health().auth_failures, 0);
        assert_eq!(
            exhausted, capacity,
            "metadata pre-admission refusal must not bypass genuine engine capacity"
        );
    }
    #[test]
    fn unknown_market_cannot_bypass_entry_capacity() {
        check(false);
    }
    #[test]
    fn missing_key_cannot_bypass_entry_capacity() {
        check(true);
    }
}

fn retired_metadata_token(margin: bool) {
    let mut h = Harness::ready();
    let before = h.engine.store().setting("BTCUSDT");
    let packet = if margin {
        margin_stage(&mut h, 980)
    } else {
        metadata(&mut h, 980)
    };
    let live = h.engine.store().metadata_executions();
    assert_eq!(live.len(), 1);
    h.engine.store_mut().retire_metadata_work(&live[0].0);
    assert!(h.engine.store().metadata_executions().is_empty());
    h.reply(packet.id, 200, if margin { ISOLATED } else { RAILS });
    assert_eq!(h.engine.store().setting("BTCUSDT"), before);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/trade/marginType"))
    );
    let terminal = h
        .done
        .iter()
        .filter(|(id, _)| *id == 980)
        .cloned()
        .collect::<Vec<_>>();
    let failure = h
        .failures
        .iter()
        .filter(|(id, _)| *id == 980)
        .cloned()
        .collect::<Vec<_>>();
    let fresh = margin_stage(&mut h, 981);
    h.reply(fresh.id, 200, ISOLATED);
    assert_eq!(metadata_rails(&h, 981), (12.0, 34.0));
    assert_eq!(
        terminal.len(),
        1,
        "missing Store custody cannot strand a live Plan"
    );
    assert!(terminal[0].1.is_err());
    assert_eq!(failure.len(), 1);
    assert_eq!(
        failure[0].1.kind,
        Some(safe_cex_engine::ErrorKind::Internal)
    );
    h.reply(packet.id, 200, ISOLATED);
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 980).count(), 1);
}
#[test]
fn retired_metadata_token_rejects_current_settings_fallback_once() {
    retired_metadata_token(false);
}
#[test]
fn retired_metadata_token_rejects_final_margin_publication_once() {
    retired_metadata_token(true);
}

#[test]
fn refresh_settings_re_reads_the_position_mode_and_positions_and_writes_nothing() {
    let mut h = Harness::ready();
    h.requests.clear();
    h.event(Event::Command(Command::Venue {
        command: 60,
        action: safe_cex_engine::VenueAction::RefreshSettings { symbol: None },
    }));
    assert!(
        h.done
            .iter()
            .any(|(command, result)| *command == 60 && result.is_ok())
    );
    assert_eq!(
        h.requests.len(),
        1,
        "{:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    assert_eq!(h.requests[0].method, Method::Get);
    h.respond(MODE, 200, r#"{"code":0,"data":{"dualSidePosition":false}}"#);
    assert_eq!(h.requests.len(), 1);
    assert_eq!(h.requests[0].method, Method::Get);
    h.respond(POSITIONS, 200, POSITION);
    assert!(
        h.requests.is_empty(),
        "{:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
}
