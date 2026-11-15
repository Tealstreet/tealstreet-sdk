//! Session sign-res routing plus native startup, independently of catalog rollout.
use super::*;
use crate::delegated::probe::venue_tests::VenueIo;
use safe_cex_venues::hosted_signing;
use std::time::Duration;

async fn startup(material: &str) -> (Result<(), crate::delegated::probe::ProbeRefusal>, usize) {
    let (mut session, mut output) = user_session(crate::accounts::tests::registry(), "alice");
    let (out, lifetime) = session.handover_out("acct").isolated();
    let link = safe_cex_remote_signer::SignerLink::new(session.socket, out.outbox());
    let remote = safe_cex_remote_signer::remote_account(
        link.clone(),
        "apexomni",
        Some("acct".into()),
        &json!({"key":"synthetic-key","passphrase":"synthetic-passphrase"}).to_string(),
    )
    .unwrap();
    let seen = Arc::new(Mutex::new(Default::default()));
    let io = VenueIo {
        venue: "apexomni",
        seen: seen.clone(),
    };
    let (finished, result) = tokio::sync::oneshot::channel();
    // Exercise the real pending-session wire without declaring ApeX proof-ready.
    // The separate catalog qualification must still use ordinary Session::init.
    // Native actors need the engine thread's LocalSet; the Session verification
    // future itself is Send and only awaits that actor's completion.
    let native = tokio::task::spawn_local(async move {
        crate::delegated::probe::run(
            "apexomni",
            &remote.options_json,
            remote.provider,
            io,
            safe_cex_native::ServerConfig::default(),
            Arc::new(|| true),
        )
        .await
    });
    session.start_probe_verification("acct".into(), Some((link.clone(), lifetime)), async move {
        let result = native
            .await
            .expect("native actor must settle without a harness panic");
        let _ = finished.send(result);
        (Err(crate::delegated::Refused), None)
    });
    let mut options = serde_json::from_value(json!({
        "key":"synthetic-key", "secret":material, "passphrase":"synthetic-passphrase"
    }))
    .unwrap();
    let (provider, identities) =
        safe_cex_venues::prepare_local_signer_for("apexomni", &mut options).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::pin!(result);
        loop {
            tokio::select! {
                result = &mut result => break result.unwrap(),
                frame = output.recv() => {
                    let frame: Value = serde_json::from_str(&frame.unwrap()).unwrap();
                    assert_eq!(frame["t"], "sign-req");
                    let request = &frame["d"];
                    let input = serde_json::from_value(json!({"kind":"wsAuth", "toSign":request["wsAuth"]["toSign"], "outputFormat":request["wsAuth"]["outputFormat"]})).unwrap();
                    let response = match hosted_signing::prepare("apexomni", &identities, input) {
                        Ok((work, kind)) => {
                            let signed = provider.sign(work).await.unwrap();
                            let signed = hosted_signing::finish(kind, &signed).unwrap();
                            json!({"id":request["id"],"wsAuthSignature":signed.signature.unwrap()})
                        }
                        Err(_) => json!({"id":request["id"],"error":"synthetic unsupported device request"}),
                    };
                    assert!(!link.resolve(session.socket + 1000, response.clone()), "foreign socket cannot answer");
                    session.handle(&json!({"t":"sign-res","d":response}).to_string()).await.unwrap();
                    assert!(!link.resolve(session.socket, response), "response consumed once");
                }
            }
        }
    }).await.expect("native startup settles through Session sign-res");
    assert!(
        session.joined.is_empty(),
        "bridge coverage never bypasses catalog admission"
    );
    session.close();
    let seen = seen.lock().unwrap();
    if result.is_ok() {
        for route in [
            "/api/v3/account",
            "/api/v3/account-balance",
            "/api/v3/open-orders",
        ] {
            assert!(
                seen.urls
                    .iter()
                    .any(|url| url == &format!("https://omni.apex.exchange{route}")),
                "missing actual native private read: {route}"
            );
        }
    }
    (result, seen.signed_reads)
}

#[tokio::test]
async fn apexomni_batch_bridge_session_signatures_complete_native_private_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (result, reads) = startup("synthetic-material").await;
            assert!(
                result.is_ok(),
                "real device handler must accept the native batch: {result:?}"
            );
            assert!(
                reads >= 3,
                "account, balance and open orders must reach venue authentication"
            );
        })
        .await;
}

#[tokio::test]
async fn apexomni_batch_bridge_session_different_key_cannot_complete_private_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (result, reads) = startup("synthetic-other-material").await;
            assert!(
                result.is_err(),
                "wrong material cannot authenticate the original public key"
            );
            assert!(
                reads > 0,
                "wrong material must reach the venue, not fail on an unsupported wire shape"
            );
        })
        .await;
}
