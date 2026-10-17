//! TransactionClient's unsigned preimage builders. Signing, nonce allocation and
//! HTTP submission are separate. The injected clock is called at source call sites.
use super::transaction_client_helpers::*;
use super::transaction_client_types::*;
use num_bigint::BigInt;
use safe_cex_num::{JsPrimitive, js_bigint_primitive, js_number_string};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionBuildError {
    pub name: &'static str,
    pub message: Vec<u16>,
}
impl TransactionBuildError {
    fn error(message: impl AsRef<str>) -> Self {
        Self {
            name: "Error",
            message: message.as_ref().encode_utf16().collect(),
        }
    }
}
impl From<Vec<u16>> for TransactionBuildError {
    fn from(message: Vec<u16>) -> Self {
        Self {
            name: "Error",
            message,
        }
    }
}
pub struct TransactionBuildContext<'a> {
    pub chain_id: f64,
    pub account_index: &'a BigInt,
    pub api_key_index: f64,
    pub now_millis: &'a dyn Fn() -> f64,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateOrderDataBuildOptions {
    pub expired_at: Option<BigInt>,
    pub default_order_expiry: Option<BigInt>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CreateOrderData {
    pub message_hex: String,
    pub l2_attributes_message_hex: Option<String>,
    pub expired_at: BigInt,
    pub order_type_value: f64,
    pub time_in_force_value: f64,
    pub trigger_price: BigInt,
    pub order_expiry: BigInt,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiringTransactionData {
    pub message_hex: String,
    pub expired_at: BigInt,
}
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateLeverageData {
    pub message_hex: String,
    pub expired_at: BigInt,
    pub initial_margin_fraction: f64,
}

fn u32_field(value: f64, name: &str) -> Result<String, TransactionBuildError> {
    to_uint32_field_hex(value, &name.encode_utf16().collect::<Vec<_>>()).map_err(Into::into)
}
fn i64_field(value: &BigInt, name: &str) -> Result<String, TransactionBuildError> {
    to_int64_field_hex(value, &name.encode_utf16().collect::<Vec<_>>()).map_err(Into::into)
}
fn number_bigint(value: f64) -> Result<BigInt, TransactionBuildError> {
    js_bigint_primitive(&JsPrimitive::Number(value)).map_err(|error| TransactionBuildError {
        name: error.kind.name(),
        message: error.message_utf16,
    })
}
fn bigint_number(value: &BigInt) -> Result<f64, TransactionBuildError> {
    safe_cex_num::js_number_primitive(&JsPrimitive::BigInt(value.clone())).map_err(|error| {
        TransactionBuildError {
            name: error.name,
            message: error.message.encode_utf16().collect(),
        }
    })
}

fn expiry(context: &TransactionBuildContext<'_>) -> Result<BigInt, TransactionBuildError> {
    number_bigint((context.now_millis)() + 600_000.0)
}
fn order_type(params: &CreateOrderParams) -> f64 {
    let value = params
        .order_type
        .map(|v| v.as_str().encode_utf16().collect::<Vec<_>>());
    map_order_type_value(value.as_deref())
}
fn time_in_force(params: &CreateOrderParams) -> f64 {
    let kind = params
        .order_type
        .map(|v| v.as_str().encode_utf16().collect::<Vec<_>>());
    let time = params
        .time_in_force
        .map(|v| v.as_str().encode_utf16().collect::<Vec<_>>());
    map_time_in_force_value(kind.as_deref(), params.post_only, time.as_deref())
}
fn integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0
}

// ts: TransactionClient#buildCreateOrderL2AttributesData
pub fn build_create_order_l2_attributes_data(
    index: &BigInt,
    taker: f64,
    maker: f64,
) -> Result<String, TransactionBuildError> {
    // Source constructs both BigInt fees before filtering zero values.
    let mut attributes = vec![
        (1.0, index.clone()),
        (2.0, number_bigint(taker)?),
        (3.0, number_bigint(maker)?),
    ];
    attributes.retain(|(_, value)| *value != BigInt::from(0));
    // Source order is already ascending; zero filtering preserves that order.
    while attributes.len() < 4 {
        attributes.push((0.0, BigInt::from(0)));
    }
    let mut parts = String::new();
    for (kind, value) in attributes {
        parts.push_str(&u32_field(
            kind,
            &format!("l2AttributeType_{}", js_number_string(kind)),
        )?);
        parts.push_str(&i64_field(
            &value,
            &format!("l2AttributeValue_{}", js_number_string(kind)),
        )?);
    }
    Ok(parts)
}
// ts: TransactionClient#buildCreateOrderData
pub fn build_create_order_data(
    context: &TransactionBuildContext<'_>,
    params: &CreateOrderParams,
    nonce: &BigInt,
    options: &CreateOrderDataBuildOptions,
) -> Result<CreateOrderData, TransactionBuildError> {
    let expired_at = if let Some(value) = params.expired_at.as_ref().or(options.expired_at.as_ref())
    {
        value.clone()
    } else {
        expiry(context)?
    };
    let order_type_value = order_type(params);
    let time_in_force_value = time_in_force(params);
    let trigger_price = params
        .trigger_price
        .clone()
        .unwrap_or_else(|| BigInt::from(0));
    let conditional = matches!(
        params.order_type,
        Some(
            CreateOrderKind::StopLoss
                | CreateOrderKind::StopLossLimit
                | CreateOrderKind::TakeProfit
                | CreateOrderKind::TakeProfitLimit
        )
    );
    let immediate = time_in_force_value == 0.0 && !conditional;
    let order_expiry = if let Some(value) = &params.order_expiry {
        value.clone()
    } else if immediate {
        BigInt::from(0)
    } else if let Some(value) = &options.default_order_expiry {
        value.clone()
    } else {
        number_bigint((context.now_millis)() + 28.0 * 24.0 * 60.0 * 60.0 * 1000.0)?
    };
    let attribution = match (
        &params.integrator_account_index,
        params.integrator_maker_fee,
        params.integrator_taker_fee,
    ) {
        (Some(index), Some(maker), Some(taker)) => {
            if *index < BigInt::from(0) {
                return Err(TransactionBuildError::error(
                    "integratorAccountIndex must be non-negative",
                ));
            }
            if !integer(maker) || maker < 0.0 {
                return Err(TransactionBuildError::error(
                    "integratorMakerFee must be a non-negative integer",
                ));
            }
            if !integer(taker) || taker < 0.0 {
                return Err(TransactionBuildError::error(
                    "integratorTakerFee must be a non-negative integer",
                ));
            }
            Some((index, maker, taker))
        }
        _ => None,
    };
    let parts = [
        u32_field(context.chain_id, "chainId")?,
        u32_field(14.0, "txType")?,
        i64_field(nonce, "nonce")?,
        i64_field(&expired_at, "expiredAt")?,
        i64_field(context.account_index, "accountIndex")?,
        u32_field(context.api_key_index, "apiKeyIndex")?,
        u32_field(params.market_id, "marketId")?,
        i64_field(&params.order_id, "orderId")?,
        i64_field(&params.amount, "amount")?,
        u32_field(bigint_number(&params.price)?, "price")?,
        u32_field(if params.is_buy { 0.0 } else { 1.0 }, "isAsk")?,
        u32_field(order_type_value, "orderType")?,
        u32_field(time_in_force_value, "timeInForce")?,
        u32_field(
            if params.reduce_only == Some(true) {
                1.0
            } else {
                0.0
            },
            "reduceOnly",
        )?,
        u32_field(bigint_number(&trigger_price)?, "triggerPrice")?,
        i64_field(&order_expiry, "orderExpiry")?,
    ];
    let l2_attributes_message_hex = attribution
        .map(|(index, maker, taker)| build_create_order_l2_attributes_data(index, taker, maker))
        .transpose()?;
    Ok(CreateOrderData {
        message_hex: parts.concat(),
        l2_attributes_message_hex,
        expired_at,
        order_type_value,
        time_in_force_value,
        trigger_price,
        order_expiry,
        integrator_account_index: attribution.map(|(index, _, _)| index.clone()),
        integrator_maker_fee: attribution.map(|(_, maker, _)| maker),
        integrator_taker_fee: attribution.map(|(_, _, taker)| taker),
    })
}
// ts: TransactionClient#buildGroupedOrderInfoData
pub fn build_grouped_order_info_data(
    params: &CreateOrderParams,
    payload: &CreateOrderData,
) -> Result<String, TransactionBuildError> {
    Ok([
        u32_field(params.market_id, "marketId")?,
        i64_field(&params.order_id, "orderId")?,
        i64_field(&params.amount, "amount")?,
        u32_field(bigint_number(&params.price)?, "price")?,
        u32_field(if params.is_buy { 0.0 } else { 1.0 }, "isAsk")?,
        u32_field(payload.order_type_value, "orderType")?,
        u32_field(payload.time_in_force_value, "timeInForce")?,
        u32_field(
            if params.reduce_only == Some(true) {
                1.0
            } else {
                0.0
            },
            "reduceOnly",
        )?,
        u32_field(bigint_number(&payload.trigger_price)?, "triggerPrice")?,
        i64_field(&payload.order_expiry, "orderExpiry")?,
    ]
    .concat())
}
// ts: TransactionClient#buildCancelOrderData
pub fn build_cancel_order_data(
    context: &TransactionBuildContext<'_>,
    params: &CancelOrderParams,
    nonce: &BigInt,
) -> Result<ExpiringTransactionData, TransactionBuildError> {
    let expired_at = expiry(context)?;
    let message_hex = [
        u32_field(context.chain_id, "chainId")?,
        u32_field(15.0, "txType")?,
        i64_field(nonce, "nonce")?,
        i64_field(&expired_at, "expiredAt")?,
        i64_field(context.account_index, "accountIndex")?,
        u32_field(context.api_key_index, "apiKeyIndex")?,
        u32_field(params.market_id, "marketId")?,
        i64_field(&params.order_id, "orderId")?,
    ]
    .concat();
    Ok(ExpiringTransactionData {
        message_hex,
        expired_at,
    })
}
// ts: TransactionClient#buildUpdateLeverageData
pub fn build_update_leverage_data(
    context: &TransactionBuildContext<'_>,
    params: &UpdateLeverageParams,
    nonce: &BigInt,
) -> Result<UpdateLeverageData, TransactionBuildError> {
    if !params.leverage.is_finite() || params.leverage <= 0.0 {
        return Err(TransactionBuildError::error(format!(
            "Invalid leverage value: {}",
            js_number_string(params.leverage)
        )));
    }
    if !integer(params.market_id) || !(0.0..=254.0).contains(&params.market_id) {
        return Err(TransactionBuildError::error(format!(
            "marketId is out of valid range (0-254): {}",
            js_number_string(params.market_id)
        )));
    }
    let initial_margin_fraction = (10_000.0 / params.leverage).floor();
    if initial_margin_fraction <= 0.0 || initial_margin_fraction > 10_000.0 {
        return Err(TransactionBuildError::error(format!(
            "Derived initial margin fraction is out of range: {} (from leverage {})",
            js_number_string(initial_margin_fraction),
            js_number_string(params.leverage)
        )));
    }
    let expired_at = expiry(context)?;
    let message_hex = [
        u32_field(context.chain_id, "chainId")?,
        u32_field(20.0, "txType")?,
        i64_field(nonce, "nonce")?,
        i64_field(&expired_at, "expiredAt")?,
        i64_field(context.account_index, "accountIndex")?,
        u32_field(context.api_key_index, "apiKeyIndex")?,
        i64_field(&number_bigint(params.market_id)?, "marketId")?,
        i64_field(
            &number_bigint(initial_margin_fraction)?,
            "initialMarginFraction",
        )?,
        u32_field(params.margin_mode.value(), "marginMode")?,
    ]
    .concat();
    Ok(UpdateLeverageData {
        message_hex,
        expired_at,
        initial_margin_fraction,
    })
}
// ts: TransactionClient#buildApproveIntegratorData
pub fn build_approve_integrator_data(
    context: &TransactionBuildContext<'_>,
    params: &ApproveIntegratorParams,
    nonce: &BigInt,
) -> Result<ExpiringTransactionData, TransactionBuildError> {
    if params.integrator_account_index < BigInt::from(0) {
        return Err(TransactionBuildError::error(
            "integratorAccountIndex must be non-negative",
        ));
    }
    let fields = [
        ("maxPerpsTakerFee", params.max_perps_taker_fee),
        ("maxPerpsMakerFee", params.max_perps_maker_fee),
        ("maxSpotTakerFee", params.max_spot_taker_fee),
        ("maxSpotMakerFee", params.max_spot_maker_fee),
    ];
    for (name, value) in fields {
        if !integer(value) || !(0.0..=1_000_000.0).contains(&value) {
            return Err(TransactionBuildError::error(format!(
                "{name} must be an integer in [0, 1000000]"
            )));
        }
    }
    let revoking = fields.iter().all(|(_, value)| *value == 0.0);
    if revoking != (params.approval_expiry == BigInt::from(0)) {
        return Err(TransactionBuildError::error(
            "approvalExpiry must be 0 iff all max fee fields are 0 (revocation semantics)",
        ));
    }
    if params.approval_expiry < BigInt::from(0)
        || params.approval_expiry > ((BigInt::from(1) << 48) - 1)
    {
        return Err(TransactionBuildError::error(
            "approvalExpiry must be in [0, 281474976710655]",
        ));
    }
    let expired_at = expiry(context)?;
    let message_hex = [
        u32_field(context.chain_id, "chainId")?,
        u32_field(45.0, "txType")?,
        i64_field(nonce, "nonce")?,
        i64_field(&expired_at, "expiredAt")?,
        i64_field(context.account_index, "accountIndex")?,
        u32_field(context.api_key_index, "apiKeyIndex")?,
        i64_field(&params.integrator_account_index, "integratorAccountIndex")?,
        u32_field(params.max_perps_taker_fee, "maxPerpsTakerFee")?,
        u32_field(params.max_perps_maker_fee, "maxPerpsMakerFee")?,
        u32_field(params.max_spot_taker_fee, "maxSpotTakerFee")?,
        u32_field(params.max_spot_maker_fee, "maxSpotMakerFee")?,
        i64_field(&params.approval_expiry, "approvalExpiry")?,
    ]
    .concat();
    Ok(ExpiringTransactionData {
        message_hex,
        expired_at,
    })
}
// ts: TransactionClient#validateCreateGroupedOrdersParams
pub fn validate_create_grouped_orders_params(
    params: &CreateGroupedOrdersParams,
) -> Result<(), TransactionBuildError> {
    validate_grouped_orders_with_side_comparison(params, &|a, b| {
        params.orders[a].is_buy == params.orders[b].is_buy
    })
}

pub(crate) fn validate_grouped_orders_with_side_comparison(
    params: &CreateGroupedOrdersParams,
    same_side: &dyn Fn(usize, usize) -> bool,
) -> Result<(), TransactionBuildError> {
    let orders = &params.orders;
    if orders.is_empty() || orders.len() > 3 {
        return Err(TransactionBuildError::error(
            "Lighter grouped orders require 1-3 orders",
        ));
    }
    let market_id = orders[0].market_id;
    if !integer(market_id) || !(0.0..=254.0).contains(&market_id) {
        return Err(TransactionBuildError::error(format!(
            "Grouped orders require a valid perps market id (0-254): {}",
            js_number_string(market_id)
        )));
    }
    let mut ids = std::collections::HashSet::new();
    for order in orders {
        if order.market_id != market_id {
            return Err(TransactionBuildError::error(
                "Grouped orders must all use the same market id",
            ));
        }
        if !ids.insert(order.order_id.clone()) {
            return Err(TransactionBuildError::error(format!(
                "Grouped orders contain duplicate client order id {}",
                order.order_id
            )));
        }
    }
    let kinds = orders.iter().map(order_type).collect::<Vec<_>>();
    match params.grouping_type {
        LighterGroupedOrderType::OneCancelsTheOther => {
            if orders.len() != 2 {
                return Err(TransactionBuildError::error(
                    "Lighter OCO grouped orders require exactly two orders",
                ));
            }
            if orders[0].amount != orders[1].amount {
                return Err(TransactionBuildError::error(
                    "Lighter OCO grouped orders require matching base amounts",
                ));
            }
            if !same_side(0, 1) {
                return Err(TransactionBuildError::error(
                    "Lighter OCO grouped orders require the same side",
                ));
            }
            if orders[0].reduce_only != Some(true) || orders[1].reduce_only != Some(true) {
                return Err(TransactionBuildError::error(
                    "Lighter OCO grouped orders must be reduce-only",
                ));
            }
            if !kinds.iter().all(|kind| is_child_grouped_order_type(*kind)) {
                return Err(TransactionBuildError::error(
                    "Lighter OCO grouped orders must be stop-loss/take-profit children",
                ));
            }
            if !kinds.iter().any(|kind| is_stop_loss_order_type(*kind))
                || !kinds.iter().any(|kind| is_take_profit_order_type(*kind))
            {
                return Err(TransactionBuildError::error(
                    "Lighter OCO grouped orders require one stop-loss and one take-profit order",
                ));
            }
        }
        LighterGroupedOrderType::OneTriggersTheOther => {
            if orders.len() != 2 {
                return Err(TransactionBuildError::error(
                    "Lighter OTO grouped orders require exactly two orders",
                ));
            }
            if kinds[0] != 0.0 && kinds[0] != 1.0 {
                return Err(TransactionBuildError::error(
                    "Lighter OTO parent order must be limit or market",
                ));
            }
            if !is_child_grouped_order_type(kinds[1]) {
                return Err(TransactionBuildError::error(
                    "Lighter OTO child order must be stop-loss or take-profit",
                ));
            }
            if same_side(0, 1) {
                return Err(TransactionBuildError::error(
                    "Lighter OTO child order must be opposite side from parent",
                ));
            }
            if orders[1].amount != BigInt::from(0) {
                return Err(TransactionBuildError::error(
                    "Lighter OTO child order amount must be 0",
                ));
            }
        }
        LighterGroupedOrderType::OneTriggersOco => {
            if orders.len() != 3 {
                return Err(TransactionBuildError::error(
                    "Lighter OTOCO grouped orders require exactly three orders",
                ));
            }
            if kinds[0] != 0.0 && kinds[0] != 1.0 {
                return Err(TransactionBuildError::error(
                    "Lighter OTOCO parent order must be limit or market",
                ));
            }
            if !kinds[1..]
                .iter()
                .all(|kind| is_child_grouped_order_type(*kind))
            {
                return Err(TransactionBuildError::error(
                    "Lighter OTOCO children must be stop-loss/take-profit orders",
                ));
            }
            if !kinds[1..].iter().any(|kind| is_stop_loss_order_type(*kind))
                || !kinds[1..]
                    .iter()
                    .any(|kind| is_take_profit_order_type(*kind))
            {
                return Err(TransactionBuildError::error(
                    "Lighter OTOCO children require one stop-loss and one take-profit order",
                ));
            }
            if same_side(0, 1) || same_side(0, 2) {
                return Err(TransactionBuildError::error(
                    "Lighter OTOCO child orders must be opposite side from parent",
                ));
            }
            if orders[1].amount != BigInt::from(0) || orders[2].amount != BigInt::from(0) {
                return Err(TransactionBuildError::error(
                    "Lighter OTOCO child order amounts must be 0",
                ));
            }
        }
    }
    Ok(())
}

// A power-of-two radix expresses a binary64's exact binary fraction directly.
fn number_hex(value: f64) -> String {
    if !value.is_finite() {
        return js_number_string(value);
    }
    if value == 0.0 {
        return "0".into();
    }
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let significand = (bits & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
    let shift = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut body = if shift >= 0 {
        (BigInt::from(significand) << shift as usize).to_str_radix(16)
    } else {
        let places = (-shift as usize).div_ceil(4);
        let digits =
            (BigInt::from(significand) << (places * 4 - (-shift as usize))).to_str_radix(16);
        let padded = format!("{digits:0>width$}", width = places + 1);
        let split = padded.len() - places;
        let integer = padded[..split].trim_start_matches('0');
        let integer = if integer.is_empty() { "0" } else { integer };
        let fraction = padded[split..].trim_end_matches('0');
        if fraction.is_empty() {
            integer.to_owned()
        } else {
            format!("{integer}.{fraction}")
        }
    };
    if value < 0.0 {
        body.insert(0, '-');
    }
    body
}
// ts: TransactionClient#buildTransferData
pub fn build_transfer_data(
    context: &TransactionBuildContext<'_>,
    params: &TransferParams,
    nonce: &BigInt,
) -> String {
    [
        format!("{:0>8}", number_hex(context.chain_id)),
        format!("{:0>64}", context.account_index.to_str_radix(16)),
        format!("{:0>64}", nonce.to_str_radix(16)),
        format!("{:0>64}", params.to_account_index.to_str_radix(16)),
        format!("{:0>64}", params.amount.to_str_radix(16)),
        format!("{:0>8}", number_hex(params.token_id.unwrap_or(0.0))),
    ]
    .concat()
}

// ts: TransactionClient.getAccountIndex
pub fn get_account_index(context: &TransactionBuildContext<'_>) -> BigInt {
    context.account_index.clone()
}
// ts: TransactionClient.getApiKeyIndex
pub fn get_api_key_index(context: &TransactionBuildContext<'_>) -> f64 {
    context.api_key_index
}
// ts: TransactionClient.getChainId
pub fn get_chain_id(context: &TransactionBuildContext<'_>) -> f64 {
    context.chain_id
}
