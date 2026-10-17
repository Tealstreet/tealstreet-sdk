//! TransactionClient parameter decoders and L2 transaction-info encoders. The
//! encoders build the source object literals: every key present, undefined kept.
use crate::{
    http_transactions::*,
    transaction_client::{SendTxBatchResponse, TransactionRef, VolumeQuotaRemaining},
    transaction_client_types::*,
    transaction_promise::{Shared, shared},
};
use num_bigint::BigInt;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;

fn mismatch(key: &str, expected: &str, value: &Js) -> Thrown {
    Thrown::type_error(&format!(
        "{key} must be {expected}, received {}",
        value.type_of()
    ))
}

fn optional<T>(
    object: &Js,
    key: &str,
    read: impl FnOnce(&str, Js) -> Result<T, Thrown>,
) -> Result<Option<T>, Thrown> {
    match object.get(key)? {
        Js::Undefined => Ok(None),
        value => read(key, value).map(Some),
    }
}

fn as_big(key: &str, value: Js) -> Result<BigInt, Thrown> {
    match value {
        Js::BigInt(n) => Ok((*n).clone()),
        other => Err(mismatch(key, "a bigint", &other)),
    }
}
fn as_number(key: &str, value: Js) -> Result<f64, Thrown> {
    match value {
        Js::Number(n) => Ok(n),
        other => Err(mismatch(key, "a number", &other)),
    }
}
fn as_text(key: &str, value: Js) -> Result<Vec<u16>, Thrown> {
    match value {
        Js::String(s) => Ok(s.to_vec()),
        other => Err(mismatch(key, "a string", &other)),
    }
}
fn big(object: &Js, key: &str) -> Result<BigInt, Thrown> {
    as_big(key, object.get(key)?)
}
fn number(object: &Js, key: &str) -> Result<f64, Thrown> {
    as_number(key, object.get(key)?)
}

fn literal<T: Copy>(key: &str, value: Js, table: &[(&str, T)]) -> Result<T, Thrown> {
    if let Js::String(text) = &value {
        let text = String::from_utf16_lossy(text);
        if let Some((_, kind)) = table.iter().find(|(name, _)| *name == text) {
            return Ok(*kind);
        }
    }
    Err(Thrown::type_error(&format!(
        "{key} is not one of the declared literals: {}",
        value.to_display()
    )))
}

const ORDER_KINDS: [(&str, CreateOrderKind); 6] = [
    ("limit", CreateOrderKind::Limit),
    ("market", CreateOrderKind::Market),
    ("stop_loss", CreateOrderKind::StopLoss),
    ("stop_loss_limit", CreateOrderKind::StopLossLimit),
    ("take_profit", CreateOrderKind::TakeProfit),
    ("take_profit_limit", CreateOrderKind::TakeProfitLimit),
];
const TIME_IN_FORCE: [(&str, CreateOrderTimeInForce); 4] = [
    ("GoodTillCancel", CreateOrderTimeInForce::GoodTillCancel),
    (
        "ImmediateOrCancel",
        CreateOrderTimeInForce::ImmediateOrCancel,
    ),
    ("FillOrKill", CreateOrderTimeInForce::FillOrKill),
    ("PostOnly", CreateOrderTimeInForce::PostOnly),
];

pub(super) struct OrderFlagValues {
    pub is_buy: Js,
    pub post_only: Js,
    pub reduce_only: Js,
}

// ts: CreateOrderParams
pub(super) fn create_order(params: &Js) -> Result<(CreateOrderParams, OrderFlagValues), Thrown> {
    let order_id = big(params, "orderId")?;
    let market_id = number(params, "marketId")?;
    let is_buy = params.get("isBuy")?;
    let price = big(params, "price")?;
    let amount = big(params, "amount")?;
    let post_only = params.get("postOnly")?;
    let reduce_only = params.get("reduceOnly")?;
    let flags = OrderFlagValues {
        is_buy,
        post_only,
        reduce_only,
    };
    Ok((
        CreateOrderParams {
            order_id,
            market_id,
            is_buy: flags.is_buy.truthy(),
            price,
            amount,
            post_only: (!flags.post_only.is_undefined()).then(|| flags.post_only.truthy()),
            reduce_only: (!flags.reduce_only.is_undefined()).then(|| flags.reduce_only.truthy()),
            order_type: optional(params, "orderType", |k, v| literal(k, v, &ORDER_KINDS))?,
            trigger_price: optional(params, "triggerPrice", as_big)?,
            order_expiry: optional(params, "orderExpiry", as_big)?,
            expired_at: optional(params, "expiredAt", as_big)?,
            time_in_force: optional(params, "timeInForce", |k, v| literal(k, v, &TIME_IN_FORCE))?,
            integrator_account_index: optional(params, "integratorAccountIndex", as_big)?,
            integrator_maker_fee: optional(params, "integratorMakerFee", as_number)?,
            integrator_taker_fee: optional(params, "integratorTakerFee", as_number)?,
        },
        flags,
    ))
}

// ts: CreateGroupedOrdersParams
pub(super) fn create_grouped_orders(
    params: &Js,
) -> Result<(CreateGroupedOrdersParams, Vec<OrderFlagValues>), Thrown> {
    let orders = params.get("orders")?;
    let values = orders
        .array_values()
        .ok_or_else(|| mismatch("orders", "an array", &orders))?;
    if values.is_empty() || values.len() > 3 {
        return Err(Thrown::error(
            "Error",
            "Lighter grouped orders require 1-3 orders",
        ));
    }
    let market = values[0].get("marketId")?;
    let valid_market = market
        .as_f64()
        .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && (0.0..=254.0).contains(&n));
    if !valid_market {
        return Err(Thrown::error(
            "Error",
            &format!(
                "Grouped orders require a valid perps market id (0-254): {}",
                market.to_display()
            ),
        ));
    }
    let mut ids: Vec<Js> = vec![];
    for order in &values {
        if !order.get("marketId")?.strict_equals(&market) {
            return Err(Thrown::error(
                "Error",
                "Grouped orders must all use the same market id",
            ));
        }
        let id = order.get("orderId")?.call_method("toString", vec![])?;
        if ids.iter().any(|previous| previous.strict_equals(&id)) {
            return Err(Thrown::error(
                "Error",
                &format!(
                    "Grouped orders contain duplicate client order id {}",
                    id.to_display()
                ),
            ));
        }
        ids.push(id);
    }
    // Source reads all order types before inspecting groupingType. The native
    // builder performs the corresponding kind-specific validation after decode.
    for order in &values {
        order.get("orderType")?;
    }
    let grouping = params.get("groupingType")?;
    let grouping_type = match grouping.as_f64() {
        Some(1.0) => LighterGroupedOrderType::OneTriggersTheOther,
        Some(2.0) => LighterGroupedOrderType::OneCancelsTheOther,
        Some(3.0) => LighterGroupedOrderType::OneTriggersOco,
        _ => {
            return Err(Thrown::error(
                "Error",
                &format!(
                    "Unsupported Lighter grouped order type: {}",
                    grouping.to_display()
                ),
            ));
        }
    };
    let decoded = orders
        .array_values()
        .ok_or_else(|| mismatch("orders", "an array", &orders))?
        .iter()
        .map(create_order)
        .collect::<Result<Vec<_>, _>>()?;
    let (orders, flags): (Vec<_>, Vec<_>) = decoded.into_iter().unzip();
    Ok((
        CreateGroupedOrdersParams {
            grouping_type,
            orders,
            integrator_account_index: optional(params, "integratorAccountIndex", as_big)?,
            integrator_maker_fee: optional(params, "integratorMakerFee", as_number)?,
            integrator_taker_fee: optional(params, "integratorTakerFee", as_number)?,
        },
        flags,
    ))
}

// ts: CancelOrderParams
pub(super) fn cancel_order(params: &Js) -> Result<CancelOrderParams, Thrown> {
    Ok(CancelOrderParams {
        order_id: big(params, "orderId")?,
        market_id: number(params, "marketId")?,
    })
}

// ts: UpdateLeverageParams
pub(super) fn update_leverage(params: &Js) -> Result<UpdateLeverageParams, Thrown> {
    let leverage = params.get("leverage")?;
    let leverage = leverage
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or_else(|| {
            Thrown::error(
                "Error",
                &format!("Invalid leverage value: {}", leverage.to_display()),
            )
        })?;
    let market_id = params.get("marketId")?;
    let market_id = market_id
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.0 && (0.0..=254.0).contains(n))
        .ok_or_else(|| {
            Thrown::error(
                "Error",
                &format!(
                    "marketId is out of valid range (0-254): {}",
                    market_id.to_display()
                ),
            )
        })?;
    let mode = params.get("marginMode")?;
    let margin_mode = mode.as_f64().and_then(MarginMode::new).ok_or_else(|| {
        Thrown::error(
            "Error",
            &format!("marginMode must be 0 or 1. Received: {}", mode.to_display()),
        )
    })?;
    Ok(UpdateLeverageParams {
        market_id,
        leverage,
        margin_mode,
    })
}

/// `params.nonce ?? await getNextNonce()`: nullish means allocate.
pub(super) fn approve_nonce(params: &Js) -> Result<Option<BigInt>, Thrown> {
    match params.get("nonce")? {
        Js::Undefined | Js::Null => Ok(None),
        value => as_big("nonce", value).map(Some),
    }
}

// ts: ApproveIntegratorParams
pub(super) fn approve_integrator(
    params: &Js,
    nonce: Option<BigInt>,
) -> Result<(ApproveIntegratorParams, Js), Thrown> {
    let parsed = ApproveIntegratorParams {
        integrator_account_index: big(params, "integratorAccountIndex")?,
        max_perps_taker_fee: number(params, "maxPerpsTakerFee")?,
        max_perps_maker_fee: number(params, "maxPerpsMakerFee")?,
        max_spot_taker_fee: number(params, "maxSpotTakerFee")?,
        max_spot_maker_fee: number(params, "maxSpotMakerFee")?,
        approval_expiry: big(params, "approvalExpiry")?,
        nonce,
        l1_signature: None,
    };
    Ok((parsed, params.get("l1Signature")?))
}

pub(super) fn deadline(value: &Js) -> Result<BigInt, Thrown> {
    as_big("deadline", value.clone())
}

// ts: SendTxBatchResponse
pub(super) fn send_tx_batch_response(value: &Js) -> Result<SendTxBatchResponse, Thrown> {
    let hashes = value.get("txHashes")?;
    let tx_hashes = hashes
        .array_values()
        .ok_or_else(|| mismatch("txHashes", "an array", &hashes))?
        .into_iter()
        .map(|hash| as_text("txHashes[]", hash))
        .collect::<Result<_, _>>()?;
    let volume_quota_remaining = optional(value, "volumeQuotaRemaining", |key, v| match v {
        Js::Number(n) => Ok(VolumeQuotaRemaining::Number(n)),
        Js::String(s) => Ok(VolumeQuotaRemaining::String(s.to_vec())),
        other => Err(mismatch(key, "a number or string", &other)),
    })?;
    Ok(SendTxBatchResponse {
        tx_hashes: shared(tx_hashes),
        predicted_execution_time_ms: optional(value, "predictedExecutionTimeMs", as_number)?,
        volume_quota_remaining,
    })
}

fn bigint(value: &BigInt) -> Js {
    Js::BigInt(Rc::new(value.clone()))
}
fn opt<T>(value: Option<T>, encode: impl FnOnce(T) -> Js) -> Js {
    value.map_or(Js::Undefined, encode)
}

fn header(kind: TransactionKind, h: &TransactionHeader, details: (&str, Js)) -> Js {
    let mut fields = vec![
        ("type", Js::str(kind.as_str())),
        ("data", Js::utf16(&h.data)),
        ("signature", Js::utf16(&h.signature)),
        ("accountIndex", bigint(&h.account_index)),
        ("apiKeyIndex", Js::Number(h.api_key_index)),
        ("nonce", bigint(&h.nonce)),
    ];
    if let Some(expired_at) = &h.expired_at {
        fields.push(("expiredAt", bigint(expired_at)));
    }
    fields.push(details);
    Js::object(fields)
}

// ts: L2CreateOrderTxInfo
pub(super) fn create_order_tx(tx: &L2CreateOrderTxInfo, flags: &OrderFlagValues) -> Js {
    let d = &tx.order_details;
    let details = Js::object(vec![
        ("orderId", bigint(&d.order_id)),
        ("marketId", Js::Number(d.market_id)),
        ("isBuy", flags.is_buy.clone()),
        ("price", bigint(&d.price)),
        ("amount", bigint(&d.amount)),
        ("postOnly", flags.post_only.clone()),
        ("reduceOnly", flags.reduce_only.clone()),
        ("orderTypeValue", opt(d.order_type_value, Js::Number)),
        ("timeInForceValue", opt(d.time_in_force_value, Js::Number)),
        ("triggerPrice", opt(d.trigger_price.as_ref(), bigint)),
        ("orderExpiry", opt(d.order_expiry.as_ref(), bigint)),
        (
            "integratorAccountIndex",
            opt(d.integrator_account_index.as_ref(), bigint),
        ),
        (
            "integratorMakerFee",
            opt(d.integrator_maker_fee, Js::Number),
        ),
        (
            "integratorTakerFee",
            opt(d.integrator_taker_fee, Js::Number),
        ),
    ]);
    header(
        TransactionKind::CreateOrder,
        &tx.header,
        ("orderDetails", details),
    )
}

fn grouped_order(d: &LighterGroupedOrderDetails, flags: &OrderFlagValues) -> Js {
    Js::object(vec![
        ("orderId", bigint(&d.order_id)),
        ("marketId", Js::Number(d.market_id)),
        ("isBuy", flags.is_buy.clone()),
        ("price", bigint(&d.price)),
        ("amount", bigint(&d.amount)),
        ("reduceOnly", flags.reduce_only.clone()),
        ("orderTypeValue", opt(d.order_type_value, Js::Number)),
        ("timeInForceValue", opt(d.time_in_force_value, Js::Number)),
        ("triggerPrice", opt(d.trigger_price.as_ref(), bigint)),
        ("orderExpiry", opt(d.order_expiry.as_ref(), bigint)),
    ])
}

// ts: L2CreateGroupedOrdersTxInfo
pub(super) fn create_grouped_orders_tx(
    tx: &L2CreateGroupedOrdersTxInfo,
    flags: &[OrderFlagValues],
) -> Js {
    let d = &tx.grouped_order_details;
    assert_eq!(d.orders.len(), flags.len());
    let details = Js::object(vec![
        ("groupingType", Js::Number(d.grouping_type.value())),
        (
            "orders",
            Js::array(
                d.orders
                    .iter()
                    .zip(flags)
                    .map(|(d, flags)| grouped_order(d, flags))
                    .collect(),
            ),
        ),
        (
            "integratorAccountIndex",
            opt(d.integrator_account_index.as_ref(), bigint),
        ),
        (
            "integratorMakerFee",
            opt(d.integrator_maker_fee, Js::Number),
        ),
        (
            "integratorTakerFee",
            opt(d.integrator_taker_fee, Js::Number),
        ),
    ]);
    header(
        TransactionKind::CreateGroupedOrders,
        &tx.header,
        ("groupedOrderDetails", details),
    )
}

// ts: L2CancelOrderTxInfo
pub(super) fn cancel_order_tx(tx: &L2CancelOrderTxInfo) -> Js {
    let d = &tx.cancel_details;
    let details = Js::object(vec![
        ("orderId", bigint(&d.order_id)),
        ("marketId", Js::Number(d.market_id)),
        ("expiredAt", opt(d.expired_at.as_ref(), bigint)),
    ]);
    header(
        TransactionKind::CancelOrder,
        &tx.header,
        ("cancelDetails", details),
    )
}

// ts: L2TransferTxInfo
pub(super) fn transfer_tx(tx: &L2TransferTxInfo) -> Js {
    let d = &tx.transfer_details;
    let details = Js::object(vec![
        ("toAccountIndex", bigint(&d.to_account_index)),
        ("amount", bigint(&d.amount)),
        ("tokenId", Js::Number(d.token_id)),
    ]);
    header(
        TransactionKind::Transfer,
        &tx.header,
        ("transferDetails", details),
    )
}

// ts: L2UpdateLeverageTxInfo
pub(super) fn update_leverage_tx(tx: &L2UpdateLeverageTxInfo) -> Js {
    let d = &tx.leverage_details;
    let details = Js::object(vec![
        ("marketId", Js::Number(d.market_id)),
        (
            "initialMarginFraction",
            Js::Number(d.initial_margin_fraction),
        ),
        ("marginMode", Js::Number(d.margin_mode.value())),
        ("expiredAt", opt(d.expired_at.as_ref(), bigint)),
    ]);
    header(
        TransactionKind::UpdateLeverage,
        &tx.header,
        ("leverageDetails", details),
    )
}

// ts: L2ApproveIntegratorTxInfo
pub(super) fn approve_integrator_tx(tx: &L2ApproveIntegratorTxInfo, l1_signature: &Js) -> Js {
    let d = &tx.approve_integrator_details;
    let details = Js::object(vec![
        (
            "integratorAccountIndex",
            bigint(&d.integrator_account_index),
        ),
        ("maxPerpsTakerFee", Js::Number(d.max_perps_taker_fee)),
        ("maxPerpsMakerFee", Js::Number(d.max_perps_maker_fee)),
        ("maxSpotTakerFee", Js::Number(d.max_spot_taker_fee)),
        ("maxSpotMakerFee", Js::Number(d.max_spot_maker_fee)),
        ("approvalExpiry", bigint(&d.approval_expiry)),
        ("expiredAt", opt(d.expired_at.as_ref(), bigint)),
        ("l1Signature", l1_signature.clone()),
    ]);
    header(
        TransactionKind::ApproveIntegrator,
        &tx.header,
        ("approveIntegratorDetails", details),
    )
}

pub(super) fn transaction_ref(tx: &TransactionRef) -> Js {
    match tx {
        TransactionRef::CreateOrder(v) => {
            let tx = v.borrow();
            create_order_tx(
                &tx,
                &OrderFlagValues {
                    is_buy: Js::Bool(tx.order_details.is_buy),
                    post_only: opt(tx.order_details.post_only, Js::Bool),
                    reduce_only: opt(tx.order_details.reduce_only, Js::Bool),
                },
            )
        }
        TransactionRef::CreateGroupedOrders(v) => {
            let tx = v.borrow();
            let flags = tx
                .grouped_order_details
                .orders
                .iter()
                .map(|d| OrderFlagValues {
                    is_buy: Js::Bool(d.is_buy),
                    post_only: Js::Undefined,
                    reduce_only: opt(d.reduce_only, Js::Bool),
                })
                .collect::<Vec<_>>();
            create_grouped_orders_tx(&tx, &flags)
        }
        TransactionRef::CancelOrder(v) => cancel_order_tx(&v.borrow()),
        TransactionRef::Transfer(v) => transfer_tx(&v.borrow()),
        TransactionRef::UpdateLeverage(v) => update_leverage_tx(&v.borrow()),
        TransactionRef::ApproveIntegrator(v) => {
            let tx = v.borrow();
            approve_integrator_tx(
                &tx,
                &opt(
                    tx.approve_integrator_details.l1_signature.as_deref(),
                    Js::utf16,
                ),
            )
        }
    }
}

pub(super) fn create_order_txs(
    txs: &[Shared<L2CreateOrderTxInfo>],
    flags: &[OrderFlagValues],
) -> Js {
    assert_eq!(txs.len(), flags.len());
    Js::array(
        txs.iter()
            .zip(flags)
            .map(|(tx, flags)| create_order_tx(&tx.borrow(), flags))
            .collect(),
    )
}
