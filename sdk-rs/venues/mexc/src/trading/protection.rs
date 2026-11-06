use super::*;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct References([Option<(f64, f64)>; 3]);
impl References {
    pub fn observe(&mut self, now: f64, values: [Option<f64>; 3]) {
        for (slot, value) in self.0.iter_mut().zip(values) {
            if let Some(value) = value {
                *slot =
                    (value.is_finite() && value > 0.0 && now.is_finite()).then_some((value, now));
            }
        }
    }
    fn get(&self, source: StopTriggerType, now: f64) -> Option<f64> {
        let (value, at) = self.0[match source {
            StopTriggerType::LastPrice => 0,
            StopTriggerType::MarkPrice => 1,
            StopTriggerType::IndexPrice => 2,
        }]?;
        (now.is_finite() && (0.0..=5_000.0).contains(&(now - at))).then_some(value)
    }
}
#[derive(Clone)]
pub struct Edit {
    pub observed: Order,
    pub scope: Scope,
    pub leg: Option<bool>,
    pub price: Option<f64>,
    pub amount: Option<f64>,
    pub native: String,
    pub replacement: Option<(String, String)>,
    pub restore: Option<(String, String)>,
    pub canceled: bool,
    pub cancel_proven: bool,
    pub restoring: bool,
    pub refusal: Option<String>,
    pub started: f64,
}
pub fn alias(id: &str) -> (&str, Option<bool>) {
    for (suffix, take) in [
        ("__take_profit", true),
        ("__stop_loss", false),
        ("__tp", true),
        ("__sl", false),
        ("_tp", true),
        ("_sl", false),
    ] {
        if let Some(parent) = id.strip_suffix(suffix) {
            return (parent, Some(take));
        }
    }
    (id, None)
}
pub fn protected(order: &Order) -> bool {
    matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
        || order.stop_loss.is_some()
        || order.take_profit.is_some()
}
fn reference(source: Option<f64>) -> Result<StopTriggerType, String> {
    match source {
        Some(1.) => Ok(StopTriggerType::LastPrice),
        Some(2.) => Ok(StopTriggerType::MarkPrice),
        Some(3.) => Ok(StopTriggerType::IndexPrice),
        _ => Err("MEXC reference stale: native trigger source is unknown; stop not changed".into()),
    }
}
fn check(
    cx: &Cx<'_, Pending>,
    refs: &HashMap<String, References>,
    order: &Order,
    take: bool,
    price: f64,
    source: Option<f64>,
    closing: bool,
) -> Result<(), String> {
    let value = refs
        .get(&order.symbol)
        .and_then(|r| r.get(reference(source).ok()?, cx.now))
        .ok_or_else(|| "MEXC reference stale; stop not changed".to_owned())?;
    let gte = (take && order.side == OrderSide::Sell) || (!take && order.side == OrderSide::Buy);
    if !price.is_finite() || price <= 0. || if gte { price <= value } else { price >= value } {
        return Err("MEXC replacement trigger is crossed; stop not changed".into());
    }
    if closing {
        let side = if order.side == OrderSide::Sell {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        if !cx.store.positions().iter().any(|p| {
            p.symbol == order.symbol
                && p.side == side
                && p.contracts.is_finite()
                && p.contracts > 0.
        }) {
            return Err("MEXC protected position is absent; stop not changed".into());
        }
    }
    Ok(())
}
// Every destructive amend send is checked here, including deferred/re-signed sends.
// Read-only: a refusal leaves the original stop and the FIFO claim unchanged.
pub fn guard(
    cx: &Cx<'_, Pending>,
    refs: &HashMap<String, References>,
    edit: &Edit,
    restore: bool,
) -> Result<(), String> {
    let original = &edit.observed;
    if !restore && cx.store.order(&original.id) != Some(original) {
        return Err("MEXC original order changed or filled; stop not changed".into());
    }
    if restore {
        let Some(current) = cx.store.order(&original.id) else {
            return Err("MEXC original protection disappeared during restore".into());
        };
        let mut normalized = current.clone();
        normalized.status = original.status;
        if current.status != OrderStatus::Canceled || normalized != *original {
            return Err("MEXC original protection changed or filled; restore withheld".into());
        }
    }
    if original.filled != 0. || original.status != OrderStatus::Open {
        return Err("MEXC original order changed or filled; stop not changed".into());
    }
    let raw: OrderRow<'_> =
        serde_json::from_str(&edit.native).map_err(|_| "MEXC native stop provenance missing")?;
    let new = if restore { None } else { edit.price };
    if edit.scope == Scope::Plan && edit.leg.is_none() {
        check(
            cx,
            refs,
            original,
            original.kind == OrderType::TakeProfit,
            new.unwrap_or(raw.trigger_price),
            raw.trend,
            raw.reduce_only || matches!(raw.side, 2. | 4.),
        )?;
    } else if edit.scope == Scope::Stop {
        for (take, old, source) in [
            (false, raw.stop_loss_price, raw.loss_trend.or(raw.trend)),
            (true, raw.take_profit_price, raw.profit_trend.or(raw.trend)),
        ] {
            let selected = edit.leg.unwrap_or(original.kind == OrderType::TakeProfit) == take;
            if let Some(price) = if selected { new.or(old) } else { old }.filter(|p| *p > 0.) {
                check(cx, refs, original, take, price, source, true)?;
            }
        }
    } else {
        for (take, old, source) in [
            (false, raw.stop_loss_price, raw.loss_trend),
            (true, raw.take_profit_price, raw.profit_trend),
        ] {
            let price = if edit.leg == Some(take) {
                new.or(old)
            } else {
                old
            };
            if let Some(price) = price.filter(|p| *p > 0.) {
                let mut closing = original.clone();
                // Attached brackets protect the side opened by their parent.
                closing.side = if original.side == OrderSide::Buy {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                check(
                    cx,
                    refs,
                    &closing,
                    take,
                    price,
                    source,
                    original.reduce_only,
                )?;
            }
        }
        if edit.scope == Scope::Plan {
            check(
                cx,
                refs,
                original,
                original.kind == OrderType::TakeProfit,
                raw.trigger_price,
                raw.trend,
                original.reduce_only,
            )?;
        }
    }
    Ok(())
}
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Native<'a> {
    #[serde(default, deserialize_with = "optional_number")]
    leverage: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    deal_vol: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    open_type: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    execute_cycle: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    position_mode: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    price_protect: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    take_profit_reverse: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    stop_loss_reverse: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    take_profit_type: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    stop_loss_type: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    take_profit_order_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    stop_loss_order_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    take_profit_vol: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    stop_loss_vol: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    vol_type: Option<f64>,
    #[serde(default, borrow, deserialize_with = "id")]
    position_id: Str<'a>,
    #[serde(default, borrow)]
    profit_loss_vol_type: Str<'a>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Pair<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_plan_order_id: Option<&'a str>,
    stop_loss_price: String,
    take_profit_price: String,
    loss_trend: u8,
    profit_trend: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_reverse: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_reverse: Option<f64>,
}
fn source(n: Option<f64>, price: f64) -> Result<u8, String> {
    if n.is_some() {
        return Ok(trend(Some(reference(n)?)));
    }
    if price <= 0. {
        Ok(1)
    } else {
        Err("MEXC reference stale: native trigger source is unknown; stop not changed".into())
    }
}

// Moving a trigger alone cannot retain a stop-limit's execution-price offset.
fn check_plan_limit(edit: &Edit, raw: &OrderRow<'_>) -> Result<(), String> {
    if edit.scope == Scope::Plan
        && edit.leg.is_none()
        && raw.order_type != 5.
        && edit.price.is_some_and(|price| price != raw.trigger_price)
    {
        return Err(
            "MEXC stop-limit trigger cannot retain its limit offset; stop not changed".into(),
        );
    }
    Ok(())
}

pub fn native_edit(edit: &Edit) -> Result<Option<Step>, String> {
    if (edit.scope == Scope::Regular && edit.leg.is_none())
        || (edit.amount.is_some()
            && !(edit.scope == Scope::Stop
                && edit.observed.amount == 0.
                && edit.amount == Some(0.)))
        || edit.price.is_none()
    {
        return Ok(None);
    }
    let raw: OrderRow<'_> = serde_json::from_str(&edit.native).map_err(|e| e.to_string())?;
    check_plan_limit(edit, &raw)?;
    let native: Native<'_> = serde_json::from_str(&edit.native).map_err(|e| e.to_string())?;
    let price = edit.price.unwrap();
    if !price.is_finite() || price <= 0. {
        return Err("MEXC replacement price must be positive; stop not changed".into());
    }
    let (path, body) = if edit.scope == Scope::Plan && edit.leg.is_none() {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Modify<'a> {
            symbol: &'a str,
            order_id: &'a str,
            trigger_price: String,
            price: String,
            order_type: f64,
            trigger_type: f64,
            trend: u8,
            from: u8,
        }
        if !(1.0..=5.0).contains(&raw.order_type) || !matches!(raw.trigger_type, 1. | 2.) {
            return Err("MEXC plan class is not proven; stop not changed".into());
        }
        (
            "/api/v1/private/planorder/change_price",
            json(&Modify {
                symbol: &raw.symbol,
                order_id: &edit.observed.id,
                trigger_price: num::format(price),
                price: num::format(raw.price),
                order_type: raw.order_type,
                trigger_type: raw.trigger_type,
                trend: source(raw.trend, price)?,
                from: 1,
            })?,
        )
    } else {
        if raw.stop_loss_price.is_none() || raw.take_profit_price.is_none() {
            return Err("MEXC TP/SL pair provenance missing; stop not changed".into());
        }
        let take = edit
            .leg
            .unwrap_or(edit.observed.kind == OrderType::TakeProfit);
        let sl = if take {
            raw.stop_loss_price.unwrap()
        } else {
            price
        };
        let tp = if take {
            price
        } else {
            raw.take_profit_price.unwrap()
        };
        let path = match edit.scope {
            Scope::Stop => "/api/v1/private/stoporder/change_plan_price",
            Scope::Plan => "/api/v1/private/planorder/change_stop_order",
            Scope::Regular if edit.observed.kind == OrderType::Limit => {
                "/api/v1/private/stoporder/change_price"
            }
            _ => {
                return Err(
                    "MEXC native protective edit class is not proven; stop not changed".into(),
                );
            }
        };
        (
            path,
            json(&Pair {
                symbol: (edit.scope == Scope::Plan).then_some(raw.symbol.as_ref()),
                order_id: (edit.scope != Scope::Stop).then_some(edit.observed.id.as_str()),
                stop_plan_order_id: (edit.scope == Scope::Stop)
                    .then_some(edit.observed.id.as_str()),
                stop_loss_price: num::format(sl),
                take_profit_price: num::format(tp),
                loss_trend: source(raw.loss_trend.or(raw.trend), sl)?,
                profit_trend: source(raw.profit_trend.or(raw.trend), tp)?,
                take_profit_reverse: native.take_profit_reverse,
                stop_loss_reverse: native.stop_loss_reverse,
            })?,
        )
    };
    Ok(Some(Step {
        receipts: vec![],
        path,
        body,
        call: Call::NativeEdit,
    }))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Plan<'a> {
    symbol: &'a str,
    price: f64,
    vol: f64,
    leverage: f64,
    side: f64,
    open_type: f64,
    trigger_price: f64,
    trigger_type: f64,
    execute_cycle: f64,
    order_type: f64,
    trend: u8,
    price_protect: f64,
    position_mode: f64,
    loss_trend: u8,
    profit_trend: u8,
    stop_loss_price: f64,
    take_profit_price: f64,
    reduce_only: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Stop<'a> {
    position_id: &'a str,
    vol: f64,
    loss_trend: u8,
    profit_trend: u8,
    stop_loss_price: f64,
    take_profit_price: f64,
    price_protect: f64,
    profit_loss_vol_type: &'a str,
    vol_type: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_vol: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_vol: Option<f64>,
    take_profit_reverse: f64,
    stop_loss_reverse: f64,
    take_profit_type: f64,
    stop_loss_type: f64,
    take_profit_order_price: f64,
    stop_loss_order_price: f64,
}
fn required(n: Option<f64>) -> Result<f64, String> {
    n.ok_or_else(|| "MEXC exact native restore provenance missing; re-place protection".into())
}
pub fn placement(edit: &Edit, restore: bool) -> Result<(&'static str, String), String> {
    let raw: OrderRow<'_> = serde_json::from_str(&edit.native).map_err(|e| e.to_string())?;
    let n: Native<'_> = serde_json::from_str(&edit.native).map_err(|e| e.to_string())?;
    if restore && (edit.observed.parent_id.is_some() || edit.leg.is_some()) {
        return Err("MEXC attached protection cannot be exactly re-placed".into());
    }
    let amount = if restore { None } else { edit.amount };
    let price = if restore { None } else { edit.price };
    if !restore {
        check_plan_limit(edit, &raw)?;
    }
    if edit.scope == Scope::Plan {
        if !matches!(n.execute_cycle, Some(1. | 2.))
            || !matches!(n.open_type, Some(1. | 2.))
            || !matches!(n.position_mode, Some(1. | 2.))
            || !matches!(n.price_protect, Some(0. | 1.))
            || !matches!(raw.trigger_type, 1. | 2.)
            || !(1.0..=5.0).contains(&raw.order_type)
            || (raw.order_type != 5. && raw.price <= 0.)
        {
            return Err(
                "MEXC native plan parameters cannot be exactly re-placed; stop not changed".into(),
            );
        }
        let sl = required(raw.stop_loss_price)?;
        let tp = required(raw.take_profit_price)?;
        if restore && (sl > 0. || tp > 0. || !raw.reduce_only || !matches!(raw.side, 2. | 4.)) {
            return Err("MEXC grouped or opening protection cannot be exactly re-placed".into());
        }
        let vol = amount.unwrap_or(raw.vol);
        if vol <= 0. {
            return Err("MEXC replacement quantity must be positive".into());
        }
        Ok((
            PLAN,
            json(&Plan {
                symbol: &raw.symbol,
                price: raw.price,
                vol,
                leverage: required(n.leverage)?,
                side: raw.side,
                open_type: required(n.open_type)?,
                trigger_price: if edit.leg.is_none() {
                    price.unwrap_or(raw.trigger_price)
                } else {
                    raw.trigger_price
                },
                trigger_type: raw.trigger_type,
                execute_cycle: required(n.execute_cycle)?,
                order_type: raw.order_type,
                trend: source(raw.trend, raw.trigger_price)?,
                price_protect: required(n.price_protect)?,
                position_mode: required(n.position_mode)?,
                loss_trend: source(Some(required(raw.loss_trend)?), sl)?,
                profit_trend: source(Some(required(raw.profit_trend)?), tp)?,
                stop_loss_price: if edit.leg == Some(false) {
                    price.unwrap_or(sl)
                } else {
                    sl
                },
                take_profit_price: if edit.leg == Some(true) {
                    price.unwrap_or(tp)
                } else {
                    tp
                },
                reduce_only: raw.reduce_only,
            })?,
        ))
    } else if edit.scope == Scope::Stop {
        if n.vol_type == Some(2.) && amount.is_some_and(|a| a > 0.) {
            return Err("MEXC full-position protection cannot be resized; stop not changed".into());
        }
        if !matches!(n.vol_type, Some(1. | 2.))
            || !matches!(n.price_protect, Some(0. | 1.))
            || !matches!(n.take_profit_reverse, Some(1. | 2.))
            || !matches!(n.stop_loss_reverse, Some(1. | 2.))
            || !matches!(n.take_profit_type, Some(0. | 1.))
            || !matches!(n.stop_loss_type, Some(0. | 1.))
            || !matches!(n.profit_loss_vol_type.as_ref(), "SAME" | "SEPARATE")
        {
            return Err(
                "MEXC native stop parameters cannot be exactly re-placed; stop not changed".into(),
            );
        }
        // A parent/group or split pair is not a standalone restore contract.
        let sl = required(raw.stop_loss_price)?;
        let tp = required(raw.take_profit_price)?;
        if n.position_id.is_empty()
            || (restore
                && (!matches!(raw.order_id.as_ref(), "" | "0")
                    || (sl > 0. && tp > 0.)
                    || n.profit_loss_vol_type != "SAME"
                    || required(n.vol_type)? != 2.))
        {
            return Err("MEXC partial or attached protection cannot be exactly re-placed".into());
        }
        let take = edit.observed.kind == OrderType::TakeProfit;
        if amount.filter(|v| *v > 0.).unwrap_or(raw.vol) <= 0. {
            return Err("MEXC native stop quantity is unproven; stop not changed".into());
        }
        Ok((
            STOP,
            json(&Stop {
                position_id: &n.position_id,
                vol: amount.filter(|v| *v > 0.).unwrap_or(raw.vol),
                loss_trend: source(Some(required(raw.loss_trend)?), sl)?,
                profit_trend: source(Some(required(raw.profit_trend)?), tp)?,
                stop_loss_price: if take { sl } else { price.unwrap_or(sl) },
                take_profit_price: if take { price.unwrap_or(tp) } else { tp },
                price_protect: required(n.price_protect)?,
                profit_loss_vol_type: &n.profit_loss_vol_type,
                vol_type: required(n.vol_type)?,
                take_profit_vol: n.take_profit_vol,
                stop_loss_vol: n.stop_loss_vol,
                take_profit_reverse: required(n.take_profit_reverse)?,
                stop_loss_reverse: required(n.stop_loss_reverse)?,
                take_profit_type: required(n.take_profit_type)?,
                stop_loss_type: required(n.stop_loss_type)?,
                take_profit_order_price: required(n.take_profit_order_price)?,
                stop_loss_order_price: required(n.stop_loss_order_price)?,
            })?,
        ))
    } else {
        Err("MEXC exact standalone protection provenance unavailable".into())
    }
}

impl MexcFutures {
    pub(crate) fn protective_send_guard(
        &self,
        cx: &Cx<'_, Pending>,
        request: &Request,
    ) -> Result<(), String> {
        let Some(job) = self
            .trading
            .active
            .as_ref()
            .filter(|j| j.command == request.command)
        else {
            return Err("MEXC superseded trading request".into());
        };
        let Some(edit) = job.protection.as_ref() else {
            return Ok(());
        };
        if let Call::Cancel { .. } | Call::NativeEdit = request.call {
            guard(cx, &self.references, edit, false)?;
        }
        if let Call::ProtectivePlace { restore: true } = request.call {
            guard(cx, &self.references, edit, true)?;
        }
        Ok(())
    }
    pub(super) fn send_protective_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        page: u8,
        after_cancel: bool,
        attempt: u8,
    ) {
        let edit = self
            .trading
            .active
            .as_ref()
            .unwrap()
            .protection
            .as_ref()
            .unwrap();
        let path = match edit.scope {
            Scope::Plan => "/api/v1/private/planorder/list/orders".to_owned(),
            Scope::Stop => "/api/v1/private/stoporder/list/orders".to_owned(),
            _ => format!("/api/v1/private/order/get/{}", edit.observed.id),
        };
        let payload = if edit.scope == Scope::Regular {
            String::new()
        } else {
            let market = cx.store.market(&edit.observed.symbol).unwrap();
            let start = if edit.observed.last_update >= 1e12 {
                edit.observed.last_update - 89. * 86400000.
            } else {
                cx.now - 89. * 86400000.
            };
            crate::sign::query(vec![
                ("symbol", market.id.clone()),
                ("start_time", num::format(start.max(0.))),
                ("end_time", num::format(cx.now)),
                ("page_num", page.to_string()),
                ("page_size", "100".into()),
                (
                    if edit.scope == Scope::Plan {
                        "states"
                    } else {
                        "state"
                    },
                    if after_cancel { "2" } else { "1" }.into(),
                ),
            ])
        };
        self.trading.waiting = true;
        self.signed(
            cx,
            Method::Get,
            &path,
            payload,
            false,
            Pending::Trade(Request {
                command,
                call: Call::ProtectiveRead {
                    page,
                    after_cancel,
                    attempt,
                },
                revision: self.account.revision,
            }),
        );
    }
    pub(super) fn protective_read_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        page: u8,
        after_cancel: bool,
        attempt: u8,
        response: Response<'_>,
    ) {
        let mut outstanding = false;
        let result = (|| -> Result<Option<Step>, String> {
            if response.status != 200 {
                return Err(format!(
                    "MEXC protection inspection HTTP {}; stop not changed",
                    response.status
                ));
            }
            let payload = Envelope::checked(response.body)?.payload()?.to_owned();
            let edit = self
                .trading
                .active
                .as_ref()
                .unwrap()
                .protection
                .as_ref()
                .unwrap();
            if !cx.now.is_finite() || !(0.0..=20_000.).contains(&(cx.now - edit.started)) {
                return Err("MEXC protection inspection deadline elapsed".into());
            }
            let rows: Vec<&serde_json::value::RawValue> = if edit.scope == Scope::Regular {
                vec![serde_json::from_str(&payload).map_err(|e| e.to_string())?]
            } else {
                serde_json::from_str(&payload).map_err(|e| e.to_string())?
            };
            let mut found = None;
            for item in &rows {
                let raw: OrderRow<'_> =
                    serde_json::from_str(item.get()).map_err(|e| e.to_string())?;
                let id = if edit.scope == Scope::Regular && !raw.order_id.is_empty() {
                    &raw.order_id
                } else if !raw.id.is_empty() {
                    &raw.id
                } else {
                    &raw.order_id
                };
                if id == &edit.observed.id {
                    if found.is_some() {
                        return Err("MEXC duplicate native order identity".into());
                    }
                    found = Some(item.get().to_owned());
                }
            }
            let Some(native) = found else {
                if rows.len() == 100 && page < 20 {
                    self.send_protective_read(cx, command, page + 1, after_cancel, attempt);
                    outstanding = true;
                    return Ok(None);
                }
                return Err(
                    "MEXC original protective order not proven; replacement withheld".into(),
                );
            };
            let raw: OrderRow<'_> = serde_json::from_str(&native).map_err(|e| e.to_string())?;
            let params: Native<'_> = serde_json::from_str(&native).map_err(|e| e.to_string())?;
            if edit.scope == Scope::Regular && params.deal_vol != Some(0.) {
                return Err("MEXC native unfilled quantity is unproven; stop not changed".into());
            }
            if (edit.scope == Scope::Stop && !matches!(raw.position_type, 1. | 2.))
                || (edit.scope != Scope::Stop && !matches!(raw.side, 1. | 2. | 3. | 4.))
            {
                return Err("MEXC native order side is unproven; stop not changed".into());
            }
            let decoded = self
                .map_order(cx, &raw, edit.scope)
                .ok_or("MEXC invalid native protection")?;
            if decoded.symbol != edit.observed.symbol
                || decoded.side != edit.observed.side
                || decoded.filled != 0.
                || raw.deal_vol != 0.
            {
                return Err("MEXC protective order changed or filled; replacement withheld".into());
            }
            if after_cancel {
                if raw.state == 1. && attempt < 5 {
                    // Reuse the existing bounded trade-read timer; no cancel is resent.
                    let retry = Request {
                        command,
                        call: Call::ProtectiveRead {
                            page: 1,
                            after_cancel: true,
                            attempt: attempt + 1,
                        },
                        revision: self.account.revision,
                    };
                    let timer = cx.set_timer(200.);
                    self.trading.read_retry = Some((timer, retry));
                    return Ok(None);
                }
                if raw.state != 2. {
                    return Err(
                        "MEXC cancellation is not authoritative; replacement withheld".into(),
                    );
                }
                let original: OrderRow<'_> =
                    serde_json::from_str(&edit.native).map_err(|e| e.to_string())?;
                let mut original_params: Native<'_> =
                    serde_json::from_str(&edit.native).map_err(|e| e.to_string())?;
                let mut terminal_params: Native<'_> =
                    serde_json::from_str(&native).map_err(|e| e.to_string())?;
                if edit.scope != Scope::Regular {
                    original_params.deal_vol = None;
                    terminal_params.deal_vol = None;
                }
                if original_params != terminal_params
                    || raw.side != original.side
                    || raw.reduce_only != original.reduce_only
                    || raw.loss_trend != original.loss_trend
                    || raw.profit_trend != original.profit_trend
                    || raw.vol_type != original.vol_type
                    || raw.position_type != original.position_type
                {
                    return Err(
                        "MEXC native protection changed during cancellation; replacement withheld"
                            .into(),
                    );
                }
                if raw.trigger_price != original.trigger_price
                    || raw.price != original.price
                    || raw.vol != original.vol
                    || raw.trend != original.trend
                    || raw.stop_loss_price != original.stop_loss_price
                    || raw.take_profit_price != original.take_profit_price
                    || raw.order_type != original.order_type
                    || raw.trigger_type != original.trigger_type
                {
                    return Err(
                        "MEXC canceled protection differs from original; replacement withheld"
                            .into(),
                    );
                }
                if cx
                    .store
                    .order(&edit.observed.id)
                    .is_none_or(|o| o.filled != 0. || o.status != OrderStatus::Canceled)
                {
                    return Err(
                        "MEXC original protection changed after cancel; replacement withheld"
                            .into(),
                    );
                }
                let job = self.trading.active.as_mut().unwrap();
                let edit = job.protection.as_mut().unwrap();
                edit.cancel_proven = true;
                let (path, body) = edit
                    .replacement
                    .clone()
                    .ok_or("MEXC replacement provenance unavailable")?;
                Ok(Some(Step {
                    receipts: vec![],
                    path: if path == PLAN { PLAN } else { STOP },
                    body,
                    call: Call::ProtectivePlace { restore: false },
                }))
            } else {
                if raw.state != if edit.scope == Scope::Regular { 2. } else { 1. }
                    || decoded.price != edit.observed.price
                    || decoded.amount != edit.observed.amount
                    || decoded.kind != edit.observed.kind
                {
                    return Err("MEXC native protection changed; stop not changed".into());
                }
                self.trading
                    .active
                    .as_mut()
                    .unwrap()
                    .protection
                    .as_mut()
                    .unwrap()
                    .native = native.clone();
                let edit = self
                    .trading
                    .active
                    .as_ref()
                    .unwrap()
                    .protection
                    .as_ref()
                    .unwrap();
                if let Some(step) = native_edit(edit)? {
                    return Ok(Some(step));
                }
                if let Some(amount) = edit.amount {
                    let market = cx
                        .store
                        .market(&edit.observed.symbol)
                        .ok_or("MEXC market unavailable")?;
                    if !(amount == 0. && edit.scope == Scope::Stop && edit.observed.amount == 0.) {
                        replacement_fits(cx, &edit.observed.symbol, amount)?;
                        let unit = self
                            .volume_units
                            .get(&edit.observed.symbol)
                            .copied()
                            .unwrap_or(1.);
                        if !amount.is_finite()
                            || amount <= 0.
                            || amount * market.contract_size.unwrap_or(1.)
                                > market.limits.amount.max
                            || num::adjust(amount, unit) != amount
                        {
                            return Err("MEXC replacement quantity is outside native limits; stop not changed".into());
                        }
                    }
                }
                guard(cx, &self.references, edit, false)?;
                if edit.scope == Scope::Regular {
                    let o = &edit.observed;
                    let opts = PlaceOrderOpts {
                        symbol: o.symbol.clone(),
                        kind: o.kind,
                        side: o.side,
                        amount: edit.amount.unwrap_or(raw.vol),
                        price: if edit.leg.is_some() {
                            Some(o.price)
                        } else {
                            edit.price.or(Some(o.price))
                        },
                        reduce_only: Some(o.reduce_only),
                        time_in_force: o.time_in_force,
                        stop_loss: if edit.leg == Some(false) {
                            edit.price.or(raw.stop_loss_price)
                        } else {
                            raw.stop_loss_price
                        }
                        .filter(|p| *p > 0.),
                        take_profit: if edit.leg == Some(true) {
                            edit.price.or(raw.take_profit_price)
                        } else {
                            raw.take_profit_price
                        }
                        .filter(|p| *p > 0.),
                        sl_trigger_type: raw.loss_trend.map(|s| reference(Some(s))).transpose()?,
                        tp_trigger_type: raw
                            .profit_trend
                            .map(|s| reference(Some(s)))
                            .transpose()?,
                        trigger_price: None,
                        trigger_type: None,
                        client_order_id: None,
                        close: o.close,
                        stp_mode: None,
                        max_slippage_pct: None,
                        bybit_bbo: None,
                        bybit_slippage_tolerance_ticks: None,
                        implied_volatility: None,
                    };
                    let id = o.id.clone();
                    let amount = edit.amount;
                    let mut step = self.placement(cx, opts)?;
                    if let Call::Place { amend, .. } = &mut step.call {
                        *amend = Some(AmendIntent {
                            id: id.clone(),
                            amount,
                        });
                    }
                    let mut steps = self.cancel_steps(cx, &[id])?;
                    steps.push_back(step);
                    self.trading.active.as_mut().unwrap().steps = steps;
                    return Ok(None);
                }
                let (path, body) = placement(edit, false)?;
                let restore = placement(edit, true).ok().map(|(p, b)| (p.to_owned(), b));
                let id = edit.observed.id.clone();
                let mut cancels = self.cancel_steps(cx, &[id])?;
                let job = self.trading.active.as_mut().unwrap();
                let edit = job.protection.as_mut().unwrap();
                edit.replacement = Some((path.into(), body));
                edit.restore = restore;
                cancels.push_back(Step {
                    receipts: vec![],
                    path: "",
                    body: String::new(),
                    call: Call::ProtectiveRead {
                        page: 1,
                        after_cancel: true,
                        attempt: 0,
                    },
                });
                job.steps = cancels;
                Ok(None)
            }
        })();
        self.trading.waiting = outstanding;
        match result {
            Ok(Some(step)) => {
                self.trading.active.as_mut().unwrap().steps.push_front(step);
                self.advance_trade(cx);
            }
            Ok(None) => {
                if self.trading.read_retry.is_none() && !self.trading.waiting {
                    self.advance_trade(cx);
                }
            }
            Err(error) => {
                let error = if after_cancel {
                    VenueError::new(error).outcome(Outcome::Unknown)
                } else {
                    protective_not_sent(error)
                };
                self.trade_failed(cx, command, error);
            }
        }
    }
    pub(super) fn start_protective_restore(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: &VenueError,
    ) -> bool {
        let Some(edit) = self
            .trading
            .active
            .as_ref()
            .and_then(|j| j.protection.as_ref())
        else {
            return false;
        };
        if !edit.cancel_proven || edit.restoring || guard(cx, &self.references, edit, true).is_err()
        {
            return false;
        }
        let Some((path, body)) = edit.restore.clone() else {
            return false;
        };
        let edit = self
            .trading
            .active
            .as_mut()
            .unwrap()
            .protection
            .as_mut()
            .unwrap();
        edit.restoring = true;
        edit.refusal = Some(error.message.clone());
        self.trading.active.as_mut().unwrap().steps = VecDeque::from([Step {
            receipts: vec![],
            path: if path == PLAN { PLAN } else { STOP },
            body,
            call: Call::ProtectivePlace { restore: true },
        }]);
        self.trading.waiting = false;
        self.advance_trade(cx);
        let _ = command;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::test_support::{ProbeVenue, seeded_engine};

    #[test]
    fn a_deferred_cancel_rechecks_at_the_single_send_path_and_unwinds_on_veto() {
        for changed in [false, true] {
            let venue = MexcFutures::new(crate::Options::default());
            let mut engine = seeded_engine(ProbeVenue::new(venue, move |venue, cx| {
                let order = Order {
                    id: "o".into(),
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: "BTCUSDT".into(),
                    kind: OrderType::StopLoss,
                    side: OrderSide::Sell,
                    price: 55000.,
                    amount: 1.,
                    filled: 0.,
                    remaining: 1.,
                    reduce_only: false,
                    close: None,
                    client_order_id: None,
                    time_in_force: None,
                    last_update: 0.,
                    trigger_type: Some(StopTriggerType::MarkPrice),
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                };
                cx.store.upsert_order(order.clone());
                cx.store.set_positions(vec![Position {
                    id: "p".into(),
                    symbol: "BTCUSDT".into(),
                    side: PositionSide::Long,
                    entry_price: 60000.,
                    notional: 60000.,
                    leverage: 1.,
                    margin_type: None,
                    position_mode: None,
                    unrealized_pnl: 0.,
                    realized_pnl: None,
                    contracts: 1.,
                    liquidation_price: 0.,
                    margin_size: None,
                    last_update: None,
                }]);
                venue
                    .references
                    .entry("BTCUSDT".into())
                    .or_default()
                    .observe(cx.now, [None, Some(60000.), None]);
                venue.trading.active=Some(Job{command:1,steps:VecDeque::new(),ids:vec![],gap:0.,total_orders:1,failed_index:None,effective:false,replaces:None,protection:Some(Box::new(Edit{observed:order,scope:Scope::Plan,leg:None,price:Some(54000.),amount:Some(2.),native:r#"{"id":"o","symbol":"BTC_USDT","side":4,"orderType":5,"state":1,"vol":1,"triggerPrice":55000,"triggerType":2,"trend":2}"#.into(),replacement:None,restore:None,canceled:false,cancel_proven:false,restoring:false,refusal:None,started:cx.now}))});
                let cancel_request = Request {
                    command: 1,
                    revision: 0,
                    call: Call::Cancel {
                        ids: vec!["o".into()],
                        regular: false,
                    },
                };
                assert!(venue.protective_send_guard(cx, &cancel_request).is_ok());
                venue.defer_private = true;
                venue.signed(
                    cx,
                    Method::Post,
                    "/api/v1/private/planorder/cancel",
                    "[]".into(),
                    false,
                    Pending::Trade(cancel_request),
                );
                assert_eq!(venue.deferred_private.len(), 1);
                if changed {
                    venue
                        .references
                        .get_mut("BTCUSDT")
                        .unwrap()
                        .observe(cx.now, [None, Some(53000.), None]);
                }
                venue.defer_private = false;
                venue.flush_private(cx);
                assert!(venue.deferred_private.is_empty());
                assert_eq!(venue.trading.active.is_none(), changed);
            }));
            engine.start(1000.);
            let actions: Vec<_> = engine.drain_actions().collect();
            if changed {
                assert!(
                    !actions
                        .iter()
                        .any(|a| matches!(a, safe_cex_engine::Action::Http(_)))
                );
            }
        }
    }
    #[test]
    fn per_field_receipts_reject_future_nonfinite_zero_and_stale_values() {
        let mut refs = References::default();
        refs.observe(1000., [Some(60000.), Some(61000.), Some(62000.)]);
        assert_eq!(refs.get(StopTriggerType::MarkPrice, 6000.), Some(61000.));
        assert_eq!(refs.get(StopTriggerType::MarkPrice, 6001.), None);
        assert_eq!(refs.get(StopTriggerType::LastPrice, 999.), None);
        assert_eq!(refs.get(StopTriggerType::IndexPrice, f64::NAN), None);
        refs.observe(5999., [Some(63000.), None, Some(0.)]);
        assert_eq!(refs.get(StopTriggerType::MarkPrice, 6001.), None);
        assert_eq!(refs.get(StopTriggerType::LastPrice, 6001.), Some(63000.));
        assert_eq!(refs.get(StopTriggerType::IndexPrice, 6000.), None);
    }
}
