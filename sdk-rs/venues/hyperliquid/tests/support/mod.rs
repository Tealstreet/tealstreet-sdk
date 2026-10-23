#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
pub const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100","prevDayPx":"90"},{"midPx":"200","markPx":"200","oraclePx":"200"}]]"#;
pub const HIP3_META: &str = r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10,"marginMode":"strictIsolated"}]},[{"midPx":"50","markPx":"50","oraclePx":"50"}]]"#;
pub const STATE: &str = r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"200","unrealizedPnl":"0","liquidationPx":"50","marginUsed":"20","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"20"},"time":1000}"#;
pub fn reply(e: &mut Engine<Hyperliquid>, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        1000.0,
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
        HIP3_META,
    );
    (engine, socket)
}
pub fn ready_with_policy(
    mode: &str,
    policy: safe_cex_engine::PublicFeedPolicy,
) -> (Engine<Hyperliquid>, SocketId, Vec<String>) {
    ready_identity_policy(mode, true, None, "30", None, policy, HIP3_META)
}
pub fn ready_with_hip3_meta(hip_meta: &str) -> (Engine<Hyperliquid>, SocketId) {
    let (engine, socket, _) = ready_identity_policy(
        "disabled",
        true,
        None,
        "30",
        None,
        safe_cex_engine::PublicFeedPolicy::Direct,
        hip_meta,
    );
    (engine, socket)
}
fn ready_identity_policy(
    mode: &str,
    hip: bool,
    builder: Option<safe_cex_venue_hyperliquid::Builder>,
    approval: &str,
    vault: Option<String>,
    policy: safe_cex_engine::PublicFeedPolicy,
    hip_meta: &str,
) -> (Engine<Hyperliquid>, SocketId, Vec<String>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
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
    let mut subscriptions = Vec::new();
    for _ in 0..20 {
        let actions = e.drain_actions().collect::<Vec<_>>();
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::Http(r) => {
                    let dex = serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap())
                        .unwrap()["dex"]
                        .as_str()
                        .map(str::to_owned);
                    let body=match kind(&r).as_str(){"metaAndAssetCtxs" if dex.is_some()=>hip_meta.into(),"metaAndAssetCtxs"=>META.to_owned(),"perpDexs"=>if hip{r#"[null,{"name":"xyz"}]"#.into()}else{"[null]".into()},"userAbstraction"=>format!("\"{mode}\""),"maxBuilderFee"=>approval.into(),"userFees"=>r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#.into(),"clearinghouseState" if dex.is_some()=>r#"{"assetPositions":[],"time":1000}"#.into(),"clearinghouseState"=>STATE.into(),"frontendOpenOrders"=>"[]".into(),"spotClearinghouseState"=>r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"#.into(),other=>panic!("unexpected read {other}")};
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

/// Drive a real close, its native retry, accepted replacement and handoff wake.
pub fn reconnect(e: &mut Engine<Hyperliquid>, socket: SocketId, now: f64) -> (SocketId, f64) {
    e.handle(now, Event::WsClosed { socket, code: 1006 });
    let (retry, at) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms > now && at_ms <= now + 16000.0 => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("native bounded reconnect backoff");
    e.handle(at, Event::TimerFired { timer: retry });
    let replacement = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("replacement socket");
    assert_ne!(replacement, socket);
    e.handle(
        at,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let batch: Vec<_> = e.drain_actions().collect();
    assert!(
        !batch.iter().any(|action| matches!(action, Action::Http(_))),
        "refresh must wait for handoff"
    );
    assert!(
        batch
            .iter()
            .any(|action| matches!(action, Action::WsSend { .. })),
        "real private subscription handoff"
    );
    let (wake, wake_at) = batch
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("ordinary handoff ingress");
    e.handle(wake_at, Event::TimerFired { timer: wake });
    (replacement, wake_at)
}
/// Drains actions with failures as the plain text the host renders.
pub trait Plain {
    fn drain_plain(&mut self) -> std::vec::IntoIter<safe_cex_engine::Action>;
}
impl<V: safe_cex_engine::Venue> Plain for safe_cex_engine::Engine<V> {
    fn drain_plain(&mut self) -> std::vec::IntoIter<safe_cex_engine::Action> {
        use safe_cex_engine::{Action, Notice};
        self.drain_actions()
            .map(|a| match a {
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    Action::Notice(Notice::CommandFailed {
                        command,
                        error: error.clone(),
                    })
                }
                a => a,
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
/// Drains the classified failures, dropping every other action.
pub fn failures<V: safe_cex_engine::Venue>(
    e: &mut safe_cex_engine::Engine<V>,
) -> Vec<safe_cex_engine::VenueError> {
    use safe_cex_engine::{Action, Notice};
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}
