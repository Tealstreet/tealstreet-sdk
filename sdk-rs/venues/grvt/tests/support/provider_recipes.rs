//! Engine recipe controls with public test keys; installed adoption stays separate.
use super::*;
use safe_cex_crypto::signer::{KeyEncoding, LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::request_context::{
    self, HttpAction, Operation as Effect, Policy, Prepared, RetryPolicy,
};
use safe_cex_engine::signing::{
    Capability, CredentialSetupPolicy, IdentitySetupPolicy, SignFuture, Signer, SigningPolicy,
};
use safe_cex_engine::{Command, Cx, Engine, Event, HttpRequest, Venue, VenueError};
use std::collections::VecDeque;
use std::sync::Mutex;
const NOW: f64 = 1_700_000_000_000.;
const KEY: &str = "0101010101010101010101010101010101010101010101010101010101010101";
const ADDRESS: &str = "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1";
#[derive(Clone, Copy)]
enum Mode {
    Local,
    External,
    WrongSigner,
    WrongDigest,
    HighS,
    WrongReceipt,
    ExtraHeader,
    Refuse,
}
#[derive(serde::Deserialize)]
struct Vectors {
    digest: String,
    signatures: Vec<String>,
    #[serde(rename = "highS")]
    high_s: String,
    signer: String,
}
fn vectors() -> Vectors {
    serde_json::from_str(include_str!("../fixtures/provider_signatures.json")).unwrap()
}
fn bytes(s: &str) -> Vec<u8> {
    s.strip_prefix("0x")
        .unwrap_or(s)
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
struct Recording {
    inner: LocalSigner,
    mode: Mode,
    deferred: bool,
    work: Mutex<Vec<SignRequest>>,
}
impl Recording {
    fn execute(&self, work: &SignRequest) -> Result<SignResult, SignError> {
        let mut requests = self.work.lock().unwrap();
        let index = requests.len();
        requests.push(work.clone());
        drop(requests);
        if matches!(self.mode, Mode::Refuse) {
            return Err(SignError::Refused);
        }
        let mut result = self.inner.sign_ready(work).unwrap()?;
        match self.mode {
            Mode::External => result.values[1] = bytes(&vectors().signatures[index % 2]),
            Mode::WrongSigner => {
                let key = "02".repeat(32);
                let address = ethereum::private_key_address(&key).unwrap();
                result.values[0] = bytes(&address);
                result.values[1] = bytes(&ethereum::sign_digest(&digest(work), &key).unwrap());
            }
            Mode::WrongDigest => {
                let mut d = digest(work);
                d[0] ^= 1;
                result.values[1] = bytes(&ethereum::sign_digest(&d, KEY).unwrap());
            }
            Mode::HighS => result.values[1] = bytes(&vectors().high_s),
            Mode::WrongReceipt => result.receipt.expiry_ms = Some(NOW + 3_600_001.),
            Mode::ExtraHeader => result.headers.push(("X-Provider".into(), "refused".into())),
            _ => {}
        }
        Ok(result)
    }
}
impl Signer for Recording {
    fn immediate(&self) -> bool {
        !self.deferred
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        (!self.deferred).then(|| self.execute(work))
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        let result = self.execute(&work);
        Box::pin(async move { result })
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
fn digest(work: &SignRequest) -> [u8; 32] {
    let Operation::Sign { payload, .. } = &work.operations[1] else {
        panic!("missing order signature")
    };
    let [PayloadPart::Bytes { bytes }] = payload.as_slice() else {
        panic!("noncanonical digest")
    };
    bytes.as_slice().try_into().unwrap()
}
struct Probe {
    recipe: Option<Box<dyn SignStep>>,
    identity: Identity,
    url: String,
    login: bool,
    errors: Arc<Mutex<Vec<VenueError>>>,
}
impl Venue for Probe {
    type Pending = request_context::Context;
    fn start(&mut self, _: &mut Cx<'_, Self::Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: safe_cex_engine::SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Self::Pending>, _: safe_cex_engine::SocketId, _: &str) {
    }
    fn on_response(
        &mut self,
        _: &mut Cx<'_, Self::Pending>,
        _: Self::Pending,
        _: safe_cex_engine::Response<'_>,
    ) {
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.on_sign_error(cx, pending, VenueError::new(error));
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Self::Pending>, _: safe_cex_engine::TimerId) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn capture_http_reconciliation(
        &self,
        _: &Self::Pending,
    ) -> Result<request_context::ReconciliationData, request_context::Error> {
        request_context::ReconciliationData::new("grvt-provider-recipe".into())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, _: Command) {
        let slots = Slots::new(Some(&self.identity), None, SlotPolicy::default());
        let role = if self.login {
            CredentialRole::SessionAuth
        } else {
            CredentialRole::Mutation
        };
        let (selection, identity) = slots.select(role).unwrap();
        let policy = Policy {
            operation: Effect::Mutation,
            credential: Some(selection.credential(identity.credential.clone())),
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let context = if self.login {
            let session = cx
                .open_session(request_context::SessionPolicy {
                    credential: policy.credential.clone(),
                    repeat_safe: false,
                    private_egress: None,
                })
                .unwrap();
            cx.session_context(&session, policy).unwrap()
        } else {
            cx.admit_http_command(policy).unwrap()
        };
        let result = cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            self.url.clone(),
            None,
            self.recipe.take().unwrap(),
            context,
        );
        assert!(result.is_ok());
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.errors.lock().unwrap().push(error.clone());
        let _ = cx.fail_context(&pending, error);
    }
}
struct Fixture {
    engine: Engine<Probe>,
    provider: Arc<Recording>,
    errors: Arc<Mutex<Vec<VenueError>>>,
}
fn fields() -> OrderFields {
    OrderFields {
        subaccount: "1234567890".into(),
        instrument: "BTC_USDT_Perp".into(),
        asset_id: sign::parse_asset_id("79228162514264337593543950337").unwrap(),
        base_decimals: 9,
        amount: "1".into(),
        limit_price: Some("50000".into()),
        buying: true,
        time_in_force: TimeInForce::GoodTillCancel,
        reduce_only: false,
        nonce: 42,
        expiration_ns: 1_700_003_600_000_000_000,
        client_id: "99".into(),
        created_ns: 1_700_000_000_000_000_000,
        trigger: None,
    }
}
fn fixture(mode: Mode, testnet: bool, login: bool) -> Fixture {
    fixture_with_fields(mode, testnet, login, fields())
}
fn fixture_with_fields(mode: Mode, testnet: bool, login: bool, material: OrderFields) -> Fixture {
    let (inner, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(" api\"key\n ".into()),
                secret: Secret::new(KEY.into()),
                secret_encoding: KeyEncoding::Hex,
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                trim_credential: Some(false),
                secret_encoding: Some(KeyEncoding::Hex),
                signing_algorithm: Some(Algorithm::Secp256k1Digest),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    let identity = ids.primary.as_ref().unwrap().clone();
    let (recipe, url): (Box<dyn SignStep>, String) = if login {
        let r = Login::new(identity.credential.clone(), testnet);
        let url = r.url();
        (Box::new(r), url)
    } else {
        let r = CreateOrder::new(
            &identity,
            ADDRESS,
            Arc::new(FrozenOrder::new(material, testnet).unwrap()),
        )
        .unwrap();
        let url = r.url();
        (Box::new(r), url)
    };
    let errors = Arc::new(Mutex::new(vec![]));
    let provider = Arc::new(Recording {
        inner,
        mode,
        deferred: matches!(mode, Mode::External),
        work: Mutex::new(vec![]),
    });
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        recipe: Some(recipe),
        identity,
        url,
        login,
        errors: errors.clone(),
    });
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.set_identities(ids).unwrap();
    engine.start(NOW);
    engine
        .handle_command(
            NOW,
            Command::PlaceOrders {
                command: 7,
                orders: vec![
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
                    )
                    .unwrap(),
                ],
            },
            None,
        )
        .unwrap();
    Fixture {
        engine,
        provider,
        errors,
    }
}
fn drive(f: &mut Fixture, now: f64) -> Vec<Action> {
    f.engine.prepare_dispatch(now);
    let mut queue: VecDeque<_> = f.engine.drain_actions().collect();
    let mut routes = std::collections::HashMap::new();
    let mut output = vec![];
    let mut count = 0;
    while let Some(action) = queue.pop_front() {
        count += 1;
        assert!(count <= 128, "unbounded recipe driver");
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(routes.insert(request.id, preparation).is_none());
                    f.engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(routes.remove(&request.id), Some(preparation));
                    match f.engine.claim_http(now, permit) {
                        Ok(request_context::Claim::Released(_)) => {
                            output.push(Action::Http(request))
                        }
                        Ok(request_context::Claim::WaitUntil(_)) => {
                            panic!("unexpected recipe pacing")
                        }
                        Err(_) => {}
                    }
                }
                HttpAction::Cancel { preparation, .. } => {
                    routes.remove(&preparation.attempt().request);
                }
                HttpAction::BindGroup { .. } => panic!("no grouped recipe"),
            },
            Action::Sign { attempt, work, .. } => f.engine.handle(
                now,
                Event::Signed {
                    attempt,
                    result: f.provider.execute(&work),
                },
            ),
            action => output.push(action),
        }
        for action in f
            .engine
            .drain_actions()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            queue.push_front(action);
        }
    }
    assert!(
        routes.is_empty(),
        "recipe route retained without a bind or cancel"
    );
    output
}
fn packet(actions: &[Action]) -> &HttpRequest {
    let packets: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(packets.len(), 1);
    packets[0]
}
fn hold_bind(f: &mut Fixture, now: f64) -> (request_context::Permit, HttpRequest) {
    f.engine.prepare_dispatch(now);
    let mut q: VecDeque<_> = f.engine.drain_actions().collect();
    for _ in 0..64 {
        let a = q
            .pop_front()
            .unwrap_or_else(|| panic!("missing real bind: {:?}", f.errors.lock().unwrap()));
        match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => f.engine.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id: preparation.attempt().request.0,
                        expires_at: Some(now + 1.),
                    },
                ),
                HttpAction::Bind {
                    permit, request, ..
                } => return (permit, request),
                HttpAction::BindGroup { .. } => panic!("no grouped recipe"),
                _ => {}
            },
            Action::Sign { attempt, work, .. } => f.engine.handle(
                now,
                Event::Signed {
                    attempt,
                    result: f.provider.execute(&work),
                },
            ),
            _ => {}
        }
        q.extend(f.engine.drain_actions());
    }
    panic!("unbounded bind fixture")
}
#[test]
fn local_order_matches_native_wire_bytes_and_both_domains() {
    for testnet in [false, true] {
        let mut f = fixture(Mode::Local, testnet, false);
        let actions = drive(&mut f, NOW);
        assert!(
            f.errors.lock().unwrap().is_empty(),
            "{:?}",
            f.errors.lock().unwrap()
        );
        let request = packet(&actions);
        let builder = if testnet {
            BUILDER_TESTNET
        } else {
            BUILDER_MAINNET
        };
        let legs = [sign::OrderLeg {
            asset_id: fields().asset_id,
            contract_size: 1_000_000_000,
            limit_price: 50_000_000_000_000,
            is_buying: true,
        }];
        let signature = sign::sign_order(
            &sign::Order {
                sub_account_id: 1_234_567_890,
                is_market: false,
                time_in_force: 1,
                post_only: false,
                reduce_only: false,
                legs: &legs,
                builder: Some(sign::BuilderFee {
                    address: sign::parse_address(builder).unwrap(),
                    fee: 300,
                }),
                nonce: 42,
                expiration: fields().expiration_ns,
            },
            KEY,
            testnet,
        )
        .unwrap();
        let expected = format!(
            r#"{{"o":{{"sa":"1234567890","im":false,"ti":"GOOD_TILL_TIME","po":false,"ro":false,"l":[{{"i":"BTC_USDT_Perp","s":"1","lp":"50000","ib":true}}],"s":{{"s":"{ADDRESS}","r":"{}","s1":"{}","v":{},"e":"1700003600000000000","n":42,"ci":"{}"}},"m":{{"co":"99","ct":"1700000000000000000"}},"b":"{builder}","bf":"0.03"}}}}"#,
            signature.r,
            signature.s,
            signature.v,
            if testnet { 326 } else { 325 }
        );
        assert_eq!(request.body.as_deref(), Some(expected.as_str()));
        assert_eq!(
            request.headers,
            vec![("Content-Type", "application/json".into())]
        );
        assert!(f.errors.lock().unwrap().is_empty());
        let batches = f.provider.work.lock().unwrap();
        assert_eq!(batches.len(), 1);
        let w = &batches[0];
        assert_eq!(w.operations.len(), 2);
        let (
            Operation::PublicAddress { key: a },
            Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: b,
                ..
            },
        ) = (&w.operations[0], &w.operations[1])
        else {
            panic!("unexpected order operations")
        };
        assert_eq!(a, b);
        assert_eq!(w.receipt.expiry_ms, Some(NOW + 3_600_000.));
        assert_eq!(w.receipt.stamp_ms, None);
        assert_eq!(w.receipt.nonce, None);
        if !testnet {
            assert_eq!(format!("0x{}", hex(&digest(w))), vectors().digest);
        }
    }
}
#[test]
fn local_stale_reprepare_preserves_body_digest_and_all_frozen_fields() {
    let mut f = fixture(Mode::Local, false, false);
    let (permit, first) = hold_bind(&mut f, NOW);
    assert!(f.engine.claim_http(NOW + 2., permit).is_err());
    let actions = drive(&mut f, NOW + 2.);
    let second = packet(&actions);
    assert_eq!(first.body, second.body);
    let w = f.provider.work.lock().unwrap();
    assert_eq!(w.len(), 2);
    assert_eq!(w[0], w[1]);
}
#[test]
fn asynchronous_external_reprepare_accepts_distinct_valid_signatures_only() {
    let v = vectors();
    assert_eq!(v.signer, ADDRESS);
    for signature in &v.signatures {
        assert!(ethereum::signed_by(
            &bytes(&v.digest),
            &bytes(signature),
            ADDRESS
        ));
    }
    let mut f = fixture(Mode::External, false, false);
    let (permit, first) = hold_bind(&mut f, NOW);
    assert!(f.engine.claim_http(NOW + 2., permit).is_err());
    let actions = drive(&mut f, NOW + 2.);
    let second = packet(&actions);
    assert_ne!(first.body, second.body);
    let without_signature = |body: &str, signature: &str| {
        let signature = bytes(signature);
        let parity = signature[64] % 27;
        let fields = format!(
            ",\"r\":\"0x{}\",\"s1\":\"0x{}\",\"v\":{}",
            hex(&signature[..32]),
            hex(&signature[32..64]),
            parity + 27
        );
        assert_eq!(body.matches(&fields).count(), 1);
        body.replace(&fields, "")
    };
    assert_eq!(
        without_signature(first.body.as_ref().unwrap(), &v.signatures[0]),
        without_signature(second.body.as_ref().unwrap(), &v.signatures[1]),
    );
    let w = f.provider.work.lock().unwrap();
    assert_eq!(w.len(), 2);
    assert_eq!(w[0], w[1]);
    assert!(f.errors.lock().unwrap().is_empty());
}
#[test]
fn wrong_signer_wrong_digest_high_s_receipt_and_provider_headers_never_release() {
    for mode in [
        Mode::WrongSigner,
        Mode::WrongDigest,
        Mode::HighS,
        Mode::WrongReceipt,
        Mode::ExtraHeader,
        Mode::Refuse,
    ] {
        let mut f = fixture(mode, false, false);
        let actions = drive(&mut f, NOW);
        assert_eq!(f.provider.work.lock().unwrap().len(), 1);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert_eq!(f.errors.lock().unwrap().len(), 1);
        assert_eq!(
            f.errors.lock().unwrap()[0].outcome,
            Some(safe_cex_engine::Outcome::NotSent)
        );
        assert!(
            drive(&mut f, NOW + 1.)
                .iter()
                .all(|a| !matches!(a, Action::Http(_)))
        );
    }
}
#[test]
fn login_presents_exact_bytes_without_order_work_or_private_bundle() {
    for testnet in [false, true] {
        let mut f = fixture(Mode::Local, testnet, true);
        let actions = drive(&mut f, NOW);
        assert!(
            f.errors.lock().unwrap().is_empty(),
            "{:?}",
            f.errors.lock().unwrap()
        );
        let request = packet(&actions);
        assert_eq!(
            request.body.as_deref(),
            Some(r#"{"api_key":" api\"key\n "}"#)
        );
        assert_eq!(
            request.headers,
            LOGIN_HEADERS
                .iter()
                .map(|(n, v)| (*n, (*v).into()))
                .collect::<Vec<_>>()
        );
        let w = f.provider.work.lock().unwrap();
        assert_eq!(w.len(), 1);
        assert!(matches!(
            w[0].operations.as_slice(),
            [Operation::Present { .. }]
        ));
        assert_eq!(w[0].receipt, SignReceipt::default());
    }
}
#[test]
fn expired_order_refuses_before_provider_or_bind_without_renewal() {
    let mut material = fields();
    material.created_ns -= 1_000_000;
    material.expiration_ns = (NOW as i64) * 1_000_000;
    let mut f = fixture_with_fields(Mode::Local, false, false, material);
    let actions = drive(&mut f, NOW);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(f.provider.work.lock().unwrap().is_empty());
    assert_eq!(f.errors.lock().unwrap().len(), 1);
    assert_eq!(
        f.errors.lock().unwrap()[0].kind,
        Some(safe_cex_engine::ErrorKind::Timeout)
    );
}

#[test]
fn credential_only_primary_cannot_construct_an_order_recipe() {
    let (_provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("grvt-credential-only-fixture".into()),
                secret: Secret::new(String::new()),
                secret_encoding: KeyEncoding::Hex,
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                signing: SigningPolicy::Optional,
                secret_encoding: Some(KeyEncoding::Hex),
                signing_algorithm: Some(Algorithm::Secp256k1Digest),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    let identity = identities
        .primary
        .as_ref()
        .expect("trusted credential-only slot");
    assert!(identity.validate_credential().is_ok());
    assert!(matches!(identity.signing_ref(), Err(SignError::MissingKey)));
    let order = Arc::new(FrozenOrder::new(fields(), false).unwrap());
    assert!(matches!(
        CreateOrder::new(identity, ADDRESS, order),
        Err(SignError::InvalidRequest)
    ));
}
