//! TransactionClient composition over explicit crypto callbacks. No WASM loader,
//! nonce allocation or HTTP scheduling is implied by this native boundary.
use super::{
    http_transactions::*, transaction_builders::*, transaction_client_helpers::*,
    transaction_client_types::*,
};
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_bigint_primitive, js_number_string};

/// Retains an arbitrary host exception without inspecting, normalizing or copying it.
#[derive(Clone)]
pub struct HostThrownValue(std::rc::Rc<dyn std::any::Any>);
impl HostThrownValue {
    pub fn new<T: std::any::Any>(value: T) -> Self {
        Self(std::rc::Rc::new(value))
    }
    pub fn value(&self) -> &dyn std::any::Any {
        self.0.as_ref()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
    /// The shared handle, so a host can hand out the same value again.
    pub fn shared(&self) -> std::rc::Rc<dyn std::any::Any> {
        self.0.clone()
    }
    pub fn from_shared(value: std::rc::Rc<dyn std::any::Any>) -> Self {
        Self(value)
    }
}
impl std::fmt::Debug for HostThrownValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostThrownValue").finish_non_exhaustive()
    }
}
impl PartialEq for HostThrownValue {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
    }
}
impl Eq for HostThrownValue {}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionSigningError {
    Generated(TransactionBuildError),
    Thrown(HostThrownValue),
}
impl From<TransactionBuildError> for TransactionSigningError {
    fn from(e: TransactionBuildError) -> Self {
        Self::Generated(e)
    }
}
impl From<HostThrownValue> for TransactionSigningError {
    fn from(e: HostThrownValue) -> Self {
        Self::Thrown(e)
    }
}
pub type SigningResult<T> = Result<T, TransactionSigningError>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoFailure {
    ReturnedError(Vec<u16>),
    Thrown(HostThrownValue),
}
pub type CryptoCallResult = Result<Vec<u16>, CryptoFailure>;
pub type Fp5Callback<'a> = dyn Fn(&[u16]) -> CryptoCallResult + 'a;
/// fp5_callback returns the extracted callable: composition invokes it without
/// implicitly binding a receiver, matching the source local-function call.
pub trait TransactionCrypto {
    fn is_ready(&self) -> Result<bool, HostThrownValue>;
    fn poseidon2_hash(&self, input: &[u16]) -> CryptoCallResult;
    fn poseidon2_two_to_one(&self, left: &[u16], right: &[u16]) -> CryptoCallResult;
    fn fp5_callback(&self) -> Result<Option<&Fp5Callback<'_>>, HostThrownValue>;
    fn schnorr_sign(&self, input: &[u16], private_key: &[u16]) -> CryptoCallResult;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedTransactionData {
    pub signature: Vec<u16>,
    pub tx_hash: Vec<u16>,
}
fn error(message: &str) -> TransactionBuildError {
    TransactionBuildError {
        name: "Error",
        message: message.encode_utf16().collect(),
    }
}
fn ready(crypto: &dyn TransactionCrypto) -> SigningResult<()> {
    if !crypto.is_ready()? {
        return Err(error("Crypto module not ready. Please wait for initialization.").into());
    }
    Ok(())
}
fn crypto_value(result: CryptoCallResult, prefix: &str) -> SigningResult<Vec<u16>> {
    result.map_err(|failure| match failure {
        CryptoFailure::ReturnedError(units) => {
            TransactionSigningError::Generated(TransactionBuildError {
                name: "Error",
                message: prefix.encode_utf16().chain(units).collect(),
            })
        }
        CryptoFailure::Thrown(error) => TransactionSigningError::Thrown(error),
    })
}
// ts: TransactionClient#signTransaction
pub fn sign_transaction(
    crypto: &dyn TransactionCrypto,
    private_key: &[u16],
    data: &[u16],
) -> SigningResult<SignedTransactionData> {
    ready(crypto)?;
    let tx_hash = crypto_value(crypto.poseidon2_hash(data), "Failed to hash transaction: ")?;
    // schnorrSign hashes internally: pass original data, never the hash output.
    let signature = crypto_value(
        crypto.schnorr_sign(data, private_key),
        "Failed to sign transaction: ",
    )?;
    Ok(SignedTransactionData { signature, tx_hash })
}
// ts: TransactionClient#signCreateOrderTransaction
pub fn sign_create_order_transaction(
    crypto: &dyn TransactionCrypto,
    private_key: &[u16],
    data: &[u16],
    attributes: Option<&[u16]>,
) -> SigningResult<SignedTransactionData> {
    let Some(attributes) = attributes.filter(|v| !v.is_empty()) else {
        return sign_transaction(crypto, private_key, data);
    };
    ready(crypto)?;
    let fp5 = crypto
        .fp5_callback()?
        .ok_or_else(|| error("Crypto module does not support poseidon2HashFp5"))?;
    let base = crypto_value(fp5(data), "Failed to fp5-hash create order transaction: ")?;
    let attributes = crypto_value(
        fp5(attributes),
        "Failed to fp5-hash create order attributes: ",
    )?;
    let preimage = to_hex_no_prefix(&base)
        .into_iter()
        .chain(to_hex_no_prefix(&attributes))
        .collect::<Vec<_>>();
    let tx_hash = crypto_value(
        fp5(&preimage),
        "Failed to fp5-hash aggregated create order payload: ",
    )?;
    let signature = crypto_value(
        crypto.schnorr_sign(&preimage, private_key),
        "Failed to sign aggregated create order payload: ",
    )?;
    Ok(SignedTransactionData { signature, tx_hash })
}
#[derive(Debug, Clone, PartialEq)]
pub struct GroupedOrdersData {
    pub message_hex: Vec<u16>,
    pub l2_attributes_message_hex: Option<Vec<u16>>,
    pub expired_at: BigInt,
    pub orders: Vec<LighterGroupedOrderDetails>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}
fn clock_bigint(value: f64) -> Result<BigInt, TransactionBuildError> {
    js_bigint_primitive(&JsPrimitive::Number(value)).map_err(|e| TransactionBuildError {
        name: e.kind.name(),
        message: e.message_utf16,
    })
}
fn u32_word(value: f64, name: &str) -> Result<String, TransactionBuildError> {
    to_uint32_field_hex(value, &name.encode_utf16().collect::<Vec<_>>()).map_err(Into::into)
}
fn i64_word(value: &BigInt, name: &str) -> Result<String, TransactionBuildError> {
    to_int64_field_hex(value, &name.encode_utf16().collect::<Vec<_>>()).map_err(Into::into)
}
// ts: TransactionClient#buildCreateGroupedOrdersData
pub fn build_create_grouped_orders_data(
    context: &TransactionBuildContext<'_>,
    crypto: &dyn TransactionCrypto,
    params: &CreateGroupedOrdersParams,
    nonce: &BigInt,
) -> SigningResult<GroupedOrdersData> {
    build_grouped_orders_with_side_comparison(context, crypto, params, nonce, &|a, b| {
        params.orders[a].is_buy == params.orders[b].is_buy
    })
}

pub(crate) fn build_grouped_orders_with_side_comparison(
    context: &TransactionBuildContext<'_>,
    crypto: &dyn TransactionCrypto,
    params: &CreateGroupedOrdersParams,
    nonce: &BigInt,
    same_side: &dyn Fn(usize, usize) -> bool,
) -> SigningResult<GroupedOrdersData> {
    validate_grouped_orders_with_side_comparison(params, same_side)?;
    ready(crypto)?;
    let expired_at = clock_bigint((context.now_millis)() + 600_000.0)?;
    let default_order_expiry =
        clock_bigint((context.now_millis)() + 28.0 * 24.0 * 60.0 * 60.0 * 1000.0)?;
    let options = CreateOrderDataBuildOptions {
        expired_at: Some(expired_at.clone()),
        default_order_expiry: Some(default_order_expiry),
    };
    let payloads = params
        .orders
        .iter()
        .map(|order| build_create_order_data(context, order, nonce, &options))
        .collect::<Result<Vec<_>, _>>()?;
    // Source constructs every payload before invoking the first order hash.
    let hashes = params
        .orders
        .iter()
        .zip(&payloads)
        .enumerate()
        .map(|(index, (order, payload))| {
            let data = build_grouped_order_info_data(order, payload)?;
            crypto_value(
                crypto.poseidon2_hash(&data.encode_utf16().collect::<Vec<_>>()),
                &format!("Failed to hash grouped order {index}: "),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Validation guarantees 2 or 3 orders in the declared grouping domain.
    let mut hashes = hashes.into_iter();
    let mut aggregated = hashes
        .next()
        .ok_or_else(|| error("Lighter grouped orders require 1-3 orders"))?;
    for (index, next) in hashes.enumerate() {
        aggregated = crypto_value(
            crypto.poseidon2_two_to_one(&aggregated, &next),
            &format!("Failed to aggregate grouped order hash {}: ", index + 1),
        )?;
    }
    let prefix = [
        u32_word(context.chain_id, "chainId")?,
        u32_word(28.0, "txType")?,
        i64_word(nonce, "nonce")?,
        i64_word(&expired_at, "expiredAt")?,
        i64_word(context.account_index, "accountIndex")?,
        u32_word(context.api_key_index, "apiKeyIndex")?,
        u32_word(params.grouping_type.value(), "groupingType")?,
    ]
    .concat();
    let message_hex = prefix.encode_utf16().chain(aggregated).collect();
    let attribution = match (
        &params.integrator_account_index,
        params.integrator_maker_fee,
        params.integrator_taker_fee,
    ) {
        (Some(index), Some(maker), Some(taker)) => Some((index, maker, taker)),
        _ => None,
    };
    let l2_attributes_message_hex = attribution
        .map(|(index, maker, taker)| {
            build_create_order_l2_attributes_data(index, taker, maker)
                .map(|v| v.encode_utf16().collect())
        })
        .transpose()?;
    let orders = params
        .orders
        .iter()
        .zip(payloads)
        .map(|(order, payload)| LighterGroupedOrderDetails {
            order_id: order.order_id.clone(),
            market_id: order.market_id,
            is_buy: order.is_buy,
            price: order.price.clone(),
            amount: order.amount.clone(),
            reduce_only: order.reduce_only,
            order_type_value: Some(payload.order_type_value),
            time_in_force_value: Some(payload.time_in_force_value),
            trigger_price: Some(payload.trigger_price),
            order_expiry: Some(payload.order_expiry),
        })
        .collect();
    Ok(GroupedOrdersData {
        message_hex,
        l2_attributes_message_hex,
        expired_at,
        orders,
        integrator_account_index: attribution.map(|(v, _, _)| v.clone()),
        integrator_maker_fee: attribution.map(|(_, v, _)| v),
        integrator_taker_fee: attribution.map(|(_, _, v)| v),
    })
}
// ts: TransactionClient#buildCreateOrderTransaction
pub fn build_create_order_transaction(
    context: &TransactionBuildContext<'_>,
    crypto: &dyn TransactionCrypto,
    private_key: &[u16],
    params: &CreateOrderParams,
    nonce: &BigInt,
) -> SigningResult<L2CreateOrderTxInfo> {
    let payload = build_create_order_data(
        context,
        params,
        nonce,
        &CreateOrderDataBuildOptions::default(),
    )?;
    let attributes = payload
        .l2_attributes_message_hex
        .as_ref()
        .map(|v| v.encode_utf16().collect::<Vec<_>>());
    let data = payload.message_hex.encode_utf16().collect::<Vec<_>>();
    let signed = sign_create_order_transaction(crypto, private_key, &data, attributes.as_deref())?;
    Ok(L2CreateOrderTxInfo {
        header: TransactionHeader {
            data,
            signature: signed.signature,
            account_index: context.account_index.clone(),
            api_key_index: context.api_key_index,
            nonce: nonce.clone(),
            expired_at: Some(payload.expired_at),
        },
        order_details: CreateOrderDetails {
            order_id: params.order_id.clone(),
            market_id: params.market_id,
            is_buy: params.is_buy,
            price: params.price.clone(),
            amount: params.amount.clone(),
            post_only: params.post_only,
            reduce_only: params.reduce_only,
            order_type_value: Some(payload.order_type_value),
            time_in_force_value: Some(payload.time_in_force_value),
            trigger_price: Some(payload.trigger_price),
            order_expiry: Some(payload.order_expiry),
            integrator_account_index: payload.integrator_account_index,
            integrator_maker_fee: payload.integrator_maker_fee,
            integrator_taker_fee: payload.integrator_taker_fee,
        },
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTokenEncoding {
    TextEncoder,
    NodeBuffer,
    Unavailable,
}
// ts: TransactionClient.getAuthToken
/// Native synchronous composition of the source no-await async body. Ambient
/// encoder availability is explicit, including the source failure branch.
pub fn get_auth_token(
    context: &TransactionBuildContext<'_>,
    crypto: &dyn TransactionCrypto,
    private_key: &[u16],
    deadline: &BigInt,
    encoding: AuthTokenEncoding,
) -> SigningResult<Vec<u16>> {
    get_auth_token_with_encoding(context, crypto, private_key, deadline, || encoding)
}
/// Resolve ambient encoder availability after the source's first readiness call.
pub fn get_auth_token_with_encoding(
    context: &TransactionBuildContext<'_>,
    crypto: &dyn TransactionCrypto,
    private_key: &[u16],
    deadline: &BigInt,
    encoding: impl FnOnce() -> AuthTokenEncoding,
) -> SigningResult<Vec<u16>> {
    ready(crypto)?;
    let message = format!(
        "{deadline}:{}:{}",
        context.account_index,
        js_number_string(context.api_key_index)
    );
    if encoding() == AuthTokenEncoding::Unavailable {
        return Err(error("No UTF-8 encoder available in current runtime").into());
    }
    let mut padded = utf8_to_hex(&message.encode_utf16().collect::<Vec<_>>());
    padded.extend(std::iter::repeat_n('0', (16 - padded.len() % 16) % 16));
    let signed = sign_transaction(
        crypto,
        private_key,
        &padded.encode_utf16().collect::<Vec<_>>(),
    )?;
    Ok(message
        .encode_utf16()
        .chain([u16::from(b':')])
        .chain(signed.signature)
        .collect())
}
