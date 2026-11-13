use super::*;
use crate::session::{Identity, Session};
use crate::state::Metrics;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct Fixture {
    registry: Arc<Registry>,
    reads: Arc<AtomicUsize>,
    generation: Arc<AtomicUsize>,
    gate: Arc<tokio::sync::Semaphore>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
async fn fixture() -> Fixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let reads = Arc::new(AtomicUsize::new(0));
    let generation = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(tokio::sync::Semaphore::new(100));
    let (count, current, wait) = (reads.clone(), generation.clone(), gate.clone());
    let app = axum::Router::new().fallback(move |uri: axum::http::Uri| {
        let (count, current, wait) = (count.clone(), current.clone(), wait.clone());
        async move {
            if uri.path().ends_with("/api/v1/apikeys") {
                count.fetch_add(1, Ordering::SeqCst);
                wait.acquire().await.unwrap().forget();
                let n = current.load(Ordering::SeqCst) as u8 + 3;
                let public = safe_cex_crypto::hex(&safe_cex_crypto::lighter::public_key(&[n; 40]).unwrap());
                let account: u64 = uri.query().unwrap().split('&').find_map(|part| part.strip_prefix("account_index=")).unwrap().parse().unwrap();
                return axum::Json(json!({"code":200,"api_keys":[{"account_index":account,"api_key_index":4,"public_key":public}]}));
            }
            axum::Json(json!({"code":200,"order_books":[],"markets":[],"accounts":[],"orders":[],"trades":[]}))
        }
    });
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let http = safe_cex_native::HttpRoute {
        prefix: base.clone(),
        ..Default::default()
    };
    let ws = safe_cex_native::WsRoute {
        host: Some(base.replacen("http:", "ws:", 1) + "/ws"),
        ..Default::default()
    };
    let plan = RoutingPlan {
        http: http.clone(),
        public_http: Some(http.clone()),
        read_http: Some(http.clone()),
        read_public_http: Some(http.clone()),
        public_read_http: Some(http),
        ws: safe_cex_native::WsRoutes {
            public: ws.clone(),
            private: ws.clone(),
            business: ws,
        },
        ..Default::default()
    };
    Fixture {
        registry: testing::probe_registry(plan),
        reads,
        generation,
        gate,
        server,
    }
}
fn session(f: &Fixture, user: &str) -> (Session, crate::out::Receiver) {
    let (out, sent, _) = crate::out::Out::channel(128);
    (
        Session::new(
            f.registry.clone(),
            Arc::new(Metrics::default()),
            Identity {
                user: Some(user.into()),
                token: Some(("synthetic-token".into(), u64::MAX)),
                chat_handle: None,
                resume_from: None,
                resumable: false,
            },
            out,
        ),
        sent,
    )
}
fn request(label: &str, account: u64) -> String {
    json!({"t":"init","d":{
        "accountName":label,"exchangeName":"lighter", "instanceId":label,
        "key":"0x1111111111111111111111111111111111111111", "applicationId":account, "apiKeyIndex":4,
        "lighterRegisteredKey":{"account":account,"index":4,"public":"synthetic-untrusted-ignored"}
    }}).to_string()
}
async fn finish(
    session: &mut Session,
    completed: &mut tokio::sync::mpsc::UnboundedReceiver<crate::session::JoinCompletion>,
) {
    let completion = tokio::time::timeout(Duration::from_secs(5), completed.recv())
        .await
        .expect("metadata completes")
        .unwrap();
    session.verification_completed(completion).await.unwrap();
}
async fn attach(
    f: &Fixture,
    user: &str,
    label: &str,
    index: u64,
) -> (Session, crate::out::Receiver) {
    let (mut session, sent) = session(f, user);
    let mut completed = session.take_verified();
    session.handle(&request(label, index)).await.unwrap();
    finish(&mut session, &mut completed).await;
    (session, sent)
}

#[tokio::test]
async fn hosted_lighter_identity_uses_fresh_registered_key_and_keeps_aliases_and_owners_separate() {
    let f = fixture().await;
    let (first, _first_sent) = attach(&f, "alice", "desktop", 12).await;
    let original = testing::accounts(&f.registry)[0].clone();
    let original_id = original.registry_name().to_owned();
    assert_ne!(original_id, "desktop");
    let (alias, _alias_sent) = attach(&f, "alice", "phone", 12).await;
    assert_eq!(
        f.registry.account_count(),
        1,
        "same credential under another label shares an actor"
    );
    assert!(Arc::ptr_eq(&testing::accounts(&f.registry)[0], &original));
    assert_eq!(
        f.reads.load(Ordering::SeqCst),
        2,
        "each admission resolves registration anew"
    );
    f.generation.store(1, Ordering::SeqCst);
    let (replacement, _replacement_sent) = attach(&f, "alice", "tablet", 12).await;
    let entries = testing::accounts(&f.registry);
    assert_eq!(
        entries.len(),
        2,
        "re-registering the same slot creates a different identity"
    );
    assert!(entries.iter().any(|a| Arc::ptr_eq(a, &original)));
    let replacement_id = entries
        .iter()
        .find(|a| !Arc::ptr_eq(a, &original))
        .unwrap()
        .registry_name()
        .to_owned();
    assert_ne!(replacement_id, original_id);
    assert!(original.handle.disposal_cause().is_none());
    let (bob, _bob_sent) = attach(&f, "bob", "tablet", 12).await;
    let entries = testing::accounts(&f.registry);
    assert_eq!(entries.len(), 3);
    let bob_account = entries.iter().find(|a| a.user == "bob").unwrap();
    assert_ne!(bob_account.registry_name(), replacement_id);
    let (different_account, _different_sent) = attach(&f, "alice", "different", 13).await;
    assert_eq!(f.registry.account_count(), 4);
    first.close();
    alias.close();
    replacement.close();
    bob.close();
    different_account.close();
}

#[tokio::test]
async fn hosted_lighter_identity_drops_a_completion_when_login_switches_during_metadata_read() {
    let f = fixture().await;
    f.gate.forget_permits(100);
    let (mut caller, _alice_sent) = session(&f, "alice");
    let mut alice_done = caller.take_verified();
    caller.handle(&request("desktop", 12)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.reads.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("real metadata read is deferred");
    assert_eq!(
        f.registry.account_count(),
        0,
        "lookup has no admitted account"
    );
    let (bob, mut bob_sent) = session(&f, "bob");
    let old = std::mem::replace(&mut caller, bob);
    let mut bob_done = caller.take_verified();
    caller.handle(&request("desktop", 13)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.reads.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Bob has his own deferred generation for the same label");
    f.gate.add_permits(1);
    let result = tokio::time::timeout(Duration::from_secs(5), alice_done.recv())
        .await
        .unwrap()
        .unwrap();
    caller.verification_completed(result).await.unwrap();
    assert_eq!(
        f.registry.account_count(),
        0,
        "Alice's deferred work cannot attach as Bob"
    );
    assert!(bob_sent.try_recv().is_err());
    f.gate.add_permits(100);
    finish(&mut caller, &mut bob_done).await;
    let entries = testing::accounts(&f.registry);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].user, "bob", "fresh Bob control attaches to Bob");
    old.close();
    caller.close();
}
