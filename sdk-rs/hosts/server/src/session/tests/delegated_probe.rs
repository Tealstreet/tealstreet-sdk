use super::*;
use axum::{
    http::{HeaderMap, Uri},
    response::IntoResponse,
};
use safe_cex_native::{HttpRoute, RoutingPlan, WsRoute, WsRoutes};
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

#[path = "delegated_negatives.rs"]
mod delegated_negatives;
#[path = "delegated_probe_more.rs"]
mod more;

struct Fixture {
    registry: Arc<Registry>,
    code: Arc<AtomicUsize>,
    private_reads: Arc<AtomicUsize>,
    gate: Arc<tokio::sync::Semaphore>,
    server: tokio::task::JoinHandle<()>,
    rejected: Arc<AtomicUsize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
async fn fixture() -> Fixture {
    fixture_with_auth(false).await
}
async fn fixture_with_auth(authenticate: bool) -> Fixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let code = Arc::new(AtomicUsize::new(0));
    let private_reads = Arc::new(AtomicUsize::new(0));
    let rejected = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new(tokio::sync::Semaphore::new(1000));
    let (codes, reads, wait) = (code.clone(), private_reads.clone(), gate.clone());
    let proof_rejections = rejected.clone();
    let app = axum::Router::new().route("/ws", axum::routing::get(|ws: axum::extract::WebSocketUpgrade| async {
        ws.on_upgrade(|mut socket| async move {
            while let Some(Ok(frame)) = socket.recv().await {
                if let axum::extract::ws::Message::Text(text) = frame {
                    let request: Value = serde_json::from_str(&text).unwrap();
                    let response = json!({"op":request["op"],"success":true,"ret_msg":"","conn_id":"synthetic-connection"});
                    if socket.send(axum::extract::ws::Message::Text(response.to_string().into())).await.is_err() { break; }
                }
            }
        })
    })).fallback(move |uri: Uri, headers: HeaderMap| {
        let (codes, reads, wait) = (codes.clone(), reads.clone(), wait.clone());
        let rejected = proof_rejections.clone();
        async move {
            if headers.contains_key("x-bapi-api-key") {
                assert!(headers.contains_key("x-bapi-sign"));
                reads.fetch_add(1, Ordering::SeqCst);
                wait.acquire().await.unwrap().forget();
                if authenticate && !delegated_negatives::signed_read_is_valid(&uri, &headers) {
                    rejected.fetch_add(1, Ordering::SeqCst);
                    return axum::Json(json!({"retCode":10003,"retMsg":"synthetic refusal","result":{}})).into_response();
                }
                let code = codes.load(Ordering::SeqCst);
                if code != 0 { return axum::Json(json!({"retCode":code,"retMsg":"synthetic refusal","result":{}})).into_response(); }
            }
            let result = if uri.path().contains("wallet-balance") {
                json!({"list":[{"accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMarginBalance":"1000","totalAvailableBalance":"1000","totalPerpUPL":"0","coin":[]}]})
            } else if uri.path().contains("instruments-info") && !uri.query().is_some_and(|q| q.contains("PreLaunch")) {
                json!({"category":"linear","list":[{"symbol":"BTCUSDT","contractType":"LinearPerpetual","status":"Trading","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","leverageFilter":{"minLeverage":"1","maxLeverage":"100","leverageStep":"1"},"priceFilter":{"minPrice":"0.1","maxPrice":"1000000","tickSize":"0.1"},"lotSizeFilter":{"minOrderQty":"0.001","maxOrderQty":"1000","qtyStep":"0.001"}}],"nextPageCursor":""})
            } else if uri.path().contains("tickers") {
                json!({"category":"linear","list":[{"symbol":"BTCUSDT","lastPrice":"60000","markPrice":"60000","indexPrice":"60000","bid1Price":"59999","ask1Price":"60001","fundingRate":"0.0001","nextFundingTime":"1760342400000"}]})
            } else if uri.path().contains("account/info") {
                json!({"unifiedMarginStatus":4,"marginMode":"REGULAR_MARGIN"})
            } else { json!({"category":"linear","list":[],"nextPageCursor":""}) };
            axum::Json(json!({"retCode":0,"retMsg":"OK","result":result})).into_response()
        }
    });
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
    let plan = RoutingPlan {
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
    };
    Fixture {
        registry: crate::accounts::testing::probe_registry(plan),
        code,
        private_reads,
        gate,
        server,
        rejected,
    }
}
fn payload(session: &mut Session, id: Option<&str>) -> InitPayload {
    let offer = session
        .handovers
        .issue("acct", crate::accounts::now_ms())
        .unwrap();
    let sealed = crate::delegated::tests::seal(
        &offer,
        "bybit",
        "synthetic-key",
        br#"{"key":"synthetic-key","secret":"synthetic-secret"}"#,
    );
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        instance_id: id.map(str::to_owned),
        signing_mode: Some("delegated".into()),
        sealed: Some(sealed),
        ..Default::default()
    }
}

#[tokio::test]
async fn apply_settings_allows_verified_delegated_settings_reinitialization() {
    let f = fixture().await;
    let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
    let mut results = session.take_verified();
    let first = payload(&mut session, Some("admitted"));
    session.init(first).await.unwrap();
    finish(&mut session, &mut results).await;
    let original = session.joined["acct"].account.clone();
    let mut changed = payload(&mut session, Some("candidate"));
    changed.enable_spot_markets = Some(true);
    session.init(changed.clone()).await.unwrap();
    finish(&mut session, &mut results).await;
    assert!(Arc::ptr_eq(&session.joined["acct"].account, &original));
    assert!(original.handle.disposal_cause().is_none());
    while sent.try_recv().is_ok() {}
    session
        .handle(r#"{"t":"apply-settings","d":{"accountName":"acct","instanceId":"admitted"}}"#)
        .await
        .unwrap();
    assert!(!session.joined.contains_key("acct"));
    assert_eq!(f.registry.account_count(), 0);
    assert_eq!(
        original.handle.disposal_cause(),
        Some(safe_cex_native::DisposalCause::Stopped)
    );
    let mut replacement = payload(&mut session, Some("replacement"));
    replacement.enable_spot_markets = Some(true);
    let fingerprint = crate::delegated::fingerprint(&replacement);
    session.init(replacement).await.unwrap();
    finish(&mut session, &mut results).await;
    let rebuilt = &session.joined["acct"].account;
    assert!(!Arc::ptr_eq(rebuilt, &original));
    assert!(rebuilt.delegated.is_some());
    assert_eq!(rebuilt.fingerprint, fingerprint);
    f.registry.dispose(rebuilt, Disposal::PageDisposed);
    session.close();
}

#[tokio::test]
async fn apply_settings_retires_a_join_proof_deferred_before_the_reset() {
    let f = fixture().await;
    let (mut owner, _) = user_session(f.registry.clone(), "alice");
    let mut admitted = owner.take_verified();
    let first = payload(&mut owner, Some("admitted"));
    owner.init(first).await.unwrap();
    finish(&mut owner, &mut admitted).await;
    let original = owner.joined["acct"].account.clone();
    f.gate.forget_permits(f.gate.available_permits());
    let before = f.private_reads.load(Ordering::SeqCst);
    let (mut pending, mut sent) = user_session(f.registry.clone(), "alice");
    let mut completed = pending.take_verified();
    let candidate = payload(&mut pending, Some("pending"));
    pending.init(candidate).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.private_reads.load(Ordering::SeqCst) == before {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("candidate is waiting in a private read");
    owner
        .handle(r#"{"t":"apply-settings","d":{"accountName":"acct","instanceId":"admitted"}}"#)
        .await
        .unwrap();
    assert!(!f.registry.holds(&original, owner.socket));
    f.gate.add_permits(1000);
    finish(&mut pending, &mut completed).await;
    assert!(
        !pending.joined.contains_key("acct"),
        "a pre-reset proof cannot restore the stopped settings"
    );
    let replies: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
        .map(|text| serde_json::from_str(&text).unwrap())
        .collect();
    assert!(
        replies
            .iter()
            .any(|f| f["d"]["data"][1]["initRefused"] == true),
        "{replies:?}"
    );
    assert_eq!(f.registry.account_count(), 0);
    pending.close();
    owner.close();
}
async fn finish(
    session: &mut Session,
    results: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
) {
    let completion = tokio::time::timeout(Duration::from_secs(5), results.recv())
        .await
        .expect("probe settles")
        .unwrap();
    session.verification_completed(completion).await.unwrap();
}
async fn wait_reads(f: &Fixture) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.private_reads.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("actual Bybit private startup read");
}

#[tokio::test]
async fn verified_pair_starts_then_reuses_the_resident_account() {
    let f = fixture().await;
    assert!(f.gate.available_permits() > 0);
    let (mut session, _sent) = user_session(f.registry.clone(), "alice");
    let mut results = session.take_verified();
    let init = payload(&mut session, Some("first"));
    session.init(init).await.unwrap();
    finish(&mut session, &mut results).await;
    let original = f
        .registry
        .held("alice", "acct", session.socket)
        .expect("verified pair becomes resident");
    wait_reads(&f).await;
    assert!(original.delegated.is_some() && original.link.is_none());
    assert!(f.private_reads.load(Ordering::SeqCst) > 0);
    session
        .msg(AccountMsg {
            op: "subscribeOHLCV".into(),
            data: json!({"symbol":"BTCUSDT","interval":"1m"}),
            account_name: Some("acct".into()),
        })
        .await;
    let subscriptions = session.joined["acct"].subscriptions.clone();
    assert!(!subscriptions.is_empty());
    let reads = f.private_reads.load(Ordering::SeqCst);
    let init = payload(&mut session, Some("second"));
    session.init(init).await.unwrap();
    finish(&mut session, &mut results).await;
    assert!(Arc::ptr_eq(
        &original,
        &f.registry.held("alice", "acct", session.socket).unwrap()
    ));
    assert!(
        f.private_reads.load(Ordering::SeqCst) > reads,
        "a running account still requires a fresh probe"
    );
    assert!(original.handle.disposal_cause().is_none());
    assert_eq!(session.joined["acct"].subscriptions, subscriptions);
    session.close();
}

#[tokio::test]
async fn classified_probe_refusals_keep_current_registration_and_legacy_bytes() {
    for (code, transient) in [(10003, false), (10005, false), (10006, true)] {
        for id in [Some("probe-registration"), None] {
            let f = fixture().await;
            f.code.store(code, Ordering::SeqCst);
            let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
            let mut results = session.take_verified();
            let init = payload(&mut session, id);
            session.init(init).await.unwrap();
            let completion = tokio::time::timeout(Duration::from_secs(5), results.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(
                completion.candidate.is_err(),
                "refused material is dropped before queueing the reply"
            );
            session.verification_completed(completion).await.unwrap();
            let expected = envelope::init_unverified("acct", transient);
            let expected = id.map_or_else(
                || expected.clone(),
                |id| envelope::with_instance_id(expected.clone(), id),
            );
            assert_eq!(sent.try_recv().unwrap(), expected);
            assert!(f.private_reads.load(Ordering::SeqCst) > 0);
            assert!(f.registry.held("alice", "acct", session.socket).is_none());
            session.close();
        }
    }
}

#[tokio::test]
async fn handover_binding_negatives_never_start_a_private_probe() {
    for case in ["key", "replay", "stale", "boot"] {
        let f = fixture().await;
        let (mut session, mut sent) = user_session(f.registry.clone(), "alice");
        let mut results = session.take_verified();
        let mut init = payload(&mut session, Some("negative"));
        match case {
            "key" => init.key = "other-synthetic-key".into(),
            "stale" => init.sealed.as_mut().unwrap().attempt_id = "stale".into(),
            "replay" => {
                session
                    .handovers
                    .issue("acct", crate::accounts::now_ms())
                    .unwrap();
            }
            "boot" => {
                let mut offer = session
                    .handovers
                    .issue("acct", crate::accounts::now_ms())
                    .unwrap();
                offer.boot_id = "synthetic-other-boot";
                init.sealed = Some(crate::delegated::tests::seal(
                    &offer,
                    "bybit",
                    "synthetic-key",
                    br#"{"key":"synthetic-key","secret":"synthetic-secret"}"#,
                ));
            }
            _ => unreachable!(),
        }
        session.init(init).await.unwrap();
        if session.pending_verifications.contains_key("acct") {
            finish(&mut session, &mut results).await;
        }
        assert_eq!(
            sent.try_recv().unwrap(),
            envelope::with_instance_id(envelope::init_unverified("acct", false), "negative")
        );
        assert!(f.registry.held("alice", "acct", session.socket).is_none());
        assert_eq!(f.private_reads.load(Ordering::SeqCst), 0);
        session.close();
    }
}
