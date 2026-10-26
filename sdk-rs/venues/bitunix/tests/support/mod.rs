#![allow(dead_code)]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{Identities, Identity, KeyRef, Provider};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, ReadCredentials, Response, SocketId};
use safe_cex_venue_bitunix::{BitunixFutures, Options as NativeOptions};
pub type TestEngine = Engine<BitunixFutures>;
/// Fixture input as trusted setup receives it; the provider holds the material.
#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub read_credentials: Option<ReadCredentials>,
    pub key: Option<String>,
    pub secret: Option<String>,
}
impl Options {
    pub fn build(self) -> (BitunixFutures, Provider, Identities) {
        let primary = self
            .key
            .as_ref()
            .zip(self.secret.as_ref())
            .filter(|(k, s)| !k.is_empty() && !s.is_empty())
            .map(|(k, s)| LocalIdentity {
                key: Secret::new(k.clone()),
                secret: Secret::new(s.clone()),
                ..Default::default()
            });
        let secondary = self
            .read_credentials
            .as_ref()
            .filter(|r| r.validate().is_ok())
            .map(|r| LocalIdentity {
                key: Secret::new(r.key.clone()),
                secret: Secret::new(r.secret.clone()),
                ..Default::default()
            });
        let (provider, ids) = LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
        // An invalid read key reaches the venue as an invalid identity, as the
        // registry would refuse it, so the venue's startup refusal stays pinned.
        let read_credentials = if self
            .read_credentials
            .as_ref()
            .is_some_and(|r| r.validate().is_err())
        {
            Some(Identity::new(
                KeyRef(String::new()),
                Some(KeyRef(String::new())),
            ))
        } else {
            ids.secondary.clone()
        };
        let venue = BitunixFutures::new(NativeOptions {
            testnet: self.testnet,
            public_data_only: self.public_data_only,
            credentials: ids.primary.clone(),
            read_credentials,
        });
        (venue, std::sync::Arc::new(provider), ids)
    }
}
/// Install the venue's provider and identities on an engine built around it.
pub fn install<V: safe_cex_engine::Venue>(
    engine: &mut Engine<V>,
    provider: Provider,
    ids: Identities,
) {
    engine.set_signer(Some(provider)).unwrap();
    engine.set_identities(ids).unwrap();
}
pub fn engine(options: Options) -> TestEngine {
    let (venue, provider, ids) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    install(&mut engine, provider, ids);
    engine
}
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
/// Signed requests leave signing dispatch only once prepared; fixtures run at 100ms.
pub fn requests(e: &mut TestEngine) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, 100.0)
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
pub fn startup() -> (TestEngine, SocketId) {
    let e = engine(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    });
    startup_with_engine(e)
}
/// Settle the initial mode-change reconciliation before the fixture's time zero.
/// Tests exercise post-startup events from 100ms onward without a hidden pending
/// debounce that a notification would previously postpone.
pub fn startup_with_engine(e: TestEngine) -> (TestEngine, SocketId) {
    startup_serving(e, PAIRS, TICKERS)
}
/// `startup_with_engine` over a caller's pairs and tickers.
pub fn startup_serving(mut e: TestEngine, pairs: &str, tickers: &str) -> (TestEngine, SocketId) {
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
                        pairs
                    } else if r.url.ends_with("/tickers") {
                        tickers
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
