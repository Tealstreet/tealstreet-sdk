//! Declared finite JSON wire shapes from bitget.types.ts.
use serde::{Deserialize, Serialize};

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

// Capture the declared numeric leaf before serde_json's strict f64 decoder.
// These fields precede the flattened extras buffer; the extras retain their
// existing Value path and are never routed through numeric ingress.
fn request_time<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?
        .decode::<f64>()
        .map_err(serde::de::Error::custom)
}

// JSON numeric bytes use ECMAScript spelling even for integral timestamps.
fn js_number<S: serde::Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return serializer.serialize_none();
    }
    let number: serde_json::Number = safe_cex_num::js_number_string(*value)
        .parse()
        .map_err(serde::ser::Error::custom)?;
    number.serialize(serializer)
}

// ts: FETCH_SINGLE_ACCOUNT_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchSingleAccountResponseData {
    #[serde(rename = "marginCoin")]
    pub margin_coin: String,
    #[serde(rename = "locked")]
    pub locked: String,
    #[serde(rename = "available")]
    pub available: String,
    #[serde(rename = "crossedMaxAvailable")]
    pub crossed_max_available: String,
    #[serde(rename = "isolatedMaxAvailable")]
    pub isolated_max_available: String,
    #[serde(rename = "maxTransferOut")]
    pub max_transfer_out: String,
    #[serde(rename = "accountEquity")]
    pub account_equity: String,
    #[serde(rename = "usdtEquity")]
    pub usdt_equity: String,
    #[serde(rename = "btcEquity")]
    pub btc_equity: String,
    #[serde(rename = "crossedRiskRate")]
    pub crossed_risk_rate: String,
    #[serde(rename = "crossedMarginLeverage")]
    pub crossed_margin_leverage: String,
    #[serde(rename = "isolatedLongLever")]
    pub isolated_long_lever: String,
    #[serde(rename = "isolatedShortLever")]
    pub isolated_short_lever: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    #[serde(rename = "posMode")]
    pub pos_mode: String,
    #[serde(rename = "unrealizedPL")]
    pub unrealized_pl: String,
    #[serde(rename = "coupon")]
    pub coupon: String,
    #[serde(rename = "assetMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub asset_mode: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_SINGLE_ACCOUNT_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchSingleAccountResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "data")]
    pub data: FetchSingleAccountResponseData,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "requestTime")]
    pub request_time: String,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: BITGET_UTA_CATEGORY
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BitgetUtaCategory {
    Usdt,
    Usdc,
    Coin,
}

// Canonical wire names live only in this private remote definition. Its generated
// permissive enum decoder must not be exposed as a public inherent operation.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BitgetUtaCategory")]
enum BitgetUtaCategoryWire {
    #[serde(rename = "USDT-FUTURES")]
    Usdt,
    #[serde(rename = "USDC-FUTURES")]
    Usdc,
    #[serde(rename = "COIN-FUTURES")]
    Coin,
}

// TS literal unions accept strings only, not serde's externally tagged objects.
impl<'de> Deserialize<'de> for BitgetUtaCategory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        BitgetUtaCategoryWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            text,
        ))
    }
}
impl Serialize for BitgetUtaCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BitgetUtaCategoryWire::serialize(self, serializer)
    }
}

#[cfg(test)]
#[path = "../tests/support/category_inventory.rs"]
mod category_inventory;

// ts: FETCH_UTA_ACCOUNT_ASSETS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaAccountAssetsResponseDataAssetsItem {
    #[serde(rename = "coin")]
    pub coin: String,
    #[serde(rename = "equity")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub equity: Option<String>,
    #[serde(rename = "usdValue")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub usd_value: Option<String>,
    #[serde(rename = "balance")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub balance: Option<String>,
    #[serde(rename = "available")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub available: Option<String>,
    #[serde(rename = "debt")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub debt: Option<String>,
    #[serde(rename = "locked")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub locked: Option<String>,
    #[serde(rename = "bonus")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub bonus: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ACCOUNT_ASSETS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaAccountAssetsResponseData {
    #[serde(rename = "accountEquity")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub account_equity: Option<String>,
    #[serde(rename = "usdtEquity")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub usdt_equity: Option<String>,
    #[serde(rename = "btcEquity")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub btc_equity: Option<String>,
    #[serde(rename = "unrealisedPnl")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub unrealised_pnl: Option<String>,
    #[serde(rename = "usdtUnrealisedPnl")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub usdt_unrealised_pnl: Option<String>,
    #[serde(rename = "btcUnrealizedPnl")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub btc_unrealized_pnl: Option<String>,
    #[serde(rename = "effEquity")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub eff_equity: Option<String>,
    #[serde(rename = "mmr")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub mmr: Option<String>,
    #[serde(rename = "imr")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub imr: Option<String>,
    #[serde(rename = "mgnRatio")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub mgn_ratio: Option<String>,
    #[serde(rename = "positionMgnRatio")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub position_mgn_ratio: Option<String>,
    #[serde(rename = "positionValue")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub position_value: Option<String>,
    #[serde(rename = "leverage")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub leverage: Option<String>,
    #[serde(rename = "assets")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub assets: Option<Vec<FetchUtaAccountAssetsResponseDataAssetsItem>>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ACCOUNT_ASSETS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaAccountAssetsResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "requestTime")]
    #[serde(serialize_with = "js_number", deserialize_with = "request_time")]
    pub request_time: f64,
    #[serde(rename = "data")]
    #[serde(deserialize_with = "nullable")]
    pub data: Option<FetchUtaAccountAssetsResponseData>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaAccountSettingsResponseDataSymbolConfigListItem {
    #[serde(rename = "category")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub category: Option<String>,
    #[serde(rename = "symbol")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub symbol: Option<String>,
    #[serde(rename = "marginMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub margin_mode: Option<String>,
    #[serde(rename = "leverage")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub leverage: Option<String>,
    #[serde(rename = "longLeverage")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub long_leverage: Option<String>,
    #[serde(rename = "shortLeverage")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub short_leverage: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaAccountSettingsResponseData {
    #[serde(rename = "accountMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub account_mode: Option<String>,
    #[serde(rename = "accountLevel")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub account_level: Option<String>,
    #[serde(rename = "assetMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub asset_mode: Option<String>,
    #[serde(rename = "holdMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub hold_mode: Option<String>,
    #[serde(rename = "symbolConfigList")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub symbol_config_list: Option<Vec<FetchUtaAccountSettingsResponseDataSymbolConfigListItem>>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaAccountSettingsResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "requestTime")]
    #[serde(serialize_with = "js_number", deserialize_with = "request_time")]
    pub request_time: f64,
    #[serde(rename = "data")]
    #[serde(deserialize_with = "nullable")]
    pub data: Option<FetchUtaAccountSettingsResponseData>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: BITGET_UTA_POSITION
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BitgetUtaPosition {
    #[serde(rename = "category")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub category: Option<String>,
    #[serde(rename = "symbol")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub symbol: Option<String>,
    #[serde(rename = "marginCoin")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub margin_coin: Option<String>,
    #[serde(rename = "holdMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub hold_mode: Option<String>,
    #[serde(rename = "posSide")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub pos_side: Option<String>,
    #[serde(rename = "holdSide")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub hold_side: Option<String>,
    #[serde(rename = "marginMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub margin_mode: Option<String>,
    #[serde(rename = "positionBalance")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub position_balance: Option<String>,
    #[serde(rename = "marginSize")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub margin_size: Option<String>,
    #[serde(rename = "available")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub available: Option<String>,
    #[serde(rename = "frozen")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub frozen: Option<String>,
    #[serde(rename = "total")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub total: Option<String>,
    #[serde(rename = "size")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub size: Option<String>,
    #[serde(rename = "leverage")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub leverage: Option<String>,
    #[serde(rename = "curRealisedPnl")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cur_realised_pnl: Option<String>,
    #[serde(rename = "avgPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub avg_price: Option<String>,
    #[serde(rename = "openPriceAvg")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub open_price_avg: Option<String>,
    #[serde(rename = "positionStatus")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub position_status: Option<String>,
    #[serde(rename = "unrealisedPnl")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub unrealised_pnl: Option<String>,
    #[serde(rename = "unrealizedPL")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub unrealized_pl: Option<String>,
    #[serde(rename = "liquidationPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub liquidation_price: Option<String>,
    #[serde(rename = "liqPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub liq_price: Option<String>,
    #[serde(rename = "mmr")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub mmr: Option<String>,
    #[serde(rename = "profitRate")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub profit_rate: Option<String>,
    #[serde(rename = "markPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub mark_price: Option<String>,
    #[serde(rename = "breakEvenPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub break_even_price: Option<String>,
    #[serde(rename = "totalFunding")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub total_funding: Option<String>,
    #[serde(rename = "totalFee")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub total_fee: Option<String>,
    #[serde(rename = "openFeeTotal")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub open_fee_total: Option<String>,
    #[serde(rename = "closeFeeTotal")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub close_fee_total: Option<String>,
    #[serde(rename = "deductedFee")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub deducted_fee: Option<String>,
    #[serde(rename = "cashDividend")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cash_dividend: Option<String>,
    #[serde(rename = "createdTime")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub created_time: Option<String>,
    #[serde(rename = "updatedTime")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub updated_time: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_POSITIONS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaPositionsResponseData {
    #[serde(rename = "list")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub list: Option<Vec<BitgetUtaPosition>>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_POSITIONS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaPositionsResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "requestTime")]
    #[serde(serialize_with = "js_number", deserialize_with = "request_time")]
    pub request_time: f64,
    #[serde(rename = "data")]
    #[serde(deserialize_with = "nullable")]
    pub data: Option<FetchUtaPositionsResponseData>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: BITGET_UTA_ORDER
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BitgetUtaOrderFeeDetailItem {
    #[serde(rename = "feeCoin")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub fee_coin: Option<String>,
    #[serde(rename = "fee")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub fee: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: BITGET_UTA_ORDER
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BitgetUtaOrder {
    #[serde(rename = "category")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub category: Option<String>,
    #[serde(rename = "symbol")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub symbol: Option<String>,
    #[serde(rename = "orderId")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub order_id: Option<String>,
    #[serde(rename = "clientOid")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub client_oid: Option<String>,
    #[serde(rename = "requestId")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub request_id: Option<String>,
    #[serde(rename = "price")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub price: Option<String>,
    #[serde(rename = "qty")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub qty: Option<String>,
    #[serde(rename = "amount")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub amount: Option<String>,
    #[serde(rename = "holdMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub hold_mode: Option<String>,
    #[serde(rename = "holdSide")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub hold_side: Option<String>,
    #[serde(rename = "posSide")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub pos_side: Option<String>,
    #[serde(rename = "delegateType")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub delegate_type: Option<String>,
    #[serde(rename = "tradeSide")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub trade_side: Option<String>,
    #[serde(rename = "orderType")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub order_type: Option<String>,
    #[serde(rename = "timeInForce")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub time_in_force: Option<String>,
    #[serde(rename = "side")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub side: Option<String>,
    #[serde(rename = "marginMode")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub margin_mode: Option<String>,
    #[serde(rename = "marginCoin")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub margin_coin: Option<String>,
    #[serde(rename = "reduceOnly")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub reduce_only: Option<String>,
    #[serde(rename = "cumExecQty")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cum_exec_qty: Option<String>,
    #[serde(rename = "cumExecValue")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cum_exec_value: Option<String>,
    #[serde(rename = "avgPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub avg_price: Option<String>,
    #[serde(rename = "orderStatus")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub order_status: Option<String>,
    #[serde(rename = "cancelReason")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cancel_reason: Option<String>,
    #[serde(rename = "leverage")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub leverage: Option<String>,
    #[serde(rename = "feeDetail")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub fee_detail: Option<Vec<BitgetUtaOrderFeeDetailItem>>,
    #[serde(rename = "createdTime")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub created_time: Option<String>,
    #[serde(rename = "updatedTime")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub updated_time: Option<String>,
    #[serde(rename = "takeProfit")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub take_profit: Option<String>,
    #[serde(rename = "stopLoss")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub stop_loss: Option<String>,
    #[serde(rename = "tpTriggerBy")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub tp_trigger_by: Option<String>,
    #[serde(rename = "slTriggerBy")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub sl_trigger_by: Option<String>,
    #[serde(rename = "tpOrderType")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub tp_order_type: Option<String>,
    #[serde(rename = "slOrderType")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub sl_order_type: Option<String>,
    #[serde(rename = "tpLimitPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub tp_limit_price: Option<String>,
    #[serde(rename = "slLimitPrice")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub sl_limit_price: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ORDERS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaOrdersResponseData {
    #[serde(rename = "list")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub list: Option<Vec<BitgetUtaOrder>>,
    #[serde(rename = "cursor")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cursor: Option<String>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_UTA_ORDERS_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchUtaOrdersResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "requestTime")]
    #[serde(serialize_with = "js_number", deserialize_with = "request_time")]
    pub request_time: f64,
    #[serde(rename = "data")]
    #[serde(deserialize_with = "nullable")]
    pub data: Option<FetchUtaOrdersResponseData>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_POSITIONS_V2_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchPositionsV2ResponseDataItem {
    #[serde(rename = "marginCoin")]
    pub margin_coin: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "holdSide")]
    pub hold_side: String,
    #[serde(rename = "openDelegateSize")]
    pub open_delegate_size: String,
    #[serde(rename = "marginSize")]
    pub margin_size: String,
    #[serde(rename = "available")]
    pub available: String,
    #[serde(rename = "locked")]
    pub locked: String,
    #[serde(rename = "total")]
    pub total: String,
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "achievedProfits")]
    pub achieved_profits: String,
    #[serde(rename = "openPriceAvg")]
    pub open_price_avg: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    #[serde(rename = "posMode")]
    pub pos_mode: String,
    #[serde(rename = "unrealizedPL")]
    pub unrealized_pl: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(rename = "keepMarginRate")]
    pub keep_margin_rate: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "breakEvenPrice")]
    pub break_even_price: String,
    #[serde(rename = "totalFee")]
    pub total_fee: String,
    #[serde(rename = "deductedFee")]
    pub deducted_fee: String,
    #[serde(rename = "takeProfit")]
    pub take_profit: String,
    #[serde(rename = "stopLoss")]
    pub stop_loss: String,
    #[serde(rename = "takeProfitId")]
    pub take_profit_id: String,
    #[serde(rename = "stopLossId")]
    pub stop_loss_id: String,
    #[serde(rename = "marginRatio")]
    pub margin_ratio: String,
    #[serde(rename = "assetMode")]
    pub asset_mode: String,
    #[serde(rename = "cTime")]
    pub c_time: String,
    #[serde(rename = "uTime")]
    pub u_time: String,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// ts: FETCH_POSITIONS_V2_RESPONSE
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchPositionsV2Response {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "requestTime")]
    #[serde(serialize_with = "js_number", deserialize_with = "request_time")]
    pub request_time: f64,
    #[serde(rename = "data")]
    pub data: Vec<FetchPositionsV2ResponseDataItem>,
    // Flatten enforces a JSON object and retains undeclared venue fields.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
