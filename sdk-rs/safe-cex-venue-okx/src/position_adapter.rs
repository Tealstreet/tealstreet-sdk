//! One OKX row is one round trip; restore gross PnL and retain signed rebates/funding.
use crate::income::{JsCoercionError, js_string, nullish_string, truthy, truthy_string};
use safe_cex_num::JsJsonValue;
use safe_cex_types::object_map::JsObjectMap;
use serde_json::{Map, Value};

pub fn num(value: &Value) -> Result<f64, JsCoercionError> {
    crate::income::to_number(value)
}

pub struct OkxPositionAdapterDeps<'a, E = JsCoercionError> {
    pub account: &'a str,
    pub to_base: &'a dyn Fn(f64, &str) -> Result<f64, E>,
    pub resolve_symbol: &'a dyn Fn(&str) -> Result<Option<String>, E>,
    pub resolve_settle_currency: &'a dyn Fn(&str) -> Result<Option<String>, E>,
}

fn number(value: f64) -> JsJsonValue {
    JsJsonValue::Number(value)
}

pub fn map_okx_position_history<E: From<JsCoercionError>>(
    row: &Map<String, Value>,
    deps: &OkxPositionAdapterDeps<'_, E>,
) -> Result<Option<JsObjectMap<JsJsonValue>>, E> {
    let get = |key: &str| row.get(key).unwrap_or(&Value::Null);
    let instrument = nullish_string(get("instId"), "")?;
    let Some(symbol) = (deps.resolve_symbol)(&instrument)?.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let side = if nullish_string(get("direction"), "")?.to_lowercase() == "short" {
        "short"
    } else {
        "long"
    };
    let close_time = num(get("uTime"))?;
    let open_time = num(get("cTime"))?;
    let closed_size = (deps.to_base)(num(get("closeTotalPos"))?.abs(), &instrument)?;
    if closed_size.is_nan() || closed_size <= 0.0 {
        return Ok(None);
    }
    let open_max = num(get("openMaxPos"))?.abs();
    let max_size = (open_max > 0.0)
        .then(|| (deps.to_base)(open_max, &instrument))
        .transpose()?;
    let fees = -num(get("fee"))?;
    let funding = num(get("fundingFee"))?;
    let realized_pnl = num(get("realizedPnl"))? + fees - funding;
    let raw_leg = nullish_string(get("posSide"), "")?.to_lowercase();
    let leg = match raw_leg.as_str() {
        "long" => "long",
        "short" => "short",
        _ => "both",
    };
    let position_id = nullish_string(get("posId"), "nopos")?;
    let mut record = JsObjectMap::new();
    record.insert(
        "key".into(),
        JsJsonValue::String(format!(
            "{}|{symbol}|{leg}|{position_id}|{}",
            deps.account,
            safe_cex_num::js_number_string(close_time)
        )),
    );
    record.insert("account".into(), JsJsonValue::String(deps.account.into()));
    record.insert("symbol".into(), JsJsonValue::String(symbol));
    let currency = truthy_string(get("ccy"))?;
    let currency = if currency.is_empty() {
        (deps.resolve_settle_currency)(&instrument)?
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "USDT".into())
    } else {
        currency
    };
    record.insert("settleCurrency".into(), JsJsonValue::String(currency));
    record.insert("side".into(), JsJsonValue::String(side.into()));
    record.insert("positionSide".into(), JsJsonValue::String(leg.into()));
    record.insert("state".into(), JsJsonValue::String("closed".into()));
    if open_time != 0.0 {
        record.insert("openedAt".into(), number(open_time));
    }
    if close_time != 0.0 {
        record.insert("closedAt".into(), number(close_time));
    }
    record.insert("closedSize".into(), number(closed_size));
    if let Some(size) = max_size {
        record.insert("maxSize".into(), number(size));
    }
    let leverage = num(get("lever"))?;
    if leverage != 0.0 {
        record.insert("leverage".into(), number(leverage));
    }
    if truthy(get("posId")) {
        record.insert(
            "venueId".into(),
            JsJsonValue::String(js_string(Some(get("posId")))?),
        );
    }
    record.insert("avgEntryPrice".into(), number(num(get("openAvgPx"))?));
    let exit = num(get("closeAvgPx"))?;
    if exit != 0.0 {
        record.insert("avgExitPrice".into(), number(exit));
    }
    record.insert("realizedPnl".into(), number(realized_pnl));
    record.insert("fees".into(), number(fees));
    record.insert("funding".into(), number(funding));
    record.insert("fundingAttributed".into(), JsJsonValue::Bool(true));
    // ts-bug: mapOkxPositionHistory inherits CLOSE_REASON prototype values.
    // Preserve their serialized shape: functions omit the field; __proto__ emits {}.
    match nullish_string(get("type"), "")?.as_str() {
        "3" | "4" => {
            record.insert(
                "closeReason".into(),
                JsJsonValue::String("liquidation".into()),
            );
        }
        "5" => {
            record.insert("closeReason".into(), JsJsonValue::String("adl".into()));
        }
        "__proto__" => {
            record.insert("closeReason".into(), JsJsonValue::Object(vec![]));
        }
        "constructor"
        | "toString"
        | "toLocaleString"
        | "valueOf"
        | "hasOwnProperty"
        | "isPrototypeOf"
        | "propertyIsEnumerable"
        | "__defineGetter__"
        | "__defineSetter__"
        | "__lookupGetter__"
        | "__lookupSetter__" => {}
        _ => {
            record.insert("closeReason".into(), JsJsonValue::String("user".into()));
        }
    }
    record.insert("provenance".into(), JsJsonValue::String("reported".into()));
    record.insert(
        "sourceAxis".into(),
        JsJsonValue::String("closed-positions".into()),
    );
    record.insert("closeCount".into(), number(1.0));
    Ok(Some(record))
}
