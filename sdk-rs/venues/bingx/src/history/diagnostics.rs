//! Failure-only inspection: export fixed paths and JSON types, never row values.
use super::Kind;
use safe_cex_engine::{
    HistoryDecode, HistoryDecodeField as Field, HistoryDecodeReason as Reason, HistoryDecodeShape,
    HistoryEnvelopeCode as Code, HistoryExpectedType as Expected, HistoryJsonType as Actual,
    HistorySchema,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) struct Detail {
    reason: Reason,
    field: Field,
    expected: Expected,
    actual: Actual,
}

fn detail(reason: Reason, field: Field, expected: Expected, actual: Actual) -> Detail {
    Detail {
        reason,
        field,
        expected,
        actual,
    }
}

fn actual(value: Option<&Value>) -> Actual {
    match value {
        None => Actual::Missing,
        Some(Value::Null) => Actual::Null,
        Some(Value::Bool(_)) => Actual::Boolean,
        Some(Value::Number(_)) => Actual::Number,
        Some(Value::String(_)) => Actual::String,
        Some(Value::Array(_)) => Actual::Array,
        Some(Value::Object(_)) => Actual::Object,
    }
}

fn integer(value: &Value) -> bool {
    value.as_i64().is_some() || value.as_u64().is_some()
}

#[derive(Deserialize)]
struct Boolean(#[serde(deserialize_with = "crate::account_wire::boolean")] bool);

fn scalar(
    value: Option<&Value>,
    field: Field,
    expected: Expected,
    required: bool,
) -> Option<Detail> {
    let Some(value) = value else {
        return required.then(|| detail(Reason::FieldType, field, expected, Actual::Missing));
    };
    let type_matches = match expected {
        Expected::String => value.is_string(),
        Expected::StringOrNull => value.is_string() || value.is_null(),
        Expected::StringOrInteger => value.is_string() || value.is_number(),
        Expected::StringOrIntegerOrNull => {
            value.is_string() || value.is_number() || value.is_null()
        }
        Expected::NumberOrNumericString => value.is_number() || value.is_string(),
        Expected::BooleanOrStringOrNull => {
            value.is_boolean() || value.is_string() || value.is_null()
        }
        _ => false,
    };
    if !type_matches {
        return Some(detail(
            Reason::FieldType,
            field,
            expected,
            actual(Some(value)),
        ));
    }
    let valid = match expected {
        Expected::NumberOrNumericString => {
            serde_json::from_value::<safe_cex_engine::de::Number>(value.clone()).is_ok()
        }
        Expected::StringOrInteger => {
            value.as_str().is_some_and(|s| !s.trim().is_empty()) || integer(value)
        }
        Expected::StringOrIntegerOrNull => value.is_string() || value.is_null() || integer(value),
        Expected::BooleanOrStringOrNull => serde_json::from_value::<Boolean>(value.clone())
            .map(|parsed| parsed.0)
            .is_ok(),
        _ => true,
    };
    (!valid).then(|| detail(Reason::FieldValue, field, expected, actual(Some(value))))
}

type Rule = (&'static str, Field, Expected, bool);
const INCOME: &[Rule] = &[
    ("symbol", Field::Symbol, Expected::String, false),
    ("incomeType", Field::IncomeType, Expected::String, true),
    (
        "income",
        Field::Income,
        Expected::NumberOrNumericString,
        true,
    ),
    ("asset", Field::Asset, Expected::String, true),
    ("time", Field::Time, Expected::NumberOrNumericString, true),
    ("tranId", Field::TranId, Expected::StringOrInteger, true),
    (
        "tradeId",
        Field::TradeId,
        Expected::StringOrIntegerOrNull,
        false,
    ),
];
const ORDERS: &[Rule] = &[
    ("orderId", Field::OrderId, Expected::StringOrInteger, true),
    ("symbol", Field::Symbol, Expected::String, true),
    ("side", Field::Side, Expected::String, true),
    ("type", Field::OrderType, Expected::String, true),
    ("status", Field::Status, Expected::String, true),
    ("price", Field::Price, Expected::NumberOrNumericString, true),
    (
        "avgPrice",
        Field::AveragePrice,
        Expected::NumberOrNumericString,
        false,
    ),
    (
        "origQty",
        Field::Quantity,
        Expected::NumberOrNumericString,
        true,
    ),
    (
        "executedQty",
        Field::FilledQuantity,
        Expected::NumberOrNumericString,
        true,
    ),
    (
        "reduceOnly",
        Field::ReduceOnly,
        Expected::BooleanOrStringOrNull,
        false,
    ),
    (
        "positionSide",
        Field::PositionSide,
        Expected::StringOrNull,
        false,
    ),
    ("time", Field::Time, Expected::NumberOrNumericString, true),
    (
        "updateTime",
        Field::UpdateTime,
        Expected::NumberOrNumericString,
        false,
    ),
];

pub(super) fn row(kind: Kind, raw: &str, error: &str) -> Option<Detail> {
    let rules = match kind {
        Kind::Income => INCOME,
        Kind::Orders => ORDERS,
        _ => return None,
    };
    let Ok(row) = serde_json::from_str::<Value>(raw) else {
        return Some(detail(
            Reason::RowShape,
            Field::Row,
            Expected::Object,
            Actual::InvalidJson,
        ));
    };
    if !row.is_object() {
        return Some(detail(
            Reason::RowShape,
            Field::Row,
            Expected::Object,
            actual(Some(&row)),
        ));
    }
    for &(name, field, expected, required) in rules {
        if let Some(failure) = scalar(row.get(name), field, expected, required) {
            return Some(failure);
        }
        if error.starts_with(&format!("duplicate field `{name}`")) {
            return Some(detail(
                Reason::FieldValue,
                field,
                expected,
                actual(row.get(name)),
            ));
        }
    }
    if matches!(kind, Kind::Orders) {
        for (name, field, reason, valid) in [
            (
                "side",
                Field::Side,
                Reason::Side,
                super::side(row["side"].as_str().unwrap_or_default()).is_ok(),
            ),
            (
                "type",
                Field::OrderType,
                Reason::OrderType,
                super::order_type(row["type"].as_str().unwrap_or_default()).is_some(),
            ),
            (
                "status",
                Field::Status,
                Reason::OrderStatus,
                super::order_status(row["status"].as_str().unwrap_or_default()).is_some(),
            ),
        ] {
            if !valid {
                return Some(detail(
                    reason,
                    field,
                    Expected::String,
                    actual(row.get(name)),
                ));
            }
        }
    }
    Some(detail(
        Reason::RowShape,
        Field::Row,
        Expected::Object,
        Actual::Object,
    ))
}

fn envelope(kind: Kind, value: Option<&Value>) -> Detail {
    let Some(value) = value else {
        return detail(
            Reason::EnvelopeShape,
            Field::Root,
            Expected::Object,
            Actual::InvalidJson,
        );
    };
    if !value.is_object() {
        return detail(
            Reason::EnvelopeShape,
            Field::Root,
            Expected::Object,
            actual(Some(value)),
        );
    }
    if value.get("code").and_then(Value::as_i64).is_none() {
        return detail(
            Reason::EnvelopeCode,
            Field::Code,
            Expected::Integer,
            actual(value.get("code")),
        );
    }
    if value.get("msg").is_some_and(|msg| !msg.is_string()) {
        return detail(
            Reason::EnvelopeShape,
            Field::Message,
            Expected::String,
            actual(value.get("msg")),
        );
    }
    let data = value.get("data");
    if matches!(kind, Kind::Orders) {
        if let Some(Value::Object(object)) = data {
            return detail(
                Reason::DataShape,
                Field::DataOrders,
                Expected::Array,
                actual(object.get("orders")),
            );
        }
        if !data.is_some_and(Value::is_array) {
            return detail(
                Reason::DataShape,
                Field::Data,
                Expected::ArrayOrObject,
                actual(data),
            );
        }
    } else if !data.is_some_and(Value::is_array) {
        return detail(
            Reason::DataShape,
            Field::Data,
            Expected::Array,
            actual(data),
        );
    }
    // A parser failure with no localized predicate remains explicitly unlocalized.
    detail(
        Reason::EnvelopeShape,
        Field::Root,
        Expected::Object,
        Actual::Object,
    )
}

pub(super) fn failure(
    kind: Kind,
    row: Option<Detail>,
    body: Option<&str>,
) -> Option<HistoryDecode> {
    let schema = match kind {
        Kind::Income => HistorySchema::BingxIncome,
        Kind::Orders => HistorySchema::BingxOrders,
        _ => return None,
    };
    let value = body.and_then(|body| serde_json::from_str::<Value>(body).ok());
    let code = match &value {
        Some(Value::Object(object)) if object.contains_key("code") => Code::Present,
        Some(_) => Code::Absent,
        None => Code::Unavailable,
    };
    let detail = row.unwrap_or_else(|| envelope(kind, value.as_ref()));
    Some(HistoryDecode {
        schema,
        reason: detail.reason,
        shape: Some(HistoryDecodeShape {
            field: detail.field,
            expected: detail.expected,
            actual: detail.actual,
            envelope_code: code,
        }),
    })
}
