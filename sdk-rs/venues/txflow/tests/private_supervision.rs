#[path = "support/account.rs"]
mod support;
use safe_cex_engine::{Action, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId};
use safe_cex_venue_txflow::{Options, TxFlow};
use std::collections::HashMap;
use support::*;

const PING: &str = r#"{"method":"ping"}"#;

struct Rig {
    engine: Engine<TxFlow>,
    now: f64,
    timers: HashMap<TimerId, f64>,
    node: SocketId,
    connected_at: f64,
}

fn connected(actions: &[Action], path: &str) -> Option<SocketId> {
    actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, url, .. } if url.ends_with(path) => Some(*socket),
        _ => None,
    })
}

fn closed(actions: &[Action], socket: SocketId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
}

impl Rig {
    fn new(policy: PublicFeedPolicy) -> (Self, Vec<Action>) {
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
            timers: HashMap::new(),
            node: SocketId(0),
            connected_at: NOW,
        };
        let mut actions = rig.drain();
        if connected(&actions, "/ws").is_none() {
            actions.extend(rig.advance(NOW + 25.0));
            rig.connected_at = rig.now;
        }
        rig.node = connected(&actions, "/ws").unwrap();
        (rig, actions)
    }
    fn drain(&mut self) -> Vec<Action> {
        let mut out = Vec::new();
        while self.engine.has_actions() {
            for action in
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now)
            {
                match action {
                    Action::Http(r) => {
                        let body = match kind(&r).as_str() {
                            "perpMeta" => META.into(),
                            "activeAssetCtx" => r#"{"success":true,"code":0,"data":{"instrumentId":1,"lastPrice":"100","markPx":"100","oraclePx":"100"}}"#.into(),
                            "clearinghouseState" => STATE.into(),
                            "frontendOpenOrders" => format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#),
                            "activeAssetData" => format!(r#"{{"success":true,"code":0,"data":{{"coin":"1","user":"{WALLET}","leverage":{{"mode":"oneWay","type":"Cross","value":10}}}}}}"#),
                            "userFills" => r#"{"success":true,"code":0,"data":{"data":[],"hasMore":false}}"#.into(),
                            other => panic!("unexpected read {other}"),
                        };
                        reply_at(&mut self.engine, &r, self.now, 200, &body);
                    }
                    Action::SetTimer { timer, at_ms } => {
                        self.timers.insert(timer, at_ms);
                        out.push(action);
                    }
                    Action::ClearTimer { timer } => {
                        self.timers.remove(&timer);
                        out.push(action);
                    }
                    other => out.push(other),
                }
            }
        }
        out
    }
    fn handle(&mut self, event: Event<'_>) -> Vec<Action> {
        self.engine.handle(self.now, event);
        self.drain()
    }
    fn advance(&mut self, until: f64) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut limit = 10_000;
        while let Some((&timer, &at)) = self
            .timers
            .iter()
            .filter(|(_, at)| **at <= until)
            .min_by(|a, b| a.1.total_cmp(b.1).then(a.0.0.cmp(&b.0.0)))
        {
            limit -= 1;
            assert!(limit > 0, "timers failed to settle");
            self.timers.remove(&timer);
            self.now = self.now.max(at);
            actions.extend(self.handle(Event::TimerFired { timer }));
        }
        self.now = self.now.max(until);
        actions
    }
    fn open(&mut self) -> Vec<String> {
        let socket = self.node;
        let mut actions = self.handle(Event::WsOpened { socket });
        actions.extend(self.advance(self.now + 1_000.0));
        actions
            .iter()
            .filter_map(|a| match a {
                Action::WsSend { socket: s, text } if *s == socket => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
    fn close_and_retry_delay(&mut self) -> f64 {
        let socket = self.node;
        let actions = self.handle(Event::WsClosed { socket, code: 1006 });
        let delays: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms - self.now),
                _ => None,
            })
            .collect();
        assert_eq!(delays.len(), 1, "one retry timer: {delays:?}");
        delays[0]
    }
    fn reconnect(&mut self, delay: f64) {
        let old = self.node;
        let actions = self.advance(self.now + delay);
        self.node = connected(&actions, "/ws").expect("one reconnect per close");
        assert_ne!(self.node, old);
    }
}

#[test]
fn flapping_private_node_backs_off_and_resubscribes_account_topics() {
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Disabled] {
        let (mut rig, _) = Rig::new(policy);
        for attempt in 0..7 {
            let subscribed = rig.open();
            for kind in ["orderUpdates", "clearinghouseState", "userAssetData"] {
                assert!(
                    subscribed.iter().any(|t| t.contains(&format!(
                        r#""type":"{kind}","subscription":{{"type":"{kind}","user":"{WALLET}"}}"#
                    ))),
                    "{policy:?} attempt {attempt} {kind}: {subscribed:?}"
                );
            }
            let nominal = (1_000.0 * 2f64.powi(attempt)).min(16_000.0);
            let delay = rig.close_and_retry_delay();
            assert!(
                (nominal * 0.75..=nominal).contains(&delay),
                "{policy:?} attempt {attempt}: {delay}"
            );
            rig.reconnect(delay);
        }
    }
}

#[test]
fn private_node_pings_expires_idle_and_never_opened_sockets_under_any_policy() {
    for policy in [
        PublicFeedPolicy::Direct,
        PublicFeedPolicy::Shared,
        PublicFeedPolicy::Disabled,
    ] {
        let (mut rig, actions) = Rig::new(policy);
        assert_eq!(
            connected(&actions, "/api-ws").is_some(),
            policy == PublicFeedPolicy::Direct
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::WsConnect {
                channel: Some(SocketChannel::Private),
                ..
            }
        )));
        let socket = rig.node;
        let started = rig.connected_at;
        assert!(!closed(&rig.advance(started + 9_999.0), socket));
        assert!(closed(&rig.advance(started + 10_000.0), socket));
        rig.node = connected(&rig.advance(started + 11_000.0), "/ws").expect("retry");
        let socket = rig.node;
        rig.handle(Event::WsOpened { socket });
        let opened = rig.now;
        let actions = rig.advance(opened + 29_999.0);
        let pings = actions
            .iter()
            .filter(
                |a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text == PING),
            )
            .count();
        assert_eq!(pings, 2, "{policy:?}");
        assert!(!closed(&actions, socket));
        assert!(closed(&rig.advance(opened + 30_000.0), socket));
    }
}
