#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_txflow::{Options, TxFlow};
pub const NOW: f64 = 1_700_000_000_000.0;
pub const WALLET: &str = "0x0000000000000000000000000000000000000001";
pub const META: &str = r#"{"success":true,"code":0,"data":{"universe":[{"index":1,"name":"BTC-USDC","baseCurrency":"BTC","quoteCurrency":"USDC","currencyType":"usdc","delisted":false,"haltTrading":false,"basePrecision":"0.001","priceTick":"0.1","maxLeverage":"50"}]}}"#;
pub const STATE: &str = r#"{"success":true,"code":0,"data":{"time":1700000000000,"assetPositions":[{"type":"oneWay","position":{"asset":"1","szi":"2","leverage":{"type":"Cross","value":10},"positionValue":"200","unrealizedPnl":"5","marginUsed":"20","entryPx":"97.5","liquidationPx":"10","updateTime":1700000000000}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"20","totalRawUsd":"995"}}}"#;
pub const EMPTY_STATE: &str = r#"{"success":true,"code":0,"data":{"time":1700000000000,"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0","totalRawUsd":"1000"}}}"#;
pub const ORDER: &str = r#"{"asset":1,"oid":42,"side":"B","orderType":"Limit","limitPx":"100","origSz":"2","sz":"1.5","timestamp":1700000000000,"isTrigger":false,"reduceOnly":false,"cloid":"caller-id","state":"Open","tif":"GTC","triggerPx":"","children":[]}"#;
pub fn kind(r: &HttpRequest) -> String {
    serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap()["type"]
        .as_str()
        .unwrap()
        .into()
}
pub fn reply(e: &mut Engine<TxFlow>, r: &HttpRequest, body: &str) {
    reply_at(e, r, NOW, 200, body)
}
pub fn reply_at(e: &mut Engine<TxFlow>, r: &HttpRequest, now: f64, status: u16, body: &str) {
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
pub fn requests(e: &mut Engine<TxFlow>) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn message(e: &mut Engine<TxFlow>, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}
pub fn bootstrap() -> (Engine<TxFlow>, SocketId, Vec<HttpRequest>, TimerId) {
    bootstrap_with_wallet(WALLET)
}
pub fn bootstrap_with_wallet(
    wallet: &str,
) -> (Engine<TxFlow>, SocketId, Vec<HttpRequest>, TimerId) {
    bootstrap_with_meta(wallet, META)
}
pub fn bootstrap_with_meta(
    wallet: &str,
    meta: &str,
) -> (Engine<TxFlow>, SocketId, Vec<HttpRequest>, TimerId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        application_id: Some(wallet.into()),
        secret: Some(format!("0x{:064x}", 1)),
        ..Options::default()
    }));
    e.start(NOW);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, meta);
    let mut actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let index = actions
        .iter()
        .position(|a| matches!(a, Action::Http(r) if kind(r) == "activeAssetCtx"))
        .unwrap();
    let Action::Http(r) = actions.remove(index) else {
        unreachable!()
    };
    reply(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"instrumentId":1,"lastPrice":"100","markPx":"100","oraclePx":"100"}}"#,
    );
    actions.extend(safe_cex_engine::test_support::prepared_actions(&mut e, NOW));
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 25.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 25.0, Event::TimerFired { timer });
    actions.extend(safe_cex_engine::test_support::prepared_actions(
        &mut e,
        NOW + 25.0,
    ));
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/ws") => Some(*socket),
            _ => None,
        })
        .unwrap();
    // Socket connect deadlines share this instant; the account refresh is armed last.
    let refresh = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 5025.0 => Some(*timer),
            _ => None,
        })
        .next_back()
        .unwrap();
    let reads = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    e.handle(NOW + 25.0, Event::WsOpened { socket });
    let mut reads: Vec<_> = reads;
    reads.extend(requests(&mut e));
    (e, socket, reads, refresh)
}
pub fn answer(e: &mut Engine<TxFlow>, reads: Vec<HttpRequest>) {
    for r in reads {
        let body = match kind(&r).as_str() {
            "clearinghouseState" => STATE.to_owned(),
            "frontendOpenOrders" => format!(r#"{{"success":true,"code":0,"data":[{ORDER}]}}"#),
            "activeAssetData" => format!(
                r#"{{"success":true,"code":0,"data":{{"coin":"1","user":"{WALLET}","leverage":{{"mode":"oneWay","type":"Cross","value":10}}}}}}"#
            ),
            "userFills" => r#"{"success":true,"code":0,"data":{"data":[],"hasMore":false}}"#.into(),
            k => panic!("unexpected {k}"),
        };
        reply(e, &r, &body);
    }
}
pub fn fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + size;
    }
    0
}
pub fn fill(tid: u64, price: f64, time: f64) -> String {
    format!(
        r#"{{"tid":{tid},"oid":42,"instrumentId":1,"px":"{price}","sz":"0.5","side":"B","time":{time}}}"#
    )
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
