//! Strict Bybit V5 write acknowledgements. Every level must be a JSON object,
//! codes are exact integers, IDs are non-empty strings, and rows are matched
//! to the request by echo, never by position. A shape this module rejects is a
//! malformed acknowledgement: the engine records it as unproven, never success.
use safe_cex_engine::classify::decode_int;
use serde::Deserialize;
use serde_json::value::RawValue;

/// Rows a batch acknowledgement may carry; larger answers are malformed.
const MAX_ROWS: usize = 500;

/// The envelope's native answer.
#[derive(Debug)]
pub(crate) enum Answer<'a> {
    /// `retCode` 0 with the raw `result` object (and `retExtInfo`, if any).
    Success {
        result: &'a RawValue,
        ext: Option<&'a RawValue>,
    },
    /// A non-zero `retCode`, with its message.
    Refused { code: i64, message: String },
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
struct MarginReasons<'a> {
    #[serde(borrow, default)]
    reasons: Option<&'a RawValue>,
}

#[derive(Deserialize)]
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

/// The envelope: an object with an exact integer `retCode`; success needs an
/// object `result`.
pub(crate) fn answer(body: &str) -> Result<Answer<'_>, Malformed> {
    let raw: &RawValue = serde_json::from_str(body).map_err(|_| Malformed)?;
    let envelope: Envelope<'_> = object(raw)?;
    let code = code(envelope.ret_code)?;
    if code != 0 {
        return Ok(Answer::Refused {
            code,
            message: envelope.ret_msg.unwrap_or_default(),
        });
    }
    let result = envelope
        .result
        .filter(|raw| is_object(raw))
        .ok_or(Malformed)?;
    let ext = envelope
        .ret_ext_info
        .filter(|raw| raw.get().trim() != "null");
    if ext.is_some_and(|raw| !is_object(raw)) {
        return Err(Malformed);
    }
    Ok(Answer::Success { result, ext })
}

/// `/v5/order/create` and `/v5/order/amend`: the native order ID, and the
/// client echo when one was sent (create) or the target (amend).
pub(crate) fn identity(
    result: &RawValue,
    order_id: Option<&str>,
    order_link_id: Option<&str>,
) -> Result<String, Malformed> {
    let row: Identity = object(result)?;
    let id = non_empty(row.order_id).ok_or(Malformed)?;
    if order_id.is_some_and(|expected| expected != id) {
        return Err(Malformed);
    }
    if order_link_id.is_some_and(|expected| row.order_link_id.as_deref() != Some(expected)) {
        return Err(Malformed);
    }
    Ok(id)
}

/// Create-batch: `result.list` and `retExtInfo.list` must each have exactly one
/// row per request, and each accepted row must echo its request's client ID.
pub(crate) fn created_rows(
    result: &RawValue,
    ext: Option<&RawValue>,
    order_link_ids: &[Option<&str>],
) -> Result<Vec<Row>, Malformed> {
    rows(result, ext, order_link_ids.len(), |index, row| {
        let expected = order_link_ids[index];
        let id = non_empty(row.order_id).ok_or(Malformed)?;
        if expected.is_some_and(|expected| row.order_link_id.as_deref() != Some(expected)) {
            return Err(Malformed);
        }
        Ok(id)
    })
}

/// Cancel-batch: one status per requested order, and each accepted row must
/// echo the requested native order ID.
pub(crate) fn cancelled_rows(
    result: &RawValue,
    ext: Option<&RawValue>,
    order_ids: &[&str],
) -> Result<Vec<Row>, Malformed> {
    rows(result, ext, order_ids.len(), |index, row| {
        let id = non_empty(row.order_id).ok_or(Malformed)?;
        if id != order_ids[index] {
            return Err(Malformed);
        }
        Ok(id)
    })
}

/// Pair each status with its result row. A refused row may carry an empty
/// identity; an accepted one must prove its target through `accept`.
fn rows(
    result: &RawValue,
    ext: Option<&RawValue>,
    expected: usize,
    mut accept: impl FnMut(usize, Identity) -> Result<String, Malformed>,
) -> Result<Vec<Row>, Malformed> {
    let results: Vec<Identity> = objects(object::<List<'_>>(result)?.list)?;
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

/// `/v5/account/set-margin-mode`: `None` is proven success; `Some` is the
/// native refusal reason. Any reason row refuses, even with blank text.
pub(crate) fn margin_refusal(result: &RawValue) -> Result<Option<String>, Malformed> {
    let reasons: MarginReasons<'_> = object(result)?;
    let Some(raw) = reasons.reasons.filter(|raw| raw.get().trim() != "null") else {
        return Ok(None);
    };
    let reasons: Vec<MarginReason> = objects(raw)?;
    Ok(reasons.into_iter().next().map(|reason| {
        non_empty(reason.reason_msg)
            .or(non_empty(reason.reason_code))
            .unwrap_or_else(|| "margin mode change refused".to_owned())
    }))
}

/// `/v5/position/list` for one symbol: hedged (`true`) or one-way. Rows must
/// be objects with an integer `positionIdx`; conflicting or absent modes fail.
pub(crate) fn position_mode(result: &RawValue, symbol: &str) -> Result<bool, String> {
    let rows: Vec<PositionRow> = object::<List<'_>>(result)
        .and_then(|list| objects(list.list))
        .map_err(|_| "Invalid position mode response".to_owned())?;
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
    mode.ok_or_else(|| "Position mode missing".into())
}

#[cfg(test)]
#[path = "acknowledgement_tests.rs"]
mod tests;
