//! Binance family uses the shared production Session/Registry/HPKE negative-join harness.
#[path = "join_negative/binance.rs"]
mod binance_fixture;
#[path = "join_negative/mod.rs"]
mod join_negative;
use binance_fixture::Binance;
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
suite!(usd_m, Binance<0,false>);
suite!(usd_m_testnet, Binance<0,true>);
suite!(portfolio_margin, Binance<1,false>);
suite!(portfolio_margin_testnet, Binance<1,true>);
suite!(inverse, Binance<2,false>);
suite!(inverse_testnet, Binance<2,true>);
suite!(spot, Binance<3,false>);
suite!(spot_testnet, Binance<3,true>);
suite!(options, Binance<4,false>);
#[tokio::test]
async fn fixture_auth_refusal_has_unauthorized_http_status() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let f = Fixture::new::<Binance<0, false>>().await;
    let http = f.websocket_url.replacen("ws://", "http://", 1);
    let base = http.strip_suffix("/ws").unwrap();
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
        .get(format!(
            "{base}/fapi/v3/account?timestamp=1&signature=invalid"
        ))
        .header("X-MBX-APIKEY", Binance::<0, false>::init().key)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["code"], -2015);
    assert_eq!(f.registry.account_count(), 0);
    let seen = f.seen.lock().unwrap();
    assert_eq!((seen.signed_reads, seen.rejections, seen.writes), (1, 1, 0));
}
#[tokio::test]
async fn options_testnet_refuses_without_private_io() {
    type V = Binance<4, true>;
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
