mod support;
use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus, OrderType};
use safe_cex_engine::wire::{section, sections};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, SocketChannel, SocketId,
};
use safe_cex_venue_bitrue::Bitrue;
use support::Options;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

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
    order_z: u32,
}
impl Harness {
    fn new() -> Self {
        Self::with_market(MARKET)
    }
    fn with_market(market: &str) -> Self {
        let mut e = support::engine(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Options::default()
        });
        e.start(0.0);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            private: SocketId(0),
            now: 0.0,
            order_z: 0,
        };
        h.collect();
        h.reply("/contracts", market);
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
                    channel,
                    ..
                } if url.contains("fapiws.bitrue.com") => {
                    assert_eq!(url, "wss://fapiws.bitrue.com/stream?listenKey=futures-key");
                    assert_eq!(channel.as_ref(), Some(&SocketChannel::Private));
                    Some(*socket)
                }
                _ => None,
            })
            .unwrap();
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now).into_iter()
        {
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
        let last = z.saturating_sub(self.order_z).max(1).min(z);
        self.order_z = self.order_z.max(z);
        self.ws(&format!(r#"{{"e":"ORDER_TRADE_UPDATE","E":{},"o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"{kind}","x":"TRADE","X":"{status}","q":"100","z":"{z}","l":"{last}","p":"100","ap":"101","L":"{price}"}}}}"#,self.now+1.0));
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
    fn private_retry(&mut self, nominal: f64) {
        // Follow the supervisor's actual jittered retry rather than the retired
        // fixed listen-key retry schedule. Account polling timers stay separate.
        self.fire_retry(nominal);
    }
    /// Fires the newest timer jittered from `nominal` after now; returns its delay.
    fn fire_retry(&mut self, nominal: f64) -> f64 {
        let at = self
            .actions
            .iter()
            .rev()
            .find_map(|action| match action {
                Action::SetTimer { at_ms, .. } if jittered(*at_ms - self.now, nominal) => {
                    Some(*at_ms)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no retry jittered from {nominal}: {:?}", self.actions));
        let delay = at - self.now;
        self.fire_at(at);
        delay
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

/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
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
    h.e.handle(h.now, Event::WsOpened { socket: old });
    h.collect();
    for at in [5000.0, 10_000.0, 15_000.0, 20_000.0] {
        h.fire_at(at);
    }
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==old))
    );
    let at = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { at_ms, .. } if *at_ms > h.now && *at_ms <= h.now + 1000.0 => {
                Some(*at_ms)
            }
            _ => None,
        })
        .unwrap();
    h.fire_at(at);
    let r = h.take("listenKey");
    assert_eq!(
        r.url,
        "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey"
    );
    assert_eq!(r.method, Method::Post);
    h.respond(r, r#"{"code":200,"data":{"listenKey":"replacement-key"}}"#);
    assert!(h.actions.iter().any(|action| matches!(
        action,
        Action::WsConnect { socket, url, channel: Some(SocketChannel::Private), .. }
            if *socket != old && url == "wss://fapiws.bitrue.com/stream?listenKey=replacement-key"
    )));
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
fn steady_balance_events_cannot_starve_the_account_snapshot() {
    let mut h = Harness::new();
    for _ in 0..2 {
        h.ws(r#"{"e":"ACCOUNT_UPDATE"}"#);
        h.reply("/account", POSITION);
        assert!(h.e.store().positions().is_empty());
    }
    h.ws(r#"{"e":"ACCOUNT_UPDATE"}"#);
    h.reply("/account", POSITION);
    assert_eq!(h.e.store().positions().len(), 1);
    // The stale commit is re-read at once.
    h.reply("/account", EMPTY);
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
            r#"{{"e":"ORDER_TRADE_UPDATE","E":{time},"o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"{amount}","z":"4","l":"4","L":"111"}}}}"#
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
impl Harness {
    /// Reopens the private stream, which re-reads the account and open orders; returns its logs.
    fn reread(&mut self, account: Option<&str>, orders: Option<&str>) -> Vec<String> {
        self.actions.clear();
        self.e.handle(
            self.now,
            Event::WsOpened {
                socket: self.private,
            },
        );
        self.collect();
        if let Some(body) = account {
            self.reply("/account", body);
        }
        if let Some(body) = orders {
            self.reply("/openOrders", body);
        }
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) => Some(event.message.clone()),
                _ => None,
            })
            .collect()
    }
}
#[test]
fn one_malformed_order_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let order = |id: &str, side: &str, price: u32| {
        format!(
            r#"{{"orderId":"{id}","type":"LIMIT","side":"{side}","status":"NEW","action":"OPEN","price":{price},"origQty":100,"executedQty":0,"transactTime":1}}"#
        )
    };
    let list = |rows: &[String]| format!(r#"{{"code":"0","data":[{}]}}"#, rows.join(","));
    let mut h = Harness::new();
    h.reply("/account", EMPTY);
    h.reply(
        "/openOrders",
        &list(&[order("a", "BUY", 100), order("b", "SELL", 120)]),
    );
    let stored = h.e.store().order("b").unwrap().clone();
    let snapshot = list(&[order("a", "BUY", 101), order("b", "HOLD", 999)]);
    for round in 0..2 {
        let logs = h.reread(None, Some(&snapshot));
        assert_eq!(h.e.store().order("a").unwrap().price, 101.0);
        assert_eq!(h.e.store().order("b"), Some(&stored));
        let skips: Vec<_> = logs.iter().filter(|l| l.contains("skipped")).collect();
        let expected: &[&str] = if round == 0 {
            &["Bitrue skipped malformed order row b"]
        } else {
            &[]
        };
        assert_eq!(skips, expected, "{logs:?}");
        assert!(!logs.iter().any(|l| l.contains("HOLD") || l.contains("999")));
    }
    h.reread(None, Some(&list(&[order("a", "BUY", 101)])));
    let ids: Vec<_> = h.e.store().orders().iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["a"]);
}
#[test]
fn a_malformed_child_without_an_id_keeps_the_cached_protective_child() {
    let child = |id: &str| {
        format!(
            r#"{{{id}"type":"MARKET","conditionOrder":true,"triggerType":1,"triggerPrice":90,"triggerPriceType":1,"side":"BUY","status":"NEW","action":"OPEN","origQty":100,"executedQty":0}}"#
        )
    };
    let tree = |children: &str| {
        format!(
            r#"{{"code":"0","data":[{{"orderId":"parent","type":"LIMIT","side":"BUY","status":"NEW","action":"OPEN","price":100,"origQty":100,"executedQty":0,"childOrders":[{children}]}}]}}"#
        )
    };
    let mut h = Harness::new();
    h.reply("/account", EMPTY);
    h.reply("/openOrders", &tree(&child(r#""orderId":"child","#)));
    let stored = h.e.store().order("child").unwrap().clone();
    assert_eq!(stored.parent_id.as_deref(), Some("parent"));
    let malformed = child("").replace(r#""side":"BUY""#, r#""side":"HOLD""#);
    let logs = h.reread(None, Some(&tree(&malformed)));
    assert_eq!(h.e.store().order("child"), Some(&stored));
    assert!(h.e.store().order("parent").is_some());
    assert!(
        logs.iter()
            .any(|l| l.contains("Bitrue skipped malformed order row parent")),
        "{logs:?}"
    );
}
#[test]
fn one_malformed_position_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let markets = MARKET.trim_end_matches(']').to_owned()
        + r#",{"symbol":"E-ETH-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
    let group = |contract: &str, side: &str, volume: u32| {
        format!(
            r#"{{"contractName":"{contract}","positions":[{{"side":"{side}","volume":{volume},"openPrice":100,"leverageLevel":25,"positionType":2,"positionBalance":200}}]}}"#
        )
    };
    let account = |groups: &[String]| {
        format!(
            r#"{{"code":"0","data":{{"account":[{{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"accountLock":5,"totalPositionNormal":7,"sumOpenRealizedAmount":2,"positionVos":[{}]}}]}}}}"#,
            groups.join(",")
        )
    };
    let mut h = Harness::with_market(&markets);
    h.reply(
        "/account",
        &account(&[
            group("E-BTC-USDT", "BUY", 2),
            group("E-ETH-USDT", "SELL", 3),
        ]),
    );
    let eth = h.e.store().position("E-ETH-USDT").unwrap().clone();
    let snapshot = account(&[
        group("E-BTC-USDT", "BUY", 4),
        group("E-ETH-USDT", "HOLD", 9),
    ]);
    for round in 0..2 {
        let logs = h.reread(Some(&snapshot), None);
        assert_eq!(h.e.store().position("E-BTC-USDT").unwrap().contracts, 4.0);
        assert_eq!(h.e.store().position("E-ETH-USDT"), Some(&eth));
        assert_eq!(h.e.store().balance().total, 100.0);
        let skips: Vec<_> = logs.iter().filter(|l| l.contains("skipped")).collect();
        let expected: &[&str] = if round == 0 {
            &["Bitrue skipped malformed position row E-ETH-USDT"]
        } else {
            &[]
        };
        assert_eq!(skips, expected, "{logs:?}");
        assert!(!logs.iter().any(|l| l.contains("HOLD")));
    }
    h.reread(Some(&account(&[group("E-BTC-USDT", "BUY", 4)])), None);
    let ids: Vec<_> =
        h.e.store()
            .positions()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
    assert_eq!(ids, ["E-BTC-USDT"]);
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
    assert!(h.actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command,result})if *command==88&&result.contains("\"buyLeverage\":25.0"))));
    assert!(h.pending.iter().all(|r| r.method == Method::Get));
}

#[test]
fn cumulative_average_fallback_recovers_execution_price_without_replaying() {
    for first in [r#""ap":"100""#, r#""ap":"0","l":"1","L":"100""#] {
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
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":"child","S":"BUY","o":"LIMIT","x":"NEW","X":"NEW","q":"100","z":"0","p":"101","conditionOrder":true,"triggerType":2,"triggerPrice":95,"triggerPriceType":2}}"#);
    let child =
        h.e.store()
            .orders()
            .iter()
            .find(|o| o.id == "child")
            .unwrap();
    assert_eq!(child.kind, OrderType::StopLoss);
    assert_eq!(child.side, OrderSide::Sell);
    assert_eq!(child.parent_id.as_deref(), Some("parent"));
    assert!(child.reduce_only);
    assert_eq!(child.price, 95.0);
    assert_eq!(
        child.trigger_type,
        Some(safe_cex_engine::model::StopTriggerType::MarkPrice)
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

#[test]
fn overflowing_average_cannot_replace_a_trusted_rest_quote_basis() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","ap":"100"}}"#);
    assert_eq!(h.fills(), [(100.0, 10.0)]);
    h.reply("/openOrders", r#"{"code":"0","data":[{"orderId":2334151155877155281,"type":"LIMIT","side":"BUY","status":"PART_FILLED","action":"OPEN","price":100,"origQty":100,"executedQty":30,"avgPrice":110}]}"#);
    assert_eq!(h.e.store().orders()[0].filled, 30.0);
    // Every input is finite, but 30 * 1e308 is not a usable quote anchor.
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"3","ap":"1e308"}}"#);
    assert!(h.fills().is_empty());
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"4","ap":"115"}}"#);
    assert_eq!(
        h.fills(),
        [(130.0, 10.0)],
        "the valid suffix keeps its trusted REST quote basis"
    );
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"4","ap":"115"}}"#);
    assert!(h.fills().is_empty());
}

#[test]
fn corrected_quote_at_a_delivered_endpoint_prices_the_next_suffix_once() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","l":"1","L":"100","ap":"1e308"}}"#);
    assert_eq!(
        h.fills(),
        [(100.0, 10.0)],
        "uniform execution evidence is independently valid"
    );
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","ap":"100"}}"#);
    assert!(
        h.fills().is_empty(),
        "quote correction cannot replay quantity"
    );
    let next = r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"2","ap":"110"}}"#;
    h.ws(next);
    assert_eq!(
        h.fills(),
        [(120.0, 10.0)],
        "the exact corrected endpoint proves the new suffix VWAP"
    );
    h.ws(next);
    assert!(h.fills().is_empty());
    assert_eq!(h.e.store().orders()[0].filled, 20.0);
}

const LOG_SECRET: &str = "audit-private-stream-secret";
const SIGNED_STREAM_URL: &str =
    "wss://fapiws.bitrue.com/stream?listenKey=futures-key&signature=audit-private-stream-signature";
fn assert_stream_notices_are_safe(h: &Harness) {
    for action in &h.actions {
        if let Action::Notice(notice) = action {
            // Debug covers every notice field, including structured venue errors.
            let text = format!("{notice:?}");
            for secret in [
                LOG_SECRET,
                SIGNED_STREAM_URL,
                "futures-key",
                "audit-private-stream-signature",
            ] {
                assert!(!text.contains(secret), "private data escaped in {notice:?}");
            }
        }
    }
}
#[test]
fn private_subscription_rejection_and_followup_failures_never_log_echoed_credentials() {
    for event in [
        "user_account_update",
        "user_order_update",
        "user_balance_update",
    ] {
        let mut h = Harness::new();
        h.seed();
        h.e.handle(h.now, Event::WsOpened { socket: h.private });
        h.collect();
        let rejected = serde_json::json!({"event":event,"status":"error","code":401,"apiKey":LOG_SECRET,"listenKey":"futures-key","url":SIGNED_STREAM_URL}).to_string();
        h.ws(&rejected);
        assert_stream_notices_are_safe(&h);
        assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("private subscription failed") && message.contains(event) && message.contains("code 401"))));
        h.private_retry(1_000.0);
        let request = h.take("listenKey");
        h.respond(
            request,
            &serde_json::json!({"code":401,"msg":format!("{LOG_SECRET} {SIGNED_STREAM_URL}")})
                .to_string(),
        );
        assert_stream_notices_are_safe(&h);
        h.private_retry(2_000.0);
        let request = h.take("listenKey");
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: request.id,
                error: SIGNED_STREAM_URL,
            },
        );
        h.collect();
        assert_stream_notices_are_safe(&h);
        assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("private stream") && message.contains("code 401"))));
    }
}
#[test]
fn an_untrusted_subscription_code_and_rejected_private_reads_are_not_echoed() {
    let mut h = Harness::new();
    let snapshot = h.take("/account");
    h.respond(
        snapshot,
        &serde_json::json!({"code":401,"msg":format!("{LOG_SECRET} {SIGNED_STREAM_URL}")})
            .to_string(),
    );
    let orders = h.take("/openOrders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: orders.id,
            error: SIGNED_STREAM_URL,
        },
    );
    h.collect();
    h.ws(&serde_json::json!({"event":"user_account_update","status":"error","code":SIGNED_STREAM_URL,"apiKey":LOG_SECRET}).to_string());
    assert_stream_notices_are_safe(&h);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("account read failed") && message.contains("code 401"))));
}
#[test]
fn rest_ahead_of_delayed_stream_execution_does_not_consume_the_fill() {
    let mut h = Harness::new();
    h.seed();
    h.order(1, "PART_FILLED", "LIMIT", 110);
    assert_eq!(h.fills(), [(110.0, 10.0)]);
    h.reply(
        "/openOrders",
        &ORDER.replace("\"executedQty\":0", "\"executedQty\":20"),
    );
    assert_eq!(h.e.store().orders()[0].filled, 20.0);
    h.order(2, "PART_FILLED", "LIMIT", 112);
    h.order(2, "PART_FILLED", "LIMIT", 112);
    assert_eq!(h.fills(), [(112.0, 10.0)]);
    assert_eq!(h.e.store().orders()[0].filled, 20.0);
}

#[test]
fn native_last_execution_quantity_never_reprices_a_rest_gap_and_late_intervals_deliver_once() {
    let mut h = Harness::new();
    h.seed();
    let event = |z, last, price| {
        let average = 95 + z * 5;
        format!(
            r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"{z}","l":"{last}","L":"{price}","ap":"{average}"}}}}"#
        )
    };
    h.ws(&event(1, 1, 100));
    assert_eq!(h.fills(), [(100.0, 10.0)]);
    h.reply("/openOrders", r#"{"code":"0","data":[{"orderId":2334151155877155281,"type":"LIMIT","side":"BUY","status":"PART_FILLED","action":"OPEN","price":100,"origQty":100,"executedQty":30,"avgPrice":110}]}"#);
    h.ws(&event(3, 1, 120));
    h.ws(&event(3, 1, 120));
    assert_eq!(
        h.fills(),
        [(120.0, 10.0)],
        "L cannot price the missing middle execution"
    );
    h.ws(&event(2, 1, 110));
    h.ws(&event(2, 1, 110));
    h.ws(&event(1, 1, 100));
    assert_eq!(
        h.fills(),
        [(110.0, 10.0)],
        "a late execution owns its unreported interval exactly once"
    );
    assert_eq!(h.e.store().orders()[0].filled, 30.0);
    h.ws(&event(4, 1, 130));
    assert_eq!(h.fills(), [(130.0, 10.0)]);
}
#[test]
fn invalid_last_execution_quantity_cannot_consume_a_later_valid_execution() {
    let mut h = Harness::new();
    h.seed();
    for last in ["0", "-1", "999"] {
        h.ws(&format!(r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","l":"{last}","L":"100"}}}}"#));
        assert!(h.fills().is_empty());
    }
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","l":"1","L":"100"}}"#);
    assert_eq!(h.fills(), [(100.0, 10.0)]);
}

#[test]
fn missing_last_quantity_uses_only_proven_interval_vwap_and_never_the_last_price_for_a_gap() {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"1","l":"1","L":"100","ap":"100"}}"#);
    assert_eq!(h.fills(), [(100.0, 10.0)]);
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"3","L":"120"}}"#);
    assert!(
        h.fills().is_empty(),
        "the last price does not price two missing executions"
    );
    h.ws(r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"E_BTCUSDT","i":2334151155877155281,"S":"BUY","o":"LIMIT","x":"TRADE","X":"PART_FILLED","q":"100","z":"3","ap":"110","L":"120"}}"#);
    assert_eq!(
        h.fills(),
        [(115.0, 20.0)],
        "two cumulative quotes prove the interval VWAP"
    );
}

#[test]
fn private_open_flaps_back_off_without_reusing_listen_keys() {
    let mut h = Harness::new();
    h.e.handle(h.now, Event::WsOpened { socket: h.private });
    h.collect();
    for nominal in [1000.0, 2000.0, 4000.0] {
        let old = h.private;
        h.e.handle(
            h.now,
            Event::WsClosed {
                socket: old,
                code: 1006,
            },
        );
        h.collect();
        let (timer, at) = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap();
        assert!((nominal * 0.75..=nominal).contains(&(at - h.now)));
        assert!(!h.pending.iter().any(|r| r.url.contains("listenKey")));
        h.now = at;
        h.e.handle(h.now, Event::TimerFired { timer });
        h.collect();
        let request = h.take("listenKey");
        assert_eq!(request.method, Method::Post);
        h.respond(request, r#"{"code":200,"data":{"listenKey":"new-key"}}"#);
        h.private = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .unwrap();
        assert_ne!(h.private, old);
        h.e.handle(h.now, Event::WsOpened { socket: h.private });
        h.collect();
    }
}

#[test]
fn currency_balances_keep_both_coins_and_failed_snapshots_preserve_them() {
    let mut h = Harness::new();
    h.reply("/account", r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"accountLock":5,"sumOpenRealizedAmount":2,"positionVos":[]},{"marginCoin":"USDC","totalEquity":30,"accountNormal":20,"accountLock":3,"sumOpenRealizedAmount":4,"positionVos":[]}]}}"#);
    let coins = h.e.store().currency_balances().unwrap().to_vec();
    assert_eq!(coins.len(), 2);
    assert_eq!(coins[0].currency, "USDC");
    assert_eq!(coins[0].balance.total, 30.0);
    assert_eq!(coins[0].balance.used, 10.0);
    // Per-currency UPnL is the venue's sumOpenRealizedAmount, as TS publishes it.
    assert_eq!(coins[0].balance.upnl, 4.0);
    assert_eq!(coins[1].currency, "USDT");
    assert_eq!(h.e.store().balance().total, 100.0);
    h.fire_at(2_000.0);
    h.reply("/account", r#"{"code":"0","data":{"account":[{"marginCoin":"USDC","totalEquity":0,"accountNormal":0,"positionVos":null}]}}"#);
    assert_eq!(h.e.store().currency_balances().unwrap(), coins.as_slice());
    assert_eq!(h.e.store().balance().total, 100.0);
    h.fire_at(4_000.0);
    h.reply("/account", EMPTY);
    assert_eq!(h.e.store().currency_balances().unwrap().len(), 1);
    assert_eq!(h.e.store().currency_balances().unwrap()[0].currency, "USDT");
}

#[test]
fn leverage_less_flat_symbol_keeps_margin_and_can_accept_explicit_leverage() {
    let mut h = Harness::new();
    h.reply("/account", r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"positionVos":[{"contractName":"E-BTC-USDT","positions":[{"side":"BUY","volume":0,"openPrice":0,"positionType":2}]}]}]}}"#);
    assert!(h.e.store().loaded().balance);
    let setting = h.e.store().setting("E-BTC-USDT").unwrap();
    assert_eq!(setting.leverage, 0.0);
    assert_eq!(setting.margin_type, Some(MarginType::Isolated));
    h.e.handle(
        h.now,
        Event::Command(Command::SetLeverage {
            command: 55,
            symbol: "E-BTC-USDT".into(),
            leverage: 8.0,
            side: None,
        }),
    );
    h.collect();
    h.reply("/level_edit", r#"{"code":"0","data":{}}"#);
    let setting = h.e.store().setting("E-BTC-USDT").unwrap();
    assert_eq!(setting.leverage, 8.0);
    assert_eq!(setting.margin_type, Some(MarginType::Isolated));
    h.reply("/account", r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"positionVos":[{"contractName":"E-BTC-USDT","positions":[{"side":"BUY","volume":0,"openPrice":0,"leverageLevel":0,"positionType":2}]}]}]}}"#);
    assert_eq!(h.e.store().setting("E-BTC-USDT").unwrap().leverage, 8.0);
}

#[test]
fn failed_symbol_finishes_initial_order_sweep_and_retries_without_clearing_known_orders() {
    let markets = MARKET.trim_end_matches(']').to_owned()
        + r#",{"symbol":"E-ETH-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
    let mut h = Harness::with_market(&markets);
    h.reply("/account", EMPTY);
    h.reply("/openOrders?contractName=E-BTC-USDT", ORDER);
    assert!(!h.e.store().loaded().orders);
    h.fire_at(200.0);
    h.reply(
        "/openOrders?contractName=E-ETH-USDT",
        r#"{"code":"-1","msg":"symbol unavailable"}"#,
    );
    assert!(h.e.store().loaded().orders);
    assert_eq!(h.e.store().orders().len(), 1);
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Error,
            ..
        }))
    )));
    h.fire_retry(500.0);
    assert!(
        h.pending
            .iter()
            .any(|r| r.url.contains("/openOrders?contractName=E-ETH-USDT"))
    );
    assert!(h.e.store().loaded().orders);
}

#[test]
fn a_persistently_failing_symbol_backs_off_to_a_cap_and_resets_on_success() {
    const ETH: &str = "/openOrders?contractName=E-ETH-USDT";
    const BTC: &str = "/openOrders?contractName=E-BTC-USDT";
    const FAILED: &str = r#"{"code":"-1","msg":"symbol unavailable"}"#;
    let markets = MARKET.trim_end_matches(']').to_owned()
        + r#",{"symbol":"E-ETH-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
    let mut h = Harness::with_market(&markets);
    h.reply("/account", EMPTY);
    h.reply(BTC, ORDER);
    h.fire_at(200.0);
    h.reply(ETH, FAILED);
    for delay in [
        500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0, 30_000.0, 30_000.0,
    ] {
        h.fire_retry(delay);
        h.reply(ETH, FAILED);
    }
    assert_eq!(h.e.store().orders().len(), 1);
    let errors = |h: &Harness| {
        h.actions
            .iter()
            .filter(|a| {
                matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. })) if message.contains("E-ETH-USDT"))
            })
            .count()
    };
    assert_eq!(errors(&h), 1);

    // BTC keeps its normal cadence while ETH waits out its backoff.
    h.order(1, "NEW", "LIMIT", 100);
    h.reply(BTC, ORDER);
    assert!(!h.pending.iter().any(|r| r.url.contains(ETH)));

    h.fire_retry(30_000.0);
    h.reply(ETH, r#"{"code":"0","data":[]}"#);
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Info, message, .. })) if message.contains("E-ETH-USDT recovered")
    )));

    // A fresh failure after recovery starts again from the initial delay.
    h.e.handle(h.now, Event::WsOpened { socket: h.private });
    h.collect();
    h.reply("/account", EMPTY);
    h.reply(BTC, ORDER);
    h.fire_at(h.now + 200.0);
    h.reply(ETH, FAILED);
    h.fire_retry(500.0);
    assert!(h.pending.iter().any(|r| r.url.contains(ETH)));
    assert_eq!(errors(&h), 2);
}

#[test]
fn failed_symbol_order_read_retries_are_jittered() {
    const ETH: &str = "/openOrders?contractName=E-ETH-USDT";
    const FAILED: &str = r#"{"code":"-1","msg":"symbol unavailable"}"#;
    let markets = MARKET.trim_end_matches(']').to_owned()
        + r#",{"symbol":"E-ETH-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
    let mut h = Harness::with_market(&markets);
    h.reply("/account", EMPTY);
    h.reply("/openOrders?contractName=E-BTC-USDT", ORDER);
    h.fire_at(200.0);
    h.reply(ETH, FAILED);
    let nominal = [500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0];
    let mut delays = Vec::new();
    for nominal in nominal {
        delays.push(h.fire_retry(nominal));
        h.reply(ETH, FAILED);
    }
    assert!(
        delays
            .iter()
            .zip(nominal)
            .any(|(delay, nominal)| *delay < nominal),
        "failed order reads must not retry in lockstep: {delays:?}"
    );
}

#[test]
fn documented_symbol_brackets_update_limits_without_guessing_account_leverage() {
    let mut h = Harness::new();
    h.reply("/account", EMPTY);
    for (data, expected) in [
        (
            r#"{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":"125"},{"initialLeverage":50}]}"#,
            125.0,
        ),
        (
            r#"[{"contractName":"E-ETH-USDT","brackets":[{"initialLeverage":100}]},{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":75}]}]"#,
            75.0,
        ),
    ] {
        h.e.handle(
            h.now,
            Event::Command(Command::FetchMaxLeverage {
                command: 51,
                symbol: "E-BTC-USDT".into(),
            }),
        );
        h.collect();
        h.reply(
            "/leverageBracket?contractName=E-BTC-USDT",
            &format!(r#"{{"code":"0","data":{data}}}"#),
        );
        assert_eq!(
            h.e.store()
                .market("E-BTC-USDT")
                .unwrap()
                .limits
                .leverage
                .max,
            expected
        );
        assert!(h.e.store().setting("E-BTC-USDT").is_none());
        assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 51, result: v }) if v.parse::<f64>().ok() == Some(expected))));
    }
    for malformed in [
        r#"{"code":"0","data":{"contractName":"E-ETH-USDT","brackets":[{"initialLeverage":125}]}}"#,
        r#"{"code":"0","data":{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":0}]}}"#,
        r#"{"code":"0","data":{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":50},{}]}}"#,
        r#"{"code":"0","data":{"contractName":"E-BTC-USDT","brackets":null}}"#,
        r#"{"code":"0","data":{"contractName":"E-BTC-USDT","brackets":[]}}"#,
        r#"{"code":"0","data":{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":126}]}}"#,
        r#"{"code":"0","data":{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":5.5}]}}"#,
        r#"{"code":"-1","msg":"denied"}"#,
    ] {
        h.e.handle(
            h.now,
            Event::Command(Command::FetchMaxLeverage {
                command: 52,
                symbol: "E-BTC-USDT".into(),
            }),
        );
        h.collect();
        h.reply("/leverageBracket?contractName=E-BTC-USDT", malformed);
        assert_eq!(
            h.e.store()
                .market("E-BTC-USDT")
                .unwrap()
                .limits
                .leverage
                .max,
            75.0
        );
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 52, .. })))
        );
    }
    h.e.handle(
        h.now,
        Event::Command(Command::SetLeverage {
            command: 53,
            symbol: "E-BTC-USDT".into(),
            leverage: 76.0,
            side: None,
        }),
    );
    h.collect();
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/level_edit")));
}

#[test]
fn failed_bracket_transport_reports_error_and_retains_market_bound() {
    let mut h = Harness::new();
    h.e.handle(
        h.now,
        Event::Command(Command::FetchMaxLeverage {
            command: 61,
            symbol: "E-BTC-USDT".into(),
        }),
    );
    h.collect();
    for _ in 0..4 {
        let request = h.take("/leverageBracket");
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: request.id,
                error: "connection lost",
            },
        );
        h.collect();
    }
    assert_eq!(
        h.e.store()
            .market("E-BTC-USDT")
            .unwrap()
            .limits
            .leverage
            .max,
        100.0
    );
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 61, .. })))
    );
}

#[test]
fn leverage_bracket_failures_never_echo_private_transport_or_response_diagnostics() {
    for transport_failure in [true, false] {
        let mut h = Harness::new();
        h.seed();
        let before =
            h.e.store()
                .market("E-BTC-USDT")
                .unwrap()
                .limits
                .leverage
                .max;
        h.e.handle(
            h.now,
            Event::Command(Command::FetchMaxLeverage {
                command: 62,
                symbol: "E-BTC-USDT".into(),
            }),
        );
        h.collect();
        if transport_failure {
            let error = format!("provider rejected {LOG_SECRET} at {SIGNED_STREAM_URL}");
            // The safe GET retries three times before the terminal account error.
            for _ in 0..4 {
                let request = h.take("/leverageBracket?contractName=E-BTC-USDT");
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: request.id,
                        error: &error,
                    },
                );
                h.collect();
                assert_stream_notices_are_safe(&h);
            }
        } else {
            let request = h.take("/leverageBracket?contractName=E-BTC-USDT");
            let body = serde_json::json!({
                "code": 401,
                "msg": format!("{LOG_SECRET} {SIGNED_STREAM_URL}"),
                "apiKey": LOG_SECRET,
            })
            .to_string();
            h.e.handle(
                h.now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 403,
                        headers: "",
                        body: &body,
                    },
                },
            );
            h.collect();
        }
        assert_stream_notices_are_safe(&h);
        let completions = h
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 62, error }) => {
                    Some(error.legacy_text())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(completions.len(), 1);
        assert_eq!(
            completions[0],
            if transport_failure {
                "Bitrue leverage bracket read failed"
            } else {
                "Bitrue leverage bracket read failed (code 401)"
            }
        );
        assert!(
            !h.pending
                .iter()
                .any(|request| request.url.contains("/leverageBracket"))
        );
        assert_eq!(
            h.e.store()
                .market("E-BTC-USDT")
                .unwrap()
                .limits
                .leverage
                .max,
            before
        );
    }
}

#[test]
fn shared_leverage_snapshot_failures_report_each_owner_before_done_without_broadcast_log() {
    for malformed in [false, true] {
        let mut h = Harness::new();
        h.seed();
        for command in [91, 92] {
            h.e.handle(
                h.now,
                Event::Command(Command::FetchLeverage {
                    command,
                    symbol: "E-BTC-USDT".into(),
                }),
            );
            h.collect();
        }
        let snapshot = h.take("/account");
        h.actions.clear();
        if malformed {
            h.respond(snapshot, r#"{"code":"0","data":{"account":[]}}"#);
        } else {
            exhaust_snapshot_transport(&mut h, snapshot);
        }
        let expected = if malformed {
            "Malformed Bitrue account snapshot; retaining previous state"
        } else {
            "Bitrue account read failed"
        };
        let errors: Vec<_> = h
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandError { command, message }) => {
                    Some((*command, message.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(errors, [(91, expected), (92, expected)]);
        for owner in [91, 92] {
            let error=h.actions.iter().position(|a| matches!(a,Action::Notice(Notice::CommandError{command,..}) if *command==owner)).unwrap();
            let done=h.actions.iter().position(|a| matches!(a,Action::Notice(Notice::CommandFailed{command,..}) if *command==owner)).unwrap();
            assert!(error < done);
        }
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message==expected))
        );
    }
}

#[test]
fn background_snapshot_failures_keep_ordinary_diagnostics_without_command_ownership() {
    for malformed in [false, true] {
        let mut h = Harness::new();
        let snapshot = h.take("/account");
        h.actions.clear();
        if malformed {
            h.respond(snapshot, r#"{"code":"0","data":{"account":[]}}"#);
        } else {
            exhaust_snapshot_transport(&mut h, snapshot);
        }
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandError { .. })))
        );
        let expected = if malformed {
            "Malformed Bitrue account snapshot; retaining previous state"
        } else {
            "Bitrue account read failed"
        };
        assert_eq!(h.actions.iter().filter(|a| matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. })) if message==expected)).count(),1);
    }
}

fn exhaust_snapshot_transport(h: &mut Harness, mut snapshot: HttpRequest) {
    for attempt in 0..4 {
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: snapshot.id,
                error: "snapshot transport failed",
            },
        );
        h.collect();
        if attempt < 3 {
            assert!(!h.actions.iter().any(|a| matches!(
                a,
                Action::Notice(
                    Notice::CommandError { .. }
                        | Notice::CommandDone { .. }
                        | Notice::CommandFailed { .. }
                )
            )));
            snapshot = h.take("/account");
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
    let mut h = Harness::new();
    h.seed();
    let engine = &mut h.e;
    let (now, mut reads) = policy_poll_read(engine, 0.0, "/account");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = EMPTY;
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
fn uncached_stream_condition_discriminators_precede_execution_kind() {
    use serde_json::json;
    for (execution, trigger, conditional, expected) in [
        ("MARKET", json!(1), true, OrderType::StopLoss),
        ("LIMIT", json!("2"), true, OrderType::TakeProfit),
        ("MARKET", json!(1), false, OrderType::Market),
        ("LIMIT", json!(2), false, OrderType::Limit),
        ("MARKET", json!(3), true, OrderType::Market),
    ] {
        let mut h = Harness::new();
        h.reply("/account", EMPTY);
        h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
        h.ws(&json!({"e":"ORDER_TRADE_UPDATE","o":{
            "s":"E_BTCUSDT","i":"uncached","S":"SELL","o":execution,
            "x":"NEW","X":"NEW","q":"100","z":"0","p":"100",
            "conditionOrder":conditional,"triggerType":trigger,
            "triggerPrice":"90","triggerPriceType":1,"open":"OPEN"
        }})
        .to_string());
        let order =
            h.e.store()
                .orders()
                .iter()
                .find(|o| o.id == "uncached")
                .unwrap();
        assert_eq!(order.kind, expected);
        assert_eq!(
            order.price,
            if matches!(expected, OrderType::StopLoss | OrderType::TakeProfit) {
                90.0
            } else {
                100.0
            }
        );
        assert_eq!(order.amount, 100.0);
        assert_eq!(order.filled, 0.0);
        assert!(
            !order.reduce_only,
            "classification cannot invent closing provenance"
        );
        assert_eq!(order.parent_id, None);
    }
}

#[test]
fn unidentified_malformed_children_keep_cached_protection_while_valid_peers_commit() {
    use serde_json::json;
    for missing in [true, false] {
        let mut h = Harness::new();
        h.reply("/account", EMPTY);
        let mut payload = json!({"code":"0","data":[
            {"orderId":"parent","type":"LIMIT","side":"BUY","status":"NEW",
             "action":"OPEN","price":100,"origQty":100,"executedQty":0,
             "childOrders":[{"orderId":"child","type":"MARKET","conditionOrder":true,
                 "triggerType":1,"triggerPrice":90,"triggerPriceType":1,"side":"BUY",
                 "status":"NEW","action":"OPEN","origQty":100,"executedQty":0}]},
            {"orderId":"peer","type":"LIMIT","side":"SELL","status":"NEW",
             "action":"OPEN","price":120,"origQty":100,"executedQty":0}
        ]});
        h.reply("/openOrders", &payload.to_string());
        let parent = h.e.store().order("parent").unwrap().clone();
        let child = h.e.store().order("child").unwrap().clone();
        assert_eq!(child.parent_id.as_deref(), Some("parent"));
        assert_eq!(child.kind, OrderType::StopLoss);
        assert_eq!(child.side, OrderSide::Sell);
        assert!(child.reduce_only);
        let damaged = payload["data"][0]["childOrders"][0]
            .as_object_mut()
            .unwrap();
        if missing {
            damaged.remove("orderId");
        } else {
            damaged.insert("orderId".into(), json!(""));
        }
        payload["data"][1]["price"] = json!(121);
        h.reread(None, Some(&payload.to_string()));
        assert_eq!(
            h.e.store().order("parent"),
            Some(&parent),
            "missing={missing}"
        );
        assert_eq!(
            h.e.store().order("child"),
            Some(&child),
            "missing={missing}"
        );
        assert_eq!(
            h.e.store().order("peer").unwrap().price,
            121.0,
            "missing={missing}"
        );
    }
}
