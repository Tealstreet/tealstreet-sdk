//! Original custody gaps and native controls for Phemex late cancel repair.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Level, Method, Notice, Outcome, Response, SocketId,
    TimerId,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use serde_json::{Value, json};
use std::collections::VecDeque;
use support::{Options, PreparedEngine};

type TestEngine = PreparedEngine<PhemexPerpetuals>;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"69000","askRp":"69001","lastRp":"69000","markRp":"69000","indexRp":"69000","openRp":"69000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;

fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}
fn deletes(actions: &[Action]) -> Vec<HttpRequest> {
    requests(actions)
        .into_iter()
        .filter(|r| r.method == Method::Delete)
        .collect()
}
fn terminals(actions: &[Action], id: u32) -> Vec<Result<String, String>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .collect()
}
fn command_terminals(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            )
        })
        .count()
}
fn messages(actions: &[Action], level: Level) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.level == level => {
                Some(event.message.as_str())
            }
            _ => None,
        })
        .collect()
}
fn capture_delete(actions: &[Action]) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.method == Method::Delete => Some(r.clone()),
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { request, .. } if request.method == Method::Delete => {
                    Some(request.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("native late DELETE or its genuine preparation")
}
fn accept(id: &str) -> String {
    json!({"code":0,"data":{"orderID":id,"ordStatus":"Canceled","cumQtyRq":"0"}}).to_string()
}
fn release(e: &mut TestEngine, now: f64, actions: Vec<Action>) -> Vec<Action> {
    let mut queue = VecDeque::from(actions);
    let mut out = Vec::new();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => e.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id: preparation.attempt().request.0,
                        expires_at: None,
                    },
                ),
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    if matches!(e.claim_http(now, permit), Ok(Claim::Released(_))) {
                        out.push(Action::Http(request));
                    }
                }
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => {
                    panic!("late cancel original has one physical call per group")
                }
            },
            a => out.push(a),
        }
        queue.extend(e.drain_raw_actions());
    }
    out
}
struct Harness {
    e: TestEngine,
    socket: SocketId,
    now: f64,
    command: u32,
    timers: Vec<(TimerId, f64)>,
}
impl Harness {
    fn record(&mut self, actions: &[Action]) {
        self.timers.extend(actions.iter().filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        }));
    }
    fn at(&mut self, now: f64, event: Event<'_>, raw: bool) -> Vec<Action> {
        assert!(now >= self.now, "fixture time is monotonic");
        self.now = now;
        self.e.handle(now, event);
        let actions = if raw {
            self.e.drain_raw_actions()
        } else {
            self.e.drain_actions().collect()
        };
        self.record(&actions);
        actions
    }
    fn event(&mut self, event: Event<'_>) -> Vec<Action> {
        self.at(self.now + 1., event, false)
    }
    fn raw_event(&mut self, event: Event<'_>) -> Vec<Action> {
        self.at(self.now + 1., event, true)
    }
    fn reply(&mut self, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        })
    }
    fn raw_reply(&mut self, r: &HttpRequest, body: &str) -> Vec<Action> {
        self.raw_event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        })
    }
    fn query_command(&mut self, command: impl FnOnce(u32) -> Command) -> (u32, Vec<Action>) {
        self.command += 1;
        let id = self.command;
        (id, self.event(Event::Command(command(id))))
    }
    fn place_pending(&mut self, client: &str, side: OrderSide) -> (u32, HttpRequest) {
        let mut order = opts(client);
        order.side = side;
        let (id, a) = self.query_command(|command| Command::PlaceOrders {
            command,
            orders: vec![order],
        });
        let r = requests(&a)
            .into_iter()
            .find(|r| r.url.contains("/g-orders/create"))
            .expect("genuine released placement");
        (id, r)
    }
    fn lose_pending(&mut self, id: u32, r: &HttpRequest) {
        let a = self.event(Event::HttpFailed {
            request: r.id,
            error: "native network loss",
        });
        assert_eq!(terminals(&a, id).len(), 1, "lost placement ends once");
        assert!(terminals(&a, id)[0].is_err());
        assert!(a.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command, error })
                if *command == id && error.outcome == Some(Outcome::Unknown)
        )));
    }
    fn lose(&mut self, client: &str, side: OrderSide) -> (u32, HttpRequest) {
        let (id, r) = self.place_pending(client, side);
        self.lose_pending(id, &r);
        (id, r)
    }
    fn cancel_all(&mut self) -> (u32, Vec<Action>) {
        self.query_command(|command| Command::CancelSymbolOrders {
            command,
            symbol: "BTCUSDT".into(),
        })
    }
    fn owe(&mut self, client: &str, side: OrderSide) -> (u32, HttpRequest) {
        let (_, r) = self.lose(client, side);
        let (id, a) = self.cancel_all();
        assert_eq!(
            terminals(&a, id),
            vec![Ok("[]".into())],
            "empty immediate cancel completes natively"
        );
        (id, r)
    }
    fn owe_concurrent(&mut self, clients: &[(&str, OrderSide)]) {
        let pending: Vec<_> = clients
            .iter()
            .map(|(client, side)| self.place_pending(client, *side))
            .collect();
        for (id, request) in pending {
            self.lose_pending(id, &request);
        }
        let (id, actions) = self.cancel_all();
        assert_eq!(terminals(&actions, id), vec![Ok("[]".into())]);
    }
    fn stream(&mut self, mut rows: Vec<Value>, raw: bool) -> Vec<Action> {
        let stamp = ((self.now + 1.) * 1e6) as u64;
        for row in &mut rows {
            row["transactTimeNs"] = json!(stamp);
        }
        let text = json!({"type":"incremental","timestamp":stamp,"orders_p":rows}).to_string();
        self.at(
            self.now + 1.,
            Event::WsMessage {
                socket: self.socket,
                text: &text,
            },
            raw,
        )
    }
    fn discover(&mut self, create: &HttpRequest, body: &str, raw: bool) -> Vec<Action> {
        let expiry: f64 = create
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-phemex-request-expiry"))
            .expect("native signing expiry")
            .1
            .parse::<u64>()
            .unwrap() as f64
            * 1000.;
        let at = expiry + 5000.;
        let timer = self
            .timers
            .iter()
            .rev()
            .find(|(_, when)| *when == at)
            .expect("native post-expiry reconcile timer")
            .0;
        let a = self.at(at, Event::TimerFired { timer }, false);
        let read = requests(&a)
            .into_iter()
            .find(|r| r.url.contains("/g-orders/activeList?"))
            .expect("genuine primary reconciliation read");
        assert_eq!(
            read.headers
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("x-phemex-access-token"))
                .unwrap()
                .1,
            "fixture-primary",
            "the reconciliation READ uses Primary"
        );
        self.at(
            self.now + 1.,
            Event::HttpResponse {
                request: read.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
            raw,
        )
    }
}
fn boot(mode: &str, snapshot: bool, secondary: bool) -> Harness {
    let mut e = support::prepared_engine(Options {
        key: Some("fixture-primary".into()),
        secret: Some("fixture-material".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "fixture-secondary".into(),
            secret: "fixture-secondary-material".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    let mut now = 1000.;
    e.start(now);
    let a: Vec<_> = e.drain_actions().collect();
    let products = requests(&a)
        .into_iter()
        .find(|r| r.url.contains("products"))
        .unwrap();
    now += 1.;
    e.handle(
        now,
        Event::HttpResponse {
            request: products.id,
            response: Response {
                status: 200,
                headers: "",
                body: PRODUCTS,
            },
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    let socket = a
        .iter()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .expect("private socket");
    for r in requests(&a)
        .into_iter()
        .filter(|r| r.url.contains("/g-accounts/"))
    {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let positions = if currency == "USDT" {
            json!([{"symbol":"BTCUSDT","currency":"USDT","posMode":mode,"posSide":if mode=="OneWay" {"Merged"} else {"Long"},"size":"0","leverageRr":"-10","crossMargin":true}])
        } else {
            json!([])
        };
        let body=json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":positions}}).to_string();
        now += 1.;
        e.handle(
            now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        e.drain_actions().for_each(drop);
    }
    let ticker = requests(&a)
        .into_iter()
        .find(|r| r.url.contains("24hr/all"))
        .unwrap();
    now += 1.;
    e.handle(
        now,
        Event::HttpResponse {
            request: ticker.id,
            response: Response {
                status: 200,
                headers: "",
                body: DAILY,
            },
        },
    );
    e.drain_actions().for_each(drop);
    let mut h = Harness {
        e,
        socket,
        now,
        command: 100,
        timers: Vec::new(),
    };
    h.event(Event::WsOpened { socket });
    h.event(Event::WsMessage {
        socket,
        text: r#"{"id":999,"result":{"status":"success"}}"#,
    });
    assert!(
        !h.e.store().loaded().orders,
        "cold account has no full Orders snapshot"
    );
    if snapshot {
        h.event(Event::WsMessage {
            socket,
            text: r#"{"type":"snapshot","orders_p":[]}"#,
        });
        assert!(h.e.store().loaded().orders);
    }
    h
}
fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

fn params(r: &HttpRequest) -> Vec<(String, String)> {
    r.url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (k.into(), v.replace("%2C", ","))
        })
        .collect()
}

fn p(r: &HttpRequest, key: &str) -> Option<String> {
    params(r)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

fn live_row(id: &str) -> Value {
    json!({"orderID":id,"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","ordType":"Limit","ordStatus":"New","orderQty":"0.1","cumQty":"0","leavesQty":"0.1","priceRp":"69000"})
}

fn rest_row(id: &str, client: &str) -> Value {
    json!({"orderID":id,"clOrdID":client,"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","orderType":"Limit","ordStatus":"New","orderQtyRq":"0.01","cumQtyRq":"0","leavesQtyRq":"0.01","priceRp":"69000","transactTimeNs":3_000_000_000_u64})
}

fn rows(rows: Vec<Value>) -> String {
    json!({"code":0,"data":{"rows":rows}}).to_string()
}

#[test]
fn late_repair_waits_for_a_real_primary_preparation() {
    let mut h = boot("OneWay", true, true);
    h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let a = h.stream(vec![row], true);
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "late repair must not release unmanaged HTTP"
    );
    assert_eq!(a.iter().filter(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{intent:RoutingIntent::Primary,request,..} if request.method==Method::Delete))).count(),1,"one genuine Primary repair preparation");
    let a = release(&mut h.e, h.now, a);
    let r = deletes(&a).remove(0);
    assert_eq!(
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-phemex-access-token"))
            .unwrap()
            .1,
        "fixture-primary"
    );
    assert_eq!(p(&r, "orderID").as_deref(), Some("native-a"));
}
#[test]
fn unclaimed_repair_ack_cannot_publish_or_finish_a_root() {
    let mut h = boot("OneWay", true, false);
    let (old, _) = h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let held = h.stream(vec![row], true);
    let r = capture_delete(&held);
    // This is the prepared request ID; Bind must retain it, so the stray
    // reply targets the same identity that later becomes live.
    let prepared_request = r.id;
    let a = h.raw_reply(&r, &accept("native-a"));
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Canceling,
        "unclaimed ACK has no Store authority"
    );
    assert!(
        messages(&a, Level::Warn)
            .iter()
            .all(|m| !m.contains("canceled late-landing"))
    );
    assert_eq!(command_terminals(&a), 0);
    assert!(terminals(&a, old).is_empty());
    let a = release(&mut h.e, h.now, held);
    let r = deletes(&a).remove(0);
    assert_eq!(
        r.id, prepared_request,
        "Bind preserves the prepared request ID"
    );
    let a = h.reply(&r, 200, &accept("native-a"));
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(command_terminals(&a), 0);
}
#[test]
fn held_repair_preparation_has_its_own_twenty_second_bound() {
    let mut h = boot("OneWay", true, false);
    h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let a = h.stream(vec![row], true);
    let deadline = h.now + 20_000.;
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
            _ => None,
        })
        .expect("repair preparation has its own 20s deadline");
    let a = h.at(deadline, Event::TimerFired { timer }, true);
    assert_eq!(
        messages(&a, Level::Error)
            .iter()
            .filter(|m| m.contains("could not cancel late-landing"))
            .count(),
        1
    );
    assert_eq!(command_terminals(&a), 0);
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Open
    );
}
#[test]
fn native_empty_cancel_all_completes_once_before_late_repair() {
    let mut h = boot("OneWay", true, false);
    let (old, _) = h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let a = h.stream(vec![row], false);
    let r = deletes(&a).remove(0);
    let a = h.reply(&r, 200, &accept("native-a"));
    assert!(terminals(&a, old).is_empty());
    assert_eq!(command_terminals(&a), 0);
    assert_eq!(
        messages(&a, Level::Warn)
            .iter()
            .filter(|m| m.contains("canceled late-landing order native-a"))
            .count(),
        1
    );
}
#[test]
fn native_nonempty_cancel_all_retains_its_result_then_repairs_only_the_owed_order() {
    let mut h = boot("OneWay", true, false);
    h.stream(vec![live_row("known")], false);
    h.lose("owed-a", OrderSide::Buy);
    let (old, a) = h.cancel_all();
    let r = deletes(&a).remove(0);
    assert_eq!(p(&r, "orderID").as_deref(), Some("known"));
    let a = h.reply(&r, 200, &accept("known"));
    assert_eq!(terminals(&a, old), vec![Ok("[]".into())]);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let a = h.stream(vec![row], false);
    assert_eq!(p(&deletes(&a)[0], "orderID").as_deref(), Some("native-a"));
}
#[test]
fn native_duplicate_stream_observation_never_resends_a_late_cancel() {
    let mut h = boot("OneWay", true, false);
    h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let a = h.stream(vec![row.clone()], false);
    assert_eq!(deletes(&a).len(), 1);
    assert!(deletes(&h.stream(vec![row.clone()], false)).is_empty());
    let r = deletes(&a).remove(0);
    h.reply(&r, 200, &accept("native-a"));
    assert!(deletes(&h.stream(vec![row], false)).is_empty());
}
#[test]
fn concurrent_lost_placements_share_one_bulk_request_after_repeated_cancel_all() {
    let mut h = boot("Hedged", true, false);
    h.owe_concurrent(&[("owed-a", OrderSide::Buy), ("owed-b", OrderSide::Buy)]);
    let (id, again) = h.cancel_all();
    assert_eq!(terminals(&again, id), vec![Ok("[]".into())]);
    assert!(deletes(&again).is_empty());
    let (id, refused) = h.query_command(|command| Command::PlaceOrders {
        command,
        orders: vec![opts("fenced-c")],
    });
    assert!(requests(&refused).iter().all(|r| r.method != Method::Post));
    assert_eq!(terminals(&refused, id).len(), 1);
    assert!(refused.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command, error })
            if *command == id && error.outcome == Some(Outcome::NotSent)
                && error.message.contains("uncertain")
    )));
    let mut a = live_row("native-a");
    a["clOrdID"] = json!("owed-a");
    a["posSide"] = json!("Long");
    let mut b = live_row("native-b");
    b["clOrdID"] = json!("owed-b");
    b["posSide"] = json!("Long");
    let mut foreign = live_row("native-c");
    foreign["clOrdID"] = json!("fenced-c");
    foreign["posSide"] = json!("Long");
    let observed = vec![a, b, foreign];
    let out = h.stream(observed.clone(), false);
    let sent = deletes(&out);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].url.contains("/g-orders?"));
    assert_eq!(p(&sent[0], "orderID").as_deref(), Some("native-a,native-b"));
    assert_eq!(p(&sent[0], "posSide").as_deref(), Some("Long"));
    assert!(deletes(&h.stream(observed, false)).is_empty());
}
#[test]
fn native_multi_order_group_keeps_bulk_path_for_its_singleton_tail() {
    let mut h = boot("OneWay", true, false);
    let mut observed = Vec::new();
    let clients: Vec<_> = (0..11).map(|i| format!("owed-{i}")).collect();
    let pending: Vec<_> = clients
        .iter()
        .map(|client| (client.as_str(), OrderSide::Buy))
        .collect();
    h.owe_concurrent(&pending);
    for i in 0..11 {
        let client = format!("owed-{i}");
        let mut row = live_row(&format!("native-{i}"));
        row["clOrdID"] = json!(client);
        observed.push(row);
    }
    let a = h.stream(observed, false);
    let r = deletes(&a).remove(0);
    assert!(r.url.contains("/g-orders?"));
    let ids = p(&r, "orderID").unwrap();
    assert_eq!(ids.split(',').count(), 10);
    let a = h.reply(&r, 200, &accept(ids.split(',').next().unwrap()));
    let next = deletes(&a).remove(0);
    assert!(next.url.contains("/g-orders?"));
    assert_eq!(p(&next, "orderID").as_deref(), Some("native-10"));
}
#[test]
fn native_independent_groups_continue_after_a_network_failure_without_resend() {
    let mut h = boot("Hedged", true, false);
    h.owe_concurrent(&[
        ("owed-long", OrderSide::Buy),
        ("owed-short", OrderSide::Sell),
    ]);
    let mut long = live_row("native-long");
    long["clOrdID"] = json!("owed-long");
    long["posSide"] = json!("Long");
    let mut short = live_row("native-short");
    short["clOrdID"] = json!("owed-short");
    short["posSide"] = json!("Short");
    short["side"] = json!("Sell");
    let a = h.stream(vec![long, short], false);
    let first = deletes(&a).remove(0);
    let a = h.event(Event::HttpFailed {
        request: first.id,
        error: "released network loss",
    });
    assert_eq!(
        messages(&a, Level::Error)
            .iter()
            .filter(|m| m.contains("could not cancel late-landing"))
            .count(),
        1
    );
    assert_eq!(command_terminals(&a), 0);
    let next = deletes(&a);
    assert_eq!(next.len(), 1);
    assert_ne!(p(&first, "posSide"), p(&next[0], "posSide"));
    assert_ne!(p(&first, "orderID"), p(&next[0], "orderID"));
}
#[test]
fn native_mixed_cancel_ack_keeps_accepted_effect_and_restores_only_the_other_row() {
    let mut h = boot("OneWay", true, false);
    h.owe_concurrent(&[("owed-a", OrderSide::Buy), ("owed-b", OrderSide::Buy)]);
    let mut a = live_row("native-a");
    a["clOrdID"] = json!("owed-a");
    let mut b = live_row("native-b");
    b["clOrdID"] = json!("owed-b");
    let out = h.stream(vec![a, b], false);
    let r = deletes(&out).remove(0);
    let a=h.reply(&r,200,r#"{"code":0,"data":[{"orderID":"native-a","ordStatus":"Canceled"},{"orderID":"native-b","code":11011,"ordStatus":"Rejected"}]}"#);
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        h.e.store().order("native-b").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(command_terminals(&a), 0);
    assert_eq!(
        messages(&a, Level::Error)
            .iter()
            .filter(|m| m.contains("could not cancel late-landing"))
            .count(),
        1
    );
}
#[test]
fn native_filled_late_order_keeps_the_manual_position_diagnostic() {
    let mut h = boot("OneWay", true, false);
    h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    row["cumQty"] = json!("0.01");
    row["leavesQty"] = json!("0.09");
    let a = h.stream(vec![row], false);
    assert_eq!(deletes(&a).len(), 1);
    assert!(
        messages(&a, Level::Error)
            .iter()
            .any(|m| m.contains("check the position") && m.contains("native-a"))
    );
}
#[test]
fn native_reconciliation_waits_for_actual_signing_expiry_then_uses_primary() {
    let mut h = boot("OneWay", true, true);
    let (_, create) = h.owe("owed-a", OrderSide::Buy);
    let a = h.discover(
        &create,
        &rows(vec![
            rest_row("native-a", "owed-a"),
            rest_row("unrelated", "other"),
        ]),
        false,
    );
    let r = deletes(&a).remove(0);
    assert_eq!(p(&r, "orderID").as_deref(), Some("native-a"));
    assert_eq!(
        h.e.store().order("unrelated").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-phemex-access-token"))
            .unwrap()
            .1,
        "fixture-primary"
    );
}
#[test]
fn native_complete_absence_releases_the_exact_lost_client_generation() {
    let mut h = boot("OneWay", true, false);
    let (_, create) = h.owe("owed-a", OrderSide::Buy);
    let a = h.discover(&create, &rows(vec![]), false);
    assert!(deletes(&a).is_empty());
    let (_, a) = h.query_command(|command| Command::PlaceOrders {
        command,
        orders: vec![opts("owed-a")],
    });
    assert!(
        requests(&a)
            .iter()
            .any(|r| r.url.contains("/g-orders/create"))
    );
}
#[test]
fn native_incomplete_read_preserves_the_unresolved_generation() {
    let mut h = boot("OneWay", true, false);
    let (_, create) = h.owe("owed-a", OrderSide::Buy);
    h.discover(&create, &rows(vec![json!({"orderID":"unreadable"})]), false);
    let (id, a) = h.query_command(|command| Command::PlaceOrders {
        command,
        orders: vec![opts("owed-a")],
    });
    assert!(
        requests(&a)
            .iter()
            .all(|r| !r.url.contains("/g-orders/create"))
    );
    assert!(
        terminals(&a, id)[0]
            .as_ref()
            .unwrap_err()
            .contains("uncertain")
    );
    assert_eq!(terminals(&a, id).len(), 1);
    assert!(a.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command, error })
            if *command == id && error.outcome == Some(Outcome::NotSent)
    )));
}
#[test]
fn native_early_reconciliation_does_not_mark_orders_loaded() {
    let mut h = boot("OneWay", false, false);
    assert!(!h.e.store().loaded().orders);
    let (_, create) = h.lose("unlatched-a", OrderSide::Buy);
    h.discover(&create, &rows(vec![]), false);
    assert!(!h.e.store().loaded().orders);
    let (id, a) = h.cancel_all();
    assert!(
        terminals(&a, id)[0]
            .as_ref()
            .unwrap_err()
            .contains("snapshot is not loaded")
    );
}
#[test]
fn native_empty_one_and_many_row_reads_keep_the_loaded_snapshot_and_stream_rows() {
    for count in 0..=2 {
        let mut h = boot("OneWay", true, false);
        let (_, create) = h.owe("owed-a", OrderSide::Buy);
        let mut live = live_row("stream-kept");
        live["clOrdID"] = json!("unrelated-client");
        h.stream(vec![live], false);
        let body = rows(
            (0..count)
                .map(|i| rest_row(&format!("read-{i}"), "other"))
                .collect(),
        );
        let a = h.discover(&create, &body, false);
        assert!(h.e.store().loaded().orders);
        assert!(deletes(&a).is_empty());
        assert_eq!(
            h.e.store().order("stream-kept").unwrap().status,
            OrderStatus::Open
        );
        assert_eq!(h.e.store().orders().len(), count + 1);
    }
}

#[test]
fn repair_admission_refusal_has_no_optimistic_cancel_or_io_and_preserves_peers() {
    let mut h = boot("OneWay", true, false);
    h.owe("owed-a", OrderSide::Buy);
    let mut peers = Vec::new();
    let mut capacity_refused = false;
    for command in 1000..1256 {
        let actions = h.raw_event(Event::Command(Command::FetchAccount { command }));
        let before = peers.len();
        for action in &actions {
            if let Action::ManagedHttp(action) = action
                && let HttpAction::Prepare { preparation, .. } = action.as_ref()
            {
                peers.push(*preparation);
            }
        }
        if peers.len() == before {
            assert_eq!(command, 1127);
            // Two native account legs plus 127 real Read root/leg pairs fill 256.
            assert_eq!(peers.len(), 127, "the registry is full, not coalesced");
            let error = actions
                .iter()
                .find_map(|action| match action {
                    Action::Notice(Notice::CommandFailed { command: id, error })
                        if *id == command =>
                    {
                        Some(error)
                    }
                    _ => None,
                })
                .expect("the stopping command is explicitly refused for capacity");
            assert_eq!(error.message, "Engine request capacity reached");
            assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Internal));
            assert_eq!(error.outcome, None);
            assert_eq!(command_terminals(&actions), 1);
            assert!(messages(&actions, Level::Warn).is_empty());
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_))),
                "refusing command installs no preparation or physical I/O"
            );
            capacity_refused = true;
            break;
        }
    }
    assert!(
        capacity_refused,
        "the loop must reach the exact capacity refusal"
    );
    assert!(peers.iter().all(|p| h.e.http_preparation_pending(*p)));
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let actions = h.stream(vec![row.clone()], true);
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Open,
        "refused repair admission must not publish optimistic Canceling"
    );
    assert!(!actions.iter().any(|action| matches!(action, Action::Http(_)) || matches!(action, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare{request,..} if request.method==Method::Delete))),
        "refused repair admission must spend no physical call");
    assert!(
        peers.iter().all(|p| h.e.http_preparation_pending(*p)),
        "all independent peers survive"
    );
    assert_eq!(
        command_terminals(&actions),
        0,
        "repair refuses without a caller terminal"
    );
    let freed = peers.remove(0);
    h.e.http_preparation_failed(
        h.now,
        freed,
        safe_cex_engine::request_context::Error::State,
        safe_cex_engine::VenueError::new("capacity control releases one actual peer"),
    );
    h.e.drain_raw_actions();
    assert!(!h.e.http_preparation_pending(freed));
    let repeated = h.stream(vec![row], true);
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Open
    );
    assert!(!repeated.iter().any(|action| matches!(action, Action::Http(_)) || matches!(action, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare{request,..} if request.method==Method::Delete))), "repeat observation alone must not retry a refused repair, even with free capacity");
    assert!(peers.iter().all(|p| h.e.http_preparation_pending(*p)));
    assert_eq!(command_terminals(&repeated), 0);
}

#[test]
fn native_pre_expiry_reconcile_timer_is_inert_then_the_read_signs_primary() {
    let mut h = boot("OneWay", true, true);
    let (_, create) = h.owe("owed-a", OrderSide::Buy);
    let expiry = create
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("x-phemex-request-expiry"))
        .unwrap()
        .1
        .parse::<u64>()
        .unwrap() as f64
        * 1000.;
    let deadline = expiry + 5000.;
    let timer = h
        .timers
        .iter()
        .rev()
        .find(|(_, when)| *when == deadline)
        .unwrap()
        .0;
    let early = h.at(deadline - 1., Event::TimerFired { timer }, false);
    assert!(
        requests(&early)
            .iter()
            .all(|r| !r.url.contains("/g-orders/activeList?"))
    );
    assert!(deletes(&early).is_empty());
    assert_eq!(command_terminals(&early), 0);
    let later = h.discover(&create, &rows(vec![rest_row("native-a", "owed-a")]), false);
    assert_eq!(
        p(&deletes(&later)[0], "orderID").as_deref(),
        Some("native-a")
    );
}

#[test]
fn native_owed_order_seen_after_expiry_repairs_before_the_read_reply() {
    let mut h = boot("OneWay", true, false);
    let (_, create) = h.owe("owed-a", OrderSide::Buy);
    let expiry = create
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("x-phemex-request-expiry"))
        .unwrap()
        .1
        .parse::<u64>()
        .unwrap() as f64
        * 1000.;
    let deadline = expiry + 5000.;
    let timer = h
        .timers
        .iter()
        .rev()
        .find(|(_, when)| *when == deadline)
        .unwrap()
        .0;
    // Keep the genuine private socket live while its reconciliation read is pending.
    while h.now + 5000. < deadline {
        let socket = h.socket;
        let keepalive = h.at(
            h.now + 5000.,
            Event::WsMessage {
                socket,
                text: r#"{"result":"pong"}"#,
            },
            false,
        );
        assert!(
            !keepalive
                .iter()
                .any(|action| matches!(action, Action::WsClose { .. }))
        );
    }
    let read_actions = h.at(deadline, Event::TimerFired { timer }, false);
    let read = requests(&read_actions)
        .into_iter()
        .find(|r| r.url.contains("/g-orders/activeList?"))
        .expect("post-expiry read is genuinely in flight");
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let time = deadline + 5000.;
    let stamp = (time * 1e6) as u64;
    row["transactTimeNs"] = json!(stamp);
    let text = json!({"type":"incremental", "timestamp":stamp, "orders_p":[row]}).to_string();
    let socket = h.socket;
    let actions = h.at(
        time,
        Event::WsMessage {
            socket,
            text: &text,
        },
        false,
    );
    assert_eq!(
        deletes(&actions).len(),
        1,
        "stream proof does not wait for the pending read"
    );
    assert_eq!(
        p(&deletes(&actions)[0], "orderID").as_deref(),
        Some("native-a")
    );
    assert_eq!(command_terminals(&actions), 0);
    let later = h.reply(&read, 200, &rows(vec![]));
    assert!(
        deletes(&later).is_empty(),
        "the old read never duplicates the stream repair"
    );
}

#[test]
fn refused_repair_admission_emits_one_manual_cancel_error() {
    let mut h = boot("OneWay", true, false);
    h.owe("owed-a", OrderSide::Buy);
    let mut peers = Vec::new();
    for command in 1000..=1127 {
        let actions = h.raw_event(Event::Command(Command::FetchAccount { command }));
        for action in &actions {
            if let Action::ManagedHttp(action) = action
                && let HttpAction::Prepare { preparation, .. } = action.as_ref()
            {
                peers.push(*preparation);
            }
        }
        if command == 1127 {
            assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command:1127,error }) if error.message == "Engine request capacity reached")));
        }
    }
    assert_eq!(peers.len(), 127);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let refused = h.stream(vec![row.clone()], true);
    assert_eq!(
        messages(&refused, Level::Error)
            .iter()
            .filter(|message| message.contains("could not cancel late-landing")
                && message.contains("native-a"))
            .count(),
        1
    );
    assert_eq!(command_terminals(&refused), 0);
    let peer = peers.remove(0);
    h.e.http_preparation_failed(
        h.now,
        peer,
        safe_cex_engine::request_context::Error::State,
        safe_cex_engine::VenueError::new("release one real peer"),
    );
    h.e.drain_raw_actions();
    assert!(!h.e.http_preparation_pending(peer));
    let repeated = h.stream(vec![row], true);
    assert!(
        messages(&repeated, Level::Error)
            .iter()
            .all(|message| !message.contains("could not cancel late-landing"))
    );
    assert!(!repeated.iter().any(|action| matches!(action,Action::Http(_)) || matches!(action,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{request,..} if request.method == Method::Delete))));
    assert!(peers.iter().all(|peer| h.e.http_preparation_pending(*peer)));
}

#[test]
fn no_owed_cancel_all_mints_nothing_and_spends_no_grant() {
    let mut h = boot("OneWay", true, false);
    for _ in 0..257 {
        let (id, actions) = h.cancel_all();
        assert_eq!(terminals(&actions, id), vec![Ok("[]".into())]);
        assert!(messages(&actions, Level::Error).is_empty());
        assert!(messages(&actions, Level::Warn).is_empty());
        assert!(deletes(&actions).is_empty());
    }
    h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let actions = h.stream(vec![row], false);
    assert_eq!(
        deletes(&actions).len(),
        1,
        "no empty grant exhausted the real later mint"
    );
}

fn host_without_repair_primary(public_data_only: bool) {
    let mut e = support::prepared_engine(Options {
        public_data_only,
        key: public_data_only.then(|| "fixture-primary".into()),
        secret: public_data_only.then(|| "fixture-material".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "fixture-secondary".into(),
            secret: "fixture-secondary-material".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    e.start(1000.);
    e.drain_actions().for_each(drop);
    e.handle(
        1001.,
        Event::Command(Command::CancelSymbolOrders {
            command: 55,
            symbol: "BTCUSDT".into(),
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|action| matches!(action,Action::Notice(Notice::CommandFailed {command:55,error}) if error.kind == Some(safe_cex_engine::ErrorKind::AuthInvalid))));
    assert!(deletes(&actions).is_empty());
    assert!(
        messages(&actions, Level::Warn)
            .iter()
            .all(|message| !message.contains("repair"))
    );
}
#[test]
fn secondary_only_host_cannot_mint_or_send_late_repair() {
    host_without_repair_primary(false);
}
#[test]
fn public_only_host_cannot_mint_or_send_late_repair() {
    host_without_repair_primary(true);
}

#[test]
fn repair_provider_refusal_retires_once_without_resend() {
    let mut h = boot("OneWay", true, false);
    let (old, _) = h.owe("owed-a", OrderSide::Buy);
    let mut row = live_row("native-a");
    row["clOrdID"] = json!("owed-a");
    let held = h.stream(vec![row], true);
    let preparation = held
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.method == Method::Delete => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real repair preparation");
    h.e.http_preparation_failed(
        h.now,
        preparation,
        safe_cex_engine::request_context::Error::State,
        safe_cex_engine::VenueError::new("typed provider refusal")
            .kind(safe_cex_engine::ErrorKind::Internal),
    );
    let actions = h.e.drain_raw_actions();
    assert_eq!(
        messages(&actions, Level::Error)
            .iter()
            .filter(|message| message.contains("could not cancel late-landing")
                && message.contains("typed provider refusal"))
            .count(),
        1
    );
    assert_eq!(
        h.e.store().order("native-a").unwrap().status,
        OrderStatus::Open
    );
    assert!(terminals(&actions, old).is_empty());
    let replay = release(&mut h.e, h.now, held);
    assert!(deletes(&replay).is_empty());
    assert!(
        messages(&replay, Level::Error)
            .iter()
            .all(|message| !message.contains("could not cancel late-landing"))
    );
}
