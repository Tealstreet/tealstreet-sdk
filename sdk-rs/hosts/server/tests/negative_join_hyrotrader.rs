//! HyroTrader family uses production Session, private-startup proof, signer and HPKE paths.
#[path = "join_negative/hyrotrader.rs"]
mod hyrotrader_fixture;
#[path = "join_negative/mod.rs"]
mod join_negative;
use hyrotrader_fixture::HyroTrader;
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
            #[tokio::test]
            async fn delegated_consumed_handover_preserves_registration() {
                delegated_consumed::<$venue>().await;
            }
        }
    };
}
suite!(mainnet, HyroTrader<false>);

#[tokio::test]
async fn native_string_login_and_private_subscriptions_are_socket_bound() {
    use futures_util::{SinkExt, StreamExt};
    use safe_cex_crypto::{Encoding, hmac_sha256};
    use safe_cex_exchange_server::accounts::now_ms;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
    async fn reply<S>(socket: &mut WebSocketStream<S>) -> Value
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("Hyro loopback reply")
            .unwrap()
            .unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
    let fixture = Fixture::new::<HyroTrader>().await;
    for other in [false, true] {
        let pair = HyroTrader::<false>::credentials(other);
        let expiry = now_ms() + 10_000;
        let auth = json!({"op":"auth","args":[pair["key"],expiry.to_string(),hmac_sha256(pair["secret"].as_str().unwrap().as_bytes(),format!("GET/realtime{expiry}").as_bytes(),Encoding::Hex)]});
        let private = json!({"op":"subscribe","args":["order","position","execution","wallet"]});
        let (mut socket, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
            .await
            .unwrap();
        socket
            .send(Message::text(private.to_string()))
            .await
            .unwrap();
        assert_eq!(reply(&mut socket).await["success"], false);
        socket.send(Message::text(auth.to_string())).await.unwrap();
        assert_eq!(reply(&mut socket).await["success"], true);
        socket
            .send(Message::text(private.to_string()))
            .await
            .unwrap();
        assert_eq!(reply(&mut socket).await["success"], true);
        let (mut foreign, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
            .await
            .unwrap();
        foreign
            .send(Message::text(private.to_string()))
            .await
            .unwrap();
        assert_eq!(reply(&mut foreign).await["success"], false);
        foreign.close(None).await.unwrap();
        let mut invalid = Vec::new();
        for (index, value) in [
            (0, json!("synthetic-unknown-key")),
            (1, json!("0")),
            (1, json!(expiry)),
            (1, json!(format!("0{expiry}"))),
            (1, json!("bad")),
            (2, json!("bad-mac")),
        ] {
            let mut bad = auth.clone();
            bad["args"][index] = value;
            invalid.push(bad);
        }
        invalid.push(json!({"op":"auth","args":[]}));
        invalid.push(
            json!({"op":"auth","args":[pair["key"],auth["args"][1],auth["args"][2],"extra"]}),
        );
        for bad in invalid {
            socket.send(Message::text(bad.to_string())).await.unwrap();
            assert_eq!(reply(&mut socket).await["success"], false);
            socket
                .send(Message::text(private.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut socket).await["success"], false);
        }
        socket.close(None).await.unwrap();
    }
    assert_eq!(fixture.registry.account_count(), 0);
    assert_eq!(fixture.seen.lock().unwrap().writes, 0);
}

#[tokio::test]
async fn hosted_testnet_refuses_without_private_io() {
    type V = HyroTrader<true>;
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
