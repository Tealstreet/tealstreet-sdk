#![cfg(all(feature = "binance", feature = "dispatch-signing"))]

use safe_cex_engine::signing::{
    Operation, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Engine, Event, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build_with_signer, local_signer_from_json};

const CATALOG: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}]}"#;
const COIN_CATALOG: &str = r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"}]}]}"#;

struct ReadStartup(&'static str, bool);
impl Visit for ReadStartup {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(1_000.0);
        let mut actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1_000.);
        let catalog = actions
            .iter()
            .find_map(|action| match action {
                Action::Http(request) if request.url.contains("exchangeInfo") => Some(request.id),
                _ => None,
            })
            .expect("native startup requests its catalogue");
        engine.handle(
            1_000.0,
            Event::HttpResponse {
                request: catalog,
                response: Response {
                    status: 200,
                    headers: "",
                    body: self.0,
                },
            },
        );
        actions.extend(safe_cex_engine::test_support::prepared_actions(
            &mut engine,
            1_000.,
        ));
        if self.1 {
            let symbol = if self.0 == COIN_CATALOG {
                "BTCUSD_PERP"
            } else {
                "BTCUSDT"
            };
            let order = serde_json::from_value(serde_json::json!({
                "symbol": symbol, "type": "limit", "side": "buy", "amount": 1, "price": 100
            }))
            .unwrap();
            engine.handle(
                1_001.,
                Event::Command(safe_cex_engine::Command::PlaceOrders {
                    command: 71,
                    orders: vec![order],
                }),
            );
            actions.extend(safe_cex_engine::test_support::prepared_actions(
                &mut engine,
                1_001.,
            ));
        }
        actions
    }
}

struct CallbackHeaders(Provider);
fn callback_headers(mut result: SignResult, signed: bool) -> SignResult {
    if signed {
        result.headers = vec![
            ("x-mbx-apikey".into(), "callback-key".into()),
            ("X-Proof".into(), "same-attempt".into()),
        ];
    }
    result
}
impl Signer for CallbackHeaders {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        let signed = work
            .operations
            .iter()
            .any(|op| matches!(op, Operation::Sign { .. }));
        self.0
            .sign_ready(work)
            .map(|result| result.map(|value| callback_headers(value, signed)))
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        let signed = work
            .operations
            .iter()
            .any(|op| matches!(op, Operation::Sign { .. }));
        let future = self.0.sign(work);
        Box::pin(async move { future.await.map(|value| callback_headers(value, signed)) })
    }
}

#[test]
fn every_binance_alias_admits_owned_payload_provider_and_keeps_callback_precedence() {
    for name in ["binance", "binancepm", "binanceinverse", "binancespot"] {
        let (local, identities) = local_signer_from_json(
            r#"{"primary":{"key":"owned-key","secret":"provider-fixture"}}"#.into(),
        )
        .unwrap();
        let mut options: EngineOptions = serde_json::from_str(r#"{"payloadSigner":true,"remoteSigning":true,"publicMode":{"requested":false,"payloadSigner":true}}"#).unwrap();
        options.signing = Some(identities);
        assert!(options.key.is_none() && options.secret.is_none());
        let actions = build_with_signer(
            name,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(std::sync::Arc::new(CallbackHeaders(local))),
            ReadStartup(
                if name == "binanceinverse" {
                    COIN_CATALOG
                } else {
                    CATALOG
                },
                false,
            ),
        )
        .unwrap();
        assert!(!actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("listenKey"))
        ));
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::WsConnect {
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            }
        )));
        let signed: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(request) if request.url.contains("signature=") => Some(request),
                _ => None,
            })
            .collect();
        assert!(
            !signed.is_empty(),
            "{name}: signed account reads must survive"
        );
        for request in signed {
            let headers: Vec<_> = request
                .headers
                .iter()
                .map(|(name, value)| (*name, value))
                .chain(
                    request
                        .signer_headers
                        .iter()
                        .map(|(name, value)| (name.as_str(), value)),
                )
                .collect();
            let keys: Vec<_> = headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("X-MBX-APIKEY"))
                .collect();
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0].1, "callback-key");
            assert!(
                headers
                    .iter()
                    .any(|(name, value)| *name == "X-Proof" && *value == "same-attempt")
            );
        }
    }
}

#[test]
fn managed_binance_writes_keep_the_same_attempt_callback_header_precedence() {
    for name in ["binance", "binancepm", "binanceinverse", "binancespot"] {
        let (local, identities) = local_signer_from_json(
            r#"{"primary":{"key":"owned-key","secret":"provider-fixture"}}"#.into(),
        )
        .unwrap();
        let mut options: EngineOptions = serde_json::from_str(r#"{"payloadSigner":true,"remoteSigning":true,"publicMode":{"requested":false,"payloadSigner":true}}"#).unwrap();
        options.signing = Some(identities);
        let actions = build_with_signer(
            name,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(std::sync::Arc::new(CallbackHeaders(local))),
            ReadStartup(
                if name == "binanceinverse" {
                    COIN_CATALOG
                } else {
                    CATALOG
                },
                true,
            ),
        )
        .unwrap();
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(request)
                    if request.method == safe_cex_engine::Method::Post
                        && request.url.contains("/order?") =>
                {
                    Some(request)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            requests.len(),
            1,
            "{name}: managed write must actually release"
        );
        let headers: Vec<_> = requests[0]
            .headers
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .chain(
                requests[0]
                    .signer_headers
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            )
            .collect();
        assert_eq!(
            headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("X-MBX-APIKEY"))
                .copied()
                .collect::<Vec<_>>(),
            [("x-mbx-apikey", "callback-key")]
        );
        assert!(headers.contains(&("X-Proof", "same-attempt")));
        assert!(requests[0].url.contains("signature="));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 71, .. })
        )));
    }
}

#[test]
fn payload_admission_requires_both_an_injected_provider_and_valid_owned_primary_refs() {
    for name in ["binance", "binancepm", "binanceinverse", "binancespot"] {
        for valid_refs in [false, true] {
            for injected in [false, true] {
                if valid_refs && injected {
                    continue;
                } // Positive path above traverses real startup.
                let (provider, identities) = local_signer_from_json(
                    r#"{"primary":{"key":"owned-key","secret":"provider-fixture"}}"#.into(),
                )
                .unwrap();
                let mut options: EngineOptions = serde_json::from_str(r#"{"payloadSigner":true,"remoteSigning":true,"publicMode":{"requested":false,"payloadSigner":true}}"#).unwrap();
                options.signing = Some(identities);
                if !valid_refs {
                    options
                        .signing
                        .as_mut()
                        .unwrap()
                        .primary
                        .as_mut()
                        .unwrap()
                        .signing
                        .0
                        .clear();
                }
                let error = build_with_signer(
                    name,
                    options,
                    None,
                    injected.then_some(provider),
                    ReadStartup(CATALOG, false),
                )
                .unwrap_err();
                assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
                assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
                assert!(
                    error
                        .message
                        .contains("configured injected signing provider")
                );
            }
        }
    }
}
