//! A real keyless Hyperliquid engine, built from the server's own init path,
//! binds its agent address through the remote signer link: sign-req out, a
//! keyed device answers sign-res, and the engine verifies the address.
use safe_cex_engine::signing::{Identities, Provider};
use safe_cex_exchange_server::accounts::engine_options;
use safe_cex_exchange_server::envelope::InitPayload;
use safe_cex_remote_signer::{SignerLink, remote_account};
use safe_cex_venues::hosted_wallet::{self, Account};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use tokio::sync::mpsc;

const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const AGENT_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const SOCKET: u64 = 7;

/// The browser's keyed device: the same Rust code the WASM `hostedWalletSigning` runs.
fn device() -> (Provider, Identities) {
    let mut options: safe_cex_venues::EngineOptions =
        serde_json::from_value(json!({ "key": ACCOUNT, "secret": AGENT_KEY })).unwrap();
    safe_cex_venues::prepare_local_signer_for("hyperliquid", &mut options).unwrap()
}

struct Bind;
impl safe_cex_venues::Visit for Bind {
    type Output = std::pin::Pin<Box<dyn std::future::Future<Output = Option<[u8; 20]>>>>;
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        mut engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        Box::pin(async move {
            engine.initialize_address_nonce(0.).unwrap();
            std::future::poll_fn(|cx| {
                engine.poll_address_setup(0., cx);
                if engine.address_nonce_setup_ready() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            assert!(!engine.failed());
            engine
                .identities()
                .primary
                .as_ref()?
                .verified_signing_address()
                .ok()
        })
    }
}

#[tokio::test]
async fn a_keyless_hyperliquid_engine_binds_its_agent_through_the_link() {
    let (sender, mut frames) = mpsc::unbounded_channel::<String>();
    let link = SignerLink::new(SOCKET, Arc::new(move |frame| sender.send(frame).is_ok()));
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let (answering, record) = (Arc::downgrade(&link), seen.clone());
    tokio::spawn(async move {
        let (provider, identities) = device();
        let account = Account {
            key: Some(ACCOUNT.into()),
            ..Account::default()
        };
        while let Some(frame) = frames.recv().await {
            let request = serde_json::from_str::<Value>(&frame).unwrap()["d"].clone();
            record.lock().unwrap().push(request.clone());
            let answer = hosted_wallet::sign(
                "hyperliquid",
                &identities,
                &account,
                &provider,
                request["wsAuth"]["toSign"].as_str().unwrap(),
                request["wsAuth"]["outputFormat"].as_str().unwrap(),
                0.,
            );
            let response = match answer {
                Ok(signature) => json!({ "id": request["id"], "wsAuthSignature": signature }),
                Err(_) => json!({ "id": request["id"], "error": "refused" }),
            };
            if let Some(link) = answering.upgrade() {
                assert!(link.resolve(SOCKET, response));
            }
        }
    });

    let init = InitPayload {
        account_name: "main".into(),
        exchange_name: "hyperliquid".into(),
        key: ACCOUNT.into(),
        ..InitPayload::default()
    };
    let options = engine_options(&init, false).to_string();
    let remote =
        remote_account(link.clone(), "hyperliquid", Some("main".into()), &options).unwrap();
    let built: Value = serde_json::from_str(&remote.options_json).unwrap();
    assert!(built.get("key").is_none() && built.get("secret").is_none());

    let bound = safe_cex_venues::build_with_signer(
        "hyperliquid",
        serde_json::from_str(&remote.options_json).unwrap(),
        Some(safe_cex_native::production_entropy()),
        Some(remote.provider.clone()),
        Bind,
    )
    .unwrap()
    .await;

    let (provider, identities) = device();
    let expected = hosted_wallet::sign(
        "hyperliquid",
        &identities,
        &Account {
            key: Some(ACCOUNT.into()),
            ..Account::default()
        },
        &provider,
        ACCOUNT,
        "hyperliquid-address",
        0.,
    )
    .unwrap();
    let bound = bound.expect("the engine verified the device's agent address");
    let bound_hex: String = bound.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(format!("0x{bound_hex}"), expected.to_ascii_lowercase());
    let sent = seen.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["type"], "ws-auth");
    assert_eq!(sent[0]["exchangeName"], "hyperliquid");
    assert_eq!(sent[0]["accountName"], "main");
    assert_eq!(
        sent[0]["wsAuth"],
        json!({ "toSign": ACCOUNT, "outputFormat": "hyperliquid-address" })
    );
}
