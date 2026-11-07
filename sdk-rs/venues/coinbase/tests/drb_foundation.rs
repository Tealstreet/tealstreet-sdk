//! Offline protocol oracles from Coinbase's technical guide and Deribit's
//! complete RPC error-code table (not the conflicting introductory summary).
use safe_cex_engine::{
    Action, Command, Cx, ErrorKind, Response, SocketId, TimerId, Venue,
    book::Book,
    classify::Verdict,
    request_context::{
        Operation, Policy, RetryPolicy, Scope, SessionBearer, SessionHeaders, SessionPolicy,
    },
};
use safe_cex_venue_coinbase::{drb::*, drb_auth::Session, sign::Credentials};
use serde_json::{Value, json};

struct Noop<F> {
    start: F,
}
impl<F: FnMut(&mut Cx<'_, ()>)> Venue for Noop<F> {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        (self.start)(cx);
    }
    fn session_bearer(&self, _: u64) -> SessionBearer {
        SessionBearer::Ready {
            headers: SessionHeaders::bearer("fixture").unwrap(),
            revision: 1,
        }
    }
    fn capture_http_reconciliation(
        &self,
        _: &(),
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("fixture-order".into())
    }
    fn session_bearer_revision(&self, _: u64) -> Option<u64> {
        Some(1)
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn run(now: f64, f: impl FnMut(&mut Cx<'_, ()>)) -> Vec<Action> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Noop { start: f });
    engine.start(now);
    safe_cex_engine::test_support::prepared_actions(&mut engine, now)
}
fn run_staged(now: f64, f: impl FnMut(&mut Cx<'_, ()>)) -> Vec<Action> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Noop { start: f });
    engine.start(now);
    engine.drain_actions().collect()
}
fn policy(operation: Operation) -> Policy {
    Policy {
        operation,
        credential: None,
        timeout_ms: REQUEST_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
// Synthetic session authority lets the shared transport fixtures exercise both
// public and private packets. Production trading retains its command context.
fn http(
    t: &mut Transport,
    cx: &mut Cx<'_, ()>,
    method: &str,
    params: Value,
    authorization: Option<&str>,
    pending: (),
) -> Result<(RpcId, safe_cex_engine::RequestId), String> {
    let session = authorization.map(|_| {
        cx.open_session(SessionPolicy {
            credential: None,
            repeat_safe: false,
            private_egress: None,
        })
        .unwrap()
    });
    let context = if let Some(session) = &session {
        cx.session_context(
            session,
            policy(if method == "private/buy" {
                Operation::Mutation
            } else {
                Operation::Read
            }),
        )
    } else {
        cx.http_context(Scope::Background, policy(Operation::Read))
    }
    .map_err(|e| e.to_string())?;
    t.http(cx, &context, method, params, session.as_ref(), pending)
}
fn send(t: &mut Transport) -> RpcId {
    let mut id = 0;
    run(1000., |cx| {
        id = http(
            t,
            cx,
            "private/buy",
            json!({"amount":0.001}),
            Some("Bearer fixture"),
            (),
        )
        .unwrap()
        .0;
    });
    id
}
fn auth_result(expires: u64) -> Value {
    json!({"access_token":"fixture-access", "expires_in":expires, "scope":"trade:read_write", "token_type":"bearer"})
}

#[test]
fn symbol_aliases_round_trip_without_changing_spot_or_options() {
    for base in ["BTC", "ETH", "1000PEPE"] {
        let alias = format!("{base}-PERP-INTX");
        let instrument = format!("{base}_USDC-PERPETUAL");
        assert_eq!(to_instrument(&alias).unwrap(), instrument);
        assert_eq!(from_instrument(&instrument).unwrap(), alias);
    }
    for invalid in [
        "BTC-USD",
        "btc-PERP-INTX",
        "-PERP-INTX",
        "BTC/ETH-PERP-INTX",
        "BTC-PERP-INTX\n",
    ] {
        assert!(to_instrument(invalid).is_err());
    }
    for invalid in [
        "BTC-PERPETUAL",
        "BTC-25MAR26-100000-C",
        "_USDC-PERPETUAL",
        "BTC_USDT-PERPETUAL",
    ] {
        assert!(from_instrument(invalid).is_err());
    }
}

#[test]
fn http_and_websocket_use_single_rpc_envelopes_and_unique_ids() {
    let mut t = Transport::default();
    let mut ids = vec![];
    let actions = run(1000., |cx| {
        ids.push(
            http(
                &mut t,
                cx,
                "public/get_instruments",
                json!({"currency":"USDC"}),
                None,
                (),
            )
            .unwrap()
            .0,
        );
        ids.push(
            t.websocket(
                cx,
                SocketId(7),
                "public/subscribe",
                json!({"channels":["ticker.BTC_USDC-PERPETUAL.raw"]}),
            )
            .unwrap(),
        );
        ids.push(
            http(
                &mut t,
                cx,
                "private/buy",
                json!({"amount":0.001,"price":65000.5}),
                Some("Bearer fixture"),
                (),
            )
            .unwrap()
            .0,
        );
    });
    assert_eq!(ids, [1, 2, 3]);
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].url,
        format!("{HTTP_URL}/public/get_instruments")
    );
    assert_eq!(requests[0].method, safe_cex_engine::Method::Post);
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(h, _)| *h == "Authorization")
    );
    assert!(
        requests[1]
            .headers
            .contains(&("Authorization", "Bearer fixture".into()))
    );
    let body: Value = serde_json::from_str(requests[1].body.as_deref().unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"jsonrpc":"2.0","id":3,"method":"private/buy","params":{"amount":0.001,"price":65000.5}})
    );
    assert!(actions.iter().any(|a| matches!(a, Action::WsSend {socket:SocketId(7),text} if serde_json::from_str::<Value>(text).unwrap()["id"] == 2)));
    assert_eq!(
        t.response(
            3,
            200,
            r#"{"jsonrpc":"2.0","id":3,"result":{"order":{"order_id":"fixture"}}}"#
        )
        .unwrap()["order"]["order_id"],
        "fixture"
    );
    assert_eq!(
        t.response(1, 200, r#"{"jsonrpc":"2.0","id":1,"result":[]}"#)
            .unwrap(),
        json!([])
    );
    assert_eq!(
        t.response(2, 200, r#"{"jsonrpc":"2.0","id":2,"result":["ticker"]}"#)
            .unwrap(),
        json!(["ticker"])
    );
    assert!(
        t.response(3, 200, r#"{"jsonrpc":"2.0","id":3,"result":true}"#)
            .is_err()
    );
}

#[test]
fn invalid_requests_are_never_released() {
    let mut t = Transport::default();
    let actions = run(1000., |cx| {
        for method in [
            "private/buy?token=secret",
            "https://evil/public/auth",
            "public/Auth",
            "public/a/b",
        ] {
            assert!(http(&mut t, cx, method, json!({}), None, ()).is_err());
        }
        assert!(http(&mut t, cx, "private/buy", json!({}), None, ()).is_err());
        assert!(http(&mut t, cx, "public/test", json!([]), None, ()).is_err());
        assert!(
            t.websocket(
                cx,
                SocketId(1),
                "public/test",
                json!({"x":"a".repeat(MAX_FRAME_BYTES)})
            )
            .is_err()
        );
    });
    assert!(actions.is_empty());
}

#[test]
fn only_documented_correlated_errors_prove_refusal() {
    for (code, kind) in [
        (10009, ErrorKind::InsufficientBalance),
        (10028, ErrorKind::RateLimited),
        (13009, ErrorKind::AuthExpired),
        (-32602, ErrorKind::InvalidInput),
        (10020, ErrorKind::InvalidInput),
    ] {
        let mut t = Transport::default();
        let id = send(&mut t);
        let failure = t.response(id,200,&json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":"fixture","data":{"reason":"fixture"}}}).to_string()).unwrap_err();
        assert_eq!(failure.code, Some(code));
        assert_eq!(failure.verdict, Verdict::Refused(kind));
    }
    for body in [
        r#"{"jsonrpc":"2.0","id":2,"error":{"code":10009,"message":"not_enough_funds"}}"#,
        r#"{"jsonrpc":"2.0","error":{"code":10009,"message":"not_enough_funds"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":"10009","message":"not_enough_funds"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":10009}}"#,
        r#"{"jsonrpc":"2.0","id":1,"result":{},"error":{"code":10009,"message":"x"}}"#,
        r#"{"jsonrpc":"1.0","id":1,"result":{}}"#,
        r#"[{"jsonrpc":"2.0","id":1,"result":{}}]"#,
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":10001,"message":"error"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":99999,"message":"unknown"}}"#,
        "bad json",
    ] {
        let mut t = Transport::default();
        let id = send(&mut t);
        assert!(
            matches!(
                t.response(id, 200, body).unwrap_err().verdict,
                Verdict::Unknown(_)
            ),
            "{body}"
        );
    }
}

#[test]
fn timeouts_transport_loss_and_server_errors_are_unknown_and_retired() {
    for (status, kind) in [
        (408, ErrorKind::Timeout),
        (500, ErrorKind::VenueUnavailable),
        (503, ErrorKind::VenueUnavailable),
    ] {
        let mut t = Transport::default();
        let id = send(&mut t);
        let body =
            json!({"jsonrpc":"2.0","id":id,"error":{"code":10009,"message":"not_enough_funds"}})
                .to_string();
        assert_eq!(
            t.response(id, status, &body).unwrap_err().verdict,
            Verdict::Unknown(Some(kind))
        );
    }
    let mut t = Transport::default();
    let id = send(&mut t);
    assert_eq!(
        t.failed(id, ErrorKind::Network).unwrap().verdict,
        Verdict::Unknown(Some(ErrorKind::Network))
    );
    assert!(t.failed(id, ErrorKind::Network).is_none());
    let id = send(&mut t);
    assert!(t.expire(1001.).is_empty());
    let expired = t.expire(31_001.);
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].0, id);
    assert_eq!(
        expired[0].1.verdict,
        Verdict::Unknown(Some(ErrorKind::Timeout))
    );
    assert!(t.expire(40_000.).is_empty());
    run(1000., |cx| {
        t.websocket(cx, SocketId(7), "private/buy", json!({}))
            .unwrap();
        t.websocket(cx, SocketId(8), "public/test", json!({}))
            .unwrap();
    });
    let lost = t.disconnected(SocketId(7));
    assert_eq!(lost.len(), 1);
    assert_eq!(
        lost[0].1.verdict,
        Verdict::Unknown(Some(ErrorKind::Network))
    );
    assert!(t.disconnected(SocketId(7)).is_empty());
    assert_eq!(t.disconnected(SocketId(8)).len(), 1);
}

#[test]
fn credit_pools_span_http_and_ws_and_refill_without_retrying_requests() {
    let mut t = Transport::default();
    run(1000., |cx| {
        for i in 0..20 {
            if i % 2 == 0 {
                http(
                    &mut t,
                    cx,
                    "private/buy",
                    json!({}),
                    Some("Bearer fixture"),
                    (),
                )
                .unwrap();
            } else {
                t.websocket(cx, SocketId(1), "private/sell", json!({}))
                    .unwrap();
            }
        }
        assert!(
            t.websocket(cx, SocketId(1), "private/buy", json!({}))
                .is_err()
        );
        // General query credits are separate from matching-engine credits.
        http(&mut t, cx, "public/test", json!({}), None, ()).unwrap();
    });
    run(1199., |cx| {
        assert!(
            t.websocket(cx, SocketId(1), "private/buy", json!({}))
                .is_err()
        )
    });
    run(1200., |cx| {
        t.websocket(cx, SocketId(1), "private/buy", json!({}))
            .unwrap();
    });
    let mut t = Transport::default();
    run(1000., |cx| {
        for _ in 0..10 {
            t.websocket(cx, SocketId(1), "public/subscribe", json!({}))
                .unwrap();
        }
        assert!(
            t.websocket(cx, SocketId(1), "private/subscribe", json!({}))
                .is_err()
        );
    });
    run(1300., |cx| {
        t.websocket(cx, SocketId(1), "public/subscribe", json!({}))
            .unwrap();
    });
    for (method, burst) in [
        ("public/get_instruments", 50),
        ("private/get_transaction_log", 8),
    ] {
        let mut t = Transport::default();
        run(1000., |cx| {
            for _ in 0..burst {
                t.websocket(cx, SocketId(1), method, json!({})).unwrap();
            }
            assert!(t.websocket(cx, SocketId(1), method, json!({})).is_err());
        });
        run(2000., |cx| {
            t.websocket(cx, SocketId(1), method, json!({})).unwrap();
        });
    }
}

#[test]
fn rate_limit_reply_exhausts_local_credits_without_resending() {
    let mut t = Transport::default();
    let id = send(&mut t);
    t.response(
        id,
        200,
        &json!({"jsonrpc":"2.0","id":id,"error":{"code":10028,"message":"too_many_requests"}})
            .to_string(),
    )
    .unwrap_err();
    let actions = run(1000., |cx| {
        assert!(
            http(
                &mut t,
                cx,
                "private/buy",
                json!({}),
                Some("Bearer fixture"),
                ()
            )
            .is_err()
        )
    });
    assert!(actions.is_empty());
    assert!(t.set_matching_limits(f64::NAN, 20.).is_err());
    assert!(t.set_matching_limits(5., 0.).is_err());
    t.set_matching_limits(10., 30.).unwrap();
}

#[test]
fn auth_caches_per_channel_and_refreshes_before_reported_or_policy_expiry() {
    let mut http = Session::new(Channel::Http);
    assert!(http.needs_refresh(1000.));
    http.accept(auth_result(3600), 1000.).unwrap();
    assert_eq!(
        http.authorization(1001.).as_deref(),
        Some("Bearer fixture-access")
    );
    let refresh = http.refresh_at().unwrap();
    assert!(refresh < 1000. + 15. * 60. * 1000.);
    assert!(!http.needs_refresh(refresh - 1.));
    assert!(http.needs_refresh(refresh));
    assert!(http.authorization(refresh).is_none());
    let mut ws = Session::new(Channel::WebSocket(SocketId(7)));
    ws.accept(auth_result(3600), 1000.).unwrap();
    assert!(ws.refresh_at().unwrap() > refresh);
    assert!(ws.refresh_at().unwrap() < 1000. + 50. * 60. * 1000.);
    assert!(ws.authorization(1001.).is_none());
    let fresh = Session::new(Channel::WebSocket(SocketId(8)));
    assert!(fresh.needs_refresh(1001.));
    http.accept(auth_result(10), 1000.).unwrap();
    assert!(http.refresh_at().unwrap() < 11_000.);
    assert!(http.needs_refresh(f64::NAN));
    assert!(http.needs_refresh(999.));
    http.invalidate();
    assert!(http.authorization(1001.).is_none());
    for result in [
        json!({}),
        auth_result(0),
        json!({"access_token":"x\nx","expires_in":100,"scope":"trade","token_type":"bearer"}),
    ] {
        assert!(http.accept(result, 1000.).is_err());
    }
}

#[test]
fn auth_exchange_signs_fresh_cdp_jwt_and_keeps_it_out_of_urls() {
    use base64::{
        Engine as _,
        engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    };
    let credentials = Credentials::new(
        "organizations/test/apiKeys/fixture",
        &STANDARD.encode([7u8; 32]),
    )
    .unwrap();
    for channel in [Channel::Http, Channel::WebSocket(SocketId(7))] {
        let s = Session::new(channel);
        let a = s
            .params(&credentials, 1_000_000., |b| {
                b.fill(1);
                Ok(())
            })
            .unwrap();
        let b = s
            .params(&credentials, 1_000_000., |b| {
                b.fill(2);
                Ok(())
            })
            .unwrap();
        assert_eq!(a["grant_type"], "coinbase_cdp");
        assert_ne!(a["token"], b["token"]);
        let parts: Vec<_> = a["token"].as_str().unwrap().split('.').collect();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["sub"], "organizations/test/apiKeys/fixture");
        assert_eq!(claims["exp"], 1120);
        let mut t = Transport::default();
        let actions = run(1_000_000., |cx| {
            http(&mut t, cx, "public/auth", a.clone(), None, ()).unwrap();
        });
        let request = actions
            .into_iter()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(!request.url.contains(a["token"].as_str().unwrap()));
        assert!(request.body.unwrap().contains(a["token"].as_str().unwrap()));
    }
}

#[test]
fn gateway_es256_jwt_uses_cdp_issuer_and_valid_signature() {
    use base64::{
        Engine as _,
        engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    };
    use p256::{
        ecdsa::{Signature, VerifyingKey, signature::Verifier},
        pkcs8::EncodePrivateKey,
    };
    let key = p256::SecretKey::from_slice(&[7; 32]).unwrap();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(key.to_pkcs8_der().unwrap().as_bytes())
    );
    let credentials = Credentials::new("organizations/test/apiKeys/fixture", &pem).unwrap();
    let s = Session::new(Channel::Http);
    let params = s
        .params(&credentials, 1_000_000., |b| {
            b.fill(3);
            Ok(())
        })
        .unwrap();
    let jwt = params["token"].as_str().unwrap();
    let parts: Vec<_> = jwt.split('.').collect();
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(claims["iss"], "cdp");
    assert_eq!(claims["uri"], "POST drb.coinbase.com/api/v2/public/auth");
    VerifyingKey::from(key.public_key())
        .verify(
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap(),
        )
        .unwrap();
}

#[test]
fn delayed_rate_limit_response_starts_cooldown_at_receipt_and_late_acks_are_unknown() {
    let mut t = Transport::default();
    let id = send(&mut t);
    t.response_at(
        id,
        200,
        &json!({"jsonrpc":"2.0","id":id,"error":{"code":10028,"message":"too_many_requests"}})
            .to_string(),
        2000.,
    )
    .unwrap_err();
    run(2000., |cx| {
        assert!(
            t.websocket(cx, SocketId(1), "private/buy", json!({}))
                .is_err()
        )
    });
    run(2199., |cx| {
        assert!(
            t.websocket(cx, SocketId(1), "private/buy", json!({}))
                .is_err()
        )
    });
    run(2200., |cx| {
        t.websocket(cx, SocketId(1), "private/buy", json!({}))
            .unwrap();
    });
    let mut t = Transport::default();
    let id = send(&mut t);
    assert_eq!(
        t.response_at(
            id,
            200,
            &json!({"jsonrpc":"2.0","id":id,"result":true}).to_string(),
            31_000.
        )
        .unwrap_err()
        .verdict,
        Verdict::Unknown(Some(ErrorKind::Timeout))
    );
}

#[test]
fn null_results_are_valid_but_duplicate_envelope_fields_are_unknown() {
    let mut t = Transport::default();
    let id = send(&mut t);
    assert_eq!(
        t.response(id, 200, r#"{"jsonrpc":"2.0","id":1,"result":null}"#)
            .unwrap(),
        Value::Null
    );
    for body in [
        r#"{"jsonrpc":"2.0","id":1,"id":1,"error":{"code":10009,"message":"x"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":10009,"code":10009,"message":"x"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"result":null,"result":null}"#,
    ] {
        let mut t = Transport::default();
        let id = send(&mut t);
        assert!(matches!(
            t.response(id, 200, body).unwrap_err().verdict,
            Verdict::Unknown(_)
        ));
    }
}

#[test]
fn matching_limit_updates_do_not_reset_used_credits_and_position_moves_obey_weekly_cap() {
    let mut t = Transport::default();
    run(1000., |cx| {
        for _ in 0..20 {
            t.websocket(cx, SocketId(1), "private/buy", json!({}))
                .unwrap();
        }
        t.set_matching_limits(10., 30.).unwrap();
        assert!(
            t.websocket(cx, SocketId(1), "private/buy", json!({}))
                .is_err()
        );
    });
    run(1100., |cx| {
        t.websocket(cx, SocketId(1), "private/buy", json!({}))
            .unwrap();
    });
    let mut t = Transport::default();
    for i in 0..100 {
        run(1000. + f64::from(i) * 10_000., |cx| {
            let id = t
                .websocket(cx, SocketId(1), "private/move_positions", json!({}))
                .unwrap();
            t.response(
                id,
                200,
                &json!({"jsonrpc":"2.0","id":id,"result":null}).to_string(),
            )
            .unwrap();
        });
    }
    run(1_001_000., |cx| {
        assert!(
            t.websocket(cx, SocketId(1), "private/move_positions", json!({}))
                .is_err()
        )
    });
    run(1000. + 168. * 60. * 60. * 1000., |cx| {
        t.websocket(cx, SocketId(1), "private/move_positions", json!({}))
            .unwrap();
    });
}

#[test]
fn auth_rejects_stale_updates_and_debug_omits_tokens_and_gateway_error_text() {
    let mut session = Session::new(Channel::Http);
    session.accept(auth_result(3600), 2000.).unwrap();
    let deadline = session.refresh_at();
    assert!(session.accept(auth_result(3600), 1000.).is_err());
    assert_eq!(session.refresh_at(), deadline);
    assert!(!format!("{session:?}").contains("fixture-access"));
    let mut t = Transport::default();
    let id = send(&mut t);
    let failure=t.response(id,200,&json!({"jsonrpc":"2.0","id":id,"error":{"code":13009,"message":"fixture-secret","data":{"token":"fixture-secret"}}}).to_string()).unwrap_err();
    assert!(!format!("{failure:?}").contains("fixture-secret"));
    assert_eq!(
        failure.rpc_error.unwrap().data.unwrap()["token"],
        "fixture-secret"
    );
}

#[test]
fn invalidation_keeps_the_auth_completion_watermark() {
    let mut session = Session::new(Channel::Http);
    session.accept(auth_result(3600), 2000.).unwrap();
    // A credential refusal invalidates the accepted authentication.
    session.invalidate();
    // A delayed, correlated reply from the older auth must not revive it.
    assert!(session.accept(auth_result(3600), 1000.).is_err());
    assert!(session.authorization(2001.).is_none());
    assert!(session.needs_refresh(2001.));
    // A genuinely fresh exchange remains admissible after invalidation.
    session.accept(auth_result(3600), 3000.).unwrap();
    assert_eq!(
        session.authorization(3001.).as_deref(),
        Some("Bearer fixture-access")
    );
}

#[test]
fn http_requests_require_managed_preparation_before_physical_release() {
    let mut transport = Transport::default();
    let actions = run_staged(1000., |cx| {
        http(
            &mut transport,
            cx,
            "public/get_instruments",
            json!({"currency":"USDC"}),
            None,
            (),
        )
        .unwrap();
    });
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::ManagedHttp(_)))
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn read_context_cannot_authorize_an_http_order_mutation() {
    let mut transport = Transport::default();
    run_staged(1000., |cx| {
        let session = cx
            .open_session(SessionPolicy {
                credential: None,
                repeat_safe: false,
                private_egress: None,
            })
            .unwrap();
        let read = cx
            .http_context(Scope::Background, policy(Operation::Read))
            .unwrap();
        assert!(
            transport
                .http(cx, &read, "private/buy", json!({}), Some(&session), ())
                .is_err()
        );
    });
}
