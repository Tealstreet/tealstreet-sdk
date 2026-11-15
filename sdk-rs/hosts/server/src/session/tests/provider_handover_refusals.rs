use super::*;

#[tokio::test]
async fn cdp_join_refusal_is_typed_visible_and_never_claims_an_attachment() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in ["coinbase", "coinbasespot"] {
                let registry = crate::accounts::tests::registry();
                let (mut session, mut sent) = user_session(registry.clone(), "synthetic-user");
                let init = InitPayload {
                    account_name: "synthetic-cdp".into(),
                    exchange_name: venue.into(),
                    key: "organizations/synthetic/apiKeys/synthetic".into(),
                    signing_mode: Some("delegated".into()),
                    instance_id: Some("current-registration".into()),
                    ..Default::default()
                };
                session.init(init).await.unwrap();
                assert_eq!(
                    sent.try_recv().unwrap(),
                    envelope::with_instance_id(
                        envelope::init_proof_unavailable("synthetic-cdp"),
                        "current-registration"
                    )
                );
                assert!(session.joined.is_empty());
                assert!(session.pending_verifications.is_empty());
                assert_eq!(registry.account_count(), 0);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn cdp_refusal_preserves_the_existing_cosigned_actor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in ["coinbase", "coinbasespot"] {
                let registry = crate::accounts::tests::registry();
                let original = InitPayload {
                    account_name: "synthetic-cdp".into(),
                    exchange_name: venue.into(),
                    key: "organizations/synthetic/apiKeys/synthetic".into(),
                    ..Default::default()
                };
                let account = crate::accounts::testing::trading_account(
                    &registry,
                    "synthetic-user",
                    &original,
                );
                let (mut session, mut sent) = user_session(registry.clone(), "synthetic-user");
                let mut candidate = original;
                candidate.signing_mode = Some("delegated".into());
                session.init(candidate).await.unwrap();
                assert_eq!(
                    sent.try_recv().unwrap(),
                    envelope::init_proof_unavailable("synthetic-cdp")
                );
                assert!(session.joined.is_empty());
                assert!(session.pending_verifications.is_empty());
                assert!(account.delegated().is_none());
                assert!(account.handle.disposal_cause().is_none());
                assert_eq!(registry.account_count(), 1);
                account.handle.dispose();
                session.close();
            }
        })
        .await;
}
