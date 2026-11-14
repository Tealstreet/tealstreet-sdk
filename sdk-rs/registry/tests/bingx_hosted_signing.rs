#![cfg(feature = "bingx")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef, SignError};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, Identities) {
    let mut provider = LocalSigner::new();
    let identity = Identity::new(
        KeyRef("ref:owned-public".into()),
        Some(KeyRef("ref:owned-primary".into())),
    );
    provider.insert(identity.credential.clone(), b"bingx-api-key".to_vec());
    provider.insert(
        identity.signing_ref().unwrap().clone(),
        b"bingx-test-secret".to_vec(),
    );
    let ids = Identities {
        primary: Some(identity),
        secondary: None,
    };
    (provider, ids)
}

// Node crypto: createHmac('sha256', 'bingx-test-secret').update(TO_SIGN).digest('hex'),
// the legacy BingX client's sorted repeat-format query.
const TO_SIGN: &str = "price=90000&quantity=0.012&side=BUY&symbol=BTC-USDT&timestamp=1700000000000";
const GOLDEN: &str = "8c8447ee861f85ae474b991b2067631c146496d94a02879011511a0fab1e81e4";

#[test]
fn bingx_http_and_ws_auth_captures_sign_the_hex_hmac_golden() {
    let (provider, ids) = fixture();
    let inputs = [
        Input::Http {
            payload: TO_SIGN.into(),
            stamp_ms: 1_700_000_000_000.,
            revision: "0".into(),
        },
        Input::WsAuth {
            to_sign: TO_SIGN.into(),
            output_format: OutputFormat::Hex,
        },
    ];
    for (index, input) in inputs.into_iter().enumerate() {
        let (work, kind) = hosted_signing::prepare("bingx", &ids, input).unwrap();
        work.validate().unwrap();
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(output.signature.as_deref(), Some(GOLDEN));
        // HTTP presents the key for the `X-BX-APIKEY` header; ws-auth only signs.
        assert_eq!(
            output.credential.as_deref(),
            (index == 0).then_some("bingx-api-key")
        );
        assert!(output.headers.is_none());
    }
    let (work, kind) = hosted_signing::prepare("bingx", &ids, Input::Credential {}).unwrap();
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(output.credential.as_deref(), Some("bingx-api-key"));
}

#[test]
fn bingx_refuses_the_bitunix_scheme_and_a_missing_primary() {
    let (_, ids) = fixture();
    let fused = Input::WsAuth {
        to_sign: TO_SIGN.into(),
        output_format: OutputFormat::BitunixDoubleSha256,
    };
    assert!(matches!(
        hosted_signing::prepare("bingx", &ids, fused),
        Err(SignError::InvalidRequest)
    ));
    let none = Identities {
        primary: None,
        secondary: None,
    };
    assert!(matches!(
        hosted_signing::prepare("bingx", &none, Input::Credential {}),
        Err(SignError::Unavailable)
    ));
}

#[test]
fn bingxspot_keeps_the_existing_hosted_hmac_contract() {
    let (provider, ids) = fixture();
    let (work, kind) = hosted_signing::prepare(
        "bingxspot",
        &ids,
        Input::Http {
            payload: TO_SIGN.into(),
            stamp_ms: 1_700_000_000_000.,
            revision: "0".into(),
        },
    )
    .unwrap();
    assert_eq!(work.context.venue, "bingxspot");
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(output.signature.as_deref(), Some(GOLDEN));
}

// Exercise actual Spot request construction; the shared registry stays closed.
#[test]
fn every_spot_rest_request_matches_hosted_signing_and_local_custody() {
    use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Query, Response};
    use safe_cex_venue_bingx::{Options, spot::BingxSpot};
    const NOW: f64 = 1_702_720_824_000.;
    const ROOT: &str = "https://open-api.bingx.com";
    let (provider, ids) = fixture();
    let mut engine = safe_cex_engine::test_support::seeded_engine(BingxSpot::new(Options {
        credentials: ids.primary.clone(),
        read_credentials: None,
        public_data_only: false,
    }));
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    fn drain(engine: &mut Engine<BingxSpot>, now: f64, seen: &mut Vec<HttpRequest>) -> Vec<Action> {
        let actions = safe_cex_engine::test_support::prepared_actions(engine, now);
        for action in &actions {
            if let Action::Http(request) = action {
                seen.push(request.clone());
                let path = request.url.split('?').next().unwrap();
                let body = if path.ends_with("/common/symbols") {
                    include_str!("../../venues/bingx/tests/fixtures/spot/markets.json")
                } else if path.ends_with("/quote/ticker") {
                    include_str!("../../venues/bingx/tests/fixtures/spot/tickers.json")
                } else if path.ends_with("/account/balance") {
                    include_str!("../../venues/bingx/tests/fixtures/spot/balances.json")
                } else if path.ends_with("/userDataStream") {
                    r#"{"listenKey":"fixture-session"}"#
                } else if path.ends_with("/trade/order") {
                    r#"{"code":0,"data":{"orderId":9007199254740993,"symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"PENDING"}}"#
                } else if path.ends_with("/trade/cancel") {
                    r#"{"code":0,"data":{"orderId":9007199254740993,"symbol":"BTC-USDT","status":"CANCELED"}}"#
                } else if path.ends_with("/trade/myTrades") {
                    r#"{"code":0,"data":[]}"#
                } else if path.ends_with("/trade/query") {
                    r#"{"code":100404,"msg":"not found"}"#
                } else {
                    r#"{"code":0,"data":{"orders":[]}}"#
                };
                engine.handle(
                    now,
                    Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body,
                        },
                    },
                );
            }
        }
        actions
    }
    let mut seen = vec![];
    let mut renewal = None;
    engine.start(NOW);
    for _ in 0..8 {
        for action in drain(&mut engine, NOW, &mut seen) {
            if let Action::SetTimer { timer, at_ms } = action
                && at_ms == NOW + 1_800_000.
            {
                renewal = Some(timer);
            }
        }
    }
    let commands = [
        Command::Query { command: 1, query: Query::OrderByClientId { symbol: "BTCUSDT".into(), client_order_id: "spot_1".into() } },
        Command::Query { command: 2, query: Query::Fills { symbol: Some("BTCUSDT".into()), order_id: None, from_ms: None, to_ms: None, limit: Some(10) } },
        Command::PlaceOrders { command: 3, orders: serde_json::from_value(serde_json::json!([{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":41000,"clientOrderId":"spot_1"}])).unwrap() },
        Command::CancelOrdersWithSymbols { command: 4, orders: serde_json::from_value(serde_json::json!([{"id":"9007199254740993","symbol":"BTCUSDT"}])).unwrap() },
        Command::CancelSymbolOrders { command: 5, symbol: "BTCUSDT".into() },
    ];
    for command in commands {
        engine.handle(NOW, Event::Command(command));
        for _ in 0..4 {
            drain(&mut engine, NOW, &mut seen);
        }
    }
    engine.handle(
        NOW + 1_800_000.,
        Event::TimerFired {
            timer: renewal.expect("listen-key renewal timer"),
        },
    );
    for _ in 0..4 {
        drain(&mut engine, NOW + 1_800_000., &mut seen);
    }
    let (provider, ids) = fixture();
    let cases = [
        (
            Method::Get,
            "/openApi/spot/v1/account/balance",
            "timestamp=1702720824000",
        ),
        (
            Method::Get,
            "/openApi/spot/v1/trade/openOrders",
            "timestamp=1702720824000",
        ),
        (
            Method::Get,
            "/openApi/spot/v1/trade/query",
            "clientOrderID=spot_1&symbol=BTC-USDT&timestamp=1702720824000",
        ),
        (
            Method::Post,
            "/openApi/spot/v1/trade/cancel",
            "orderId=9007199254740993&symbol=BTC-USDT&timestamp=1702720824000",
        ),
        (
            Method::Post,
            "/openApi/spot/v1/trade/cancelOpenOrders",
            "symbol=BTC-USDT&timestamp=1702720824000",
        ),
        (
            Method::Get,
            "/openApi/spot/v1/trade/myTrades",
            "limit=10&symbol=BTC-USDT&timestamp=1702720824000",
        ),
        (
            Method::Post,
            "/openApi/spot/v1/trade/order",
            "newClientOrderId=spot_1&price=41000&quantity=0.002&side=BUY&symbol=BTC-USDT&timeInForce=GTC&timestamp=1702720824000&type=LIMIT",
        ),
        (Method::Post, "/openApi/user/auth/userDataStream", ""),
        (
            Method::Put,
            "/openApi/user/auth/userDataStream",
            "listenKey=fixture-session",
        ),
    ];
    for (method, path, payload) in cases {
        let request = seen
            .iter()
            .find(|r| {
                r.method == method && r.url.split('?').next() == Some(&format!("{ROOT}{path}"))
            })
            .unwrap_or_else(|| panic!("missing {method:?} {path}"));
        let signed = path.contains("/spot/");
        let input = if signed {
            Input::Http {
                payload: payload.into(),
                stamp_ms: NOW,
                revision: "0".into(),
            }
        } else {
            Input::Credential {}
        };
        let (work, kind) = hosted_signing::prepare("bingxspot", &ids, input).unwrap();
        let remote = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
        assert_eq!(
            request
                .headers
                .iter()
                .find(|(key, _)| *key == "X-BX-APIKEY")
                .map(|(_, value)| value.as_str()),
            remote.credential.as_deref(),
            "{path}"
        );
        if method == Method::Post {
            let mut expected = serde_json::Map::new();
            for pair in payload.split('&').filter(|pair| !pair.is_empty()) {
                let (key, value) = pair.split_once('=').unwrap();
                let value = if ["price", "quantity", "timestamp"].contains(&key) {
                    serde_json::from_str(value).unwrap()
                } else {
                    serde_json::json!(value)
                };
                expected.insert(key.into(), value);
            }
            if let Some(signature) = remote.signature {
                expected.insert("signature".into(), signature.into());
            }
            if expected.is_empty() {
                assert_eq!(request.body, None, "{path}");
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(request.body.as_deref().unwrap())
                        .unwrap(),
                    serde_json::Value::Object(expected),
                    "{path}"
                );
            }
            assert_eq!(request.url, format!("{ROOT}{path}"));
        } else {
            let query = match remote.signature {
                Some(signature) => format!("{payload}&signature={signature}"),
                None => payload.into(),
            };
            assert_eq!(request.url, format!("{ROOT}{path}?{query}"), "{path}");
            assert_eq!(request.body, None);
        }
    }
}
