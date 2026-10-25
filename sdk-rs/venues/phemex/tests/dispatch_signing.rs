//! Original native signer controls and provider-custody REDs, before production.
mod support;
use support::Options;
#[path = "support/native_sign.rs"]
mod native_sign;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::signing::{
    Capability, Context, Identities, PayloadPart, Provider, SignError, SignFuture, SignReceipt,
    SignRequest, SignResult, Signer, hmac_sha256_work,
};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Query, ReadCredentials, Response, SocketChannel, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_phemex::{Pending, PhemexPerpetuals};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
const NOW: f64 = 1_700_000_000_000.;
type ReleaseLog =
    Arc<std::sync::Mutex<Vec<(safe_cex_engine::RequestId, safe_cex_engine::SigningRelease)>>>;
struct Probe {
    venue: PhemexPerpetuals,
    protocol: Option<f64>,
    releases: ReleaseLog,
}
impl Venue for Probe {
    type Pending = Pending;
    fn guard_pending_placements(&self) -> bool {
        self.venue.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.venue.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.venue.fence_uncertain_cancel_all()
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        self.venue.capture_http_reconciliation(pending)
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.venue.on_http_owner_retired(owner);
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        pending: Vec<Pending>,
    ) {
        self.venue.read_root_retired(owner, pending);
    }
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error);
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.venue.on_socket_event(cx, event)
    }
    fn socket_control(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.venue.socket_control(store, feed, authenticated, text)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        interests: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.venue
            .encode_socket_interests(store, feed, subscribe, interests, emit)
    }
    fn on_signing_release(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        release: safe_cex_engine::SigningRelease,
    ) {
        if let safe_cex_engine::SigningTarget::Http { request, pending } = &target
            && matches!(pending, Pending::Trade(_) | Pending::PlannedTrade(_))
        {
            self.releases.lock().unwrap().push((*request, release));
        }
        self.venue.on_signing_release(target, release);
    }
    fn on_signing_receipt(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        receipt: SignReceipt,
    ) {
        self.venue.on_signing_receipt(target, receipt);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command
            && symbols
                .first()
                .is_some_and(|s| s == "__protocol_observation__")
        {
            assert!(cx.observe_protocol_time(symbols[1].parse().unwrap()));
            return;
        }
        if let Some(protocol) = self.protocol.take() {
            assert!(cx.observe_protocol_time(protocol));
        }
        self.venue.on_command(cx, command);
    }
}
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"},{"symbol":"ETHUSDC","type":"PerpetualV2","settleCurrency":"USDC","baseCurrency":"ETH","quoteCurrency":"USDC","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"},{"symbol":"ETHUSDC","bidRp":"1999","askRp":"2001","lastRp":"2000","markRp":"2000","indexRp":"2000","openRp":"1900","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}"#;
fn primary(read: bool) -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        read_credentials: read.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    }
}
fn engine(options: Options, provider: Option<Provider>, protocol: Option<f64>) -> Engine<Probe> {
    observed_engine(options, provider, protocol).0
}
fn observed_engine(
    options: Options,
    provider: Option<Provider>,
    protocol: Option<f64>,
) -> (Engine<Probe>, ReleaseLog) {
    let releases = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: options.build().0,
        protocol,
        releases: releases.clone(),
    });
    e.set_signer(provider).unwrap();
    (e, releases)
}
fn local() -> (Provider, Identities) {
    let (signer, refs) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("key".into()),
            secret: Secret::new("secret".into()),
            ..Default::default()
        }),
        secondary: Some(LocalIdentity {
            key: Secret::new("read-key".into()),
            secret: Secret::new("read-secret".into()),
            ..Default::default()
        }),
    })
    .unwrap();
    (Arc::new(signer), refs)
}
struct Refused {
    calls: AtomicUsize,
}
impl Signer for Refused {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(async { Err(SignError::Refused) })
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Some(Err(SignError::Refused))
    }
    fn can_sign(
        &self,
        _: &safe_cex_engine::signing::KeyRef,
        _: safe_cex_engine::signing::Algorithm,
    ) -> Capability {
        Capability::Supported
    }
}
struct Parked {
    inner: Provider,
    paused: AtomicBool,
}
impl Signer for Parked {
    fn immediate(&self) -> bool {
        !self.paused.load(Ordering::SeqCst)
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        if self.paused.load(Ordering::SeqCst) {
            Box::pin(std::future::pending())
        } else {
            self.inner.sign(request)
        }
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.paused.load(Ordering::SeqCst) {
            None
        } else {
            self.inner.sign_ready(request)
        }
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
fn parked(paused: bool) -> (Arc<Parked>, Provider) {
    let (local, _) = local();
    (
        Arc::new(Parked {
            inner: local.clone(),
            paused: AtomicBool::new(paused),
        }),
        local,
    )
}
fn actions(e: &mut Engine<Probe>, now: f64) -> Vec<Action> {
    e.prepare_dispatch(now);
    let initial = e.drain_actions().collect();
    drive_routes(e, now, initial)
}
fn drive_routes(e: &mut Engine<Probe>, now: f64, initial: Vec<Action>) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    let mut queue = std::collections::VecDeque::from(initial);
    let mut out = Vec::new();
    let mut count = 0;
    while let Some(action) = queue.pop_front() {
        count += 1;
        assert!(count < 65_536);
        if let Action::ManagedHttp(action) = action {
            if matches!(action.as_ref(), HttpAction::Prepare { .. }) {
                out.push(Action::ManagedHttp(action.clone()));
            }
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert_eq!(request.id, preparation.attempt().request);
                    if e.http_preparation_pending(preparation) {
                        e.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                }
                HttpAction::Bind {
                    permit, request, ..
                } => match e.claim_http(now, permit) {
                    Ok(Claim::Released(_)) => out.push(Action::Http(request)),
                    Ok(Claim::WaitUntil(_)) => {
                        panic!("native immediate route cannot wait for pacing")
                    }
                    Err(_) => {}
                },
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => {
                    panic!("single-request fixture cannot release a request group")
                }
            }
        } else {
            out.push(action);
        }
        for action in e.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    out
}
fn physical(actions: &[Action], path: &str) -> HttpRequest {
    let mut requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "one exact native request for {path}");
    requests.remove(0)
}
fn ask(e: &mut Engine<Probe>, now: f64) -> Vec<Action> {
    e.handle(now, Event::Command(Command::FetchAccount { command: 700 }));
    actions(e, now)
}
fn reply(e: &mut Engine<Probe>, now: f64, request: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
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
    actions(e, now)
}
fn token(request: &HttpRequest) -> &str {
    &request
        .headers
        .iter()
        .find(|(name, _)| *name == "X-PHEMEX-ACCESS-TOKEN")
        .unwrap()
        .1
}
fn bootstrap(read: bool, provider: Option<Provider>) -> (Engine<Probe>, SocketId) {
    let (e, socket, _) = bootstrap_observed(read, provider, None);
    (e, socket)
}
fn bootstrap_observed(
    read: bool,
    provider: Option<Provider>,
    protocol: Option<f64>,
) -> (Engine<Probe>, SocketId, ReleaseLog) {
    let (mut e, releases) = observed_engine(
        primary(read),
        provider.or_else(|| Some(local().0)),
        protocol,
    );
    e.start(NOW);
    let products = physical(&actions(&mut e, NOW), "/public/products");
    let mut output = reply(&mut e, NOW, &products, PRODUCTS);
    let ticker = physical(&output, "24hr/all");
    output.extend(reply(&mut e, NOW, &ticker, DAILY));
    let socket = output
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .expect("real private socket after native discovery");
    let requests: Vec<_> = output
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains("/g-accounts/accountPositions") => Some(r),
            _ => None,
        })
        .collect();
    for r in requests {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let positions = if currency == "USDT" {
            r#"[{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","size":"0","leverageRr":"-10","crossMargin":true}]"#
        } else {
            "[]"
        };
        let body = format!(
            r#"{{"code":0,"data":{{"account":{{"currency":"{currency}","accountBalanceRv":"1000","totalUsedBalanceRv":"0"}},"positions":{positions}}}}}"#
        );
        reply(&mut e, NOW, &r, &body);
    }
    (e, socket, releases)
}
fn stage(actions: &[Action]) -> (safe_cex_engine::RequestId, SignRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "provider-owned work cannot release eager HTTP"
    );
    let values: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(values.len(), 1, "one genuine dispatch signing attempt");
    values[0].clone()
}
#[test]
fn native_rest_fixed_vector_and_all_query_decoding_edges() {
    let c = native_sign::Credentials {
        key: "key".into(),
        secret: "secret".into(),
    };
    let h = c.headers("/g-accounts/accountPositions", "currency=USDT", "", NOW);
    assert_eq!(
        h.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        [
            "X-PHEMEX-ACCESS-TOKEN",
            "X-PHEMEX-REQUEST-EXPIRY",
            "X-PHEMEX-REQUEST-SIGNATURE",
            "Content-Type"
        ]
    );
    assert_eq!(
        h[2].1,
        "f5c2373fe7fd936754713178bcd405a0f810fb23b2ff086a50529815189a036f"
    );
    assert_eq!(
        c.headers(
            "/g-orders/create",
            "",
            r#"{"symbol":"BTCUSDT","clOrdID":"a/b"}"#,
            NOW
        )[2]
        .1,
        "c71903ea2be3eb4bbb67d46bb836b2ac377a5dcb0ed01632735d3d8e6210029b"
    );
    for (query, signature) in [
        (
            "orderID=%zz",
            "7e28ee752fc7ffec8d95de587b7b564ec3f3965cfc3a781408492c7e4f1a27c2",
        ),
        (
            "orderID=%",
            "641a488ad0b26aaa8d47bc0eac7d28061b7a9cedab243f3e7c50d7d9fcc84548",
        ),
        (
            "orderID=%4",
            "f03be27a8bb8ed4f39fc23e3700c0cdaad2664ec60a698cd1d0398f6925f87e7",
        ),
        (
            "orderID=+",
            "de7053e57220fdde799ed9889d1cc3e95810a90aaddb52485a6ec9ced82ec1c4",
        ),
        (
            "orderID=%FF",
            "a7d6680f4112529e01d17d41fdaa9fc41a6128c6f0b2190fd317055bbcb40133",
        ),
        (
            "orderID=%E2%82%AC",
            "c8cfec736b9db2ffa4b9d3c30f3f61a575cba49f7e7b905c38abccdb1f1fa871",
        ),
    ] {
        assert_eq!(
            c.headers("/g-orders/cancel", query, "", NOW)[2].1,
            signature
        );
    }
    for (delta, expiry) in [(499.9, "1700000060"), (500., "1700000061")] {
        assert_eq!(
            c.headers(
                "/g-accounts/accountPositions",
                "currency=USDT",
                "",
                NOW + delta
            )[1]
            .1,
            expiry
        );
    }
}
#[test]
fn existing_local_provider_is_an_independent_working_hmac_control() {
    let (local, refs) = local();
    let receipt = SignReceipt {
        nonce: None,
        stamp_ms: Some(NOW),
        expiry_ms: Some(NOW + 60_000.),
    };
    let work = hmac_sha256_work(
        refs.primary.as_ref().unwrap(),
        vec![PayloadPart::Bytes {
            bytes: b"/g-accounts/accountPositionscurrency=USDT1700000060".to_vec(),
        }],
        Context {
            venue: "phemex".into(),
            ..Default::default()
        },
        receipt,
    );
    let result = local.sign_ready(&work).unwrap().unwrap();
    assert_eq!(result.values.len(), 2);
    assert_eq!(result.values[0], b"key");
    assert_eq!(result.receipt, receipt);
    assert_eq!(
        safe_cex_crypto::hex(&result.values[1]),
        "f5c2373fe7fd936754713178bcd405a0f810fb23b2ff086a50529815189a036f"
    );
}
#[test]
fn native_identity_get_uses_secondary_and_client_lookup_uses_primary() {
    let (mut e, _) = bootstrap(true, None);
    let ordinary = physical(&ask(&mut e, NOW), "/g-accounts/accountPositions");
    assert_eq!(token(&ordinary), "read-key");
    assert_eq!(ordinary.route, None);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 701,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "caller/a,b +".into(),
            },
        }),
    );
    let output = actions(&mut e, NOW);
    assert!(output.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(action.as_ref(),safe_cex_engine::request_context::HttpAction::Prepare { request, intent:safe_cex_engine::request_context::RoutingIntent::Primary,.. } if request.url.contains("/by-order-id")))));
    let lookup = physical(&output, "/by-order-id");
    assert_eq!(token(&lookup), "key");
    assert_eq!(lookup.route, None);
    assert!(lookup.url.contains("clOrdID=caller%2Fa%2Cb%20%2B"));
}
#[test]
fn truly_unsigned_native_products_have_no_headers_or_provider_work() {
    let mut e = engine(Options::default(), None, None);
    e.start(NOW);
    let output = actions(&mut e, NOW);
    let r = physical(&output, "/public/products");
    assert!(r.headers.is_empty());
    assert_eq!(r.body, None);
    assert!(!output.iter().any(|a| matches!(a, Action::Sign { .. })));
}
#[test]
fn private_open_preserves_exact_auth_then_accepted_ack_aop_and_ping_wire() {
    let (mut e, socket) = bootstrap(false, None);
    e.handle(NOW, Event::WsOpened { socket });
    let output = actions(&mut e, NOW);
    let frames: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        frames[0],
        r#"{"method":"user.auth","params":["API","key","46a48d7b73f809f511182a4807f8301321bf3efe6f397c40f50b393e649a1419",1700000060],"id":999}"#
    );
    assert_eq!(
        frames[0],
        native_sign::Credentials {
            key: "key".into(),
            secret: "secret".into()
        }
        .websocket_auth(NOW, 999)
    );
    assert_eq!(frames.len(), 1, "opening ping requires accepted login");
    accepted_login(&mut e, socket, NOW);
}
#[test]
fn refused_provider_controls_native_read_before_any_http() {
    let provider = Arc::new(Refused {
        calls: AtomicUsize::new(0),
    });
    let mut e = engine(primary(false), Some(provider.clone()), None);
    let output = ask(&mut e, NOW);
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "native signed read must reach the owned provider"
    );
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
    let failures: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 700,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    // engine/src/signing.rs SignError::venue_error maps Refused to InvalidInput
    // and NotSent; these are the shared projection, not fixture policy.
    assert_eq!(failures[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
}
#[test]
fn missing_provider_cannot_fall_back_to_venue_held_material() {
    let mut e = engine(primary(false), None, None);
    let mut output = ask(&mut e, NOW);
    let (attempt, _) = stage(&output);
    e.handle(
        NOW,
        Event::Signed {
            attempt,
            result: Err(SignError::Unavailable),
        },
    );
    output.extend(actions(&mut e, NOW));
    assert!(
        !output.iter().any(|a| matches!(a, Action::Http(_))),
        "missing provider must not use a retained local secret"
    );
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 700, .. })
            ))
            .count(),
        1
    );
}
#[test]
fn deferred_read_signing_waits_then_preserves_exact_native_headers() {
    let (parked, local) = parked(true);
    let mut e = engine(primary(false), Some(parked), None);
    let output = ask(&mut e, NOW);
    let (attempt, work) = stage(&output);
    assert_eq!(work.receipt.expiry_ms, Some(NOW + 60_000.));
    let result = local.sign_ready(&work).unwrap();
    e.handle(NOW + 5., Event::Signed { attempt, result });
    let r = physical(&actions(&mut e, NOW + 5.), "/g-accounts/accountPositions");
    assert_eq!(
        r.headers,
        native_sign::Credentials {
            key: "key".into(),
            secret: "secret".into()
        }
        .headers("/g-accounts/accountPositions", "currency=USDT", "", NOW)
    );
    e.handle(
        NOW + 6.,
        Event::Signed {
            attempt,
            result: Err(SignError::Refused),
        },
    );
    assert!(
        !actions(&mut e, NOW + 6.)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
}
fn calibrated_expiry(offset: f64) {
    let (local, _) = local();
    let mut e = engine(primary(false), Some(local), Some(NOW + offset));
    let r = physical(&ask(&mut e, NOW), "/g-accounts/accountPositions");
    assert_eq!(
        r.headers,
        native_sign::Credentials {
            key: "key".into(),
            secret: "secret".into(),
        }
        .headers(
            "/g-accounts/accountPositions",
            "currency=USDT",
            "",
            NOW + offset
        ),
        "signature, expiry and header order use calibrated protocol UTC"
    );
}
fn deferred_rounded_expiry(offset: f64, expected_expiry: f64) {
    let (parked, local) = parked(true);
    let protocol = NOW + offset;
    let mut e = engine(primary(false), Some(parked), Some(protocol));
    let output = ask(&mut e, NOW);
    let (attempt, work) = stage(&output);
    let expiry = native_sign::Credentials::expiry(protocol);
    assert_eq!(expiry as f64 * 1000., expected_expiry);
    assert_eq!(work.receipt.expiry_ms, Some(expected_expiry));
    let result = local.sign_ready(&work).unwrap();
    e.handle(NOW + 5., Event::Signed { attempt, result });
    let r = physical(&actions(&mut e, NOW + 5.), "/g-accounts/accountPositions");
    assert_eq!(r.headers[1].1, expiry.to_string());
    assert_eq!(
        r.headers,
        native_sign::Credentials {
            key: "key".into(),
            secret: "secret".into(),
        }
        .headers(
            "/g-accounts/accountPositions",
            "currency=USDT",
            "",
            protocol
        )
    );
}
#[test]
fn deferred_dispatch_rounds_down_before_the_half_second() {
    deferred_rounded_expiry(499., NOW + 60_000.);
}
#[test]
fn deferred_dispatch_rounds_up_at_the_half_second() {
    deferred_rounded_expiry(500., NOW + 61_000.);
}
#[test]
fn deferred_dispatch_rounds_a_negative_fractional_calibration() {
    deferred_rounded_expiry(-30_000.4, NOW + 30_000.);
}
#[test]
fn positive_calibration_changes_only_protocol_expiry() {
    calibrated_expiry(30_000.);
}
#[test]
fn negative_calibration_changes_only_protocol_expiry() {
    calibrated_expiry(-30_000.);
}
#[test]
fn deferred_private_auth_cannot_be_overtaken_by_the_opening_ping() {
    let (parked, local) = parked(false);
    let (mut e, socket) = bootstrap(false, Some(parked.clone()));
    parked.paused.store(true, Ordering::SeqCst);
    e.handle(NOW, Event::WsOpened { socket });
    let output = actions(&mut e, NOW);
    assert!(
        !output.iter().any(|a| matches!(a, Action::WsSend { .. })),
        "parked authentication and its later ping must not leak before release"
    );
    let work: Vec<_> = output
        .into_iter()
        .filter_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((attempt, work)),
            _ => None,
        })
        .collect();
    assert_eq!(work.len(), 1);
    let (attempt, work) = work.into_iter().next().unwrap();
    let result = local.sign_ready(&work).unwrap();
    e.handle(NOW + 5., Event::Signed { attempt, result });
    let released = actions(&mut e, NOW + 5.);
    let texts: Vec<_> = released
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts.len(),
        1,
        "provider completion releases only auth before accepted ACK"
    );
    assert!(texts[0].contains("user.auth"));
    accepted_login(&mut e, socket, NOW + 5.);
}

fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

#[test]
fn native_active_orders_repair_uses_primary_even_with_secondary_credentials() {
    let (mut e, _) = bootstrap(true, None);
    e.handle(
        NOW + 1000.,
        Event::Command(Command::PlaceOrders {
            command: 702,
            orders: vec![opts("signed-primary-control")],
        }),
    );
    let output = actions(&mut e, NOW + 1000.);
    let create = physical(&output, "/g-orders/create");
    assert_eq!(create.method, Method::Put);
    assert_eq!(token(&create), "key");
    let expiry: f64 = create
        .headers
        .iter()
        .find(|(name, _)| *name == "X-PHEMEX-REQUEST-EXPIRY")
        .unwrap()
        .1
        .parse()
        .unwrap();
    let wake = expiry * 1000. + 5000.;
    let output = reply(&mut e, NOW + 1001., &create, r#"{"code":0,"data":{}}"#);
    let timer = output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == wake => Some(*timer),
            _ => None,
        })
        .expect("native unproven write schedules repair after signed expiry plus skew");
    e.handle(wake, Event::TimerFired { timer });
    let prepared: Vec<_> = e.drain_actions().collect();
    assert!(prepared.iter().any(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),safe_cex_engine::request_context::HttpAction::Prepare {intent:safe_cex_engine::request_context::RoutingIntent::Primary,request,..} if request.url.contains("activeList")))));
    let output = drive_routes(&mut e, wake, prepared);
    let read = physical(&output, "/g-orders/activeList");
    assert_eq!(read.method, Method::Get);
    assert_eq!(token(&read), "key");
}

#[test]
fn deferred_native_write_stays_with_the_provider_before_any_transport() {
    let (parked, _) = parked(false);
    let (mut e, _) = bootstrap(false, Some(parked.clone()));
    parked.paused.store(true, Ordering::SeqCst);
    e.handle(
        NOW + 1000.,
        Event::Command(Command::PlaceOrders {
            command: 703,
            orders: vec![opts("parked-native-write")],
        }),
    );
    let output = actions(&mut e, NOW + 1000.);
    let (_, work) = stage(&output);
    assert_eq!(work.receipt.expiry_ms, Some(NOW + 61_000.));
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 703, .. })))
    );
}

fn accepted_login(e: &mut Engine<Probe>, socket: SocketId, now: f64) {
    e.handle(
        now,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"result":{"status":"success"}}"#,
        },
    );
    let out = actions(e, now);
    let texts: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        [
            r#"{"id":1202,"method":"aop_p.subscribe","params":[]}"#,
            r#"{"id":1201,"method":"server.ping","params":[]}"#
        ]
    );
}
fn observe_protocol(e: &mut Engine<Probe>, raw: f64, protocol: f64) {
    e.handle(
        raw,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["__protocol_observation__".into(), protocol.to_string()],
        }),
    );
    assert!(actions(e, raw).is_empty());
}
fn place(e: &mut Engine<Probe>, raw: f64, command: u32, ids: &[&str]) -> Vec<Action> {
    e.handle(
        raw,
        Event::Command(Command::PlaceOrders {
            command,
            orders: ids.iter().map(|id| opts(id)).collect(),
        }),
    );
    actions(e, raw)
}
fn expiry(request: &HttpRequest) -> f64 {
    request
        .headers
        .iter()
        .find(|(name, _)| *name == "X-PHEMEX-REQUEST-EXPIRY")
        .unwrap()
        .1
        .parse::<f64>()
        .unwrap()
        * 1000.
}
fn latest_release(log: &ReleaseLog, request: &HttpRequest) -> safe_cex_engine::SigningRelease {
    let releases = log.lock().unwrap();
    assert_eq!(
        releases.iter().filter(|(id, _)| *id == request.id).count(),
        1,
        "one actual release observation for exact native Trade request"
    );
    releases.iter().find(|(id, _)| *id == request.id).unwrap().1
}
fn repair_at(actions: &[Action], expected: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == expected => Some(*timer),
            _ => None,
        })
        .expect("native repair uses MAX uncapped raw expiry + 5000")
}
fn calibrated_write_release(offset: f64) {
    let (parked, local) = parked(false);
    let (mut e, _, log) = bootstrap_observed(false, Some(parked.clone()), None);
    observe_protocol(&mut e, NOW + 1000., NOW + 1000. + offset);
    parked.paused.store(true, Ordering::SeqCst);
    let out = place(&mut e, NOW + 1000., 810, &["raw-expiry-owner"]);
    let (attempt, work) = stage(&out);
    e.handle(
        NOW + 4000.,
        Event::Signed {
            attempt,
            result: local.sign_ready(&work).unwrap(),
        },
    );
    let out = actions(&mut e, NOW + 4000.);
    let request = physical(&out, "/g-orders/create");
    let release = latest_release(&log, &request);
    let raw_expiry = expiry(&request) - offset;
    assert_eq!(release.raw_expiry_ms(), Some(raw_expiry));
    assert_eq!(release.time().raw_ms, NOW + 4000.);
    assert!(
        raw_expiry > NOW + 1000. + 20_000.,
        "signature replayability is not capped at HTTP lifecycle"
    );
    parked.paused.store(false, Ordering::SeqCst);
    let out = reply(&mut e, NOW + 4001., &request, r#"{"code":0,"data":{}}"#);
    let repair = repair_at(&out, raw_expiry + 5000.);
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("/g-orders/activeList")))
    );
    e.handle(NOW + 21_001., Event::TimerFired { timer: repair });
    let early = actions(&mut e, NOW + 21_001.);
    assert!(
        !early
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("/g-orders/activeList"))),
        "HTTP lifecycle expiry cannot authorize early reconciliation"
    );
    let repair = repair_at(&early, raw_expiry + 5000.);
    e.handle(raw_expiry + 5000., Event::TimerFired { timer: repair });
    physical(&actions(&mut e, raw_expiry + 5000.), "/g-orders/activeList");
}
#[test]
fn release_write_positive_calibration_retains_uncapped_raw_expiry() {
    calibrated_write_release(30_000.);
}
#[test]
fn release_write_negative_calibration_retains_uncapped_raw_expiry() {
    calibrated_write_release(-30_000.);
}
#[test]
fn release_write_reprepare_discards_old_clock_revision_and_publishes_only_fresh_bound() {
    let (parked, local) = parked(false);
    let (mut e, _, log) = bootstrap_observed(false, Some(parked.clone()), None);
    parked.paused.store(true, Ordering::SeqCst);
    let out = place(&mut e, NOW + 1000., 811, &["revision-owner"]);
    let (old, work) = stage(&out);
    observe_protocol(&mut e, NOW + 1001., NOW + 31_001.);
    e.handle(
        NOW + 1002.,
        Event::Signed {
            attempt: old,
            result: local.sign_ready(&work).unwrap(),
        },
    );
    let out = actions(&mut e, NOW + 1002.);
    assert!(log.lock().unwrap().is_empty());
    let (fresh, work) = stage(&out);
    assert_ne!(old, fresh);
    e.handle(
        NOW + 1003.,
        Event::Signed {
            attempt: fresh,
            result: local.sign_ready(&work).unwrap(),
        },
    );
    let out = actions(&mut e, NOW + 1003.);
    let request = physical(&out, "/g-orders/create");
    let release = latest_release(&log, &request);
    assert_eq!(release.raw_expiry_ms(), Some(expiry(&request) - 30_000.));
    e.handle(
        NOW + 1004.,
        Event::Signed {
            attempt: old,
            result: Err(SignError::Refused),
        },
    );
    assert!(actions(&mut e, NOW + 1004.).is_empty());
    assert_eq!(log.lock().unwrap().len(), 1);
}
fn accepted(order_id: &str, client_id: &str) -> String {
    format!(
        r#"{{"code":0,"data":{{"orderID":"{order_id}","clOrdID":"{client_id}","symbol":"BTCUSDT","side":"Buy","posSide":"Merged","orderType":"Limit","orderQtyRq":"0.01","priceRp":"69000","ordStatus":"Init","execStatus":"Init","bizError":0}}}}"#
    )
}
#[test]
fn release_write_job_keeps_larger_first_raw_expiry_when_second_release_is_earlier() {
    let (mut e, _, log) = bootstrap_observed(false, None, None);
    let out = place(&mut e, NOW + 1000., 812, &["max-first", "max-second"]);
    let first = physical(&out, "/g-orders/create");
    let first_bound = latest_release(&log, &first).raw_expiry_ms().unwrap();
    observe_protocol(&mut e, NOW + 1100., NOW + 400.);
    let out = reply(
        &mut e,
        NOW + 1100.,
        &first,
        &accepted("native-max-first", "max-first"),
    );
    let second = physical(&out, "/g-orders/create");
    let second_bound = latest_release(&log, &second).raw_expiry_ms().unwrap();
    assert!(
        second_bound < first_bound,
        "working fixture must exercise earlier later-release expiry"
    );
    let out = reply(&mut e, NOW + 1101., &second, r#"{"code":0,"data":{}}"#);
    let repair = repair_at(&out, first_bound + 5000.);
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("/g-orders/activeList")))
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==second_bound+5000.)),
        "smaller released bound cannot schedule repair"
    );
    e.handle(second_bound + 5000., Event::TimerFired { timer: repair });
    let early = actions(&mut e, second_bound + 5000.);
    assert!(
        !early
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("/g-orders/activeList"))),
        "larger released signature is still replayable"
    );
    let repair = repair_at(&early, first_bound + 5000.);
    e.handle(first_bound + 5000., Event::TimerFired { timer: repair });
    physical(
        &actions(&mut e, first_bound + 5000.),
        "/g-orders/activeList",
    );
}
struct RefuseWrite {
    inner: Provider,
    writes: AtomicUsize,
    refuse_at: usize,
}
impl Signer for RefuseWrite {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.inner.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if request.context.path == "/g-orders/create"
            && self.writes.fetch_add(1, Ordering::SeqCst) == self.refuse_at
        {
            Some(Err(SignError::Refused))
        } else {
            self.inner.sign_ready(request)
        }
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
#[test]
fn release_write_refused_second_leg_retains_accepted_ids_without_whole_job_not_sent() {
    let provider = Arc::new(RefuseWrite {
        inner: local().0,
        writes: AtomicUsize::new(0),
        refuse_at: 1,
    });
    let (mut e, _, log) = bootstrap_observed(false, Some(provider), None);
    let out = place(
        &mut e,
        NOW + 1000.,
        813,
        &["accepted-prefix", "refused-suffix"],
    );
    let first = physical(&out, "/g-orders/create");
    let out = reply(
        &mut e,
        NOW + 1001.,
        &first,
        &accepted("native-accepted-prefix", "accepted-prefix"),
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.method!=Method::Get)),
        "refused suffix must never release"
    );
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 813,
                error,
            }) => Some(error),
            _ => None,
        })
        .expect("one typed signer failure");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_ne!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["native-accepted-prefix"]
    );
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.client_order_id.as_deref() == Some("refused-suffix"))
    );
    assert_eq!(
        log.lock().unwrap().len(),
        1,
        "only the accepted first leg was released"
    );
}
#[test]
fn release_write_refused_first_leg_has_no_release_and_preserves_typed_not_sent() {
    let provider = Arc::new(RefuseWrite {
        inner: local().0,
        writes: AtomicUsize::new(0),
        refuse_at: 0,
    });
    let (mut e, _, log) = bootstrap_observed(false, Some(provider), None);
    let out = place(&mut e, NOW + 1000., 814, &["refused-first"]);
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.method!=Method::Get))
    );
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 814,
                error,
            }) => Some(error),
            _ => None,
        })
        .expect("one typed signer failure");
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::InvalidInput), Some(Outcome::NotSent))
    );
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.client_order_id.as_deref() == Some("refused-first"))
    );
    assert!(log.lock().unwrap().is_empty());
}

struct CorruptResult {
    inner: Provider,
    variant: usize,
    ws: bool,
}
impl Signer for CorruptResult {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.inner.sign(work)
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        let result = self.inner.sign_ready(work)?;
        Some(result.map(|mut result| {
            if (work.context.method == "WS") == self.ws {
                match self.variant {
                    0 => {
                        result.values.pop();
                    }
                    1 => {
                        result.values[0] = vec![0xff];
                    }
                    2 => {
                        result.values[0] = b"invalid\r\nheader".to_vec();
                    }
                    3 => {
                        result.values[1].pop();
                    }
                    4 => {
                        result.receipt.expiry_ms = Some(1.);
                    }
                    5 => {
                        result.values[0] = vec![b'k'; 8193];
                    }
                    6 => {
                        result.receipt.expiry_ms = None;
                    }
                    _ => unreachable!(),
                }
            }
            result
        }))
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
#[test]
fn malformed_present_mac_or_receipt_never_releases_http() {
    for variant in 0..7 {
        let provider = Arc::new(CorruptResult {
            inner: local().0,
            variant,
            ws: false,
        });
        let mut e = engine(primary(false), Some(provider), None);
        let output = ask(&mut e, NOW);
        assert!(
            !output.iter().any(|a| matches!(a, Action::Http(_))),
            "variant {variant}"
        );
        assert_eq!(
            output
                .iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 700, .. })
                ))
                .count(),
            1
        );
    }
}
#[test]
fn malformed_present_mac_or_receipt_never_releases_private_auth_or_ping() {
    for variant in 0..7 {
        let provider = Arc::new(CorruptResult {
            inner: local().0,
            variant,
            ws: true,
        });
        let (mut e, socket) = bootstrap(false, Some(provider));
        e.handle(NOW, Event::WsOpened { socket });
        let output = actions(&mut e, NOW);
        assert!(
            !output
                .iter()
                .any(|a| matches!(a, Action::WsSend{socket:s,..} if *s==socket)),
            "variant {variant}"
        );
        assert!(
            output
                .iter()
                .any(|a| matches!(a, Action::WsClose{socket:s} if *s==socket))
        );
    }
}
