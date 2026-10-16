use crate::deployment::is_lighter_robinhood_exchange;
use safe_cex_types::ExchangeName;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ts: LighterBuilderFeeMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LighterBuilderFeeMode {
    Dynamic,
    Static,
}

// ts: TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET
pub const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET: &str = "158385";
// ts: TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET
pub const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET: &str =
    "REPLACE_WITH_LIGHTER_TESTNET_INTEGRATOR_ACCOUNT_INDEX";
// ts: TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD
pub const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD: &str = "";
// ts: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE
pub const LIGHTER_INTEGRATOR_MAKER_FEE_VALUE: f64 = 0.0;
// ts: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE
pub const LIGHTER_INTEGRATOR_TAKER_FEE_VALUE: f64 = 250.0;

// ts: resolveMode
pub fn resolve_mode(value: Option<&Value>) -> LighterBuilderFeeMode {
    if value.and_then(Value::as_str) == Some("static") {
        LighterBuilderFeeMode::Static
    } else {
        LighterBuilderFeeMode::Dynamic
    }
}

// ts: getDefaultIntegratorAccountIndex
pub fn get_default_integrator_account_index(
    testnet: Option<bool>,
    exchange_name: Option<&ExchangeName>,
) -> Option<&'static str> {
    if is_lighter_robinhood_exchange(exchange_name) {
        return (!TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD.is_empty())
            .then_some(TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD);
    }
    Some(if testnet.unwrap_or(false) {
        TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET
    } else {
        TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET
    })
}

// Require a string before applying serde's variant table. Its default unit-enum
// decoder also accepts tagged objects, which the TypeScript literal union rejects.
#[derive(Deserialize)]
#[serde(remote = "LighterBuilderFeeMode", rename_all = "lowercase")]
enum LighterBuilderFeeModeWire {
    Dynamic,
    Static,
}
impl<'de> Deserialize<'de> for LighterBuilderFeeMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterBuilderFeeModeWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )
    }
}

// ts: LighterBuilderFeeConfig
// BigInt is a Rust integer, not a JSON string/number surrogate; TS JSON.stringify
// also cannot encode this record without a caller-supplied BigInt projection.
#[derive(Debug, Clone, PartialEq)]
pub struct LighterBuilderFeeConfig {
    pub integrator_account_index: num_bigint::BigInt,
    pub integrator_maker_fee: f64,
    pub integrator_taker_fee: f64,
    pub mode: LighterBuilderFeeMode,
}

// ts: parsePositiveIntegerBigInt
pub fn parse_positive_integer_big_int(value: Option<&Value>) -> Option<num_bigint::BigInt> {
    let text = safe_cex_num::js_trim(value?.as_str()?);
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // The TS digit guard has already narrowed BigInt conversion to unsigned
    // decimal text; the arbitrary-precision parser does not round through f64.
    num_bigint::BigInt::parse_bytes(text.as_bytes(), 10)
}

// ts: resolveLighterBuilderFeeConfig
// Project the exact options fields read by TS; retain unknown input handling
// at this boundary instead of imposing a stronger wire decoder on the helper.
pub fn resolve_lighter_builder_fee_config(
    testnet: Option<bool>,
    mode: Option<&Value>,
    configured_integrator_index: Option<&Value>,
    exchange_name: Option<&ExchangeName>,
) -> Option<LighterBuilderFeeConfig> {
    let mode = resolve_mode(mode);
    let default = get_default_integrator_account_index(testnet, exchange_name)
        .map(|s| Value::String(s.to_owned()));
    let integrator_account_index = parse_positive_integer_big_int(
        configured_integrator_index
            .filter(|v| !v.is_null())
            .or(default.as_ref()),
    )?;
    Some(LighterBuilderFeeConfig {
        integrator_account_index,
        integrator_maker_fee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
        integrator_taker_fee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
        mode,
    })
}
