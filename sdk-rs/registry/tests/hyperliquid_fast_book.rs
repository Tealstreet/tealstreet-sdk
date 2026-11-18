#![cfg(feature = "hyperliquid")]
use safe_cex_engine::test_support::{prepared_now, seeded_entropy};
use safe_cex_engine::{Action, Command, Engine, Event, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build};

struct Topics;
impl Visit for Topics {
    type Output = Vec<String>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(1000.);
        let mut socket = None;
        for _ in 0..10 {
            for a in prepared_now(&mut engine) {
                match a {
                    Action::Http(r) => {
                        let kind =
                            serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap())
                                .unwrap()["type"]
                                .as_str()
                                .unwrap()
                                .to_owned();
                        let body = match kind.as_str() {
                            "metaAndAssetCtxs" => {
                                r#"[{"universe":[{"name":"BTC","szDecimals":5}]},[{"midPx":"100"}]]"#
                            }
                            "spotMetaAndAssetCtxs" => {
                                r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],"universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},[{"coin":"PURR/USDC","midPx":"0.2","markPx":"0.2"}]]"#
                            }
                            "perpDexs" => "[null]",
                            other => panic!("unexpected read {other}"),
                        };
                        engine.handle(
                            1000.,
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
                    Action::WsConnect { socket: s, .. } => socket = Some(s),
                    _ => {}
                }
            }
            if socket.is_some() {
                break;
            }
        }
        let socket = socket.expect("registered public socket");
        engine.handle(1000., Event::WsOpened { socket });
        prepared_now(&mut engine).into_iter().for_each(drop);
        let symbol = engine.store().markets()[0].symbol.clone();
        engine.handle(
            2000.,
            Event::Command(Command::SubscribeOrderBook { symbol }),
        );
        prepared_now(&mut engine)
            .into_iter()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }
}
#[test]
fn registry_options_default_off_and_explicit_opt_in_reaches_native_socket() {
    for venue in ["hyperliquid", "hyperliquidspot"] {
        for fast in [None, Some(false), Some(true)] {
            let mut raw = serde_json::json!({"publicDataOnly":true});
            if let Some(fast) = fast {
                raw["hyperliquidFastBook"] = fast.into();
            }
            let options: EngineOptions = serde_json::from_value(raw).unwrap();
            let topics = build(venue, options, Some(seeded_entropy(17)), Topics).unwrap();
            assert_eq!(
                topics.iter().any(|t| t.contains("\"fast\":true")),
                venue == "hyperliquid" && fast == Some(true),
                "{venue} {fast:?}: {topics:?}"
            );
            // The watched coin's trades stream rides beside its depth topics, exactly once.
            let coin = if venue == "hyperliquidspot" {
                "PURR/USDC"
            } else {
                "BTC"
            };
            let trades = format!(
                r#"{{"method":"subscribe","subscription":{{"type":"trades","coin":"{coin}"}}}}"#
            );
            assert_eq!(
                topics
                    .iter()
                    .filter(|t| t.contains("\"type\":\"trades\""))
                    .count(),
                1,
                "{venue} {fast:?}: {topics:?}"
            );
            assert!(topics.contains(&trades), "{venue} {fast:?}: {topics:?}");
            assert_eq!(
                topics.len() - 1,
                if venue == "hyperliquidspot" {
                    5
                } else if fast == Some(true) {
                    3
                } else {
                    2
                },
                "{venue} {fast:?}: depth topics besides trades"
            );
        }
    }
}
