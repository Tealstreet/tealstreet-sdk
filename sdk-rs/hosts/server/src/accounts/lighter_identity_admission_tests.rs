use super::*;

fn init() -> InitPayload {
    serde_json::from_value(
        serde_json::from_str::<Value>(&request("desktop", 12)).unwrap()["d"].clone(),
    )
    .unwrap()
}
async fn resolve(f: &Fixture, expiry: u64) -> InitPayload {
    f.registry
        .resolve_identity(
            "alice",
            init(),
            Some(("synthetic-token".into(), expiry)),
            Arc::new(|| true),
        )
        .await
        .unwrap()
}
async fn publish(f: &Fixture, init: &InitPayload) -> Result<Arc<Account>, String> {
    f.registry
        .obtain(
            "alice",
            init,
            false,
            7,
            Some(("synthetic-token".into(), now_ms() + 60_000)),
            Arc::new(|_| true),
        )
        .await
}

#[tokio::test]
async fn resolved_registration_is_bound_to_owner_namespace_and_indices_and_cannot_roundtrip_through_json()
 {
    let f = fixture().await;
    let resolved = resolve(&f, now_ms() + 60_000).await;
    assert!(identity::digest("alice", &resolved).unwrap().is_some());
    assert!(identity::digest("bob", &resolved).is_err());
    for mutation in ["network", "venue", "account", "index"] {
        let mut changed = resolved.clone();
        match mutation {
            "network" => changed.testnet = Some(true),
            "venue" => changed.exchange_name = "lighter-robinhood".into(),
            "account" => changed.application_id = Some("13".into()),
            _ => changed.api_key_index = Some("5".into()),
        }
        assert!(identity::digest("alice", &changed).is_err(), "{mutation}");
    }
    let wire = serde_json::to_value(&resolved).unwrap();
    assert!(wire.get("lighterRegisteredKey").is_none());
    let untrusted: InitPayload = serde_json::from_value(wire).unwrap();
    assert!(identity::digest("alice", &untrusted).is_err());
}

#[tokio::test]
async fn resolved_registration_keeps_its_original_expiry_when_actor_admission_gets_a_fresher_token()
{
    let f = fixture().await;
    let expiry = now_ms() + 2_000;
    let resolved = resolve(&f, expiry).await;
    tokio::time::sleep(Duration::from_millis(expiry.saturating_sub(now_ms()) + 1)).await;
    let error = publish(&f, &resolved)
        .await
        .err()
        .expect("captured metadata expiry must refuse publication");
    assert!(error.contains("resolution expired"));
    assert_eq!(f.registry.account_count(), 0);
    let fresh = resolve(&f, now_ms() + 60_000).await;
    let account = publish(&f, &fresh).await.unwrap();
    assert_eq!(account.user, "alice");
    f.registry.dispose(&account, Disposal::PageDisposed);
}

fn grant(f: &Fixture, epoch: u64) {
    let now = now_ms();
    f.registry
        .router_leases
        .accept(&crate::router_lease::Heartbeat {
            r#box: "01".into(),
            issued_at: now,
            users: vec![crate::router_lease::Assertion {
                user_id: "alice".into(),
                owner_epoch: epoch,
                sequence: 1,
                not_after: now + 45_000,
            }],
        })
        .unwrap();
}

#[tokio::test]
async fn resolved_registration_keeps_the_original_lease_before_polling_and_through_publication() {
    let mut f = fixture().await;
    Arc::get_mut(&mut f.registry).unwrap().router_leases = Arc::new(
        crate::router_lease::Users::new(crate::router_lease::Config {
            box_id: Some("01".into()),
            enforce: true,
            ..Default::default()
        }),
    );
    grant(&f, 1);
    let resolved = resolve(&f, now_ms() + 60_000).await;
    grant(&f, 2);
    assert!(
        f.registry
            .router_leases
            .get("alice")
            .unwrap()
            .permit()
            .allowed()
    );
    assert!(
        publish(&f, &resolved).await.is_err(),
        "a new lease cannot authorize old metadata"
    );
    assert_eq!(f.registry.account_count(), 0);
    let before = f.reads.load(Ordering::SeqCst);
    let future = f.registry.resolve_identity(
        "alice",
        init(),
        Some(("synthetic-token".into(), now_ms() + 60_000)),
        Arc::new(|| true),
    );
    grant(&f, 3);
    assert!(
        future.await.is_err(),
        "authority is captured before the task's first poll"
    );
    assert_eq!(f.reads.load(Ordering::SeqCst), before);
    let fresh = resolve(&f, now_ms() + 60_000).await;
    let account = publish(&f, &fresh).await.unwrap();
    assert_eq!(account.user, "alice");
    f.registry.dispose(&account, Disposal::PageDisposed);
}
