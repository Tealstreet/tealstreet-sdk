use super::*;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Provider;
use safe_cex_native::{NetworkTransport, ServerConfig};
use safe_cex_remote_signer::{SignerLink, remote_account};

const KEY: &str = "synthetic-key";
const SECRET: &str = "synthetic-secret";
const OTHER_KEY: &str = "synthetic-other-key";
const OTHER_SECRET: &str = "synthetic-other-secret";

pub(super) fn signed_read_is_valid(uri: &Uri, headers: &HeaderMap) -> bool {
    let header = |name| headers.get(name).and_then(|v| v.to_str().ok());
    let Some(key) = header("x-bapi-api-key") else {
        return false;
    };
    let secret = match key {
        KEY => SECRET,
        OTHER_KEY => OTHER_SECRET,
        _ => return false,
    };
    let (Some(stamp), Some(window), Some(signature)) = (
        header("x-bapi-timestamp"),
        header("x-bapi-recv-window"),
        header("x-bapi-sign"),
    ) else {
        return false;
    };
    let payload = format!("{stamp}{key}{window}{}", uri.query().unwrap_or_default());
    signature == hmac_sha256(secret.as_bytes(), payload.as_bytes(), Encoding::Hex)
}

#[test]
fn signature_fixture_checks_the_complete_signed_read() {
    let uri: Uri = "/v5/account/wallet-balance?accountType=UNIFIED"
        .parse()
        .unwrap();
    let signature = hmac_sha256(
        SECRET.as_bytes(),
        b"1700000000000synthetic-key20000accountType=UNIFIED",
        Encoding::Hex,
    );
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("x-bapi-api-key", KEY),
        ("x-bapi-timestamp", "1700000000000"),
        ("x-bapi-recv-window", "20000"),
        ("x-bapi-sign", signature.as_str()),
    ] {
        headers.insert(
            axum::http::HeaderName::from_static(name),
            value.parse().unwrap(),
        );
    }
    assert!(signed_read_is_valid(&uri, &headers));
    for (name, value) in [
        ("x-bapi-api-key", OTHER_KEY),
        ("x-bapi-timestamp", "1700000000001"),
        ("x-bapi-recv-window", "10000"),
        ("x-bapi-sign", "invalid"),
    ] {
        let mut changed = headers.clone();
        changed.insert(
            axum::http::HeaderName::from_static(name),
            value.parse().unwrap(),
        );
        assert!(!signed_read_is_valid(&uri, &changed));
    }
    let changed: Uri = "/v5/account/wallet-balance?accountType=CONTRACT"
        .parse()
        .unwrap();
    assert!(!signed_read_is_valid(&changed, &headers));
}

struct RunningAccount {
    session: Option<Session>,
    account: Arc<Account>,
    provider: Provider,
    fingerprint: String,
    holders: Vec<u64>,
    subscriptions: HashMap<String, u32>,
    counts: (usize, usize),
    _sent: tokio::sync::mpsc::Receiver<String>,
}

impl RunningAccount {
    async fn start(f: &Fixture) -> Self {
        let (mut session, sent) = user_session(f.registry.clone(), "alice");
        let mut done = session.take_verified();
        let init = payload(&mut session, Some("resident"));
        session.init(init).await.unwrap();
        finish(&mut session, &mut done).await;
        let account = f.registry.held("alice", "acct", session.socket).unwrap();
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
            provider: account.delegated.as_ref().unwrap().provider(),
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
        let current = f.registry.held("alice", "acct", session.socket).unwrap();
        assert!(Arc::ptr_eq(&current, &self.account));
        assert!(Arc::ptr_eq(
            &self.provider,
            &current.delegated.as_ref().unwrap().provider()
        ));
        assert_eq!(current.fingerprint, self.fingerprint);
        assert!(current.handle.disposal_cause().is_none());
        assert!(!current.algos.as_ref().unwrap().is_closed());
        assert_eq!(current.running.counts(), self.counts);
        let mut holders = current.live_holders_besides(0);
        holders.sort_unstable();
        assert_eq!(holders, self.holders);
        assert_eq!(session.joined["acct"].subscriptions, self.subscriptions);
    }
}
impl Drop for RunningAccount {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            session.close();
        }
    }
}

fn handover(session: &mut Session, key: &str, secret: &str, id: Option<&str>) -> InitPayload {
    let offer = session
        .handovers
        .issue("acct", crate::accounts::now_ms())
        .unwrap();
    let sealed = crate::delegated::tests::seal(
        &offer,
        "bybit",
        key,
        json!({"key": key, "secret": secret}).to_string().as_bytes(),
    );
    InitPayload {
        key: key.into(),
        signing_mode: Some("delegated".into()),
        sealed: Some(sealed),
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        instance_id: id.map(str::to_owned),
        ..Default::default()
    }
}

#[tokio::test]
async fn delegated_candidates_preserve_the_running_account() {
    for case in ["wrong-secret", "other-pair", "consumed", "new-attempt"] {
        for id in [Some("negative-registration"), None, Some("")] {
            let f = fixture_with_auth(true).await;
            let resident = RunningAccount::start(&f).await;
            let (mut candidate, mut sent) = user_session(f.registry.clone(), "alice");
            let mut done = candidate.take_verified();
            let mut init = handover(&mut candidate, KEY, SECRET, id);
            match case {
                "wrong-secret" => init = handover(&mut candidate, KEY, OTHER_SECRET, id),
                "other-pair" => init = handover(&mut candidate, OTHER_KEY, OTHER_SECRET, id),
                "consumed" => {
                    let sealed = init.sealed.as_ref().unwrap();
                    let opening = candidate
                        .handovers
                        .take("acct", sealed, crate::accounts::now_ms())
                        .unwrap();
                    assert!(
                        opening
                            .open("bybit", KEY, sealed, crate::accounts::now_ms())
                            .is_ok()
                    );
                }
                "new-attempt" => {
                    let fresh = candidate
                        .handovers
                        .issue("acct", crate::accounts::now_ms())
                        .unwrap();
                    init.sealed.as_mut().unwrap().attempt_id = fresh.attempt_id;
                }
                _ => unreachable!(),
            }
            candidate.init(init).await.unwrap();
            if candidate.pending_verifications.contains_key("acct") {
                let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(completion.candidate.is_ok(), case == "other-pair");
                if case == "other-pair" {
                    assert!(completion.probe.as_ref().unwrap().permit.is_ok());
                }
                candidate.verification_completed(completion).await.unwrap();
            }
            assert!(
                f.registry.held("alice", "acct", candidate.socket).is_none(),
                "candidate refused"
            );
            let expected = envelope::init_unverified("acct", false);
            let expected = id.filter(|s| !s.is_empty()).map_or_else(
                || expected.clone(),
                |id| envelope::with_instance_id(expected.clone(), id),
            );
            assert_eq!(sent.try_recv().unwrap(), expected);
            resident.untouched(&f);
            if case == "wrong-secret" {
                assert!(f.rejected.load(Ordering::SeqCst) > 0);
            }
            candidate.close();
        }
    }
}

#[derive(Clone, Copy)]
enum DeviceAnswer {
    Correct,
    OtherKey,
    Replay,
}

#[derive(Default)]
struct Proofs {
    signed: HashMap<String, String>,
    replayed: usize,
    consumed: usize,
}

async fn remote_proof(f: &Fixture, answer: DeviceAnswer, proofs: Arc<Mutex<Proofs>>) -> bool {
    let (requests, mut received) = tokio::sync::mpsc::unbounded_channel();
    let link = SignerLink::new(901, Arc::new(move |text| requests.send(text).is_ok()));
    let device = link.clone();
    let responder = tokio::spawn(async move {
        while let Some(text) = received.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let request = &frame["d"];
            let preimage = request["wsAuth"]["toSign"].as_str().unwrap();
            let ws = preimage.starts_with("GET/realtime");
            let secret = if !ws && matches!(answer, DeviceAnswer::OtherKey) {
                OTHER_SECRET
            } else {
                SECRET
            };
            let fresh = hmac_sha256(secret.as_bytes(), preimage.as_bytes(), Encoding::Hex);
            let signature = {
                let mut proofs = proofs.lock().unwrap();
                let operation = preimage
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .to_owned();
                if !ws && matches!(answer, DeviceAnswer::Replay) {
                    let prior = proofs
                        .signed
                        .get(&operation)
                        .cloned()
                        .expect("positive proof precedes replay");
                    assert!(prior != fresh, "fresh attempt has a new signing preimage");
                    proofs.replayed += 1;
                    prior
                } else {
                    if !ws {
                        proofs.signed.insert(operation, fresh.clone());
                    }
                    fresh
                }
            };
            let response = json!({"id":request["id"], "wsAuthSignature":signature});
            assert!(
                !device.resolve(902, response.clone()),
                "another socket cannot answer"
            );
            assert!(device.resolve(901, response.clone()));
            assert!(
                !device.resolve(901, response),
                "a consumed request refuses a second answer"
            );
            proofs.lock().unwrap().consumed += 1;
        }
    });
    let options = crate::accounts::engine_options(
        &InitPayload {
            account_name: "pending-proof".into(),
            exchange_name: "bybit".into(),
            key: KEY.into(),
            ..Default::default()
        },
        false,
    );
    let remote = remote_account(
        link,
        "bybit",
        Some("pending-proof".into()),
        &options.to_string(),
    )
    .unwrap();
    let prepared = crate::delegated::PreparedDelegated {
        plan: f.plan.clone(),
        options: remote.options_json,
        resident: crate::delegated::ResidentCopy::test_provider(remote.provider),
    };
    let transport = NetworkTransport::with_routing(
        f.plan.clone(),
        Arc::new(|| Box::pin(async { Err("synthetic direct route has no proxy token".into()) })),
    );
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        crate::delegated::probe::run(
            prepared,
            transport,
            ServerConfig::default(),
            Arc::new(|| true),
        ),
    )
    .await
    .expect("isolated proof settles");
    responder.abort();
    if let Err(error) = responder.await {
        assert!(error.is_cancelled(), "proof responder panicked");
    }
    result.is_ok()
}

#[tokio::test]
async fn cosigned_private_read_negatives_preserve_the_running_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let f = fixture_with_auth(true).await;
            let resident = RunningAccount::start(&f).await;
            let proofs = Arc::new(Mutex::new(Proofs::default()));
            assert!(remote_proof(&f, DeviceAnswer::Correct, proofs.clone()).await);
            assert!(proofs.lock().unwrap().consumed > 0);
            resident.untouched(&f);
            assert!(!remote_proof(&f, DeviceAnswer::OtherKey, Arc::default()).await);
            resident.untouched(&f);
            tokio::time::sleep(Duration::from_millis(2)).await;
            assert!(!remote_proof(&f, DeviceAnswer::Replay, proofs.clone()).await);
            assert!(proofs.lock().unwrap().replayed > 0);
            resident.untouched(&f);
            assert!(remote_proof(&f, DeviceAnswer::Correct, Arc::default()).await);
            resident.untouched(&f);
        })
        .await;
}
