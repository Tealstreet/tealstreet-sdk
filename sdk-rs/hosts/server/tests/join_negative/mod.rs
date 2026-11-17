use axum::{
    body::Bytes,
    extract::WebSocketUpgrade,
    http::{HeaderMap, Method, Uri},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hpke::{
    Deserializable, Serializable, aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use safe_cex_engine::signing::{
    Algorithm, KeyRef, Operation, PayloadPart, Provider, SignRequest, SignResult,
};
use safe_cex_exchange_server::{
    accounts::{Account, Registry, now_ms, testing},
    envelope::InitPayload,
    out::{Out, Receiver},
    session::{Identity, JoinCompletion, Session},
    state::Metrics,
    threads::EngineThreads,
};
use safe_cex_native::{HttpRoute, RoutingPlan, WsRoute, WsRoutes};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

pub const ACCOUNT: &str = "negative-join-fixture";
pub const USER: &str = "negative-join-owner";
const WAIT: Duration = Duration::from_secs(15);

pub trait Venue: Send + Sync + 'static {
    const NAME: &'static str;
    fn init() -> InitPayload;
    fn algorithm() -> Algorithm;
    fn replayable(_frame: &Value) -> bool {
        true
    }
    fn replay_delay() -> Duration {
        Duration::from_millis(5)
    }
    fn credentials(other: bool) -> Value;
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value);
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value;
    fn websocket(request: &Value, seen: &mut Observed) -> Value;
}

#[derive(Default)]
pub struct Observed {
    pub signed_reads: usize,
    pub rejections: usize,
    pub writes: usize,
}

#[derive(Clone, Copy)]
pub enum Answer {
    Correct,
    OtherKey,
    Replay,
}
#[derive(Default)]
pub struct Device {
    prior: HashMap<String, (Vec<u8>, Value)>,
    pub responses: Vec<Value>,
    pub replayed: usize,
}
impl Device {
    fn answer<V: Venue>(&mut self, frame: &Value, answer: Answer) -> Value {
        let (slot, preimage, fresh) = V::signature(frame, matches!(answer, Answer::OtherKey));
        let mut reply = if matches!(answer, Answer::Replay) && V::replayable(frame) {
            let (old, reply) = self
                .prior
                .get(&slot)
                .expect("a successful prior attempt signed this operation");
            assert_ne!(
                old, &preimage,
                "replay must cross a real signing-byte boundary"
            );
            self.replayed += 1;
            reply.clone()
        } else {
            if matches!(answer, Answer::Correct) && V::replayable(frame) {
                self.prior.insert(slot, (preimage, fresh.clone()));
            }
            fresh
        };
        reply["id"] = frame["d"]["id"].clone();
        self.responses.push(reply.clone());
        reply
    }
}

pub struct Fixture {
    pub registry: Arc<Registry>,
    pub seen: Arc<Mutex<Observed>>,
    server: tokio::task::JoinHandle<()>,
}
impl Fixture {
    pub async fn new<V: Venue>() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Observed::default()));
        let ws_seen = seen.clone();
        let http_seen = seen.clone();
        let app = axum::Router::new()
            .route(
                "/ws",
                get(move |upgrade: WebSocketUpgrade| {
                    let seen = ws_seen.clone();
                    async move {
                        upgrade.on_upgrade(move |mut socket| async move {
                            while let Some(Ok(axum::extract::ws::Message::Text(text))) =
                                socket.recv().await
                            {
                                let request: Value = serde_json::from_str(&text).unwrap();
                                let reply = V::websocket(&request, &mut seen.lock().unwrap());
                                if socket
                                    .send(axum::extract::ws::Message::Text(
                                        reply.to_string().into(),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        })
                    }
                }),
            )
            .fallback(
                move |method: Method, uri: Uri, headers: HeaderMap, bytes: Bytes| {
                    let seen = http_seen.clone();
                    async move {
                        let body = if bytes.is_empty() {
                            Value::Null
                        } else {
                            serde_json::from_slice(&bytes).unwrap()
                        };
                        let reply =
                            V::http(&method, &uri, &headers, &body, &mut seen.lock().unwrap());
                        axum::Json(reply)
                    }
                },
            );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let http = HttpRoute {
            prefix: base.clone(),
            ..Default::default()
        };
        let ws = WsRoute {
            host: Some(base.replacen("http:", "ws:", 1) + "/ws"),
            ..Default::default()
        };
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        testing::set_routing(
            &mut registry,
            USER,
            RoutingPlan {
                http: http.clone(),
                public_http: Some(http.clone()),
                read_http: Some(http.clone()),
                read_public_http: Some(http.clone()),
                public_read_http: Some(http),
                ws: WsRoutes {
                    public: ws.clone(),
                    private: ws.clone(),
                    business: ws,
                },
                ..Default::default()
            },
        );
        Self {
            registry: Arc::new(registry),
            seen,
            server,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

pub struct Client {
    session: Option<Session>,
    sent: Receiver,
    done: mpsc::UnboundedReceiver<JoinCompletion>,
    pub frames: Vec<Value>,
}
impl Client {
    pub fn new(f: &Fixture) -> Self {
        let (out, sent, _) = Out::channel(1024);
        let mut session = Session::new(
            f.registry.clone(),
            Arc::new(Metrics::default()),
            Identity {
                user: Some(USER.into()),
                chat_handle: None,
                token: Some(("synthetic-negative-join-token".into(), now_ms() + 120_000)),
                resume_from: None,
                resumable: false,
            },
            out,
        );
        let done = session.take_verified();
        Self {
            session: Some(session),
            sent,
            done,
            frames: Vec::new(),
        }
    }
    pub fn socket(&self) -> u64 {
        self.session.as_ref().unwrap().socket
    }
    pub async fn send(&mut self, frame: Value) {
        self.session
            .as_mut()
            .unwrap()
            .handle(&frame.to_string())
            .await
            .unwrap();
    }
    pub async fn init(&mut self, init: InitPayload) {
        let mut frame = json!({"t":"init", "d":init});
        // InitPayload deliberately omits sealed material from its diagnostic serializer.
        if let Some(sealed) = &init.sealed {
            frame["d"]["sealed"] = serde_json::to_value(sealed).unwrap();
        }
        self.send(frame).await;
    }
    pub async fn offer<V: Venue>(&mut self) -> Value {
        self.send(
            json!({"t":"msg", "d":{"op":"delegated-key-req", "accountName":ACCOUNT,
            "data":{"exchangeName":V::NAME}}}),
        )
        .await;
        tokio::time::timeout(WAIT, async {
            loop {
                let frame: Value = serde_json::from_str(&self.sent.recv().await.unwrap()).unwrap();
                let found = frame["d"]["op"] == "delegated-key";
                self.frames.push(frame.clone());
                if found {
                    return frame["d"].clone();
                }
            }
        })
        .await
        .expect("real Session handover offer")
    }
    pub async fn finish<V: Venue>(&mut self, device: &mut Device, answer: Answer) {
        tokio::time::timeout(WAIT, async {
            loop {
                tokio::select! {
                    biased;
                    text = self.sent.recv() => {
                        let frame: Value = serde_json::from_str(&text.unwrap()).unwrap();
                        self.frames.push(frame.clone());
                        if frame["t"] == "sign-req" {
                            let reply = device.answer::<V>(&frame, answer);
                            self.send(json!({"t":"sign-res", "d":reply})).await;
                        } else if frame["d"]["data"][1]["initRefused"] == true { break; }
                    }
                    completed = self.done.recv() => {
                        self.session.as_mut().unwrap().verification_completed(completed.unwrap()).await.unwrap();
                        break;
                    }
                }
            }
        }).await.expect("production admission completes");
        self.drain();
    }
    fn drain(&mut self) {
        while let Ok(text) = self.sent.try_recv() {
            self.frames.push(serde_json::from_str(&text).unwrap());
        }
    }
    fn assert_refused(&self, f: &Fixture) {
        assert!(
            f.registry.held(USER, ACCOUNT, self.socket()).is_none(),
            "unproved socket attached"
        );
        assert!(
            self.frames.iter().any(|x| x["d"]["accountName"] == ACCOUNT
                && x["d"]["instanceId"] == "negative-join-registration"
                && x["d"]["data"][1]["kind"] == "unverified"
                && x["d"]["data"][1]["transient"] == false),
            "typed proof refusal required"
        );
        assert!(
            !self
                .frames
                .iter()
                .any(|x| x["d"]["op"] == "account-signing-mode"),
            "refusal confirmed a registration"
        );
        assert_eq!(f.seen.lock().unwrap().writes, 0);
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(s) = self.session.take() {
            s.close();
        }
    }
}

pub fn seal(offer: &Value, init: &InitPayload, credentials: Value) -> Value {
    struct Fixed;
    impl hpke::rand_core::RngCore for Fixed {
        fn next_u32(&mut self) -> u32 {
            panic!("fill_bytes only")
        }
        fn next_u64(&mut self) -> u64 {
            panic!("fill_bytes only")
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(0x42);
        }
    }
    impl hpke::rand_core::CryptoRng for Fixed {}
    let public = <X25519HkdfSha256 as hpke::Kem>::PublicKey::from_bytes(
        &STANDARD
            .decode(offer["publicKey"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let (enc, mut sender) =
        hpke::setup_sender::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256, _>(
            &hpke::OpModeS::Base,
            &public,
            b"tealstreet/delegated-handover/v1",
            &mut Fixed,
        )
        .unwrap();
    let aad = format!(
        "{}|{}|{}|{}",
        offer["attemptId"].as_str().unwrap(),
        offer["bootId"].as_str().unwrap(),
        init.exchange_name,
        init.key
    );
    json!({"attemptId":offer["attemptId"], "enc":STANDARD.encode(enc.to_bytes()),
        "ciphertext":STANDARD.encode(sender.seal(credentials.to_string().as_bytes(), aad.as_bytes()).unwrap())})
}
async fn delegated_init<V: Venue>(client: &mut Client, credentials: Value) -> InitPayload {
    let offer = client.offer::<V>().await;
    let mut init = V::init();
    init.signing_mode = Some("delegated".into());
    init.sealed = Some(serde_json::from_value(seal(&offer, &init, credentials)).unwrap());
    init
}

struct Resident {
    client: Client,
    init: InitPayload,
    account: Arc<Account>,
    provider: Provider,
    holdings: BTreeMap<(String, String), testing::Held>,
    fingerprint: String,
    probe: SignRequest,
    signed: SignResult,
    owner: safe_cex_algo_twap::TwapOwner,
}
impl Resident {
    async fn new<V: Venue>(f: &Fixture) -> Self {
        let mut client = Client::new(f);
        let init = delegated_init::<V>(&mut client, V::credentials(false)).await;
        client.init(init.clone()).await;
        client
            .finish::<V>(&mut Device::default(), Answer::Correct)
            .await;
        let account = f
            .registry
            .held(USER, ACCOUNT, client.socket())
            .expect("valid pair must create a running resident");
        assert!(account.delegated().is_some());
        assert_eq!(f.registry.account_count(), 1);
        tokio::time::timeout(WAIT, async {
            loop {
                let loaded = account
                    .handle
                    .read(|store| {
                        let s = store.loaded();
                        s.markets && s.balance && s.orders && s.positions
                    })
                    .await
                    .unwrap();
                if loaded {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("legitimate resident fully loads before negative joins");
        let fingerprint = account.fingerprint.clone();
        let provider = account.delegated().unwrap().provider();
        let holdings = testing::holdings(&f.registry);
        let probe = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: V::algorithm(),
                key: KeyRef("primary.signing".into()),
                payload: vec![PayloadPart::Bytes { bytes: vec![3; 32] }],
            }],
        };
        let signed = provider.sign_ready(&probe).unwrap().unwrap();
        let owner = account.twap_owner.snapshot();
        Self {
            client,
            init,
            account,
            provider,
            holdings,
            fingerprint,
            probe,
            signed,
            owner,
        }
    }
    async fn untouched(&self, f: &Fixture) {
        let current = f
            .registry
            .held(USER, ACCOUNT, self.client.socket())
            .unwrap();
        assert!(Arc::ptr_eq(&current, &self.account));
        assert!(Arc::ptr_eq(
            &self.provider,
            &current.delegated().unwrap().provider()
        ));
        assert_eq!(current.fingerprint, self.fingerprint);
        assert_eq!(current.twap_owner.snapshot(), self.owner);
        let signed = self.provider.sign_ready(&self.probe).unwrap().unwrap();
        assert!(
            signed.values == self.signed.values,
            "resident signer changed or was erased"
        );
        assert_eq!(testing::holdings(&f.registry), self.holdings);
        assert!(current.handle.disposal_cause().is_none());
        assert!(!current.algos.as_ref().unwrap().is_closed());
        assert!(
            current.handle.read(|_| true).await.unwrap(),
            "resident actor remains live"
        );
        assert_eq!(f.registry.account_count(), 1);
        assert_eq!(f.seen.lock().unwrap().writes, 0);
    }
}

pub async fn cosigned_wrong_key<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let resident = Resident::new::<V>(&f).await;
    let mut client = Client::new(&f);
    client.init(V::init()).await;
    client
        .finish::<V>(&mut Device::default(), Answer::OtherKey)
        .await;
    client.assert_refused(&f);
    resident.untouched(&f).await;
    // Correct proof must still attach to that same resident, without replacing its provider.
    drop(client);
    let mut positive = Client::new(&f);
    positive.init(V::init()).await;
    positive
        .finish::<V>(&mut Device::default(), Answer::Correct)
        .await;
    assert!(Arc::ptr_eq(
        &f.registry.held(USER, ACCOUNT, positive.socket()).unwrap(),
        &resident.account
    ));
    drop(positive);
    resident.untouched(&f).await;
}

pub async fn cosigned_replay<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let resident = Resident::new::<V>(&f).await;
    let mut device = Device::default();
    let mut first = Client::new(&f);
    first.init(V::init()).await;
    first.finish::<V>(&mut device, Answer::Correct).await;
    assert!(f.registry.holds(&resident.account, first.socket()));
    drop(first);
    tokio::time::sleep(V::replay_delay()).await;
    let mut second = Client::new(&f);
    second.init(V::init()).await;
    second.finish::<V>(&mut device, Answer::Replay).await;
    assert!(device.replayed > 0);
    second.assert_refused(&f);
    resident.untouched(&f).await;
}

pub async fn cosigned_consumed<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let resident = Resident::new::<V>(&f).await;
    let mut device = Device::default();
    let mut client = Client::new(&f);
    client.init(V::init()).await;
    client.finish::<V>(&mut device, Answer::Correct).await;
    assert!(f.registry.holds(&resident.account, client.socket()));
    let holdings = testing::holdings(&f.registry);
    let confirmations = client
        .frames
        .iter()
        .filter(|x| x["d"]["op"] == "account-signing-mode")
        .count();
    assert_eq!(confirmations, 1);
    for reply in device.responses {
        client.send(json!({"t":"sign-res", "d":reply})).await;
    }
    tokio::task::yield_now().await;
    client.drain();
    assert!(
        client.done.try_recv().is_err(),
        "consumed replies started a second completion"
    );
    assert_eq!(testing::holdings(&f.registry), holdings);
    assert_eq!(
        client
            .frames
            .iter()
            .filter(|x| x["d"]["op"] == "account-signing-mode")
            .count(),
        confirmations
    );
    drop(client);
    resident.untouched(&f).await;
}

pub async fn delegated_wrong_pair<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let resident = Resident::new::<V>(&f).await;
    // An independently valid pair cannot claim the original public init key.
    // A matching public key with a different signing secret must also fail proof.
    for mismatched_key in [true, false] {
        let mut client = Client::new(&f);
        let mut credentials = V::credentials(true);
        if !mismatched_key {
            credentials["key"] = V::init().key.into();
        }
        let init = delegated_init::<V>(&mut client, credentials).await;
        client.init(init).await;
        client
            .finish::<V>(&mut Device::default(), Answer::Correct)
            .await;
        client.assert_refused(&f);
        resident.untouched(&f).await;
    }
}

pub async fn delegated_replay<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let resident = Resident::new::<V>(&f).await;
    for rebind_id in [false, true] {
        let mut client = Client::new(&f);
        let mut init = delegated_init::<V>(&mut client, V::credentials(false)).await;
        let first = init.sealed.as_ref().unwrap().attempt_id.clone();
        let second = client.offer::<V>().await;
        assert_ne!(first, second["attemptId"].as_str().unwrap());
        if rebind_id {
            init.sealed.as_mut().unwrap().attempt_id = second["attemptId"].as_str().unwrap().into();
        }
        client.init(init).await;
        client
            .finish::<V>(&mut Device::default(), Answer::Correct)
            .await;
        client.assert_refused(&f);
        resident.untouched(&f).await;
    }
}

// Establish that the "other" fixture really passes production admission for its own identity.
pub async fn independent_pair<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let mut client = Client::new(&f);
    let credentials = V::credentials(true);
    let mut init = V::init();
    init.key = credentials["key"].as_str().unwrap().into();
    if init.hyperliquid_signer_address.is_some() {
        init.hyperliquid_signer_address = Some(init.key.clone());
    }
    init.signing_mode = Some("delegated".into());
    let offer = client.offer::<V>().await;
    init.sealed = Some(serde_json::from_value(seal(&offer, &init, credentials)).unwrap());
    client.init(init).await;
    client
        .finish::<V>(&mut Device::default(), Answer::Correct)
        .await;
    assert!(
        f.registry.held(USER, ACCOUNT, client.socket()).is_some(),
        "second pair must be independently valid"
    );
    assert_eq!(f.registry.account_count(), 1);
    assert_eq!(f.seen.lock().unwrap().writes, 0);
}

pub async fn delegated_consumed<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let mut resident = Resident::new::<V>(&f).await;
    resident.client.frames.clear();
    resident.client.init(resident.init.clone()).await;
    resident
        .client
        .finish::<V>(&mut Device::default(), Answer::Correct)
        .await;
    assert_permanent_bound_refusal(&resident.client.frames);
    resident.untouched(&f).await;
}

fn assert_permanent_bound_refusal(frames: &[Value]) {
    assert!(
        frames
            .iter()
            .any(|x| x["d"]["data"][1]["kind"] == "unverified")
    );
    assert!(
        !frames
            .iter()
            .any(|x| x["d"]["op"] == "account-signing-mode")
    );
}

#[test]
fn consumed_refusal_requires_permanent_original_binding() {
    let mut expected: Value = serde_json::from_str(
        &safe_cex_exchange_server::envelope::init_unverified(ACCOUNT, false),
    )
    .unwrap();
    expected["d"]["instanceId"] = "negative-join-registration".into();
    assert_permanent_bound_refusal(std::slice::from_ref(&expected));
    for field in ["transient", "accountName", "instanceId", "missing-instance"] {
        let mut bad = expected.clone();
        match field {
            "transient" => bad["d"]["data"][1]["transient"] = true.into(),
            "accountName" => bad["d"]["accountName"] = "foreign-account".into(),
            "instanceId" => bad["d"]["instanceId"] = "foreign-registration".into(),
            _ => {
                bad["d"].as_object_mut().unwrap().remove("instanceId");
            }
        }
        assert!(
            std::panic::catch_unwind(|| assert_permanent_bound_refusal(&[bad])).is_err(),
            "consumed handover accepted invalid refusal field {field}"
        );
    }
}
