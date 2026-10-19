#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
pub const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100","prevDayPx":"90"},{"midPx":"200","markPx":"200","oraclePx":"200"}]]"#;
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
    let mut e = Engine::new(Hyperliquid::new(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        builder,
        ..Options::default()
    }));
    e.start(1000.0);
    let mut socket = None;
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
                    let body=match kind(&r).as_str(){"metaAndAssetCtxs" if dex.is_some()=>r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50","markPx":"50","oraclePx":"50"}]]"#.into(),"metaAndAssetCtxs"=>META.to_owned(),"perpDexs"=>if hip{r#"[null,{"name":"xyz"}]"#.into()}else{"[null]".into()},"userAbstraction"=>format!("\"{mode}\""),"maxBuilderFee"=>approval.into(),"userFees"=>r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#.into(),"clearinghouseState" if dex.is_some()=>r#"{"assetPositions":[],"time":1000}"#.into(),"clearinghouseState"=>STATE.into(),"frontendOpenOrders"=>"[]".into(),"spotClearinghouseState"=>r#"{"balances":[{"coin":"USDC","total":"1100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"1050"]]}"#.into(),other=>panic!("unexpected read {other}")};
                    reply(&mut e, &r, 200, &body);
                }
                Action::WsConnect { socket: s, .. } => {
                    socket = Some(s);
                    e.handle(1000.0, Event::WsOpened { socket: s });
                }
                _ => {}
            }
        }
    }
    (e, socket.unwrap())
}
pub fn message(e: &mut Engine<Hyperliquid>, s: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket: s, text });
}
