use super::*;
use safe_cex_engine::request_budget::RequestClass;
use safe_cex_remote_signer::SIGN_TIMEOUT;
use std::future::Future;
use std::task::Poll;

async fn assert_probe_deadline(delegated: bool) {
    let mut registry = Registry::new(
        EngineThreads::start(1).unwrap(),
        Duration::from_millis(1),
        Duration::from_secs(60),
    );
    registry.pacer = crate::pacing::Pacer::new(1, Duration::ZERO);
    // Reserve starts without advancing the clock, leaving this proof paced
    // beyond its deadline. No fixture request may reach an upstream venue.
    for _ in 0..40 {
        let reservation = registry.pacer.admit();
        tokio::pin!(reservation);
        let _ = std::future::poll_fn(|cx| Poll::Ready(reservation.as_mut().poll(cx))).await;
    }
    let init = InitPayload {
        account_name: "paced-proof".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        signing_mode: Some(if delegated { "delegated" } else { "cosigned" }.into()),
        ..Default::default()
    };
    let credentials: Credentials =
        serde_json::from_str(r#"{"key":"synthetic-key","secret":"synthetic-secret"}"#).unwrap();
    let link = (!delegated).then(|| {
        SignerLink::new(
            7,
            Arc::new(|_| panic!("paced proof cannot request a signature")),
        )
    });
    let permit = registry.probe_permit("alice", &init).unwrap();
    let proof = registry.probe_join(
        "alice",
        &init,
        Some(("synthetic-token".into(), now_ms() + 60_000)),
        delegated.then_some(&credentials),
        link,
        permit,
    );
    tokio::pin!(proof);
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(proof.as_mut().poll(cx)))
            .await
            .is_pending()
    );

    tokio::time::advance(Duration::from_secs(30)).await;
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(proof.as_mut().poll(cx)))
            .await
            .is_pending(),
        "the signing and private-read budgets must both remain available"
    );
    let budget = SIGN_TIMEOUT + Duration::from_millis(u64::from(RequestClass::Read.budget_ms()));
    tokio::time::advance(budget - Duration::from_secs(30) - Duration::from_millis(1)).await;
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(proof.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    // Tokio's timer wheel has millisecond resolution.
    tokio::time::advance(Duration::from_millis(2)).await;
    let Poll::Ready(Err(error)) =
        std::future::poll_fn(|cx| Poll::Ready(proof.as_mut().poll(cx))).await
    else {
        panic!("the composed admission deadline must refuse the join");
    };
    assert!(error.transient);
    assert!(registry.pending_inits.lock().unwrap().is_empty());
    assert!(registry.accounts.lock().unwrap().is_empty());
    assert_eq!(registry.threads.accounts(), 0);
}

#[tokio::test(start_paused = true)]
async fn cosigned_probe_deadline_uses_existing_budgets() {
    assert_probe_deadline(false).await;
}

#[tokio::test(start_paused = true)]
async fn delegated_probe_deadline_uses_existing_budgets() {
    assert_probe_deadline(true).await;
}
