use safe_cex_crypto::{
    admission::{Family, PublicKey},
    ed25519,
};
use safe_cex_engine::{HttpRequest, RequestId, SocketId};
use safe_cex_exchange_server::admission::{LocalProof, fetch_lighter_registered_key};
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp, Transport,
};
use safe_cex_venues::lighter_registration::Network;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::mpsc;

async fn assert_pending<F: std::future::Future>(mut task: std::pin::Pin<&mut F>) {
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(task.as_mut().poll(cx)))
            .await
            .is_pending()
    );
}

fn key() -> PublicKey {
    PublicKey::new(Family::Ed25519, &ed25519::public_key(&[7; 32]).unwrap()).unwrap()
}

#[test]
fn connection_bound_one_use_and_freshness() {
    let mut proof = LocalProof::issue(10, "one", [2; 32], key(), 1000).unwrap();
    let signature = ed25519::sign(&proof.statement().message(Family::Ed25519), &[7; 32]).unwrap();
    assert!(proof.verify(10, 1001, &signature));
    assert!(!proof.verify(10, 1001, &signature));
    let mut next = LocalProof::issue(10, "two", [2; 32], key(), 1000).unwrap();
    assert!(!next.verify(10, 1001, &signature));
    let mut wrong_connection = LocalProof::issue(10, "one", [2; 32], key(), 1000).unwrap();
    let signature = ed25519::sign(
        &wrong_connection.statement().message(Family::Ed25519),
        &[7; 32],
    )
    .unwrap();
    assert!(!wrong_connection.verify(11, 1001, &signature));
    assert!(!wrong_connection.verify(10, 1001, &signature));
    for now in [999, 16_001, u64::MAX] {
        let mut expired = LocalProof::issue(10, "one", [2; 32], key(), 1000).unwrap();
        let signature =
            ed25519::sign(&expired.statement().message(Family::Ed25519), &[7; 32]).unwrap();
        assert!(!expired.verify(10, now, &signature));
    }
    let a = LocalProof::issue(10, "one", [2; 32], key(), 1000).unwrap();
    let b = LocalProof::issue(10, "one", [2; 32], key(), 1000).unwrap();
    assert_ne!(a.statement(), b.statement());
}

#[tokio::test(start_paused = true)]
async fn monotonic_deadline_survives_wall_clock_rollback() {
    let mut proof = LocalProof::issue(10, "one", [2; 32], key(), 1000).unwrap();
    let signature = ed25519::sign(&proof.statement().message(Family::Ed25519), &[7; 32]).unwrap();
    tokio::time::advance(std::time::Duration::from_millis(15_001)).await;
    assert!(!proof.verify(10, 1001, &signature));
}

#[test]
fn candidate_providers_share_the_same_connection_and_consumption_checks() {
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::{KeyRef, Provider};
    fn candidate(family: Family, n: u8) -> (Provider, KeyRef) {
        let mut bytes = vec![0; if family == Family::Schnorr { 40 } else { 32 }];
        if family == Family::Schnorr {
            bytes[0] = n;
        } else {
            bytes[31] = n;
        }
        let (provider,ids)=LocalSigner::from_json_with_policy(
            serde_json::json!({"primary":{"key":"synthetic","secret":safe_cex_crypto::hex(&bytes)}}).to_string(),
            serde_json::from_value(serde_json::json!({"primary":{"secretEncoding":"hex","signingAlgorithm":family.algorithm()}})).unwrap()
        ).unwrap();
        (
            Arc::new(provider),
            ids.primary.unwrap().signing_ref().unwrap().clone(),
        )
    }
    for family in [
        Family::Ethereum,
        Family::Ed25519,
        Family::Stark,
        Family::Schnorr,
    ] {
        let (provider, reference) = candidate(family, 7);
        let (other, other_ref) = candidate(family, 8);
        let expected = || PublicKey::from_provider(family, &provider, &reference).unwrap();
        let mut proof = LocalProof::issue(10, "candidate", [2; 32], expected(), 1000).unwrap();
        assert!(
            proof
                .verify_candidate(10, 1000, &provider, &reference)
                .unwrap()
        );
        assert!(
            !proof
                .verify_candidate(10, 1000, &provider, &reference)
                .unwrap()
        );
        let mut proof = LocalProof::issue(10, "candidate", [2; 32], expected(), 1000).unwrap();
        assert!(
            !proof
                .verify_candidate(10, 1000, &other, &other_ref)
                .unwrap()
        );
        assert!(
            !proof
                .verify_candidate(10, 1000, &provider, &reference)
                .unwrap()
        );
        // Verification does not alter the provider retained by a running account.
        assert!(expected().matches_provider(&provider, &reference).unwrap());
        for (connection, now) in [(11, 1000), (10, 16_001)] {
            let mut proof = LocalProof::issue(10, "candidate", [2; 32], expected(), 1000).unwrap();
            assert!(
                !proof
                    .verify_candidate(connection, now, &provider, &reference)
                    .unwrap()
            );
        }
    }
}

#[test]
fn each_family_has_a_single_use_signature_path() {
    use safe_cex_crypto::signer::LocalSigner;
    for family in [
        Family::Ethereum,
        Family::Ed25519,
        Family::Stark,
        Family::Schnorr,
    ] {
        let mut bytes = vec![0; if family == Family::Schnorr { 40 } else { 32 }];
        if family == Family::Schnorr {
            bytes[0] = 7;
        } else {
            bytes[31] = 7;
        }
        let (provider,ids)=LocalSigner::from_json_with_policy(
            serde_json::json!({"primary":{"key":"synthetic","secret":safe_cex_crypto::hex(&bytes)}}).to_string(),
            serde_json::from_value(serde_json::json!({"primary":{"secretEncoding":"hex","signingAlgorithm":family.algorithm()}})).unwrap()
        ).unwrap();
        let provider: safe_cex_engine::signing::Provider = Arc::new(provider);
        let reference = ids.primary.unwrap().signing_ref().unwrap().clone();
        let expected = || PublicKey::from_provider(family, &provider, &reference).unwrap();
        let mut proof = LocalProof::issue(10, "one", [2; 32], expected(), 1000).unwrap();
        let signature = proof
            .statement()
            .sign(family, &provider, &reference, &[9; 32])
            .unwrap();
        assert!(proof.verify(10, 1000, &signature));
        assert!(!proof.verify(10, 1000, &signature));
        let mut next = LocalProof::issue(10, "two", [2; 32], expected(), 1000).unwrap();
        assert!(!next.verify(10, 1000, &signature));
    }
}

#[derive(Clone, Debug, Default)]
struct Fake {
    pending: Arc<Mutex<Option<(HttpRequest, Events)>>>,
    prepare_gate: Option<Arc<tokio::sync::Notify>>,
}
#[derive(Debug)]
struct Prepared {
    fake: Fake,
    request: HttpRequest,
}
impl PreparedHttp for Prepared {
    fn bind(self: Box<Self>, packet: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        assert_eq!(packet, self.request);
        Ok(self)
    }
}
impl BoundHttp for Prepared {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        let canceled = events.clone();
        *self.fake.pending.lock().unwrap() = Some((self.request, events));
        tokio::spawn(async move {
            canceled.closed().await;
        })
        .abort_handle()
    }
}
impl Transport for Fake {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("HTTP only")
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("prepared HTTP only")
    }
    fn prepare_http(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        assert_eq!(
            intent,
            safe_cex_engine::request_context::RoutingIntent::Public
        );
        let fake = self.clone();
        Box::pin(async move {
            if let Some(gate) = &fake.prepare_gate {
                gate.notified().await;
            }
            Ok(Box::new(Prepared { fake, request }) as Box<dyn PreparedHttp>)
        })
    }
}
fn answer(fake: &Fake, body: Vec<u8>, status: u16) -> HttpRequest {
    let (request, events) = fake.pending.lock().unwrap().take().unwrap();
    let id = request.id;
    events
        .send(HostEvent::HttpStart {
            request: id,
            status,
            headers: String::new(),
        })
        .unwrap();
    events.send(HostEvent::HttpChunk(id, body)).unwrap();
    events.send(HostEvent::HttpEnd(id)).unwrap();
    request
}
fn body(account: u64, index: u8) -> Vec<u8> {
    let public = safe_cex_crypto::lighter::public_key(&[3; 40]).unwrap();
    serde_json::json!({"code":200,"api_keys":[{"account_index":account,"api_key_index":index,"public_key":safe_cex_crypto::hex(&public),"nonce":0,"transaction_time":0}]}).to_string().into_bytes()
}

#[tokio::test]
async fn registration_fetch_is_fixed_scoped_and_refuses_invalid_responses() {
    for (data, status, accepted) in [
        (body(12,4),200,true), (body(13,4),200,false), (body(12,5),200,false),
        (body(12,4),401,false), (b"{}".to_vec(),200,false),
        (vec![b' ';16_385],200,false),
        (br#"{"code":200,"api_keys":[]}"#.to_vec(),200,false),
        (br#"{"code":200,"api_keys":[{"account_index":12,"api_key_index":4,"public_key":"00"}]}"#.to_vec(),200,false),
    ] {
        let fake=Fake::default();
        let task=fetch_lighter_registered_key(&fake,Network::Mainnet,12,4,Arc::new(||true));
        tokio::pin!(task);
        assert_pending(task.as_mut()).await;
        let request=answer(&fake,data,status);
        assert_eq!(request.url,"https://mainnet.zklighter.elliot.ai/api/v1/apikeys?account_index=12&api_key_index=4");
        assert!(request.body.is_none() && request.headers.is_empty() && request.signer_headers.is_empty());
        assert_eq!(task.await.is_ok(),accepted);
    }
}

#[tokio::test]
async fn registration_drops_a_response_after_owner_replacement_and_cancels_on_drop() {
    let fake = Fake::default();
    let current = Arc::new(AtomicU64::new(11));
    let captured = current.clone();
    let task = fetch_lighter_registered_key(
        &fake,
        Network::Mainnet,
        12,
        4,
        Arc::new(move || captured.load(Ordering::SeqCst) == 11),
    );
    tokio::pin!(task);
    assert_pending(task.as_mut()).await;
    current.store(22, Ordering::SeqCst); // second owner while the first owner's read awaits IO
    answer(&fake, body(12, 4), 200);
    assert!(task.await.is_err());

    let mut task = Box::pin(fetch_lighter_registered_key(
        &fake,
        Network::Mainnet,
        12,
        4,
        Arc::new(|| true),
    ));
    assert_pending(task.as_mut()).await;
    drop(task);
    assert!(fake.pending.lock().unwrap().as_ref().unwrap().1.is_closed());
    let (request, events) = fake.pending.lock().unwrap().take().unwrap();
    assert_eq!(request.id, RequestId(1));
    assert!(events.send(HostEvent::HttpEnd(request.id)).is_err());
}

#[tokio::test]
async fn registration_drops_prepared_work_after_owner_replacement() {
    let gate = Arc::new(tokio::sync::Notify::new());
    let fake = Fake {
        prepare_gate: Some(gate.clone()),
        ..Default::default()
    };
    let current = Arc::new(AtomicU64::new(11));
    let captured = current.clone();
    let task = fetch_lighter_registered_key(
        &fake,
        Network::Mainnet,
        12,
        4,
        Arc::new(move || captured.load(Ordering::SeqCst) == 11),
    );
    tokio::pin!(task);
    assert_pending(task.as_mut()).await;
    current.store(22, Ordering::SeqCst);
    gate.notify_one();
    assert!(task.await.is_err());
    assert!(fake.pending.lock().unwrap().is_none());
}

#[tokio::test(start_paused = true)]
async fn registration_read_and_preparation_are_bounded() {
    for preparation in [false, true] {
        let fake = Fake {
            prepare_gate: preparation.then(|| Arc::new(tokio::sync::Notify::new())),
            ..Default::default()
        };
        assert!(
            fetch_lighter_registered_key(&fake, Network::Mainnet, 12, 4, Arc::new(|| true))
                .await
                .is_err()
        );
        let pending = fake.pending.lock().unwrap();
        if preparation {
            assert!(pending.is_none());
        } else {
            assert!(pending.as_ref().unwrap().1.is_closed());
        }
    }
}
