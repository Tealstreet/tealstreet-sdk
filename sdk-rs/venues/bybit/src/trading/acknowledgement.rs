//! Strict Bybit V5 write acknowledgements. Every level must be a JSON object,
//! codes are exact integers, IDs are bounded non-empty strings, and rows are
//! matched to the request by echo, never by position alone. A shape this module
//! rejects is a malformed acknowledgement: the engine records it as unproven.
use safe_cex_engine::classify::decode_int;
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

/// One JSON object's fields, borrowed raw: a single decoder for every level
/// (one serde instantiation instead of one per struct shape).
struct Fields<'a>(Vec<(String, &'a RawValue)>);

impl<'de> serde::Deserialize<'de> for Fields<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(raw: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Fields<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut fields = Vec::new();
                while let Some(entry) = map.next_entry::<String, &'de RawValue>()? {
                    fields.push(entry);
                }
                Ok(Fields(fields))
            }
        }
        raw.deserialize_map(Visit)
    }
}

impl<'a> Fields<'a> {
    /// A present field, `null` included; a repeated one is malformed, as a
    /// derived struct refuses a duplicate field.
    fn raw(&self, name: &str) -> Result<Option<&'a RawValue>, Malformed> {
        let mut found = self
            .0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, raw)| *raw);
        let first = found.next();
        match found.next() {
            Some(_) => Err(Malformed),
            None => Ok(first),
        }
    }
    fn required(&self, name: &str) -> Result<&'a RawValue, Malformed> {
        self.raw(name)?.ok_or(Malformed)
    }
    /// Optional text: absent or `null` is none; anything but a string is malformed.
    fn text(&self, name: &str) -> Result<Option<String>, Malformed> {
        match self.raw(name)? {
            None => Ok(None),
            Some(raw) if is_null(raw) => Ok(None),
            Some(raw) => string(raw).map(Some),
        }
    }
}

fn string(raw: &RawValue) -> Result<String, Malformed> {
    serde_json::from_str::<String>(raw.get()).map_err(|_| Malformed)
}

/// An order identity row: its native ID and client echo, both optional text.
struct Identity {
    order_id: Option<String>,
    order_link_id: Option<String>,
}

impl Identity {
    fn read(raw: &RawValue) -> Result<Self, Malformed> {
        let fields = object(raw)?;
        Ok(Self {
            order_id: fields.text("orderId")?,
            order_link_id: fields.text("orderLinkId")?,
        })
    }
}

/// A batch status row: an exact integer code and optional text.
struct Status {
    code: i64,
    msg: Option<String>,
}

impl Status {
    fn read(raw: &RawValue) -> Result<Self, Malformed> {
        let fields = object(raw)?;
        Ok(Self {
            code: code(fields.required("code")?)?,
            msg: fields.text("msg")?,
        })
    }
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

/// `raw`'s fields, only when it is a JSON object.
fn object(raw: &RawValue) -> Result<Fields<'_>, Malformed> {
    if !is_object(raw) {
        return Err(Malformed);
    }
    serde_json::from_str(raw.get()).map_err(|_| Malformed)
}

/// A bounded array of raw elements.
fn elements(raw: &RawValue) -> Result<Vec<&RawValue>, Malformed> {
    if !is_array(raw) {
        return Err(Malformed);
    }
    let items: Vec<&RawValue> = serde_json::from_str(raw.get()).map_err(|_| Malformed)?;
    if items.len() > MAX_ROWS {
        return Err(Malformed);
    }
    Ok(items)
}

/// A bounded array whose every element is an object, read by `row`.
fn rows_of<T>(
    raw: &RawValue,
    row: fn(&RawValue) -> Result<T, Malformed>,
) -> Result<Vec<T>, Malformed> {
    elements(raw)?.into_iter().map(row).collect()
}

/// An object's required `list` field.
fn list(raw: &RawValue) -> Result<&RawValue, Malformed> {
    object(raw)?.required("list")
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
    let envelope = object(raw)?;
    let code = code(envelope.required("retCode")?)?;
    let message = envelope.text("retMsg")?;
    let result = envelope.raw("result")?;
    let ext = envelope.raw("retExtInfo")?.filter(|raw| !is_null(raw));
    if code != 0 {
        return Ok(Answer::Refused {
            code,
            message: message.unwrap_or_default(),
            result: result.filter(|raw| !is_null(raw)),
            ext,
        });
    }
    if result.is_some_and(|raw| !is_object(raw)) || ext.is_some_and(|raw| !is_object(raw)) {
        return Err(Malformed);
    }
    Ok(Answer::Success { result, ext })
}

/// The client echo a create may carry: none when no client ID was sent, the
/// sent one (or none) otherwise.
fn echo_matches(echo: Option<String>, sent: Option<&str>) -> bool {
    match non_empty(echo) {
        None => true,
        Some(echo) => sent == Some(echo.as_str()),
    }
}

/// `/v5/order/create`: the native order ID, and the client echo when one was
/// sent. A single create names one order, so an unrequested echo is harmless.
pub(crate) fn created(
    result: Option<&RawValue>,
    order_link_id: Option<&str>,
) -> Result<String, Malformed> {
    let row = Identity::read(result.ok_or(Malformed)?)?;
    if order_link_id.is_some() && !echo_matches(row.order_link_id, order_link_id) {
        return Err(Malformed);
    }
    native_id(row.order_id)
}

/// `/v5/order/amend` and `/v5/order/cancel`: the result must name exactly the
/// targeted native order. Any existing client echo is accepted.
pub(crate) fn targeted(result: Option<&RawValue>, order_id: &str) -> Result<(), Malformed> {
    let row = Identity::read(result.ok_or(Malformed)?)?;
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
    let rows = rows(
        result,
        ext,
        order_link_ids.len(),
        true,
        true,
        &mut |index, row| {
            let row = row.ok_or(Malformed)?;
            if !echo_matches(row.order_link_id, order_link_ids[index]) {
                return Err(Malformed);
            }
            native_id(row.order_id)
        },
    )?;
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

/// Cancel-batch: one status per requested order. A returned `result.list`
/// must match the request one-for-one and echo each accepted target.
pub(crate) fn cancelled_rows(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    order_ids: &[&str],
) -> Result<Vec<Row>, Malformed> {
    rows(
        result,
        ext,
        order_ids.len(),
        false,
        false,
        &mut |index, row| {
            let Some(row) = row else {
                return Ok(order_ids[index].to_owned());
            };
            let id = native_id(row.order_id)?;
            if id != order_ids[index] {
                return Err(Malformed);
            }
            Ok(id)
        },
    )
}

/// A one-order cancel-batch for a partial position stop: exactly one status
/// row; an accepted one may omit the result but never name another order.
pub(crate) fn cancelled_one(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    order_id: &str,
) -> Result<Row, Malformed> {
    let mut statuses = rows_of(list(ext.ok_or(Malformed)?)?, Status::read)?;
    if statuses.len() != 1 {
        return Err(Malformed);
    }
    let status = statuses.pop().ok_or(Malformed)?;
    if status.code != 0 {
        return Ok(Row::Refused {
            code: status.code,
            message: status.msg.unwrap_or_default(),
        });
    }
    if let Some(result) = result {
        let echoed = match list(result) {
            Ok(rows) => rows_of(rows, Identity::read)?,
            Err(_) => vec![Identity::read(result)?],
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
/// identity; an accepted one must prove its target through `accept`. With
/// `required`, the result list must be present; with `anonymous_refusals`, a
/// refused row naming an order contradicts itself (a create that may exist).
fn rows(
    result: Option<&RawValue>,
    ext: Option<&RawValue>,
    expected: usize,
    required: bool,
    anonymous_refusals: bool,
    accept: &mut dyn FnMut(usize, Option<Identity>) -> Result<String, Malformed>,
) -> Result<Vec<Row>, Malformed> {
    let echoed = match result {
        Some(result) => object(result)?.raw("list")?,
        None => None,
    };
    let results = match echoed {
        Some(rows) => Some(rows_of(rows, Identity::read)?),
        None if required => return Err(Malformed),
        None => None,
    };
    let statuses = rows_of(list(ext.ok_or(Malformed)?)?, Status::read)?;
    if expected == 0
        || statuses.len() != expected
        || results.as_ref().is_some_and(|rows| rows.len() != expected)
    {
        return Err(Malformed);
    }
    let mut results = results.map(Vec::into_iter);
    let mut rows = Vec::with_capacity(statuses.len());
    for (index, status) in statuses.into_iter().enumerate() {
        let row = results.as_mut().and_then(Iterator::next);
        rows.push(match status.code {
            0 => Row::Accepted(accept(index, row)?),
            _ if anonymous_refusals && row.is_some_and(|row| non_empty(row.order_id).is_some()) => {
                return Err(Malformed);
            }
            code => Row::Refused {
                code,
                message: status.msg.unwrap_or_default(),
            },
        });
    }
    Ok(rows)
}

/// Cancel-all: the affected native order IDs. `success` must be "1" when
/// present; every row must be an object with a bounded ID.
pub(crate) fn cancelled_all(result: Option<&RawValue>) -> Result<Vec<String>, Malformed> {
    let affected = object(result.ok_or(Malformed)?)?;
    let rows = affected.required("list")?;
    if affected
        .text("success")?
        .is_some_and(|success| success != "1")
    {
        return Err(Malformed);
    }
    let rows = rows_of(rows, Identity::read)?;
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
    let rows = match list(result) {
        Ok(rows) => rows_of(rows, Identity::read).unwrap_or_default(),
        Err(_) => Identity::read(result)
            .map(|row| vec![row])
            .unwrap_or_default(),
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
        // Anything but an object (an array of rows, a scalar) is no whole refusal.
        !is_object(raw)
            || list(raw).is_ok_and(|rows| {
                serde_json::from_str::<Vec<&RawValue>>(rows.get())
                    .map_or(true, |rows| !rows.is_empty())
            })
    })
}

/// A margin refusal's native reasons, joined, when they are well-formed text.
pub(crate) fn margin_reasons(result: Option<&RawValue>) -> Option<String> {
    let reasons = object(result?).ok()?.raw("reasons").ok()??;
    let text: Vec<String> = rows_of(reasons, margin_reason)
        .ok()?
        .into_iter()
        .filter_map(non_empty)
        .collect();
    (!text.is_empty()).then(|| text.join(", "))
}

/// `/v5/account/set-margin-mode`: `true` is proven success (no reasons). A
/// reason row, even with blank text, proves nothing on a retCode-0 answer.
pub(crate) fn margin_applied(result: Option<&RawValue>) -> Result<bool, Malformed> {
    let Some(result) = result else {
        return Ok(true);
    };
    let Some(raw) = object(result)?.raw("reasons")? else {
        return Ok(true);
    };
    Ok(rows_of(raw, margin_reason)?.is_empty())
}

/// `/v5/position/list` for one symbol: hedged (`true`) or one-way. Rows must
/// be objects with an integer `positionIdx`; conflicting or absent modes fail.
pub(crate) fn position_mode(result: Option<&RawValue>, symbol: &str) -> Result<bool, String> {
    let invalid = || "Invalid position mode response".to_owned();
    let rows = list(result.ok_or_else(invalid)?)
        .and_then(|rows| rows_of(rows, position_row))
        .map_err(|_| invalid())?;
    let mut mode = None;
    for (_, position_idx) in rows.into_iter().filter(|(row, _)| row == symbol) {
        let hedged = match position_idx {
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

/// A margin reason row: its text, with a well-typed (unused) code.
fn margin_reason(raw: &RawValue) -> Result<Option<String>, Malformed> {
    let fields = object(raw)?;
    fields.text("reasonCode")?;
    fields.text("reasonMsg")
}

/// A position row: its symbol and exact integer `positionIdx`.
fn position_row(raw: &RawValue) -> Result<(String, i64), Malformed> {
    let fields = object(raw)?;
    let symbol = string(fields.required("symbol")?)?;
    // A JSON number only, as serde's i64 required (no quoted index).
    let index = fields.required("positionIdx")?;
    if index.get().trim_start().starts_with('"') {
        return Err(Malformed);
    }
    Ok((symbol, code(index)?))
}

#[cfg(test)]
#[path = "acknowledgement_tests.rs"]
mod tests;
