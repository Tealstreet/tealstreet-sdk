//! Concrete account/position hosts and the adapters running the position,
//! balance, history and stream-merge bodies on their bound receiver.
use crate::{
    account_positions::{self, AccountFuture, AccountPositionsHost},
    account_predicates::{self, ApiCode},
    binding::{
        globals::arg,
        promise::{JsFuture, await_js},
        rt::{Rt, Shared, Variant},
        value::{Js, Thrown, error_object, from_json, from_typed, to_json, to_typed},
    },
    inverse_positions::{self, InversePositionsHost},
    normalization::{self, SymbolProfile},
    numeric::{FiniteNumberInput, to_finite_number},
};
use safe_cex_exchange::{
    build_records::{BuildRecordsOptions, FoldableExchange, build_folded_position_records},
    exchange::Exchange,
    journal::{HistoricalPositionOptions, JournalFolder},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{Execution, ExecutionsOptions, IncomeRecord, Market, Position};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
};

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// `v?.k`
fn opt(v: &Js, k: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        Ok(Js::Undefined)
    } else {
        v.get(k)
    }
}

fn optional_json(v: &Js) -> Result<Option<Value>, Thrown> {
    if v.is_undefined() {
        Ok(None)
    } else {
        to_json(v).map(Some)
    }
}

fn endpoint(name: &str) -> Result<&'static str, Thrown> {
    crate::protocol::ENDPOINTS
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.path)
        .ok_or_else(|| Thrown::error("Error", &format!("Bitget protocol table is missing {name}")))
}

/// `client.get(url[, { params }])` started now; the future yields `response.data`.
fn get_data(client: &Js, url: &str, params: Option<Js>) -> Result<JsFuture<'static>, Thrown> {
    let get = client.get("get")?;
    if !get.is_callable() {
        return Err(Thrown::type_error("client.get is not a function"));
    }
    let mut args = vec![Js::str(url)];
    if let Some(params) = params {
        args.push(Js::object(vec![("params", params)]));
    }
    let response = get.call(client.clone(), args)?;
    Ok(Box::pin(
        async move { await_js(response).await?.get("data") },
    ))
}

/// `this.toFiniteNumber(value, fallback)` over the observed typeof branches.
fn finite(v: &Js, fallback: f64) -> f64 {
    match v {
        Js::Number(n) => to_finite_number(FiniteNumberInput::Number(*n), fallback),
        Js::String(s) => to_finite_number(
            FiniteNumberInput::String(&String::from_utf16_lossy(s)),
            fallback,
        ),
        _ => to_finite_number(FiniteNumberInput::Other, fallback),
    }
}

fn testnet(rt: &Rt) -> Result<bool, Thrown> {
    Ok(rt.this.get("options")?.get("testnet")?.truthy())
}

/// `this._resolveMarket(symbol)` over the typed store the facade projects.
fn resolve_market(rt: &Rt, symbol: &str) -> Result<Option<Market>, Thrown> {
    let coin = text(&rt.this.get("apiMarginCoin")?)?;
    let markets = rt.shared.store.markets();
    let by_symbol = rt.shared.store.markets_by_symbol();
    Ok(crate::market::resolve_market(symbol, &coin, &markets, |s| by_symbol.get(s)).cloned())
}

/// `this.normalizeBitgetSymbol(value)`
fn normalize_symbol(rt: &Rt, value: Option<&Value>) -> Result<String, ExchangeError> {
    let profile = match rt.shared.extra.variant {
        Variant::Classic => SymbolProfile::Usdt,
        Variant::Usdc => SymbolProfile::Usdc,
        Variant::Inverse => SymbolProfile::Inverse,
    };
    normalization::normalize_bitget_symbol(value, profile, testnet(rt)?, |stripped| {
        Ok(resolve_market(rt, stripped)?.map(|m| m.symbol))
    })
}

fn venue(rt: &Rt) -> Result<Rc<dyn Exchange>, Thrown> {
    rt.shared
        .sockets
        .venue
        .borrow()
        .as_ref()
        .and_then(Weak::upgrade)
        .ok_or_else(|| Thrown::error("Error", "Bitget exchange is not assembled"))
}

fn js_json(value: safe_cex_num::JsJsonValue) -> Js {
    use safe_cex_num::JsJsonValue as J;
    match value {
        J::Undefined => Js::Undefined,
        J::Null => Js::Null,
        J::Bool(b) => Js::Bool(b),
        J::Number(n) => Js::Number(n),
        J::String(s) => Js::str(&s),
        J::Array(items) => Js::array(items.into_iter().map(js_json).collect()),
        J::Object(fields) => {
            let out = Js::object(vec![]);
            for (k, v) in fields {
                let _ = out.define(&k, js_json(v));
            }
            out
        }
        J::BigInt(n) => Js::str(&n.to_string()),
    }
}

/// The mapper's object literal, in source key order, keeping the raw row as `info`.
fn position_object(p: &Position, info: Js, invalid_payload: bool) -> Result<Js, Thrown> {
    let opt_num = |v: Option<f64>| v.map_or(Js::Undefined, Js::Number);
    let mut fields = vec![
        ("id", Js::str(&p.id)),
        ("symbol", Js::str(&p.symbol)),
        ("side", from_typed(&p.side)?),
        ("entryPrice", Js::Number(p.entry_price)),
        ("notional", Js::Number(p.notional)),
        ("leverage", Js::Number(p.leverage)),
        ("unrealizedPnl", Js::Number(p.unrealized_pnl)),
        ("realizedPnl", opt_num((p.realized_pnl).into_value())),
        ("contracts", Js::Number(p.contracts)),
        ("liquidationPrice", Js::Number(p.liquidation_price)),
    ];
    if !invalid_payload {
        fields.push(("marginSize", opt_num((p.margin_size).into_value())));
    }
    fields.push((
        "positionMode",
        p.position_mode
            .as_ref()
            .map_or(Ok(Js::Undefined), from_typed)?,
    ));
    fields.push((
        "marginType",
        p.margin_type
            .as_ref()
            .map_or(Ok(Js::Undefined), from_typed)?,
    ));
    fields.push(("info", info));
    fields.push(("lastUpdate", opt_num((p.last_update).into_value())));
    Ok(Js::object(fields))
}

impl AccountPositionsHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn receiver(&self, name: &str) -> Result<Js, Thrown> {
        self.path(name)
    }
    fn property(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        value.get(name)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn same_value_zero(&self, left: &Js, right: &Js) -> bool {
        left.same_value_zero(right)
    }
    fn equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn text(&self, value: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn upper(&self, value: &Js) -> Result<Js, Thrown> {
        let method = value.get("toUpperCase")?;
        if !method.is_callable() {
            return Err(Thrown::type_error(&format!(
                "{}.toUpperCase is not a function",
                value.to_display()
            )));
        }
        method.call(value.clone(), vec![])
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn string(&self, value: &[u16]) -> Js {
        Js::utf16(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn is_array(&self, value: &Js) -> bool {
        value.is_array()
    }
    fn length(&self, array: &Js) -> Result<usize, Thrown> {
        let length = array.get("length")?.to_number()?;
        Ok(if length.is_finite() && length > 0.0 {
            length as usize
        } else {
            0
        })
    }
    fn has_index(&self, array: &Js, index: usize) -> Result<bool, Thrown> {
        Ok(array.has_own(&index.to_string()))
    }
    fn index(&self, array: &Js, index: usize) -> Result<Js, Thrown> {
        array.get(&index.to_string())
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn keyed_property(&self, object: &Js, key: &Js) -> Result<Js, Thrown> {
        object.get_key(&key.to_js_string()?)
    }
    fn spread_metadata(&self, target: Js, metadata: Js) -> Result<Js, Thrown> {
        if !metadata.is_nullish() {
            for (k, v) in metadata.own_entries()? {
                target.define_key(&k, v)?;
            }
        }
        Ok(target)
    }
    fn numeric(&self, value: &Js) -> Result<f64, Thrown> {
        value.to_number()
    }
    fn define_key(&self, object: &Js, key: &Js, value: Js) -> Result<(), Thrown> {
        object.define_key(&key.to_js_string()?, value)
    }
    fn set_key(&self, object: &Js, key: &Js, value: Js) -> Result<(), Thrown> {
        object.set_key(&key.to_js_string()?, value)
    }
    fn await_value(&self, value: Js) -> AccountFuture<'_, Js, Thrown> {
        Box::pin(await_js(value))
    }
    fn call(&self, name: &str, args: Vec<Js>) -> Result<AccountFuture<'_, Js, Thrown>, Thrown> {
        let value = self.call_path(name, args)?;
        Ok(Box::pin(await_js(value)))
    }
    fn read(
        &self,
        client: Js,
        endpoint: &'static str,
        params: Vec<(&'static str, Js)>,
    ) -> Result<AccountFuture<'_, Js, Thrown>, Thrown> {
        let url = self::endpoint(endpoint)?;
        // The source omits the request config entirely where it passes no params.
        let params = (!params.is_empty()).then(|| Js::object(params));
        get_data(&client, url, params)
    }
    fn finite(&self, value: &Js, fallback: f64) -> Result<f64, Thrown> {
        Ok(finite(value, fallback))
    }
    fn parse_float(&self, value: &Js) -> Result<f64, Thrown> {
        value.parse_float()
    }
    fn parse_int(&self, value: &Js, radix: Option<i32>) -> Result<f64, Thrown> {
        Ok(safe_cex_num::parse_int(&text(value)?, radix))
    }
    // ts: isBitgetSuccessCode — parseInt(`${code ?? 0}`, 10) === 0
    fn success(&self, code: &Js) -> Result<bool, Thrown> {
        let code = if code.is_nullish() {
            "0".to_owned()
        } else {
            text(code)?
        };
        Ok(safe_cex_num::parse_int(&code, Some(10)) == 0.0)
    }
    fn promote(&self, response: &Js) -> Result<bool, Thrown> {
        Ok(self
            .call_path(
                "promoteUtaRoutingFromClassicUnsupported",
                vec![response.clone()],
            )?
            .truthy())
    }
    fn account_upnl(&self, fallback: f64) -> Result<f64, Thrown> {
        self.call_path("calculateAccountUpnl", vec![Js::Number(fallback)])?
            .to_number()
    }
    // ts: getBitgetApiErrorMessage — payload?.msg || fallback
    fn error_message(&self, response: &Js, fallback: &str) -> Result<Js, Thrown> {
        let msg = opt(response, "msg")?;
        Ok(if msg.truthy() { msg } else { Js::str(fallback) })
    }
    // ts: isBitgetEnvironmentMismatch — message?.toLowerCase().includes(...) ?? false
    fn environment_mismatch(&self, message: &Js) -> Result<bool, Thrown> {
        if message.is_nullish() {
            return Ok(false);
        }
        let lower = message.get("toLowerCase")?;
        if !lower.is_callable() {
            return Err(Thrown::type_error("message?.toLowerCase is not a function"));
        }
        let lowered = lower.call(message.clone(), vec![])?;
        Ok(lowered
            .call_method("includes", vec![Js::str("environment is incorrect")])?
            .truthy())
    }
    fn emit(&self, error: Js, detail: Option<Js>) -> Result<(), Thrown> {
        let mut args = vec![error];
        args.extend(detail);
        self.call_path("emitAndLogError", args).map(|_| ())
    }
    fn make_error(&self, message: Js) -> Thrown {
        let message = if message.is_undefined() {
            String::new()
        } else {
            message.to_display()
        };
        Thrown::new(error_object("Error", &message))
    }
    fn caught_value(&self, error: &Thrown) -> Js {
        error.value.clone()
    }
    // ts: err?.response?.data?.msg || err?.message
    fn caught_detail(&self, error: &Thrown) -> Result<Js, Thrown> {
        let msg = opt(&opt(&opt(&error.value, "response")?, "data")?, "msg")?;
        if msg.truthy() {
            Ok(msg)
        } else {
            opt(&error.value, "message")
        }
    }
    fn map_positions(&self, array: Js, uta: bool) -> Result<Js, Thrown> {
        let map = array.get("map")?;
        let mapper = self
            .this
            .get(if uta { "mapUtaPosition" } else { "mapPosition" })?;
        if !map.is_callable() {
            return Err(Thrown::type_error("data.map is not a function"));
        }
        map.call(array, vec![mapper])
    }
    // ts: createPositionId — mode === OneWay || !side ? symbol : `${symbol}:${side}`
    fn position_id(&self, symbol: &Js, side: &Js, mode: &Js) -> Result<Js, Thrown> {
        if mode.strict_equals(&Js::str("oneway")) || !side.truthy() {
            return Ok(symbol.clone());
        }
        let mut id = symbol.to_js_string()?.to_vec();
        id.push(b':' as u16);
        id.extend_from_slice(&side.to_js_string()?);
        Ok(Js::utf16(&id))
    }
}

impl InversePositionsHost for Rt {
    fn capture_replace(&self, receiver: Js) -> Result<Js, Thrown> {
        crate::binding::hosts::assembly::bound_member(&receiver, "replace")
    }
    fn invoke_replace(&self, method: Js, needle: Js, replacement: Js) -> Result<Js, Thrown> {
        if !method.is_callable() {
            return Err(Thrown::type_error("raw.symbol.replace is not a function"));
        }
        method.call(Js::Undefined, vec![needle, replacement])
    }
    fn sync_call(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.call_path(name, args)
    }
}

/// The imported journal fold, reading fills and income through the JS receiver.
struct ReceiverFolder {
    this: Js,
}

impl JournalFolder for ReceiverFolder {
    fn build_folded_position_records<'a>(
        &'a self,
        _exchange: &'a dyn Exchange,
        account: &'a str,
        from: f64,
        to: f64,
        symbol_scoped: bool,
        resolve_settle_currency: Box<dyn Fn(&str) -> String + 'a>,
    ) -> ExchangeFuture<'a, Vec<Value>> {
        Box::pin(async move {
            let this = self.this.clone();
            let fills_this = this.clone();
            let mut fetch_fills = |opts: ExecutionsOptions| -> ExchangeFuture<'_, Vec<Execution>> {
                let this = fills_this.clone();
                Box::pin(async move {
                    let call =
                        this.call_method("fetchHistoricalFills", vec![from_typed(&opts)?])?;
                    Ok(to_typed::<Vec<Execution>>(&await_js(call).await?)?)
                })
            };
            let income_this = this.clone();
            let mut fetch_income = |from: f64, to: f64| -> ExchangeFuture<'_, Vec<IncomeRecord>> {
                let this = income_this.clone();
                Box::pin(async move {
                    let call = this.call_method(
                        "fetchIncomeHistoryRange",
                        vec![Js::Number(from), Js::Number(to)],
                    )?;
                    Ok(to_typed::<Vec<IncomeRecord>>(&await_js(call).await?)?)
                })
            };
            let has_income = this.get("fetchIncomeHistoryRange")?.truthy();
            let mut settle = |symbol: &str| resolve_settle_currency(symbol);
            let result = build_folded_position_records(
                &mut FoldableExchange {
                    fetch_historical_fills: &mut fetch_fills,
                    fetch_income_history_range: if has_income {
                        Some(&mut fetch_income)
                    } else {
                        None
                    },
                },
                &mut BuildRecordsOptions {
                    account,
                    from,
                    to,
                    symbol_scoped,
                    resolve_settle_currency: &mut settle,
                },
            )
            .await
            .map_err(|e| e.error)?;
            result
                .records
                .iter()
                .map(|r| serde_json::to_value(r).map_err(|e| ExchangeError::new(e.to_string())))
                .collect()
        })
    }
}

fn sync_member(
    rt: &Rt,
    name: &str,
    body: impl Fn(&Rt, Vec<Js>) -> Result<Js, Thrown> + 'static,
) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, a| body(&rt, a))
}

fn async_member(
    rt: &Rt,
    name: &str,
    body: impl Fn(Rt, Vec<Js>) -> JsFuture<'static> + 'static,
) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, a| Ok(rt.spawn(body(rt.clone(), a))))
}

/// `this._fetchFillsPage(params)` projected to the body's page tuple.
async fn fills_page(
    rt: &Rt,
    params: safe_cex_num::JsJsonValue,
) -> Result<(Vec<Value>, Option<String>), Thrown> {
    page_rows(rt, "_fetchFillsPage", "endId", params).await
}

/// One fills page through the receiver's page member: `{ rows, <cursor> }`.
async fn page_rows(
    rt: &Rt,
    member: &str,
    cursor: &str,
    params: safe_cex_num::JsJsonValue,
) -> Result<(Vec<Value>, Option<String>), Thrown> {
    let page = await_js(rt.call_path(member, vec![js_json(params)])?).await?;
    let rows = page.get("rows")?;
    let rows = rows
        .array_values()
        .ok_or_else(|| Thrown::type_error(&format!("{member} rows is not an array")))?
        .iter()
        .map(to_json)
        .collect::<Result<_, _>>()?;
    let next = page.get(cursor)?;
    Ok((
        rows,
        if next.is_nullish() {
            None
        } else {
            Some(text(&next)?)
        },
    ))
}

pub(crate) fn register(_globals: &Js, _weak: Weak<Shared>) {}

pub(crate) fn bind(
    rt: &Rt,
    _owner: &str,
    name: &str,
    targets: &[&str],
) -> Result<Option<Js>, Thrown> {
    let Some(target) = targets.first() else {
        return Ok(None);
    };
    let target = target
        .strip_prefix("safe-cex-venue-bitget/src/")
        .unwrap_or(target);
    Ok(Some(match target {
        "pnl.rs::calculate_account_upnl" => sync_member(rt, name, |rt, a| {
            let api = finite(&arg(&a, 0), 0.0);
            let positions = rt.shared.store.positions();
            let tickers = rt.shared.store.tickers_by_symbol();
            Ok(Js::Number(crate::pnl::calculate_account_upnl(
                api,
                &positions,
                |s| tickers.get(s).map(|t| t.last),
            )))
        }),
        "account_positions.rs::fetch_balance" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let opts = arg(&a, 0);
                if matches!(opts, Js::Null) {
                    return Err(Thrown::type_error(
                        "Cannot destructure property 'preferFast' of 'object null' as it is null.",
                    ));
                }
                let prefer = opt(&opts, "preferFast")?;
                account_positions::fetch_balance(&rt, !prefer.is_undefined() && prefer.truthy())
                    .await
            })
        }),
        "account_positions.rs::fetch_uta_balance" => async_member(rt, name, |rt, a| {
            Box::pin(async move { account_positions::fetch_uta_balance(&rt, arg(&a, 0)).await })
        }),
        "account_positions.rs::fetch_position_metadata" => async_member(rt, name, |rt, a| {
            Box::pin(
                async move { account_positions::fetch_position_metadata(&rt, arg(&a, 0)).await },
            )
        }),
        "account_positions.rs::fetch_uta_position_metadata" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                account_positions::fetch_uta_position_metadata(&rt, arg(&a, 0)).await
            })
        }),
        "account_positions.rs::fetch_positions" => async_member(rt, name, |rt, a| {
            Box::pin(async move { account_positions::fetch_positions(&rt, Some(arg(&a, 0))).await })
        }),
        "account_positions.rs::fetch_open_positions" => async_member(rt, name, |rt, _| {
            Box::pin(async move { account_positions::fetch_open_positions(&rt).await })
        }),
        "account_positions.rs::fetch_uta_open_positions" => async_member(rt, name, |rt, _| {
            Box::pin(async move { account_positions::fetch_uta_open_positions(&rt).await })
        }),
        "account_positions.rs::fetch_inverse_balance" => async_member(rt, name, |rt, _| {
            Box::pin(async move { account_positions::fetch_inverse_balance(&rt).await })
        }),
        "inverse_positions.rs::fetch_open_positions" => async_member(rt, name, |rt, _| {
            Box::pin(async move { inverse_positions::fetch_open_positions(&rt).await })
        }),
        "normalization.rs::resolve_liquidation_price" => sync_member(rt, name, |rt, a| {
            let id = arg(&a, 0);
            let raw = optional_json(&arg(&a, 1))?;
            let positions = rt.shared.store.positions();
            // Stored ids are strings: a non-string id strictly equals none of them.
            let (id, stored): (String, &[Position]) = match &id {
                Js::String(s) => (String::from_utf16_lossy(s), &positions),
                _ => (String::new(), &[]),
            };
            Ok(Js::Number(normalization::resolve_liquidation_price(
                &id,
                raw.as_ref(),
                stored,
            )?))
        }),
        "positions.rs::map_uta_position" | "positions.rs::map_position" => {
            let uta = target == "positions.rs::map_uta_position";
            sync_member(rt, name, move |rt, a| {
                let p = arg(&a, 0);
                let raw = optional_json(&p)?;
                let invalid = !uta && !matches!(raw, Some(Value::Object(_) | Value::Array(_)));
                let positions = rt.shared.store.positions();
                let symbol = |v: Option<&Value>| normalize_symbol(rt, v);
                let mapped = if uta {
                    crate::positions::map_uta_position(raw.as_ref(), symbol, &positions, || {
                        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
                    })?
                } else {
                    crate::positions::map_position(raw.as_ref(), symbol, &positions, || {
                        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
                    })?
                };
                drop(positions);
                position_object(&mapped, p, invalid)
            })
        }
        "numeric.rs::to_finite_number" => sync_member(rt, name, |_, a| {
            let fallback = arg(&a, 1);
            let fallback = if fallback.is_undefined() {
                Js::Number(0.0)
            } else {
                fallback
            };
            // NaN marks the source's `return fallback`, which returns the raw argument.
            let parsed = finite(&arg(&a, 0), f64::NAN);
            Ok(if parsed.is_nan() {
                fallback
            } else {
                Js::Number(parsed)
            })
        }),
        "stream_positions.rs::get_position_payload_update_time" => sync_member(rt, name, |_, a| {
            let info = arg(&a, 0).get("info")?;
            let probe = Position {
                id: String::new(),
                symbol: String::new(),
                side: (safe_cex_types::PositionSide::Long).into(),
                entry_price: 0.0,
                notional: 0.0,
                leverage: 0.0,
                margin_type: (None).into(),
                position_mode: (None).into(),
                unrealized_pnl: 0.0,
                realized_pnl: (None).into(),
                contracts: 0.0,
                liquidation_price: 0.0,
                margin_size: (None).into(),
                info: if info.is_nullish() {
                    (None).into()
                } else {
                    (Some(safe_cex_exchange::js_runtime::value::to_retained_input(
                        &info,
                    )?))
                    .into()
                },
                extra: (None).into(),
                last_update: (None).into(),
                option: (None).into(),
            };
            Ok(
                crate::stream_positions::get_position_payload_update_time(&probe)
                    .map_or(Js::Undefined, Js::Number),
            )
        }),
        "stream_positions.rs::merge_rest_positions_with_fresh_ws_state" => {
            sync_member(rt, name, |rt, a| {
                let rest_js = arg(&a, 0)
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("restPositions is not iterable"))?;
                let current_js = rt
                    .path("store.positions")?
                    .array_values()
                    .unwrap_or_default();
                let rest: Vec<Position> = rest_js.iter().map(to_typed).collect::<Result<_, _>>()?;
                let current: Vec<Position> =
                    current_js.iter().map(to_typed).collect::<Result<_, _>>()?;
                let snapshot_ids: HashSet<String> =
                    match rt.this.get("lastWsPositionSnapshotIds")? {
                        Js::Set(set) => set
                            .borrow()
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| String::from_utf16_lossy(s)))
                            .collect(),
                        _ => {
                            return Err(Thrown::type_error(
                                "this.lastWsPositionSnapshotIds.has is not a function",
                            ));
                        }
                    };
                let updated_at: HashMap<String, f64> =
                    match rt.this.get("wsPositionUpdatedAtById")? {
                        Js::Map(map) => {
                            let mut out = HashMap::new();
                            for (k, v) in map.borrow().iter() {
                                if let Js::String(k) = k {
                                    let v = if v.is_nullish() { 0.0 } else { v.to_number()? };
                                    out.insert(String::from_utf16_lossy(k), v);
                                }
                            }
                            out
                        }
                        _ => {
                            return Err(Thrown::type_error(
                                "this.wsPositionUpdatedAtById.get is not a function",
                            ));
                        }
                    };
                let merged = crate::stream_positions::merge_rest_positions_with_fresh_ws_state(
                    &rest,
                    &current,
                    arg(&a, 1).to_number()?,
                    rt.this.get("lastWsPositionSnapshotAt")?.to_number()?,
                    &snapshot_ids,
                    &updated_at,
                    rt.this.get("positionRestStaleGuardMs")?.to_number()?,
                );
                // Hand back the original objects the selected records were decoded from.
                let mut out = Vec::with_capacity(merged.len());
                for picked in merged {
                    let original = rest
                        .iter()
                        .position(|p| std::ptr::eq(p, picked))
                        .map(|i| rest_js[i].clone())
                        .or_else(|| {
                            current
                                .iter()
                                .position(|p| std::ptr::eq(p, picked))
                                .map(|i| current_js[i].clone())
                        })
                        .ok_or_else(|| {
                            Thrown::error("Error", "merged position is not from either input")
                        })?;
                    out.push(original);
                }
                Ok(Js::array(out))
            })
        }
        "history_reads.rs::fetch_historical_positions" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let opts = arg(&a, 0);
                let account = text(&opts.get("account")?)?;
                let bound = |k: &str| -> Result<Option<f64>, Thrown> {
                    let v = opts.get(k)?;
                    Ok(if v.is_nullish() {
                        None
                    } else {
                        Some(v.to_number()?)
                    })
                };
                let options = HistoricalPositionOptions {
                    account,
                    from: bound("from")?,
                    to: bound("to")?,
                };
                let venue = venue(&rt)?;
                let folder = ReceiverFolder {
                    this: rt.this.clone(),
                };
                let records = crate::history_reads::fetch_historical_positions(
                    venue.as_ref(),
                    &folder,
                    options,
                )
                .await?;
                Ok(Js::array(records.iter().map(from_json).collect()))
            })
        }),
        "account_read.rs::get_account" => async_member(rt, name, |rt, _| {
            Box::pin(async move {
                let account = crate::account_read::get_account(|url| {
                    let rt = rt.clone();
                    async move {
                        let data = get_data(&rt.this.get("xhr")?, url, None)?.await?;
                        optional_json(&data)
                    }
                })
                .await?;
                from_typed(&account)
            })
        }),
        "account_predicates.rs::promote_uta_routing_from_classic_unsupported" => {
            sync_member(rt, name, |rt, a| {
                let payload = arg(&a, 0);
                let is_uta = Cell::new(false);
                let detected = Cell::new(false);
                let promoted = account_predicates::promote_uta_routing_from_classic_unsupported(
                    Some(&payload),
                    |payload| {
                        let payload = payload.cloned().unwrap_or(Js::Undefined);
                        // ts: `${payload?.code ?? ''}` and payload?.msg?.toLowerCase() ?? ''
                        let code = opt(&payload, "code")?;
                        let code = match code {
                            Js::Undefined | Js::Null => None,
                            Js::Number(n) => Some(ApiCode::Number(n)),
                            other => Some(ApiCode::Text(text(&other)?)),
                        };
                        let msg = opt(&payload, "msg")?;
                        let msg = if msg.is_nullish() {
                            None
                        } else {
                            let lower = msg.get("toLowerCase")?;
                            if !lower.is_callable() {
                                return Err(Thrown::type_error(
                                    "payload?.msg?.toLowerCase is not a function",
                                ));
                            }
                            Some(text(&lower.call(msg.clone(), vec![])?)?)
                        };
                        Ok(account_predicates::is_bitget_uta_classic_unsupported(
                            code.as_ref(),
                            msg.as_deref(),
                        ))
                    },
                    || Ok(rt.this.get("supportsUtaRouting")?.truthy()),
                    || testnet(rt),
                    &is_uta,
                    &detected,
                )?;
                if is_uta.get() {
                    rt.this.set("isUta", Js::Bool(true))?;
                }
                if detected.get() {
                    rt.this.set("_accountTypeDetected", Js::Bool(true))?;
                }
                Ok(Js::Bool(promoted))
            })
        }
        "history_reads.rs::fetch_bill_page" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let opts = optional_json(&arg(&a, 0))?;
                let public_only = rt.this.get("isPublicDataOnly")?.truthy();
                let product = text(&rt.this.get("apiProductType")?)?;
                let records = crate::history_reads::fetch_bill_page::<Thrown, _>(
                    public_only,
                    &product,
                    opts.as_ref(),
                    |symbol| {
                        let symbol = safe_cex_num::js_string(Some(symbol))?;
                        Ok(resolve_market(&rt, &symbol)?.map(|m| Value::String(m.id)))
                    },
                    |url, params| {
                        let rt = rt.clone();
                        async move {
                            let data =
                                get_data(&rt.this.get("xhr")?, url, Some(js_json(params)))?.await?;
                            optional_json(&data)
                        }
                    },
                )
                .await?;
                from_typed(&records)
            })
        }),
        "history_reads.rs::fetch_income_history" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let records = crate::history_reads::fetch_income_history(
                    arg(&a, 0),
                    |opts| {
                        let rt = rt.clone();
                        async move {
                            let page =
                                await_js(rt.call_path("_fetchBillPage", vec![opts])?).await?;
                            to_typed::<Vec<IncomeRecord>>(&page)
                        }
                    },
                    |error: &Thrown| {
                        if error.value.is_undefined() {
                            return None;
                        }
                        Some(
                            to_json(&error.value)
                                .unwrap_or_else(|_| Value::String(error.value.to_display())),
                        )
                    },
                    |message| {
                        rt.call_path("emitAndLogError", vec![Js::str(&message)])
                            .map(|_| ())
                    },
                )
                .await?;
                from_typed(&records)
            })
        }),
        "history_reads.rs::fetch_fills_page" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let params = to_json(&arg(&a, 0))?;
                let (rows, end_id) =
                    crate::history_reads::fetch_fills_page::<Thrown, _>(params, |url, params| {
                        let rt = rt.clone();
                        async move {
                            let data =
                                get_data(&rt.this.get("xhr")?, url, Some(from_json(&params)))?
                                    .await?;
                            optional_json(&data)
                        }
                    })
                    .await?;
                Ok(Js::object(vec![
                    ("rows", Js::array(rows.iter().map(from_json).collect())),
                    ("endId", end_id.map_or(Js::Undefined, |id| Js::str(&id))),
                ]))
            })
        }),
        "history_reads.rs::fetch_historical_fills" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let public_only = rt.this.get("isPublicDataOnly")?.truthy();
                let original = arg(&a, 0);
                let opts: ExecutionsOptions = to_typed(&original)?;
                // The page error's JS value (response.data.msg) survives the typed error channel here.
                let page_error: Rc<RefCell<Option<Thrown>>> = Rc::default();
                let executions = crate::history_reads::fetch_historical_fills(
                    public_only,
                    &opts,
                    || {
                        let rt = rt.clone();
                        async move {
                            await_js(rt.call_path("ensureAccountTypeDetected", vec![])?).await?;
                            Ok(rt.this.get("usesUtaRouting")?.truthy())
                        }
                    },
                    |_| {
                        let rt = rt.clone();
                        async move {
                            let rows =
                                await_js(rt.call_path("_fetchHistoricalUtaFills", vec![original])?)
                                    .await?;
                            to_typed::<Vec<safe_cex_types::Execution>>(&rows)
                                .map_err(ExchangeError::from)
                        }
                    },
                    |symbol| Ok(resolve_market(&rt, symbol)?.map(|m| (m.id, m.symbol))),
                    || Ok(text(&rt.this.get("apiProductType")?)?),
                    |params| {
                        let rt = rt.clone();
                        let page_error = page_error.clone();
                        async move {
                            fills_page(&rt, params).await.map_err(|thrown| {
                                let error = thrown.to_exchange_error();
                                *page_error.borrow_mut() = Some(thrown);
                                error
                            })
                        }
                    },
                    (
                        |error: &ExchangeError| match page_error.borrow_mut().take() {
                            Some(thrown) if thrown.value.is_undefined() => None,
                            Some(thrown) => Some(
                                to_json(&thrown.value)
                                    .unwrap_or_else(|_| Value::String(thrown.value.to_display())),
                            ),
                            None => Some(serde_json::json!({ "message": error.message })),
                        },
                        |message: Option<Value>| {
                            let message = message.as_ref().map_or(Js::Undefined, from_json);
                            rt.call_path("emitAndLogError", vec![message])
                                .map(|_| ())
                                .map_err(ExchangeError::from)
                        },
                    ),
                )
                .await?;
                from_typed(&executions)
            })
        }),
        "history_reads.rs::fetch_uta_fills_page" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let params = to_js_json_value(&arg(&a, 0))?;
                let (rows, cursor) = crate::history_reads::fetch_uta_fills_page::<Thrown, _>(
                    params,
                    |url, params| {
                        let rt = rt.clone();
                        async move {
                            let data =
                                get_data(&rt.this.get("xhr")?, url, Some(js_json(params)))?.await?;
                            optional_json(&data)
                        }
                    },
                )
                .await?;
                Ok(Js::object(vec![
                    ("rows", Js::array(rows.iter().map(from_json).collect())),
                    ("cursor", cursor.map_or(Js::Undefined, |c| Js::str(&c))),
                ]))
            })
        }),
        "history_reads.rs::fetch_historical_uta_fills" => async_member(rt, name, |rt, a| {
            Box::pin(async move {
                let opts: ExecutionsOptions = to_typed(&arg(&a, 0))?;
                let page_error: Rc<RefCell<Option<Thrown>>> = Rc::default();
                let executions = crate::history_reads::fetch_historical_uta_fills(
                    &opts,
                    |symbol| Ok(resolve_market(&rt, symbol)?.map(|m| (m.id, m.symbol))),
                    || Ok(text(&rt.this.get("apiUtaCategory")?)?),
                    |params| {
                        let rt = rt.clone();
                        let page_error = page_error.clone();
                        async move {
                            page_rows(&rt, "_fetchUtaFillsPage", "cursor", params)
                                .await
                                .map_err(|thrown| {
                                    let error = thrown.to_exchange_error();
                                    *page_error.borrow_mut() = Some(thrown);
                                    error
                                })
                        }
                    },
                    (
                        |error: &ExchangeError| match page_error.borrow_mut().take() {
                            Some(thrown) if thrown.value.is_undefined() => None,
                            Some(thrown) => Some(
                                to_json(&thrown.value)
                                    .unwrap_or_else(|_| Value::String(thrown.value.to_display())),
                            ),
                            None => Some(serde_json::json!({ "message": error.message })),
                        },
                        |message: Option<Value>| {
                            let message = message.as_ref().map_or(Js::Undefined, from_json);
                            rt.call_path("emitAndLogError", vec![message])
                                .map(|_| ())
                                .map_err(ExchangeError::from)
                        },
                    ),
                )
                .await?;
                from_typed(&executions)
            })
        }),
        _ => return Ok(None),
    }))
}

fn to_js_json_value(value: &Js) -> Result<safe_cex_num::JsJsonValue, Thrown> {
    super::assembly::to_js_json(value)
}
