//! KuCoin futures negative joins use the shared production admission harness.
#[path = "join_negative/mod.rs"]
mod join_negative;
#[path = "join_negative/kucoin.rs"]
mod kucoin_fixture;
use join_negative::*;
use kucoin_fixture::Kucoin;
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
suite!(mainnet, Kucoin<false>);
#[tokio::test]
async fn fixture_routes_only_to_loopback() {
    let f = Fixture::new::<Kucoin<false>>().await;
    assert!(f.websocket_url.starts_with("ws://127.0.0.1:"));
}
#[tokio::test]
async fn hosted_testnet_refuses_without_private_io() {
    type V = Kucoin<true>;
    let f = Fixture::new::<V>().await;
    assert!(f.websocket_url.starts_with("ws://127.0.0.1:"));
    for delegated in [false, true] {
        let mut client = Client::new(&f);
        let mut init = V::init();
        if delegated {
            let offer = client.offer::<V>().await;
            init.signing_mode = Some("delegated".into());
            init.sealed =
                Some(serde_json::from_value(seal(&offer, &init, V::credentials(false))).unwrap());
        }
        client.init(init).await;
        client
            .finish::<V>(&mut Device::default(), Answer::Correct)
            .await;
        assert!(f.registry.held(USER, ACCOUNT, client.socket()).is_none());
        assert!(
            client
                .frames
                .iter()
                .any(|v| v["d"]["data"][1]["initRefused"] == true)
        );
        assert!(
            !client
                .frames
                .iter()
                .any(|v| v["d"]["op"] == "account-signing-mode" || v["t"] == "sign-req")
        );
    }
    let seen = f.seen.lock().unwrap();
    assert_eq!((seen.signed_reads, seen.rejections, seen.writes), (0, 0, 0));
}
