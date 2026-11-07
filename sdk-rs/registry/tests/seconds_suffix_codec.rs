#![cfg(all(feature = "okx", feature = "bitget"))]
//! Native WS login work must match the remote wire codec declared per alias.
use safe_cex_engine::signing::{
    Operation, PayloadPart, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Engine, Event, Response, Venue};
use safe_cex_venues::{Visit, build};
use std::sync::{Arc, Mutex};

const NOW: f64 = 1_709_209_849_123.9;
const OKX_MARKET: &str = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","state":"live","lotSz":"0.01","tickSz":"0.1","minSz":"0.01","maxLmtSz":"1000","maxMktSz":"1000","lever":"100"}"#;

struct Recorder {
    inner: Provider,
    seen: Arc<Mutex<Vec<SignRequest>>>,
}
impl Signer for Recorder {
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.seen.lock().unwrap().push(work.clone());
        self.inner.sign(work)
    }
    fn immediate(&self) -> bool {
        self.inner.immediate()
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.seen.lock().unwrap().push(work.clone());
        self.inner.sign_ready(work)
    }
}

fn body(url: &str) -> Option<String> {
    let okx = url.contains("okx.com");
    if okx && url.contains("instruments") {
        let row = if url.contains("instType=FUTURES") {
            OKX_MARKET
                .replace("BTC-USDT-SWAP", "BTC-USDT-261225")
                .replace(
                    "\"instType\":\"SWAP\"",
                    "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\"",
                )
        } else {
            OKX_MARKET.into()
        };
        Some(format!(r#"{{"code":"0","data":[{row}]}}"#))
    } else if okx && url.contains("/account/config") {
        Some(r#"{"code":"0","data":[{"posMode":"net_mode","acctLv":"2"}]}"#.into())
    } else if okx && url.contains("tickers") {
        Some(r#"{"code":"0","data":[]}"#.into())
    } else if url.contains("/contracts") {
        let (quote, symbol) = if url.contains("coin-futures") {
            ("USD", "BTCUSD")
        } else if url.contains("usdc-futures") {
            ("USDC", "BTCPERP")
        } else {
            ("USDT", "BTCUSDT")
        };
        Some(format!(
            r#"{{"code":"00000","data":[{{"symbol":"{symbol}","baseCoin":"BTC","quoteCoin":"{quote}","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"}}]}}"#
        ))
    } else if url.contains("/tickers") {
        Some(r#"{"code":"00000","data":[]}"#.into())
    } else if url.contains("/account/settings") {
        Some(r#"{"code":"40084","msg":"Classic Account mode","data":null}"#.into())
    } else {
        None
    }
}

struct Logins;
impl Visit for Logins {
    type Output = Vec<SignRequest>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let inner = engine.signer().expect("dispatch provider installed");
        engine
            .set_signer(Some(Arc::new(Recorder {
                inner,
                seen: seen.clone(),
            })))
            .unwrap();
        engine.start(NOW);
        // Managed reads release at the time of the event that issued them.
        let mut now = NOW;
        for step in 0..8 {
            let out = prepared_actions(&mut engine, now);
            if out.is_empty() {
                break;
            }
            now = NOW + f64::from(step);
            for action in out {
                if let Action::Http(http) = &action
                    && let Some(body) = body(&http.url)
                {
                    engine.handle(
                        NOW + f64::from(step),
                        Event::HttpResponse {
                            request: http.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: &body,
                            },
                        },
                    );
                }
                if let Action::WsConnect { socket, url, .. } = &action
                    && (url.ends_with("/private") || url.ends_with("/business"))
                {
                    engine.handle(NOW + f64::from(step), Event::WsOpened { socket: *socket });
                }
            }
        }
        let seen = seen.lock().unwrap();
        seen.iter()
            .filter(|work| work.context.method == "WS")
            .cloned()
            .collect()
    }
}

fn declared(alias: &str) -> (String, String) {
    let rules: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    let wire = &rules["venues"][alias]["remoteSigning"]["wire"];
    assert_eq!(wire["extraData"], "secondsTimestamp", "{alias}");
    let suffix = &wire["secondsSuffix"];
    (
        suffix["method"].as_str().unwrap().into(),
        suffix["path"].as_str().unwrap().into(),
    )
}

#[test]
fn native_login_work_matches_each_declared_seconds_suffix_for_both_families() {
    for (alias, channels) in [
        ("okx", 2),
        ("okxeu", 2),
        ("okxdemo", 2),
        ("okxeudemo", 2),
        ("bitget", 1),
        ("bitgetusdc", 1),
        ("bitgetinverse", 1),
    ] {
        let (method, path) = declared(alias);
        let logins = build(
            alias,
            serde_json::from_str(r#"{"key":"k","secret":"s","passphrase":"p"}"#).unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(7)),
            Logins,
        )
        .unwrap();
        assert_eq!(logins.len(), channels, "{alias}");
        let seconds = ((NOW / 1000.).floor() as u64).to_string();
        for work in logins {
            assert_eq!(work.receipt.stamp_ms, Some((NOW / 1000.).floor() * 1000.));
            let Operation::Sign { payload, .. } = &work.operations[1] else {
                panic!("{alias}: login signs its second operation");
            };
            let parts: Vec<_> = payload
                .iter()
                .map(|part| match part {
                    PayloadPart::Bytes { bytes } => String::from_utf8(bytes.clone()).unwrap(),
                    _ => panic!("{alias}: login parts are bytes only"),
                })
                .collect();
            assert_eq!(
                parts,
                [seconds.clone(), method.clone(), path.clone()],
                "{alias}"
            );
        }
    }
}
