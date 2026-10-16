//! Source signing helpers; legacy string chain IDs intentionally differ from placement.
use crate::{
    builder_workflow::{JsPromise, JsResult},
    session_host::SessionHost,
};
use std::rc::Rc;
pub trait SigningHost: SessionHost {
    fn undefined_argument(&self, value: &Self::Value) -> bool;
    fn new_wallet(&self, key: Self::Value) -> JsResult<Self::Value>;
    fn legacy_domain(&self, testnet: bool) -> Self::Value;
    fn legacy_order_types(&self) -> Self::Value;
    fn signature_from(&self, signature: Self::Value) -> JsResult<Self::Value>;
}
/// ts: validatePrivateKey. Wallet construction validates scalar range as well as spelling.
pub fn validate_private_key<H: SigningHost>(h: &H, key: H::Value) -> bool {
    h.new_wallet(key).is_ok()
}
/// ts: signOrder. Default isTestnet=true applies to undefined, preserving false/null.
pub fn sign_order<H: SigningHost>(
    host: Rc<H>,
    wallet: H::Value,
    payload: H::Value,
    testnet: Option<H::Value>,
) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        let testnet = match testnet {
            Some(value) if !h.undefined_argument(&value) => value,
            _ => h.boolean(true),
        };
        let domain = h.legacy_domain(h.truthy(&testnet));
        let sign = h.get(&wallet, "signTypedData")?;
        let signature = h
            .await_value(h.call(
                sign,
                wallet.clone(),
                vec![domain, h.legacy_order_types(), payload.clone()],
            )?)
            .await?;
        let signature = h.signature_from(signature)?;
        Ok(h.object(vec![
            ("r", h.get(&signature, "r")?),
            ("s", h.get(&signature, "s")?),
            ("v", h.get(&signature, "v")?),
            ("signer", h.get(&wallet, "address")?),
            ("nonce", h.get(&payload, "nonce")?),
            ("expiration", h.get(&payload, "expiration")?),
        ]))
    }))
}
