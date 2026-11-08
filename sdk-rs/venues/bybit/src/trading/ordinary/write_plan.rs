//! Owned exact mutation payloads, effect targets and native rule snapshots.
use super::{Leg, fields, not_sent};
use crate::trading::{BATCH_LIMIT, Call, LimitPrice, validate_limit_price};
use crate::{BybitLinear, Category, Pending};
use safe_cex_engine::model::Precision;
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, PlannedWrite, WritePlan};
use safe_cex_engine::{Cx, VenueError, num};
use std::collections::VecDeque;

pub(super) struct NativeWrite {
    pub body: String,
    rows: Vec<CreateRow>,
    category: Category,
    pub effects: Vec<EffectTarget>,
    controls: Vec<(f64, Option<f64>)>,
}

#[derive(Clone)]
pub(super) struct EffectTarget {
    effect: AppliedEffect,
    target_order_id: Option<String>,
    position_key: Option<String>,
}

impl EffectTarget {
    pub fn applied(&self) -> ElementResult {
        ElementResult::Applied {
            effect: self.effect,
            target_order_id: self.target_order_id.clone(),
            position_key: self.position_key.clone(),
        }
    }
}

fn effects(leg: &Leg, value: &serde_json::Value) -> Result<Vec<EffectTarget>, VenueError> {
    let target = |effect, id: String| EffectTarget {
        effect,
        target_order_id: Some(id),
        position_key: None,
    };
    let scoped = |effect| {
        let fields: Vec<_> = [
            "category",
            "symbol",
            "coin",
            "settleCoin",
            "positionIdx",
            "orderFilter",
        ]
        .into_iter()
        .filter_map(|name| value.get(name).map(|value| (name, value)))
        .collect();
        EffectTarget {
            effect,
            target_order_id: None,
            position_key: Some(format!(
                "{}:{}",
                leg.planned.path,
                serde_json::json!(fields)
            )),
        }
    };
    let amend = || {
        value
            .get("orderId")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty())
            .map(|id| vec![target(AppliedEffect::Amend, id.to_owned())])
            .ok_or_else(|| not_sent("Bybit planned amend is missing its native target"))
    };
    Ok(match &leg.planned.call {
        Call::PlaceOne | Call::PlaceBatch => Vec::new(),
        Call::Cancel { ids } => ids
            .iter()
            .cloned()
            .map(|id| target(AppliedEffect::Cancel, id))
            .collect(),
        Call::CancelPartialStop { id, .. } => vec![target(AppliedEffect::Cancel, id.clone())],
        Call::CancelAll { .. } => vec![scoped(AppliedEffect::Cancel)],
        Call::Amend | Call::ClearAttachedLeg { .. } => return amend(),
        Call::TradingStop | Call::PartialStop { .. } => vec![scoped(AppliedEffect::TradingStop)],
        Call::Leverage { .. } | Call::MarginMode { .. } | Call::PositionMode { .. } => {
            vec![scoped(AppliedEffect::Settings)]
        }
        Call::PartialProof(_) => return Err(not_sent("Read-only proof is not a mutation")),
    })
}

struct CreateRow {
    wire: WireRow,
    precision: Precision,
    min: f64,
    max: f64,
    min_notional: Option<f64>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRow {
    symbol: String,
    order_type: String,
    qty: String,
    price: Option<String>,
    trigger_price: Option<String>,
    stop_loss: Option<String>,
    take_profit: Option<String>,
    bbo_side_type: Option<String>,
    order_iv: Option<String>,
    #[serde(default)]
    reduce_only: bool,
}

pub(super) fn build(
    venue: &BybitLinear,
    cx: &Cx<'_, Pending>,
    legs: &VecDeque<Leg>,
) -> Result<Option<WritePlan<NativeWrite>>, VenueError> {
    let mut writes = Vec::new();
    let mut index = 0;
    for leg in legs {
        if matches!(leg.planned.call, Call::PartialProof(_)) {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(&leg.planned.body)
            .map_err(|_| not_sent("Malformed Bybit planned create payload"))?;
        let effects = effects(leg, &value)?;
        let category = match value.get("category").and_then(serde_json::Value::as_str) {
            Some("spot") => Category::Spot,
            Some("linear") => Category::Linear,
            Some("inverse") => Category::Inverse,
            Some("option") => Category::Option,
            _ if !effects.is_empty() => venue.category,
            _ => return Err(not_sent("Missing Bybit planned create category")),
        };
        let mut controls = Vec::new();
        if !effects.is_empty() {
            let market = value
                .get("symbol")
                .and_then(serde_json::Value::as_str)
                .and_then(|symbol| cx.store.market(symbol));
            for field in [
                "price",
                "triggerPrice",
                "stopLoss",
                "takeProfit",
                "trailingStop",
                "activePrice",
                "qty",
                "slSize",
                "tpSize",
                "buyLeverage",
                "sellLeverage",
                "orderIv",
            ] {
                if let Some(text) = value.get(field).and_then(serde_json::Value::as_str) {
                    let parsed = text
                        .parse::<f64>()
                        .map_err(|_| not_sent("Malformed Bybit planned mutation value"))?;
                    let precision = market.and_then(|market| match field {
                        "qty" | "slSize" | "tpSize" => Some(market.precision.amount),
                        "buyLeverage" | "sellLeverage" | "orderIv" => None,
                        _ => Some(market.precision.price),
                    });
                    controls.push((parsed, precision));
                }
            }
        }
        let values = match &leg.planned.call {
            Call::PlaceBatch => value
                .get("request")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .ok_or_else(|| not_sent("Missing Bybit planned create rows"))?,
            Call::PlaceOne => vec![value],
            _ => Vec::new(),
        };
        let mut rows = Vec::new();
        for value in values {
            let wire: WireRow = serde_json::from_value(value)
                .map_err(|_| not_sent("Malformed Bybit planned create row"))?;
            let market = cx
                .store
                .market(&wire.symbol)
                .ok_or_else(|| not_sent("Missing Bybit planned create market"))?;
            let max = if category == Category::Option {
                market.limits.amount.max
            } else {
                venue
                    .order_caps
                    .get(&wire.symbol)
                    .map_or(market.limits.amount.max, |caps| {
                        caps.for_order(market.limits.amount.max, wire.order_type == "Market")
                    })
            };
            let min_notional = venue.min_notionals.get(&wire.symbol).copied();
            rows.push(CreateRow {
                wire,
                precision: market.precision,
                min: market.limits.amount.min,
                max,
                min_notional,
            });
        }
        let own = fields(&leg.planned.body);
        let links = match &leg.planned.call {
            Call::PlaceBatch => own.items.into_iter().map(|row| row.order_link_id).collect(),
            Call::PlaceOne => vec![own.order_link_id],
            _ => vec![None; effects.len()],
        };
        let elements = links
            .into_iter()
            .map(|client_order_id| {
                let element = Element {
                    index,
                    client_order_id,
                };
                index += 1;
                element
            })
            .collect();
        writes.push(PlannedWrite {
            payload: NativeWrite {
                body: leg.planned.body.clone(),
                rows,
                category,
                effects,
                controls,
            },
            elements,
        });
    }
    if writes.is_empty() {
        return Ok(None);
    }
    WritePlan::validate(writes, validate).map(Some)
}

fn positive(text: &str) -> Result<f64, VenueError> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| not_sent("Bybit planned wire value must be finite and positive"))
}

fn validate(write: &NativeWrite) -> Result<(), VenueError> {
    if write.controls.iter().any(|(value, precision)| {
        !value.is_finite()
            || *value < 0.0
            || precision.is_some_and(|step| {
                !step.is_finite() || step <= 0.0 || num::adjust(*value, step) != *value
            })
    }) {
        return Err(not_sent(
            "Bybit planned mutation is outside native precision",
        ));
    }
    if !write.effects.is_empty() {
        if write.effects.iter().any(|target| {
            target
                .target_order_id
                .as_ref()
                .is_some_and(String::is_empty)
        }) {
            return Err(not_sent("Bybit planned effect target is empty"));
        }
        return Ok(());
    }
    if write.rows.is_empty() || write.rows.len() > BATCH_LIMIT {
        return Err(not_sent(
            "Bybit planned create count exceeds the native limit",
        ));
    }
    for row in &write.rows {
        let qty = positive(&row.wire.qty)?;
        if !row.precision.is_tradable()
            || !row.max.is_finite()
            || row.max <= 0.0
            || qty < row.min
            || qty > row.max
            || num::adjust(qty, row.precision.amount) != qty
        {
            return Err(not_sent(
                "Bybit planned quantity is outside native limits or precision",
            ));
        }
        let price = row.wire.price.as_deref().map(positive).transpose()?;
        let iv = row.wire.order_iv.as_deref().map(positive).transpose()?;
        if let Some(minimum) = row.min_notional
            && (!minimum.is_finite()
                || minimum < 0.0
                || (!row.wire.reduce_only
                    && price.is_some_and(|price| {
                        let notional = if write.category == Category::Inverse {
                            qty
                        } else {
                            qty * price
                        };
                        !notional.is_finite() || notional < minimum
                    })))
        {
            return Err(not_sent(
                "Bybit planned order is below the native minimum notional",
            ));
        }
        if row.wire.order_type == "Limit" {
            validate_limit_price(&LimitPrice {
                price,
                bbo: row.wire.bbo_side_type.is_some()
                    || (write.category == Category::Option && iv.is_some()),
            })?;
        }
        for text in [
            &row.wire.price,
            &row.wire.trigger_price,
            &row.wire.stop_loss,
            &row.wire.take_profit,
        ]
        .into_iter()
        .flatten()
        {
            let price = positive(text)?;
            if num::adjust(price, row.precision.price) != price {
                return Err(not_sent("Bybit planned price is outside native precision"));
            }
        }
    }
    Ok(())
}
