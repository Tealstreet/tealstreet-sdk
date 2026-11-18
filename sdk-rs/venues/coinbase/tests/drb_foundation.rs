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
        cx.session_context(session, policy(Operation::Mutation))
    } else {
        cx.http_context(Scope::Background, policy(Operation::Read))
    }
    .map_err(|e| e.to_string())?;
    t.http(cx, &context, method, params, session.as_ref(), pending)
}
// Mutation and general-query credit oracles use the managed HTTP path;
// the socket helper now accepts only typed control operations.
fn http_rpc(
    t: &mut Transport,
    cx: &mut Cx<'_, ()>,
    method: &str,
    params: Value,
) -> Result<RpcId, String> {
    http(t, cx, method, params, Some("Bearer fixture"), ()).map(|(id, _)| id)
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
                ControlMethod::PublicSubscribe,
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
                ControlMethod::Test,
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
        // send() releases private/buy: rate limiting cannot prove a write was refused.
        let expected = if code == 10028 {
            Verdict::Unknown(Some(kind))
        } else {
            Verdict::Refused(kind)
        };
        assert_eq!(failure.verdict, expected);
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
        t.websocket(cx, SocketId(7), ControlMethod::PrivateSubscribe, json!({}))
            .unwrap();
        t.websocket(cx, SocketId(8), ControlMethod::Test, json!({}))
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
                http_rpc(&mut t, cx, "private/sell", json!({})).unwrap();
            }
        }
        assert!(http_rpc(&mut t, cx, "private/buy", json!({})).is_err());
        // General query credits are separate from matching-engine credits.
        http(&mut t, cx, "public/test", json!({}), None, ()).unwrap();
    });
    run(1199., |cx| {
        assert!(http_rpc(&mut t, cx, "private/buy", json!({})).is_err())
    });
    run(1200., |cx| {
        http_rpc(&mut t, cx, "private/buy", json!({})).unwrap();
    });
    let mut t = Transport::default();
    run(1000., |cx| {
        for i in 0..10 {
            if i % 2 == 0 {
                t.websocket(cx, SocketId(1), ControlMethod::PublicSubscribe, json!({}))
                    .unwrap();
            } else {
                http_rpc(&mut t, cx, "public/subscribe", json!({})).unwrap();
            }
        }
        assert!(
            t.websocket(cx, SocketId(1), ControlMethod::PrivateSubscribe, json!({}))
                .is_err()
        );
    });
    run(1300., |cx| {
        t.websocket(cx, SocketId(1), ControlMethod::PublicSubscribe, json!({}))
            .unwrap();
    });
    for (method, burst) in [
        ("public/get_instruments", 50),
        ("private/get_transaction_log", 8),
    ] {
        let mut t = Transport::default();
        run(1000., |cx| {
            for _ in 0..burst {
                http_rpc(&mut t, cx, method, json!({})).unwrap();
            }
            assert!(http_rpc(&mut t, cx, method, json!({})).is_err());
        });
        run(2000., |cx| {
            http_rpc(&mut t, cx, method, json!({})).unwrap();
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
        assert!(http_rpc(&mut t, cx, "private/buy", json!({})).is_err())
    });
    run(2199., |cx| {
        assert!(http_rpc(&mut t, cx, "private/buy", json!({})).is_err())
    });
    run(2200., |cx| {
        http_rpc(&mut t, cx, "private/buy", json!({})).unwrap();
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
            http_rpc(&mut t, cx, "private/buy", json!({})).unwrap();
        }
        t.set_matching_limits(10., 30.).unwrap();
        assert!(http_rpc(&mut t, cx, "private/buy", json!({})).is_err());
    });
    run(1100., |cx| {
        http_rpc(&mut t, cx, "private/buy", json!({})).unwrap();
    });
    let mut t = Transport::default();
    for i in 0..100 {
        run(1000. + f64::from(i) * 10_000., |cx| {
            let id = http_rpc(&mut t, cx, "private/move_positions", json!({})).unwrap();
            t.response(
                id,
                200,
                &json!({"jsonrpc":"2.0","id":id,"result":null}).to_string(),
            )
            .unwrap();
        });
    }
    run(1_001_000., |cx| {
        assert!(http_rpc(&mut t, cx, "private/move_positions", json!({})).is_err())
    });
    run(1000. + 168. * 60. * 60. * 1000., |cx| {
        http_rpc(&mut t, cx, "private/move_positions", json!({})).unwrap();
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
fn invalidation_fences_pending_authentication_and_completed_replays() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let credentials = Credentials::new(
        "organizations/test/apiKeys/fixture",
        &STANDARD.encode([7u8; 32]),
    )
    .unwrap();
    for channel in [Channel::Http, Channel::WebSocket(SocketId(7))] {
        let mut session = Session::new(channel);
        session.accept(auth_result(3600), 1000.).unwrap();
        for started in [2000., 2500.] {
            session
                .params(&credentials, started, |bytes| {
                    bytes.fill(3);
                    Ok(())
                })
                .unwrap();
        }
        session.invalidate();
        for started in [2000., 2500., 1000.] {
            assert!(
                session.accept(auth_result(3600), started).is_err(),
                "pre-invalidation authentication at {started} revived {channel:?}"
            );
            assert!(session.needs_refresh(3000.));
            assert!(session.authorization(3000.).is_none());
        }
        session
            .params(&credentials, 4000., |bytes| {
                bytes.fill(4);
                Ok(())
            })
            .unwrap();
        session.accept(auth_result(3600), 4000.).unwrap();
        assert!(!session.needs_refresh(4001.));
        session.invalidate();
        assert!(
            session.accept(auth_result(3600), 4000.).is_err(),
            "a same-start replay revived {channel:?}"
        );
    }
}

#[test]
fn invalidation_fences_authentication_before_the_first_completion() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let credentials = Credentials::new(
        "organizations/test/apiKeys/fixture",
        &STANDARD.encode([7u8; 32]),
    )
    .unwrap();
    let mut session = Session::new(Channel::Http);
    session
        .params(&credentials, 1000., |bytes| {
            bytes.fill(3);
            Ok(())
        })
        .unwrap();
    session.invalidate();
    assert!(session.accept(auth_result(3600), 1000.).is_err());
    assert!(session.authorization(1001.).is_none());
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

#[test]
fn background_read_cannot_release_a_margin_model_change() {
    let mut transport = Transport::default();
    let mut refused = false;
    let actions = run(1000., |cx| {
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
        refused = transport
            .http(
                cx,
                &read,
                "private/change_margin_model",
                json!({"margin_model":"cross_pm"}),
                Some(&session),
                (),
            )
            .is_err();
    });
    let http = actions
        .iter()
        .filter(|action| matches!(action, Action::Http(_)))
        .count();
    assert_eq!(
        http, 0,
        "a Background Read released an account-setting HTTP POST"
    );
    assert!(refused, "the setting write must require Mutation authority");
}

// Closed snapshot of the supported Coinbase gateway methods (OpenAPI 2.1.1),
// plus the existing position-move transport contract. Authentication/heartbeat/subscription
// control is Read; account/trading changes require Mutation authority.
const READ_METHODS: &[&str] = &[
    "private/get_access_log",
    "private/get_account_summaries",
    "private/get_account_summary",
    "private/get_cancel_on_disconnect",
    "private/get_leg_prices",
    "private/get_leverage",
    "private/get_margins",
    "private/get_max_order_size",
    "private/get_open_orders",
    "private/get_open_orders_by_currency",
    "private/get_open_orders_by_instrument",
    "private/get_open_orders_by_label",
    "private/get_order_history_by_currency",
    "private/get_order_history_by_instrument",
    "private/get_order_margin_by_ids",
    "private/get_order_state",
    "private/get_order_state_by_label",
    "private/get_pme_params",
    "private/get_position",
    "private/get_positions",
    "private/get_risk_profile",
    "private/get_settlement_history_by_currency",
    "private/get_settlement_history_by_instrument",
    "private/get_subaccounts",
    "private/get_subaccounts_details",
    "private/get_trading_limits",
    "private/get_transaction_log",
    "private/get_trigger_order_history",
    "private/get_user_locks",
    "private/get_user_trades_by_currency",
    "private/get_user_trades_by_currency_and_time",
    "private/get_user_trades_by_instrument",
    "private/get_user_trades_by_instrument_and_time",
    "private/get_user_trades_by_order",
    "private/simulate_portfolio",
    "private/subscribe",
    "private/unsubscribe",
    "private/unsubscribe_all",
    "public/auth",
    "public/disable_heartbeat",
    "public/get_announcements",
    "public/get_book_summary_by_currency",
    "public/get_book_summary_by_instrument",
    "public/get_combo_details",
    "public/get_combo_ids",
    "public/get_combos",
    "public/get_contract_size",
    "public/get_currencies",
    "public/get_delivery_prices",
    "public/get_expirations",
    "public/get_funding_chart_data",
    "public/get_funding_rate_history",
    "public/get_funding_rate_value",
    "public/get_historical_volatility",
    "public/get_index_chart_data",
    "public/get_index_price",
    "public/get_index_price_names",
    "public/get_instrument",
    "public/get_instruments",
    "public/get_last_settlements_by_currency",
    "public/get_last_settlements_by_instrument",
    "public/get_last_trades_by_currency",
    "public/get_last_trades_by_currency_and_time",
    "public/get_last_trades_by_instrument",
    "public/get_last_trades_by_instrument_and_time",
    "public/get_mark_price_history",
    "public/get_order_book",
    "public/get_order_book_by_instrument_id",
    "public/get_supported_index_names",
    "public/get_time",
    "public/get_trade_volumes",
    "public/get_tradingview_chart_data",
    "public/get_volatility_index_data",
    "public/hello",
    "public/set_heartbeat",
    "public/status",
    "public/subscribe",
    "public/test",
    "public/ticker",
    "public/tickers_by_currency",
    "public/unsubscribe",
    "public/unsubscribe_all",
];
const MUTATION_METHODS: &[&str] = &[
    "private/buy",
    "private/cancel",
    "private/cancel_all",
    "private/cancel_all_by_currency",
    "private/cancel_all_by_currency_pair",
    "private/cancel_all_by_instrument",
    "private/cancel_all_by_kind_or_type",
    "private/cancel_by_label",
    "private/change_margin_model",
    "private/close_position",
    "private/create_combo",
    "private/disable_cancel_on_disconnect",
    "private/edit",
    "private/edit_by_label",
    "private/enable_cancel_on_disconnect",
    "private/logout",
    "private/move_positions",
    "private/sell",
    "private/set_leverage",
    "private/submit_transfer_between_subaccounts",
];

#[test]
fn every_supported_method_has_explicit_http_authority() {
    for (methods, required) in [
        (READ_METHODS, Operation::Read),
        (MUTATION_METHODS, Operation::Mutation),
    ] {
        for &method in methods {
            for operation in [Operation::Read, Operation::Mutation] {
                let mut transport = Transport::default();
                let allowed = required == Operation::Read || operation == Operation::Mutation;
                let actions = run(1000., |cx| {
                    let session = cx
                        .open_session(SessionPolicy {
                            credential: None,
                            repeat_safe: false,
                            private_egress: None,
                        })
                        .unwrap();
                    let context = cx.session_context(&session, policy(operation)).unwrap();
                    assert_eq!(
                        transport
                            .http(cx, &context, method, json!({}), Some(&session), ())
                            .is_ok(),
                        allowed,
                        "{method} under {operation:?}"
                    );
                });
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, Action::Http(_)))
                        .count(),
                    usize::from(allowed),
                    "physical release of {method} under {operation:?}"
                );
            }
        }
    }
}

#[test]
fn unknown_methods_are_rejected_on_both_transports_before_admission() {
    for method in [
        "public/get_future_data",
        "public/future_control",
        "private/get_future_setting",
        "private/set_position_mode",
        "private/cancel_future",
        "private/unknown_write",
        "private/mass_quote",
        "private/cancel_quotes",
        "private/mass_quote_cancel",
    ] {
        let mut transport = Transport::default();
        let actions = run(1000., |cx| {
            let session = cx
                .open_session(SessionPolicy {
                    credential: None,
                    repeat_safe: false,
                    private_egress: None,
                })
                .unwrap();
            for operation in [Operation::Read, Operation::Mutation] {
                let context = cx.session_context(&session, policy(operation)).unwrap();
                assert!(
                    transport
                        .http(cx, &context, method, json!({}), Some(&session), ())
                        .is_err(),
                    "unknown {method} admitted under {operation:?}"
                );
            }
            // String methods are rejected by the WS type boundary (see the
            // ControlMethod compile-fail doctest).
            // Rejection consumes no RPC ID: two rejected attempts above
            // precede this first send.
            assert_eq!(
                transport
                    .websocket(cx, SocketId(7), ControlMethod::PublicSubscribe, json!({}))
                    .unwrap(),
                1
            );
        });
        assert_eq!(actions.len(), 1, "unexpected effects for {method}");
        assert!(matches!(&actions[0], Action::WsSend { .. }));
    }
}
