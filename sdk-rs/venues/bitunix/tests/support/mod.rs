#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_bitunix::{BitunixFutures, Options};
pub type TestEngine = Engine<BitunixFutures>;
pub const PAIRS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}]}"#;
pub const TICKERS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","last":"96000","open":"95000","markPrice":"96001","baseVol":"1000","quoteVol":"96000000"}]}"#;
pub const ACCOUNT: &str = r#"{"code":0,"data":[{"marginCoin":"BTC","available":"1"},{"marginCoin":"USDT","available":"900","margin":"100","frozen":"3","positionMode":"HEDGE","crossUnrealizedPNL":"10","isolationUnrealizedPNL":"2"}]}"#;
pub const POSITIONS: &str = r#"{"code":0,"data":{"positionList":[{"symbol":"BTCUSDT","positionId":"position-1","qty":"0.01","maxQty":"0.015","side":"SELL","avgOpenPrice":"95000","entryPrice":"94000","entryValue":"950","leverage":20,"marginMode":"ISOLATION","positionMode":"HEDGE","unrealizedPNL":"10","realizedPNL":"-3.5","fee":"2","funding":"-1.5","liqPrice":"110000"}]}}"#;
pub const ORDERS: &str = r#"{"code":0,"data":{"orderList":[{"symbol":"BTCUSDT","orderId":"one","side":"SELL","orderType":"LIMIT","qty":"3","tradeQty":"1","price":"100","clientId":"web","effect":"GTC","tpPrice":"80","slPrice":"110"}]}}"#;
pub const TPSL: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","id":"protect","positionId":"position-1","tpPrice":"90000","slPrice":"100000","tpQty":"0.01","slQty":"0.01","tpStopType":"LAST_PRICE","slStopType":"MARK_PRICE","tpOrderType":null,"slOrderType":"MARKET","tpOrderPrice":null,"slOrderPrice":null}]}"#;
pub fn reply(e: &mut TestEngine, id: safe_cex_engine::RequestId, body: &str) {
    e.handle(
        0.0,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
pub fn event(e: &mut TestEngine, s: SocketId, text: &str) {
    e.handle(100.0, Event::WsMessage { socket: s, text });
}
pub fn requests(e: &mut TestEngine) -> Vec<HttpRequest> {
    e.drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn startup() -> (TestEngine, SocketId) {
    let e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    }));
    startup_with_engine(e)
}
/// Settle the initial mode-change reconciliation before the fixture's time zero.
/// Tests exercise post-startup events from 100ms onward without a hidden pending
/// debounce that a notification would previously postpone.
pub fn startup_with_engine(mut e: TestEngine) -> (TestEngine, SocketId) {
    let mut now = -1000.0;
    e.start(now);
    let mut private = None;
    let mut repairs = Vec::new();
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        if actions.is_empty() {
            if let Some((timer, at)) = repairs.pop() {
                now = at;
                e.handle(at, Event::TimerFired { timer });
                continue;
            }
            break;
        }
        for a in actions {
            match a {
                Action::WsConnect { socket, url, .. } => {
                    if url.contains("/private/") {
                        private = Some(socket);
                    }
                    e.handle(now, Event::WsOpened { socket });
                }
                Action::Http(r) => {
                    let body = if r.url.ends_with("/trading_pairs") {
                        PAIRS
                    } else if r.url.ends_with("/tickers") {
                        TICKERS
                    } else if r.url.contains("get_pending_positions") {
                        POSITIONS
                    } else if r.url.contains("/tpsl/") {
                        TPSL
                    } else if r.url.contains("get_pending_orders") {
                        ORDERS
                    } else {
                        ACCOUNT
                    };
                    e.handle(
                        now,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::SetTimer { timer, at_ms } if at_ms == 0.0 => repairs.push((timer, at_ms)),
                Action::ClearTimer { timer } => repairs.retain(|(pending, _)| *pending != timer),
                _ => {}
            }
        }
    }
    assert!(
        e.store().loaded().orders,
        "startup must settle both order endpoints"
    );
    (e, private.unwrap())
}
