//! Nondefault offline qualification. No arbitrary key, endpoint or venue input.
use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_venue_bingx::{Options, spot::BingxSpot};

#[wasm_bindgen(js_name = bingxSpotReplayFixture)]
pub fn fixture(public_data_only: bool) -> Result<WasmEngine, js_sys::Error> {
    let (signer, identities) = LocalSigner::from_setup(LocalSetup {
        primary: (!public_data_only).then(|| LocalIdentity {
            key: Secret::new("replay-key".into()),
            secret: Secret::new("replay-secret".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .map_err(|_| js_sys::Error::new("Invalid Spot fixture signer"))?;
    let mut engine = Engine::new(
        BingxSpot::new(Options {
            credentials: identities.primary,
            read_credentials: None,
            public_data_only,
        }),
        Some(production_entropy().map_err(construction_error)?),
    )
    .map_err(construction_error)?;
    engine
        .set_signer(Some(std::sync::Arc::new(signer)))
        .map_err(construction_error)?;
    Ok(WasmEngine {
        inner: Box::new(engine),
        dispatch_signing: true,
        external_signer: false,
        construction: "{}".into(),
        #[cfg(feature = "coinbase")]
        coinbase_binding: None,
        #[cfg(feature = "hosted-signing")]
        hosted_identity: None,
        #[cfg(feature = "hosted-signing")]
        hosted_captures: Default::default(),
        #[cfg(feature = "hosted-wallet")]
        hosted_wallet: None,
        #[cfg(feature = "algos")]
        algos: Default::default(),
    })
}
