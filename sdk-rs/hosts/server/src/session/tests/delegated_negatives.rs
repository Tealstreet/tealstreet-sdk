use super::*;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Provider;

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
    _sent: crate::out::Receiver,
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
        .issue("acct", "bybit", crate::accounts::now_ms())
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
                        .issue("acct", "bybit", crate::accounts::now_ms())
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
            if case == "other-pair" {
                let other = f
                    .registry
                    .held("alice", "acct", candidate.socket)
                    .expect("separately verified identity is admitted independently");
                assert!(!Arc::ptr_eq(&other, &resident.account));
                assert_ne!(other.registry_name(), resident.account.registry_name());
                assert!(!Arc::ptr_eq(
                    &other.delegated.as_ref().unwrap().provider(),
                    &resident.provider,
                ));
                candidate.close();
                resident.untouched(&f);
                continue;
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

#[path = "cosigned_negatives.rs"]
mod cosigned_negatives;
