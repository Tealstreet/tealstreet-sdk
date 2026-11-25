use super::*;
use crate::delegated::{DelegatedPair, prepare_delegated};
use crate::envelope::InitPayload;
use safe_cex_engine::{HttpRequest, SocketId};
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use tokio::sync::mpsc;

const WALLET: &str = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";
const MATERIAL: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

#[derive(Clone)]
struct WalletIo {
    private_allowed: bool,
    reads: Arc<Mutex<Vec<String>>>,
}
impl Transport for WalletIo {
    fn prepare_http(
        &self,
        _: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let io = self.clone();
        Box::pin(async move { Ok(Box::new(Prepared(io)) as Box<dyn PreparedHttp>) })
    }
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _ = events.send(HostEvent::WsOpened(socket));
        tokio::task::spawn_local(async move { while rx.recv().await.is_some() {} });
        tx
    }
    fn request(&self, request: HttpRequest, events: Events) {
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap_or("{}")).unwrap();
        let kind = body["type"].as_str().unwrap();
        self.reads.lock().unwrap().push(kind.into());
        let public = kind == "spotMetaAndAssetCtxs";
        let reply = match kind {
            "spotMetaAndAssetCtxs" => json!([
                {"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"PURR","index":1,"szDecimals":0}],
                "universe":[{"name":"PURR/USDC","tokens":[1,0],"index":0}]},
                [{"coin":"PURR/USDC","midPx":"0.2","markPx":"0.2","prevDayPx":"0.2","dayNtlVlm":"1"}]
            ]),
            "spotClearinghouseState" => {
                json!({"balances":[{"coin":"USDC","total":"1000","hold":"0"}]})
            }
            _ => json!([]),
        };
        let allowed = public || self.private_allowed && body["user"] == WALLET;
        let response = if allowed {
            reply.to_string()
        } else {
            "synthetic private read refused".into()
        };
        let _ = events.send(HostEvent::HttpStart {
            request: request.id,
            status: if allowed { 200 } else { 401 },
            headers: String::new(),
        });
        let _ = events.send(HostEvent::HttpChunk(request.id, response.into_bytes()));
        let _ = events.send(HostEvent::HttpEnd(request.id));
    }
}
#[derive(Debug)]
struct Prepared(WalletIo);
#[derive(Debug)]
struct Bound(WalletIo, HttpRequest);
impl std::fmt::Debug for WalletIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WalletIo")
    }
}
impl PreparedHttp for Prepared {
    fn bind(self: Box<Self>, request: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        Ok(Box::new(Bound(self.0, request)))
    }
}
impl BoundHttp for Bound {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        self.0.request(self.1, events);
        tokio::spawn(async {}).abort_handle()
    }
}
fn wallet_init() -> InitPayload {
    InitPayload {
        account_name: "fixture".into(),
        exchange_name: "hyperliquidspot".into(),
        key: WALLET.into(),
        application_id: Some(WALLET.into()),
        // MATERIAL's own address: construction checks the claimed signer.
        hyperliquid_signer_address: Some(WALLET.into()),
        ..Default::default()
    }
}

#[tokio::test(start_paused = true)]
async fn wallet_public_metadata_cannot_satisfy_private_admission() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for material in [
                MATERIAL,
                "0x0000000000000000000000000000000000000000000000000000000000000002",
            ] {
                let init = InitPayload {
                    hyperliquid_signer_address: Some(
                        safe_cex_crypto::ethereum::private_key_address(material).unwrap(),
                    ),
                    ..wallet_init()
                };
                let prepared = prepare_delegated(
                    &init,
                    DelegatedPair::new(WALLET.into(), material.into(), None),
                )
                .unwrap();
                let io = WalletIo {
                    private_allowed: false,
                    reads: Arc::default(),
                };
                let mut config = ServerConfig::default();
                prepared.resident.configure(&mut config);
                // Match probe_join's fail-closed budget when a local wallet
                // mismatch prevents private startup before any HTTP request.
                let answer = tokio::time::timeout(
                    Duration::from_secs(30),
                    run(
                        "hyperliquidspot",
                        &prepared.options,
                        prepared.resident.provider(),
                        io.clone(),
                        config,
                        Arc::new(|| true),
                    ),
                )
                .await
                .unwrap_or(Err(ProbeRefusal::TRANSIENT));
                assert!(
                    answer.is_err(),
                    "public metadata cannot prove a private account"
                );
                if material == MATERIAL {
                    assert!(
                        io.reads
                            .lock()
                            .unwrap()
                            .iter()
                            .any(|kind| kind != "spotMetaAndAssetCtxs")
                    );
                }
            }
        })
        .await;
}

#[tokio::test]
async fn wallet_private_startup_retains_the_public_address() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prepared = prepare_delegated(
                &wallet_init(),
                DelegatedPair::new(WALLET.into(), MATERIAL.into(), None),
            )
            .unwrap();
            let fields: Value = serde_json::from_str(&prepared.options).unwrap();
            assert_eq!(fields["key"], WALLET);
            assert!(fields.get("secret").is_none());
            let io = WalletIo {
                private_allowed: true,
                reads: Arc::default(),
            };
            let mut config = ServerConfig::default();
            prepared.resident.configure(&mut config);
            let answer = tokio::time::timeout(
                Duration::from_secs(3),
                run(
                    "hyperliquidspot",
                    &prepared.options,
                    prepared.resident.provider(),
                    io.clone(),
                    config,
                    Arc::new(|| true),
                ),
            )
            .await
            .unwrap();
            assert!(answer.is_ok(), "valid private startup control: {answer:?}");
            assert!(
                io.reads
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|kind| kind != "spotMetaAndAssetCtxs")
            );
        })
        .await;
}

fn stark_public(scalar: u8) -> String {
    let mut bytes = [0; 32];
    bytes[31] = scalar;
    format!(
        "0x{}",
        safe_cex_crypto::hex(&safe_cex_crypto::starknet::public_key_bytes(&bytes))
    )
}

fn paradex_init(subkey: &str) -> InitPayload {
    InitPayload {
        account_name: "fixture".into(),
        exchange_name: "paradex".into(),
        key: subkey.into(),
        paradex_address: Some("0x123".into()),
        subkey_address: Some(subkey.into()),
        ..Default::default()
    }
}

fn paradex_pair(subkey: &str) -> DelegatedPair {
    DelegatedPair::new(subkey.into(), String::new(), None).with_signing_key(MATERIAL.into())
}

#[test]
fn wallet_paradex_key_only_shape_uses_the_existing_trusted_mapping() {
    let subkey = stark_public(1);
    let input = json!({"key":MATERIAL,"paradexAddress":"0x123","subkeyAddress":subkey});
    let mut local: safe_cex_venues::EngineOptions = serde_json::from_value(input).unwrap();
    let (_, identities) = safe_cex_venues::prepare_local_signer_for("paradex", &mut local).unwrap();
    assert!(identities.primary.unwrap().signing_ref().is_ok());
    assert!(prepare_delegated(&paradex_init(&subkey), paradex_pair(&subkey)).is_ok());
}

#[test]
fn wallet_delegated_identity_claims_must_be_the_sealed_key() {
    let other_signer = safe_cex_crypto::ethereum::private_key_address(&format!("0x{:064x}", 2))
        .unwrap()
        .to_lowercase();
    assert_eq!(
        safe_cex_crypto::ethereum::private_key_address(MATERIAL)
            .unwrap()
            .to_lowercase(),
        WALLET
    );
    for venue in ["hyperliquid", "hyperliquidspot", "grvt"] {
        let init = |signer: Option<&str>| InitPayload {
            account_name: "fixture".into(),
            exchange_name: venue.into(),
            key: if venue == "grvt" {
                "synthetic-grvt-api-key"
            } else {
                WALLET
            }
            .into(),
            hyperliquid_signer_address: signer.filter(|_| venue != "grvt").map(Into::into),
            grvt_signer_address: signer.filter(|_| venue == "grvt").map(Into::into),
            grvt_trading_account_id: (venue == "grvt").then(|| "42".into()),
            ..Default::default()
        };
        let pair = |init: &InitPayload| DelegatedPair::new(init.key.clone(), MATERIAL.into(), None);
        let own = init(Some(WALLET));
        assert!(
            prepare_delegated(&own, pair(&own)).is_ok(),
            "{venue}: own signer"
        );
        let upper = init(Some(&WALLET.to_uppercase().replacen("0X", "0x", 1)));
        assert!(
            prepare_delegated(&upper, pair(&upper)).is_ok(),
            "{venue}: case"
        );
        for claim in [Some(other_signer.as_str()), None] {
            let claimed = init(claim);
            assert!(
                prepare_delegated(&claimed, pair(&claimed)).is_err(),
                "{venue}: {claim:?}"
            );
        }
    }
    let other = stark_public(2);
    assert!(prepare_delegated(&paradex_init(&other), paradex_pair(&other)).is_err());
}

#[test]
fn wallet_paradex_ethereum_derived_key_goes_through_the_sealed_signer_check() {
    // 64 hex digits is an Ethereum key; a 66-character key would be a Stark scalar.
    let ethereum = "11".repeat(32);
    let subkey_on = |chain: &str| {
        let scalar = safe_cex_crypto::starknet::derive_from_ethereum(&ethereum, chain).unwrap();
        let public = safe_cex_crypto::starknet::public_key_bytes(&scalar);
        format!("0x{}", safe_cex_crypto::hex(&public))
    };
    let pair = |subkey: &str| {
        DelegatedPair::new(subkey.into(), String::new(), None).with_signing_key(ethereum.clone())
    };
    // Mainnet derives on chain 1 (`safe_cex_venue_paradex::sign::ethereum_chain(false)`).
    let mainnet = subkey_on("1");
    assert!(prepare_delegated(&paradex_init(&mainnet), pair(&mainnet)).is_ok());
    for other in [subkey_on("11155111"), stark_public(1)] {
        assert!(
            prepare_delegated(&paradex_init(&other), pair(&other)).is_err(),
            "{other}"
        );
    }
    // Testnet derives on Sepolia (11155111); the mainnet derivation is another key there.
    let on_testnet = |subkey: &str| InitPayload {
        testnet: Some(true),
        ..paradex_init(subkey)
    };
    let testnet = subkey_on("11155111");
    assert!(prepare_delegated(&on_testnet(&testnet), pair(&testnet)).is_ok());
    assert!(prepare_delegated(&on_testnet(&mainnet), pair(&mainnet)).is_err());
}

#[test]
fn wallet_key_only_support_does_not_admit_empty_hmac_material() {
    for venue in ["bybit", "binance"] {
        let init = InitPayload {
            exchange_name: venue.into(),
            ..wallet_init()
        };
        assert!(
            prepare_delegated(
                &init,
                DelegatedPair::new(WALLET.into(), String::new(), None)
            )
            .is_err()
        );
    }
}

#[test]
fn wallet_resident_reconfiguration_keeps_public_setup_metadata() {
    let init = wallet_init();
    let prepared = prepare_delegated(
        &init,
        DelegatedPair::new(WALLET.into(), MATERIAL.into(), None),
    )
    .unwrap();
    let reconfigured = prepared.resident.reconfigure(&init).unwrap();
    let fields: Value = serde_json::from_str(&reconfigured.options).unwrap();
    assert_eq!(fields["key"], WALLET);
    assert!(fields.get("secret").is_none());
    let mut config = ServerConfig::default();
    reconfigured.resident.configure(&mut config);
    let debug = format!("{:?}", config.trusted_signer_state);
    assert!(!debug.contains(WALLET));
}
