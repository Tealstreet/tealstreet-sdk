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
        ("PARADEX-SIGNATURE-EXPIRATION", "1700000300"),
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

#[test]
fn each_bracket_write_gets_its_own_delegated_hash_signature_and_headers() {
    let mut h = loaded();
    let mut opts = order(OrderType::Limit);
    opts.take_profit = Some(110.);
    opts.stop_loss = Some(90.);
    h.command(Command::PlaceOrders {
        command: 6,
        orders: vec![opts],
    });
    let mut hashes = std::collections::HashSet::new();
    for (id, kind) in [
        ("entry", "LIMIT"),
        ("tp", "TAKE_PROFIT_MARKET"),
        ("sl", "STOP_LOSS_MARKET"),
    ] {
        assert!(h.pending.is_empty());
        let (request, payload) = signing(&mut h);
        assert!(hashes.insert(payload.payload));
        signed(&mut h, request);
        let write = h.take("/orders");
        #[derive(serde::Deserialize)]
        struct Body {
            #[serde(rename = "type")]
            kind: String,
            client_id: Option<String>,
            signature: String,
        }
        let body: Body = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        assert_eq!(body.kind, kind);
        assert_eq!(body.signature, "opaque signer answer \"quoted\"");
        assert!(
            write
                .signer_headers
                .iter()
                .any(|(key, value)| key == "X-Signer-Header" && value == "delegation-proof")
        );
        let ack = match body.client_id {
            Some(client) => format!(r#"{{"id":"{id}","client_id":"{client}"}}"#),
            None => format!(r#"{{"id":"{id}"}}"#),
        };
        h.respond(write, 200, &ack);
    }
    assert_eq!(h.result(6), Ok(r#"["entry","tp","sl"]"#));
}

#[test]
fn a_delegated_amendment_binds_the_stable_venue_id_and_rolls_back_unsent_failures() {
    let mut h = loaded();
    let socket = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    const ROW: &str = r#"{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"BUY","size":"2","remaining_size":"2","price":"99","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000000000}"#;
    let text = format!(
        r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"orders.ALL","data":{ROW}}}}}"#
    );
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    h.collect();
    h.command(Command::AmendOrder {
        command: 7,
        id: "123".into(),
        price: Some(101.),
        amount: None,
    });
    let (id, _) = signing(&mut h);
    h.e.handle(
        h.now,
        Event::PayloadSignFailed {
            request: id,
            error: "signer refused",
        },
    );
    h.collect();
    assert!(h.result(7).is_err());
    assert!(h.pending.is_empty());
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    h.collect();
    h.command(Command::AmendOrder {
        command: 8,
        id: "123".into(),
        price: Some(102.),
        amount: None,
    });
    let (id, payload) = signing(&mut h);
    let config = safe_cex_venue_paradex::sign::SystemConfig {
        starknet_chain_id: "PRIVATE_SN_PARACLEAR_MAINNET".into(),
        l1_chain_id: None,
        paraclear_account_hash: None,
        paraclear_account_proxy_hash: None,
    };
    let signer = safe_cex_venue_paradex::sign::Credentials::delegated(
        "opaque-public-subkey",
        "0x123",
        "0x456",
        &config,
    )
    .unwrap();
    assert_eq!(
        payload.payload,
        signer
            .modify(
                h.now as u64,
                "BTC-USD-PERP",
                "BUY",
                "LIMIT",
                "2",
                "102",
                "123"
            )
            .unwrap()
            .signature
    );
    signed(&mut h, id);
    let write = h.take("/orders/123");
    assert_eq!(write.method, safe_cex_engine::Method::Put);
    assert!(
        write
            .body
            .as_deref()
            .unwrap()
            .contains(r#""signature":"opaque signer answer \"quoted\"""#)
    );
}

#[test]
fn identities_survive_the_jwt_queue_and_precede_delegated_order_signing() {
    let mut h = seed();
    // Auth has not completed. The order may be planned but its receipt does
    // not claim a write while it remains in the JWT prerequisite queue.
    h.command(Command::PlaceOrders {
        command: 77,
        orders: vec![order(OrderType::Limit)],
    });
    assert!(
        !h.receipt_trace
            .iter()
            .any(|a| matches!(a, ReceiptTrace::Progress(77, _)))
    );
    let (auth, _) = signing(&mut h);
    signed(&mut h, auth);
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    let (request, _) = signing(&mut h);
    let sign_index = h
        .receipt_trace
        .iter()
        .position(|a| matches!(a, ReceiptTrace::Sign(id) if *id == request))
        .unwrap();
    let (owner, receipts) = match &h.receipt_trace[sign_index - 1] {
        ReceiptTrace::Progress(command, receipts) => (*command, receipts.clone()),
        _ => panic!("receipt must precede delegated signing"),
    };
    assert_eq!(owner, 77);
    assert_eq!(receipts.len(), 1);
    assert_eq!(
        receipts[0].kind,
        safe_cex_engine::OrderIdentityKind::ClientOrderId
    );
    signed(&mut h, request);
    let write = h.take("/orders/batch");
    let payload: serde_json::Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
    assert_eq!(receipts[0].symbol, payload[0]["market"]);
    assert_eq!(receipts[0].identity, payload[0]["client_id"]);
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|a| matches!(a, ReceiptTrace::Progress(77, _)))
            .count(),
        1
    );
}
