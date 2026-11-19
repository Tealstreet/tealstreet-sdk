use super::*;

async fn proof(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
) -> JoinCompletion {
    let mut device = Device::default();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            tokio::select! {
                Some(text) = sent.recv() => {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    if frame["t"] == "sign-req" {
                        device.answer(session, frame, Answer::Correct).await;
                    } else {
                        assert_eq!(frame["t"], "msg", "only the existing public stream may interleave");
                    }
                }
                completion = done.recv() => break completion.unwrap(),
            }
        }
    }).await.expect("real signed private startup completes")
}

async fn settings_conflict(parked: bool, delegated: bool) {
    let f = fixture_with_auth(true).await;
    let (mut old, mut old_sent) = candidate(&f, "alice");
    let mut old_done = old.take_verified();
    let request = if delegated {
        payload(&mut old, Some("old"))
    } else {
        init(Some("old"))
    };
    old.init(request).await.unwrap();
    let completed = proof(&mut old, &mut old_sent, &mut old_done).await;
    assert!(completed.probe.as_ref().unwrap().permit.is_ok());
    old.verification_completed(completed).await.unwrap();
    let original = old.joined["acct"].account.clone();
    let fingerprint = original.fingerprint.clone();
    let old_holder = old.socket;
    if parked {
        old.identity.resumable = true;
        old.close();
        assert!(original.has_parked_holders());
    }
    let (mut web, mut sent) = candidate(&f, "alice");
    let mut done = web.take_verified();
    let mut request = if delegated {
        payload(&mut web, Some("web"))
    } else {
        init(Some("web"))
    };
    request.tuning = Some(safe_cex_venues::DisplayTuning {
        calculate_upnl_from_realtime_last_price: true,
    });
    web.init(request).await.unwrap();
    let completed = proof(&mut web, &mut sent, &mut done).await;
    assert!(
        completed.probe.as_ref().unwrap().permit.is_ok(),
        "same credentials were independently verified"
    );
    let capture = crate::state::log_capture::capture();
    web.verification_completed(completed).await.unwrap();
    let lines = capture.finish();
    let replies: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
        .map(|s| serde_json::from_str(&s).unwrap())
        .collect();
    let refusal = replies
        .iter()
        .find(|f| f["d"]["data"][1]["initRefused"] == true)
        .expect("settings conflict is explicit");
    assert_eq!(
        refusal["d"]["data"][1]["kind"], "settings-conflict",
        "settings are not invalid credentials: {refusal}"
    );
    assert_eq!(
        refusal["d"]["data"][1]["differing_fields"],
        json!(["tuning"])
    );
    assert!(refusal["d"]["data"][0].as_str().unwrap().contains("tuning"));
    assert!(
        !refusal["d"]["data"][0]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("verify your key")
    );
    let audit = lines
        .iter()
        .find_map(|line| {
            let outer: Value = serde_json::from_str(line).ok()?;
            let event: Value = serde_json::from_str(outer["msg"].as_str()?).ok()?;
            (event["event"] == "hosted.admission_refused").then_some(event)
        })
        .expect("classified admission audit");
    assert_eq!(audit["kind"], "settings-conflict");
    assert_eq!(audit["differing_fields"], json!(["tuning"]));
    assert!(!audit.to_string().contains(SECRET));
    assert!(!audit.to_string().contains(KEY));
    assert!(!web.joined.contains_key("acct"));
    assert_eq!(original.fingerprint, fingerprint);
    assert!(original.handle.disposal_cause().is_none());
    assert!(f.registry.holds(&original, old_holder) || original.has_parked_holders());
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    web.close();
    f.registry
        .dispose(&original, crate::accounts::Disposal::PageDisposed);
}

#[tokio::test]
async fn first_connect_live_cosigned_settings_conflict_is_not_key_rejection() {
    settings_conflict(false, false).await;
}

#[tokio::test]
async fn first_connect_parked_cosigned_settings_conflict_is_not_key_rejection() {
    settings_conflict(true, false).await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn first_connect_live_delegated_settings_conflict_is_not_key_rejection() {
    settings_conflict(false, true).await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn first_connect_parked_delegated_settings_conflict_is_not_key_rejection() {
    settings_conflict(true, true).await;
}

#[tokio::test]
async fn first_connect_public_before_keyed_and_connection_cadence_share_identity() {
    let f = fixture_with_auth(true).await;
    let mut resident = Resident::start(&f, &mut Device::default()).await;
    let original = resident.account.clone();
    let (mut web, mut sent) = candidate(&f, "alice");
    let mut done = web.take_verified();
    let public = InitPayload {
        account_name: "__pub_bybit".into(),
        exchange_name: "bybit".into(),
        public_only: Some(true),
        ..Default::default()
    };
    web.init(public).await.unwrap();
    assert!(web.joined["__pub_bybit"].account.is_public());
    let mut private = init(Some("web"));
    private.publish_interval_ms = serde_json::from_value(json!(500)).unwrap();
    assert_eq!(
        private.fingerprint(),
        original.fingerprint,
        "connection cadence must already be excluded"
    );
    web.init(private).await.unwrap();
    let completed = proof(&mut web, &mut sent, &mut done).await;
    assert!(completed.probe.as_ref().unwrap().permit.is_ok());
    web.verification_completed(completed).await.unwrap();
    assert!(Arc::ptr_eq(&web.joined["acct"].account, &original));
    assert!(web.joined["__pub_bybit"].account.is_public());
    assert!(f.registry.holds(&original, web.socket));
    assert_eq!(f.private_writes.load(Ordering::SeqCst), 0);
    web.close();
    resident.session.take().unwrap().close();
    f.registry
        .dispose(&original, crate::accounts::Disposal::PageDisposed);
}
