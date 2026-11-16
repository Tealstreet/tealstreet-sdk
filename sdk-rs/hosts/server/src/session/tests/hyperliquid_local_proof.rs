//! Production selection, not a fixture-injected local-proof binding.
use super::*;
use crate::delegated_e2e_io::{MockVenue, Observed};
use safe_cex_crypto::admission::{
    Family,
    identity::Projection,
    wire::{Request, Response},
};
use std::{sync::Mutex, time::Duration};

fn scalar(last: u8) -> String {
    let mut bytes = [0; 32];
    bytes[31] = last;
    safe_cex_crypto::hex(&bytes)
}

fn address(last: u8) -> String {
    safe_cex_crypto::ethereum::private_key_address(&scalar(last)).unwrap()
}

fn init(venue: &str) -> InitPayload {
    serde_json::from_value(json!({
        "accountName":"acct", "exchangeName":venue, "signingMode":"cosigned",
        "instanceId":"hyperliquid-registration", "key":address(8),
        "hyperliquidSignerAddress":address(7)
    }))
    .unwrap()
}

fn registry() -> (Arc<Registry>, Arc<Mutex<Observed>>) {
    let mock = MockVenue::new("synthetic-key".into(), "synthetic-material".into());
    let observed = mock.observed.clone();
    (
        crate::accounts::testing::e2e_registry("alice", mock),
        observed,
    )
}

fn no_io(observed: &Mutex<Observed>) {
    let seen = observed.lock().unwrap();
    assert_eq!(
        (seen.preparations, seen.requests, seen.sockets),
        (0, 0, 0),
        "public account reads and venue writes cannot substitute for possession"
    );
}

async fn challenge(sent: &mut crate::out::Receiver) -> Request {
    let text = tokio::time::timeout(Duration::from_secs(5), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "production Hyperliquid admission must issue its own challenge"
    );
    Request::from_json(&frame["d"].to_string()).unwrap()
}

fn response(request: &Request, last: u8) -> String {
    let signature = safe_cex_crypto::ethereum::sign_digest(
        &request.statement().message(Family::Ethereum),
        &scalar(last),
    )
    .unwrap();
    let hex = signature.strip_prefix("0x").unwrap_or(&signature);
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    json!({"t":"sign-res", "d":Response::signed(request.id(), Family::Ethereum, &bytes).unwrap()})
        .to_string()
}

async fn correct_and_other_key(venue: &str) {
    for correct in [true, false] {
        let (registry, observed) = registry();
        let resident = crate::accounts::testing::trading_account(&registry, "alice", &init(venue));
        let original_link = resident.link.clone().unwrap();
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        let mut done = session.take_verified();
        session.init(init(venue)).await.unwrap();
        assert!(session.joined.is_empty());
        let request = challenge(&mut sent).await;
        let projection = Projection::from_json(
            "alice",
            &json!({"venue":venue,"network":"mainnet",
            "credential":{"kind":"agent","signer":address(7),"accountScope":{"kind":"main"}}})
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(request.statement()).unwrap()["identityDigest"],
            json!(projection.digest())
        );
        assert_eq!(request.instance_id(), "hyperliquid-registration");
        session
            .handle(&response(&request, if correct { 7 } else { 8 }))
            .await
            .unwrap();
        let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(completed.probe.as_ref().unwrap().permit.is_ok(), correct);
        session.verification_completed(completed).await.unwrap();
        assert_eq!(registry.holds(&resident, session.socket), correct);
        assert_eq!(session.joined.is_empty(), !correct);
        assert_eq!(registry.account_count(), 1);
        if !correct {
            assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original_link));
        }
        no_io(&observed);
        session.close();
    }
}

async fn replay_and_foreign_socket(venue: &str) {
    let (registry, observed) = registry();
    let resident = crate::accounts::testing::trading_account(&registry, "alice", &init(venue));
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    let mut done = session.take_verified();
    session.init(init(venue)).await.unwrap();
    let first = challenge(&mut sent).await;
    session.init(init(venue)).await.unwrap();
    let second = challenge(&mut sent).await;
    assert_ne!(first.statement(), second.statement());
    let mut replay: Value = serde_json::from_str(&response(&first, 7)).unwrap();
    replay["d"]["id"] = second.id().into();
    session.handle(&replay.to_string()).await.unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(completed.probe.as_ref().unwrap().permit.is_err());
    session.verification_completed(completed).await.unwrap();
    assert!(session.joined.is_empty());
    assert!(!registry.holds(&resident, session.socket));
    while sent.try_recv().is_ok() {}
    session.init(init(venue)).await.unwrap();
    let third = challenge(&mut sent).await;
    let reply = response(&third, 7);
    for owner in ["alice", "bob"] {
        let (mut foreign, _) = user_session(registry.clone(), owner);
        foreign.handle(&reply).await.unwrap();
        tokio::task::yield_now().await;
        assert!(done.try_recv().is_err());
        assert!(!registry.holds(&resident, foreign.socket));
        foreign.close();
    }
    session.handle(&reply).await.unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    session.verification_completed(completed).await.unwrap();
    assert!(registry.holds(&resident, session.socket));
    session.handle(&reply).await.unwrap();
    tokio::task::yield_now().await;
    assert!(
        done.try_recv().is_err(),
        "a consumed response cannot settle twice"
    );
    no_io(&observed);
    session.close();
}

#[tokio::test]
async fn hyperliquid_signer_not_read_wallet() {
    tokio::task::LocalSet::new()
        .run_until(correct_and_other_key("hyperliquid"))
        .await;
}

#[tokio::test]
async fn hyperliquid_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(replay_and_foreign_socket("hyperliquid"))
        .await;
}

#[tokio::test]
async fn hyperliquidspot_signer_not_read_wallet() {
    tokio::task::LocalSet::new()
        .run_until(correct_and_other_key("hyperliquidspot"))
        .await;
}

#[tokio::test]
async fn hyperliquidspot_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(replay_and_foreign_socket("hyperliquidspot"))
        .await;
}

#[tokio::test]
async fn hyperliquid_scope_captures_main_vault_network_and_verified_owner() {
    let mut digests = Vec::new();
    for venue in ["hyperliquid", "hyperliquidspot"] {
        let mut scopes = Vec::new();
        for (owner, vault, testnet) in [
            ("alice", None, false),
            ("alice", Some(address(9)), false),
            ("alice", Some(address(10)), false),
            ("alice", None, true),
            ("bob", None, false),
        ] {
            let (registry, observed) = registry();
            let (mut session, mut sent) = user_session(registry, owner);
            let mut payload = init(venue);
            payload.vault = vault;
            payload.testnet = Some(testnet);
            session.init(payload).await.unwrap();
            let request = challenge(&mut sent).await;
            let digest =
                serde_json::to_value(request.statement()).unwrap()["identityDigest"].clone();
            assert!(!scopes.contains(&digest));
            scopes.push(digest);
            no_io(&observed);
            session.close();
        }
        digests.push(scopes);
    }
    assert_eq!(
        digests[0], digests[1],
        "spot and perpetual share the agent namespace"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn hyperliquid_delegated_candidate_uses_the_same_captured_signer_without_device_or_venue_io()
{
    for venue in ["hyperliquid", "hyperliquidspot"] {
        for correct in [true, false] {
            let (registry, observed) = registry();
            let mut payload = init(venue);
            payload.signing_mode = Some("delegated".into());
            let credentials: crate::delegated::Credentials = serde_json::from_str(
                &json!({
                    "key":payload.key, "secret":scalar(if correct { 7 } else { 8 })
                })
                .to_string(),
            )
            .unwrap();
            let permit = registry.probe_permit("alice", &payload).unwrap();
            let result = registry
                .probe_join(
                    "alice",
                    &payload,
                    Some(("synthetic-token".into(), now_ms() + 60_000)),
                    Some(&credentials),
                    None,
                    11,
                    permit,
                )
                .await;
            assert_eq!(result.is_ok(), correct);
            assert_eq!(registry.account_count(), 0);
            no_io(&observed);
        }
    }
}

#[test]
fn hyperliquid_signer_address_is_captured_by_the_init_fingerprint() {
    for venue in ["hyperliquid", "hyperliquidspot"] {
        let first = init(venue);
        let mut changed = serde_json::to_value(&first).unwrap();
        changed["hyperliquidSignerAddress"] = address(8).into();
        let changed: InitPayload = serde_json::from_value(changed).unwrap();
        assert_ne!(first.fingerprint(), changed.fingerprint());
    }
}

#[tokio::test]
async fn hyperliquid_missing_or_invalid_signer_never_falls_back_to_unsigned_reads() {
    for venue in ["hyperliquid", "hyperliquidspot"] {
        for signer in [
            Value::Null,
            json!(""),
            json!("not-an-address"),
            json!(format!("0x{}", "00".repeat(20))),
        ] {
            let (registry, observed) = registry();
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut payload = serde_json::to_value(init(venue)).unwrap();
            payload["hyperliquidSignerAddress"] = signer;
            session
                .init(serde_json::from_value(payload).unwrap())
                .await
                .unwrap();
            let text = tokio::time::timeout(Duration::from_secs(5), sent.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                text,
                envelope::with_instance_id(
                    envelope::init_proof_unavailable("acct"),
                    "hyperliquid-registration"
                )
            );
            assert!(session.pending_verifications.is_empty());
            assert!(session.joined.is_empty());
            assert_eq!(registry.account_count(), 0);
            no_io(&observed);
            session.close();
        }
    }
}

#[tokio::test]
async fn hyperliquid_old_owner_completion_cannot_attach_to_a_successor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in ["hyperliquid", "hyperliquidspot"] {
                let (registry, observed) = registry();
                let alice =
                    crate::accounts::testing::trading_account(&registry, "alice", &init(venue));
                let bob = crate::accounts::testing::trading_account(&registry, "bob", &init(venue));
                let (mut old, mut old_sent) = user_session(registry.clone(), "alice");
                let mut old_done = old.take_verified();
                old.init(init(venue)).await.unwrap();
                let request = challenge(&mut old_sent).await;
                old.handle(&response(&request, 7)).await.unwrap();
                let completion = tokio::time::timeout(Duration::from_secs(5), old_done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                old.close();
                let (mut successor, mut sent) = user_session(registry.clone(), "bob");
                let mut done = successor.take_verified();
                successor.init(init(venue)).await.unwrap();
                let request = challenge(&mut sent).await;
                successor.verification_completed(completion).await.unwrap();
                assert!(successor.joined.is_empty());
                assert!(!registry.holds(&alice, successor.socket));
                assert!(!registry.holds(&bob, successor.socket));
                successor.handle(&response(&request, 7)).await.unwrap();
                let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                successor.verification_completed(completion).await.unwrap();
                assert!(registry.holds(&bob, successor.socket));
                assert!(!registry.holds(&alice, successor.socket));
                no_io(&observed);
                successor.close();
            }
        })
        .await;
}
