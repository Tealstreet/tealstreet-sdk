#![cfg(feature = "bitget")]
use safe_cex_engine::{Action, Engine, Event, Response, Venue};
use safe_cex_venues::{Visit, build};
const NOW: f64 = 1_709_209_849_123.9;
struct Start;
impl Visit for Start {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Vec<Action> {
        engine.start(NOW);
        let mut all = vec![];
        for step in 0..8 {
            let output: Vec<_> = engine.drain_actions().collect();
            if output.is_empty() {
                break;
            }
            for action in output {
                if let Action::Http(http) = &action {
                    let body = if http.url.contains("/contracts") {
                        let (base, quote, symbol) = if http.url.contains("coin-futures") {
                            ("BTC", "USD", "BTCUSD")
                        } else if http.url.contains("usdc-futures") {
                            ("BTC", "USDC", "BTCPERP")
                        } else {
                            ("BTC", "USDT", "BTCUSDT")
                        };
                        Some(format!(
                            r#"{{"code":"00000","data":[{{"symbol":"{symbol}","baseCoin":"{base}","quoteCoin":"{quote}","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"}}]}}"#
                        ))
                    } else if http.url.contains("/tickers") {
                        Some(r#"{"code":"00000","data":[]}"#.into())
                    } else if http.url.contains("/account/settings") {
                        Some(r#"{"code":"40084","msg":"Classic Account mode","data":null}"#.into())
                    } else {
                        None
                    };
                    if let Some(body) = body {
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
                }
                if let Action::WsConnect { socket, url, .. } = &action
                    && url.ends_with("/private")
                {
                    engine.handle(NOW + f64::from(step), Event::WsOpened { socket: *socket });
                }
                all.push(action);
            }
        }
        all
    }
}
#[test]
fn standalone_bitget_feature_installs_provider_for_all_aliases_and_slots() {
    const {
        assert!(cfg!(feature = "dispatch-signing"));
    }
    for venue in ["bitget", "bitgetusdc", "bitgetinverse"] {
        for own_pass in [false, true] {
            let mut setup = serde_json::json!({"key":"primary-key","secret":"primary-material","passphrase":"primary-pass","readCredentials":{"key":"read-key","secret":"read-material"}});
            if own_pass {
                setup["readCredentials"]["passphrase"] = "read-pass".into();
            }
            let output = build(
                venue,
                serde_json::from_value(setup).unwrap(),
                Some(safe_cex_engine::test_support::seeded_entropy(7)),
                Start,
            )
            .unwrap();
            let signed: Vec<_> = output
                .iter()
                .filter_map(|a| match a {
                    Action::Http(h) if h.headers.iter().any(|(k, _)| *k == "ACCESS-SIGN") => {
                        Some(h)
                    }
                    _ => None,
                })
                .collect();
            assert!(!signed.is_empty(), "{venue}");
            for http in signed {
                let header = |name| &http.headers.iter().find(|(k, _)| *k == name).unwrap().1;
                assert_eq!(header("ACCESS-KEY"), "read-key");
                assert_eq!(
                    header("ACCESS-PASSPHRASE"),
                    if own_pass {
                        "read-pass"
                    } else {
                        "primary-pass"
                    }
                );
                let path = &http.url[http.url.find("/api/").unwrap()..];
                let payload = format!("{}GET{path}", header("ACCESS-TIMESTAMP"));
                assert_eq!(
                    *header("ACCESS-SIGN"),
                    safe_cex_crypto::hmac_sha256(
                        b"read-material",
                        payload.as_bytes(),
                        safe_cex_crypto::Encoding::Base64
                    )
                );
                assert_eq!(
                    http.headers.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                    [
                        "ACCESS-KEY",
                        "ACCESS-PASSPHRASE",
                        "ACCESS-SIGN",
                        "ACCESS-TIMESTAMP",
                        "X-CHANNEL-API-CODE",
                        "Content-Type"
                    ]
                );
            }
            let socket = output
                .iter()
                .find_map(|a| match a {
                    Action::WsConnect { socket, url, .. } if url.ends_with("/private") => {
                        Some(*socket)
                    }
                    _ => None,
                })
                .unwrap();
            let login: serde_json::Value = serde_json::from_str(
                output
                    .iter()
                    .find_map(|a| match a {
                        Action::WsSend { socket: s, text }
                            if *s == socket && text.contains("login") =>
                        {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .unwrap(),
            )
            .unwrap();
            let login = &login["args"][0];
            assert_eq!(login["apiKey"], "primary-key");
            assert_eq!(login["passphrase"], "primary-pass");
            let payload = format!("{}GET/user/verify", login["timestamp"].as_str().unwrap());
            assert_eq!(
                login["sign"],
                safe_cex_crypto::hmac_sha256(
                    b"primary-material",
                    payload.as_bytes(),
                    safe_cex_crypto::Encoding::Base64
                )
            );
        }
    }
}
#[test]
fn required_passphrases_are_checked_before_startup() {
    for venue in ["bitget", "bitgetusdc", "bitgetinverse"] {
        for input in [
            r#"{"key":"key","secret":"material"}"#,
            r#"{"key":"key","secret":"material","passphrase":""}"#,
            r#"{"key":"key","secret":"material","passphrase":"primary","readCredentials":{"key":"read","secret":"material","passphrase":""}}"#,
        ] {
            let error = build(
                venue,
                serde_json::from_str(input).unwrap(),
                Some(safe_cex_engine::test_support::seeded_entropy(7)),
                Start,
            )
            .unwrap_err();
            assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        }
        build(
            venue,
            serde_json::from_str(r#"{"publicDataOnly":true,"key":"key","secret":"material"}"#)
                .unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(7)),
            Start,
        )
        .unwrap();
    }
}
