//! Delegated signing resumes the same typed request only after the host answers.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, RequestId};
use safe_cex_venue_paradex::Options;
use support::*;

fn seed() -> Harness {
    let mut h = Harness::empty(Options {
        key: Some("opaque-public-subkey".into()),
        paradex_address: Some("0x123".into()),
        subkey_address: Some("0x456".into()),
        payload_signer: true,
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h
}
fn signing(h: &mut Harness) -> (RequestId, safe_cex_engine::PayloadSignRequest) {
    let index = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SignPayload { .. }))
        .expect("delegated signing action");
    let Action::SignPayload {
        request,
        payload,
        timeout_ms,
    } = h.actions.remove(index)
    else {
        unreachable!()
    };
    assert_eq!(timeout_ms, 20_000);
    assert_eq!(payload.exchange_name, "paradex");
    assert_eq!(payload.key, "opaque-public-subkey");
    assert_eq!(payload.scheme, "stark");
    assert_eq!(payload.encoding, "felt-pair");
    assert!(payload.payload.starts_with("0x"));
    (request, payload)
}
fn signed(h: &mut Harness, request: RequestId) {
    h.e.handle(
        h.now,
        Event::PayloadSigned {
            request,
            signature: "opaque signer answer \"quoted\"",
            headers: vec![
                ("X-Signer-Header".into(), "delegation-proof".into()),
                ("authorization".into(), "must-not-overwrite-jwt".into()),
                (
                    "paradex-starknet-signature".into(),
                    "must-not-overwrite-auth".into(),
                ),
                ("content-type".into(), "text/plain".into()),
            ],
        },
    );
    h.collect();
}
fn loaded() -> Harness {
    let mut h = seed();
    let (id, _) = signing(&mut h);
    signed(&mut h, id);
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h
}
fn order(kind: OrderType) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTC-USD-PERP","type":{},"side":"buy","amount":1,"price":100}}"#,
        serde_json::to_string(&kind).unwrap()
    ))
    .unwrap()
}
#[test]
fn auth_waits_for_signer_then_forwards_exact_answer_and_headers() {
    let mut h = seed();
    assert!(h.pending.is_empty());
    let (id, payload) = signing(&mut h);
    assert!(!payload.payload.contains("opaque-public-subkey"));
    h.now += 123.;
    signed(&mut h, id);
    let request = h.take("/auth/");
    assert_eq!(request.id, id);
    assert_eq!(request.route, Some(safe_cex_engine::RequestRoute::Direct));
    assert_eq!(request.timeout_ms, Some(19_877));
    assert_eq!(request.url, "https://api.prod.paradex.trade/v1/auth/0x456");
    for (key, expected) in [
        ("PARADEX-STARKNET-ACCOUNT", "0x123"),
        (
            "PARADEX-STARKNET-SIGNATURE",
            "opaque signer answer \"quoted\"",
        ),
        ("PARADEX-TIMESTAMP", "1700000000"),
        ("PARADEX-SIGNATURE-EXPIRATION", "1700086400"),
        ("Content-Type", "application/json"),
    ] {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == key && value == expected)
        );
    }
    assert!(
        request
            .signer_headers
            .iter()
            .any(|(name, value)| name == "X-Signer-Header" && value == "delegation-proof")
    );
    assert!(
        !request
            .signer_headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("PARADEX-STARKNET-SIGNATURE"))
    );
    h.e.handle(
        h.now,
        Event::PayloadSigned {
            request: id,
            signature: "duplicate",
            headers: vec![],
        },
    );
    h.collect();
    assert!(h.pending.is_empty());
}
#[test]
fn a_single_limit_batch_and_market_preserve_opaque_signatures_and_jwt() {
    for kind in [OrderType::Limit, OrderType::Market] {
        let mut h = loaded();
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(kind)],
        });
        assert!(h.pending.is_empty());
        let (id, _) = signing(&mut h);
        signed(&mut h, id);
        let request = h.take("/orders");
        #[derive(serde::Deserialize)]
        struct Body {
            signature: String,
        }
        let signature = if kind == OrderType::Limit {
            assert!(request.url.ends_with("/orders/batch"));
            serde_json::from_str::<Vec<Body>>(request.body.as_deref().unwrap())
                .unwrap()
                .remove(0)
                .signature
        } else {
            serde_json::from_str::<Body>(request.body.as_deref().unwrap())
                .unwrap()
                .signature
        };
        assert_eq!(signature, "opaque signer answer \"quoted\"");
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == "Authorization" && value == "Bearer jwt")
        );
        assert!(
            !request
                .signer_headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case("Authorization"))
        );
    }
}
#[test]
fn batch_refusal_precedes_any_quote_or_signing_work() {
    let mut h = loaded();
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![order(OrderType::Limit), order(OrderType::Market)],
    });
    assert!(h.pending.is_empty());
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::SignPayload { .. }))
    );
    assert!(
        h.result(2)
            .unwrap_err()
            .contains("not supported with payloadSigner")
    );
}
#[test]
fn rejected_or_expired_signatures_never_send_a_write_and_late_results_are_ignored() {
    for expired in [false, true] {
        let mut h = loaded();
        h.command(Command::PlaceOrders {
            command: 3,
            orders: vec![order(OrderType::Market)],
        });
        let (id, _) = signing(&mut h);
        if expired {
            h.now += 20_000.;
            signed(&mut h, id);
        } else {
            h.e.handle(
                h.now,
                Event::PayloadSignFailed {
                    request: id,
                    error: "delegation rejected",
                },
            );
            h.collect();
        }
        assert!(h.result(3).is_err());
        assert!(h.pending.is_empty());
        signed(&mut h, id);
        assert!(h.pending.is_empty());
    }
}
#[test]
fn malformed_signer_headers_fail_before_http_and_preserve_account_readiness() {
    let mut h = loaded();
    h.command(Command::PlaceOrders {
        command: 4,
        orders: vec![order(OrderType::Market)],
    });
    let (id, _) = signing(&mut h);
    h.e.handle(
        h.now,
        Event::PayloadSigned {
            request: id,
            signature: "answer",
            headers: vec![("X-Proof".into(), "invalid\r\nInjected: yes".into())],
        },
    );
    h.collect();
    assert!(h.result(4).is_err());
    assert!(h.pending.is_empty());
    h.command(Command::PlaceOrders {
        command: 5,
        orders: vec![order(OrderType::Market)],
    });
    let _ = signing(&mut h);
}
