//! Plain transaction-value encoders; strings retain arbitrary UTF-16 code units.
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_bigint_primitive, js_number_string};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingError {
    pub name: &'static str,
    pub message: Vec<u16>,
}
impl EncodingError {
    pub(crate) fn error(message: impl AsRef<str>) -> Self {
        Self {
            name: "Error",
            message: message.as_ref().encode_utf16().collect(),
        }
    }
}
pub(crate) fn bigint_string(input: &[u16]) -> Result<BigInt, EncodingError> {
    if let Ok(text) = String::from_utf16(input) {
        return js_bigint_primitive(&JsPrimitive::String(text)).map_err(|e| EncodingError {
            name: e.kind.name(),
            message: e.message_utf16,
        });
    }
    let mut message: Vec<u16> = "Cannot convert ".encode_utf16().collect();
    message.extend(input.iter().take(1000));
    if input.len() > 1000 {
        message.push(0x2026);
    }
    message.extend(" to a BigInt".encode_utf16());
    Err(EncodingError {
        name: "SyntaxError",
        message,
    })
}
fn hex(bytes: &[u8]) -> Vec<u16> {
    bytes
        .iter()
        .flat_map(|b| format!("{b:02x}").encode_utf16().collect::<Vec<_>>())
        .collect()
}
fn u32_field(n: f64) -> Result<Vec<u16>, EncodingError> {
    if n < 0.0 || n > u32::MAX as f64 {
        return Err(EncodingError::error(format!(
            "Value {} out of range for uint32 (0 to {})",
            js_number_string(n),
            u32::MAX
        )));
    }
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&(n as u32).to_le_bytes());
    Ok(hex(&bytes))
}
fn i64_field(input: &[u16]) -> Result<Vec<u16>, EncodingError> {
    let n = bigint_string(input)?;
    let value = i64::try_from(&n).map_err(|_| {
        EncodingError::error(format!(
            "Value {n} out of range for int64 ({} to {})",
            i64::MIN,
            i64::MAX
        ))
    })?;
    Ok(hex(&value.to_le_bytes()))
}
#[derive(Clone)]
pub struct HashOptions {
    pub account_index: f64,
    pub api_key_index: f64,
    pub nonce: f64,
    pub expiry: f64,
}
#[derive(Clone)]
pub struct HashHeader {
    pub tx_type: f64,
    pub opts: HashOptions,
}
#[derive(Clone)]
pub struct CreateOrderInput {
    pub header: HashHeader,
    pub market_id: Vec<u16>,
    pub order_type: f64,
    pub side: f64,
    pub price: Vec<u16>,
    pub quantity: Vec<u16>,
    pub time_in_force: f64,
    pub reduce_only: bool,
    pub post_only: bool,
    pub stop_price: Option<Vec<u16>>,
}
#[derive(Clone)]
pub struct CancelOrderInput {
    pub header: HashHeader,
    pub market_id: Vec<u16>,
    pub order_id: Vec<u16>,
}
#[derive(Clone)]
pub struct TransferInput {
    pub header: HashHeader,
    pub token_address: Vec<u16>,
    pub recipient: Vec<u16>,
    pub amount: Vec<u16>,
}
#[derive(Clone)]
pub struct WithdrawInput {
    pub header: HashHeader,
    pub token_address: Vec<u16>,
    pub amount: Vec<u16>,
    pub l1_recipient: Option<Vec<u16>>,
    pub fast_withdraw: Option<bool>,
}
fn prefix(header: &HashHeader, chain_id: f64) -> Result<Vec<u16>, EncodingError> {
    let mut output = Vec::new();
    for n in [
        chain_id,
        header.tx_type,
        header.opts.account_index,
        header.opts.api_key_index,
        header.opts.nonce,
        header.opts.expiry,
    ] {
        output.extend(u32_field(n)?);
    }
    Ok(output)
}
fn append_identifier(output: &mut Vec<u16>, identifier: &[u16]) {
    let clean = identifier.strip_prefix(&[48, 120]).unwrap_or(identifier);
    let padding = (16 - clean.len() % 16) % 16;
    output.extend(std::iter::repeat_n(48, padding));
    output.extend_from_slice(clean);
}
pub fn hash_create_order_tx(
    tx: &CreateOrderInput,
    chain_id: f64,
) -> Result<Vec<u16>, EncodingError> {
    let mut output = prefix(&tx.header, chain_id)?;
    append_identifier(&mut output, &tx.market_id);
    output.extend(u32_field(tx.order_type)?);
    output.extend(u32_field(tx.side)?);
    output.extend(i64_field(&tx.price)?);
    output.extend(i64_field(&tx.quantity)?);
    output.extend(u32_field(tx.time_in_force)?);
    output.extend(u32_field(if tx.reduce_only { 1.0 } else { 0.0 })?);
    output.extend(u32_field(if tx.post_only { 1.0 } else { 0.0 })?);
    if let Some(stop) = &tx.stop_price {
        output.extend(i64_field(stop)?);
    }
    Ok(output)
}
pub fn hash_cancel_order_tx(
    tx: &CancelOrderInput,
    chain_id: f64,
) -> Result<Vec<u16>, EncodingError> {
    let mut output = prefix(&tx.header, chain_id)?;
    append_identifier(&mut output, &tx.market_id);
    append_identifier(&mut output, &tx.order_id);
    Ok(output)
}
pub fn hash_transfer_tx(tx: &TransferInput, chain_id: f64) -> Result<Vec<u16>, EncodingError> {
    let mut output = prefix(&tx.header, chain_id)?;
    append_identifier(&mut output, &tx.token_address);
    append_identifier(&mut output, &tx.recipient);
    output.extend(i64_field(&tx.amount)?);
    Ok(output)
}
pub fn hash_withdraw_tx(tx: &WithdrawInput, chain_id: f64) -> Result<Vec<u16>, EncodingError> {
    let mut output = prefix(&tx.header, chain_id)?;
    append_identifier(&mut output, &tx.token_address);
    output.extend(i64_field(&tx.amount)?);
    if let Some(recipient) = &tx.l1_recipient {
        if !recipient.is_empty() {
            append_identifier(&mut output, recipient);
        }
    }
    output.extend(u32_field(if tx.fast_withdraw.unwrap_or(false) {
        1.0
    } else {
        0.0
    })?);
    Ok(output)
}
pub fn hash_auth_token(
    account_index: f64,
    api_key_index: f64,
    deadline: &BigInt,
    chain_id: f64,
) -> Result<Vec<u16>, EncodingError> {
    let mut output = u32_field(account_index)?;
    output.extend(u32_field(api_key_index)?);
    let deadline_value = u64::try_from(deadline).map_err(|_| {
        EncodingError::error(format!(
            "Value {deadline} out of range for uint64 (0 to {})",
            u64::MAX
        ))
    })?;
    output.extend(hex(&deadline_value.to_le_bytes()));
    output.extend(u32_field(chain_id)?);
    Ok(output)
}
