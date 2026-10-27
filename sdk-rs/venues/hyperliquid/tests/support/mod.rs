#![allow(dead_code)]

use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
pub const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100","prevDayPx":"90"},{"midPx":"200","markPx":"200","oraclePx":"200"}]]"#;
pub const HIP3_META: &str = r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10,"marginMode":"strictIsolated"}]},[{"midPx":"50","markPx":"50","oraclePx":"50"}]]"#;
pub const STATE: &str = r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"200","unrealizedPnl":"0","liquidationPx":"50","marginUsed":"20","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"20"},"time":1000}"#;
pub fn reply(e: &mut Engine<Hyperliquid>, r: &HttpRequest, status: u16, body: &str) {
    let now = safe_cex_engine::test_support::dispatch_now(e).max(1000.0);
    e.handle(
        now,
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
    let (signer, identities) = agent(TEST_SECRET);
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        credentials: identities.primary.clone(),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        builder,
        vault,
        ..Options::default()
    }));
    e.set_signer(Some(signer)).unwrap();
    e.set_identities(identities).unwrap();
    e.initialize_address_nonce(999.0).unwrap();
    assert!(e.address_nonce_setup_ready());
    e.set_public_feed_policy(policy).unwrap();
    e.start(1000.0);
    let mut socket = None;
    let mut subscriptions = Vec::new();
    for _ in 0..20 {
        let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
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
                Action::SetTimer { timer, at_ms } if at_ms == 1000.0 => {
                    e.handle(at_ms, Event::TimerFired { timer });
                }
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
    let (retry, at) = prepared_now(e)
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if at_ms > now && at_ms <= now + 16000.0 => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("native bounded reconnect backoff");
    e.handle(at, Event::TimerFired { timer: retry });
    let replacement = prepared_now(e)
        .into_iter()
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
    let batch: Vec<_> = prepared_now(e).into_iter().collect();
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
        prepared_now(self)
            .into_iter()
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
    prepared_now(e)
        .into_iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}

pub const TEST_SECRET: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
pub fn agent(
    scalar: &str,
) -> (
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
) {
    let policy = serde_json::from_str(r#"{"primary":{"source":"scalarOnly","signingAlgorithm":"secp256k1Digest","secretEncoding":"hex","trimSigning":false}}"#).unwrap();
    let (provider, identities, _) = safe_cex_crypto::signer::LocalSigner::from_json_with_facts(
        serde_json::json!({"primary":{"secret":scalar}}).to_string(),
        policy,
        false,
    )
    .unwrap();
    (std::sync::Arc::new(provider), identities)
}

pub fn private_engine(mut options: Options, scalar: String, spot: bool) -> Engine<Hyperliquid> {
    let (provider, identities) = agent(&scalar);
    options.credentials = identities.primary.clone();
    let mut engine = safe_cex_engine::test_support::seeded_engine(if spot {
        Hyperliquid::new_spot(options)
    } else {
        Hyperliquid::new(options)
    });
    engine.set_signer(Some(provider)).unwrap();
    engine.set_identities(identities).unwrap();
    engine.initialize_address_nonce(999.0).unwrap();
    assert!(engine.address_nonce_setup_ready());
    engine
}

/// Native packet oracle only: uses real Prepare/Bind/Claim, without claiming host resource cleanup.
/// A prepared route can wait across calls while the engine holds its Bind for domain pacing.
pub fn prepared_now<V: safe_cex_engine::Venue>(engine: &mut Engine<V>) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    let now = safe_cex_engine::test_support::dispatch_now(engine);
    engine.prepare_dispatch(now);
    let mut queue = std::collections::VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut output = Vec::new();
    let mut visits = 0;
    while let Some(action) = queue.pop_front() {
        visits += 1;
        assert!(visits <= 65536, "native packet oracle did not settle");
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    if engine.http_preparation_pending(preparation) {
                        assert_eq!(preparation.attempt().request, request.id);
                        engine.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                }
                HttpAction::Bind {
                    permit, request, ..
                } => match engine.claim_http(now, permit) {
                    Ok(Claim::Released(_)) => output.push(Action::Http(request)),
                    Ok(Claim::WaitUntil(_)) => {
                        panic!("use the explicit held-Claim oracle for a host wait")
                    }
                    Err(_) => {}
                },
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => panic!("HL does not admit an atomic domain group"),
            },
            action => output.push(action),
        }
        queue.extend(engine.drain_actions());
    }
    output
}

/// Deliver only emitted timers through the supplied horizon; never invent a wake.
pub fn advance_packet_timers<V: safe_cex_engine::Venue>(
    engine: &mut Engine<V>,
    mut actions: Vec<Action>,
    until: f64,
) -> Vec<Action> {
    let mut pending = Vec::new();
    let mut scanned = 0;
    for _ in 0..256 {
        for action in &actions[scanned..] {
            if let Action::SetTimer { timer, at_ms } = action
                && *at_ms <= until
            {
                pending.push((*timer, *at_ms));
            }
        }
        scanned = actions.len();
        pending.sort_by(|a, b| b.1.total_cmp(&a.1));
        let Some((timer, at)) = pending.pop() else {
            return actions;
        };
        engine.handle(
            at.max(safe_cex_engine::test_support::dispatch_now(engine)),
            Event::TimerFired { timer },
        );
        actions.extend(prepared_now(engine));
    }
    panic!("native packet timers did not settle");
}
