use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus, OrderType};
use safe_cex_engine::wire::{section, sections};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, SocketChannel, SocketId,
};
use safe_cex_venue_bitrue::{Bitrue, Options};
const MARKET: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":10,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
const EMPTY: &str = r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"accountLock":5,"totalPositionNormal":7,"sumOpenRealizedAmount":2,"positionVos":[]}]}}"#;
const POSITION: &str = r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"accountLock":5,"totalPositionNormal":7,"sumOpenRealizedAmount":2,"positionVos":[{"contractName":"E-BTC-USDT","positions":[{"side":"BUY","volume":2,"openPrice":100,"leverageLevel":25,"positionType":2,"positionBalance":200,"forceLiquidationPrice":50}]}]}]}}"#;
const ORDER: &str = r#"{"code":"0","data":[{"orderId":2334151155877155281,"clientOrderId":"caller","type":"LIMIT","side":"BUY","status":"NEW","action":"OPEN","price":100,"origQty":100,"executedQty":0,"transactTime":1}]}"#;
struct Harness {
    e: Engine<Bitrue>,
    pending: Vec<HttpRequest>,
    actions: Vec<Action>,
    private: SocketId,
    now: f64,
}
impl Harness {
    fn new() -> Self {
        let mut e = Engine::new(Bitrue::new(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        }));
        e.start(0.0);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            private: SocketId(0),
            now: 0.0,
        };
        h.collect();
        h.reply("/contracts", MARKET);
        h.reply(
            "listenKey",
            r#"{"code":200,"data":{"listenKey":"futures-key"}}"#,
        );
        h.private = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    url,
                    channel: Some(SocketChannel::Private),
                    ..
                } if url.contains("fapiws.bitrue.com") => Some(*socket),
                _ => None,
            })
            .unwrap();
        h
    }
    fn collect(&mut self) {
        for a in self.e.drain_actions() {
            if let Action::Http(r) = a {
                self.pending.push(r)
            } else {
                self.actions.push(a)
            }
        }
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .pending
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.pending));
        self.pending.remove(i)
    }
    fn respond(&mut self, r: HttpRequest, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    fn reply(&mut self, path: &str, body: &str) {
        let r = self.take(path);
        self.respond(r, body);
    }
    fn seed(&mut self) {
        self.reply("/account", EMPTY);
        self.reply("/openOrders", ORDER);
    }
    fn ws(&mut self, text: &str) {
        self.now += 1.0;
        self.e.handle(
            self.now,
            Event::WsMessage {
                socket: self.private,
                text,
            },
        );
        self.collect();
    }
    fn order(&mut self, z: u32, status: &str, kind: &str, price: u32) {
        self.ws(&format!(r#"{{"e":"ORDER_TRADE_UPDATE","E":{},"o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"{kind}","x":"TRADE","X":"{status}","q":"100","z":"{z}","l":"999","p":"100","ap":"101","L":"{price}"}}}}"#,self.now+1.0));
    }
    fn fire_at(&mut self, at: f64) {
        let i = self
            .actions
            .iter()
            .rposition(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==at))
            .unwrap_or_else(|| panic!("no timer at {at}: {:?}", self.actions));
        let Action::SetTimer { timer, .. } = self.actions.remove(i) else {
            unreachable!()
        };
        self.now = at;
        self.e.handle(at, Event::TimerFired { timer });
        self.collect();
    }
    fn fills(&mut self) -> Vec<(f64, f64)> {
        let Some(frame) = self.e.take_frame(self.now) else {
            return vec![];
        };
        let Some((_, p)) = sections(frame).find(|(kind, _)| *kind == section::FILLS) else {
            return vec![];
        };
        let n = u32::from_le_bytes(p[..4].try_into().unwrap()) as usize;
        let start = (4 + n * 12 + 7) & !7;
        (0..n)
            .map(|i| {
                let at = start + i * 16;
                (
                    f64::from_le_bytes(p[at..at + 8].try_into().unwrap()),
                    f64::from_le_bytes(p[at + 8..at + 16].try_into().unwrap()),
                )
            })
            .collect()
    }
}
#[test]
fn futures_listen_key_subscriptions_ping_timestamp_and_keepalive() {
    let mut h = Harness::new();
    h.seed();
    h.e.handle(0.0, Event::WsOpened { socket: h.private });
    h.collect();
    let sent: Vec<_> = h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket, text } if *socket == h.private => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sent.len(), 3);
    assert!(sent.iter().all(|t| t.contains("user_")));
    h.ws(r#"{"event":"ping","ts":"1635221621062"}"#);
    assert!(h.actions.iter().any(
        |a| matches!(a,Action::WsSend{text,..}if text==r#"{"event":"pong","ts":"1635221621062"}"#)
    ));
    h.fire_at(1_800_000.0);
    let r = h.take("listenKey/futures-key");
    assert_eq!(r.method, Method::Put);
    assert!(r.url.starts_with("https://fapiws-auth.bitrue.com/"));
}
#[test]
fn private_socket_watchdog_recovers_without_a_spot_fallback() {
    let mut h = Harness::new();
    let old = h.private;
    h.fire_at(20_000.0);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==old))
    );
    h.fire_at(21_000.0);
    let r = h.take("listenKey");
    assert_eq!(
        r.url,
        "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey"
    );
    assert_eq!(r.method, Method::Post);
}
#[test]
fn account_positions_and_settings_are_authoritative_and_missing_schema_is_not_empty() {
    let mut h = Harness::new();
    h.reply("/account", POSITION);
    assert_eq!(h.e.store().positions()[0].contracts, 2.0);
    assert_eq!(h.e.store().positions()[0].liquidation_price, 50.0);
    let setting = h.e.store().setting("E-BTC-USDT").unwrap();
    assert_eq!(setting.leverage, 25.0);
    assert_eq!(setting.margin_type, Some(MarginType::Isolated));
    assert!(!h.e.store().hedged());
    assert_eq!(h.e.store().balance().used, 20.0);
    h.fire_at(2_000.0);
    h.reply("/account",r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":0,"accountNormal":0}]}}"#);
    assert_eq!(h.e.store().positions().len(), 1);
    assert_eq!(h.e.store().balance().total, 100.0);
    h.fire_at(4_000.0);
    h.reply("/account", EMPTY);
    assert!(h.e.store().positions().is_empty());
}
#[test]
fn a_newer_balance_event_invalidates_an_older_account_snapshot() {
    let mut h = Harness::new();
    h.ws(r#"{"e":"ACCOUNT_UPDATE"}"#);
    h.reply("/account", POSITION);
    assert!(h.e.store().positions().is_empty());
    h.reply("/account", EMPTY);
    assert_eq!(h.e.store().balance().total, 100.0);
    assert!(h.e.store().positions().is_empty());
}
#[test]
fn market_fills_have_stable_units_exact_ids_execution_prices_and_no_replays() {
    let mut h = Harness::new();
    h.seed();
    h.order(2, "PART_FILLED", "MARKET", 111);
    h.order(4, "PART_FILLED", "MARKET", 112);
    h.order(4, "PART_FILLED", "MARKET", 112);
    h.order(3, "PART_FILLED", "MARKET", 110);
    h.order(10, "FILLED", "MARKET", 113);
    h.order(10, "FILLED", "MARKET", 113);
    assert_eq!(h.fills(), vec![(111.0, 20.0), (112.0, 20.0), (113.0, 60.0)]);
    let order = &h.e.store().orders()[0];
    assert_eq!(order.id, "2334151155877155281");
    assert_eq!(order.filled, 100.0);
    assert_eq!(order.remaining, 0.0);
    assert_eq!(order.status, OrderStatus::Closed);
    assert_eq!(order.kind, OrderType::Market);
}
#[test]
fn partial_regressions_and_late_open_events_cannot_reopen_terminal_orders() {
    let mut h = Harness::new();
    h.seed();
    h.order(4, "PART_FILLED", "LIMIT", 111);
    h.order(2, "PART_FILLED", "LIMIT", 110);
    assert_eq!(h.e.store().orders()[0].filled, 40.0);
    assert_eq!(h.e.store().orders()[0].remaining, 60.0);
    h.order(4, "CANCELED", "LIMIT", 111);
    h.order(3, "NEW", "LIMIT", 111);
    assert_eq!(h.e.store().orders()[0].status, OrderStatus::Canceled);
    h.order(10, "FILLED", "LIMIT", 112);
    h.order(10, "CANCELED", "LIMIT", 112);
    assert_eq!(h.e.store().orders()[0].status, OrderStatus::Closed);
}
#[test]
fn an_inflight_order_snapshot_cannot_undo_stream_fills_and_reconciliation_cannot_resurrect() {
    let mut h = Harness::new();
    h.reply("/account", EMPTY);
    h.order(4, "PART_FILLED", "LIMIT", 111);
    h.reply("/openOrders", ORDER);
    assert_eq!(h.e.store().orders()[0].filled, 40.0);
    let retry = h.now + 500.0;
    h.fire_at(retry);
    h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
    assert!(h.e.store().orders().is_empty());
    h.order(2, "PART_FILLED", "LIMIT", 111);
    assert!(h.e.store().orders().is_empty());
    h.order(10, "FILLED", "LIMIT", 112);
    h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
    h.order(4, "NEW", "LIMIT", 111);
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn terminal_status_survives_empty_reconciliation_without_losing_closed_precedence() {
    for initial_status in ["FILLED", "CANCELED"] {
        let mut h = Harness::new();
        h.seed();
        h.order(10, initial_status, "LIMIT", 111);
        h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
        assert!(h.e.store().orders().is_empty());
        h.order(10, "CANCELED", "LIMIT", 111);
        if initial_status == "FILLED" {
            assert!(h.e.store().orders().is_empty());
        } else {
            assert_eq!(h.e.store().orders()[0].status, OrderStatus::Canceled);
        }
        h.order(10, "FILLED", "LIMIT", 111);
        assert_eq!(h.e.store().orders()[0].status, OrderStatus::Closed);
        h.order(10, "CANCELED", "LIMIT", 111);
        assert_eq!(h.e.store().orders()[0].status, OrderStatus::Closed);
        assert_eq!(h.fills(), vec![(111.0, 100.0)]);
    }
}
#[test]
fn progress_time_and_amount_survive_an_empty_authoritative_snapshot() {
    let mut h = Harness::new();
    h.seed();
    let event = |time, amount| {
        format!(
            r#"{{"e":"ORDER_TRADE_UPDATE","E":{time},"o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"{amount}","z":"4","L":"111"}}}}"#
        )
    };
    h.ws(&event(100, 100));
    h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
    assert!(h.e.store().orders().is_empty());
    h.ws(&event(50, 100));
    assert!(h.e.store().orders().is_empty());
    h.ws(&event(101, 80));
    let order = &h.e.store().orders()[0];
    assert_eq!(order.last_update, 101.0);
    assert_eq!(order.amount, 100.0);
    assert_eq!(order.filled, 40.0);
    assert_eq!(order.remaining, 60.0);
    assert_eq!(h.fills(), vec![(111.0, 40.0)]);
}
#[test]
fn malformed_order_snapshot_retains_the_complete_symbol() {
    let mut h = Harness::new();
    h.seed();
    h.order(1, "PART_FILLED", "LIMIT", 111);
    h.reply("/openOrders",r#"{"code":"0","data":[{"orderId":"bad","type":"LIMIT","side":"BUY","status":"NEW","origQty":100}]}"#);
    assert_eq!(h.e.store().orders().len(), 1);
    assert_eq!(h.e.store().orders()[0].filled, 10.0);
}
#[test]
fn conditional_type_and_atomic_attached_children_preserve_native_identity() {
    let mut h = Harness::new();
    h.reply("/account", EMPTY);
    h.reply("/openOrders",r#"{"code":"0","data":[{"orderId":"parent","clientOrderId":"parent-client","type":"LIMIT","side":"BUY","status":"NEW","action":"OPEN","price":100,"origQty":100,"executedQty":0,"childOrders":[{"orderId":2334151155877155282,"clientOrderId":2334151155877155283,"type":"MARKET","conditionOrder":true,"triggerType":1,"triggerPrice":90,"side":"BUY","status":"NEW","action":"OPEN","origQty":100,"executedQty":0}]}]}"#);
    let rows = h.e.store().orders();
    assert_eq!(rows.len(), 2);
    let child = &rows[1];
    assert_eq!(child.id, "2334151155877155282");
    assert_eq!(child.parent_id.as_deref(), Some("parent"));
    assert_eq!(child.kind, OrderType::StopLoss);
    assert_eq!(child.side, OrderSide::Sell);
    assert!(child.reduce_only);
    assert_eq!(child.amount, 100.0);
    assert_eq!(rows[0].stop_loss, Some(90.0));
}
#[test]
fn fetch_leverage_reads_account_metadata_without_any_order_or_leverage_probe() {
    let mut h = Harness::new();
    h.seed();
    h.e.handle(
        1.0,
        Event::Command(Command::FetchLeverage {
            command: 88,
            symbol: "E-BTC-USDT".into(),
        }),
    );
    h.collect();
    h.reply("/account", POSITION);
    assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command,result})if *command==88&&result.as_ref().is_ok_and(|s|s.contains("\"buyLeverage\":25.0")))));
    assert!(h.pending.iter().all(|r| r.method == Method::Get));
}

#[test]
fn cumulative_average_fallback_recovers_execution_price_without_replaying() {
    for first in [r#""ap":"100""#, r#""ap":"0","L":"100""#] {
        let mut h = Harness::new();
        h.seed();
        for (z, average) in [(1, 100), (3, 120), (3, 120)] {
            let fields = if z == 1 {
                first.to_owned()
            } else {
                format!("\"ap\":\"{average}\"")
            };
            h.ws(&format!(r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"{z}","p":"99",{fields}}}}}"#));
        }
        assert_eq!(h.fills(), vec![(100.0, 10.0), (130.0, 20.0)]);
    }
}

#[test]
fn private_deltas_preserve_authoritative_child_parent_direction_and_reference() {
    let mut h = Harness::new();
    h.reply("/account", EMPTY);
    h.reply("/openOrders", r#"{"code":"0","data":[{"orderId":"parent","type":"LIMIT","side":"BUY","status":"NEW","action":"OPEN","price":100,"origQty":100,"executedQty":0,"childOrders":[{"orderId":"child","type":"MARKET","conditionOrder":true,"triggerType":1,"triggerPrice":90,"triggerPriceType":1,"side":"BUY","status":"NEW","action":"OPEN","origQty":100,"executedQty":0}]}]}"#);
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":"child","S":"BUY","o":"MARKET","x":"NEW","X":"NEW","q":"100","z":"0","p":"0"}}"#);
    let child =
        h.e.store()
            .orders()
            .iter()
            .find(|o| o.id == "child")
            .unwrap();
    assert_eq!(child.parent_id.as_deref(), Some("parent"));
    assert_eq!(child.kind, OrderType::StopLoss);
    assert_eq!(child.side, OrderSide::Sell);
    assert_eq!(child.price, 90.0);
    assert!(child.reduce_only);
    assert_eq!(
        child.trigger_type,
        Some(safe_cex_engine::model::StopTriggerType::LastPrice)
    );
}

#[test]
fn rest_progress_reseeds_the_quote_basis_before_an_average_only_execution() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","ap":"100"}}"#);
    h.reply("/openOrders", r#"{"code":"0","data":[{"orderId":2334151155877155281,"type":"LIMIT","side":"BUY","status":"PART_FILLED","action":"OPEN","price":100,"origQty":100,"executedQty":30,"avgPrice":110}]}"#);
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"4","ap":"115"}}"#);
    assert_eq!(h.fills(), vec![(100.0, 10.0), (130.0, 10.0)]);
}
