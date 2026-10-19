//! ContractBuilder/JsonRpcSigner subset of the imported zkLink SDK.
//!
//! Adapted from zkLinkProtocol/zklink_sdk at
//! 3152525e2615ecbbdb56fd6728ce265ad2dde485 (Apache-2.0):
//! types/src/{tx_type/contract/contract_matching,basic_types/float_convert}.rs
//! and interface/src/json_rpc_signer.rs. Copyright 2023-present, zk.link.
//! Venue validation and wasm-bindgen argument coercion belong to the caller.
use alloy_primitives::hex;
use serde::Deserialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

/// Inputs after the SDK's ContractBuilder wasm argument conversions.
/// Decimal strings retain the source BigUint spelling until serialization.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contract {
    pub account_id: u32,
    pub sub_account_id: u8,
    pub slot_id: u32,
    pub nonce: u32,
    pub pair_id: u16,
    pub size: String,
    pub price: String,
    pub direction: bool,
    pub fee_rates: [u8; 2],
    pub has_subsidy: bool,
}

// num::BigUint's decimal parser permits one leading '+' and underscores after
// the first digit. Retain those semantics without adding a bigint dependency;
// the SDK packing path itself requires u128 and price is bounded to 120 bits.
fn decimal(value: &str) -> Result<u128, &'static str> {
    let value = value.strip_prefix('+').unwrap_or(value);
    if !value.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return Err("zkLink invalid unsigned decimal");
    }
    let mut result = 0u128;
    for byte in value.bytes() {
        if byte == b'_' {
            continue;
        }
        if !byte.is_ascii_digit() {
            return Err("zkLink invalid unsigned decimal");
        }
        result = result
            .checked_mul(10)
            .and_then(|number| number.checked_add(u128::from(byte - b'0')))
            .ok_or("zkLink decimal exceeds u128")?;
    }
    Ok(result)
}

// Return the packed integer and its decoded value. Choose between the minimal
// exponent and the preceding exponent's maximum mantissa, as FloatConversions
// does. A tie selects the preceding exponent, not the truncated next mantissa.
fn packed(number: u128, mantissa_bits: u32) -> Result<(u64, u128), &'static str> {
    let max_mantissa = (1u128 << mantissa_bits) - 1;
    let mut exponent = 0u32;
    let mut scale = 1u128;
    loop {
        let ceiling = max_mantissa
            .checked_mul(scale)
            .ok_or("zkLink amount exceeds safe SDK packing range")?;
        if number <= ceiling {
            break;
        }
        if exponent == 31 {
            return Err("zkLink amount exceeds packed exponent range");
        }
        scale = scale
            .checked_mul(10)
            .ok_or("zkLink amount exceeds safe SDK packing range")?;
        exponent += 1;
    }
    let mut mantissa = number / scale;
    if exponent != 0 {
        let previous = max_mantissa * (scale / 10);
        if number - mantissa * scale >= number - previous {
            exponent -= 1;
            scale /= 10;
            mantissa = max_mantissa;
        }
    }
    Ok((
        ((mantissa as u64) << 5) | u64::from(exponent),
        mantissa * scale,
    ))
}

/// The SDK's closestPackableTransactionAmount, with explicit overflow errors.
pub fn closest_packable_amount(amount: &str) -> Result<String, &'static str> {
    packed(decimal(amount)?, 35).map(|(_, value)| value.to_string())
}

/// The SDK's closestPackableTransactionFee, with explicit overflow errors.
pub fn closest_packable_fee(fee: &str) -> Result<String, &'static str> {
    packed(decimal(fee)?, 11).map(|(_, value)| value.to_string())
}

impl Contract {
    /// Exact 36-byte Contract::get_bytes preimage, including SDK truncations.
    /// A price wider than 120 bits or an unsafe SDK packing input returns an
    /// error instead of reaching the upstream assertion/overflow panic.
    pub fn signing_bytes(&self) -> Result<[u8; 36], &'static str> {
        let (size, _) = packed(decimal(&self.size)?, 35)?;
        let price = decimal(&self.price)?;
        if price >= (1u128 << 120) {
            return Err("zkLink contract price exceeds 120 bits");
        }
        let mut bytes = [0u8; 36];
        bytes[0] = 0xfe;
        bytes[1..5].copy_from_slice(&self.account_id.to_be_bytes());
        bytes[5] = self.sub_account_id;
        bytes[6..8].copy_from_slice(&(self.slot_id as u16).to_be_bytes());
        bytes[8..11].copy_from_slice(&self.nonce.to_be_bytes()[1..]);
        bytes[11] = self.pair_id as u8;
        bytes[12] = u8::from(self.direction);
        bytes[13..18].copy_from_slice(&size.to_be_bytes()[3..]);
        bytes[18..33].copy_from_slice(&price.to_be_bytes()[1..]);
        bytes[33..35].copy_from_slice(&self.fee_rates);
        bytes[35] = u8::from(self.has_subsidy);
        Ok(bytes)
    }
}

/// Seed-only JsonRpcSigner capability. No provider, key export, Debug or serde.
/// The owned Ethereum signature is erased on drop; caller buffers and scalar
/// temporaries inside the existing primitive remain outside that guarantee.
pub struct ContractSigner {
    seed: Zeroizing<[u8; 65]>,
}

impl ContractSigner {
    /// initZklinkSigner's Ethereum signature branch: exact 65-byte hex input,
    /// optional lowercase 0x prefix. The recovery byte is retained verbatim.
    pub fn from_signature(signature: &str) -> Result<Self, &'static str> {
        let signature = signature.strip_prefix("0x").unwrap_or(signature);
        if signature.len() != 130 {
            return Err("zkLink signature seed must contain exactly 65 bytes");
        }
        let mut seed = Zeroizing::new([0u8; 65]);
        hex::decode_to_slice(signature, seed.as_mut())
            .map_err(|_| "zkLink invalid signature seed hex")?;
        Ok(Self { seed })
    }

    /// createSignedContract: sign the source preimage and return its exact wire
    /// record. Unpacked size/price stay decimal strings in the signed record.
    pub fn sign_contract(&self, contract: &Contract) -> Result<Value, &'static str> {
        let bytes = contract.signing_bytes()?;
        let signature = super::sign(self.seed.as_ref(), &bytes)?;
        Ok(json!({
            "accountId": contract.account_id,
            "subAccountId": contract.sub_account_id,
            "slotId": contract.slot_id,
            "nonce": contract.nonce,
            "pairId": contract.pair_id,
            "size": decimal(&contract.size)?.to_string(),
            "price": decimal(&contract.price)?.to_string(),
            "direction": u8::from(contract.direction),
            "feeRates": contract.fee_rates,
            "hasSubsidy": u8::from(contract.has_subsidy),
            "signature": {
                "pubKey": format!("0x{}", hex::encode(signature.public_key)),
                "signature": hex::encode(signature.signature),
            }
        }))
    }
}
