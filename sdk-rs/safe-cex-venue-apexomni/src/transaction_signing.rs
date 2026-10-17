//! Contract signing over the imported zkLink capability.
use crate::{
    signer_utils as check,
    transaction_records::SignContractPayload,
    zklink_host::{TransactionKind as K, ZkLinkHost},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue as J;

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
