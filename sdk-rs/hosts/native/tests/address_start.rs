#![cfg(all(not(target_arch = "wasm32"), feature = "address-nonce"))]
#[path = "support/address_start.rs"]
mod support;
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::nonce::{ASTERDEX_V3, HYPERLIQUID_AGENT};
use safe_cex_engine::signing::*;
use safe_cex_engine::*;
use safe_cex_native::{Events, FrameServer, Host, ServerConfig, Transport};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::*;
use tokio::sync::mpsc;

struct NoTransport;
impl Transport for NoTransport {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("address startup must not connect")
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("address startup must not send HTTP")
    }
}
fn host(
    domain: Option<&'static safe_cex_engine::nonce::NonceDomain>,
    supplied: bool,
    slots: AddressSetupSlots,
) -> (Host<Probe, NoTransport>, Observations) {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let key = KeyRef("actual-scalar".into());
    let mut provider = LocalSigner::new();
    let mut scalar = vec![0; 32];
    scalar[31] = 1;
    provider
        .insert_declared(key.clone(), scalar, Algorithm::Secp256k1Digest)
        .unwrap();
    let primary = Identity::new(KeyRef("credential".into()), Some(key));
    let identities = Identities {
        primary: Some(primary.clone()),
        secondary: Some(primary),
    };
    let engine = engine(
        Probe {
            observations: observations.clone(),
            domain,
            slots,
        },
        supplied.then(|| Arc::new(provider) as Provider),
        identities,
    );
    (Host::new(engine, NoTransport), observations)
}
#[tokio::test]
async fn host_start_initializes_local_alias_slots_before_venue_start() {
    for domain in [Some(&HYPERLIQUID_AGENT), Some(&ASTERDEX_V3), None] {
        let (host, observations) = host(domain, true, AddressSetupSlots::Both);
        let (commands, inbox) = mpsc::unbounded_channel();
        drop(commands);
        let engine = host
            .run(inbox, Duration::from_millis(1), |_| {}, |_| {})
            .await;
        assert_eq!(
            *observations.lock().unwrap(),
            vec![vec![Ok(ADDRESS), Ok(ADDRESS)]]
        );
        assert_eq!(
            engine
                .identities()
                .primary
                .as_ref()
                .unwrap()
                .verified_signing_address(),
            Ok(ADDRESS)
        );
    }
}
#[tokio::test]
async fn frame_server_start_initializes_local_alias_slots_before_venue_start() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (host, observations) = host(Some(&ASTERDEX_V3), true, AddressSetupSlots::Both);
            let mut server = FrameServer::new(ServerConfig::default()).unwrap();
            server.add_account("probe", "opaque-token", host).unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while observations.lock().unwrap().is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                *observations.lock().unwrap(),
                vec![vec![Ok(ADDRESS), Ok(ADDRESS)]]
            );
            drop(server);
        })
        .await;
}
#[tokio::test]
async fn unsupported_host_remains_public_and_undeclared_slots_get_no_binding() {
    for (supplied, slots, error) in [
        (false, AddressSetupSlots::Both, SignError::Unsupported),
        (true, AddressSetupSlots::None, SignError::InvalidRequest),
    ] {
        let (host, observations) = host(None, supplied, slots);
        let (commands, inbox) = mpsc::unbounded_channel();
        drop(commands);
        let engine = host
            .run(inbox, Duration::from_millis(1), |_| {}, |_| {})
            .await;
        assert_eq!(
            *observations.lock().unwrap(),
            vec![vec![Err(error), Err(error)]]
        );
        assert!(engine.address_nonce_setup_ready());
        assert!(!engine.failed());
    }
}

#[tokio::test]
async fn host_start_reuses_an_explicitly_completed_setup() {
    let (mut host, observations) = host(Some(&HYPERLIQUID_AGENT), true, AddressSetupSlots::Both);
    assert_eq!(
        host.initialize_address_setup().unwrap(),
        AddressSetupProgress::Ready
    );
    let (commands, inbox) = mpsc::unbounded_channel();
    drop(commands);
    let engine = host
        .run(inbox, Duration::from_millis(1), |_| {}, |_| {})
        .await;
    assert_eq!(
        *observations.lock().unwrap(),
        vec![vec![Ok(ADDRESS), Ok(ADDRESS)]]
    );
    assert_eq!(
        engine
            .identities()
            .primary
            .as_ref()
            .unwrap()
            .verified_signing_address(),
        Ok(ADDRESS)
    );
}
