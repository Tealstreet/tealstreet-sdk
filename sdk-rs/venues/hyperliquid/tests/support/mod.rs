#![allow(dead_code)]
use safe_cex_engine::{Action, Event, HttpRequest, Response, SocketId, TimerId, Venue};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use std::collections::{HashMap, VecDeque};
use std::ops::{Deref, DerefMut};
/// Scripted fixtures retain all emitted actions. Only explicit HTTP-wait helpers
/// advance causal timers. Host event time is monotonic; raw venue timestamps
/// remain unchanged, independently exercising the account's revision fences.
pub struct TestEngine<V: Venue> {
    inner: safe_cex_engine::Engine<V>,
    pending: VecDeque<Action>,
    timers: HashMap<TimerId, f64>,
    pub now: f64,
}
impl<V: Venue> Deref for TestEngine<V> {
    type Target = safe_cex_engine::Engine<V>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<V: Venue> DerefMut for TestEngine<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl<V: Venue> TestEngine<V> {
    pub fn new(venue: V) -> Self {
        Self {
            inner: safe_cex_engine::Engine::new(venue),
            pending: VecDeque::new(),
            timers: HashMap::new(),
            now: 0.0,
        }
    }
    pub fn start(&mut self, now: f64) {
        self.now = now;
        self.inner.start(now);
    }
    pub fn handle(&mut self, now: f64, event: Event<'_>) {
        let now = now.max(self.now);
        self.now = now;
        if let Event::TimerFired { timer } = event {
            self.timers.remove(&timer);
        }
        self.inner.handle(now, event);
    }
    fn collect(&mut self) {
        for action in self.inner.drain_actions() {
            match &action {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(timer);
                }
                _ => {}
            }
            self.pending.push_back(action);
        }
    }
    pub fn drain_actions(&mut self) -> std::collections::vec_deque::IntoIter<Action> {
        self.collect();
        std::mem::take(&mut self.pending).into_iter()
    }
    pub fn has_near_timer(&self) -> bool {
        self.timers.values().any(|at| *at <= self.now + 2000.0)
    }
    pub fn advance_next_timer(&mut self) -> bool {
        self.collect();
        let Some((&timer, &at)) = self
            .timers
            .iter()
            .min_by(|a, b| a.1.total_cmp(b.1).then_with(|| a.0.0.cmp(&b.0.0)))
        else {
            return false;
        };
        self.handle(at.max(self.now), Event::TimerFired { timer });
        true
    }
    pub fn wait_done(&mut self, command: u32) -> Vec<Action> {
        for _ in 0..200 {
            self.collect();
            if self.pending.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandDone { command: c, .. }) if *c == command)) {
                return self.drain_actions().collect();
            }
            assert!(
                self.advance_next_timer(),
                "expected command completion, no live timer remains"
            );
        }
        panic!("command did not complete within 200 causal timers");
    }
    pub fn wait_http(&mut self, matches: impl Fn(&HttpRequest) -> bool) -> HttpRequest {
        for _ in 0..200 {
            self.collect();
            if let Some(index) = self
                .pending
                .iter()
                .position(|action| matches!(action, Action::Http(r) if matches(r)))
            {
                let Action::Http(request) = self.pending.remove(index).unwrap() else {
                    unreachable!()
                };
                return request;
            }
            assert!(
                self.advance_next_timer(),
                "expected an HTTP request, no live timer remains"
            );
        }
        panic!("HTTP request did not appear within 200 causal timers");
    }
}
use TestEngine as Engine;
pub const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100","prevDayPx":"90"},{"midPx":"200","markPx":"200","oraclePx":"200"}]]"#;
pub const STATE: &str = r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"200","unrealizedPnl":"0","liquidationPx":"50","marginUsed":"20","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"20"},"time":1000}"#;
pub fn reply(e: &mut Engine<Hyperliquid>, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        e.now,
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
pub fn kind(r: &HttpRequest) -> String {
    serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap()["type"]
        .as_str()
        .unwrap_or("")
        .into()
}
pub fn ready(mode: &str) -> (Engine<Hyperliquid>, SocketId) {
    ready_with_dex(mode, false)
}
pub fn ready_with_dex(mode: &str, hip: bool) -> (Engine<Hyperliquid>, SocketId) {
    ready_custom(mode, hip, None, "30")
}
pub fn ready_custom(
    mode: &str,
    hip: bool,
    builder: Option<safe_cex_venue_hyperliquid::Builder>,
    approval: &str,
) -> (Engine<Hyperliquid>, SocketId) {
    ready_identity(mode, hip, builder, approval, None)
}
pub fn ready_identity(
    mode: &str,
    hip: bool,
    builder: Option<safe_cex_venue_hyperliquid::Builder>,
    approval: &str,
    vault: Option<String>,
) -> (Engine<Hyperliquid>, SocketId) {
    let (engine, socket, _) = ready_identity_policy(
        mode,
        hip,
        builder,
        approval,
        vault,
        safe_cex_engine::PublicFeedPolicy::Direct,
    );
    (engine, socket)
}
pub fn ready_with_policy(
    mode: &str,
    policy: safe_cex_engine::PublicFeedPolicy,
) -> (Engine<Hyperliquid>, SocketId, Vec<String>) {
    ready_identity_policy(mode, true, None, "30", None, policy)
}
fn ready_identity_policy(
    mode: &str,
    hip: bool,
    builder: Option<safe_cex_venue_hyperliquid::Builder>,
    approval: &str,
    vault: Option<String>,
    policy: safe_cex_engine::PublicFeedPolicy,
) -> (Engine<Hyperliquid>, SocketId, Vec<String>) {
    let mut e = Engine::new(Hyperliquid::new(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        builder,
        vault,
        ..Options::default()
    }));
    e.set_public_feed_policy(policy).unwrap();
    e.start(1000.0);
    let mut socket = None;
    let mut answered = std::collections::HashSet::new();
    let mut subscriptions = Vec::new();
    for _ in 0..200 {
        let actions = e.drain_actions().collect::<Vec<_>>();
        if actions.is_empty() {
            let basic = [
                "userAbstraction:",
                "maxBuilderFee:",
                "clearinghouseState:",
                "frontendOpenOrders:",
            ]
            .iter()
            .all(|key| answered.contains(*key));
            let hip_ready = !hip
                || ["clearinghouseState:xyz", "frontendOpenOrders:xyz"]
                    .iter()
                    .all(|key| answered.contains(*key));
            let spot_ready = !matches!(mode, "unifiedAccount" | "portfolioMargin")
                || answered.contains("spotClearinghouseState:");
            if socket.is_some() && basic && hip_ready && spot_ready && !e.has_near_timer() {
                break;
            }
            assert!(e.advance_next_timer(), "private bootstrap stalled");
            continue;
        }
        for a in actions {
            match a {
                Action::Http(r) => {
                    let dex = serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap())
                        .unwrap()["dex"]
                        .as_str()
                        .map(str::to_owned);
                    answered.insert(format!("{}:{}", kind(&r), dex.as_deref().unwrap_or("")));
                    let body=match kind(&r).as_str(){"metaAndAssetCtxs" if dex.is_some()=>r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50","markPx":"50","oraclePx":"50"}]]"#.into(),"metaAndAssetCtxs"=>META.to_owned(),"perpDexs"=>if hip{r#"[null,{"name":"xyz"}]"#.into()}else{"[null]".into()},"userAbstraction"=>format!("\"{mode}\""),"maxBuilderFee"=>approval.into(),"userFees"=>r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#.into(),"clearinghouseState" if dex.is_some()=>r#"{"assetPositions":[],"time":1000}"#.into(),"clearinghouseState"=>STATE.into(),"frontendOpenOrders"=>"[]".into(),"spotClearinghouseState"=>r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"#.into(),other=>panic!("unexpected read {other}")};
                    reply(&mut e, &r, 200, &body);
                }
                Action::WsConnect { socket: s, .. } => {
                    socket = Some(s);
                    e.handle(1000.0, Event::WsOpened { socket: s });
                }
                Action::WsSend { text, .. } => subscriptions.push(text),
                _ => {}
            }
        }
    }
    (e, socket.unwrap(), subscriptions)
}
pub fn message(e: &mut Engine<Hyperliquid>, s: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket: s, text });
}
