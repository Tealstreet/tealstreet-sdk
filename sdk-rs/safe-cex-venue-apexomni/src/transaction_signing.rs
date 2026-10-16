//! Transaction helpers and explicit zkLink wallet capabilities.
use crate::{
    signer_utils as check,
    transaction_records::*,
    zklink_host::{TransactionKind as K, ZkLinkHost},
};
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue as J;
use safe_cex_num::js_number_string;

fn invalid_integer(value: &str) -> String {
    format!(
        "invalid BigNumber string (argument=\"value\", value={}, code=INVALID_ARGUMENT, version=bignumber/5.8.0)",
        serde_json::to_string(value).expect("string JSON")
    )
}

fn integer(value: &str) -> Result<BigInt, String> {
    let (negative, body) = value
        .strip_prefix('-')
        .map_or((false, value), |v| (true, v));
    let (body, radix) = if body.starts_with("0x") || body.starts_with("0X") {
        (&body[2..], 16)
    } else {
        (body, 10)
    };
    if body.is_empty()
        || !body.bytes().all(|byte| {
            if radix == 16 {
                byte.is_ascii_hexdigit()
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err(invalid_integer(value));
    }
    let number =
        BigInt::parse_bytes(body.as_bytes(), radix).ok_or_else(|| invalid_integer(value))?;
    Ok(if negative { -number } else { number })
}

fn numeric_fault(value: f64, fault: &str) -> String {
    let value = if value.is_finite() {
        js_number_string(value)
    } else {
        "null".to_owned()
    };
    format!(
        "{fault} [ See: https://links.ethers.org/v5-errors-NUMERIC_FAULT-{fault} ] (fault=\"{fault}\", operation=\"BigNumber.from\", value={value}, code=NUMERIC_FAULT, version=bignumber/5.8.0)"
    )
}

// ts: discardBeyondPrecision
pub fn discard_beyond_precision(amount: &str, token_decimals: f64) -> Result<String, String> {
    let default_decimals = 18.0;
    if token_decimals < default_decimals {
        let dust_decimals = default_decimals - token_decimals;
        if dust_decimals < 0.0 {
            return Err(format!(
                "Invalid token decimals: {}",
                js_number_string(token_decimals)
            ));
        }
        let amount = integer(amount)?;
        let scale = 10.0_f64.powf(dust_decimals);
        if scale.abs() >= 9_007_199_254_740_991.0 || !scale.is_finite() {
            return Err(numeric_fault(scale, "overflow"));
        }
        if scale.fract() != 0.0 {
            return Err(numeric_fault(scale, "underflow"));
        }
        let divisor = BigInt::from_f64(scale).ok_or_else(|| invalid_integer("NaN"))?;
        return Ok(((amount / &divisor) * divisor).to_string());
    }
    Ok(amount.to_owned())
}

fn validation(result: Result<(), String>) -> Result<(), ExchangeError> {
    result.map_err(ExchangeError::new)
}

fn validate_contract_values(
    account_id: f64,
    nonce: f64,
    fees: &[f64; 2],
    label: &str,
) -> Result<(), ExchangeError> {
    validation(check::assert_account_id(&J::Number(account_id), label))?;
    validation(check::assert_nonce(&J::Number(nonce), label))?;
    for fee in fees {
        validation(check::assert_trading_fee_ratio(&J::Number(*fee), label))?;
    }
    Ok(())
}

// ts: signContract. Host methods correspond to the source WASM/wallet calls.
pub async fn sign_contract(
    host: &dyn ZkLinkHost,
    payload: &SignContractPayload,
) -> Result<J, ExchangeError> {
    let has_subsidy = payload.has_subsidy.unwrap_or(payload.fee_rates[0] <= 0.0);
    validate_contract_values(
        payload.account_id,
        payload.nonce,
        &payload.fee_rates,
        "Contract",
    )?;
    let builder = host.builder(
        K::Contract,
        vec![
            J::Number(payload.account_id),
            J::Number(payload.sub_account_id),
            J::Number(payload.slot_id),
            J::Number(payload.nonce),
            J::Number(payload.pair_id),
            J::String(payload.size.clone()),
            J::String(payload.price.clone()),
            J::Bool(payload.direction),
            J::Number(payload.fee_rates[0].abs()),
            J::Number(payload.fee_rates[1]),
            J::Bool(has_subsidy),
        ],
    )?;
    let contract = host.new_transaction(K::Contract, builder)?;
    let signed = host.create_signed_contract(contract)?;
    Ok(crate::data::object([("tx", signed)]))
}

// ts: signOrder
pub async fn sign_order(
    host: &dyn ZkLinkHost,
    payload: &SignOrderPayload,
) -> Result<J, ExchangeError> {
    let packed_amount = host.pack_amount(&J::String(payload.amount.clone()))?;
    let has_subsidy = payload.has_subsidy.unwrap_or(payload.fee_rates[0] <= 0.0);
    validate_contract_values(
        payload.account_id,
        payload.nonce,
        &payload.fee_rates,
        "Order",
    )?;
    let builder = host.builder(
        K::Order,
        vec![
            J::Number(payload.account_id),
            J::Number(payload.sub_account_id),
            J::Number(payload.slot_id),
            J::Number(payload.nonce),
            J::Number(payload.base_token_id),
            J::Number(payload.quote_token_id),
            packed_amount,
            J::String(payload.price.clone()),
            J::Bool(payload.is_sell),
            J::Number(payload.fee_rates[0].abs()),
            J::Number(payload.fee_rates[1]),
            J::Bool(has_subsidy),
        ],
    )?;
    let signed = host.create_signed_order(builder)?;
    Ok(crate::data::object([("tx", signed)]))
}

fn transfer_validation(payload: &SignTransferPayload, ts: f64) -> Result<(), ExchangeError> {
    validation(check::assert_account_id(
        &J::Number(payload.account_id),
        "Transfer",
    ))?;
    validation(check::assert_address(
        &J::String(payload.to.clone()),
        "Transfer",
    ))?;
    validation(check::assert_sub_account_id(
        &J::Number(payload.from_sub_account_id),
        "Transfer",
    ))?;
    validation(check::assert_sub_account_id(
        &J::Number(payload.to_sub_account_id),
        "Transfer",
    ))?;
    validation(check::assert_nonce(&J::Number(payload.nonce), "Transfer"))?;
    validation(check::assert_token_id(
        &J::Number(payload.token_id),
        "Transfer",
    ))?;
    validation(check::assert_ts(&J::Number(ts), "Transfer"))
}

// ts: signTransfer
pub async fn sign_transfer(
    host: &dyn ZkLinkHost,
    payload: &SignTransferPayload,
) -> Result<J, ExchangeError> {
    let ts = payload
        .ts
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or_else(|| check::timestamp(|| host.now_ms()));
    let packed_amount = host.pack_amount(&J::String(payload.amount.clone()))?;
    let packed_fee = host.pack_fee(&J::String(payload.fee.clone()))?;
    transfer_validation(payload, ts)?;
    let builder = host.builder(
        K::Transfer,
        vec![
            J::Number(payload.account_id),
            J::String(payload.to.clone()),
            J::Number(payload.from_sub_account_id),
            J::Number(payload.to_sub_account_id),
            J::Number(payload.token_id),
            packed_fee,
            packed_amount,
            J::Number(payload.nonce),
            J::Number(ts),
        ],
    )?;
    let tx = host.new_transaction(K::Transfer, builder)?;
    host.sign_transfer(tx, J::String(payload.token_symbol.clone()))
        .await
}

// ts: signWithdraw
pub async fn sign_withdraw(
    host: &dyn ZkLinkHost,
    payload: &SignWithdrawPayload,
) -> Result<J, ExchangeError> {
    let ts = payload
        .ts
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or_else(|| check::timestamp(|| host.now_ms()));
    let amount = discard_beyond_precision(&payload.amount, payload.token_decimals)
        .map_err(ExchangeError::new)?;
    let packed_amount = host.pack_amount(&J::String(amount))?;
    let packed_fee = host.pack_fee(&J::String(payload.fee.clone()))?;
    validation(check::assert_chain_id(
        &J::Number(payload.to_chain_id),
        "Withdraw",
    ))?;
    validation(check::assert_account_id(
        &J::Number(payload.account_id),
        "Withdraw",
    ))?;
    validation(check::assert_sub_account_id(
        &J::Number(payload.sub_account_id),
        "Withdraw",
    ))?;
    validation(check::assert_address(
        &J::String(payload.to.clone()),
        "Withdraw",
    ))?;
    validation(check::assert_token_id(
        &J::Number(payload.l2_source_token_id),
        "Withdraw",
    ))?;
    validation(check::assert_token_id(
        &J::Number(payload.l1_target_token_id),
        "Withdraw",
    ))?;
    validation(check::assert_nonce(&J::Number(payload.nonce), "Withdraw"))?;
    validation(check::assert_withdraw_fee_ratio(
        &J::Number(payload.withdraw_fee_ratio),
        "Withdraw",
    ))?;
    validation(check::assert_ts(&J::Number(ts), "Withdraw"))?;
    let builder = host.builder(
        K::Withdraw,
        vec![
            J::Number(payload.account_id),
            J::Number(payload.sub_account_id),
            J::Number(payload.to_chain_id),
            J::String(payload.to.clone()),
            J::Number(payload.l2_source_token_id),
            packed_fee,
            J::Number(payload.withdraw_fee_ratio),
            J::Number(payload.l1_target_token_id),
            packed_amount,
            J::Number(payload.nonce),
            J::Bool(payload.withdraw_to_l1),
            J::Number(ts),
        ],
    )?;
    let tx = host.new_transaction(K::Withdraw, builder)?;
    host.sign_withdraw(tx, J::String(payload.l2_source_token_symbol.clone()))
        .await
}

// ts: signChangePubKey
pub async fn sign_change_pub_key(
    host: &dyn ZkLinkHost,
    payload: &SignChangePubKeyPayload,
) -> Result<J, ExchangeError> {
    let ts = payload
        .ts
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or_else(|| check::timestamp(|| host.now_ms()));
    let new_pk_hash = match &payload.new_pk_hash {
        Some(value) => J::String(value.clone()),
        None => host.pubkey_hash()?,
    };
    let fee = host.pack_fee(&J::String(payload.fee.clone()))?;
    validation(check::assert_chain_id(
        &J::Number(payload.chain_id),
        "ChangePubKey",
    ))?;
    validation(check::assert_account_id(
        &J::Number(payload.account_id),
        "ChangePubKey",
    ))?;
    validation(check::assert_sub_account_id(
        &J::Number(payload.sub_account_id),
        "ChangePubKey",
    ))?;
    validation(check::assert_pub_key_hash(&new_pk_hash, "ChangePubKey"))?;
    validation(check::assert_nonce(
        &J::Number(payload.nonce),
        "ChangePubKey",
    ))?;
    validation(check::assert_token_id(
        &J::Number(payload.fee_token_id),
        "ChangePubKey",
    ))?;
    validation(check::assert_ts(&J::Number(ts), "ChangePubKey"))?;
    let builder = host.builder(
        K::ChangePubKey,
        vec![
            J::Number(payload.chain_id),
            J::Number(payload.account_id),
            J::Number(payload.sub_account_id),
            new_pk_hash,
            J::Number(payload.fee_token_id),
            fee,
            J::Number(payload.nonce),
            J::String(format!("0x{}1b", "0".repeat(128))),
            J::Number(ts),
        ],
    )?;
    let tx = host.new_transaction(K::ChangePubKey, builder)?;
    match payload.eth_auth_type {
        ChangePubkeyTypes::EthECDSA => host.sign_change_pubkey(tx, true).await,
        ChangePubkeyTypes::Onchain => host.sign_change_pubkey(tx, false).await,
        ChangePubkeyTypes::EthCreate2 => Err(ExchangeError::new("ethAuthType not supported")),
    }
}
