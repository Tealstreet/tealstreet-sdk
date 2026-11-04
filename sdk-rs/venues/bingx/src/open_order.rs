//! REST display recovery; execution frames retain the strict NativeOrder decoder.
use super::{OrderMeta, map_order, observation};
use crate::account_wire::{NativeOrder, boolean, identifier};
use safe_cex_engine::{Cx, model::Order};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{collections::HashMap, fmt};

#[derive(Default)]
struct Fields<'a>(HashMap<&'a str, &'a RawValue>);
impl<'de> Deserialize<'de> for Fields<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = Fields<'de>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an order object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields = Fields::default();
                while let Some((key, value)) = map.next_entry::<&str, &RawValue>()? {
                    if fields.0.insert(key, value).is_some() {
                        return Err(serde::de::Error::custom("duplicate order field"));
                    }
                }
                Ok(fields)
            }
        }
        d.deserialize_map(Object)
    }
}

pub struct Issue {
    pub id: String,
    pub fields: Vec<&'static str>,
    pub displayed: bool,
}

pub struct Parsed {
    pub order: Order,
    pub metadata: Option<OrderMeta>,
    pub fields: Vec<&'static str>,
}

pub fn log_issues(
    cx: &mut Cx<'_, crate::Pending>,
    logged: &mut HashMap<String, bool>,
    issues: &[Issue],
    kept_ids: &[String],
) {
    logged.retain(|key, _| {
        key.strip_prefix("open order:")
            .is_none_or(|id| issues.iter().any(|issue| issue.id == id))
    });
    for issue in issues {
        if logged.len() >= super::SKIP_LOG_LIMIT {
            logged.clear();
        }
        let key = format!("open order:{}", issue.id);
        let kept = issue.displayed
            || kept_ids
                .iter()
                .any(|id| issue.id.is_empty() || id == &issue.id);
        if logged.get(&key).is_some_and(|&errored| errored || kept) {
            continue;
        }
        logged.insert(key, !kept);
        let identity = if issue.id.is_empty() {
            "without an id"
        } else {
            &issue.id
        };
        let action = if issue.displayed {
            "retained open order"
        } else {
            "skipped malformed open order"
        };
        cx.notice(
            if kept {
                safe_cex_engine::Level::Warn
            } else {
                safe_cex_engine::Level::Error
            },
            safe_cex_engine::log::code::ROW_SKIPPED,
            format!(
                "BingX {action} row {identity} (malformed fields: {})",
                issue.fields.join(", ")
            ),
        );
    }
}

fn decode(fields: &HashMap<&str, &RawValue>, cx: &Cx<'_, crate::Pending>) -> Option<Order> {
    let text = serde_json::to_string(fields).ok()?;
    let native: NativeOrder<'_> = serde_json::from_str(&text).ok()?;
    map_order(&native, cx)
}

pub fn parse(item: &str, cx: &Cx<'_, crate::Pending>) -> Result<Parsed, Vec<&'static str>> {
    if let Ok(native) = serde_json::from_str::<NativeOrder<'_>>(item)
        && let Some(order) = map_order(&native, cx)
    {
        let meta = native
            .complete_metadata()
            .then(|| observation(order.clone(), &native));
        return Ok(Parsed {
            order,
            metadata: meta,
            fields: Vec::new(),
        });
    }
    let fields: Fields<'_> = serde_json::from_str(item).map_err(|_| vec!["row"])?;
    #[derive(Deserialize)]
    struct Id<'a>(#[serde(borrow, deserialize_with = "identifier")] crate::wire::Str<'a>);
    #[derive(Deserialize)]
    struct Qty(
        #[serde(deserialize_with = "safe_cex_engine::de::optional_number_or_blank")] Option<f64>,
    );
    #[derive(Deserialize)]
    struct Bool(#[serde(deserialize_with = "boolean")] bool);
    let mut bad = Vec::new();
    let mut core = HashMap::new();
    let mut identity = None;
    for key in ["orderId", "orderID", "i"] {
        if let Some(value) = fields.0.get(key) {
            match serde_json::from_str::<Id<'_>>(value.get()) {
                Ok(Id(id)) if id.is_empty() => (),
                Ok(Id(id)) if !id.is_empty() && identity.as_ref().is_none_or(|old| old == &id) => {
                    identity = Some(id)
                }
                _ => bad.push("orderId"),
            }
            core.insert(key, *value);
        }
    }
    if identity.is_none() {
        bad.push("orderId");
    }
    for names in [["symbol", "s"], ["side", "S"]] {
        let values: Vec<_> = names
            .into_iter()
            .filter_map(|name| fields.0.get(name).map(|value| (name, *value)))
            .collect();
        let valid = values.len() == 1
            && serde_json::from_str::<String>(values[0].1.get()).is_ok_and(|s| {
                if names[0] == "side" {
                    matches!(s.as_str(), "BUY" | "SELL")
                } else {
                    !s.is_empty()
                }
            });
        if !valid {
            bad.push(names[0]);
        }
        core.extend(values);
    }
    let quantities: Vec<_> = ["quantity", "origQty", "q"]
        .into_iter()
        .filter_map(|name| fields.0.get(name).map(|value| (name, *value)))
        .collect();
    let closes: Vec<_> = ["closePosition", "cp"]
        .into_iter()
        .filter_map(|name| fields.0.get(name))
        .collect();
    let full_position =
        closes.len() == 1 && serde_json::from_str::<Bool>(closes[0].get()).is_ok_and(|v| v.0);
    let quantity = (quantities.len() == 1)
        .then(|| serde_json::from_str::<Qty>(quantities[0].1.get()))
        .transpose();
    match quantity {
        Ok(Some(Qty(Some(q)))) if q >= 0.0 => core.extend(quantities),
        // Preserve the existing full-position marker, never a malformed quantity.
        Ok(None | Some(Qty(None))) if full_position && quantities.len() <= 1 => {
            core.insert("quantity", serde_json::from_str::<&RawValue>("0").unwrap());
        }
        _ => bad.push("origQty"),
    }
    if !bad.is_empty() {
        bad.sort_unstable();
        bad.dedup();
        return Err(bad);
    }
    core.insert(
        "type",
        serde_json::from_str::<&RawValue>(r#""LIMIT""#).unwrap(),
    );
    let mut has_type = false;
    for aliases in [
        &["type", "o"][..],
        &["status", "X"],
        &["price", "p"],
        &["stopPrice", "sp"],
        &["filled", "executedQty", "z"],
        &["lastQuantity", "l"],
        &["lastPrice", "L"],
        &["avgPrice", "ap"],
        &["reduceOnly", "R", "ro"],
        &["closePosition", "cp"],
        &["postOnly"],
        &["clientOrderId", "c"],
        &["timeInForce", "f"],
        &["workingType", "wt"],
        &["positionSide", "ps"],
        &["tradeId", "t"],
        &["time", "T"],
        &["takeProfit"],
        &["stopLoss"],
    ] {
        let present: Vec<_> = aliases
            .iter()
            .filter_map(|&name| fields.0.get(name).map(|value| (name, *value)))
            .collect();
        if present.is_empty() {
            continue;
        }
        let mut candidate = core.clone();
        for key in aliases {
            candidate.remove(key);
        }
        candidate.extend(present.iter().copied());
        if present.len() == 1 && decode(&candidate, cx).is_some() {
            has_type |= aliases[0] == "type";
            core = candidate;
        } else {
            bad.extend(present.iter().map(|(name, _)| *name));
        }
    }
    if !has_type && !bad.contains(&"type") {
        bad.push("type");
    }
    let mut order = decode(&core, cx).ok_or_else(|| vec!["row"])?;
    if !has_type {
        order.kind = safe_cex_engine::model::OrderType::Conditional;
    }
    if let Some(old) = cx.store.order(&order.id)
        && bad
            .iter()
            .any(|key| matches!(*key, "filled" | "executedQty" | "z"))
    {
        order.filled = old.filled;
        order.remaining = (order.amount - order.filled).max(0.0);
    }
    // A display-only row must invalidate any previous amendment observation.
    let meta = Some((
        order.clone(),
        String::new(),
        String::new(),
        false,
        String::new(),
    ));
    Ok(Parsed {
        order,
        metadata: meta,
        fields: bad,
    })
}
