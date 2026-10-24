//! NEW-CONTRACT recipe controls; native caller-adoption originals stay separate.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason};
use safe_cex_engine::request_context::{Operation as HttpOperation, Policy, RetryPolicy};
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, SignError, SignFuture, SignRequest, SignResult,
    Signer,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, Event, HttpRelease, HttpRequest, Method,
    Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_kraken::sign::{dispatch::{Selected, challenge_work}, setup};
use std::sync::{Arc, Mutex};
const NOW: f64 = 1_700_000_000_000.;
const SECRET: &str = "a3Jha2VuLXRlc3Qtc2VjcmV0";
const FUTURES_MAC: &str = "UJjfP/I++ZnynbKoDc73lGxQMzNrzWTg9iEv5CkmFUNfPn/nZqGrCzwzPcX9HB6SFP1CuPhKFMBI5Ar52tJP4Q==";
const SPOT_MAC: &str = "vkWVoPHiCpSwNZz8duXYFcqT2SZq3GwKIFzE3Ef0mcI1ibEAdkyAYiTVQ5NS/i/mNblY1rKxDOWnSQNWna9beA==";
struct Provider {
    inner: LocalSigner,
    work: Mutex<Vec<SignRequest>>,
    tamper: u8,
}
impl Signer for Provider {
    fn immediate(&self) -> bool { true }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.work.lock().unwrap().push(request.clone());
        if self.tamper == 1 { return Some(Err(SignError::Refused)); }
        self.inner.sign_ready(request).map(|result| result.map(|mut result| {
            match self.tamper {
                2 => result.receipt.nonce = Some(1),
                3 => result.values[2].truncate(63),
                4 => result.headers.push(("Injected".into(), "bad".into())),
                5 => result.values[0] = b"bad\r\nkey".to_vec(),
                _ => {},
            }
            result
        }))
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_ready(&request).unwrap();
        Box::pin(async move { result })
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
fn local(key: &str, secret: &str, subject: Option<&str>) -> LocalIdentity {
    LocalIdentity {
        key: Secret::new(key.into()), secret: Secret::new(secret.into()),
        stable_id: subject.map(str::to_owned), ..Default::default()
    }
}
fn provider(subject: &str, read: bool, tamper: u8) -> (Arc<Provider>, Identities) {
    let (inner, ids) = LocalSigner::from_setup_with_policy(LocalSetup {
        primary: Some(local("fixture-primary", SECRET, Some(subject))),
        secondary: read.then(|| local("fixture-read", "cmVhZC1zZWNyZXQ=", Some(subject))),
    }, setup::futures_policy()).unwrap();
    (Arc::new(Provider {inner, work: Mutex::new(vec![]), tamper}), ids)
}
struct Probe {
    ids: Identities, spot: bool, role: CredentialRole, path: String, form: String,
    method: Method, tamper_url: bool, errors: Vec<VenueError>,
}
impl Venue for Probe {
    type Pending = ();
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE}
    }
    fn http_release() -> HttpRelease<Self> { HttpRelease::NonceUnscheduled(|_, _, _| Ok(())) }
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        let selected = if self.spot { Selected::spot(&self.ids, self.role) }
            else { Selected::futures(&self.ids, self.role) }.unwrap();
        let root = cx.admit_http_command(Policy {
            operation: HttpOperation::Read, credential: Some(selected.credential()),
            timeout_ms: 20_000, pacing: None, retry: RetryPolicy::Never,
        }).unwrap();
        let recipe = if self.spot { selected.spot_http(&self.path, &self.form, 5000.) }
            else { selected.futures_http(self.method, &self.path, &self.form, false, 5000.) }.unwrap();
        let url = if self.tamper_url { "https://wrong.test/".into() } else { recipe.url().to_owned() };
        let body = recipe.body().map(str::to_owned);
        cx.request_signed_managed(root, self.method, url, body, Box::new(recipe), ()).unwrap();
    }
    fn on_sign_error(&mut self, _: &mut Cx<'_, ()>, _: (), error: VenueError) { self.errors.push(error); }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> { None }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> { Box::new(std::iter::empty()) }
}
fn run(p: Arc<Provider>, ids: Identities, spot: bool, form: &str, tamper_url: bool) -> Vec<Action> {
    let path = if spot { "/0/private/Balance" } else { "/derivatives/api/v3/sendorder" };
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        ids: ids.clone(), spot, role: CredentialRole::PrimaryRead {reason: PrimaryReason::History, scope: None},
        path: path.into(), form: form.into(), method: Method::Post, tamper_url, errors: vec![],
    });
    engine.set_identities(ids).unwrap(); engine.set_signer(Some(p)).unwrap(); engine.start(NOW);
    engine.handle(NOW, Event::Command(Command::FetchAccount { command: 1 }));
    safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
}
fn packets(actions: &[Action]) -> Vec<&HttpRequest> {
    actions.iter().filter_map(|a| if let Action::Http(r) = a {Some(r)} else {None}).collect()
}
#[test]
fn futures_atomic_batch_matches_independent_vector_and_native_header_order() {
    let (p, ids) = provider("recipe-futures-vector", false, 0);
    let actions = run(p.clone(), ids, false, "limitPrice=100&orderType=lmt&side=buy&size=1&symbol=PF_XBTUSD", false);
    let requests = packets(&actions); assert_eq!(requests.len(), 1);
    let r = requests[0];
    assert_eq!(r.headers.iter().map(|h| h.0).collect::<Vec<_>>(), ["APIKey", "Authent", "Nonce", "Content-Type"]);
    assert_eq!(r.headers[1].1, FUTURES_MAC); assert_eq!(r.headers[2].1, "1700000000000000");
    assert_eq!(r.url, "https://futures.kraken.com/derivatives/api/v3/sendorder");
    let work = p.work.lock().unwrap(); assert_eq!(work.len(), 1); assert_eq!(work[0].operations.len(), 3);
}
#[test]
fn spot_nonce_is_last_and_matches_independent_vector() {
    let (p, ids) = provider("recipe-spot-vector", false, 0);
    let actions = run(p, ids, true, "", false);
    let requests = packets(&actions); assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body.as_deref(), Some("nonce=1700000000000000"));
    assert_eq!(requests[0].headers.iter().map(|h| h.0).collect::<Vec<_>>(), ["API-Key", "API-Sign", "Content-Type", "Accept"]);
    assert_eq!(requests[0].headers[1].1, SPOT_MAC);
    let (p, ids) = provider("recipe-spot-insertion", false, 0);
    let actions = run(p, ids, true, "z=a%20b&a=%2B", false);
    assert_eq!(packets(&actions)[0].body.as_deref(), Some("z=a%20b&a=%2B&nonce=1700000000000000"));
}
#[test]
fn primary_read_and_mutation_roles_stay_primary_and_read_uses_secondary() {
    let (_, ids) = provider("recipe-role", true, 0);
    assert_eq!(Selected::futures(&ids, CredentialRole::Read {scope: None}).unwrap().selection().slot(), CredentialSlot::Secondary);
    for role in [CredentialRole::Mutation, CredentialRole::SessionAuth, CredentialRole::PrimaryRead {reason: PrimaryReason::History, scope: None}] {
        assert_eq!(Selected::futures(&ids, role).unwrap().selection().slot(), CredentialSlot::Primary);
    }
    assert_eq!(Selected::spot(&ids, CredentialRole::Read {scope: None}).unwrap().selection().slot(), CredentialSlot::Primary);
    let read_only = Identities {primary: None, secondary: ids.secondary};
    assert!(Selected::futures(&read_only, CredentialRole::Read {scope: None}).is_err());
    assert!(Selected::futures(&read_only, CredentialRole::PrimaryRead {reason: PrimaryReason::History, scope: None}).is_err());
}
#[test]
fn invalid_subject_is_refused_before_any_dispatch() {
    let (_, mut ids) = provider("recipe-subject", false, 0);
    for subject in [None, Some(""), Some("bad\nid")] {
        ids.primary.as_mut().unwrap().stable_id = subject.map(str::to_owned);
        assert!(Selected::futures(&ids, CredentialRole::Read {scope: None}).is_err());
    }
}
#[test]
fn declared_base64_rejects_invalid_material_and_preserves_untrimmed_key() {
    let bad = LocalSetup {primary: Some(local("key", "*", Some("bad-secret"))), secondary: None};
    assert!(LocalSigner::from_setup_with_policy(bad, setup::futures_policy()).is_err());
    let (signer, ids) = LocalSigner::from_setup_with_policy(LocalSetup {
        primary: Some(local(" key ", SECRET, Some("untrimmed-key"))), secondary: None,
    }, setup::futures_policy()).unwrap();
    let work = challenge_work(&ids, "c100b894-1729-464d-ace1-52dbce11db42").unwrap();
    let result = signer.sign_ready(&work).unwrap().unwrap();
    assert_eq!(result.values[0], b" key ");
    assert_eq!(STANDARD.encode(&result.values[2]), "RBpQqz7vfqXJ38JklfnuiQNVp5NimxtQPdKwk8kPa9pK1yz62RP2yMEqeYgIzfYbdUF/vQlZvIUASliWvHOXcw==");
    assert_eq!(work.receipt.nonce, None);
}
#[test]
fn provider_refusal_and_tampering_never_release_http() {
    for tamper in 1..=5 {
        let (p, ids) = provider(&format!("recipe-tamper-{tamper}"), false, tamper);
        assert!(packets(&run(p.clone(), ids, false, "", false)).is_empty());
        assert_eq!(p.work.lock().unwrap().len(), 1);
    }
}
#[test]
fn wrong_transport_is_refused_before_provider() {
    let (p, ids) = provider("recipe-transport-tamper", false, 0);
    assert!(packets(&run(p.clone(), ids, false, "", true)).is_empty());
    assert!(p.work.lock().unwrap().is_empty());
}
#[test]
fn shared_subject_across_products_and_engines_advances_nonce() {
    let (p, ids) = provider("recipe-shared-subject", true, 0);
    let first = run(p.clone(), ids.clone(), false, "", false);
    let second = run(p.clone(), ids, true, "", false);
    assert_eq!(packets(&first)[0].headers[2].1, "1700000000000000");
    assert_eq!(packets(&second)[0].body.as_deref(), Some("nonce=1700000000000001"));
}
