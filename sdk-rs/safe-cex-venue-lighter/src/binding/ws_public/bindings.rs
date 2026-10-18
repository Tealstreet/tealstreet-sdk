//! Module bindings lighter.ws-public.ts imports that no other installer binds:
//! the ws-policy/parsers/ohlcv helpers, config, json-parse, transport routing,
//! lodash/flatten and the host timer/clock globals the socket bodies call.
use super::super::{
    ops::{Arithmetic, arithmetic, to_string},
    realm::Realm,
};
use crate::{
    ws_policy::{calculate_lighter_public_reconnect_delay, classify_lighter_public_close},
    ws_public::normalize_lighter_funding_rate,
    ws_public_ohlcv::{
        build_lighter_candle_topic, build_lighter_trades_topic,
        should_attempt_lighter_ohlcv_recovery, should_promote_lighter_ohlcv_recovery_to_proxy,
    },
    ws_public_parsers::{
        extract_lighter_ticker_market_id_from_channel, parse_lighter_ws_book_side_price,
        parse_lighter_ws_number, should_dispatch_lighter_public_message,
    },
};
use safe_cex_candle_cache::calendar::get_candle_start_time;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg, from_typed};
use safe_cex_num::PrecisionInput;
use serde_json::Value;

fn text(value: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&to_string(value)?))
}

/// `${marketId}` operand: numbers keep their JS number text.
fn precision(value: &Js) -> Result<PrecisionInput, Thrown> {
    Ok(match value {
        Js::Number(n) => PrecisionInput::Number(*n),
        other => PrecisionInput::String(text(other)?),
    })
}

/// A primitive the ws parsers inspect; anything else is skipped by both sides.
fn parser_scalar(value: &Js) -> Value {
    match value {
        Js::Number(n) if n.is_finite() => {
            serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number)
        }
        Js::String(s) => Value::String(String::from_utf16_lossy(s)),
        _ => Value::Null,
    }
}

fn function(name: &str, f: impl Fn(Vec<Js>) -> Result<Js, Thrown> + 'static) -> Js {
    Js::function(name, move |_, args| f(args))
}

fn helpers() -> Vec<(&'static str, Js)> {
    vec![
        (
            "buildLighterCandleTopic",
            function("buildLighterCandleTopic", |a| {
                let id = precision(&arg(&a, 0))?;
                Ok(Js::str(&build_lighter_candle_topic(
                    id,
                    &text(&arg(&a, 1))?,
                )))
            }),
        ),
        (
            "buildLighterTradesTopic",
            function("buildLighterTradesTopic", |a| {
                Ok(Js::str(&build_lighter_trades_topic(precision(&arg(
                    &a, 0,
                ))?)))
            }),
        ),
        (
            "shouldAttemptLighterOhlcvRecovery",
            function("shouldAttemptLighterOhlcvRecovery", |a| {
                let p = arg(&a, 0);
                Ok(Js::Bool(should_attempt_lighter_ohlcv_recovery(
                    p.get("hasTopicHandlers")?.truthy(),
                    p.get("nowMs")?.to_number()?,
                    p.get("lastRecoveryReconnectAtMs")?.to_number()?,
                    p.get("recoveryCooldownMs")?.to_number()?,
                )))
            }),
        ),
        (
            "shouldPromoteLighterOhlcvRecoveryToProxy",
            function("shouldPromoteLighterOhlcvRecoveryToProxy", |a| {
                let p = arg(&a, 0);
                Ok(Js::Bool(should_promote_lighter_ohlcv_recovery_to_proxy(
                    p.get("isWsProxyRouteEnabled")?.truthy(),
                    p.get("useProxyRoute")?.truthy(),
                    p.get("connectedViaProxyRoute")?.truthy(),
                )))
            }),
        ),
        (
            "classifyLighterPublicClose",
            function("classifyLighterPublicClose", |a| {
                let code = arg(&a, 0).as_f64();
                let reason = arg(&a, 1);
                let reason = if reason.truthy() {
                    text(&reason)?
                } else {
                    String::new()
                };
                from_typed(&classify_lighter_public_close(code, &reason))
            }),
        ),
        (
            "calculateLighterPublicReconnectDelay",
            function("calculateLighterPublicReconnectDelay", |a| {
                let p = arg(&a, 0);
                Ok(Js::Number(calculate_lighter_public_reconnect_delay(
                    p.get("baseDelayMs")?.to_number()?,
                    p.get("failureCount")?.to_number()?,
                    p.get("isRateLimited")?.truthy(),
                    p.get("rateLimitReconnectDelayMs")?.to_number()?,
                )))
            }),
        ),
        (
            "shouldDispatchLighterPublicMessage",
            function("shouldDispatchLighterPublicMessage", |a| {
                let (topic, data) = (text(&arg(&a, 0))?, text(&arg(&a, 1))?);
                Ok(Js::Bool(should_dispatch_lighter_public_message(
                    &topic, &data,
                )))
            }),
        ),
        (
            "parseLighterWsNumber",
            function("parseLighterWsNumber", |a| {
                let values: Vec<Value> = a.iter().map(parser_scalar).collect();
                Ok(parse_lighter_ws_number(&values).map_or(Js::Undefined, Js::Number))
            }),
        ),
        (
            "parseLighterWsBookSidePrice",
            function("parseLighterWsBookSidePrice", |a| {
                let value = arg(&a, 0);
                if !value.truthy() {
                    return Ok(Js::Undefined);
                }
                let shaped = match &value {
                    Js::Array(_) => Value::Array(
                        value
                            .array_values()?
                            .unwrap_or_default()
                            .iter()
                            .take(1)
                            .map(parser_scalar)
                            .collect(),
                    ),
                    Js::Object(_) | Js::Function(_) => {
                        let mut fields = serde_json::Map::new();
                        for key in ["price", "p", "bestPrice", "best_price"] {
                            fields.insert(key.into(), parser_scalar(&value.get(key)?));
                        }
                        Value::Object(fields)
                    }
                    other => parser_scalar(other),
                };
                Ok(parse_lighter_ws_book_side_price(&shaped).map_or(Js::Undefined, Js::Number))
            }),
        ),
        (
            "extractLighterTickerMarketIdFromChannel",
            function("extractLighterTickerMarketIdFromChannel", |a| {
                let channel = arg(&a, 0);
                let channel = if channel.truthy() {
                    text(&channel)?
                } else {
                    String::new()
                };
                Ok(extract_lighter_ticker_market_id_from_channel(&channel)
                    .map_or(Js::Undefined, Js::str))
            }),
        ),
        (
            "normalizeLighterFundingRate",
            function("normalizeLighterFundingRate", |a| match arg(&a, 0) {
                Js::Number(rate) => Ok(Js::Number(normalize_lighter_funding_rate(rate))),
                // `rate === 0.0012` is false for every non-Number operand.
                other => arithmetic(
                    Arithmetic::Divide,
                    &arithmetic(Arithmetic::Multiply, &other, &Js::Number(8.0))?,
                    &Js::Number(100.0),
                ),
            }),
        ),
        (
            "getCandleStartTime",
            function("getCandleStartTime", |a| {
                let interval = text(&arg(&a, 0))?;
                Ok(Js::Number(get_candle_start_time(
                    &interval,
                    arg(&a, 1).to_number()?,
                )))
            }),
        ),
    ]
}

fn define_missing(
    globals: &Js,
    name: &str,
    value: impl FnOnce() -> Result<Js, Thrown>,
) -> Result<(), Thrown> {
    if globals.has_own(name) {
        return Ok(());
    }
    globals.define(name, value()?)
}

pub(super) fn install(realm: &Realm) -> Result<(), Thrown> {
    let shared = &realm.0;
    let globals = &shared.globals;
    for (name, value) in helpers() {
        define_missing(globals, name, || Ok(value))?;
    }
    define_missing(globals, "flatten", || globals.get("lodash")?.get("flatten"))?;
    Ok(())
}
