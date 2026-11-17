#![cfg(feature = "bingx")]
use safe_cex_engine::{Action, Engine, ErrorKind, Event, HttpRequest, Outcome, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer, supported};
const NOW: f64 = 1_702_720_824_000.;
struct Startup;
impl Visit for Startup {
    type Output = Vec<HttpRequest>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(NOW);
        let mut requests = Vec::new();
        for _ in 0..8 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
            let mut count = 0;
            for action in actions {
                if let Action::Http(request) = action {
                    count += 1;
                    let path = request.url.split('?').next().unwrap();
                    let body = if path.ends_with("/common/symbols") {
                        include_str!("../../venues/bingx/tests/fixtures/spot/markets.json")
                    } else if path.ends_with("/quote/ticker") {
                        include_str!("../../venues/bingx/tests/fixtures/spot/tickers.json")
                    } else if path.ends_with("/account/balance") {
                        include_str!("../../venues/bingx/tests/fixtures/spot/balances.json")
                    } else if path.ends_with("/trade/openOrders") {
                        r#"{"code":0,"data":{"orders":[]}}"#
                    } else if path.ends_with("/userDataStream") {
                        r#"{"listenKey":"spot-fixture-session"}"#
                    } else {
                        panic!("unexpected startup route {path}")
                    };
                    engine.handle(
                        NOW,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                    requests.push(request);
                }
            }
            if count == 0 {
                break;
            }
        }
        requests
    }
}
#[test]
fn normal_public_and_private_registry_factories_construct_only_spot_routes() {
    assert!(supported("bingxspot"));
    assert!(supported("bingx"));
    for private in [false, true] {
        let json = if private {
            r#"{"key":"spot-fixture-key","secret":"spot-fixture-secret"}"#
        } else {
            r#"{"publicDataOnly":true}"#
        };
        let requests = build(
            "bingxspot",
            serde_json::from_str(json).unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Startup,
        )
        .unwrap();
        assert!(!requests.is_empty());
        for request in &requests {
            assert!(request.url.starts_with("https://open-api.bingx.com/"));
            assert!(!request.url.contains("/swap/"));
        }
        assert!(
            requests
                .iter()
                .any(|r| r.url.contains("/spot/v1/common/symbols"))
        );
        assert_eq!(
            requests.iter().any(|r| r.url.contains("/account/balance")),
            private
        );
        assert_eq!(
            requests.iter().any(|r| r.url.contains("/userDataStream")),
            private
        );
        for request in requests
            .iter()
            .filter(|r| r.url.contains("/account/balance") || r.url.contains("/userDataStream"))
        {
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(k, v)| *k == "X-BX-APIKEY" && v == "spot-fixture-key")
            );
            // Listen-key requests present the API key; account reads also require HMAC.
            assert_eq!(
                request.url.contains("signature="),
                request.url.contains("/account/balance")
            );
        }
    }
}
struct NeverConstruct;
impl Visit for NeverConstruct {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {
        panic!("refused input reached a live factory");
    }
}
#[test]
fn testnet_refuses_before_either_native_or_injected_factory_runs() {
    for injected in [false, true] {
        let options: EngineOptions =
            serde_json::from_str(r#"{"publicDataOnly":true,"testnet":true}"#).unwrap();
        let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
        let result = if injected {
            build_with_signer("bingxspot", options, entropy, None, NeverConstruct)
        } else {
            build("bingxspot", options, entropy, NeverConstruct)
        };
        let error = result.unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(error.message.contains("testnet"));
    }
}
