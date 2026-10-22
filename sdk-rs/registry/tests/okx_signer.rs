#![cfg(feature = "okx")]

use safe_cex_engine::{Action, Engine, Event, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build};

const STAMP: f64 = 1_709_209_849_123.9;
const MARKET: &str = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","state":"live","lotSz":"0.01","tickSz":"0.1","minSz":"0.01","maxLmtSz":"1000","maxMktSz":"1000","lever":"100"}"#;

struct Discovery;
impl Visit for Discovery {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(STAMP);
        let mut all = Vec::new();
        for step in 0..8 {
            let out: Vec<_> = engine.drain_actions().collect();
            if out.is_empty() {
                break;
            }
            for action in out {
                if let Action::Http(http) = &action {
                    let body = if http.url.contains("instruments") {
                        let row = if http.url.contains("instType=FUTURES") {
                            MARKET.replace("BTC-USDT-SWAP", "BTC-USDT-261225").replace(
                                "\"instType\":\"SWAP\"",
                                "\"instType\":\"FUTURES\",\"ruleType\":\"xperp\"",
                            )
                        } else {
                            MARKET.into()
                        };
                        Some(format!(r#"{{"code":"0","data":[{row}]}}"#))
                    } else if http.url.contains("/account/config") {
                        Some(r#"{"code":"0","data":[{"posMode":"net_mode","acctLv":"2"}]}"#.into())
                    } else if http.url.contains("tickers") {
                        Some(r#"{"code":"0","data":[]}"#.into())
                    } else {
                        None
                    };
                    if let Some(body) = body {
                        engine.handle(
                            STAMP + f64::from(step),
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
                }
                if let Action::WsConnect { socket, url, .. } = &action
                    && (url.ends_with("/private") || url.ends_with("/business"))
                {
                    engine.handle(STAMP + f64::from(step), Event::WsOpened { socket: *socket });
                }
                all.push(action);
            }
        }
        all
    }
}

#[test]
fn okx_only_feature_installs_provider_and_all_aliases_sign_secondary_gets() {
    const {
        assert!(cfg!(feature = "dispatch-signing"));
    }
    for venue in ["okx", "okxeu", "okxdemo", "okxeudemo"] {
        for own_passphrase in [false, true] {
            let mut input = serde_json::json!({"key":"primary-key","secret":"primary-material","passphrase":"primary-pass",
                "readCredentials":{"key":"read-key","secret":"read-material"}});
            if own_passphrase {
                input["readCredentials"]["passphrase"] = "read-pass".into();
            }
            let actions = build(
                venue,
                serde_json::from_value(input).unwrap(),
                Some(safe_cex_engine::test_support::seeded_entropy(7)),
                Discovery,
            )
            .unwrap();
            let signed: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Http(http)
                        if http
                            .headers
                            .iter()
                            .any(|(name, _)| *name == "OK-ACCESS-SIGN") =>
                    {
                        Some(http)
                    }
                    _ => None,
                })
                .collect();
            assert!(!signed.is_empty(), "{venue}");
            for suffix in ["/private", "/business"] {
                let socket = actions
                    .iter()
                    .find_map(|action| match action {
                        Action::WsConnect { socket, url, .. } if url.ends_with(suffix) => {
                            Some(*socket)
                        }
                        _ => None,
                    })
                    .expect("both native login sockets connect");
                let text = actions
                    .iter()
                    .find_map(|action| match action {
                        Action::WsSend {
                            socket: current,
                            text,
                        } if *current == socket && text.contains("login") => Some(text),
                        _ => None,
                    })
                    .expect("both native login builders release through provider");
                let value: serde_json::Value = serde_json::from_str(text).unwrap();
                let login = &value["args"][0];
                assert_eq!(login["apiKey"], "primary-key");
                assert_eq!(login["passphrase"], "primary-pass");
                let stamp = login["timestamp"].as_str().unwrap();
                assert_eq!(
                    login["sign"],
                    safe_cex_crypto::hmac_sha256(
                        b"primary-material",
                        format!("{stamp}GET/users/self/verify").as_bytes(),
                        safe_cex_crypto::Encoding::Base64
                    )
                );
            }
            for http in signed {
                let header = |name| &http.headers.iter().find(|(key, _)| *key == name).unwrap().1;
                assert_eq!(header("OK-ACCESS-KEY"), "read-key", "{venue}");
                assert_eq!(
                    header("OK-ACCESS-PASSPHRASE"),
                    if own_passphrase {
                        "read-pass"
                    } else {
                        "primary-pass"
                    }
                );
                let path = &http.url[http.url.find("/api/").unwrap()..];
                let text = format!("{}GET{path}", header("OK-ACCESS-TIMESTAMP"));
                assert_eq!(
                    *header("OK-ACCESS-SIGN"),
                    safe_cex_crypto::hmac_sha256(
                        b"read-material",
                        text.as_bytes(),
                        safe_cex_crypto::Encoding::Base64
                    )
                );
                assert_eq!(
                    http.headers
                        .iter()
                        .any(|(name, value)| *name == "x-simulated-trading" && value == "1"),
                    venue.ends_with("demo")
                );
                assert!(http.url.starts_with(if venue.starts_with("okxeu") {
                    "https://eea.okx.com/"
                } else if venue.ends_with("demo") {
                    "https://openapi.okx.com/"
                } else {
                    "https://www.okx.com/"
                }));
            }
        }
    }
}

#[test]
fn missing_or_explicit_empty_passphrases_are_refused_before_engine_startup() {
    for venue in ["okx", "okxeu", "okxdemo", "okxeudemo"] {
        for input in [
            r#"{"key":"primary-key","secret":"primary-material"}"#,
            r#"{"key":"primary-key","secret":"primary-material","passphrase":""}"#,
            r#"{"key":"primary-key","secret":"primary-material","passphrase":"primary-pass","readCredentials":{"key":"read-key","secret":"read-material","passphrase":""}}"#,
        ] {
            let error = build(
                venue,
                serde_json::from_str::<EngineOptions>(input).unwrap(),
                Some(safe_cex_engine::test_support::seeded_entropy(7)),
                Discovery,
            )
            .unwrap_err();
            assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
            assert!(!error.message.contains("primary-material"));
        }
        build(
            venue,
            serde_json::from_str(
                r#"{"publicDataOnly":true,"key":"primary-key","secret":"primary-material"}"#,
            )
            .unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(7)),
            Discovery,
        )
        .unwrap();
    }
}
