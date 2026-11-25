//! ApeX Omni negative joins use the shared production admission harness.
#[path = "join_negative/apexomni.rs"]
mod apexomni_fixture;
#[path = "join_negative/mod.rs"]
mod join_negative;
use apexomni_fixture::Apex;
use join_negative::*;
macro_rules! suite {
    ($module:ident, $venue:ty) => {
        mod $module {
            use super::*;
            #[tokio::test]
            async fn second_key_is_a_valid_independent_pair() {
                independent_pair::<$venue>().await;
            }
            #[tokio::test]
            async fn delegated_consumed_handover_preserves_registration() {
                delegated_consumed::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_refused_proof_read_is_never_re_signed() {
                refused_proof_read_is_never_re_signed::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_other_valid_key_preserves_resident() {
                cosigned_wrong_key::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_prior_attempt_signature_preserves_resident() {
                cosigned_replay::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_consumed_response_cannot_confirm_twice() {
                cosigned_consumed::<$venue>().await;
            }
            #[tokio::test]
            async fn delegated_wrong_credentials_preserve_resident() {
                delegated_wrong_pair::<$venue>().await;
            }
            #[tokio::test]
            async fn delegated_old_seal_cannot_join_a_new_attempt() {
                delegated_replay::<$venue>().await;
            }
        }
    };
}
suite!(mainnet, Apex<false>);
suite!(testnet, Apex<true>);
#[tokio::test]
async fn fixture_routes_only_to_loopback() {
    let f = Fixture::new::<Apex<false>>().await;
    assert!(f.websocket_url.starts_with("ws://127.0.0.1:"));
}

// AGT-75: setup is answered correctly and only private proof reads get a wrong-key
// signature. The refused read fails the join; no rounded legacy re-sign is requested.
async fn refused_proof_read_is_never_re_signed<V: Venue>() {
    let f = Fixture::new::<V>().await;
    let mut client = Client::new(&f);
    client.init(V::init()).await;
    client
        .finish_with::<V>(&mut Device::default(), |frame| {
            if V::replayable(frame) {
                Answer::OtherKey
            } else {
                Answer::Correct
            }
        })
        .await;
    let mut requests = std::collections::BTreeMap::new();
    for frame in &client.frames {
        if frame["t"] == "sign-req" && V::replayable(frame) {
            let (endpoint, _, _) = V::signature(frame, false);
            *requests.entry(endpoint).or_insert(0_usize) += 1;
        }
    }
    assert!(!requests.is_empty(), "the private proof must actually run");
    assert!(
        requests.values().all(|count| *count == 1),
        "a refused proof read was signed again: {requests:?}"
    );
    assert!(f.registry.held(USER, ACCOUNT, client.socket()).is_none());
    assert!(
        client
            .frames
            .iter()
            .any(|frame| frame["d"]["data"][1]["initRefused"] == true)
    );
    assert_eq!(f.seen.lock().unwrap().writes, 0);
}
