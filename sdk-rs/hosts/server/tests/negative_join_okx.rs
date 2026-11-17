//! OKX family uses the real shared Session/Registry/HPKE join suite.
#[path = "join_negative/mod.rs"]
mod join_negative;
#[path = "join_negative/okx.rs"]
mod okx_fixture;
use join_negative::*;
use okx_fixture::Okx;
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
suite!(mainnet, Okx<0>);
suite!(demo, Okx<1>);
suite!(eea, Okx<2>);
suite!(eea_demo, Okx<3>);
#[tokio::test]
async fn fixture_routes_only_to_loopback() {
    let f = Fixture::new::<Okx<0>>().await;
    assert!(f.websocket_url.starts_with("ws://127.0.0.1:"));
}
