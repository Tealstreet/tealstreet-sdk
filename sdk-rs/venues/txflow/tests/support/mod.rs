#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_txflow::{Options, TxFlow};
pub const META: &str = r#"{"success":true,"code":0,"data":{"universe":[{"index":1,"name":"BTC-USDC","baseCurrency":"BTC","quoteCurrency":"USDC","currencyType":"usdc","basePrecision":"0.0001","priceTick":"0.1","maxLeverage":50,"haltTrading":false,"delisted":false,"useNotionalMargin":false}]}}"#;
pub const TICKER: &str = r#"{"success":true,"code":0,"data":{"instrumentId":1,"lastPrice":"65000","prevDayPx":"64000","markPx":"65001","oraclePx":"65000","openInterest":"2","coinVlm":"3","dayNtlVlm":"195000","nodeCtx":{"funding":"0.01","markPx":"65001","oraclePx":"65000","openInterest":"2","lastPrice":"65000","midPx":"65000.5","dayBaseVlm":"3","dayNtlVlm":"195000","prevDayPx":"64000","nextFundingTime":7200000}}}"#;
pub fn reply(e: &mut Engine<TxFlow>, r: &HttpRequest, status: u16, body: &str) {
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
pub fn ready() -> (Engine<TxFlow>, SocketId, SocketId) {
    let mut e = Engine::new(TxFlow::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
    let mut sockets = Vec::new();
    for _ in 0..12 {
        let actions = e.drain_actions().collect::<Vec<_>>();
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    let body = if r.body.as_deref().unwrap_or("").contains("perpMeta") {
                        META
                    } else {
                        TICKER
                    };
                    reply(&mut e, &r, 200, body);
                }
                Action::SetTimer { timer, at_ms } if at_ms < 2000.0 => {
                    e.handle(at_ms, Event::TimerFired { timer })
                }
                Action::WsConnect { socket, .. } => {
                    sockets.push(socket);
                    e.handle(2000.0, Event::WsOpened { socket });
                }
                _ => {}
            }
        }
    }
    assert_eq!(sockets.len(), 2);
    (e, sockets[0], sockets[1])
}
pub fn message(e: &mut Engine<TxFlow>, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}
