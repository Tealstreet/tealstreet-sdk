use super::*;
use safe_cex_remote_signer::SignerLink;

// The image workflow runs `cargo test -p safe-cex-exchange-server` alone: its
// tests compete at full intra-crate parallelism and can starve each other,
// unlike Rust / native test, which spreads the load across the workspace run.
// Two different Resident::start callers exceeded five seconds there. The handle
// exposes loaded state through read(), not a load-completion receiver, so keep
// the existing readiness check with scheduling headroom and a bounded wait.
// Loading is a fixture precondition, not a latency assertion.
const ADMITTED_ACCOUNT_LOAD_TIMEOUT: Duration = Duration::from_secs(30);

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

#[tokio::test]
async fn cosigned_probe_deadline_refuses_and_releases_its_engine() {
    let f = fixture_with_auth(true).await;
    f.gate.forget_permits(1000);
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut done = session.take_verified();
    session.init(init(Some("timed-proof"))).await.unwrap();
    let link = session.pending_verifications["acct"]
        .cosigned_link
        .clone()
        .unwrap();
    let mut device = Device::default();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.private_reads.load(Ordering::SeqCst) == 0 {
            tokio::select! {
                Some(text) = sent.recv() => {
                    device.answer(&mut session, serde_json::from_str(&text).unwrap(), Answer::Correct).await;
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
    })
    .await
    .expect("the signed private read is held by the fixture");

    tokio::time::pause();
    let budget = safe_cex_remote_signer::SIGN_TIMEOUT
        + Duration::from_millis(u64::from(
            safe_cex_engine::request_budget::RequestClass::Read.budget_ms(),
        ));
    tokio::time::advance(budget + Duration::from_millis(1)).await;
    let completion = done.recv().await.unwrap();
    tokio::time::resume();
    // Discard other challenges already queued by the temporary startup.
    while sent.try_recv().is_ok() {}
    session.verification_completed(completion).await.unwrap();
    assert_eq!(
        sent.try_recv().unwrap(),
        envelope::with_instance_id(envelope::init_unverified("acct", true), "timed-proof")
    );
    assert!(!link.is_available());
    assert!(session.pending_verifications.is_empty());
    assert!(session.joined.is_empty());
    assert!(f.registry.held("alice", "acct", session.socket).is_none());
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    tokio::time::timeout(Duration::from_secs(5), async {
        while crate::accounts::testing::engine_count(&f.registry) != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("probe actor disposed");
    session.close();
}

#[tokio::test]
async fn cosigned_rpc_expiry_completion_preserves_request_and_registration() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let mut resident = Resident::start(&f, &mut device).await;
    assert_rpc_expiry_completion(
        &f,
        resident.session.as_mut().unwrap(),
        &mut resident._sent,
        Some("resident"),
    )
    .await;
    resident.untouched(&f);
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
        assert!(result.probe.as_ref().unwrap().permit.is_ok());
        session.verification_completed(result).await.unwrap();
        let account = f.registry.held("alice", "acct", session.socket).unwrap();
        assert!(account.delegated.is_none());
        tokio::time::timeout(ADMITTED_ACCOUNT_LOAD_TIMEOUT, async {
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
    unattached_after_refusal(f, session, sent, expected);
}

fn unattached_after_refusal(
    f: &Fixture,
    session: &Session,
    sent: &mut crate::out::Receiver,
    expected: String,
) {
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
                assert!(stale.probe.as_ref().unwrap().permit.is_ok());
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
            assert!(result.probe.as_ref().unwrap().permit.is_err());
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
            assert!(result.probe.as_ref().unwrap().permit.is_ok());
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
async fn cosigned_test_admission_does_not_start_unhosted_venue_proofs() {
    let mut f = fixture_with_auth(true).await;
    crate::accounts::testing::set_join_admission(Arc::get_mut(&mut f.registry).unwrap(), true);
    let mut device = Device::default();
    let resident = Resident::start(&f, &mut device).await;
    let before = f.private_reads.load(Ordering::SeqCst);
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut request = init(Some("other-venue"));
    request.exchange_name = "hyrotrader".into();
    session.init(request).await.unwrap();
    unattached_after_refusal(
        &f,
        &session,
        &mut sent,
        envelope::with_instance_id(envelope::init_proof_unavailable("acct"), "other-venue"),
    );
    assert_eq!(f.private_reads.load(Ordering::SeqCst), before);
    resident.untouched(&f);
    one_actor(&f).await;
    session.close();
}

#[tokio::test]
async fn shared_admission_keeps_pending_operations_isolated() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let resident = Resident::start(&f, &mut device).await;
    for mode in [Some("cosigned"), Some("local"), None] {
        let (mut session, mut sent) = candidate(&f, "alice");
        let mut done = session.take_verified();
        let mut request = init(Some("joining"));
        request.signing_mode = mode.map(str::to_owned);
        session.init(request).await.unwrap();
        assert!(session.pending_verifications.contains_key("acct"));
        assert!(session.joined.is_empty());
        assert!(f.registry.held("alice", "acct", session.socket).is_none());
        for op in [
            "sendSnapshot",
            "callMethod",
            "chaser:start",
            "twap:state",
            "exec",
            "subscribeOHLCV",
        ] {
            session
                .msg(AccountMsg {
                    op: op.into(),
                    account_name: Some("acct".into()),
                    data: json!({"requestId":7,"method":"nuke","args":[]}),
                })
                .await;
            let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
            if op == "callMethod" {
                assert_eq!(frame["d"]["data"]["result"]["message"], "verifying");
            } else {
                assert_eq!(frame["d"]["data"][0], "verifying");
            }
            assert_eq!(frame["d"]["instanceId"], "joining");
        }
        assert!(session.early.is_empty());
        resident.untouched(&f);
        let result = complete(
            &mut session,
            &mut sent,
            &mut done,
            &mut device,
            Answer::Correct,
            Some("joining"),
        )
        .await;
        resident.untouched(&f);
        session.verification_completed(result).await.unwrap();
        assert!(Arc::ptr_eq(
            &resident.account,
            &session.joined["acct"].account
        ));
        assert_eq!(resident.link.socket(), session.socket);
        session.close();
        resident.untouched(&f);
    }
}

#[tokio::test]
async fn shared_admission_checks_each_device_response() {
    for answer in [Answer::OtherKey, Answer::Replay] {
        let f = fixture_with_auth(true).await;
        let mut device = Device::default();
        let resident = Resident::start(&f, &mut device).await;
        let (mut session, mut sent) = candidate(&f, "alice");
        let mut done = session.take_verified();
        tokio::time::sleep(Duration::from_millis(2)).await;
        session.init(init(None)).await.unwrap();
        assert!(session.pending_verifications.contains_key("acct"));
        let link = session.pending_verifications["acct"]
            .cosigned_link
            .clone()
            .unwrap();
        let result = complete(
            &mut session,
            &mut sent,
            &mut done,
            &mut device,
            answer,
            None,
        )
        .await;
        session.verification_completed(result).await.unwrap();
        refused(&f, &session, &mut sent, None);
        assert!(!link.is_available());
        assert!(f.rejected.load(Ordering::SeqCst) > 0);
        resident.untouched(&f);
        one_actor(&f).await;
        session.close();
    }
}

#[tokio::test]
async fn shared_admission_rechecks_token_before_attachment() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let resident = Resident::start(&f, &mut device).await;
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut done = session.take_verified();
    session.init(init(None)).await.unwrap();
    assert!(session.pending_verifications.contains_key("acct"));
    let result = complete(
        &mut session,
        &mut sent,
        &mut done,
        &mut device,
        Answer::Correct,
        None,
    )
    .await;
    session.identity.token = Some(("synthetic-expired".into(), 0));
    session.verification_completed(result).await.unwrap();
    unattached_after_refusal(
        &f,
        &session,
        &mut sent,
        envelope::error(Some("acct"), crate::auth::TOKEN_EXPIRED),
    );
    resident.untouched(&f);
    session.close();
}

#[tokio::test]
async fn shared_admission_retains_the_cosigned_resume_window() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let mut resident = Resident::start(&f, &mut device).await;
    let mut previous = resident.session.take().unwrap();
    previous.identity.resumable = true;
    let from = previous.resume_id.clone();
    previous.close();
    let (mut elapsed, _elapsed_sent) = candidate(&f, "alice");
    elapsed.identity.resumable = true;
    elapsed.identity.resume_from = Some(from.clone());
    elapsed.identity.token = Some(("synthetic-expired".into(), 0));
    elapsed.open().await;
    assert!(elapsed.joined.is_empty());
    assert!(f.registry.held("alice", "acct", elapsed.socket).is_none());
    elapsed.close();
    let (mut resumed, _sent) = candidate(&f, "alice");
    resumed.identity.resumable = true;
    resumed.identity.resume_from = Some(from);
    let mut done = resumed.take_verified();
    resumed.open().await;
    assert!(Arc::ptr_eq(
        &resident.account,
        &resumed.joined["acct"].account
    ));
    resumed.init(init(Some("resumed"))).await.unwrap();
    assert!(resumed.pending_verifications.is_empty());
    assert!(done.try_recv().is_err());
    assert_eq!(resident.link.socket(), resumed.socket);
    assert!(Arc::ptr_eq(
        &resident.account,
        &resumed.joined["acct"].account
    ));
    resumed.identity.resumable = false;
    resumed.close();
}

#[tokio::test]
async fn shared_admission_dispose_retires_queued_signing_output() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let resident = Resident::start(&f, &mut device).await;
    let (mut session, mut sent) = candidate(&f, "alice");
    let mut done = session.take_verified();
    session.init(init(Some("pending"))).await.unwrap();
    assert!(session.pending_verifications.contains_key("acct"));
    tokio::time::timeout(Duration::from_secs(5), async {
        while sent.queued() == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the probe queued its signing request");
    session
        .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
        .await
        .unwrap();
    assert!(
        sent.try_recv().is_err(),
        "retired admission output is dropped at dequeue"
    );
    assert!(done.try_recv().is_err());
    resident.untouched(&f);
    one_actor(&f).await;
    session.close();
}

#[tokio::test]
async fn shared_admission_delegated_resume_uses_a_pending_init() {
    let f = fixture_with_auth(true).await;
    let (mut resident, _resident_sent) = user_session(f.registry.clone(), "alice");
    let mut ready = resident.take_verified();
    let request = super::super::payload(&mut resident, Some("resident"));
    resident.init(request).await.unwrap();
    super::super::finish(&mut resident, &mut ready).await;
    let account = resident.joined["acct"].account.clone();
    let from = resident.resume_id.clone();
    resident.close();
    let (mut resumed, mut sent) = candidate(&f, "alice");
    resumed.identity.resumable = true;
    resumed.identity.resume_from = Some(from);
    let mut done = resumed.take_verified();
    resumed.open().await;
    assert!(resumed.joined.is_empty());
    assert!(f.registry.held("alice", "acct", resumed.socket).is_none());
    assert!(account.handle.disposal_cause().is_none());
    let session: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
    assert_eq!(session["t"], "session");
    assert_eq!(session["d"]["resumed"], false);
    assert!(sent.try_recv().is_err());
    let request = super::super::payload(&mut resumed, Some("admitted"));
    resumed.init(request).await.unwrap();
    assert!(resumed.pending_verifications.contains_key("acct"));
    assert!(resumed.joined.is_empty());
    super::super::finish(&mut resumed, &mut done).await;
    assert!(Arc::ptr_eq(&account, &resumed.joined["acct"].account));
    resumed.identity.resumable = false;
    resumed.close();
}

#[tokio::test]
async fn shared_admission_rechecks_token_on_a_matching_init() {
    let f = fixture_with_auth(true).await;
    let mut device = Device::default();
    let mut resident = Resident::start(&f, &mut device).await;
    while resident._sent.try_recv().is_ok() {}
    let session = resident.session.as_mut().unwrap();
    session.identity.token = Some(("synthetic-expired".into(), 0));
    session.init(init(Some("elapsed"))).await.unwrap();
    assert!(session.pending_verifications.is_empty());
    let expected = envelope::with_instance_id(
        envelope::error(Some("acct"), crate::auth::TOKEN_EXPIRED),
        "elapsed",
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let text = resident
                ._sent
                .recv()
                .await
                .expect("resident output remains open");
            if text == expected {
                break;
            }
            // The resident's private startup may still publish after the earlier drain.
            // Other resident output keeps the admitted registration.
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(frame["d"]["accountName"], "acct");
            assert_eq!(frame["d"]["instanceId"], "resident");
        }
    })
    .await
    .expect("expired matching init emits its exact refusal");
    assert!(
        resident.session.as_ref().unwrap().joined["acct"]
            .out
            .order_binding(Some("resident"))
    );
    resident.untouched(&f);
}

#[tokio::test]
async fn shared_admission_uses_a_probe_after_the_resume_window() {
    let mut f = fixture_with_auth(true).await;
    Arc::get_mut(&mut f.registry).unwrap().resume_grace = Duration::from_millis(1);
    let mut device = Device::default();
    let mut resident = Resident::start(&f, &mut device).await;
    let mut previous = resident.session.take().unwrap();
    previous.identity.resumable = true;
    let from = previous.resume_id.clone();
    previous.close();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.registry.account_count() != 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("the previous resume window elapsed");
    let (mut session, _sent) = candidate(&f, "alice");
    session.identity.resumable = true;
    session.identity.resume_from = Some(from);
    session.open().await;
    assert!(session.joined.is_empty());
    session.init(init(Some("fresh"))).await.unwrap();
    assert!(session.pending_verifications.contains_key("acct"));
    assert!(f.registry.held("alice", "acct", session.socket).is_none());
    session.identity.resumable = false;
    session.close();
}
