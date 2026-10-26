//! The account node multiplexes public data and private subscriptions.
#[path = "support/account.rs"]
mod support;
use safe_cex_engine::command::{HostInput, decode_host_input};
use safe_cex_engine::{
    Action, Command, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId, Venue,
};
use safe_cex_venue_txflow::{Options, TxFlow};
use support::*;

struct Rig {
    engine: Engine<TxFlow>,
    now: f64,
    timers: Vec<(TimerId, f64)>,
    sockets: Vec<SocketId>,
    closed: Vec<SocketId>,
    sent: Vec<(SocketId, String)>,
    reads: Vec<String>,
}
impl Rig {
    fn new(policy: PublicFeedPolicy) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
            testnet: true,
            application_id: Some(WALLET.into()),
            secret: Some(format!("0x{:064x}", 1)),
            ..Options::default()
        }));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(NOW);
        let mut rig = Self {
            engine,
            now: NOW,
            timers: Vec::new(),
            sockets: Vec::new(),
            closed: Vec::new(),
            sent: Vec::new(),
            reads: Vec::new(),
        };
        rig.run_until(NOW + 2000.);
        rig
    }
    fn run_until(&mut self, until: f64) {
        for _ in 0..1000 {
            let actions =
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now);
            for action in actions {
                match action {
                    Action::Http(request) => {
                        let name = kind(&request);
                        self.reads.push(name.clone());
                        let body = match name.as_str() {
                            "perpMeta" => META.into(),
                            "activeAssetCtx" => r#"{"success":true,"code":0,"data":{"instrumentId":1,"lastPrice":"100","markPx":"100","oraclePx":"100"}}"#.into(),
                            "clearinghouseState" => STATE.into(),
                            "frontendOpenOrders" => format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#),
                            "activeAssetData" => format!(r#"{{"success":true,"code":0,"data":{{"coin":"1","user":"{WALLET}","leverage":{{"mode":"oneWay","type":"Cross","value":10}}}}}}"#),
                            "userFills" => r#"{"success":true,"code":0,"data":{"data":[],"hasMore":false}}"#.into(),
                            other => panic!("unexpected read {other}"),
                        };
                        reply_at(&mut self.engine, &request, self.now, 200, &body);
                    }
                    Action::WsConnect { headers, .. } if !headers.is_empty() => {
                        panic!("unexpected authenticated upgrade")
                    }
                    Action::WsConnect {
                        socket,
                        url,
                        channel,
                        ..
                    } => {
                        assert_eq!(channel, Some(SocketChannel::Private));
                        assert_eq!(url, "wss://rpc.txflow-testnet.xyz/ws");
                        self.sockets.push(socket);
                        self.engine.handle(self.now, Event::WsOpened { socket });
                    }
                    Action::WsSend { socket, text } => self.sent.push((socket, text)),
                    Action::WsClose { socket } => self.closed.push(socket),
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => {
                        self.timers.retain(|(pending, _)| *pending != timer)
                    }
                    Action::Notice(_) => {}
                    Action::ManagedHttp(_) => {
                        panic!("managed HTTP requires the prepared test-driver handshake")
                    }
                    Action::RetireAddressSetup { .. } => {
                        panic!("unexpected address setup retirement")
                    }
                    Action::Sign { .. }
                    | Action::SignedOutbound { .. }
                    | Action::SignPayload { .. } => {
                        panic!("unexpected delegated signing request")
                    }
                }
            }
            if self.engine.has_actions() {
                continue;
            }
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some((timer, at)) = self.timers.first().copied().filter(|(_, at)| *at <= until)
            else {
                return;
            };
            self.timers.remove(0);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
        }
        panic!("scripted timer loop did not settle");
    }
    fn message(&mut self, socket: SocketId, now: f64, text: &str) {
        self.now = now;
        message(&mut self.engine, socket, now, text);
        self.run_until(now);
    }
    fn assert_private_subscriptions(&self, socket: SocketId) {
        let subscriptions: Vec<serde_json::Value> = self
            .sent
            .iter()
            .filter(|(id, _)| *id == socket)
            .filter_map(|(_, text)| serde_json::from_str::<serde_json::Value>(text).ok())
            .filter(|frame| frame["method"] == "subscribe")
            .collect();
        assert_eq!(
            subscriptions.len(),
            3,
            "only the three paced private intents may be sent: {subscriptions:?}"
        );
        for kind in ["orderUpdates", "clearinghouseState", "userAssetData"] {
            assert!(subscriptions.iter().any(|frame| frame["type"] == kind
                && frame["subscription"]["type"] == kind
                && frame["subscription"]["user"] == WALLET));
        }
    }
}

#[test]
fn non_direct_policies_keep_rest_metadata_private_account_and_only_private_node_topics() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        assert_eq!(
            rig.sockets.len(),
            1,
            "the public API-service socket is suppressed"
        );
        let socket = rig.sockets[0];
        rig.assert_private_subscriptions(socket);
        assert!(rig.reads.iter().any(|kind| kind == "perpMeta"));
        assert!(rig.reads.iter().any(|kind| kind == "activeAssetCtx"));
        assert!(rig.reads.iter().any(|kind| kind == "clearinghouseState"));
        assert_eq!(rig.engine.store().market("BTC-USDC").unwrap().id, "1");
        assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap().last, 100.);
        assert_eq!(rig.engine.store().positions()[0].contracts, 2.);
        assert_eq!(rig.engine.store().orders()[0].id, "42");
        assert_eq!(rig.engine.store().balance().total, 1000.);
        assert_eq!(rig.engine.store().balance().upnl, 5.);
        assert_eq!(
            rig.engine.store().setting("BTC-USDC").unwrap().leverage,
            10.
        );
        let sent = rig.sent.len();
        for command in [
            Command::SubscribeOrderBook {
                symbol: "BTC-USDC".into(),
            },
            Command::SubscribeCandles {
                symbol: "BTC-USDC".into(),
                interval: safe_cex_engine::model::Interval::M1,
            },
        ] {
            rig.engine.handle(NOW + 2000., Event::Command(command));
        }
        rig.run_until(NOW + 2000.);
        assert_eq!(rig.sent.len(), sent);
        assert!(rig.engine.venue().book("BTC-USDC").is_none());
        rig.engine
            .handle(NOW + 2500., Event::WsClosed { socket, code: 1006 });
        rig.run_until(NOW + 5000.);
        assert_eq!(rig.sockets.len(), 2);
        rig.assert_private_subscriptions(rig.sockets[1]);
    }
}

#[test]
fn mixed_private_node_ignores_unsolicited_public_payloads_and_retains_private_pushes() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        let socket = rig.sockets[0];
        if policy == PublicFeedPolicy::Shared {
            let HostInput::PublicFeedUpdate(update) = decode_host_input(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"BTC-USDC","last":110,"mark":111,"index":112}]}"#).unwrap() else { panic!("shared feed") };
            rig.engine.apply_public_feed(NOW + 2000., update).unwrap();
        }
        let before = rig.engine.store().ticker("BTC-USDC").unwrap().clone();
        rig.message(socket, NOW + 2500., r#"{"channel":"allAssetCtxs","data":{"ctxs":[{"coin":"1","ctx":{"markPx":"999","oraclePx":"998","openInterest":"7","funding":"1","lastPrice":"999","midPx":"999.5","prevDayPx":"10","dayBaseVlm":"3","dayNtlVlm":"999","nextFundingTime":7200000}}]}}"#);
        assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap(), &before);
        assert_eq!(
            rig.engine.store().balance().upnl,
            5.,
            "equity remains an authoritative private field"
        );
        let mut state = serde_json::from_str::<serde_json::Value>(STATE).unwrap()["data"].clone();
        state["time"] = serde_json::json!(NOW + 3000.);
        state["assetPositions"][0]["position"]["szi"] = serde_json::json!("3");
        state["marginSummary"]["accountValue"] = serde_json::json!("1200");
        state["marginSummary"]["totalMarginUsed"] = serde_json::json!("30");
        state["marginSummary"]["totalRawUsd"] = serde_json::json!("1194");
        rig.message(
            socket,
            NOW + 3000.,
            &format!(r#"{{"channel":"clearinghouseState","data":{state}}}"#),
        );
        assert_eq!(rig.engine.store().positions()[0].contracts, 3.);
        assert_eq!(rig.engine.store().balance().total, 1200.);
        assert_eq!(rig.engine.store().balance().free, 1170.);
        assert_eq!(rig.engine.store().balance().upnl, 6.);
        rig.message(socket, NOW + 3100., &format!(r#"{{"channel":"userAssetData","data":{{"user":"{WALLET}","coins":[{{"coin":"1","leverage":{{"mode":"oneWay","type":"Cross","value":8}}}}]}}}}"#));
        assert_eq!(rig.engine.store().setting("BTC-USDC").unwrap().leverage, 8.);
        assert_eq!(rig.engine.store().ticker("BTC-USDC").unwrap(), &before);
    }
}

#[test]
fn non_direct_rate_limit_controls_still_recover_private_subscriptions_and_heartbeat() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        let socket = rig.sockets[0];
        rig.message(
            socket,
            NOW + 2500.,
            r#"{"type":"error","code":"RISK_RATE_LIMITED","message":"rate limit exceeded"}"#,
        );
        assert!(rig.closed.contains(&socket));
        rig.run_until(NOW + 5000.);
        assert_eq!(rig.sockets.len(), 2);
        let replacement = rig.sockets[1];
        rig.assert_private_subscriptions(replacement);
        rig.run_until(NOW + 15000.);
        assert!(
            rig.sent
                .iter()
                .any(|(id, text)| *id == replacement && text == r#"{"method":"ping"}"#)
        );
    }
}
