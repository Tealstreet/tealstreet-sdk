mod support;
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{Action, Command, Event, Notice, Outcome, Query};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use support::*;

struct Gate {
    local: Provider,
    parked: Arc<AtomicBool>,
}
impl Signer for Gate {
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.local.sign(work)
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.parked.load(Ordering::SeqCst) {
            None
        } else {
            self.local.sign_ready(work)
        }
    }
}
/// Refuses synchronously, so managed preparation sees it; the immediate
/// fixture cannot park a managed read's signature.
struct Refusing {
    local: Provider,
    refuse: Arc<AtomicBool>,
}
impl Signer for Refusing {
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.local.sign(work)
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.refuse.load(Ordering::SeqCst) {
            Some(Err(SignError::Refused))
        } else {
            self.local.sign_ready(work)
        }
    }
}
fn options() -> FixtureOptions {
    FixtureOptions {
        key: Some("key".into()),
        secret: Some("material".into()),
        passphrase: Some("pass".into()),
        ..Default::default()
    }
}
fn configured(parked: Arc<AtomicBool>) -> Harness {
    Harness::started_configure(options(), false, |engine| {
        let local = engine.signer().unwrap();
        engine
            .set_signer(Some(Arc::new(Gate { local, parked })))
            .unwrap();
    })
}
fn refusing(refuse: Arc<AtomicBool>) -> Harness {
    Harness::started_configure(options(), false, |engine| {
        let local = engine.signer().unwrap();
        engine
            .set_signer(Some(Arc::new(Refusing { local, refuse })))
            .unwrap();
    })
}
fn load_classic(h: &mut Harness) {
    h.load_public();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    h.login();
}
#[test]
fn provider_refused_probe_does_not_promote_classic_or_open_private_socket() {
    let refuse = Arc::new(AtomicBool::new(false));
    let mut h = refusing(refuse.clone());
    let catalogue = h.take("/contracts");
    refuse.store(true, Ordering::SeqCst);
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: catalogue.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: CONTRACTS,
            },
        },
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(http) if http.url.contains("/account/settings")))
    );
    // A refused provider releases no signed request at all.
    assert!(!out.iter().any(|a| matches!(a, Action::Http(http)
        if http.headers.iter().any(|(k, _)| k.starts_with("ACCESS-"))
            || http.signer_headers.iter().any(|(k, _)| k.starts_with("ACCESS-")))));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::WsConnect {url,..} if url.ends_with("/private")))
    );
    assert!(out.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    assert!(!h.engine.store().loaded().balance);
    assert!(!h.engine.store().loaded().positions);
}
#[test]
fn unsent_identity_provider_refusal_is_typed_and_not_an_empty_success() {
    let refuse = Arc::new(AtomicBool::new(false));
    let mut h = refusing(refuse.clone());
    load_classic(&mut h);
    refuse.store(true, Ordering::SeqCst);
    h.engine
        .handle(h.now, Event::Command(Command::FetchAccount { command: 43 }));
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    assert!(out.iter().any(|a| matches!(a,Action::Notice(Notice::CommandFailed {command:43,error}) if error.outcome==Some(Outcome::NotSent))));
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { command: 43, .. })
    )));
}
#[test]
fn history_provider_refusal_is_strict_without_transport_retry_timer() {
    let (mut h, refuse) = refusing_classic();
    refuse.store(true, Ordering::SeqCst);
    h.engine.handle(
        h.now,
        Event::Command(Command::Query {
            command: 44,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: None,
                limit: Some(10),
                order_id: None,
            },
        }),
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    let failures = failed(&out, 44);
    assert_eq!(failures.len(), 1);
    assert_eq!(
        failures[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(failures[0].message, "Signing provider refused the request");
    assert!(out.iter().any(|a| matches!(a,Action::Notice(Notice::CommandFailed {command:44,error}) if error.outcome==Some(Outcome::NotSent))));
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { command: 44, .. })
    )));
    // Managed lifecycle bounds are expected; native transport retries are not.
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. }
        if [250., 500., 750.].iter().any(|delay| *at_ms == h.now + delay)))
    );
}

fn place(h: &mut Harness, command: u32) {
    h.engine.handle(h.now,Event::Command(Command::PlaceOrders { command, orders:vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90,"clientOrderId":"Owned123"}"#).unwrap()] }));
}
/// Trade calls sign through managed preparation; a synchronous provider
/// refusal is that call's unreleased failure.
fn refusing_classic() -> (Harness, Arc<AtomicBool>) {
    let refuse = Arc::new(AtomicBool::new(false));
    let mut h = refusing(refuse.clone());
    load_classic(&mut h);
    (h, refuse)
}
fn failed(out: &[Action], command: u32) -> Vec<&safe_cex_engine::VenueError> {
    out.iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(error)
            }
            _ => None,
        })
        .collect()
}
#[test]
fn first_mutation_refusal_is_not_sent_and_duplicate_completion_is_inert() {
    let (mut h, refuse) = refusing_classic();
    refuse.store(true, Ordering::SeqCst);
    place(&mut h, 45);
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    let failures = failed(&out, 45);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let later = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    assert!(!later.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { command: 45, .. })
    )));
}
#[test]
fn signature_retry_provider_refusal_preserves_the_prior_sent_attempt() {
    let (mut h, refuse) = refusing_classic();
    place(&mut h, 46);
    h.collect();
    let write = h.take("/batch-place-order");
    refuse.store(true, Ordering::SeqCst);
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: write.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":"40009","msg":"signature error","data":null}"#,
            },
        },
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    let failures = failed(&out, 46);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::Unknown));
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn metadata_provider_refusal_bypasses_transport_read_retry() {
    let (mut h, refuse) = refusing_classic();
    h.engine.store_mut().set_positions(vec![]);
    refuse.store(true, Ordering::SeqCst);
    place(&mut h, 47);
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    let failures = failed(&out, 47);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
    // Engine call-deadline timers only; never the native 250 ms read retry.
    let retry = h.now + 250.;
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))
        || matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == retry)));
}

fn uta() -> (Harness, Arc<AtomicBool>) {
    let parked = Arc::new(AtomicBool::new(false));
    let mut h = refusing(parked.clone());
    h.load_public();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        &POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}"),
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h.engine.store_mut().set_hedged(true);
    (h, parked)
}
#[test]
fn rollback_provider_refusal_preserves_original_native_rejection_details() {
    let (mut h, parked) = uta();
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"stop","symbol":"BTCUSDT","qty":"0.5","stopLoss":"80","side":"sell","posSide":"long","status":"pending"}]}"#);
    h.command(Command::AmendOrder {
        command: 48,
        id: "stop".into(),
        price: Some(85.),
        amount: Some(0.),
    });
    h.respond(
        "/cancel-strategy-order",
        200,
        r#"{"code":"00000","data":{}}"#,
    );
    let replace = h.take("/place-strategy-order");
    parked.store(true, Ordering::SeqCst);
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: replace.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":"40017","msg":"replace rejected","data":null}"#,
            },
        },
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 48, error }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.venue_code.as_deref(), Some("40017"));
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Rejected));
    assert_eq!(error.outcome, None);
    // The note now comes from `end`, which every unfinished rollback reaches.
    assert_eq!(
        error.message,
        "Bitget 40017: replace rejected (HTTP 200); protection rollback failed: Signing provider refused the request"
    );
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn managed_mutation_signs_the_exact_body_and_client_id_at_release() {
    let (mut h, _) = refusing_classic();
    h.now += 6001.;
    place(&mut h, 49);
    h.collect();
    let http = h.take("/batch-place-order");
    let header = |key| {
        &http
            .headers
            .iter()
            .find(|(name, _)| *name == key)
            .unwrap()
            .1
    };
    assert_eq!(header("ACCESS-KEY"), "key");
    assert_eq!(header("ACCESS-TIMESTAMP"), &h.now.floor().to_string());
    let body = http.body.as_deref().unwrap();
    let rows: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(rows["orderList"][0]["clientOid"], "Owned123");
    assert_eq!(rows["orderList"][0]["size"], "0.002");
    let path = &http.url[http.url.find("/api/").unwrap()..];
    assert_eq!(
        *header("ACCESS-SIGN"),
        safe_cex_crypto::hmac_sha256(
            b"material",
            format!("{}POST{path}{body}", header("ACCESS-TIMESTAMP")).as_bytes(),
            safe_cex_crypto::Encoding::Base64
        )
    );
    assert_eq!(http.route, None);
}
#[test]
fn private_login_blocks_ping_and_late_signature_after_retirement_is_inert() {
    let parked = Arc::new(AtomicBool::new(false));
    let mut h = configured(parked.clone());
    h.load_public();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    let socket = h
        .sockets
        .iter()
        .find(|(_, url)| url.ends_with("/private"))
        .unwrap()
        .0;
    parked.store(true, Ordering::SeqCst);
    h.engine.handle(h.now, Event::WsOpened { socket });
    let out: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::WsSend {socket:s,..} if *s==socket))
    );
    let (attempt, work) = out
        .into_iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } if work.context.method == "WS" => {
                Some((attempt, work))
            }
            _ => None,
        })
        .unwrap();
    parked.store(false, Ordering::SeqCst);
    let result = h.engine.signer().unwrap().sign_ready(&work).unwrap();
    parked.store(true, Ordering::SeqCst);
    h.engine
        .handle(h.now, Event::WsClosed { socket, code: 1006 });
    let _: Vec<_> = h.engine.drain_actions().collect();
    h.engine.handle(h.now, Event::Signed { attempt, result });
    assert!(
        !h.engine
            .drain_actions()
            .any(|a| matches!(a,Action::WsSend {socket:s,..} if s==socket))
    );
}
#[test]
fn private_login_work_is_the_declared_three_part_seconds_suffix() {
    let parked = Arc::new(AtomicBool::new(false));
    let mut h = configured(parked.clone());
    h.load_public();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.orders(OPEN_ORDER);
    let socket = h
        .sockets
        .iter()
        .find(|(_, url)| url.ends_with("/private"))
        .unwrap()
        .0;
    parked.store(true, Ordering::SeqCst);
    h.engine.handle(h.now, Event::WsOpened { socket });
    let work = h
        .engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Sign { work, .. } if work.context.method == "WS" => Some(work),
            _ => None,
        })
        .unwrap();
    let seconds = (h.now / 1000.).floor();
    assert_eq!(work.receipt.stamp_ms, Some(seconds * 1000.));
    let safe_cex_engine::signing::Operation::Sign { payload, .. } = &work.operations[1] else {
        panic!("login signs its second operation");
    };
    assert!(
        payload
            == &[
                (seconds as u64).to_string(),
                "GET".into(),
                "/user/verify".into()
            ]
            .map(
                |part: String| safe_cex_engine::signing::PayloadPart::Bytes {
                    bytes: part.into_bytes()
                }
            ),
        "typed WS grammar distinguishes login from an opaque HTTP capture"
    );
    parked.store(false, Ordering::SeqCst);
    let result = h
        .engine
        .signer()
        .unwrap()
        .sign_ready(&work)
        .unwrap()
        .unwrap();
    assert_eq!(
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Base64),
        safe_cex_crypto::hmac_sha256(
            b"material",
            format!("{}GET/user/verify", seconds as u64).as_bytes(),
            safe_cex_crypto::Encoding::Base64
        )
    );
}
