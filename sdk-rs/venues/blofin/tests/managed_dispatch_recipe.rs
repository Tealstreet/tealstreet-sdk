//! Managed component proof; native caller REDs remain in hmac_dispatch_original.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::request_context::{
    Admission, Claim, Context, Credential, Error, HttpAction, HttpRelease, Operation, Permit,
    Policy, Preparation, Prepared, RetryPolicy, RoutingIntent, Scope,
};
use safe_cex_engine::signing::{
    Identities, Identity, Operation as Work, PayloadPart, SignError, SignFuture, SignRequest,
    SignResult, Signer,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Engine, Event, HttpRequest, Method,
    RequestId, Response, SignStep, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_blofin::dispatch_signing::Http;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

const NOW: f64 = 1_700_000_000_123.875;
const PATH: &str = "/api/v1/account/batch-leverage-info?instId=BTC-USDT&marginMode=cross";
const PRIMARY: &str =
    "MDIzM2U3NzAzYjk1ZWFmYjhlMGExY2IyOWYwODcyZDEwNWQ4YjQyM2M1NGQwOThjYzU2NWQzNTM3MWE1NTczZg==";
const SECONDARY: &str =
    "Y2IxNzMwYjY0NzY0YzU3MmM0YzNmODNmOGRlY2Q1MDNjMDcxNmQ5MGZiMGRiMzU4ZWNhYjg3Yjg1MDkxZDFkYg==";

#[derive(Clone, Copy)]
enum Alter {
    None,
    Method,
    Origin,
    Path,
    Body,
    Header,
    SignerHeader,
}
fn alter(action: &mut Action, change: Alter) {
    let Action::Http(r) = action else {
        panic!("HTTP component")
    };
    match change {
        Alter::None => {}
        Alter::Method => r.method = Method::Delete,
        Alter::Origin => r.url = r.url.replace("openapi.blofin.com", "foreign.invalid"),
        Alter::Path => r.url.push_str("&extra=1"),
        Alter::Body => r.body = Some("{}".into()),
        Alter::Header => r.headers.push(("unexpected", "fixture".into())),
        Alter::SignerHeader => r
            .signer_headers
            .push(("unexpected".into(), "fixture".into())),
    }
}
struct Attested {
    recipe: Http,
    prepare: Alter,
    apply: Alter,
}
impl SignStep for Attested {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        self.recipe.unsigned_http_headers()
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        alter(action, self.prepare);
        self.recipe.prepare(action, cx)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        alter(action, self.apply);
        self.recipe.apply(action, result)
    }
    fn freshness_ms(&self) -> f64 {
        self.recipe.freshness_ms()
    }
}
struct Probe {
    identity: Identity,
    slot: CredentialSlot,
    testnet: bool,
    method: Method,
    path: String,
    body: Option<String>,
    prepare_alter: Alter,
    apply_alter: Alter,
    context: Option<Context>,
    failures: Vec<VenueError>,
    releases: Vec<HttpRelease>,
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: Some(Credential {
                        slot: self.slot,
                        reference: self.identity.credential.clone(),
                    }),
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        self.context = Some(context.clone());
        let recipe = Http::new(
            self.identity.clone(),
            self.testnet,
            self.method,
            self.path.clone(),
            self.body.clone(),
        );
        let (url, _, body) = recipe.unsigned();
        cx.request_signed_managed(
            context.clone(),
            self.method,
            url,
            body,
            Box::new(Attested {
                recipe,
                prepare: self.prepare_alter,
                apply: self.apply_alter,
            }),
            context,
        )
        .unwrap_or_else(|_| panic!("real managed admission"));
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::Mutable(|this, _: &mut Context, release: &HttpRelease| {
            this.releases.push(*release);
            Ok(Admission::Unscheduled)
        })
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        pending: Context,
        _: Error,
        error: VenueError,
    ) {
        self.on_http_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, pending: Context, error: VenueError) {
        let _ = cx.finish_http_context(&pending);
        self.failures.push(error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Context>, pending: Context, error: VenueError) {
        self.on_http_failed(cx, pending, error);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let Command::SetTickerSymbols { symbols } = command else {
            panic!("clock observation only")
        };
        assert!(cx.observe_protocol_time(cx.now + symbols[0].parse::<f64>().unwrap()));
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, pending: Context, _: Response<'_>) {
        cx.finish_http_context(&pending).unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("typed failure required")
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {
        panic!("engine-owned timer")
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn setup() -> (LocalSigner, Identities) {
    LocalSigner::from_json(serde_json::json!({
        "primary":{"key":"fixture-primary-key","secret":"fixture-primary-secret","passphrase":"fixture-primary-pass"},
        "secondary":{"key":"fixture-secondary-key","secret":"fixture-secondary-secret","passphrase":"fixture-secondary-pass"},
    }).to_string()).unwrap()
}
fn probe(ids: &Identities, slot: CredentialSlot) -> Probe {
    Probe {
        identity: match slot {
            CredentialSlot::Primary => ids.primary.clone(),
            CredentialSlot::Secondary => ids.secondary.clone(),
        }
        .unwrap(),
        slot,
        testnet: false,
        method: Method::Get,
        path: PATH.into(),
        body: None,
        prepare_alter: Alter::None,
        apply_alter: Alter::None,
        context: None,
        failures: vec![],
        releases: vec![],
    }
}
struct Rig {
    e: Engine<Probe>,
    provider: LocalSigner,
    draws: Arc<Mutex<Vec<usize>>>,
    first: Preparation,
    initial: Vec<Action>,
    resource: u64,
}
impl Rig {
    fn with(
        make: impl FnOnce(&Identities) -> Probe,
        entropy: u8,
        installed: Option<Arc<dyn Signer>>,
    ) -> Self {
        let (provider, ids) = setup();
        let mut e = safe_cex_engine::test_support::seeded_engine(make(&ids));
        e.set_identities(ids).unwrap();
        e.set_signer(installed).unwrap();
        let draws = Arc::new(Mutex::new(Vec::new()));
        let copy = draws.clone();
        e.set_signing_entropy(Box::new(move |bytes| {
            let mut n = copy.lock().unwrap();
            let turn = n.len();
            n.push(bytes.len());
            if entropy == 255 {
                return Err("fixture unavailable".into());
            }
            bytes.fill(7 + (turn as u8) * 2);
            if entropy == 1 {
                for (i, b) in bytes.iter_mut().enumerate() {
                    *b = if i == 0 {
                        4
                    } else if i % 2 == 1 {
                        8
                    } else {
                        44
                    };
                }
            }
            Ok(())
        }))
        .unwrap();
        e.start(NOW);
        let initial = e.drain_actions().collect::<Vec<_>>();
        let first = prepare(&initial).0;
        assert!(draws.lock().unwrap().is_empty());
        no_wire(&initial);
        Self {
            e,
            provider,
            draws,
            first,
            initial,
            resource: 100,
        }
    }
    fn new(slot: CredentialSlot) -> Self {
        Self::with(|ids| probe(ids, slot), 0, None)
    }
    fn route(&mut self, p: Preparation, at: f64, expires_at: Option<f64>) -> Vec<Action> {
        assert!(self.e.http_preparation_pending(p));
        self.resource += 1;
        self.e.http_prepared(
            at,
            p,
            Prepared {
                id: self.resource,
                expires_at,
            },
        );
        self.e.drain_actions().collect()
    }
    fn complete(
        &mut self,
        at: f64,
        attempt: RequestId,
        result: Result<SignResult, SignError>,
    ) -> Vec<Action> {
        self.e.handle(at, Event::Signed { attempt, result });
        self.e.drain_actions().collect()
    }
    fn sign(&mut self, at: f64, attempt: RequestId, work: &SignRequest) -> Vec<Action> {
        let result = self.provider.sign_now(work).unwrap();
        self.complete(at, attempt, Ok(result))
    }
    fn claim(&mut self, at: f64, out: &[Action]) -> HttpRequest {
        let permit = bind(out);
        assert_eq!(permit.attempt(), self.first.attempt());
        assert_eq!(permit.prepared_id(), self.resource);
        assert!(matches!(
            self.e.claim_http(at, permit),
            Ok(Claim::Released(_))
        ));
        out.iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Bind { request, .. } => Some(request.clone()),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    }
}
fn prepare(out: &[Action]) -> (Preparation, RoutingIntent) {
    out.iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(request.timeout_ms, Some(20_000));
                    Some((*preparation, *intent))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("real Prepare")
}
fn work(out: &[Action]) -> (RequestId, SignRequest) {
    out.iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work.clone())),
            _ => None,
        })
        .expect("provider work after route")
}
fn bind(out: &[Action]) -> Permit {
    out.iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Bind { permit, .. } => Some(*permit),
                _ => None,
            },
            _ => None,
        })
        .expect("real Bind before Claim")
}
fn no_wire(out: &[Action]) {
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsSend { .. }))
    );
}
fn no_bind(out: &[Action]) {
    no_wire(out);
    assert!(
        !out.iter().any(
            |a| matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Bind{..}))
        )
    );
}
fn parts(work: &SignRequest) -> Vec<String> {
    let Work::Sign { payload, .. } = &work.operations[1] else {
        panic!("HMAC work")
    };
    payload
        .iter()
        .map(|p| match p {
            PayloadPart::Bytes { bytes } => String::from_utf8(bytes.clone()).unwrap(),
            _ => panic!("exact public bytes"),
        })
        .collect()
}

#[test]
fn selected_identity_intent_preimage_headers_and_independent_vectors_match() {
    for slot in [CredentialSlot::Primary, CredentialSlot::Secondary] {
        for testnet in [false, true] {
            let mut r = Rig::with(
                |ids| {
                    let mut p = probe(ids, slot);
                    p.testnet = testnet;
                    p
                },
                0,
                None,
            );
            assert_eq!(
                prepare(&r.initial).1,
                match slot {
                    CredentialSlot::Primary => RoutingIntent::Primary,
                    CredentialSlot::Secondary => RoutingIntent::Secondary,
                }
            );
            let first = r.first;
            let out = r.route(first, NOW, None);
            let (attempt, w) = work(&out);
            let id = r.e.venue().identity.clone();
            assert_eq!(w.operations.len(), 3);
            assert!(matches!(&w.operations[0],Work::Present{key} if *key==id.credential));
            assert!(
                matches!(&w.operations[1],Work::Sign{algorithm,key,..} if *algorithm==safe_cex_engine::signing::Algorithm::HmacSha256 && *key==id.signing)
            );
            assert!(
                matches!(&w.operations[2],Work::Present{key} if Some(key)==id.passphrase.as_ref())
            );
            assert_eq!(
                parts(&w),
                vec![
                    PATH.into(),
                    "GET".into(),
                    "1700000000123".into(),
                    "m".repeat(21),
                    String::new()
                ]
            );
            assert_eq!(w.context.venue, "blofin");
            assert_eq!(w.context.method, "GET");
            assert_eq!(w.context.path, PATH);
            let out = r.sign(NOW, attempt, &w);
            no_wire(&out);
            let packet = r.claim(NOW, &out);
            assert_eq!(packet.method, Method::Get);
            assert_eq!(packet.body, None);
            assert_eq!(
                packet.url,
                format!(
                    "https://{}{PATH}",
                    if testnet {
                        "demo-trading-openapi.blofin.com"
                    } else {
                        "openapi.blofin.com"
                    }
                )
            );
            assert_eq!(
                packet.headers,
                vec![
                    (
                        "ACCESS-KEY",
                        match slot {
                            CredentialSlot::Primary => "fixture-primary-key",
                            CredentialSlot::Secondary => "fixture-secondary-key",
                        }
                        .into()
                    ),
                    (
                        "ACCESS-SIGN",
                        match slot {
                            CredentialSlot::Primary => PRIMARY,
                            CredentialSlot::Secondary => SECONDARY,
                        }
                        .into()
                    ),
                    ("ACCESS-NONCE", "m".repeat(21)),
                    ("ACCESS-TIMESTAMP", "1700000000123".into()),
                    (
                        "ACCESS-PASSPHRASE",
                        match slot {
                            CredentialSlot::Primary => "fixture-primary-pass",
                            CredentialSlot::Secondary => "fixture-secondary-pass",
                        }
                        .into()
                    ),
                    ("Content-Type", "application/json".into())
                ]
            );
            assert_eq!(*r.draws.lock().unwrap(), vec![21]);
            assert_eq!(r.e.venue().releases[0].deadline, NOW + 20_000.);
        }
    }
}

#[test]
fn primary_post_body_has_an_independent_fixed_vector() {
    let body = r#"{"instId":"BTC-USDT","leverage":"12","marginMode":"cross"}"#;
    let mut r = Rig::with(
        |ids| {
            let mut p = probe(ids, CredentialSlot::Primary);
            p.method = Method::Post;
            p.path = "/api/v1/account/set-leverage".into();
            p.body = Some(body.into());
            p
        },
        0,
        None,
    );
    let first = r.first;
    let out = r.route(first, NOW, None);
    let (attempt, w) = work(&out);
    assert_eq!(
        parts(&w),
        vec![
            "/api/v1/account/set-leverage".into(),
            "POST".into(),
            "1700000000123".into(),
            "m".repeat(21),
            body.into()
        ]
    );
    let out = r.sign(NOW, attempt, &w);
    let packet = r.claim(NOW, &out);
    assert_eq!(packet.body.as_deref(), Some(body));
    assert_eq!(
        packet.headers[1].1,
        "MzRjMzZmZWFmOWMyZjFhYTAzZmQ5NmY1Y2ZiZjFiOGNmMGUyZWRhODExMjc5ZDcwM2JmODliZTg4NjI1YmY4YQ=="
    );
}
#[test]
fn filtered_nonce_uses_one_native_21_byte_draw() {
    let mut r = Rig::with(|ids| probe(ids, CredentialSlot::Secondary), 1, None);
    let first = r.first;
    let out = r.route(first, NOW, None);
    let (attempt, w) = work(&out);
    assert_eq!(parts(&w)[3], "n");
    let out = r.sign(NOW, attempt, &w);
    let packet = r.claim(NOW, &out);
    assert_eq!(packet.headers[2], ("ACCESS-NONCE", "n".into()));
    assert_eq!(
        packet.headers[1].1,
        "ZTYxMjQ1MTI0MGU5YzgzYjNhOGY2OTZmYzAyM2YwY2Q5YTljNDRjNjEyMDE1NjY2MWEwM2M1ZTMxNWJkMmFhZg=="
    );
    assert_eq!(*r.draws.lock().unwrap(), vec![21]);
}
#[test]
fn delayed_route_then_parked_remote_result_and_claim_keep_the_original_bound() {
    let mut r = Rig::new(CredentialSlot::Secondary);
    r.e.prepare_dispatch(NOW + 799.);
    let out = r.e.drain_actions().collect::<Vec<_>>();
    no_bind(&out);
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert!(r.draws.lock().unwrap().is_empty());
    let first = r.first;
    let out = r.route(first, NOW + 800., None);
    no_bind(&out);
    let (attempt, w) = work(&out);
    assert_eq!(w.receipt.stamp_ms, Some((NOW + 800.).floor()));
    assert!(r.e.venue().releases.is_empty());
    let out = r.sign(NOW + 810., attempt, &w);
    no_wire(&out);
    assert!(r.e.venue().releases.is_empty());
    let packet = r.claim(NOW + 820., &out);
    assert_eq!(packet.headers[3].1, (NOW + 800.).floor().to_string());
    assert_eq!(r.e.venue().releases.len(), 1);
    assert_eq!(r.e.venue().releases[0].deadline, NOW + 20_000.);
    assert_eq!(r.e.claim_http(NOW + 821., bind(&out)), Err(Error::Revoked));
}
struct Parked(Arc<AtomicUsize>);
impl Signer for Parked {
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        None
    }
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}
#[test]
fn actual_installed_parked_provider_is_not_called_before_route_or_after_entropy_refusal() {
    for entropy in [0, 255] {
        let calls = Arc::new(AtomicUsize::new(0));
        let parked = Arc::new(Parked(calls.clone()));
        let mut r = Rig::with(
            |ids| probe(ids, CredentialSlot::Primary),
            entropy,
            Some(parked.clone()),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let first = r.first;
        let out = r.route(first, NOW + 1., None);
        no_bind(&out);
        assert_eq!(*r.draws.lock().unwrap(), vec![21]);
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(entropy == 0));
        if entropy == 0 {
            let (_, w) = work(&out);
            let mut future = parked.sign(w);
            assert!(matches!(
                future
                    .as_mut()
                    .poll(&mut std::task::Context::from_waker(std::task::Waker::noop())),
                std::task::Poll::Pending
            ));
            assert!(r.e.venue().failures.is_empty());
        } else {
            assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
            assert_eq!(
                r.e.venue().failures,
                vec![SignError::Unavailable.venue_error()]
            );
        }
    }
}
#[test]
fn missing_passphrase_ref_and_invalid_identity_refuse_before_entropy_or_provider() {
    for invalid in [false, true] {
        let mut r = Rig::with(
            |ids| {
                let mut p = probe(ids, CredentialSlot::Primary);
                if invalid {
                    p.identity.signing.0 = String::new();
                } else {
                    p.identity.passphrase = None;
                }
                p
            },
            0,
            None,
        );
        let first = r.first;
        let out = r.route(first, NOW, None);
        no_bind(&out);
        assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
        assert!(r.draws.lock().unwrap().is_empty());
        assert_eq!(
            r.e.venue().failures,
            vec![SignError::InvalidRequest.venue_error()]
        );
    }
}
#[test]
fn unsigned_intent_is_attested_at_prepare_before_drawing_entropy() {
    for change in [
        Alter::Method,
        Alter::Origin,
        Alter::Path,
        Alter::Body,
        Alter::Header,
        Alter::SignerHeader,
    ] {
        let mut r = Rig::with(
            |ids| {
                let mut p = probe(ids, CredentialSlot::Primary);
                p.prepare_alter = change;
                p
            },
            0,
            None,
        );
        let first = r.first;
        let out = r.route(first, NOW, None);
        no_bind(&out);
        assert!(r.draws.lock().unwrap().is_empty());
        assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
        assert_eq!(
            r.e.venue().failures,
            vec![SignError::InvalidRequest.venue_error()]
        );
    }
}
#[test]
fn signed_intent_cannot_change_during_apply() {
    for change in [
        Alter::Method,
        Alter::Origin,
        Alter::Path,
        Alter::Body,
        Alter::Header,
        Alter::SignerHeader,
    ] {
        let mut r = Rig::with(
            |ids| {
                let mut p = probe(ids, CredentialSlot::Primary);
                p.apply_alter = change;
                p
            },
            0,
            None,
        );
        let first = r.first;
        let out = r.route(first, NOW, None);
        let (attempt, w) = work(&out);
        let out = r.sign(NOW, attempt, &w);
        no_bind(&out);
        assert_eq!(
            r.e.venue().failures,
            vec![SignError::InvalidRequest.venue_error()]
        );
    }
}
#[test]
fn invalid_provider_receipt_count_mac_text_or_headers_never_bind() {
    for invalid in 0..8 {
        let mut r = Rig::new(CredentialSlot::Primary);
        let first = r.first;
        let out = r.route(first, NOW, None);
        let (attempt, w) = work(&out);
        let mut result = r.provider.sign_now(&w).unwrap();
        match invalid {
            0 => result.receipt.stamp_ms = Some(NOW.floor() + 1.),
            1 => {
                result.values.pop();
            }
            2 => result.values[1].pop().map(|_| ()).unwrap(),
            3 => result.values[0] = Vec::new(),
            4 => result.values[2] = b"bad\r\npass".to_vec(),
            5 => result.values[0] = vec![255],
            6 => result.headers = vec![("ACCESS-SIGN".into(), "injected".into())],
            _ => result.values[2] = b"bad\0pass".to_vec(),
        }
        let out = r.complete(NOW, attempt, Ok(result));
        no_bind(&out);
        assert_eq!(
            r.e.venue().failures,
            vec![SignError::InvalidResult.venue_error()]
        );
        assert!(r.e.venue().releases.is_empty());
    }
}
#[test]
fn provider_refusal_retires_once_without_a_wire_packet() {
    let mut r = Rig::new(CredentialSlot::Secondary);
    let first = r.first;
    let out = r.route(first, NOW, None);
    let (attempt, _) = work(&out);
    let out = r.complete(NOW + 1., attempt, Err(SignError::Refused));
    no_bind(&out);
    assert_eq!(r.e.venue().failures, vec![SignError::Refused.venue_error()]);
    let out = r.complete(NOW + 2., attempt, Err(SignError::Refused));
    no_bind(&out);
    assert_eq!(r.e.venue().failures.len(), 1);
}
#[test]
fn late_host_preparation_does_not_draw_nonce_or_start_provider() {
    let mut r = Rig::new(CredentialSlot::Primary);
    let first = r.first;
    let out = r.route(first, NOW + 20_000., None);
    no_bind(&out);
    assert!(r.draws.lock().unwrap().is_empty());
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert_eq!(r.e.venue().failures.len(), 1);
    assert_eq!(
        r.e.venue().failures[0].kind,
        Some(safe_cex_engine::ErrorKind::Timeout)
    );
}
#[test]
fn protocol_clock_reprepare_uses_fresh_nonce_and_preserves_physical_attempt_and_deadline() {
    let mut r = Rig::new(CredentialSlot::Secondary);
    let first = r.first;
    let out = r.route(first, NOW + 1., None);
    let (old, w) = work(&out);
    r.e.handle(
        NOW + 2.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["1000".into()],
        }),
    );
    let out = r.sign(NOW + 3., old, &w);
    no_bind(&out);
    let fresh_route = renewed_prepare(&out, first, 19_997);
    assert_eq!(*r.draws.lock().unwrap(), vec![21]);
    let out = r.sign(NOW + 4., old, &w);
    no_bind(&out);
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    let out = r.route(fresh_route, NOW + 5., None);
    let (new, fresh) = work(&out);
    assert_ne!(old, new);
    assert_eq!(fresh.receipt.stamp_ms, Some((NOW + 1005.).floor()));
    assert_ne!(parts(&w)[3], parts(&fresh)[3]);
    assert_eq!(*r.draws.lock().unwrap(), vec![21, 21]);
    let out = r.sign(NOW + 6., new, &fresh);
    let packet = r.claim(NOW + 7., &out);
    assert_eq!(packet.id, first.attempt().request);
    assert_eq!(r.e.venue().releases[0].deadline, NOW + 20_000.);
    assert_eq!(packet.headers[2].1, parts(&fresh)[3]);
}

fn renewed_prepare(out: &[Action], old: Preparation, remaining: u32) -> Preparation {
    no_bind(out);
    assert!(!out.iter().any(|a| matches!(a, Action::Sign { .. })));
    assert_eq!(out.iter().filter(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Cancel{preparation,..} if *preparation==old))).count(),1);
    let preparations = out
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert_eq!(request.timeout_ms, Some(remaining));
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(preparations.len(), 1);
    let fresh = preparations[0];
    assert_ne!(fresh, old);
    assert_eq!(fresh.attempt(), old.attempt());
    fresh
}
#[test]
fn expired_host_route_reprepares_before_new_entropy_under_the_same_raw_bound() {
    let mut r = Rig::new(CredentialSlot::Primary);
    let first = r.first;
    let out = r.route(first, NOW + 1., Some(NOW + 50.));
    let (old, w) = work(&out);
    let out = r.sign(NOW + 50., old, &w);
    no_wire(&out);
    assert_eq!(r.e.claim_http(NOW + 50., bind(&out)), Err(Error::Stale));
    assert!(r.e.venue().releases.is_empty());
    let out = r.e.drain_actions().collect::<Vec<_>>();
    let fresh_route = renewed_prepare(&out, first, 19_950);
    assert_eq!(*r.draws.lock().unwrap(), vec![21]);
    let out = r.route(fresh_route, NOW + 60., None);
    let (new, fresh) = work(&out);
    assert_ne!(old, new);
    assert_eq!(fresh.receipt.stamp_ms, Some((NOW + 60.).floor()));
    assert_ne!(parts(&w)[3], parts(&fresh)[3]);
    let out = r.sign(NOW + 61., new, &fresh);
    let packet = r.claim(NOW + 62., &out);
    assert_eq!(packet.id, first.attempt().request);
    assert_eq!(r.e.venue().releases[0].deadline, NOW + 20_000.);
}
