//! Original native-builder custody assertions, before any signer source cutover.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::signing::{
    Algorithm, Capability, Context as SignContext, Identities, Identity, KeyRef, Operation,
    PayloadPart, SignError, SignFuture, SignReceipt, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, ReadCredentials, Response,
};
use safe_cex_venue_bitrue::Bitrue;
mod support;
use std::sync::{Arc, Mutex};
use support::Options;

const NOW: f64 = 1234.9;
const SYMBOL: &str = "E-BTC-USDT";
const BRACKET: &str = "/fapi/v2/leverageBracket?contractName=E-BTC-USDT";
const LISTEN: &str = "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey";
const CONTRACT: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;

struct RecordingSigner {
    inner: LocalSigner,
    requests: Arc<Mutex<Vec<SignRequest>>>,
}
impl Signer for RecordingSigner {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.sign(request)
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.requests.lock().unwrap().push(request.clone());
        self.inner.sign_ready(request)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
struct Harness {
    engine: Engine<Bitrue>,
    requests: Arc<Mutex<Vec<SignRequest>>>,
    provider: Arc<RecordingSigner>,
    identities: Identities,
}
fn options() -> Options {
    Options {
        key: Some("fixture-primary-key".into()),
        secret: Some("fixture-primary-secret".into()),
        read_credentials: Some(ReadCredentials {
            key: "fixture-read-key".into(),
            secret: "fixture-read-secret".into(),
            passphrase: Some("unused-bitrue-passphrase".into()),
        }),
        ..Options::default()
    }
}
fn fixture(options: Options) -> Harness {
    let primary = options
        .key
        .as_ref()
        .zip(options.secret.as_ref())
        .map(|(key, secret)| LocalIdentity {
            key: Secret::new(key.clone()),
            secret: Secret::new(secret.clone()),
            ..Default::default()
        });
    let secondary = options.read_credentials.as_ref().map(|read| LocalIdentity {
        key: Secret::new(read.key.clone()),
        secret: Secret::new(read.secret.clone()),
        ..Default::default()
    });
    let (inner, identities) = LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let provider = Arc::new(RecordingSigner {
        inner,
        requests: requests.clone(),
    });
    let mut engine =
        safe_cex_engine::test_support::seeded_engine(Bitrue::new(options.native(&identities)));
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.set_identities(identities.clone()).unwrap();
    Harness {
        engine,
        requests,
        provider,
        identities,
    }
}
fn seed_market(h: &mut Harness) {
    h.engine.store_mut().set_markets(vec![Market {
        id: SYMBOL.into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: Some(0.001),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
}
fn actions(h: &mut Harness, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(&mut h.engine, now)
}
fn take_http(actions: &[Action], method: Method, suffix: &str) -> HttpRequest {
    let found: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.method == method && request.url.ends_with(suffix) => {
                Some(request)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one genuine native request {method:?} {suffix}: {actions:?}"
    );
    found[0].clone()
}
fn response(
    h: &mut Harness,
    now: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    h.engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    actions(h, now)
}
/// The routing intent a managed request for `path` declares at preparation;
/// managed requests carry their route there, not in `HttpRequest::route`.
fn prepared_intent(
    h: &mut Harness,
    now: f64,
    path: &str,
) -> Option<safe_cex_engine::request_context::RoutingIntent> {
    h.engine.prepare_dispatch(now);
    h.engine.drain_actions().find_map(|action| match action {
        Action::ManagedHttp(managed) => match *managed {
            safe_cex_engine::request_context::HttpAction::Prepare {
                intent, request, ..
            } if request.url.ends_with(path) => Some(intent),
            _ => None,
        },
        _ => None,
    })
}
fn assert_primary_catalogue(options: Options) {
    let mut probe = fixture(options);
    probe.engine.start(NOW);
    assert_eq!(
        prepared_intent(&mut probe, NOW, "/fapi/v1/contracts"),
        Some(safe_cex_engine::request_context::RoutingIntent::Primary)
    );
}
fn catalogue(h: &mut Harness) -> HttpRequest {
    h.engine.start(NOW);
    take_http(&actions(h, NOW), Method::Get, "/fapi/v1/contracts")
}
fn bracket(h: &mut Harness) -> HttpRequest {
    seed_market(h);
    h.engine
        .handle_command(
            NOW,
            Command::FetchMaxLeverage {
                command: 81,
                symbol: SYMBOL.into(),
            },
            Some(120_000),
        )
        .unwrap();
    take_http(&actions(h, NOW), Method::Get, BRACKET)
}
fn setting(h: &mut Harness) -> HttpRequest {
    seed_market(h);
    h.engine
        .handle_command(
            NOW,
            Command::SetLeverage {
                command: 82,
                symbol: SYMBOL.into(),
                leverage: 10.,
                side: None,
            },
            Some(120_000),
        )
        .unwrap();
    take_http(&actions(h, NOW), Method::Post, "/fapi/v2/level_edit")
}
fn listen(h: &mut Harness) -> HttpRequest {
    let request = catalogue(h);
    let startup = response(h, NOW, &request, 200, CONTRACT);
    take_http(&startup, Method::Post, LISTEN)
}
fn keepalive(h: &mut Harness) -> HttpRequest {
    let request = listen(h);
    let result = response(
        h,
        NOW,
        &request,
        200,
        r#"{"code":200,"data":{"listenKey":"fixture-token_1"}}"#,
    );
    assert!(result.iter().any(|action| matches!(action,
        Action::WsConnect { url, channel:Some(safe_cex_engine::SocketChannel::Private), .. }
        if url == "wss://fapiws.bitrue.com/stream?listenKey=fixture-token_1")));
    let (timer, at) = result
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 1_800_000. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("native 30 minute listen-key keepalive");
    h.engine.handle(at, Event::TimerFired { timer });
    take_http(&actions(h, at), Method::Put, "/listenKey/fixture-token_1")
}
fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    &request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}
fn batch(h: &Harness, identity: &Identity, method: Method, canonical: &str, stamp: f64) {
    let requests = h.requests.lock().unwrap();
    let found: Vec<_> = requests
        .iter()
        .filter(|request| {
            request.context.method == method.as_str() && request.context.path == canonical
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "native signing must use the injected provider for {canonical}"
    );
    let request = found[0];
    assert_eq!(request.receipt.stamp_ms, Some(stamp));
    assert_eq!(
        request.operations.len(),
        2,
        "API-key presentation plus one atomic HMAC, no passphrase"
    );
    assert!(
        matches!(&request.operations[0], Operation::Present { key } if key == &identity.credential)
    );
    assert!(
        matches!(&request.operations[1], Operation::Sign { algorithm:Algorithm::HmacSha256, key, .. }
        if key == identity.signing_ref().unwrap())
    );
}

#[test]
fn original_catalogue_uses_primary_atomic_provider() {
    assert_primary_catalogue(options());
    let mut h = fixture(options());
    catalogue(&mut h);
    batch(
        &h,
        h.identities.primary.as_ref().unwrap(),
        Method::Get,
        "/fapi/v1/contracts",
        1234.,
    );
}
#[test]
fn original_bracket_read_uses_secondary_atomic_provider() {
    let mut h = fixture(options());
    let request = bracket(&mut h);
    assert_eq!(header(&request, "X-CH-APIKEY"), "fixture-read-key");
    batch(
        &h,
        h.identities.secondary.as_ref().unwrap(),
        Method::Get,
        BRACKET,
        1234.,
    );
}
#[test]
fn original_setting_post_uses_primary_atomic_provider() {
    let mut h = fixture(options());
    let request = setting(&mut h);
    assert_eq!(
        request.body.as_deref(),
        Some(r#"{"contractName":"E-BTC-USDT","leverage":10.0}"#)
    );
    batch(
        &h,
        h.identities.primary.as_ref().unwrap(),
        Method::Post,
        "/fapi/v2/level_edit",
        1234.,
    );
}
#[test]
fn original_absolute_listen_post_uses_primary_atomic_provider() {
    let mut h = fixture(options());
    let request = listen(&mut h);
    assert_eq!(request.url, LISTEN);
    batch(
        &h,
        h.identities.primary.as_ref().unwrap(),
        Method::Post,
        "/user_stream/api/v1/listenKey",
        1234.,
    );
}
#[test]
fn original_absolute_keepalive_put_uses_primary_atomic_provider() {
    let mut h = fixture(options());
    let request = keepalive(&mut h);
    assert_eq!(request.url, format!("{LISTEN}/fixture-token_1"));
    batch(
        &h,
        h.identities.primary.as_ref().unwrap(),
        Method::Put,
        "/user_stream/api/v1/listenKey/fixture-token_1",
        1_801_234.,
    );
}
#[test]
fn original_fixed_clock_signature_resend_uses_a_new_atomic_receipt() {
    let mut h = fixture(options());
    let first = bracket(&mut h);
    let result = response(
        &mut h,
        NOW,
        &first,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    let second = take_http(&result, Method::Get, BRACKET);
    assert_eq!(header(&second, "X-CH-TS"), "1235");
    let requests = h.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "each eligible signature resend requires fresh provider preparation"
    );
    assert_eq!(requests[0].receipt.stamp_ms, Some(1234.));
    assert_eq!(requests[1].receipt.stamp_ms, Some(1235.));
    assert!(requests[0].operations[1] != requests[1].operations[1]);
}
fn assert_unknown_setting_failure(h: &Harness, result: &[Action], provider_posts: usize) {
    assert!(!result.iter().any(|action| matches!(action,
        Action::Http(request) if request.method == Method::Post && request.url.ends_with("/fapi/v2/level_edit")
    )), "HTTP500 must not replay the unknown setting write; native reconciliation GETs remain allowed");
    let sequence: Vec<_> = result
        .iter()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 82, error }) => {
                assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
                assert_eq!(
                    error.message,
                    // A 5xx write is the engine's unproven seal; no body is decoded.
                    "HTTP response does not prove the write outcome"
                );
                Some("failure")
            }
            Action::Notice(safe_cex_engine::Notice::CommandError { command: 82, .. }) => {
                Some("unexpected diagnostic")
            }
            Action::Notice(safe_cex_engine::Notice::CommandDone { command: 82, .. }) => {
                Some("unexpected success")
            }
            _ => None,
        })
        .collect();
    assert_eq!(sequence, ["failure"]);
    assert_eq!(h.engine.store().setting(SYMBOL).unwrap().leverage, 5.);
    assert!(h.engine.store().orders().is_empty());
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(
        h.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.context.method == "POST"
                && request.context.path == "/fapi/v2/level_edit")
            .count(),
        provider_posts
    );
}
fn provider_setting_posts(h: &Harness) -> usize {
    h.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| {
            request.context.method == "POST" && request.context.path == "/fapi/v2/level_edit"
        })
        .count()
}
#[test]
fn original_unknown_500_write_code_cannot_authorize_signature_resend() {
    let mut h = fixture(options());
    let first = setting(&mut h);
    h.engine
        .store_mut()
        .update_setting(SYMBOL, |setting| setting.leverage = 5.);
    let count = provider_setting_posts(&h);
    let result = response(
        &mut h,
        NOW + 10.,
        &first,
        500,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    assert_unknown_setting_failure(&h, &result, count);
}
#[test]
fn native_500_after_spent_signature_budget_has_one_unknown_terminal() {
    let mut h = fixture(options());
    let first = setting(&mut h);
    h.engine
        .store_mut()
        .update_setting(SYMBOL, |setting| setting.leverage = 5.);
    let resent = response(
        &mut h,
        NOW,
        &first,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    let second = take_http(&resent, Method::Post, "/fapi/v2/level_edit");
    let count = provider_setting_posts(&h);
    let result = response(
        &mut h,
        NOW + 10.,
        &second,
        500,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    assert_unknown_setting_failure(&h, &result, count);
}

#[test]
fn native_wire_matches_independent_hex_vectors_and_ordered_header_bytes() {
    let mut h = fixture(options());
    let request = catalogue(&mut h);
    // Independent Python hmac/hashlib vectors, UTF-8 keys and exact preimages.
    assert_eq!(
        request.headers,
        vec![
            ("Content-Type", "application/json".into()),
            ("X-Referer", "Tealstreet".into()),
            ("X-CH-APIKEY", "fixture-primary-key".into()),
            ("X-CH-TS", "1234".into()),
            ("recvWindow", "20000".into()),
            (
                "X-CH-SIGN",
                "ea3ed60ddfc11d27aa87ef6e7129f43b38ff0c9d9cb8dc08670fad5b968e78b9".into()
            ),
        ]
    );
    let mut h = fixture(options());
    let request = bracket(&mut h);
    assert_eq!(
        header(&request, "X-CH-SIGN"),
        "18a9cec93a1106866bfbc0fbe439c2b0905a159678a2d61dfd3c839dfc7b3806"
    );
    let mut h = fixture(options());
    let request = setting(&mut h);
    assert_eq!(
        header(&request, "X-CH-SIGN"),
        "4cd171fc46d027f73ed9ccf756b724d1f41922deb156e15ff8ea37788e4959ef"
    );
    let mut h = fixture(options());
    let request = listen(&mut h);
    assert_eq!(
        header(&request, "X-CH-SIGN"),
        "0d80e5da969a9095c6f8d066c73f1b39b73eaa2d54b44853475828c2edc7596b"
    );
    let mut h = fixture(options());
    let request = keepalive(&mut h);
    assert_eq!(header(&request, "X-CH-APIKEY"), "fixture-primary-key");
    assert_eq!(header(&request, "X-CH-TS"), "1801234");
    assert_eq!(request.body, None);
    assert_eq!(
        header(&request, "X-CH-SIGN"),
        "a2708669d9046e55fc53e49882f4ee4af231746aa34e830c0fd7243847d694b5"
    );
}
#[test]
fn recording_provider_independently_signs_the_expected_atomic_work() {
    let h = fixture(options());
    let primary = h.identities.primary.as_ref().unwrap();
    let request = safe_cex_engine::signing::hmac_sha256_work(
        &primary.credential,
        primary.signing_ref().unwrap(),
        vec![PayloadPart::Bytes {
            bytes: b"1234GET/fapi/v1/contracts".to_vec(),
        }],
        SignContext {
            venue: "bitrue".into(),
            method: "GET".into(),
            host: "fapi.bitrue.com".into(),
            path: "/fapi/v1/contracts".into(),
        },
        SignReceipt {
            nonce: None,
            stamp_ms: Some(1234.),
            expiry_ms: None,
        },
    );
    let result = h.provider.sign_ready(&request).unwrap().unwrap();
    assert_eq!(result.values.len(), 2);
    assert_eq!(result.values[0], b"fixture-primary-key");
    assert_eq!(
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex),
        "ea3ed60ddfc11d27aa87ef6e7129f43b38ff0c9d9cb8dc08670fad5b968e78b9"
    );
    assert_eq!(h.requests.lock().unwrap().len(), 1);
}
#[test]
fn native_recv_window_remains_header_only() {
    let mut a = fixture(options());
    let mut changed = options();
    changed.recv_window = 60_000;
    let mut b = fixture(changed);
    let first = catalogue(&mut a);
    let second = catalogue(&mut b);
    assert_eq!(header(&first, "X-CH-SIGN"), header(&second, "X-CH-SIGN"));
    assert_eq!(header(&first, "recvWindow"), "20000");
    assert_eq!(header(&second, "recvWindow"), "60000");
}
#[test]
fn native_keyless_catalogue_keeps_primary_route_and_static_headers() {
    assert_primary_catalogue(Options::default());
    let mut h = fixture(Options::default());
    let request = catalogue(&mut h);
    assert_eq!(
        request.headers,
        vec![
            ("Content-Type", "application/json".into()),
            ("X-Referer", "Tealstreet".into())
        ]
    );
    assert!(h.requests.lock().unwrap().is_empty());
}
#[test]
fn native_keyed_public_only_catalogue_still_signs_primary() {
    let mut opts = options();
    opts.public_data_only = true;
    let mut h = fixture(opts);
    let request = catalogue(&mut h);
    assert_eq!(header(&request, "X-CH-APIKEY"), "fixture-primary-key");
    assert_eq!(
        header(&request, "X-CH-SIGN"),
        "ea3ed60ddfc11d27aa87ef6e7129f43b38ff0c9d9cb8dc08670fad5b968e78b9"
    );
    let after = response(&mut h, NOW, &request, 200, CONTRACT);
    assert!(!after.iter().any(|action| matches!(action, Action::Http(_))));
}
#[test]
fn native_read_key_without_primary_does_not_sign_catalogue_or_start_private() {
    let mut opts = options();
    opts.key = None;
    opts.secret = None;
    let mut h = fixture(opts);
    let request = catalogue(&mut h);
    assert!(
        !request
            .headers
            .iter()
            .any(|(name, _)| name.starts_with("X-CH-"))
    );
    let after = response(&mut h, NOW, &request, 200, CONTRACT);
    assert!(!after.iter().any(|action| matches!(action, Action::Http(_))));
    assert!(h.requests.lock().unwrap().is_empty());
}
#[test]
fn native_invalid_read_key_refuses_before_public_only_startup_io() {
    let mut opts = options();
    opts.public_data_only = true;
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = support::engine(opts);
    engine.start(NOW);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::WsConnect { .. }))
    );
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level, code, message, .. }))
        if *level == safe_cex_engine::Level::Error
            && *code == safe_cex_engine::log::code::CONFIG_INVALID
            && message == "Secondary read credentials require a nonempty key and secret"
    )));
}
#[test]
fn native_fixed_and_backward_clock_resends_increment_once() {
    for clock in [NOW, NOW - 10.] {
        let mut h = fixture(options());
        let first = bracket(&mut h);
        let next = response(
            &mut h,
            clock,
            &first,
            400,
            r#"{"code":"-1022","msg":"Invalid signature"}"#,
        );
        let second = take_http(&next, Method::Get, BRACKET);
        assert_eq!(header(&first, "X-CH-TS"), "1234");
        assert_eq!(header(&second, "X-CH-TS"), "1235");
        assert_ne!(header(&first, "X-CH-SIGN"), header(&second, "X-CH-SIGN"));
        let final_actions = response(
            &mut h,
            clock,
            &second,
            400,
            r#"{"code":"-1022","msg":"Invalid signature"}"#,
        );
        assert!(
            !final_actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}
#[test]
fn native_signature_and_transport_retry_budgets_stay_independent() {
    let mut h = fixture(options());
    let first = bracket(&mut h);
    let result = response(
        &mut h,
        NOW,
        &first,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    let mut current = take_http(&result, Method::Get, BRACKET);
    for attempt in 1..=3 {
        h.engine.handle(
            NOW + f64::from(attempt),
            Event::HttpFailed {
                request: current.id,
                error: "fixture network failure",
            },
        );
        current = take_http(
            &actions(&mut h, NOW + f64::from(attempt)),
            Method::Get,
            BRACKET,
        );
    }
    h.engine.handle(
        NOW + 4.,
        Event::HttpFailed {
            request: current.id,
            error: "fixture network failure",
        },
    );
    assert!(
        !actions(&mut h, NOW + 4.)
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}
#[test]
fn native_catalogue_network_failure_uses_backoff_instead_of_immediate_retry() {
    let mut h = fixture(options());
    let request = catalogue(&mut h);
    h.engine.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "fixture network failure",
        },
    );
    let result = actions(&mut h, NOW);
    assert!(
        !result
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(
        result
            .iter()
            .any(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms > NOW))
    );
}
fn fills(h: &mut Harness) -> HttpRequest {
    seed_market(h);
    h.engine
        .handle_command(
            NOW,
            Command::Query {
                command: 83,
                query: safe_cex_engine::Query::Fills {
                    symbol: Some(SYMBOL.into()),
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            },
            Some(120_000),
        )
        .unwrap();
    take_http(
        &actions(h, NOW),
        Method::Get,
        "/fapi/v2/myTrades?contractName=E-BTC-USDT&limit=200",
    )
}
#[test]
fn native_streamed_empty_fills_is_a_working_codec_control() {
    let mut h = fixture(options());
    let request = fills(&mut h);
    let result = response(&mut h, NOW, &request, 200, r#"{"code":0,"data":[]}"#);
    let terminals: Vec<_> = result
        .iter()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 83,
                result,
            }) => Some(result.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(terminals, ["[]"]);
    assert!(!result.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 83, .. })
    )));
}
#[test]
fn native_real_stream_codec_failures_never_enter_transport_retry() {
    for bytes in [br#"{"code":0,"data":{}}"#.as_slice(), &[0xff, 0xfe]] {
        let mut h = fixture(options());
        let request = fills(&mut h);
        h.engine.handle(
            NOW,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        h.engine.handle(
            NOW,
            Event::HttpChunk {
                request: request.id,
                data: bytes,
            },
        );
        h.engine.handle(
            NOW,
            Event::HttpEnd {
                request: request.id,
            },
        );
        let result = actions(&mut h, NOW);
        assert!(
            !result
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert_eq!(
            result
                .iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 83, .. })
                ))
                .count(),
            1
        );
        assert!(!result.iter().any(|action| matches!(
            action,
            Action::Notice(safe_cex_engine::Notice::CommandDone { command: 83, .. })
        )));
    }
}
#[test]
fn native_transport_reissues_increment_stamp_at_a_fixed_clock() {
    let mut h = fixture(options());
    let mut current = bracket(&mut h);
    for expected in 1235..=1237 {
        h.engine.handle(
            NOW,
            Event::HttpFailed {
                request: current.id,
                error: "fixture network failure",
            },
        );
        current = take_http(&actions(&mut h, NOW), Method::Get, BRACKET);
        assert_eq!(header(&current, "X-CH-TS"), expected.to_string());
    }
    h.engine.handle(
        NOW,
        Event::HttpFailed {
            request: current.id,
            error: "fixture network failure",
        },
    );
    let result = actions(&mut h, NOW);
    assert!(
        !result
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(
        result
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 81, .. })
            ))
            .count(),
        1
    );
}
#[test]
fn native_keyless_signature_refusal_gets_one_unsigned_primary_reissue() {
    let mut h = fixture(Options::default());
    let first = catalogue(&mut h);
    let result = response(
        &mut h,
        NOW,
        &first,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    let second = take_http(&result, Method::Get, "/fapi/v1/contracts");
    // The reissue is again a managed read on the Primary route.
    let mut probe = fixture(Options::default());
    let refused = catalogue(&mut probe);
    probe.engine.handle(
        NOW,
        Event::HttpResponse {
            request: refused.id,
            response: Response {
                status: 400,
                headers: "",
                body: r#"{"code":"-1022","msg":"Invalid signature"}"#,
            },
        },
    );
    assert_eq!(
        prepared_intent(&mut probe, NOW, "/fapi/v1/contracts"),
        Some(safe_cex_engine::request_context::RoutingIntent::Primary)
    );
    assert_eq!(second.headers, first.headers);
    assert!(h.requests.lock().unwrap().is_empty());
    let result = response(
        &mut h,
        NOW,
        &second,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    assert!(
        !result
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn original_transport_retries_prepare_fresh_atomic_receipts() {
    let mut h = fixture(options());
    let mut current = bracket(&mut h);
    for _ in 0..3 {
        h.engine.handle(
            NOW,
            Event::HttpFailed {
                request: current.id,
                error: "fixture network failure",
            },
        );
        current = take_http(&actions(&mut h, NOW), Method::Get, BRACKET);
    }
    let requests = h.requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        4,
        "transport retries must re-enter the provider with their own released stamp floor"
    );
    for (request, stamp) in requests.iter().zip([1234., 1235., 1236., 1237.]) {
        assert_eq!(request.receipt.stamp_ms, Some(stamp));
        assert_eq!(request.operations.len(), 2);
        assert!(matches!(&request.operations[0], Operation::Present { key }
            if key == &h.identities.secondary.as_ref().unwrap().credential));
    }
}

fn network_retry(h: &mut Harness, request: &HttpRequest) -> HttpRequest {
    h.engine.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "fixture network failure",
        },
    );
    take_http(&actions(h, NOW), Method::Get, BRACKET)
}
fn assert_failed_bracket_without_retry(result: &[Action]) {
    assert!(
        !result
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(
        result
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 81, .. })
            ))
            .count(),
        1
    );
    assert!(!result.iter().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::CommandDone { command: 81, .. })
    )));
}
#[test]
fn native_signature_retry_cannot_reset_an_already_spent_transport_attempt() {
    let mut h = fixture(options());
    let first = bracket(&mut h);
    let second = network_retry(&mut h, &first);
    let result = response(
        &mut h,
        NOW,
        &second,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    let third = take_http(&result, Method::Get, BRACKET);
    let fourth = network_retry(&mut h, &third);
    let fifth = network_retry(&mut h, &fourth);
    for (request, stamp) in [&first, &second, &third, &fourth, &fifth]
        .into_iter()
        .zip(1234..=1238)
    {
        assert_eq!(header(request, "X-CH-TS"), stamp.to_string());
    }
    h.engine.handle(
        NOW,
        Event::HttpFailed {
            request: fifth.id,
            error: "fixture network failure",
        },
    );
    assert_failed_bracket_without_retry(&actions(&mut h, NOW));
}
#[test]
fn native_transport_retry_cannot_renew_a_spent_signature_resend() {
    let mut h = fixture(options());
    let first = bracket(&mut h);
    let result = response(
        &mut h,
        NOW,
        &first,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    let second = take_http(&result, Method::Get, BRACKET);
    let third = network_retry(&mut h, &second);
    let result = response(
        &mut h,
        NOW,
        &third,
        400,
        r#"{"code":"-1022","msg":"Invalid signature"}"#,
    );
    assert_failed_bracket_without_retry(&result);
}

#[test]
fn bracket_fresh_leg_near_physical_deadline_keeps_original_caller_wait() {
    let mut h = fixture(options());
    let first = bracket(&mut h);
    let later = NOW + 19_999.;
    let second = take_http(
        &response(
            &mut h,
            later,
            &first,
            400,
            r#"{"code":-1022,"msg":"signature"}"#,
        ),
        Method::Get,
        BRACKET,
    );
    let result = response(
        &mut h,
        later + 19_999.,
        &second,
        200,
        r#"{"code":0,"data":{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":50}]}}"#,
    );
    assert!(result.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandDone { command:81, result }) if result=="50")));
    assert_eq!(
        h.engine.store().market(SYMBOL).unwrap().limits.leverage.max,
        50.
    );
    assert_eq!(h.requests.lock().unwrap().len(), 2);
}

#[test]
fn bracket_caller_expiry_discards_late_result_without_store_projection() {
    let mut h = fixture(options());
    let first = bracket(&mut h);
    let result = response(
        &mut h,
        NOW + 120_000.,
        &first,
        200,
        r#"{"code":0,"data":{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":50}]}}"#,
    );
    let failures: Vec<_> = result
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 81, error }) => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(safe_cex_engine::ErrorKind::Timeout));
    assert_eq!(failures[0].outcome, None);
    assert_eq!(
        h.engine.store().market(SYMBOL).unwrap().limits.leverage.max,
        100.
    );
    assert!(!result.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(safe_cex_engine::Notice::CommandDone { command: 81, .. })
    )));
}

struct AlterProvider {
    inner: Arc<RecordingSigner>,
    alter: u8,
}
impl Signer for AlterProvider {
    fn immediate(&self) -> bool {
        true
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(self.sign_ready(&request).unwrap()))
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        let result = self
            .inner
            .sign_ready(request)
            .unwrap()
            .and_then(|mut result| {
                match self.alter {
                    0 => return Err(SignError::Refused),
                    1 => {
                        result.values.pop();
                    }
                    2 => {
                        result.receipt.stamp_ms = Some(77.);
                    }
                    3 => {
                        result.headers.push(("X-CH-SIGN".into(), "injected".into()));
                    }
                    4 => {
                        result.values[0] = b"key\r\nInjected: value".to_vec();
                    }
                    5 => {
                        result.values[1].pop();
                    }
                    6 => {
                        if request.receipt.stamp_ms.is_some_and(|stamp| stamp > 1234.) {
                            return Err(SignError::Refused);
                        }
                    }
                    _ => unreachable!(),
                }
                Ok(result)
            });
        Some(result)
    }
}
#[test]
fn bracket_provider_refusal_and_invalid_atomic_results_release_nothing() {
    for alter in 0..6 {
        let mut h = fixture(options());
        h.engine
            .set_signer(Some(Arc::new(AlterProvider {
                inner: h.provider.clone(),
                alter,
            })))
            .unwrap();
        seed_market(&mut h);
        h.engine
            .handle_command(
                NOW,
                Command::FetchMaxLeverage {
                    command: 81,
                    symbol: SYMBOL.into(),
                },
                Some(120_000),
            )
            .unwrap();
        let result = actions(&mut h, NOW);
        assert!(!result.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(safe_cex_engine::Notice::CommandDone { .. })
        )));
        let failures: Vec<_> = result
            .iter()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 81, error }) => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "{result:?}");
        assert_eq!(failures[0].outcome, None);
        assert_eq!(h.requests.lock().unwrap().len(), 1);
        assert_eq!(
            h.engine.store().market(SYMBOL).unwrap().limits.leverage.max,
            100.
        );
    }
}

struct ParkedProvider(Arc<RecordingSigner>);
impl Signer for ParkedProvider {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.0.sign(request)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}
/// `prepared_actions` refuses parked signing, so this drive keeps the
/// prepared route across the Sign/Signed hand-off and releases on Bind.
fn staged_actions(h: &mut Harness, now: f64) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    h.engine.prepare_dispatch(now);
    let mut queue: std::collections::VecDeque<_> = h.engine.drain_actions().collect();
    let mut released = Vec::new();
    while let Some(action) = queue.pop_front() {
        let Action::ManagedHttp(action) = action else {
            released.push(action);
            continue;
        };
        match *action {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } if h.engine.http_preparation_pending(preparation) => {
                let prepared = Prepared {
                    id: request.id.0,
                    expires_at: None,
                };
                h.engine.http_prepared(now, preparation, prepared);
            }
            HttpAction::Bind {
                permit, request, ..
            } => {
                if let Ok(Claim::Released(_)) = h.engine.claim_http(now, permit) {
                    released.push(Action::Http(request));
                }
            }
            _ => {}
        }
        for action in h
            .engine
            .drain_actions()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            queue.push_front(action);
        }
    }
    released
}
#[test]
fn staged_catalogue_waits_for_atomic_async_provider_and_never_replays_partial_headers() {
    for refuse in [false, true] {
        let mut h = fixture(options());
        h.engine
            .set_signer(Some(Arc::new(ParkedProvider(h.provider.clone()))))
            .unwrap();
        h.engine.start(NOW);
        let staged = staged_actions(&mut h, NOW);
        assert!(!staged.iter().any(|a| matches!(a, Action::Http(_))));
        let work: Vec<_> = staged
            .into_iter()
            .filter_map(|a| match a {
                Action::Sign { attempt, work, .. } => Some((attempt, work)),
                _ => None,
            })
            .collect();
        assert_eq!(work.len(), 1);
        let (attempt, work) = work.into_iter().next().unwrap();
        let result = if refuse {
            Err(SignError::Refused)
        } else {
            h.provider.sign_ready(&work).unwrap()
        };
        h.engine.handle(NOW, Event::Signed { attempt, result });
        let released = staged_actions(&mut h, NOW);
        if refuse {
            assert!(!released.iter().any(|a| matches!(a, Action::Http(_))));
        } else {
            let request = take_http(&released, Method::Get, "/fapi/v1/contracts");
            assert_eq!(header(&request, "X-CH-APIKEY"), "fixture-primary-key");
            assert_eq!(header(&request, "X-CH-TS"), "1234");
            assert_eq!(request.headers.len(), 6);
        }
    }
}

#[test]
fn setting_provider_failure_is_not_sent_only_before_any_attempt_release() {
    for (alter, expected) in [
        (0, safe_cex_engine::Outcome::NotSent),
        (6, safe_cex_engine::Outcome::Unknown),
    ] {
        let mut h = fixture(options());
        h.engine
            .set_signer(Some(Arc::new(AlterProvider {
                inner: h.provider.clone(),
                alter,
            })))
            .unwrap();
        seed_market(&mut h);
        h.engine
            .handle_command(
                NOW,
                Command::SetLeverage {
                    command: 82,
                    symbol: SYMBOL.into(),
                    leverage: 10.,
                    side: None,
                },
                Some(120_000),
            )
            .unwrap();
        let mut result = actions(&mut h, NOW);
        if alter == 6 {
            let first = take_http(&result, Method::Post, "/fapi/v2/level_edit");
            result = response(
                &mut h,
                NOW,
                &first,
                400,
                r#"{"code":-1022,"msg":"signature"}"#,
            );
        }
        assert!(
            !result
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.method==Method::Post))
        );
        let failures: Vec<_> = result
            .iter()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 82, error }) => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "{result:?}");
        assert_eq!(failures[0].outcome, Some(expected));
    }
}

#[test]
fn malformed_signature_code_cannot_replay_a_mutating_post() {
    for token in [
        r#""\"-1022\"""#,
        r#""-01022""#,
        r#""-001022""#,
        r#"" -1022""#,
        r#""-1022 ""#,
        r#""-1022.0""#,
        r#""-1.022e3""#,
        "-1022.0",
        "-1.022e3",
    ] {
        let mut h = fixture(options());
        let first = setting(&mut h);
        let result = response(
            &mut h,
            NOW,
            &first,
            400,
            &format!(r#"{{"code":{token},"msg":"malformed native code"}}"#),
        );
        assert!(
            !result
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.method==Method::Post)),
            "noncanonical {token} replayed a write: {result:?}"
        );
        assert_eq!(provider_setting_posts(&h), 1);
        assert_eq!(
            result
                .iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 82, .. })
                ))
                .count(),
            1
        );
    }
}
#[test]
fn malformed_signature_code_cannot_reissue_a_streamed_read() {
    for token in [
        r#""\"-1022\"""#,
        r#""-01022""#,
        r#""-001022""#,
        r#""-1022.0""#,
        "-1022.0",
        "-1.022e3",
    ] {
        let mut h = fixture(options());
        let first = fills(&mut h);
        let result = response(
            &mut h,
            NOW,
            &first,
            400,
            &format!(r#"{{"code":{token},"msg":"malformed native code"}}"#),
        );
        assert!(
            !result.iter().any(|a| matches!(a, Action::Http(_))),
            "{token}: {result:?}"
        );
        assert_eq!(
            result
                .iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 83, .. })
                ))
                .count(),
            1
        );
    }
}
#[test]
fn canonical_numeric_and_string_signature_codes_reprepare_once() {
    for token in ["-1022", r#""-1022""#] {
        let mut h = fixture(options());
        let first = setting(&mut h);
        let result = response(
            &mut h,
            NOW,
            &first,
            400,
            &format!(r#"{{"code":{token},"msg":"signature"}}"#),
        );
        let second = take_http(&result, Method::Post, "/fapi/v2/level_edit");
        assert_eq!(header(&second, "X-CH-TS"), "1235");
        assert_eq!(provider_setting_posts(&h), 2);
    }
}
#[test]
fn refused_inspect_resign_does_not_invent_an_unknown_write() {
    use safe_cex_engine::model::{
        Order, OrderExtra, OrderSide, OrderStatus, OrderType, TimeInForce,
    };
    let mut h = fixture(options());
    h.engine
        .set_signer(Some(Arc::new(AlterProvider {
            inner: h.provider.clone(),
            alter: 6,
        })))
        .unwrap();
    seed_market(&mut h);
    let order = Order {
        id: "2334151155877155281".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: SYMBOL.into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 2000.,
        amount: 0.01,
        filled: 0.,
        remaining: 0.01,
        reduce_only: false,
        close: None,
        client_order_id: Some("callerOld".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: Some(OrderExtra {
            remaining_native: Some(0.01),
            filled_in_placement_units: Some(0.),
            ..Default::default()
        }),
    };
    h.engine.store_mut().upsert_order(order.clone());
    h.engine
        .handle_command(
            NOW,
            Command::AmendOrder {
                command: 84,
                id: order.id.clone(),
                price: Some(1990.),
                amount: None,
            },
            Some(120_000),
        )
        .unwrap();
    let first = actions(&mut h, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.method == Method::Get && r.url.contains("/fapi/v2/order?") => {
                Some(r)
            }
            _ => None,
        })
        .expect("genuine pre-write Inspect");
    let result = response(
        &mut h,
        NOW,
        &first,
        400,
        r#"{"code":-1022,"msg":"signature"}"#,
    );
    assert!(
        !result
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.method==Method::Post))
    );
    let failures: Vec<_> = result
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 84, error }) => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "{result:?}");
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert_eq!(h.engine.store().orders(), [order]);
}
