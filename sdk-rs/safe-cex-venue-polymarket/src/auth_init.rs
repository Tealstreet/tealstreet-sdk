//! Source signing helper and constructor bodies. Ethers, the official order
//! builder, HTTP factories/retry and base websocket are explicit dependencies.
//! Class composition and the REST interceptor factory remain separate rows.
use crate::{
    account_reads::{AccountHost, returned, start, wait},
    constants::POLYMARKET_CONFIG,
};
use std::{future::Future, rc::Rc};

/// A source object literal with two executable ethers-v5-compatible callbacks.
pub struct WalletSigner<H: AuthHost> {
    host: Rc<H>,
    wallet: H::Value,
}
impl<H: AuthHost> WalletSigner<H> {
    /// Resolve the current method on the originally captured wallet.
    pub fn get_address(&self) -> Result<H::Value, H::Error> {
        let c = self.host.capture_value(self.wallet.clone(), "getAddress")?;
        self.host.call_sync(c, vec![])
    }
    pub fn sign_typed_data(
        &self,
        domain: H::Value,
        types: H::Value,
        value: H::Value,
    ) -> Result<H::Value, H::Error> {
        let c = self
            .host
            .capture_value(self.wallet.clone(), "signTypedData")?;
        self.host.call_sync(c, vec![domain, types, value])
    }
}
/// Capabilities used by these source bodies; imported libraries are bound here.
pub trait AuthHost: AccountHost {
    fn capture_value(
        &self,
        receiver: Self::Value,
        method: &'static str,
    ) -> Result<Self::Callable, Self::Error>;
    fn call_sync(
        &self,
        call: Self::Callable,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    type Constructor;
    fn capture_constructor(&self, name: &'static str) -> Result<Self::Constructor, Self::Error>;
    fn construct(
        &self,
        constructor: Self::Constructor,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    /// The single module-level CLOB_AUTH_TYPES object, including current mutations.
    fn clob_auth_types(&self) -> Result<Self::Value, Self::Error>;
    fn signer(&self, signer: WalletSigner<Self>) -> Result<Self::Value, Self::Error>
    where
        Self: Sized;
    fn sdk_side(&self, buy: bool) -> Self::Value;
    fn string_value(&self, units: Vec<u16>) -> Self::Value;
    fn uppercase(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn replace_ascii(
        &self,
        value: Self::Value,
        from: u16,
        to: u16,
    ) -> Result<Self::Value, Self::Error>;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn date_now(&self) -> Result<f64, Self::Error>;
    fn retry_delay(
        &self,
        delay: Rc<dyn Fn(Self::Value) -> Result<Self::Value, Self::Error>>,
    ) -> Result<Self::Value, Self::Error>;
    fn multiply(&self, left: Self::Value, right: Self::Value) -> Result<Self::Value, Self::Error>;
}
pub fn create_wallet<H: AuthHost>(host: &H, private_key: H::Value) -> Result<H::Value, H::Error> {
    let ctor = host.capture_constructor("ethers.Wallet")?;
    host.construct(ctor, vec![private_key])
}
pub fn create_signed_order<H: AuthHost>(
    host: Rc<H>,
    wallet: H::Value,
    params: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let signer = host.signer(WalletSigner {
                host: host.clone(),
                wallet,
            })?;
            let ctor = host.capture_constructor("SDK.OrderBuilder")?;
            let signature_type = host.field(&params, "signatureType")?;
            let signature_type = if host.nullish(&signature_type) {
                host.number(0.0)
            } else {
                signature_type
            };
            let funder = host.field(&params, "funderAddress")?;
            let builder = host.construct(
                ctor,
                vec![
                    signer,
                    host.number(POLYMARKET_CONFIG.chain_id),
                    signature_type,
                    funder,
                ],
            )?;
            let call = host.capture_value(builder, "buildOrder")?;
            let token = host.field(&params, "tokenId")?;
            let price = host.field(&params, "price")?;
            let size = host.field(&params, "size")?;
            let side = host.sdk_side(host.is_text(&host.field(&params, "side")?, "BUY"));
            let order = host.object(vec![
                ("tokenID", token),
                ("price", price),
                ("size", size),
                ("side", side),
            ])?;
            let options = host.object(vec![
                ("tickSize", host.field(&params, "tickSize")?),
                ("negRisk", host.field(&params, "negRisk")?),
            ])?;
            let signed = wait(host.invoke(call, vec![order, options, host.number(2.0)])).await?;
            let check = host.capture("SDK", "isV2Order")?;
            let valid = host.call_sync(check, vec![signed.clone()])?;
            if !host.truthy(&valid) {
                return Err(host.new_error("Polymarket official client returned a non-V2 order"));
            }
            returned(host.as_ref(), signed).await
        }
    })
}
pub fn generate_hmac_signature<H: AuthHost>(
    host: &H,
    secret: H::Value,
    timestamp: H::Value,
    method: H::Value,
    path: H::Value,
    body: H::Value,
) -> Result<H::Value, H::Error> {
    let mut message = host.string(&timestamp)?;
    message.extend(host.string(&host.uppercase(method)?)?);
    message.extend(host.string(&path)?);
    if !host.is_undefined(&body) {
        message.extend(host.string(&body)?);
    }
    let normalized = host.replace_ascii(secret, b'-' as u16, b'+' as u16)?;
    let normalized = host.replace_ascii(normalized, b'_' as u16, b'/' as u16)?;
    let mut padded = host.string(&normalized)?;
    padded.extend(std::iter::repeat_n(b'=' as u16, (4 - padded.len() % 4) % 4));
    let decode = host.capture("ethers", "decodeBase64")?;
    let key = host.call_sync(decode, vec![host.string_value(padded)])?;
    let compute = host.capture("ethers", "computeHmac")?;
    // Outer computeHmac is captured before evaluating its toUtf8Bytes argument.
    let utf8 = host.capture("ethers", "toUtf8Bytes")?;
    let bytes = host.call_sync(utf8, vec![host.string_value(message)])?;
    let hmac = host.call_sync(compute, vec![host.text("sha256"), key, bytes])?;
    let encode = host.capture("ethers", "encodeBase64")?;
    let get = host.capture("ethers", "getBytes")?;
    let bytes = host.call_sync(get, vec![hmac])?;
    let encoded = host.call_sync(encode, vec![bytes])?;
    let encoded = host.replace_ascii(encoded, b'+' as u16, b'-' as u16)?;
    host.replace_ascii(encoded, b'/' as u16, b'_' as u16)
}
pub fn build_l2_headers<H: AuthHost>(
    host: &H,
    credentials: H::Value,
    address: H::Value,
    method: H::Value,
    path: H::Value,
    body: H::Value,
    timestamp: H::Value,
) -> Result<H::Value, H::Error> {
    let timestamp = if host.is_undefined(&timestamp) {
        host.number((host.date_now()? / 1000.0).floor())
    } else {
        timestamp
    };
    let to_string = host.capture_value(timestamp, "toString")?;
    let timestamp = host.call_sync(to_string, vec![])?;
    let signature = generate_hmac_signature(
        host,
        host.field(&credentials, "secret")?,
        timestamp.clone(),
        method,
        path,
        body,
    )?;
    host.object(vec![
        ("POLY_ADDRESS", address),
        ("POLY_SIGNATURE", signature),
        ("POLY_TIMESTAMP", timestamp),
        ("POLY_API_KEY", host.field(&credentials, "apiKey")?),
        ("POLY_PASSPHRASE", host.field(&credentials, "passphrase")?),
    ])
}
pub fn sign_api_key_derivation_message<H: AuthHost>(
    host: Rc<H>,
    wallet: H::Value,
    nonce: H::Value,
    timestamp: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let host = host.clone();
        async move {
            let get = host.capture_value(wallet.clone(), "getAddress")?;
            let address = wait(host.invoke(get, vec![])).await?;
            let sign = host.capture_value(wallet, "signTypedData")?;
            let domain = host.object(vec![
                ("name", host.text("ClobAuthDomain")),
                ("version", host.text("1")),
                ("chainId", host.number(POLYMARKET_CONFIG.chain_id)),
            ])?;
            let types = host.clob_auth_types()?;
            let ts_call = host.capture_value(timestamp.clone(), "toString")?;
            let timestamp_string = host.call_sync(ts_call, vec![])?;
            let value = host.object(vec![
                ("address", address.clone()),
                ("timestamp", timestamp_string),
                ("nonce", nonce),
                (
                    "message",
                    host.text("This message attests that I control the given wallet"),
                ),
            ])?;
            let signature = wait(host.invoke(sign, vec![domain, types, value])).await?;
            let result = host.object(vec![
                ("address", address),
                ("signature", signature),
                ("timestamp", timestamp),
            ])?;
            returned(host.as_ref(), result).await
        }
    })
}
fn retry<H: AuthHost>(host: Rc<H>, receiver: &H::Value, key: &'static str) -> Result<(), H::Error> {
    let retry = host.capture("imports", "retry")?;
    let client = host.field(receiver, key)?;
    let condition = host.capture("imports", "networkErrorOnRead")?;
    let condition = host.call_sync(condition, vec![])?;
    let h = host.clone();
    let delay = host.retry_delay(Rc::new(move |count| h.multiply(count, h.number(1000.0))))?;
    let config = host.object(vec![
        ("retries", host.number(3.0)),
        ("retryCondition", condition),
        ("retryDelay", delay),
    ])?;
    host.call_sync(retry, vec![client, config])?;
    Ok(())
}
/// Source constructor body over its already-allocated receiver. Ordinary Set,
/// current slot reads and partial writes remain observable to bound factories.
/// Class declarations/allocation and createPolymarketAPI itself stay separate.
pub fn initialize_api<H: AuthHost>(
    host: Rc<H>,
    receiver: H::Value,
    options: H::Value,
    exchange: H::Value,
    get_credentials: H::Value,
    get_wallet_address: H::Value,
) -> Result<H::Value, H::Error> {
    host.assign(&receiver, "getCredentials", get_credentials.clone())?;
    host.assign(&receiver, "exchange", exchange.clone())?;
    let call = host.capture("imports", "createPolymarketAPI")?;
    let xhr = host.call_sync(
        call,
        vec![
            options.clone(),
            exchange.clone(),
            get_credentials,
            get_wallet_address,
        ],
    )?;
    host.assign(&receiver, "xhr", xhr)?;
    let create = host.capture("imports", "safeCexAxiosCreate")?;
    let headers = host.object(vec![
        ("Content-Type", host.text("application/json")),
        ("Accept", host.text("application/json")),
    ])?;
    let config = host.object(vec![
        ("baseURL", host.text(POLYMARKET_CONFIG.gamma_api)),
        ("timeout", host.number(30000.0)),
        ("headers", headers),
    ])?;
    let gamma = host.call_sync(create, vec![config, options.clone(), exchange.clone()])?;
    host.assign(&receiver, "gammaXhr", gamma)?;
    retry(host.clone(), &receiver, "gammaXhr")?;
    let create = host.capture("imports", "safeCexAxiosCreate")?;
    let headers = host.object(vec![("Accept", host.text("application/json"))])?;
    let config = host.object(vec![
        ("baseURL", host.text(POLYMARKET_CONFIG.data_api)),
        ("timeout", host.number(30000.0)),
        ("headers", headers),
    ])?;
    let data = host.call_sync(create, vec![config, options, exchange])?;
    host.assign(&receiver, "dataXhr", data)?;
    retry(host, &receiver, "dataXhr")?;
    Ok(receiver)
}
pub fn initialize_private<H: AuthHost>(
    host: &H,
    parent: H::Value,
    get_credentials: H::Value,
) -> Result<H::Value, H::Error> {
    let ctor = host.capture_constructor("BaseWebSocket")?;
    let receiver = host.construct(ctor, vec![parent])?;
    host.assign(&receiver, "getCredentials", get_credentials)?;
    Ok(receiver)
}
pub fn initialize_public<H: AuthHost>(host: &H, parent: H::Value) -> Result<H::Value, H::Error> {
    let ctor = host.capture_constructor("BaseWebSocket")?;
    host.construct(ctor, vec![parent])
}
