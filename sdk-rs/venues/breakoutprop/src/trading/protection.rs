use super::*;
use safe_cex_engine::model::PositionSide;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct Intent {
    pub original: Order,
    pub price: f64,
    pub amount: Option<f64>,
}
#[derive(Clone)]
pub(crate) struct Edit {
    pub intent: Intent,
    pub body: String,
    pub etag: String,
    pub native_id: String,
    pub below: bool,
    pub position: Option<String>,
}
#[derive(Clone)]
pub(crate) struct Bbo {
    pub bid: (f64, f64),
    pub ask: (f64, f64),
}
impl Bbo {
    fn check(&self, now: f64, price: f64, below: bool) -> Result<(), String> {
        let (value, at) = if below { self.bid } else { self.ask };
        if !now.is_finite()
            || !(0.0..=5000.).contains(&(now - at))
            || !value.is_finite()
            || value <= 0.
            || !self.bid.0.is_finite()
            || !self.ask.0.is_finite()
            || self.bid.0 > self.ask.0
        {
            return Err("Breakout Prop reference stale; protection not changed".into());
        }
        if if below {
            price >= value
        } else {
            price <= value
        } {
            return Err(
                "Breakout Prop replacement trigger is crossed; protection not changed".into(),
            );
        }
        Ok(())
    }
}
#[derive(Deserialize)]
struct List<'a> {
    #[serde(borrow)]
    orders: Vec<&'a serde_json::value::RawValue>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Identity<'a> {
    #[serde(borrow)]
    order_code: Cow<'a, str>,
    #[serde(default, borrow)]
    client_order_id: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Native {
    account: String,
    order_id: NativeId,
    order_code: String,
    client_order_id: String,
    version: u64,
    action_code: String,
    leg_count: u8,
    #[serde(rename = "type")]
    kind: String,
    instrument: String,
    status: String,
    final_status: bool,
    side: String,
    tif: String,
    limit_price: Option<f64>,
    stop_price: Option<f64>,
    price_offset: Option<f64>,
    price_link: Option<String>,
    expire_date: Option<String>,
    metadata: Option<BTreeMap<String, String>>,
    #[serde(default)]
    links: Vec<serde::de::IgnoredAny>,
    legs: Vec<Leg>,
    issue_time: String,
    transaction_time: String,
    margin_rate: Option<f64>,
    #[serde(default)]
    executions: Vec<serde::de::IgnoredAny>,
    #[serde(default)]
    cash_transactions: Vec<serde::de::IgnoredAny>,
    hedged_order_id: Option<serde::de::IgnoredAny>,
    external_order_id: Option<serde::de::IgnoredAny>,
    #[serde(rename = "audit")]
    _audit: Option<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Leg {
    instrument: String,
    side: Option<String>,
    position_effect: String,
    position_code: Option<String>,
    quantity: Option<f64>,
    filled_quantity: f64,
    remaining_quantity: f64,
    price: Option<f64>,
    average_price: Option<f64>,
    leg_ratio: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Modify<'a> {
    order_code: &'a str,
    instrument: &'a str,
    side: &'a str,
    position_effect: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit_price: Option<f64>,
    tif: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expire_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<&'a BTreeMap<String, String>>,
}
fn etag(response: &safe_cex_engine::Response<'_>) -> Result<String, String> {
    let mut values = response.headers.lines().filter_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("etag")
            .then_some(value.trim())
    });
    let value = values
        .next()
        .ok_or("Breakout Prop native ETag missing; protection not changed")?;
    if values.next().is_some()
        || value.len() < 2
        || value.len() > 512
        || !value.starts_with('"')
        || !value.ends_with('"')
        || !value.bytes().all(|b| (0x21..=0x7e).contains(&b))
        || value.as_bytes()[1..value.len() - 1].contains(&b'"')
    {
        return Err("Breakout Prop native ETag is unproven; protection not changed".into());
    }
    Ok(value.into())
}
impl Edit {
    pub fn inspect(
        intent: Intent,
        response: &safe_cex_engine::Response<'_>,
        account: &str,
    ) -> Result<Self, String> {
        let etag = etag(response)?;
        let list: List<'_> = serde_json::from_str(response.body)
            .map_err(|_| "Breakout Prop invalid native orders; protection not changed")?;
        if list.orders.len() > 10000 {
            return Err("Breakout Prop native order inspection exceeds limit".into());
        }
        let mut found = None;
        for value in list.orders {
            let id: Identity<'_> = serde_json::from_str(value.get())
                .map_err(|_| "Breakout Prop ambiguous native identity")?;
            if id.order_code == intent.original.id
                || id.client_order_id.as_deref() == Some(intent.original.id.as_str())
                || intent
                    .original
                    .client_order_id
                    .as_deref()
                    .is_some_and(|c| c == id.order_code || Some(c) == id.client_order_id.as_deref())
            {
                if found.is_some() {
                    return Err(
                        "Breakout Prop duplicate native order; protection not changed".into(),
                    );
                }
                found = Some(value);
            }
        }
        let raw = found.ok_or("Breakout Prop original native protection missing")?;
        let raw: Native = serde_json::from_str(raw.get()).map_err(|_| {
            "Breakout Prop complete standalone native provenance unavailable; protection not changed"
        })?;
        let original = &intent.original;
        let side = if original.side == OrderSide::Buy {
            "BUY"
        } else {
            "SELL"
        };
        let effect = if original.reduce_only {
            "CLOSE"
        } else {
            "OPEN"
        };
        if raw.account != account
            || raw.order_code != original.id
            || original
                .client_order_id
                .as_deref()
                .is_some_and(|id| id != raw.client_order_id)
            || raw.order_code.is_empty()
            || raw.action_code.is_empty()
            || raw.version == 0
            || raw.leg_count != 1
            || raw.legs.len() != 1
            || !raw.links.is_empty()
            || !raw.executions.is_empty()
            || !raw.cash_transactions.is_empty()
            || raw.hedged_order_id.is_some()
            || raw.external_order_id.is_some()
            || raw.price_offset.is_some()
            || raw.price_link.as_ref().is_some_and(|s| !s.is_empty())
            || raw.instrument != original.symbol
            || raw.side != side
            || raw.final_status
            || raw.status != "WORKING"
        {
            return Err(
                "Breakout Prop native order changed or grouped; protection not changed".into(),
            );
        }
        crate::wire::timestamp(&raw.issue_time)?;
        crate::wire::timestamp(&raw.transaction_time)?;
        if !matches!(raw.tif.as_str(), "GTC" | "DAY" | "GTD")
            || (raw.tif == "GTD") != raw.expire_date.is_some()
            || raw.margin_rate.is_some_and(|n| !n.is_finite() || n < 0.)
        {
            return Err("Breakout Prop native expiry or order controls unproven".into());
        }
        if let Some(date) = &raw.expire_date {
            crate::wire::timestamp(date)?;
        }
        let leg = &raw.legs[0];
        let qty = leg.quantity.unwrap_or(0.);
        if leg.instrument != raw.instrument
            || leg.side.as_deref().is_some_and(|s| s != side)
            || leg.position_effect != effect
            || leg.filled_quantity != 0.
            || leg.remaining_quantity != qty
            || qty != original.amount
            || !qty.is_finite()
            || qty < 0.
            || leg.average_price.is_some_and(|n| n != 0.)
            || leg.leg_ratio.is_some_and(|n| n != 1.)
            || leg.price.is_some_and(|n| !n.is_finite())
            || leg.position_code.as_ref().is_some_and(|s| s.is_empty())
            || (qty == 0. && (effect != "CLOSE" || leg.position_code.is_none()))
        {
            return Err(
                "Breakout Prop native order changed or filled; protection not changed".into(),
            );
        }
        let stop = original.kind == OrderType::StopLoss;
        if (stop
            && (raw.kind != "STOP"
                || raw.stop_price != Some(original.price)
                || raw.limit_price.is_some()))
            || (!stop
                && (raw.kind != "LIMIT"
                    || effect != "CLOSE"
                    || raw.limit_price != Some(original.price)
                    || raw.stop_price.is_some()))
        {
            return Err("Breakout Prop native protective class or price changed".into());
        }
        if leg.position_code.is_some() && intent.amount.is_some_and(|v| v != qty) {
            return Err(
                "Breakout Prop cannot resize position-linked protection; protection not changed"
                    .into(),
            );
        }
        let quantity = if intent.amount.is_some() {
            intent.amount
        } else {
            leg.quantity
        };
        let body = serde_json::to_string(&Modify {
            order_code: &raw.order_code,
            instrument: &raw.instrument,
            side,
            position_effect: effect,
            position_code: leg.position_code.as_deref(),
            quantity,
            stop_price: stop.then_some(intent.price),
            limit_price: (!stop).then_some(intent.price),
            tif: &raw.tif,
            expire_date: raw.expire_date.as_deref(),
            metadata: raw.metadata.as_ref(),
        })
        .map_err(|_| "Breakout Prop cannot encode native edit")?;
        let below = if stop {
            original.side == OrderSide::Sell
        } else {
            original.side == OrderSide::Buy
        };
        let position = leg.position_code.clone();
        Ok(Self {
            intent,
            body,
            etag,
            native_id: raw.order_id.text()?,
            below,
            position,
        })
    }
    // Every native amend send revalidates the snapshot and genuine BBO receipts.
    // Pure admission, including rate deferral and the single re-login replay.
    pub fn guard(&self, cx: &Cx<'_, Pending>, venue: &BreakoutProp) -> Result<(), String> {
        let o = &self.intent.original;
        if cx.store.order(&o.id) != Some(o) || o.status != OrderStatus::Open || o.filled != 0. {
            return Err(
                "Breakout Prop original protection changed or filled; protection not changed"
                    .into(),
            );
        }
        venue
            .bbo
            .get(&o.symbol)
            .ok_or("Breakout Prop reference stale; protection not changed")?
            .check(cx.now, self.intent.price, self.below)?;
        if o.reduce_only {
            let side = if o.side == OrderSide::Sell {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            if !cx.store.positions().iter().any(|p| {
                p.symbol == o.symbol
                    && p.side == side
                    && p.contracts.is_finite()
                    && p.contracts > 0.
                    && self.position.as_ref().is_none_or(|id| p.id == *id)
            }) {
                return Err(
                    "Breakout Prop protected position missing; protection not changed".into(),
                );
            }
        }
        Ok(())
    }
}
