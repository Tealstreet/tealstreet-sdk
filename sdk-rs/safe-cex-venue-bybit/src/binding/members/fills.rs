//! Per-order executed quantity read from REST, for the demo reduce-only reconciliation.
use crate::{
    binding::{
        host::{get_key, text},
        promise::await_js,
        rt::{Rt, venue},
        value::{Js, Thrown, error_object},
    },
    primary_order_lifecycle as lifecycle,
};
use safe_cex_num::parse_float;
use std::{cell::RefCell, rc::Rc};

const FILL_RECONCILIATION_MAX_PAGES: u32 = 20;
const USER_TRADES: &str = "/v5/execution/list";

fn fail(message: &str) -> Thrown {
    Thrown::new(error_object("Error", message))
}

fn now(rt: &Rt) -> Result<f64, Thrown> {
    rt.global("virtualClock.getCurrentTimeMs")?
        .call(Js::Undefined, vec![])?
        .to_number()
}

// ts: BybitExchange#resolveOrderSymbolForFills
pub(crate) fn resolve_order_symbol_for_fills(
    rt: &Rt,
    order_id: &str,
) -> Result<Option<String>, Thrown> {
    let venue = venue(rt)?;
    let store = venue.base.store.clone();
    lifecycle::resolve_order_symbol_for_fills(
        &venue.order_state,
        order_id,
        || {
            Ok(Rc::new(RefCell::new(
                store
                    .orders()
                    .iter()
                    .map(|o| Rc::new(RefCell::new(o.clone())))
                    .collect(),
            )))
        },
        || now(rt),
    )
}

/// `exec.execQty ?? exec.closedSize ?? exec.qty ?? '0'`.
fn executed_quantity(exec: &Js) -> Result<f64, Thrown> {
    for key in ["execQty", "closedSize", "qty"] {
        let value = exec.get(key)?;
        if !value.is_nullish() {
            return Ok(parse_float(&text(&value)?));
        }
    }
    Ok(parse_float("0"))
}

async fn fetch_order_fills(
    rt: &Rt,
    order_id: &str,
    fills: &mut Vec<(String, f64)>,
) -> Result<(), Thrown> {
    let symbol = resolve_order_symbol_for_fills(rt, order_id)?
        .filter(|symbol| !symbol.is_empty())
        .ok_or_else(|| fail(&format!("cannot resolve a symbol for order {order_id}")))?;
    let category: String = venue(rt)?.get_category_for_symbol(&symbol).into();
    let mut cursor = String::new();
    for _ in 1..=FILL_RECONCILIATION_MAX_PAGES {
        let mut params = vec![
            ("category", Js::str(&category)),
            ("symbol", Js::str(&symbol)),
            ("orderId", Js::str(order_id)),
            ("limit", Js::Number(50.)),
        ];
        if !cursor.is_empty() {
            params.push(("cursor", Js::str(&cursor)));
        }
        let xhr = rt.this.get("xhr")?;
        let request = xhr.call_method(
            "get",
            vec![
                Js::str(USER_TRADES),
                Js::object(vec![("params", Js::object(params))]),
            ],
        )?;
        let data = await_js(&rt.exchange_runtime(), request)
            .await?
            .get("data")?;
        let ret_msg = get_key(&data, "retMsg")?;
        let ret_msg = if ret_msg.truthy() {
            text(&ret_msg)?
        } else {
            "UNKNOWN".to_owned()
        };
        if ret_msg != "OK" {
            return Err(fail(&ret_msg));
        }
        let result = if data.is_nullish() {
            Js::Undefined
        } else {
            data.get("result")?
        };
        let list = if result.is_nullish() {
            Js::Undefined
        } else {
            result.get("list")?
        };
        for exec in list.array_values().unwrap_or_default() {
            let exec_order_id = exec.get("orderId")?;
            let exec_order_id = if exec_order_id.is_nullish() {
                String::new()
            } else {
                text(&exec_order_id)?
            };
            if exec_order_id != order_id {
                continue;
            }
            let qty = executed_quantity(&exec)?;
            if qty.is_finite() && qty > 0. {
                match fills.iter_mut().find(|(id, _)| id == order_id) {
                    Some((_, total)) => *total += qty,
                    None => fills.push((order_id.to_owned(), qty)),
                }
            }
        }
        let next = if result.is_nullish() {
            Js::Undefined
        } else {
            result.get("nextPageCursor")?
        };
        let next = if next.truthy() {
            text(&next)?
        } else {
            String::new()
        };
        if next.is_empty() || next == cursor {
            return Ok(());
        }
        cursor = next;
    }
    Err(fail(&format!(
        "fill history for order {order_id} exceeded {FILL_RECONCILIATION_MAX_PAGES} pages"
    )))
}

// ts: BybitExchange.fetchFillsForOrderIds
pub(crate) fn fetch_fills_for_order_ids(rt: &Rt, order_ids: Js) -> Result<Js, Thrown> {
    let rt = rt.clone();
    let body_rt = rt.clone();
    Ok(rt.spawn(Box::pin(async move {
        let rt = body_rt;
        let map = Js::new_map();
        let ids = order_ids.array_values().unwrap_or_default();
        if ids.is_empty() {
            return Ok(map);
        }
        let mut requested: Vec<String> = vec![];
        for id in &ids {
            let id = text(id)?;
            if !requested.contains(&id) {
                requested.push(id);
            }
        }
        let mut fills = vec![];
        for order_id in &requested {
            if let Err(error) = fetch_order_fills(&rt, order_id, &mut fills).await {
                let message = error.value.get("message")?;
                let message = message.to_js_string()?;
                rt.this.call_method(
                    "log",
                    vec![
                        Js::str(&format!(
                            "Error fetching fills for order IDs: {}",
                            String::from_utf16_lossy(&message)
                        )),
                        rt.global("LogSeverity.Warning")?,
                    ],
                )?;
                return Err(error);
            }
        }
        for (id, qty) in fills {
            map.call_method("set", vec![Js::str(&id), Js::Number(qty)])?;
        }
        Ok(map)
    })))
}
