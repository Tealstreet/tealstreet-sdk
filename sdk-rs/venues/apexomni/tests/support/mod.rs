#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_apexomni::{ApexOmni, Options};
pub type TestEngine = Engine<ApexOmni>;
pub const MARKETS: &str = r#"{"code":0,"data":{"contractConfig":{"perpetualContract":[{"symbol":"BTC-USDT","crossSymbolName":"BTCUSDT","baseTokenId":"BTC","settleAssetId":"USDT","enableTrade":true,"stepSize":"0.001","tickSize":"0.1","minOrderSize":"0.001","maxOrderSize":"1000","displayMinLeverage":"1","displayMaxLeverage":"100","l2PairId":"50000","initialMarginRate":"0.01","defaultInitialMarginRate":"0.05","maxMarketPriceRange":"0.045"}]}}}"#;
/// Actions released through the engine's managed-HTTP fixture handshake at
/// `now`, the time of the event that produced them. Legacy actions pass through.
pub trait Prepared {
    fn prepared(&mut self, now: f64) -> std::vec::IntoIter<Action>;
}
impl<V: safe_cex_engine::Venue> Prepared for Engine<V> {
    fn prepared(&mut self, now: f64) -> std::vec::IntoIter<Action> {
        safe_cex_engine::test_support::prepared_actions(self, now).into_iter()
    }
}
pub fn requests(e: &mut TestEngine, now: f64) -> Vec<HttpRequest> {
    e.prepared(now)
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn prepared_requests(e: &mut TestEngine, now: f64) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
pub fn response(e: &mut TestEngine, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
pub fn started() -> (TestEngine, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e, 0.0).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let socket = e
        .prepared(10.0)
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(20.0, Event::WsOpened { socket });
    (e, socket)
}
pub fn ws(e: &mut TestEngine, socket: SocketId, time: f64, text: &str) {
    e.handle(time, Event::WsMessage { socket, text });
}

/// Fixture account material, installed only in the provider; the venue gets
/// opaque refs (RUST_APEXOMNI_SIGNER_ADMISSION.md).
pub struct Material<'a> {
    pub key: &'a str,
    pub signing: &'a str,
    pub passphrase: Option<&'a str>,
    pub seed: Option<String>,
}
impl Default for Material<'_> {
    fn default() -> Self {
        Self {
            key: "key",
            signing: "secret",
            passphrase: Some("passphrase"),
            seed: Some("11".repeat(65)),
        }
    }
}
pub fn fixture_provider(
    material: &Material<'_>,
) -> (
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
) {
    use safe_cex_crypto::signer::{LocalExtra, LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{
        CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy, IdentitySetupPolicy,
        KeyEncoding,
    };
    let mut identity = LocalIdentity {
        key: Secret::new(material.key.into()),
        secret: Secret::new(material.signing.into()),
        passphrase: material.passphrase.map(|p| Secret::new(p.into())),
        ..Default::default()
    };
    if let Some(seed) = &material.seed {
        identity.extra.0.insert(
            ExtraKey::ZklinkSeed,
            LocalExtra {
                value: Secret::new(seed.clone()),
                encoding: KeyEncoding::Hex,
            },
        );
    }
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Base64Text),
            extra: Some(ExtraKeyPolicy {
                key: ExtraKey::ZklinkSeed,
                field: ExtraFieldPolicy::Optional,
                encoding: Some(KeyEncoding::Hex),
            }),
            ..Default::default()
        }),
        secondary: None,
    };
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(identity),
            secondary: None,
        },
        policy,
    )
    .unwrap();
    (std::sync::Arc::new(signer), identities)
}
/// A seeded engine for a private account: refs in `Options`, material in the
/// provider. `options.identity` is replaced by the fixture's.
pub fn private_engine(options: Options, material: &Material<'_>) -> TestEngine {
    let (signer, identities) = fixture_provider(material);
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..options
    }));
    e.set_identities(identities).unwrap();
    e.set_signer(Some(signer)).unwrap();
    e
}
