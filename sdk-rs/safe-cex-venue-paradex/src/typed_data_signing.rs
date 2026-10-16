//! Paradex typed-data signer composition; SDK and language operations retain their hosts.
use std::{future::Future, pin::Pin};

// ts: signParadexTypedData
pub type SigningFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;

// ts: signParadexTypedData
pub trait TypedDataSigningHost {
    type Value: Clone;
    type Error;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn literal(&self, text: &str) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Result<Self::Value, Self::Error>;
    fn array(&self, values: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn import_starknet(&self) -> Result<Self::Value, Self::Error>;
    fn await_value(&self, value: Self::Value) -> SigningFuture<'_, Self::Value, Self::Error>;
    fn call(
        &self,
        method: Self::Value,
        receiver: &Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn is_array(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn is_bigint(&self, value: &Self::Value) -> bool;
    fn select_string(&self) -> Result<Self::Value, Self::Error>;
    fn string(&self, method: Self::Value, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn stringify(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn map(
        &self,
        method: Self::Value,
        receiver: &Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
}

// ts: signParadexTypedData
pub async fn sign_paradex_typed_data<H: TypedDataSigningHost>(
    h: &H,
    sdk_account: &H::Value,
    typed_data: &H::Value,
    options: &H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.get(options, "payloadSigner")?) {
        let module = h.await_value(h.import_starknet()?).await?;
        let starknet_typed_data = h.get(&module, "typedData")?;
        let signer = h.get(options, "payloadSigner")?;
        let key = h.get(options, "key")?;
        let hash_method = h.get(&starknet_typed_data, "getMessageHash")?;
        let address = h.get(&h.get(sdk_account, "account")?, "address")?;
        let payload = h.call(
            hash_method,
            &starknet_typed_data,
            vec![typed_data.clone(), address],
        )?;
        let request = h.object(vec![
            ("exchangeName", h.literal("paradex")),
            ("key", key),
            ("scheme", h.literal("stark")),
            ("encoding", h.literal("felt-pair")),
            ("payload", payload),
        ])?;
        return h.await_value(h.call(signer, options, vec![request])?).await;
    }
    let account = h.get(sdk_account, "account")?;
    let sign_message = h.get(&account, "signMessage")?;
    let sig = h
        .await_value(h.call(sign_message, &account, vec![typed_data.clone()])?)
        .await?;
    let sig_array = if h.is_array(&sig)? {
        let map = h.get(&sig, "map")?;
        h.map(map, &sig, &mut |value| {
            if h.is_bigint(&value) {
                let method = h.get(&value, "toString")?;
                h.call(method, &value, vec![])
            } else {
                h.string(h.select_string()?, value)
            }
        })?
    } else {
        let string = h.select_string()?;
        let r = h.string(string, h.get(&sig, "r")?)?;
        let string = h.select_string()?;
        let s = h.string(string, h.get(&sig, "s")?)?;
        h.array(vec![r, s])?
    };
    let signature = h.stringify(sig_array)?;
    h.object(vec![("signature", signature)])
}

// ts: signParadexTypedData
pub fn native_message_hash(raw: &str, account_address: &str) -> Result<String, String> {
    let address = safe_cex_crypto::starknet::parse_felt(account_address)?;
    let hash = safe_cex_crypto::starknet::hash_typed_data(raw, &address)?;
    Ok(format!("{hash:#x}"))
}

// ts: signParadexTypedData
pub fn native_sign_message(
    raw: &str,
    account_address: &str,
    private_key: &str,
) -> Result<Vec<String>, String> {
    let address = safe_cex_crypto::starknet::parse_felt(account_address)?;
    let key = safe_cex_crypto::starknet::parse_felt(private_key)?;
    let hash = safe_cex_crypto::starknet::hash_typed_data(raw, &address)?;
    let (r, s) = safe_cex_crypto::starknet::sign(&hash, &key)?;
    Ok(vec![r.to_string(), s.to_string()])
}
