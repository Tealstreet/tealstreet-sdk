//! WOOXExchange members ported outside the legacy host modules: response and
//! transaction helpers, history readers, order paging, order mappers and the
//! lifecycle bodies, each over the JSON/typed boundary its port declares.
use crate::binding::{
    client::JsRestAdapter,
    ops,
    rt::{Rt, Shared},
    value::{Js, Thrown, arg, from_json, from_typed, to_json, to_typed},
};
use crate::{
    historical_fills, history_fetch, legacy_fills, legacy_fills::WooLegacyFillsQuery,
    legacy_lifecycle, order_pages, orders, response, transaction,
};
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use serde_json::{Map, Value};
use std::rc::{Rc, Weak};

fn coercion(e: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::type_error(e.message)
}

fn optional_json(v: &Js) -> Result<Option<Value>, Thrown> {
    Ok(if v.is_undefined() {
        None
    } else {
        Some(to_json(v)?)
    })
}

fn object(v: &Js) -> Result<Map<String, Value>, Thrown> {
    match to_json(v)? {
        Value::Object(map) => Ok(map),
        other => Err(Thrown::type_error(&format!(
            "expected an object, received {other}"
        ))),
    }
}

fn number_or_undefined(v: Option<f64>) -> Js {
    v.map_or(Js::Undefined, Js::Number)
}

fn venue(rt: &Rt) -> Result<Rc<dyn Exchange>, Thrown> {
    rt.shared
        .sockets
        .venue
        .borrow()
        .as_ref()
        .and_then(Weak::upgrade)
        .ok_or_else(|| Thrown::error("Error", "WOO exchange is not assembled"))
}

fn text_option(v: &Js) -> Result<Option<String>, Thrown> {
    Ok(if v.is_nullish() {
        None
    } else {
        Some(String::from_utf16_lossy(&v.to_js_string()?))
    })
}

fn number_option(v: &Js) -> Result<Option<f64>, Thrown> {
    Ok(if v.is_undefined() {
        None
    } else {
        Some(v.to_number()?)
    })
}

/// `{ symbol?, fromMilliseconds?, toMilliseconds?, limit }` as the legacy query.
fn history_query(opts: &Js) -> Result<WooLegacyFillsQuery, Thrown> {
    Ok(WooLegacyFillsQuery {
        symbol: text_option(&opts.get("symbol")?)?,
        from_milliseconds: number_option(&opts.get("fromMilliseconds")?)?,
        to_milliseconds: number_option(&opts.get("toMilliseconds")?)?,
        limit: opts.get("limit")?.to_number()?,
        page: number_option(&opts.get("page")?)?,
    })
}

fn rows(values: Vec<Value>) -> Js {
    Js::array(values.iter().map(from_json).collect())
}

fn parsed_transaction(row: transaction::ParsedWooTransaction) -> Result<Js, Thrown> {
    Ok(Js::object(vec![
        ("symbol", Js::str(&row.symbol)),
        ("side", from_typed(&row.side)?),
        ("price", Js::Number(row.price)),
        ("qty", Js::Number(row.qty)),
        (
            "timestampMilliseconds",
            Js::Number(row.timestamp_milliseconds),
        ),
        ("timestampSeconds", Js::Number(row.timestamp_seconds)),
        (
            "orderId",
            row.order_id.map_or(Js::Undefined, |s| Js::str(&s)),
        ),
        ("fillId", row.fill_id.map_or(Js::Undefined, |s| Js::str(&s))),
        ("maker", row.maker.map_or(Js::Undefined, Js::Bool)),
        ("commission", number_or_undefined(row.commission)),
        (
            "commissionAsset",
            row.commission_asset
                .as_ref()
                .map_or(Js::Undefined, from_json),
        ),
        ("realizedPnl", number_or_undefined(row.realized_pnl)),
        ("positionSide", from_typed(&row.position_side)?),
    ]))
}

fn error_of(adapter: &JsRestAdapter, error: Box<dyn std::error::Error>) -> Thrown {
    match error.downcast::<safe_cex_transport::RestFailure>() {
        Ok(failure) => adapter.thrown(*failure),
        Err(error) => match error.downcast::<ExchangeError>() {
            Ok(error) => Thrown::from(*error),
            Err(error) => Thrown::error("Error", &error.to_string()),
        },
    }
}

fn sync(name: &str, f: impl Fn(&Rt, Vec<Js>) -> Result<Js, Thrown> + 'static, rt: &Rt) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, a| f(&rt, a))
}

/// An async arrow: the body runs on the runtime and its throw rejects the promise.
fn asynchronous<F>(name: &str, rt: &Rt, f: F) -> Js
where
    F: Fn(Rt, Vec<Js>) -> crate::legacy_metadata::Task<Js, Thrown> + 'static,
{
    let rt = rt.clone();
    Js::function(name, move |_, a| Ok(rt.spawn(f(rt.clone(), a))))
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    let weak = Rc::downgrade(&rt.shared);
    let runtime: ops::Runtime = rt.shared.runtime.clone();
    let lifecycle = move || ops::lifecycle(weak.clone(), runtime.clone());
    Some(Ok(match name {
        // ts: WOOXExchange.start / tick / fetchHistoricalPositions
        "start" => {
            let rt = rt.clone();
            Js::function(name, move |_, _| {
                Ok(legacy_lifecycle::start(lifecycle(), rt.this.clone()))
            })
        }
        "tick" => {
            let rt = rt.clone();
            Js::function(name, move |_, a| {
                let count = a.first().cloned();
                Ok(legacy_lifecycle::tick(lifecycle(), rt.this.clone(), count))
            })
        }
        "fetchHistoricalPositions" => {
            let rt = rt.clone();
            Js::function(name, move |_, a| {
                Ok(legacy_lifecycle::fetch_historical_positions(
                    lifecycle(),
                    rt.this.clone(),
                    arg(&a, 0),
                ))
            })
        }
        // ts: WOOXExchange#parseFiniteFundingNumber
        "parseFiniteFundingNumber" => sync(
            name,
            |_, a| {
                let raw = optional_json(&arg(&a, 0))?;
                Ok(number_or_undefined(
                    response::parse_finite_funding_number(raw.as_ref()).map_err(coercion)?,
                ))
            },
            rt,
        ),
        // ts: WOOXExchange#getWooResponseMessage
        "getWooResponseMessage" => sync(
            name,
            |_, a| {
                let fallback = String::from_utf16_lossy(&arg(&a, 1).to_js_string()?);
                Ok(Js::str(&response::get_woo_response_message(
                    &object(&arg(&a, 0))?,
                    &fallback,
                )))
            },
            rt,
        ),
        // ts: WOOXExchange#normalizeTimestampSeconds / normalizeTimestampMilliseconds
        "normalizeTimestampSeconds" => sync(
            name,
            |_, a| {
                let raw = optional_json(&arg(&a, 0))?;
                Ok(number_or_undefined(
                    response::normalize_timestamp_seconds(raw.as_ref()).map_err(coercion)?,
                ))
            },
            rt,
        ),
        "normalizeTimestampMilliseconds" => sync(
            name,
            |_, a| {
                let raw = optional_json(&arg(&a, 0))?;
                Ok(number_or_undefined(
                    response::normalize_timestamp_milliseconds(raw.as_ref()).map_err(coercion)?,
                ))
            },
            rt,
        ),
        // ts: WOOXExchange#getTransactionHistorySymbolCandidates
        "getTransactionHistorySymbolCandidates" => sync(
            name,
            |_, a| {
                let symbol = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
                Ok(Js::array(
                    response::get_transaction_history_symbol_candidates(&symbol)
                        .iter()
                        .map(|s| Js::str(s))
                        .collect(),
                ))
            },
            rt,
        ),
        // ts: WOOXExchange#parseTransactionHistoryRow
        "parseTransactionHistoryRow" => sync(
            name,
            |_, a| match transaction::parse_transaction_history_row(&object(&arg(&a, 0))?)
                .map_err(|e| Thrown::type_error(&e))?
            {
                Some(row) => parsed_transaction(row),
                None => Ok(Js::Null),
            },
            rt,
        ),
        // ts: WOOXExchange#parseTransactionRowsFromResponse
        "parseTransactionRowsFromResponse" => sync(
            name,
            |_, a| {
                let data = to_json(&arg(&a, 0))?;
                Ok(rows(
                    response::parse_transaction_rows_from_response(&data).to_vec(),
                ))
            },
            rt,
        ),
        // ts: WOOXExchange#fetchLegacyTransactionHistoryRows
        "fetchLegacyTransactionHistoryRows" => asynchronous(name, rt, |rt, a| {
            Box::pin(async move {
                let query = history_query(&arg(&a, 0))?;
                let adapter = JsRestAdapter::new(rt.exchange_runtime(), rt.this.get("xhr")?);
                match legacy_fills::fetch_legacy_transaction_history_rows(&adapter, &query).await {
                    Ok(values) => Ok(rows(values)),
                    Err(failure) => Err(adapter.thrown(failure)),
                }
            })
        }),
        // ts: WOOXExchange#fetchTransactionHistoryRows
        "fetchTransactionHistoryRows" => asynchronous(name, rt, |rt, a| {
            Box::pin(async move {
                let query = history_query(&arg(&a, 0))?;
                let adapter = JsRestAdapter::new(rt.exchange_runtime(), rt.this.get("xhr")?);
                match history_fetch::fetch_transaction_history_rows(&adapter, &query).await {
                    Ok(values) => Ok(rows(values)),
                    Err(failure) => Err(adapter.thrown(failure)),
                }
            })
        }),
        // ts: WOOXExchange#fetchHistoricalFills
        "fetchHistoricalFills" => asynchronous(name, rt, |rt, a| {
            Box::pin(async move {
                let opts = arg(&a, 0);
                // ts: `opts: ExecutionsOptions = {}`
                let opts = to_typed(&if opts.is_undefined() {
                    Js::object(vec![])
                } else {
                    opts
                })?;
                let adapter = JsRestAdapter::new(rt.exchange_runtime(), rt.this.get("xhr")?);
                let venue = venue(&rt)?;
                let fills =
                    historical_fills::fetch_historical_fills(venue.as_ref(), &adapter, &opts)
                        .await
                        .map_err(Thrown::from)?;
                from_typed(&fills)
            })
        }),
        // ts: WOOXExchange#fetchHistoricalTradesInternal
        "fetchHistoricalTradesInternal" => asynchronous(name, rt, |rt, a| {
            Box::pin(async move {
                let opts = to_typed(&arg(&a, 0))?;
                let adapter = JsRestAdapter::new(rt.exchange_runtime(), rt.this.get("xhr")?);
                let venue = venue(&rt)?;
                let trades = historical_fills::fetch_historical_trades_internal(
                    venue.as_ref(),
                    &adapter,
                    &opts,
                )
                .await
                .map_err(Thrown::from)?;
                from_typed(&trades)
            })
        }),
        // ts: WOOXExchange.fetchOrdersRecursively
        "fetchOrdersRecursively" => asynchronous(name, rt, |rt, a| {
            Box::pin(async move {
                let endpoint = String::from_utf16_lossy(&arg(&a, 0).to_js_string()?);
                let page = number_option(&arg(&a, 1))?;
                let adapter = JsRestAdapter::new(rt.exchange_runtime(), rt.this.get("xhr")?);
                match order_pages::fetch_orders_recursively(rt.base(), &adapter, &endpoint, page)
                    .await
                {
                    Ok(values) => Ok(rows(values)),
                    Err(error) => Err(error_of(&adapter, error)),
                }
            })
        }),
        // ts: WOOXExchange.mapLimitOrder
        "mapLimitOrder" => sync(
            name,
            |rt, a| super::order_mapping::map_limit_order(rt, arg(&a, 0)),
            rt,
        ),
        // ts: WOOXExchange.mapAlgoOrder
        "mapAlgoOrder" => sync(
            name,
            |rt, a| super::order_mapping::map_algo_order(rt, arg(&a, 0)),
            rt,
        ),
        // ts: WOOXExchange#isAlgoOrder
        "isAlgoOrder" => sync(
            name,
            |_, a| {
                let kind = optional_json(&arg(&a, 0))?;
                Ok(Js::Bool(orders::is_algo_order(kind.as_ref())))
            },
            rt,
        ),
        _ => return None,
    }))
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}

fn canonical_safe_integer(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && (b'1'..=b'9').contains(&bytes[0])
        && bytes.iter().all(u8::is_ascii_digit)
        && id.parse::<u64>().is_ok_and(|n| n <= 9_007_199_254_740_991)
}

// ts: WOOXExchange.clientOrderIdRule
pub(crate) fn client_order_id_rule() -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
        "WOO".into(),
        "/^[1-9]\\d*$/".into(),
        Rc::new(canonical_safe_integer),
        Default::default(),
    )
}
