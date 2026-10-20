//! Real-engine causal dispatch controls; event times never move backwards.
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use std::collections::{HashMap, VecDeque};
const KEY: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
const SECRET: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100"}]]"#;
const HIP: &str =
    r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50"}]]"#;
const STATE: &str = r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"200","unrealizedPnl":"0","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"20"},"time":1}"#;
fn kind(r: &HttpRequest) -> String {
    serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap()["type"]
        .as_str()
        .unwrap_or("")
        .into()
}
struct Driver {
    engine: Engine<Hyperliquid>,
    pending: VecDeque<Action>,
    timers: HashMap<TimerId, f64>,
    now: f64,
    sent: Vec<(f64, String)>,
}
impl Driver {
    fn new(private: bool) -> Self {
        let mut d = Self {
            engine: Engine::new(Hyperliquid::new(Options {
                public_data_only: !private,
                key: private.then(|| KEY.into()),
                secret: private.then(|| SECRET.into()),
                application_id: private.then(|| KEY.into()),
                ..Options::default()
            })),
            pending: VecDeque::new(),
            timers: HashMap::new(),
            now: 0.,
            sent: vec![],
        };
        d.engine.start(0.);
        d.collect();
        d
    }
    fn collect(&mut self) {
        for a in self.engine.drain_actions() {
            match &a {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(timer);
                }
                Action::Http(r) => self.sent.push((self.now, kind(r))),
                _ => {}
            }
            self.pending.push_back(a);
        }
    }
    fn event(&mut self, at: f64, event: Event<'_>) {
        assert!(at >= self.now);
        self.now = at;
        if let Event::TimerFired { timer } = event {
            self.timers.remove(&timer);
        }
        self.engine.handle(at, event);
        self.collect();
    }
    fn fire(&mut self) {
        let (&timer, &at) = self
            .timers
            .iter()
            .min_by(|a, b| a.1.total_cmp(b.1).then_with(|| a.0.0.cmp(&b.0.0)))
            .expect("live timer");
        self.event(at.max(self.now), Event::TimerFired { timer });
    }
    fn request(&mut self, filter: impl Fn(&HttpRequest) -> bool) -> HttpRequest {
        for _ in 0..200 {
            self.collect();
            if let Some(i) = self
                .pending
                .iter()
                .position(|a| matches!(a,Action::Http(r)if filter(r)))
            {
                let Action::Http(r) = self.pending.remove(i).unwrap() else {
                    unreachable!()
                };
                return r;
            }
            self.fire();
        }
        panic!("missing causal request");
    }
    fn reply(&mut self, r: &HttpRequest, status: u16, body: &str) {
        self.event(
            self.now + 1.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn defaults(&mut self, r: &HttpRequest) {
        let body = match kind(r).as_str() {
            "metaAndAssetCtxs" => META,
            "userFees" => r#"{"userCrossRate":"0.0003","userAddRate":"-0.0001"}"#,
            "perpDexs" => "[null]",
            "userAbstraction" => r#""disabled""#,
            "maxBuilderFee" => "30",
            "clearinghouseState" => STATE,
            "frontendOpenOrders" => "[]",
            other => panic!("unexpected bootstrap request {other}"),
        };
        self.reply(r, 200, body);
    }
    fn bootstrap(&mut self) -> SocketId {
        let mut socket = None;
        for _ in 0..200 {
            if let Some(i) = self
                .pending
                .iter()
                .position(|a| matches!(a, Action::WsConnect { .. }))
            {
                let Action::WsConnect { socket: s, .. } = self.pending.remove(i).unwrap() else {
                    unreachable!()
                };
                socket = Some(s);
            }
            if let Some(i) = self
                .pending
                .iter()
                .position(|a| matches!(a, Action::Http(_)))
            {
                let Action::Http(r) = self.pending.remove(i).unwrap() else {
                    unreachable!()
                };
                self.defaults(&r);
                continue;
            }
            let near = self.timers.values().any(|at| *at <= self.now + 2000.);
            if let Some(socket) = socket.filter(|_| !near) {
                self.event(self.now + 1., Event::WsOpened { socket });
                return socket;
            }
            self.fire();
        }
        panic!("bootstrap stalled");
    }
}
#[test]
fn public_bootstrap_waits_for_all_three_results_and_publishes_complete_catalog_with_cached_fees() {
    for fee_status in [200, 503] {
        let mut d = Driver::new(false);
        let core = d.request(|r| kind(r) == "metaAndAssetCtxs");
        d.reply(&core, 200, META);
        assert!(!d.engine.store().loaded().markets);
        assert!(!d.engine.store().loaded().tickers);
        let fees = d.request(|r| kind(r) == "userFees");
        assert_eq!(d.now, 1000.);
        let dexs = d.request(|r| kind(r) == "perpDexs");
        assert_eq!(d.now, 2000.);
        d.reply(&dexs, 200, r#"[null,{"name":"xyz"}]"#);
        assert!(!d.engine.store().loaded().markets);
        assert!(
            !d.pending
                .iter()
                .any(|a| matches!(a,Action::Http(r)if kind(r)=="metaAndAssetCtxs"))
        );
        d.reply(
            &fees,
            fee_status,
            r#"{"userCrossRate":"0.0003","userAddRate":"-0.0001"}"#,
        );
        let hip = d.request(|r| kind(r) == "metaAndAssetCtxs");
        assert_eq!(d.now, 3000.);
        assert!(!d.engine.store().loaded().markets);
        d.reply(&hip, 200, HIP);
        assert_eq!(d.engine.store().markets().len(), 2);
        assert!(d.engine.store().loaded().markets && d.engine.store().loaded().tickers);
        for market in d.engine.store().markets() {
            assert_eq!(
                market.taker_fee,
                Some(if fee_status == 200 { 0.0003 } else { 0.00045 })
            );
            assert_eq!(
                market.maker_fee,
                Some(if fee_status == 200 { -0.0001 } else { 0.00015 })
            );
        }
        assert_eq!(
            d.sent.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>(),
            [
                "metaAndAssetCtxs",
                "userFees",
                "perpDexs",
                "metaAndAssetCtxs"
            ]
        );
    }
}
#[test]
fn private_mode_and_core_state_precede_catalog_and_early_state_is_retained_typed() {
    let mut d = Driver::new(true);
    let mode = d.request(|r| kind(r) == "userAbstraction");
    assert_eq!(d.now, 0.);
    d.reply(&mode, 200, r#""disabled""#);
    let builder = d.request(|r| kind(r) == "maxBuilderFee");
    d.reply(&builder, 200, "30");
    let state = d.request(|r| kind(r) == "clearinghouseState");
    assert_eq!(d.now, 1000.);
    d.reply(&state, 200, STATE);
    assert!(!d.engine.store().loaded().balance);
    assert!(d.engine.store().positions().is_empty());
    let core = d.request(|r| kind(r) == "metaAndAssetCtxs");
    assert_eq!(d.now, 1100.);
    d.reply(&core, 200, META);
    let fee = d.request(|r| kind(r) == "userFees");
    d.defaults(&fee);
    let dexs = d.request(|r| kind(r) == "perpDexs");
    d.defaults(&dexs);
    assert_eq!(d.engine.store().positions()[0].contracts, 2.);
    assert_eq!(d.engine.store().balance().total, 1000.);
    assert_eq!(
        d.sent
            .iter()
            .filter(|(_, k)| k == "userAbstraction")
            .count(),
        1
    );
    assert_eq!(d.sent.iter().filter(|(_, k)| k == "userFees").count(), 1);
}
#[test]
fn queued_amend_revalidates_current_order_before_cancel_or_preplacement() {
    for terminal in [false, true] {
        let mut d = Driver::new(true);
        let socket = d.bootstrap();
        let open = r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#;
        d.event(d.now + 1., Event::WsMessage { socket, text: open });
        d.event(
            d.now + 1.,
            Event::Command(Command::FetchCandles {
                command: 10,
                symbol: "BTC-PERP".into(),
                interval: "1m".into(),
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let candle = d.request(|r| kind(r) == "candleSnapshot");
        d.event(
            d.now + 1.,
            Event::Command(Command::AmendOrder {
                command: 20,
                id: "9".into(),
                price: Some(85.),
                amount: None,
            }),
        );
        let changed = if terminal {
            open.replace("\"status\":\"open\"", "\"status\":\"canceled\"")
        } else {
            open.replace("\"limitPx\":\"90\"", "\"limitPx\":\"91\"")
                .replace("\"sz\":\"1\"", "\"sz\":\"0.5\"")
        };
        d.event(
            d.now + 1.,
            Event::WsMessage {
                socket,
                text: &changed,
            },
        );
        let _ = candle; // Its response is irrelevant: admission never waits for completion.
        for _ in 0..30 {
            if d.pending.iter().any(|a| {
                matches!(
                    a,
                    Action::Notice(Notice::CommandDone {
                        command: 20,
                        result: Err(_)
                    })
                )
            }) {
                break;
            }
            d.fire();
        }
        assert!(d.pending.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 20,
                result: Err(_)
            })
        )));
        assert!(
            !d.pending
                .iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
        );
        if !terminal {
            let order = d.engine.store().order("9").unwrap();
            assert_eq!(order.price, 91.);
            assert_eq!(order.remaining, 0.5);
            assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Open);
        }
    }
}
#[test]
fn queued_write_nonce_uses_actual_dispatch_after_weighted_wait() {
    let mut d = Driver::new(true);
    d.bootstrap();
    d.event(
        d.now + 1.,
        Event::Command(Command::FetchCandles {
            command: 10,
            symbol: "BTC-PERP".into(),
            interval: "1m".into(),
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        }),
    );
    let _candle = d.request(|r| kind(r) == "candleSnapshot");
    let enqueued_at = d.now + 1.;
    let order:PlaceOrderOpts=serde_json::from_str(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1,"clientOrderId":"0x00000000000000000000000000000001"}"#).unwrap();
    d.event(
        enqueued_at,
        Event::Command(Command::PlaceOrders {
            command: 20,
            orders: vec![order],
        }),
    );
    assert!(
        !d.pending
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    let r = d.request(|r| r.url.ends_with("/exchange"));
    let body: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert!(d.now > enqueued_at);
    assert_eq!(body["nonce"].as_u64(), Some(d.now as u64));
    assert_eq!(
        body["action"]["orders"][0]["c"],
        "0x00000000000000000000000000000001"
    );
    assert!(body["signature"]["r"].as_str().unwrap().starts_with("0x"));
}
#[test]
fn hung_startup_builder_settles_once_at_eight_seconds_and_late_reply_cannot_enable_fees() {
    let mut d = Driver::new(true);
    let builder = d.request(|r| kind(r) == "maxBuilderFee");
    let _socket = d.bootstrap();
    assert!(d.now < 8000.);
    let order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
    )
    .unwrap();
    d.event(
        d.now + 1.,
        Event::Command(Command::PlaceOrders {
            command: 20,
            orders: vec![order.clone()],
        }),
    );
    let first = d.request(|r| r.url.ends_with("/exchange"));
    assert_eq!(d.now, 8000.);
    let payload: serde_json::Value = serde_json::from_str(first.body.as_ref().unwrap()).unwrap();
    assert!(payload["action"].get("builder").is_none());
    d.reply(&builder, 200, "30");
    d.reply(&first,200,r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":101}}]}}}"#);
    d.event(
        d.now + 1.,
        Event::Command(Command::PlaceOrders {
            command: 21,
            orders: vec![order],
        }),
    );
    let second = d.request(|r| r.url.ends_with("/exchange"));
    let payload: serde_json::Value = serde_json::from_str(second.body.as_ref().unwrap()).unwrap();
    assert!(payload["action"].get("builder").is_none());
    assert_eq!(
        d.sent
            .iter()
            .filter(|(_, kind)| kind == "maxBuilderFee")
            .count(),
        1
    );
}
#[test]
fn initial_open_keeps_the_one_core_bootstrap_read_but_reopen_refreshes() {
    let mut d = Driver::new(true);
    let socket = d.bootstrap();
    assert_eq!(
        d.sent
            .iter()
            .filter(|(_, kind)| kind == "clearinghouseState")
            .count(),
        1
    );
    assert!(
        d.pending
            .iter()
            .any(|a| matches!(a,Action::WsSend{socket:s,..}if *s==socket)),
        "bootstrap must actually open and subscribe"
    );
    d.event(d.now + 1., Event::WsClosed { socket, code: 1006 });
    let mut replacement = None;
    for _ in 0..30 {
        if let Some(i) = d
            .pending
            .iter()
            .position(|a| matches!(a, Action::WsConnect { .. }))
        {
            let Action::WsConnect { socket, .. } = d.pending.remove(i).unwrap() else {
                unreachable!()
            };
            replacement = Some(socket);
            break;
        }
        d.fire();
    }
    let replacement = replacement.expect("actual reconnect");
    assert_ne!(socket, replacement);
    d.event(
        d.now + 1.,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let request = d.request(|r| kind(r) == "clearinghouseState");
    d.reply(&request, 200, STATE);
    assert_eq!(
        d.sent
            .iter()
            .filter(|(_, kind)| kind == "clearinghouseState")
            .count(),
        2
    );
}
#[test]
fn failed_or_timed_out_identity_resolution_still_starts_public_catalog_and_ignores_late_role() {
    for timeout in [false, true] {
        let mut d = Driver {
            engine: Engine::new(Hyperliquid::new(Options {
                key: Some(KEY.into()),
                secret: Some(SECRET.into()),
                ..Options::default()
            })),
            pending: VecDeque::new(),
            timers: HashMap::new(),
            now: 0.,
            sent: vec![],
        };
        d.engine.start(0.);
        d.collect();
        let role = d.request(|r| kind(r) == "userRole");
        assert_eq!(d.now, 0.);
        assert_eq!(d.sent.len(), 1);
        if !timeout {
            d.reply(&role, 503, "unavailable");
        }
        let core = d.request(|r| kind(r) == "metaAndAssetCtxs");
        assert_eq!(d.now, if timeout { 6000. } else { 3000. });
        if timeout {
            d.reply(&role, 200, r#"{"role":"user","data":{}}"#);
        }
        d.reply(&core, 200, META);
        let fee = d.request(|r| kind(r) == "userFees");
        d.defaults(&fee);
        let dexs = d.request(|r| kind(r) == "perpDexs");
        d.defaults(&dexs);
        assert!(d.engine.store().loaded().markets);
        assert!(!d.sent.iter().any(|(_, kind)| {
            ["userAbstraction", "clearinghouseState", "maxBuilderFee"].contains(&kind.as_str())
        }));
    }
}

#[test]
fn actual_cancel_batch_weight_changes_at_forty_without_response_refunds() {
    for count in [39, 40] {
        let mut d = Driver::new(true);
        let socket = d.bootstrap();
        let rows = (1..=count).map(|id| format!(r#"{{"order":{{"coin":"BTC","oid":{id},"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}},"status":"open"}}"#)).collect::<Vec<_>>().join(",");
        let frame = format!(r#"{{"channel":"orderUpdates","data":[{rows}]}}"#);
        d.event(
            d.now + 1.,
            Event::WsMessage {
                socket,
                text: &frame,
            },
        );
        d.event(
            d.now + 1.,
            Event::Command(Command::CancelOrders {
                command: 30,
                ids: (1..=count).map(|id| id.to_string()).collect(),
            }),
        );
        let cancel = d.request(|r| r.url.ends_with("/exchange"));
        let sent_at = d.now;
        let payload: serde_json::Value =
            serde_json::from_str(cancel.body.as_ref().unwrap()).unwrap();
        assert_eq!(
            payload["action"]["cancels"].as_array().unwrap().len(),
            count
        );
        let statuses = vec!["\"success\""; count].join(",");
        let ack = format!(
            r#"{{"status":"ok","response":{{"type":"cancel","data":{{"statuses":[{statuses}]}}}}}}"#
        );
        d.reply(&cancel, 200, &ack);
        let order: PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
        )
        .unwrap();
        d.event(
            d.now + 1.,
            Event::Command(Command::PlaceOrders {
                command: 31,
                orders: vec![order],
            }),
        );
        assert!(
            !d.pending
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
        let _next = d.request(|r| r.url.ends_with("/exchange"));
        assert_eq!(d.now, sent_at + if count == 39 { 50. } else { 100. });
    }
}
