//! Owned exact create payloads and immutable native rule snapshots.
use super::{Leg, fields, not_sent};
use crate::trading::{BATCH_LIMIT, Call, LimitPrice, validate_limit_price};
use crate::{BybitLinear, Category, Pending};
use safe_cex_engine::model::Precision;
use safe_cex_engine::write_plan::{Element, PlannedWrite, WritePlan};
use safe_cex_engine::{Cx, VenueError, num};
use std::collections::VecDeque;

pub(super) struct CreateWrite {
    pub body: String,
    rows: Vec<CreateRow>,
    category: Category,
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
) -> Result<Option<WritePlan<CreateWrite>>, VenueError> {
    let mut writes = Vec::new();
    let mut index = 0;
    for leg in legs {
        if !matches!(leg.planned.call, Call::PlaceOne | Call::PlaceBatch) {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(&leg.planned.body)
            .map_err(|_| not_sent("Malformed Bybit planned create payload"))?;
        let category = match value.get("category").and_then(serde_json::Value::as_str) {
            Some("spot") => Category::Spot,
            Some("linear") => Category::Linear,
            Some("inverse") => Category::Inverse,
            Some("option") => Category::Option,
            _ => return Err(not_sent("Missing Bybit planned create category")),
        };
        let values = match &leg.planned.call {
            Call::PlaceBatch => value
                .get("request")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .ok_or_else(|| not_sent("Missing Bybit planned create rows"))?,
            _ => vec![value],
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
            _ => vec![own.order_link_id],
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
            payload: CreateWrite {
                body: leg.planned.body.clone(),
                rows,
                category,
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

fn validate(write: &CreateWrite) -> Result<(), VenueError> {
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
