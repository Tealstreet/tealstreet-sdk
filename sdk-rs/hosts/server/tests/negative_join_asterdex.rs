//! AsterDEX V1 negative joins use the shared production admission harness.
#[path = "join_negative/mod.rs"]
mod join_negative;
#[path = "join_negative/asterdex.rs"]
mod asterdex_fixture;
use join_negative::*;
use asterdex_fixture::Aster;
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
suite!(mainnet, Aster<false>);
suite!(testnet, Aster<true>);
#[tokio::test]
async fn fixture_routes_only_to_loopback() {
    let f = Fixture::new::<Aster<false>>().await;
    assert!(f.websocket_url.starts_with("ws://127.0.0.1:"));
}
#[tokio::test]
async fn wallet_v3_shapes_remain_cosigned_proof_unavailable() {
    for wallet_key in [false, true] {
        let f = Fixture::new::<Aster<false>>().await;
        let mut client = Client::new(&f);
        let mut init = Aster::<false>::init();
        if wallet_key { init.key = format!("0x{}", "11".repeat(20)); }
        else { init.application_id = Some("synthetic-main-wallet".into()); }
        client.init(init).await;
        client.finish::<Aster<false>>(&mut Device::default(), Answer::Correct).await;
        assert!(f.registry.held(USER, ACCOUNT, client.socket()).is_none());
        assert!(client.frames.iter().any(|v| v["d"]["data"][1]["kind"] == "proof-unavailable"));
        assert!(!client.frames.iter().any(|v| v["d"]["op"] == "account-signing-mode" || v["t"] == "sign-req"));
        let seen = f.seen.lock().unwrap();
        assert_eq!((seen.signed_reads, seen.rejections, seen.writes), (0, 0, 0));
    }
}
