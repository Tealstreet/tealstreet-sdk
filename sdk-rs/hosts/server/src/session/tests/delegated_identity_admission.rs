use super::*;
use crate::accounts::{Account, testing};
use std::future::Future;
use std::task::Poll;

fn public_account(registry: &Registry, user: &str, init: &InitPayload) -> Arc<Account> {
    let (account, _inbox) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        (user, &init.account_name, &init.exchange_name),
        None,
        init.fingerprint(),
        Arc::default(),
    );
    assert!(account.is_public());
    testing::register(registry, &account);
    account
}

#[tokio::test]
async fn fixround_public_coexistence_preserves_identity_settings_gate() {
    tokio::task::LocalSet::new().run_until(async {
        for parked in [false, true] {
            for owner in [None, Some("bob"), Some("alice")] {
                let registry = crate::accounts::tests::registry();
                let private = InitPayload {
                    account_name: "desktop".into(), exchange_name: "bybit".into(),
                    key: "synthetic-key".into(), ..Default::default()
                };
                let alias = InitPayload { account_name: "phone".into(), ..private.clone() };
                let public: Vec<_> = [&private, &alias].into_iter().map(|init| InitPayload {
                    public_only: Some(true), ..init.clone()
                }).collect();
                let public_accounts: Vec<_> = owner.into_iter().flat_map(|owner| {
                    public.iter().map(|init| public_account(&registry, owner, init)).collect::<Vec<_>>()
                }).collect();
                let account = testing::trading_account(&registry, "alice", &private);
                for init in [&private, &alias] {
                    let joined = registry.obtain("alice", init, false, 7, None, Arc::new(|_| true)).await.unwrap();
                    assert!(Arc::ptr_eq(&joined, &account));
                }
                if parked { registry.park("resume".into(), "alice", vec![(account.clone(), 7)]); }
                for init in &public {
                    for holder in [7, 8] {
                        assert!(registry.rebuild_refusal("alice", init, true, holder).is_some(),
                            "same-user public coexistence cannot hide the private account: owner={owner:?}, parked={parked}");
                        assert!(registry.obtain("alice", init, true, holder, None, Arc::new(|_| true)).await.is_err());
                    }
                }
                assert!(account.handle.disposal_cause().is_none());
                registry.dispose(&account, Disposal::PageDisposed);
                for account in public_accounts { registry.dispose(&account, Disposal::PageDisposed); }
            }
        }
    }).await;
}

#[tokio::test]
async fn fixround_public_coexistence_socket_keeps_the_private_registration() {
    tokio::task::LocalSet::new().run_until(async {
        for renamed in [false, true] {
            for owner in [None, Some("bob"), Some("alice")] {
                let registry = crate::accounts::tests::registry();
                let private = InitPayload {
                    account_name: "desktop".into(), exchange_name: "bybit".into(),
                    key: "synthetic-key".into(), instance_id: Some("private-registration".into()),
                    ..Default::default()
                };
                let labelled = InitPayload {
                    account_name: if renamed { "phone" } else { "desktop" }.into(), ..private.clone()
                };
                let public = InitPayload { public_only: Some(true), ..labelled.clone() };
                let public_account = owner.map(|owner| public_account(&registry, owner, &public));
                let account = testing::trading_account(&registry, "alice", &private);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                session.handle(&json!({"t":"init", "d":labelled}).to_string()).await.unwrap();
                assert!(Arc::ptr_eq(&session.joined[&labelled.account_name].account, &account));
                while sent.try_recv().is_ok() {}
                session.handle(&json!({"t":"init", "d":public}).to_string()).await.unwrap();
                assert!(Arc::ptr_eq(&session.joined[&labelled.account_name].account, &account),
                    "a public actor must not replace the held identity: owner={owner:?}, alias={renamed}");
                assert!(registry.holds(&account, session.socket));
                assert!(account.handle.disposal_cause().is_none());
                assert!(session.joined[&labelled.account_name].out.order_binding(Some("private-registration")));
                let replies: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                    .map(|text| serde_json::from_str(&text).unwrap()).collect();
                assert!(replies.iter().any(|frame| frame["d"]["data"][1]["initRefused"] == true));
                if let Some(account) = public_account { registry.dispose(&account, Disposal::PageDisposed); }
                session.close();
            }
        }
    }).await;
}

async fn settings_expiry_case(expired: bool) {
    let f = fixture().await;
    let (mut caller, _a_sent) = user_session(f.registry.clone(), "alice");
    let mut done = caller.take_verified();
    admitted(&mut caller, "acct", "synthetic-key", "current", &mut done).await;
    let original = caller.joined["acct"].account.clone();
    let provider = original.delegated.as_ref().unwrap().provider();
    let old_socket = caller.socket;
    let (release, blocked) = std::sync::mpsc::channel();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    assert!(f.registry.threads().place(move |_| {
        entered.send(()).unwrap();
        blocked.recv_timeout(Duration::from_secs(20)).unwrap();
    }));
    waiting.await.unwrap();
    let baseline = f.registry.threads().accounts();
    let settings = InitPayload {
        account_name: "acct".into(),
        exchange_name: "bybit".into(),
        key: "synthetic-key".into(),
        signing_mode: Some("delegated".into()),
        enable_spot_markets: Some(true),
        ..Default::default()
    };
    let token = crate::auth::unsigned_token(
        "alice",
        crate::accounts::now_ms() / 1000 + if expired { 2 } else { 60 },
    );
    let verified = crate::auth::Verifier::unsigned()
        .verify(&token, crate::accounts::now_ms())
        .unwrap();
    assert_eq!(caller.user(), verified.user_id);
    let deadline = verified.expires_at_ms;
    caller.identity.token = Some((token, deadline));
    let work = caller.settings_change(settings, original.clone());
    tokio::pin!(work);
    assert!(
        crate::accounts::now_ms() < deadline,
        "capture precedes token expiry"
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(work.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            if f.registry.threads().accounts() > baseline {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("settings construction queues on the real engine lane");
    let (next, mut b_sent) = user_session(f.registry.clone(), "bob");
    caller = next;
    if expired {
        tokio::time::sleep(Duration::from_millis(
            deadline.saturating_sub(crate::accounts::now_ms()) + 10,
        ))
        .await;
        assert!(crate::accounts::now_ms() >= deadline);
    }
    assert!(original.handle.disposal_cause().is_none());
    release.send(()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .unwrap();
    if expired {
        assert!(
            result.is_err(),
            "expired captured settings must refuse before replacing the resident"
        );
        assert!(original.handle.disposal_cause().is_none());
        assert!(Arc::ptr_eq(
            &f.registry.held("alice", "acct", old_socket).unwrap(),
            &original
        ));
    } else {
        let rebuilt = result.expect("valid captured token remains a positive control");
        assert_eq!(rebuilt.user, "alice");
        assert!(f.registry.holds(&rebuilt, old_socket));
        assert!(!f.registry.holds(&rebuilt, caller.socket));
        assert!(Arc::ptr_eq(
            &provider,
            &rebuilt.delegated.as_ref().unwrap().provider()
        ));
    }
    assert!(
        b_sent.try_recv().is_err(),
        "the old continuation never adopts the new user's output"
    );
    let mut fresh = caller.take_verified();
    admitted(
        &mut caller,
        "acct",
        "synthetic-key",
        "bob-current",
        &mut fresh,
    )
    .await;
    assert_eq!(caller.joined["acct"].account.user, "bob");
    assert_ne!(
        original.registry_name(),
        caller.joined["acct"].account.registry_name()
    );
    let alice = f.registry.held("alice", "acct", old_socket).unwrap();
    f.registry.dispose(&alice, Disposal::PageDisposed);
    caller.close();
}

#[tokio::test]
async fn fixround_settings_expiry_during_placement_keeps_resident() {
    settings_expiry_case(true).await;
}

#[tokio::test]
async fn fixround_settings_valid_deadline_keeps_captured_owner() {
    settings_expiry_case(false).await;
}
