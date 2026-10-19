//! Fixtures follow KuCoin Classic Futures documented account and private-order examples.
//! https://www.kucoin.com/docs-new/3470090w0 (Orders)
//! https://www.kucoin.com/docs-new/rest/futures-trading/orders/get-order-list
use safe_cex_engine::model::{MarginType, OrderSide, OrderType, PositionMode};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
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
                Action::WsConnect { socket, url } if url.contains("/private?") => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.event(Event::WsOpened { socket: h.socket });
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
