#![cfg(feature = "okx")]

use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{Action, Command, Engine, Event, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build};

const STAMP: f64 = 1_709_209_849_123.9;
const MARKET: &str = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","state":"live","lotSz":"0.01","tickSz":"0.1","minSz":"0.01","maxLmtSz":"1000","maxMktSz":"1000","lever":"100"}"#;

struct Discovery;
impl Visit for Discovery {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        discover(&mut engine).0
    }
}

/// Loads the catalogue and answers the account config read; returns every
/// action and the time of the last event.
fn discover<V: Venue>(engine: &mut Engine<V>) -> (Vec<Action>, f64) {
    engine.start(STAMP);
    let mut all = Vec::new();
    let mut now = STAMP;
    for step in 0..8 {
        let out = prepared_actions(engine, now);
        if out.is_empty() {
            break;
        }
        now = STAMP + f64::from(step);
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
                        now,
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
                engine.handle(now, Event::WsOpened { socket: *socket });
            }
            all.push(action);
        }
    }
    (all, now)
}

/// Discovery, then one FetchLeverage; returns the actions it releases.
struct LeverageRead;
impl Visit for LeverageRead {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        let (_, now) = discover(&mut engine);
        engine.handle(
            now + 1.,
            Event::Command(Command::FetchLeverage {
                command: 7,
                symbol: "BTCUSDT".into(),
            }),
        );
        prepared_actions(&mut engine, now + 1.)
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
fn all_aliases_send_the_leverage_read_with_the_read_key_and_demo_header() {
    for venue in ["okx", "okxeu", "okxdemo", "okxeudemo"] {
        let input = serde_json::json!({"key":"primary-key","secret":"primary-material","passphrase":"primary-pass",
            "readCredentials":{"key":"read-key","secret":"read-material","passphrase":"read-pass"}});
        let actions = build(
            venue,
            serde_json::from_value(input).unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(7)),
            LeverageRead,
        )
        .unwrap();
        let reads: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(http) if http.url.contains("/api/v5/account/leverage-info?") => {
                    Some(http)
                }
                _ => None,
            })
            .collect();
        assert_eq!(reads.len(), 1, "{venue}");
        let http = reads[0];
        let header = |name| &http.headers.iter().find(|(key, _)| *key == name).unwrap().1;
        assert_eq!(header("OK-ACCESS-KEY"), "read-key", "{venue}");
        assert_eq!(header("OK-ACCESS-PASSPHRASE"), "read-pass", "{venue}");
        let path = &http.url[http.url.find("/api/").unwrap()..];
        let text = format!("{}GET{path}", header("OK-ACCESS-TIMESTAMP"));
        assert_eq!(
            *header("OK-ACCESS-SIGN"),
            safe_cex_crypto::hmac_sha256(
                b"read-material",
                text.as_bytes(),
                safe_cex_crypto::Encoding::Base64
            ),
            "{venue}"
        );
        assert_eq!(
            http.headers
                .iter()
                .filter(|(name, value)| *name == "x-simulated-trading" && value == "1")
                .count(),
            usize::from(venue.ends_with("demo")),
            "{venue}"
        );
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
