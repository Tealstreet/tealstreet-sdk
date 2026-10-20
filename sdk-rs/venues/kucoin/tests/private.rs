//! Fixtures follow KuCoin Classic Futures documented account and private-order examples.
//! https://www.kucoin.com/docs-new/3470090w0 (Orders)
//! https://www.kucoin.com/docs-new/rest/futures-trading/orders/get-order-list
use safe_cex_engine::model::{MarginType, OrderSide, OrderType, PositionMode, TimeInForce};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Response, SocketChannel, SocketId,
};
use safe_cex_venue_kucoin::{Kucoin, Options};
const MARKET: &str = r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"XBT","quoteCurrency":"USDT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"multiplier":0.001,"maxOrderQty":100000,"maxLeverage":100}]}"#;
const TICKERS: &str = r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100","bestBidPrice":"99","bestAskPrice":"101"}]}"#;
const EMPTY: &str =
    r#"{"code":"200000","data":{"currentPage":1,"totalPage":1,"totalNum":0,"items":[]}}"#;
const ORDER: &str = r#"{"id":"5cdfc138b21023a909e5ad55","symbol":"XBTUSDTM","type":"limit","status":"open","side":"buy","price":"90","size":10,"filledSize":2,"createdAt":1610000000000}"#;
const POSITION: &str = r#"{"symbol":"XBTUSDTM","currentQty":10,"avgEntryPrice":"90","markPrice":"100","leverage":"10","unrealisedPnl":"0.1","liquidationPrice":"70","marginMode":"CROSS","positionSide":"BOTH","positionMode":0,"currentTimestamp":1610000000000}"#;
struct Harness {
    engine: Engine<Kucoin>,
    actions: Vec<Action>,
    now: f64,
    socket: SocketId,
}
impl Harness {
    fn new() -> Self {
        let mut h = Self {
            engine: Engine::new(Kucoin::new(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                passphrase: Some("passphrase".into()),
                ..Default::default()
            })),
            actions: vec![],
            now: 1610000000000.0,
            socket: SocketId(0),
        };
        h.engine.start(h.now);
        h.collect();
        h.reply("/contracts/active", MARKET);
        h.reply("/allTickers", TICKERS);
        h.reply("/bullet-private",r#"{"code":"200000","data":{"token":"private-token","instanceServers":[{"endpoint":"wss://example.test/private","pingInterval":18000,"pingTimeout":10000}]}}"#);
        h.socket = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    binary,
                    socket,
                    url,
                    channel,
                    context,
                } if url.contains("/private?") => {
                    assert!(!binary, "KuCoin sockets use the text protocol");
                    assert_eq!(*channel, Some(SocketChannel::Private));
                    assert_eq!(
                        context.as_ref().expect("routing feed context").feed,
                        "private"
                    );
                    Some(*socket)
                }
                _ => None,
            })
            .unwrap();
        h.event(Event::WsOpened { socket: h.socket });
        h.event(Event::WsMessage {
            socket: h.socket,
            text: r#"{"type":"welcome"}"#,
        });
        h
    }
    fn collect(&mut self) {
        self.actions.extend(self.engine.drain_actions());
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn http(&mut self, path: &str) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r)if r.url.contains(path)))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.actions));
        match self.actions.remove(i) {
            Action::Http(r) => r,
            _ => unreachable!(),
        }
    }
    fn respond(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn reply(&mut self, path: &str, body: &str) {
        let r = self.http(path);
        self.respond(&r, 200, body);
    }
    fn ws(&mut self, topic: &str, data: &str) {
        let text = format!(r#"{{"type":"message","topic":"{topic}","data":{data}}}"#);
        self.event(Event::WsMessage {
            socket: self.socket,
            text: &text,
        });
    }
    fn page(rows: &str, total: u32) -> String {
        format!(
            r#"{{"code":"200000","data":{{"currentPage":1,"totalPage":{total},"totalNum":1,"items":[{rows}]}}}}"#
        )
    }
    fn ready(&mut self) {
        self.reply(
            "/positions",
            &format!(r#"{{"code":"200000","data":[{POSITION}]}}"#),
        );
        self.reply("/account-overview",r#"{"code":"200000","data":{"availableBalance":"950","orderMargin":"50","accountEquity":"1010","unrealisedPNL":"10"}}"#);
        self.reply(
            "/getPositionMode",
            r#"{"code":"200000","data":{"positionMode":0}}"#,
        );
        self.reply("/orders?", &Self::page(ORDER, 1));
        self.reply("/stopOrders?", EMPTY);
    }
    fn poll(&mut self) {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms==self.now+500.0))
            .unwrap();
        let Action::SetTimer { timer, at_ms } = self.actions.remove(i) else {
            unreachable!()
        };
        self.now = at_ms;
        self.event(Event::TimerFired { timer });
    }
    fn pace(&mut self) {
        self.advance(20.0);
    }
    fn advance(&mut self, delay: f64) {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == self.now + delay))
            .unwrap();
        let Action::SetTimer { timer, at_ms } = self.actions.remove(i) else {
            unreachable!()
        };
        self.now = at_ms;
        self.event(Event::TimerFired { timer });
    }
    fn fill_rows(&mut self) -> Vec<(f64, f64)> {
        let Some(frame) = self.engine.take_frame(self.now) else {
            return vec![];
        };
        let Some((_, bytes)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
        else {
            return vec![];
        };
        let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let offset = (4 + 12 * count).div_ceil(8) * 8;
        (0..count)
            .map(|i| {
                let at = offset + 16 * i;
                (
                    f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()),
                    f64::from_le_bytes(bytes[at + 8..at + 16].try_into().unwrap()),
                )
            })
            .collect()
    }
}
#[test]
fn native_contracts_become_base_and_zero_rows_only_populate_settings() {
    let mut h = Harness::new();
    h.ready();
    let p = &h.engine.store().positions()[0];
    assert_eq!((p.contracts, p.notional, p.leverage), (0.01, 1.0, 10.0));
    assert_eq!(p.id, "BTCUSDT");
    assert_eq!(p.position_mode, Some(PositionMode::Oneway));
    let o = &h.engine.store().orders()[0];
    assert_eq!((o.amount, o.filled, o.remaining), (0.01, 0.002, 0.008));
    assert_eq!(o.extra.as_ref().unwrap().remaining_native, Some(8.0));
    assert_eq!(
        (
            h.engine.store().balance().free,
            h.engine.store().balance().upnl
        ),
        (950.0, 10.0)
    );
    h.poll();
    h.reply(
        "/positions",
        &format!(
            r#"{{"code":"200000","data":[{}]}}"#,
            POSITION
                .replace("\"currentQty\":10", "\"currentQty\":0")
                .replace("1610000000000", "1610000000500")
        ),
    );
    assert!(h.engine.store().positions().is_empty());
    let s = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!((s.leverage, s.margin_type), (10.0, Some(MarginType::Cross)));
}
#[test]
fn fills_use_incremental_lots_and_execution_price_once() {
    let mut h = Harness::new();
    h.ready();
    h.engine.take_frame(h.now);
    let matched = r#"{"orderId":"5cdfc138b21023a909e5ad55","symbol":"XBTUSDTM","type":"match","orderType":"limit","status":"match","side":"buy","price":"90","size":10,"filledSize":3,"remainSize":7,"matchSize":1,"matchPrice":"89.5","tradeId":"1794175373644","ts":1610000000001000000}"#;
    h.ws("/contractMarket/tradeOrders", matched);
    assert_eq!(h.fill_rows(), vec![(89.5, 0.001)]);
    h.ws("/contractMarket/tradeOrders", matched);
    assert!(h.fill_rows().is_empty());
    let terminal = matched
        .replace("\"type\":\"match\"", "\"type\":\"filled\"")
        .replace("\"matchSize\":1,", "")
        .replace("\"remainSize\":7", "\"remainSize\":0");
    h.ws("/contractMarket/tradeOrders", &terminal);
    assert!(h.fill_rows().is_empty());
    assert!(h.engine.store().orders().is_empty());
}
#[test]
fn inflight_and_cached_snapshots_cannot_resurrect_terminal_orders() {
    let mut h = Harness::new();
    h.ready();
    h.poll();
    let inflight = h.http("/orders?");
    h.ws(
        "/contractMarket/tradeOrders",
        &ORDER
            .replace("\"id\":", "\"orderId\":")
            .replace("\"type\":\"limit\"", "\"type\":\"canceled\"")
            .replace("\"status\":\"open\"", "\"status\":\"canceled\""),
    );
    h.respond(&inflight, 200, &Harness::page(ORDER, 1));
    assert!(h.engine.store().orders().is_empty());
    h.reply("/positions", r#"{"code":"200000","data":[]}"#);
    h.reply("/stopOrders?", EMPTY);
    h.poll();
    h.reply("/orders?", &Harness::page(ORDER, 1));
    assert!(h.engine.store().orders().is_empty());
}
#[test]
fn failed_stop_book_preserves_its_orders_and_virtual_parent_closure_removes_legs() {
    let mut h = Harness::new();
    h.ready();
    let stop = r#"{"orderId":"protect","symbol":"XBTUSDTM","type":"open","orderType":"market","status":"open","side":"sell","stop":"down","stopPrice":"80","stopPriceType":"MP","size":10,"reduceOnly":true}"#;
    h.ws("/contractMarket/advancedOrders", stop);
    h.poll();
    h.reply("/orders?", EMPTY);
    for _ in 0..3 {
        h.reply("/stopOrders?", r#"{"code":"429000","msg":"rate limit"}"#);
    }
    assert_eq!(
        h.engine.store().order("protect").unwrap().kind,
        OrderType::StopLoss
    );
    let attached = ORDER.replace(
        "\"createdAt\"",
        "\"triggerStopDownPrice\":80,\"triggerStopUpPrice\":120,\"createdAt\"",
    );
    h.ws("/contractMarket/tradeOrders", &attached);
    assert!(
        h.engine
            .store()
            .order("5cdfc138b21023a909e5ad55__stop_loss")
            .is_some()
    );
    h.ws(
        "/contractMarket/tradeOrders",
        &ORDER.replace("\"type\":\"limit\"", "\"type\":\"canceled\""),
    );
    assert!(
        h.engine
            .store()
            .order("5cdfc138b21023a909e5ad55__stop_loss")
            .is_none()
    );
}
#[test]
fn multi_page_normal_book_commits_only_after_complete_success() {
    let mut h = Harness::new();
    h.ready();
    h.poll();
    h.reply(
        "/orders?",
        &Harness::page(&ORDER.replace("ad55", "ad56"), 2),
    );
    assert!(h.engine.store().order("5cdfc138b21023a909e5ad55").is_some());
    h.reply("currentPage=2",r#"{"code":"200000","data":{"currentPage":2,"totalPage":2,"totalNum":2,"items":[{"id":"next","symbol":"XBTUSDTM","type":"limit","side":"sell","size":1,"price":"110"}]}}"#);
    assert!(h.engine.store().order("next").is_some());
    assert!(h.engine.store().order("5cdfc138b21023a909e5ad55").is_none());
}
#[test]
fn standalone_stop_direction_accounts_for_side_and_keeps_trigger_basis() {
    let mut h = Harness::new();
    h.ready();
    let stop = r#"{"orderId":"short-protect","symbol":"XBTUSDTM","type":"open","orderType":"market","status":"open","side":"buy","stop":"up","stopPrice":"120","stopPriceType":"MP","closeOrder":true}"#;
    h.ws("/contractMarket/advancedOrders", stop);
    let o = h.engine.store().order("short-protect").unwrap();
    assert_eq!(
        (o.kind, o.side, o.close),
        (OrderType::StopLoss, OrderSide::Buy, Some(true))
    );
    assert_eq!(
        o.trigger_type,
        Some(safe_cex_engine::model::StopTriggerType::MarkPrice)
    );
}
#[test]
fn failed_positions_snapshot_never_becomes_empty() {
    let mut h = Harness::new();
    h.ready();
    h.poll();
    for _ in 0..3 {
        h.reply("/positions", r#"{"code":"429000","msg":"rate limit"}"#);
    }
    assert_eq!(h.engine.store().positions().len(), 1);
}

#[test]
fn closed_positions_reject_late_stream_rows_and_cached_rest_keeps_reopened_exposure() {
    let mut h = Harness::new();
    h.ready();
    h.now += 1000.0;
    let closed = POSITION
        .replace("\"currentQty\":10", "\"currentQty\":0")
        .replace("1610000000000", "1610000001000");
    h.ws("/contract/positionAll", &closed);
    assert!(h.engine.store().positions().is_empty());
    h.ws("/contract/positionAll", POSITION);
    assert!(h.engine.store().positions().is_empty());
    h.now += 1000.0;
    let reopened = POSITION
        .replace("\"currentQty\":10", "\"currentQty\":20")
        .replace("1610000000000", "1610000002000");
    h.ws("/contract/positionAll", &reopened);
    assert_eq!(h.engine.store().positions()[0].contracts, 0.02);
    let (i, timer) = h
        .actions
        .iter()
        .enumerate()
        .find_map(|(i, a)| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1610000000500.0 => Some((i, *timer)),
            _ => None,
        })
        .unwrap();
    h.actions.remove(i);
    h.event(Event::TimerFired { timer });
    h.reply(
        "/positions",
        &format!(r#"{{"code":"200000","data":[{POSITION}]}}"#),
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 0.02);
    assert_eq!(
        h.engine.store().positions()[0].last_update,
        Some(1610000002000.0)
    );
}
#[test]
fn late_mode_snapshot_does_not_rewind_newer_private_position_mode() {
    let mut h = Harness::new();
    h.ws(
        "/contract/positionAll",
        &POSITION
            .replace("\"positionSide\":\"BOTH\"", "\"positionSide\":\"LONG\"")
            .replace("\"positionMode\":0", "\"positionMode\":1"),
    );
    assert!(h.engine.store().hedged());
    h.reply(
        "/getPositionMode",
        r#"{"code":"200000","data":{"positionMode":0}}"#,
    );
    assert!(h.engine.store().hedged());
}

#[test]
fn incomplete_page_census_cannot_replace_existing_open_orders() {
    let mut h = Harness::new();
    h.ready();
    h.poll();
    h.reply(
        "/orders?",
        &Harness::page(&ORDER.replace("ad55", "ad56"), 1)
            .replace("\"totalNum\":1", "\"totalNum\":2"),
    );
    assert!(h.engine.store().order("5cdfc138b21023a909e5ad55").is_some());
    assert!(h.engine.store().order("5cdfc138b21023a909e5ad56").is_none());
}

#[test]
fn documented_partial_position_change_preserves_risk_and_settings() {
    // https://www.kucoin.com/en-au/docs-new/3470093w0 isolated position.change.
    let mut h = Harness::new();
    h.ready();
    let before = h.engine.store().positions()[0].clone();
    let partial = r#"{"symbol":"XBTUSDTM","currentQty":20,"currentTimestamp":1610000000500,"crossMode":true,"changeReason":"positionChange","unrealisedPnl":"0.2"}"#;
    h.ws("/contract/positionAll", partial);
    let after = &h.engine.store().positions()[0];
    assert_eq!(
        (after.entry_price, after.leverage, after.liquidation_price),
        (
            before.entry_price,
            before.leverage,
            before.liquidation_price
        )
    );
    assert_eq!(
        (after.contracts, after.notional, after.unrealized_pnl),
        (0.02, 2.0, 0.2)
    );
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    h.ws("/contract/positionAll",r#"{"symbol":"ETHUSDTM","currentQty":2,"currentTimestamp":1610000000500,"crossMode":false,"changeReason":"positionChange"}"#);
    assert!(
        h.engine
            .store()
            .positions()
            .iter()
            .all(|p| p.symbol != "ETHUSDT")
    );
}

#[test]
fn decimal_contract_multipliers_do_not_leak_binary_lot_noise() {
    let mut h = Harness::new();
    h.ready();
    let mut market = h.engine.store().markets()[0].clone();
    market.contract_size = Some(0.1);
    h.engine.store_mut().set_markets(vec![market]);
    h.ws("/contractMarket/tradeOrders",r#"{"orderId":"decimal-lots","symbol":"XBTUSDTM","type":"match","orderType":"limit","status":"match","side":"buy","size":7,"filledSize":3,"remainSize":4,"matchSize":3,"matchPrice":"90","tradeId":"decimal-fill"}"#);
    let o = h.engine.store().order("decimal-lots").unwrap();
    assert_eq!((o.amount, o.filled, o.remaining), (0.7, 0.3, 0.4));
    assert_eq!(h.fill_rows(), vec![(90.0, 0.3)]);
}

#[test]
fn documented_partial_order_events_preserve_safety_fields_through_amend() {
    for tif in [
        TimeInForce::PostOnly,
        TimeInForce::ImmediateOrCancel,
        TimeInForce::FillOrKill,
    ] {
        let mut h = Harness::new();
        h.ready();
        let id = "5cdfc138b21023a909e5ad55";
        h.engine.store_mut().update_order(id, |order| {
            order.reduce_only = true;
            order.close = Some(false);
            order.time_in_force = Some(tif);
            order.client_order_id = Some("caller-id".into());
        });
        h.now += 1.0;
        h.ws("/contractMarket/tradeOrders", r#"{"orderId":"5cdfc138b21023a909e5ad55","symbol":"XBTUSDTM","orderType":"limit","side":"buy","type":"match","status":"open","size":"10","filledSize":"3","remainSize":"7","price":"90","matchPrice":"90","matchSize":"1","tradeId":"trade-3","ts":1610000000001000000}"#);
        let order = h.engine.store().order(id).unwrap();
        assert!(order.reduce_only);
        assert_eq!(order.close, Some(false));
        assert_eq!(order.time_in_force, Some(tif));
        assert_eq!(order.client_order_id.as_deref(), Some("caller-id"));
        assert_eq!((order.filled, order.remaining), (0.003, 0.007));
        h.event(Event::Command(Command::AmendOrder {
            command: 1,
            id: id.into(),
            price: Some(91.0),
            amount: None,
        }));
        h.reply(
            &format!("/orders/{id}"),
            r#"{"code":"200000","data":{"cancelledOrderIds":["5cdfc138b21023a909e5ad55"]}}"#,
        );
        h.pace();
        h.reply(&format!("/orders/{id}"), r#"{"code":"200000","data":{"id":"5cdfc138b21023a909e5ad55","isActive":false,"cancelExist":true,"size":10,"filledSize":3}}"#);
        h.pace();
        let body = h.http("/api/v1/orders").body.unwrap();
        assert!(body.contains(r#""reduceOnly":true"#), "{body}");
        assert!(body.contains(r#""size":"7""#), "{body}");
        let expected = match tif {
            TimeInForce::PostOnly => r#""postOnly":true"#,
            TimeInForce::ImmediateOrCancel => r#""timeInForce":"IOC""#,
            _ => r#""timeInForce":"FOK""#,
        };
        assert!(body.contains(expected), "{body}");
        assert_eq!(h.fill_rows(), vec![(90.0, 0.001)]);
    }
}

#[test]
fn unknown_partial_order_requires_rest_metadata_before_amending() {
    let mut h = Harness::new();
    h.ready();
    h.ws("/contractMarket/tradeOrders", r#"{"orderId":"unknown","symbol":"XBTUSDTM","side":"sell","type":"open","status":"open","size":"10","filledSize":"0","remainSize":"10","price":"100","ts":1610000000001000000}"#);
    h.event(Event::Command(Command::AmendOrder {
        command: 9,
        id: "unknown".into(),
        price: Some(101.0),
        amount: None,
    }));
    assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::CommandDone {command: 9, result: Err(error)}) if error.contains("metadata unavailable"))));
    assert!(!h.actions.iter().any(|action| matches!(action, Action::Http(request) if request.method == safe_cex_engine::Method::Delete)));
    h.poll();
    h.reply("/orders?", &Harness::page(r#"{"id":"unknown","symbol":"XBTUSDTM","type":"limit","side":"sell","size":10,"filledSize":0,"price":"100","reduceOnly":true,"closeOrder":false,"timeInForce":"GTC","postOnly":true}"#, 1));
    h.event(Event::Command(Command::AmendOrder {
        command: 10,
        id: "unknown".into(),
        price: Some(101.0),
        amount: None,
    }));
    assert_eq!(
        h.http("/orders/unknown").method,
        safe_cex_engine::Method::Delete
    );
}

#[test]
fn full_position_stop_keeps_close_flag_after_documented_partial_event() {
    let mut h = Harness::new();
    h.ready();
    h.poll();
    h.reply("/stopOrders?", &Harness::page(r#"{"id":"full-stop","symbol":"XBTUSDTM","type":"market","side":"sell","stop":"down","stopPrice":"80","stopPriceType":"MP","size":0,"filledSize":0,"closeOrder":true,"reduceOnly":true}"#, 1));
    h.reply("/orders?", &Harness::page(ORDER, 1));
    h.reply(
        "/positions",
        &format!(r#"{{"code":"200000","data":[{POSITION}]}}"#),
    );
    h.ws("/contractMarket/advancedOrders", r#"{"orderId":"full-stop","symbol":"XBTUSDTM","orderType":"stop","side":"sell","stop":"down","stopPrice":"80","stopPriceType":"MP","size":0,"type":"open","ts":1610000000500000000}"#);
    assert_eq!(
        h.engine.store().order("full-stop").unwrap().close,
        Some(true)
    );
    h.event(Event::Command(Command::AmendOrder {
        command: 1,
        id: "full-stop".into(),
        price: Some(81.0),
        amount: None,
    }));
    h.reply(
        "/orders/full-stop",
        r#"{"code":"200000","data":{"cancelledOrderIds":["full-stop"]}}"#,
    );
    h.pace();
    h.reply("/orders/full-stop", r#"{"code":"200000","data":{"id":"full-stop","isActive":false,"cancelExist":true,"size":0,"filledSize":0}}"#);
    h.pace();
    let body = h.http("/api/v1/orders").body.unwrap();
    assert!(body.contains(r#""closeOrder":true"#), "{body}");
    assert!(body.contains(r#""reduceOnly":true"#), "{body}");
    assert!(!body.contains(r#""size""#), "{body}");
}

#[test]
fn documented_settings_events_update_flat_symbols_and_fence_pending_metadata() {
    let mut h = Harness::new();
    h.ready();
    h.engine.store_mut().set_positions(vec![]);
    h.ws(
        "/contract/crossLeverage",
        r#"{"XBTUSDTM":{"leverage":"51"}}"#,
    );
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 51.0);
    h.event(Event::Command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":90}"#,
            )
            .unwrap(),
        ],
    }));
    let request = h.http("/api/v1/orders");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains(r#""leverage":51"#)
    );
    h.respond(
        &request,
        200,
        r#"{"code":"200000","data":{"orderId":"created"}}"#,
    );
    h.pace();
    h.engine
        .store_mut()
        .update_setting("BTCUSDT", |setting| setting.leverage = f64::NAN);
    h.event(Event::Command(Command::PlaceOrders {
        command: 2,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":90}"#,
            )
            .unwrap(),
        ],
    }));
    let stale = h.http("getMarginMode");
    h.ws("/contract/marginMode", r#"{"XBTUSDTM":"ISOLATED"}"#);
    h.respond(
        &stale,
        200,
        r#"{"code":"200000","data":{"marginMode":"CROSS"}}"#,
    );
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    h.reply(
        "getMarginMode",
        r#"{"code":"200000","data":{"marginMode":"ISOLATED"}}"#,
    );
    h.pace();
    let body = h.http("/api/v1/orders").body.unwrap();
    assert!(body.contains(r#""marginMode":"ISOLATED""#), "{body}");
    assert!(body.contains(r#""leverage":1"#), "{body}");
}

#[test]
fn triggered_limit_stop_can_migrate_after_stop_snapshot_absence() {
    let mut h = Harness::new();
    h.ready();
    h.poll();
    let stop = r#"{"id":"migrating","symbol":"XBTUSDTM","type":"limit","side":"sell","stop":"down","stopPrice":"80","price":"79","size":10,"filledSize":0}"#;
    h.reply("/stopOrders?", &Harness::page(stop, 1));
    h.reply("/orders?", EMPTY);
    h.reply("/positions", r#"{"code":"200000","data":[]}"#);
    h.poll();
    h.reply("/stopOrders?", EMPTY);
    let active = stop.replace(r#""stop":"down""#, r#""stop":"""#);
    h.reply("/orders?", &Harness::page(&active, 1));
    assert!(h.engine.store().order("migrating").is_none());
    h.reply("/positions", r#"{"code":"200000","data":[]}"#);
    h.poll();
    h.reply("/orders?", &Harness::page(&active, 1));
    assert_eq!(
        h.engine.store().order("migrating").unwrap().kind,
        OrderType::Limit
    );
    h.reply("/stopOrders?", EMPTY);
    assert!(h.engine.store().order("migrating").is_some());
}

#[test]
fn successful_own_placement_hydrates_partial_events_on_either_side_of_the_response() {
    for stream_first in [false, true] {
        let mut h = Harness::new();
        h.ready();
        h.event(Event::Command(Command::PlaceOrders { command: 1, orders: vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"sell","amount":10,"price":100,"reduceOnly":true,"timeInForce":"PostOnly","clientOrderId":"caller-id"}"#).unwrap()] }));
        let placement = h.http("/api/v1/orders");
        let partial = r#"{"orderId":"own","symbol":"XBTUSDTM","side":"sell","orderType":"limit","type":"match","status":"open","size":10,"filledSize":1,"remainSize":9,"price":"100","matchSize":1,"matchPrice":"100","tradeId":"own-trade","ts":1610000000001000000}"#;
        if stream_first {
            h.ws("/contractMarket/tradeOrders", partial);
        }
        h.respond(
            &placement,
            200,
            r#"{"code":"200000","data":{"orderId":"own"}}"#,
        );
        if !stream_first {
            h.ws("/contractMarket/tradeOrders", partial);
        }
        h.pace();
        let order = h.engine.store().order("own").unwrap();
        assert!(order.reduce_only);
        assert_eq!(order.close, Some(false));
        assert_eq!(order.time_in_force, Some(TimeInForce::PostOnly));
        assert_eq!(order.client_order_id.as_deref(), Some("caller-id"));
        assert_eq!((order.filled, order.remaining), (0.001, 0.009));
        h.event(Event::Command(Command::AmendOrder {
            command: 2,
            id: "own".into(),
            price: Some(101.0),
            amount: None,
        }));
        assert_eq!(
            h.http("/orders/own").method,
            safe_cex_engine::Method::Delete
        );
        assert_eq!(h.fill_rows(), vec![(100.0, 0.001)]);
    }
}

#[test]
fn leverage_stream_fences_an_inflight_flat_symbol_leverage_read() {
    let mut h = Harness::new();
    h.ready();
    h.engine.store_mut().set_positions(vec![]);
    h.engine
        .store_mut()
        .update_setting("BTCUSDT", |setting| setting.leverage = f64::NAN);
    h.event(Event::Command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":90}"#,
            )
            .unwrap(),
        ],
    }));
    h.reply(
        "getMarginMode",
        r#"{"code":"200000","data":{"marginMode":"CROSS"}}"#,
    );
    let stale = h.http("getCrossUserLeverage");
    h.ws(
        "/contract/crossLeverage",
        r#"{"XBTUSDTM":{"leverage":"51"}}"#,
    );
    h.respond(&stale, 200, r#"{"code":"200000","data":{"leverage":5}}"#);
    h.pace();
    let body = h.http("/api/v1/orders").body.unwrap();
    assert!(body.contains(r#""leverage":51"#), "{body}");
}

fn amend_own_bracket(reconcile: bool) {
    let mut h = Harness::new();
    h.ready();
    h.event(Event::Command(Command::PlaceOrders { command: 1, orders: vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":10,"price":100,"stopLoss":90,"takeProfit":110,"slTriggerType":"mark_price","tpTriggerType":"mark_price"}"#).unwrap()] }));
    h.reply(
        "/api/v1/st-orders",
        r#"{"code":"200000","data":{"orderId":"own-bracket"}}"#,
    );
    h.ws("/contractMarket/tradeOrders", r#"{"orderId":"own-bracket","symbol":"XBTUSDTM","side":"buy","orderType":"limit","type":"open","status":"open","size":10,"filledSize":1,"remainSize":9,"price":"100","ts":1610000000001000000}"#);
    h.pace();
    if reconcile {
        let index = h.actions.iter().position(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms == 1610000000500.0)).unwrap();
        let Action::SetTimer { timer, at_ms } = h.actions.remove(index) else {
            unreachable!()
        };
        h.now = at_ms;
        h.event(Event::TimerFired { timer });
        h.reply("/positions", r#"{"code":"200000","data":[]}"#);
        h.reply("/orders?", &Harness::page(r#"{"id":"own-bracket","symbol":"XBTUSDTM","type":"limit","side":"buy","size":10,"filledSize":1,"price":"100","createdAt":1610000000000,"reduceOnly":false,"closeOrder":false,"timeInForce":"GTC","postOnly":false}"#, 1));
        h.reply("/stopOrders?", EMPTY);
    }
    h.event(Event::Command(Command::AmendOrder {
        command: 2,
        id: "own-bracket".into(),
        price: Some(101.0),
        amount: None,
    }));
    h.reply(
        "/orders/own-bracket",
        r#"{"code":"200000","data":{"cancelledOrderIds":["own-bracket"]}}"#,
    );
    h.pace();
    h.reply("/orders/own-bracket", r#"{"code":"200000","data":{"id":"own-bracket","isActive":false,"cancelExist":true,"size":10,"filledSize":1}}"#);
    h.pace();
    let body = h.http("/api/v1/st-orders").body.unwrap();
    for expected in [
        r#""size":"9""#,
        r#""triggerStopDownPrice":"90""#,
        r#""triggerStopUpPrice":"110""#,
        r#""stopPriceType":"MP""#,
    ] {
        assert!(body.contains(expected), "{body}");
    }
}

#[test]
fn own_bracket_amend_before_reconciliation_retains_both_protections() {
    amend_own_bracket(false);
}

#[test]
fn own_bracket_amend_after_delayed_rest_retains_both_protections() {
    amend_own_bracket(true);
}

#[test]
fn cancellation_proof_cannot_lag_newer_open_or_terminal_stream_fills() {
    for terminal in [false, true] {
        let mut h = Harness::new();
        h.ready();
        let id = "5cdfc138b21023a909e5ad55";
        h.event(Event::Command(Command::AmendOrder {
            command: 1,
            id: id.into(),
            price: Some(91.0),
            amount: None,
        }));
        h.reply(
            &format!("/orders/{id}"),
            r#"{"code":"200000","data":{"cancelledOrderIds":["5cdfc138b21023a909e5ad55"]}}"#,
        );
        h.pace();
        let stale = h.http(&format!("/orders/{id}"));
        let event = if terminal { "canceled" } else { "match" };
        h.ws("/contractMarket/tradeOrders", &format!(r#"{{"orderId":"{id}","symbol":"XBTUSDTM","side":"buy","orderType":"limit","type":"{event}","status":"open","size":10,"filledSize":6,"remainSize":4,"price":"90","ts":1610000000021000000}}"#));
        if terminal {
            assert!(h.engine.store().order(id).is_none());
        }
        h.respond(&stale, 200, &format!(r#"{{"code":"200000","data":{{"id":"{id}","isActive":false,"cancelExist":true,"size":10,"filledSize":4}}}}"#));
        assert!(!h.actions.iter().any(|action| matches!(action, Action::Http(request) if request.method == safe_cex_engine::Method::Post && request.url.contains("/api/v1/orders"))));
        h.advance(100.0);
        h.reply(&format!("/orders/{id}"), &format!(r#"{{"code":"200000","data":{{"id":"{id}","isActive":false,"cancelExist":true,"size":10,"filledSize":6}}}}"#));
        h.pace();
        let body = h.http("/api/v1/orders").body.unwrap();
        assert!(body.contains(r#""size":"4""#), "{body}");
    }
}

#[test]
fn newer_terminal_fills_after_confirmation_abort_the_queued_replacement() {
    let mut h = Harness::new();
    h.ready();
    let id = "5cdfc138b21023a909e5ad55";
    h.event(Event::Command(Command::AmendOrder {
        command: 1,
        id: id.into(),
        price: Some(91.0),
        amount: None,
    }));
    h.reply(
        &format!("/orders/{id}"),
        r#"{"code":"200000","data":{"cancelledOrderIds":["5cdfc138b21023a909e5ad55"]}}"#,
    );
    h.pace();
    h.reply(&format!("/orders/{id}"), &format!(r#"{{"code":"200000","data":{{"id":"{id}","isActive":false,"cancelExist":true,"size":10,"filledSize":4}}}}"#));
    h.ws("/contractMarket/tradeOrders", &format!(r#"{{"orderId":"{id}","symbol":"XBTUSDTM","side":"buy","orderType":"limit","type":"canceled","size":10,"filledSize":6,"remainSize":0,"price":"90","ts":1610000000021000000}}"#));
    h.pace();
    assert!(!h.actions.iter().any(|action| matches!(action, Action::Http(request) if request.method == safe_cex_engine::Method::Post && request.url.contains("/api/v1/orders"))));
    h.pace();
    assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::CommandDone { command: 1, result: Err(error) }) if error.contains("newer fills"))));
}

#[test]
fn account_reconciliation_starts_without_the_private_socket() {
    let mut h = Harness {
        engine: Engine::new(Kucoin::new(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            ..Default::default()
        })),
        actions: vec![],
        now: 1610000000000.0,
        socket: SocketId(0),
    };
    h.engine.start(h.now);
    h.collect();
    h.reply("/contracts/active", MARKET);
    h.reply("/allTickers", TICKERS);
    let bullet = h.http("/bullet-private");
    h.respond(&bullet, 503, "unavailable");
    h.ready();
    assert!(h.engine.store().loaded().positions);
    assert!(h.engine.store().loaded().orders);
}
