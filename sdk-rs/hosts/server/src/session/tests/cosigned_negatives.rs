use super::*;
use safe_cex_remote_signer::SignerLink;

#[derive(Clone, Copy)]
enum Answer {
    Correct,
    OtherKey,
    Replay,
}

#[derive(Default)]
struct Device {
    signed: HashMap<String, String>,
    responses: Vec<Value>,
    replayed: usize,
}

impl Device {
    async fn answer(&mut self, session: &mut Session, frame: Value, answer: Answer) {
        assert_eq!(frame["t"], "sign-req");
        let request = &frame["d"];
        let preimage = request["wsAuth"]["toSign"].as_str().unwrap();
        let ws = preimage.starts_with("GET/realtime");
        let secret = if !ws && matches!(answer, Answer::OtherKey) {
            OTHER_SECRET
        } else {
            SECRET
        };
        let fresh = hmac_sha256(secret.as_bytes(), preimage.as_bytes(), Encoding::Hex);
        let operation = preimage
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .to_owned();
        let signature = if !ws && matches!(answer, Answer::Replay) {
            let prior = self
                .signed
                .get(&operation)
                .expect("accepted attempt precedes replay");
            assert!(prior != &fresh, "attempts have different preimages");
            self.replayed += 1;
            prior.clone()
        } else {
            if !ws {
                self.signed.insert(operation, fresh.clone());
            }
            fresh
        };
        let response = json!({"id":request["id"], "wsAuthSignature":signature});
        let link = session
            .pending_verifications
            .get("acct")
            .and_then(|p| p.cosigned_link.clone())
            .or_else(|| {
                session
                    .joined
                    .get("acct")
                    .and_then(|j| j.account.link.clone())
            })
            .unwrap();
        assert_eq!(link.socket(), session.socket);
        session
            .handle(&json!({"t":"sign-res", "d":response}).to_string())
            .await
            .unwrap();
        assert!(
            !link.resolve(session.socket, response.clone()),
            "response was consumed by Session"
        );
        session
            .handle(&json!({"t":"sign-res", "d":response}).to_string())
            .await
            .unwrap();
        self.responses.push(response);
    }
}

fn init(id: Option<&str>) -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: KEY.into(),
        signing_mode: Some("cosigned".into()),
        instance_id: id.map(str::to_owned),
        ..Default::default()
    }
}

fn candidate(f: &Fixture, user: &str) -> (Session, crate::out::Receiver) {
    let (mut session, sent) = user_session(f.registry.clone(), user);
    session.cosigned_admission_test = true;
    session.identity.resumable = false;
    (session, sent)
}

async fn complete(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
    device: &mut Device,
    answer: Answer,
    id: Option<&str>,
) -> JoinCompletion {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                biased;
                Some(text) = sent.recv() => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(frame["t"], "sign-req", "pending Session only gets its challenge");
                    assert_eq!(frame["d"].get("instanceId").and_then(Value::as_str), id.filter(|s| !s.is_empty()));
                    device.answer(session, frame, answer).await;
                }
                completion = done.recv() => return completion.unwrap(),
            }
        }
    }).await.expect("Session proof completes")
}

struct Resident {
    session: Option<Session>,
    account: Arc<Account>,
    link: Arc<SignerLink>,
    fingerprint: String,
    holders: Vec<u64>,
    subscriptions: HashMap<String, u32>,
    counts: (usize, usize),
    _sent: crate::out::Receiver,
}

impl Resident {
    async fn start(f: &Fixture, device: &mut Device) -> Self {
        let (mut session, mut sent) = candidate(f, "alice");
        let mut done = session.take_verified();
        session.init(init(Some("resident"))).await.unwrap();
        assert!(f.registry.held("alice", "acct", session.socket).is_none());
        let result = complete(
            &mut session,
            &mut sent,
            &mut done,
            device,
            Answer::Correct,
            Some("resident"),
        )
        .await;
        assert!(result.cosigned.as_ref().unwrap().proof.is_ok());
        session.verification_completed(result).await.unwrap();
        let account = f.registry.held("alice", "acct", session.socket).unwrap();
        assert!(account.delegated.is_none());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let loaded = account.handle.read(|store| {
                    let s = store.loaded(); s.markets && s.balance && s.orders && s.positions
                }).await.unwrap();
                if loaded { break; }
                tokio::select! {
                    Some(text) = sent.recv() => {
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        if frame["t"] == "sign-req" { device.answer(&mut session, frame, Answer::Correct).await; }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(1)) => {},
                }
            }
        }).await.expect("admitted COSIGNED account loads");
        session
            .msg(AccountMsg {
                op: "subscribeOHLCV".into(),
                data: json!({"symbol":"BTCUSDT", "interval":"1m"}),
                account_name: Some("acct".into()),
            })
            .await;
        let subscriptions = session.joined["acct"].subscriptions.clone();
        assert!(!subscriptions.is_empty());
        let mut holders = account.live_holders_besides(0);
        holders.sort_unstable();
        Self {
            link: account.link.clone().unwrap(),
            fingerprint: account.fingerprint.clone(),
            counts: account.running.counts(),
            account,
            session: Some(session),
            holders,
            subscriptions,
            _sent: sent,
        }
    }

    fn untouched(&self, f: &Fixture) {
        let session = self.session.as_ref().unwrap();
        let account = f.registry.held("alice", "acct", session.socket).unwrap();
        assert!(Arc::ptr_eq(&account, &self.account));
        assert!(Arc::ptr_eq(account.link.as_ref().unwrap(), &self.link));
        assert!(account.delegated.is_none());
        assert_eq!(self.link.socket(), session.socket);
        assert_eq!(account.fingerprint, self.fingerprint);
        assert!(account.handle.disposal_cause().is_none());
        assert!(!account.algos.as_ref().unwrap().is_closed());
        assert_eq!(account.running.counts(), self.counts);
        let mut holders = account.live_holders_besides(0);
        holders.sort_unstable();
        assert_eq!(holders, self.holders);
        assert_eq!(session.joined["acct"].subscriptions, self.subscriptions);
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.close();
        }
    }
}

fn refused(f: &Fixture, session: &Session, sent: &mut crate::out::Receiver, id: Option<&str>) {
    let raw = envelope::init_unverified("acct", false);
    let expected = id.filter(|s| !s.is_empty()).map_or_else(
        || raw.clone(),
        |id| envelope::with_instance_id(raw.clone(), id),
    );
    assert_eq!(sent.try_recv().unwrap(), expected);
    assert!(sent.try_recv().is_err());
    assert!(f.registry.held("alice", "acct", session.socket).is_none());
    assert!(session.pending_verifications.is_empty());
    assert!(session.joined.is_empty());
    assert!(session.early.is_empty());
}

async fn one_actor(f: &Fixture) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while crate::accounts::testing::engine_count(&f.registry) != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("temporary proof actor is gone");
    assert_eq!(f.registry.account_count(), 1);
}

#[tokio::test]
async fn cosigned_session_negatives_preserve_the_running_account() {
    for case in ["other-key", "replay", "consumed"] {
        for id in [Some("negative-registration"), None, Some("")] {
            let f = fixture_with_auth(true).await;
            let mut device = Device::default();
            let resident = Resident::start(&f, &mut device).await;
            let (mut session, mut sent) = candidate(&f, "alice");
            let mut done = session.take_verified();
            tokio::time::sleep(Duration::from_millis(2)).await;
            session.init(init(id)).await.unwrap();
            assert!(f.registry.held("alice", "acct", session.socket).is_none());
            if case == "consumed" {
                let stale = complete(
                    &mut session,
                    &mut sent,
                    &mut done,
                    &mut device,
                    Answer::Correct,
                    id,
                )
                .await;
                assert!(stale.cosigned.as_ref().unwrap().proof.is_ok());
                let old_link = session.pending_verifications["acct"]
                    .cosigned_link
                    .clone()
                    .unwrap();
                let old_response = device.responses.last().unwrap().clone();
                session.init(init(id)).await.unwrap();
                let generation = session.pending_verifications["acct"].generation;
                assert_ne!(generation, stale.generation);
                assert!(!old_link.is_available());
                session.verification_completed(stale).await.unwrap();
                session
                    .handle(&json!({"t":"sign-res", "d":old_response}).to_string())
                    .await
                    .unwrap();
                assert_eq!(session.pending_verifications["acct"].generation, generation);
                assert!(done.try_recv().is_err());
                assert!(f.registry.held("alice", "acct", session.socket).is_none());
                resident.untouched(&f);
            }
            let pending_link = session.pending_verifications["acct"]
                .cosigned_link
                .clone()
                .unwrap();
            let answer = if case == "replay" {
                Answer::Replay
            } else {
                Answer::OtherKey
            };
            let result =
                complete(&mut session, &mut sent, &mut done, &mut device, answer, id).await;
            assert!(result.cosigned.as_ref().unwrap().proof.is_err());
            assert!(
                result.candidate.is_err(),
                "COSIGNED completion retains no credential copy"
            );
            session.verification_completed(result).await.unwrap();
            refused(&f, &session, &mut sent, id);
            assert!(!pending_link.is_available());
            assert!(done.try_recv().is_err());
            assert!(f.rejected.load(Ordering::SeqCst) > 0);
            if case == "replay" {
                assert!(device.replayed > 0);
            }
            resident.untouched(&f);
            one_actor(&f).await;
            session.close();

            let (mut fresh, mut sent) = candidate(&f, "alice");
            let mut done = fresh.take_verified();
            fresh.init(init(Some("fresh"))).await.unwrap();
            let result = complete(
                &mut fresh,
                &mut sent,
                &mut done,
                &mut device,
                Answer::Correct,
                Some("fresh"),
            )
            .await;
            assert!(result.cosigned.as_ref().unwrap().proof.is_ok());
            resident.untouched(&f);
            fresh.verification_completed(result).await.unwrap();
            assert!(Arc::ptr_eq(
                &resident.account,
                &f.registry.held("alice", "acct", fresh.socket).unwrap()
            ));
            assert_eq!(resident.link.socket(), fresh.socket);
            fresh.close();
            resident.untouched(&f);
            one_actor(&f).await;
        }
    }
}

#[tokio::test]
async fn cosigned_two_users_switch_during_deferred_private_reads() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let resident = Resident::start(&f, &mut device).await;
    f.gate.forget_permits(f.gate.available_permits());
    let before = f.private_reads.load(Ordering::SeqCst);
    let (mut active, mut a_sent) = candidate(&f, "alice");
    let mut a_done = active.take_verified();
    active.init(init(Some("alice"))).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.private_reads.load(Ordering::SeqCst) == before {
            tokio::select! {
                Some(text) = a_sent.recv() => {
                    device.answer(&mut active, serde_json::from_str(&text).unwrap(), Answer::Correct).await;
                }
                _ = tokio::time::sleep(Duration::from_millis(1)) => {},
            }
        }
    }).await.expect("A is deferred inside actual private HTTP");
    let (bob, mut b_sent) = candidate(&f, "bob");
    let mut alice = std::mem::replace(&mut active, bob);
    let mut b_done = active.take_verified();
    active.init(init(Some("bob"))).await.unwrap();
    let generation = active.pending_verifications["acct"].generation;
    f.gate.add_permits(1000);
    let late = complete(
        &mut alice,
        &mut a_sent,
        &mut a_done,
        &mut device,
        Answer::Correct,
        Some("alice"),
    )
    .await;
    active.verification_completed(late).await.unwrap();
    assert_eq!(active.pending_verifications["acct"].generation, generation);
    assert!(f.registry.held("bob", "acct", active.socket).is_none());
    alice.close();
    resident.untouched(&f);
    let result = complete(
        &mut active,
        &mut b_sent,
        &mut b_done,
        &mut device,
        Answer::Correct,
        Some("bob"),
    )
    .await;
    active.verification_completed(result).await.unwrap();
    let bob = f.registry.held("bob", "acct", active.socket).unwrap();
    assert_eq!(bob.user, "bob");
    assert!(!Arc::ptr_eq(&resident.account, &bob));
    resident.untouched(&f);
    active.close();
    one_actor(&f).await;
}

#[tokio::test]
async fn cosigned_test_admission_does_not_start_other_venue_proofs() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let resident = Resident::start(&f, &mut device).await;
    let before = f.private_reads.load(Ordering::SeqCst);
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut request = init(Some("other-venue"));
    request.exchange_name = "bybitspot".into();
    session.init(request).await.unwrap();
    refused(&f, &session, &mut sent, Some("other-venue"));
    assert_eq!(f.private_reads.load(Ordering::SeqCst), before);
    resident.untouched(&f);
    one_actor(&f).await;
    session.close();
}
