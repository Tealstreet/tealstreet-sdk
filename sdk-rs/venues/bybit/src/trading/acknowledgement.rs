//! Strict Bybit V5 write acknowledgements. Every level must be a JSON object,
//! codes are exact integers, IDs are bounded non-empty strings, and rows are
//! matched to the request by echo, never by position alone. A shape this module
//! rejects is a malformed acknowledgement: the engine records it as unproven.
use safe_cex_engine::classify::decode_int;
use serde::Deserialize;
use serde_json::value::RawValue;

/// Rows a batch acknowledgement may carry; larger answers are malformed.
const MAX_ROWS: usize = 500;
/// The engine's native ID bound; a longer ID is never admitted evidence.
const MAX_ID_BYTES: usize = 1024;

/// The envelope's native answer.
#[derive(Debug)]
pub(crate) enum Answer<'a> {
    /// `retCode` 0 with the raw `result` object, when present, and
    /// `retExtInfo`, if any.
    Success {
        result: Option<&'a RawValue>,
        ext: Option<&'a RawValue>,
    },
    /// A non-zero `retCode`, with its message and whatever rows it carried.
    Refused {
        code: i64,
        message: String,
        result: Option<&'a RawValue>,
        ext: Option<&'a RawValue>,
    },
}

/// One row of a batch acknowledgement, already matched to its request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Accepted(String),
    Refused { code: i64, message: String },
}

/// The acknowledgement does not prove anything about the write.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Malformed;

#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(rename = "retCode", borrow)]
    ret_code: &'a RawValue,
    #[serde(rename = "retMsg", default)]
    ret_msg: Option<String>,
    #[serde(borrow, default)]
    result: Option<&'a RawValue>,
    #[serde(rename = "retExtInfo", borrow, default)]
    ret_ext_info: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct Identity {
    #[serde(rename = "orderId", default)]
    order_id: Option<String>,
    #[serde(rename = "orderLinkId", default)]
    order_link_id: Option<String>,
}

#[derive(Deserialize)]
struct List<'a> {
    #[serde(borrow)]
    list: &'a RawValue,
}

#[derive(Deserialize)]
struct Status<'a> {
    #[serde(borrow)]
    code: &'a RawValue,
    #[serde(default)]
    msg: Option<String>,
}

#[derive(Deserialize)]
struct Affected<'a> {
    #[serde(borrow)]
    list: &'a RawValue,
    #[serde(default)]
    success: Option<String>,
}

#[derive(Deserialize)]
struct MarginReasons<'a> {
    #[serde(borrow, default)]
    reasons: Option<&'a RawValue>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct MarginReason {
    #[serde(rename = "reasonCode", default)]
    reason_code: Option<String>,
    #[serde(rename = "reasonMsg", default)]
    reason_msg: Option<String>,
}

#[derive(Deserialize)]
struct PositionRow {
    symbol: String,
    #[serde(rename = "positionIdx")]
    position_idx: i64,
}

fn is_object(raw: &RawValue) -> bool {
    raw.get().trim_start().starts_with('{')
}

fn is_array(raw: &RawValue) -> bool {
    raw.get().trim_start().starts_with('[')
}

fn is_null(raw: &RawValue) -> bool {
    raw.get().trim() == "null"
}

/// Decode `raw` as `T` only when it is a JSON object (serde would otherwise
/// accept a positional array for a struct).
fn object<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> Result<T, Malformed> {
    if !is_object(raw) {
        return Err(Malformed);
    }
    serde_json::from_str(raw.get()).map_err(|_| Malformed)
}

/// A bounded array whose every element is an object.
fn objects<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> Result<Vec<T>, Malformed> {
    if !is_array(raw) {
        return Err(Malformed);
    }
    let items: Vec<&'a RawValue> = serde_json::from_str(raw.get()).map_err(|_| Malformed)?;
    if items.len() > MAX_ROWS {
        return Err(Malformed);
    }
    items.into_iter().map(object).collect()
}

fn code(raw: &RawValue) -> Result<i64, Malformed> {
    decode_int(raw.get()).ok_or(Malformed)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

/// A native order ID the engine can admit as evidence.
fn native_id(value: Option<String>) -> Result<String, Malformed> {
    non_empty(value)
        .filter(|id| id.len() <= MAX_ID_BYTES)
        .ok_or(Malformed)
}

/// The envelope: an object with an exact integer `retCode`. A present
/// `result` must be an object (an absent one is a bare success); a present
/// `retExtInfo` must be an object or null.
pub(crate) fn answer(body: &str) -> Result<Answer<'_>, Malformed> {
    let raw: &RawValue = serde_json::from_str(body).map_err(|_| Malformed)?;
    let envelope: Envelope<'_> = object(raw)?;
    let code = code(envelope.ret_code)?;
    let ext = envelope.ret_ext_info.filter(|raw| !is_null(raw));
    if code != 0 {
        return Ok(Answer::Refused {
            code,
            message: envelope.ret_msg.unwrap_or_default(),
            result: envelope.result.filter(|raw| !is_null(raw)),
            ext,
        });
    }
    if envelope.result.is_some_and(|raw| !is_object(raw)) || ext.is_some_and(|raw| !is_object(raw))
    {
        return Err(Malformed);
    }
    Ok(Answer::Success {
        result: envelope.result,
        ext,
    })
}

/// The client echo a create may carry: none when no client ID was sent, the
/// sent one (or none) otherwise.
fn echo_matches(echo: Option<String>, sent: Option<&str>) -> bool {
    match non_empty(echo) {
        None => true,
        Some(echo) => sent == Some(echo.as_str()),
    }
}

/// `/v5/order/create`: the native order ID and a consistent client echo.
pub(crate) fn created(result: Option<&RawValue>, order_link_id: Option<&str>) -> Result<String, Malformed> {
    let row: Identity = object(result.ok_or(Malformed)?)?;
    if !echo_matches(row.order_link_id, order_link_id) {
        return Err(Malformed);
    }
    native_id(row.order_id)
}

/// `/v5/order/amend` and `/v5/order/cancel`: the result must name exactly the
/// targeted native order. Any existing client echo is accepted.
pub(crate) fn targeted(result: Option<&RawValue>, order_id: &str) -> Result<(), Malformed> {
    let row: Identity = object(result.ok_or(Malformed)?)?;
    if native_id(row.order_id)? != order_id {
        return Err(Malformed);
    }
    Ok(())
}

/// Create-batch: `result.list` and `retExtInfo.list` must each have exactly one
/// row per request, each accepted row must carry a consistent client echo, and
/// no native ID may repeat.
pub(crate) fn created_rows(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    order_link_ids: &[Option<&str>],
) -> Result<Vec<Row>, Malformed> {
    let rows = rows(result, ext, order_link_ids.len(), |index, row| {
        if !echo_matches(row.order_link_id, order_link_ids[index]) {
            return Err(Malformed);
        }
        native_id(row.order_id)
    })?;
    let mut ids: Vec<&str> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Accepted(id) => Some(id.as_str()),
            Row::Refused { .. } => None,
        })
        .collect();
    let accepted = ids.len();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() != accepted {
        return Err(Malformed);
    }
    Ok(rows)
}

/// Cancel-batch: one status per requested order, and each accepted row must
/// echo the requested native order ID.
pub(crate) fn cancelled_rows(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    order_ids: &[&str],
) -> Result<Vec<Row>, Malformed> {
    rows(result, ext, order_ids.len(), |index, row| {
        let id = native_id(row.order_id)?;
        if id != order_ids[index] {
            return Err(Malformed);
        }
        Ok(id)
    })
}

/// A one-order cancel-batch for a partial position stop: exactly one status
/// row; an accepted one may omit the result but never name another order.
pub(crate) fn cancelled_one(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    order_id: &str,
) -> Result<Row, Malformed> {
    let mut statuses: Vec<Status<'_>> =
        objects(object::<List<'_>>(ext.ok_or(Malformed)?)?.list)?;
    if statuses.len() != 1 {
        return Err(Malformed);
    }
    let status = statuses.pop().ok_or(Malformed)?;
    let code = code(status.code)?;
    if code != 0 {
        return Ok(Row::Refused {
            code,
            message: status.msg.unwrap_or_default(),
        });
    }
    if let Some(result) = result {
        let echoed: Vec<Identity> = match object::<List<'_>>(result) {
            Ok(list) => objects(list.list)?,
            Err(_) => vec![object(result)?],
        };
        if echoed.len() > 1
            || echoed
                .into_iter()
                .any(|row| non_empty(row.order_id).is_some_and(|id| id != order_id))
        {
            return Err(Malformed);
        }
    }
    Ok(Row::Accepted(order_id.to_owned()))
}

/// Pair each status with its result row. A refused row may carry an empty
/// identity; an accepted one must prove its target through `accept`.
fn rows(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    expected: usize,
    mut accept: impl FnMut(usize, Identity) -> Result<String, Malformed>,
) -> Result<Vec<Row>, Malformed> {
    let results: Vec<Identity> = objects(object::<List<'_>>(result.ok_or(Malformed)?)?.list)?;
    let statuses: Vec<Status<'_>> = objects(object::<List<'_>>(ext.ok_or(Malformed)?)?.list)?;
    if expected == 0 || results.len() != expected || statuses.len() != expected {
        return Err(Malformed);
    }
    results
        .into_iter()
        .zip(statuses)
        .enumerate()
        .map(|(index, (row, status))| match code(status.code)? {
            0 => accept(index, row).map(Row::Accepted),
            code => Ok(Row::Refused {
                code,
                message: status.msg.unwrap_or_default(),
            }),
        })
        .collect()
}

/// Cancel-all: the affected native order IDs. `success` must be "1" when
/// present; every row must be an object with a bounded ID.
pub(crate) fn cancelled_all(result: Option<&RawValue>) -> Result<Vec<String>, Malformed> {
    let affected: Affected<'_> = object(result.ok_or(Malformed)?)?;
    if affected.success.as_deref().is_some_and(|success| success != "1") {
        return Err(Malformed);
    }
    let rows: Vec<Identity> = objects(affected.list)?;
    let mut ids = rows
        .into_iter()
        .map(|row| native_id(row.order_id))
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

/// Bounded native IDs a malformed or contradictory answer carried, as
/// reconciliation evidence only: the `result` object's own ID or every row of
/// its list, and nothing at all when any row is not a well-formed object.
pub(crate) fn evidence_ids(result: Option<&RawValue>) -> Vec<String> {
    let Some(result) = result.filter(|raw| is_object(raw)) else {
        return Vec::new();
    };
    let rows: Vec<Identity> = match object::<List<'_>>(result) {
        Ok(list) => objects(list.list).unwrap_or_default(),
        Err(_) => object(result).map(|row| vec![row]).unwrap_or_default(),
    };
    rows.into_iter()
        .map(|row| native_id(row.order_id))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
}

/// Whether a refusal carried any per-item rows (`result.list` or
/// `retExtInfo.list` with at least one element), which contradict a whole one.
pub(crate) fn carries_rows(result: Option<&RawValue>, ext: Option<&RawValue>) -> bool {
    [result, ext].into_iter().flatten().any(|raw| {
        object::<List<'_>>(raw).is_ok_and(|list| {
            serde_json::from_str::<Vec<&RawValue>>(list.list.get()).map_or(true, |rows| !rows.is_empty())
        })
    })
}

/// `/v5/account/set-margin-mode`: `true` is proven success (no reasons). A
/// reason row, even with blank text, proves nothing on a retCode-0 answer.
pub(crate) fn margin_applied(result: Option<&RawValue>) -> Result<bool, Malformed> {
    let Some(result) = result else {
        return Ok(true);
    };
    let reasons: MarginReasons<'_> = object(result)?;
    let Some(raw) = reasons.reasons else {
        return Ok(true);
    };
    let reasons: Vec<MarginReason> = objects(raw)?;
    Ok(reasons.is_empty())
}

/// `/v5/position/list` for one symbol: hedged (`true`) or one-way. Rows must
/// be objects with an integer `positionIdx`; conflicting or absent modes fail.
pub(crate) fn position_mode(result: Option<&RawValue>, symbol: &str) -> Result<bool, String> {
    let invalid = || "Invalid position mode response".to_owned();
    let rows: Vec<PositionRow> = object::<List<'_>>(result.ok_or_else(invalid)?)
        .and_then(|list| objects(list.list))
        .map_err(|_| invalid())?;
    let mut mode = None;
    for row in rows.into_iter().filter(|row| row.symbol == symbol) {
        let hedged = match row.position_idx {
            0 => false,
            1 | 2 => true,
            _ => return Err("Invalid position mode".into()),
        };
        if mode
            .replace(hedged)
            .is_some_and(|previous| previous != hedged)
        {
            return Err("Conflicting position modes".into());
        }
    }
    mode.ok_or_else(|| format!("Position mode missing for {symbol}"))
}

#[cfg(test)]
#[path = "acknowledgement_tests.rs"]
mod tests;
