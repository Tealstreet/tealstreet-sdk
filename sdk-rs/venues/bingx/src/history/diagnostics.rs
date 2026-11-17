//! Failure-only streaming inspection: fixed fields/types, never row values.
use super::Kind;
use safe_cex_engine::{
    HistoryDecode, HistoryDecodeField as Field, HistoryDecodeReason as Reason, HistoryDecodeShape,
    HistoryEnvelopeCode as Code, HistoryExpectedType as Expected, HistoryJsonType as Actual,
    HistorySchema,
};
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

#[derive(Clone, Copy)]
pub(super) struct Detail {
    reason: Reason,
    field: Field,
    actual: Actual,
}
fn detail(reason: Reason, field: Field, actual: Actual) -> Detail {
    Detail {
        reason,
        field,
        actual,
    }
}

#[derive(Clone, Copy)]
enum Scope {
    IncomeRow,
    OrdersRow,
    IncomeEnvelope,
    OrdersEnvelope,
    OrdersData,
}
impl Scope {
    fn schema(self) -> HistorySchema {
        match self {
            Self::OrdersRow | Self::OrdersEnvelope | Self::OrdersData => HistorySchema::BingxOrders,
            Self::IncomeRow | Self::IncomeEnvelope => HistorySchema::BingxIncome,
        }
    }
    fn fields(self) -> &'static [(Field, bool)] {
        use Field as F;
        match self {
            Self::IncomeRow => &[
                (F::Symbol, false),
                (F::IncomeType, true),
                (F::Income, true),
                (F::Asset, true),
                (F::Time, true),
                (F::TranId, true),
                (F::TradeId, false),
            ],
            Self::OrdersRow => &[
                (F::OrderId, true),
                (F::Symbol, true),
                (F::Side, true),
                (F::OrderType, true),
                (F::Status, true),
                (F::Price, true),
                (F::AveragePrice, false),
                (F::Quantity, true),
                (F::FilledQuantity, true),
                (F::ReduceOnly, false),
                (F::PositionSide, false),
                (F::Time, true),
                (F::UpdateTime, false),
            ],
            Self::IncomeEnvelope | Self::OrdersEnvelope => {
                &[(F::Code, true), (F::Message, false), (F::Data, true)]
            }
            Self::OrdersData => &[(F::DataOrders, true)],
        }
    }
    fn reason(self, field: Field, value: bool) -> Reason {
        match self {
            Self::IncomeRow | Self::OrdersRow => {
                if value {
                    Reason::FieldValue
                } else {
                    Reason::FieldType
                }
            }
            Self::OrdersData => Reason::DataShape,
            Self::IncomeEnvelope | Self::OrdersEnvelope => match field {
                Field::Code => Reason::EnvelopeCode,
                Field::Message => Reason::EnvelopeShape,
                _ => Reason::DataShape,
            },
        }
    }
}

struct Key(Option<Field>);
impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Name;
        impl Visitor<'_> for Name {
            type Value = Key;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("key")
            }
            fn visit_str<E: de::Error>(self, name: &str) -> Result<Key, E> {
                use Field as F;
                Ok(Key(match name {
                    "code" => Some(F::Code),
                    "msg" => Some(F::Message),
                    "data" => Some(F::Data),
                    "orders" => Some(F::DataOrders),
                    "symbol" => Some(F::Symbol),
                    "incomeType" => Some(F::IncomeType),
                    "income" => Some(F::Income),
                    "asset" => Some(F::Asset),
                    "time" => Some(F::Time),
                    "tranId" => Some(F::TranId),
                    "tradeId" => Some(F::TradeId),
                    "orderId" => Some(F::OrderId),
                    "side" => Some(F::Side),
                    "type" => Some(F::OrderType),
                    "status" => Some(F::Status),
                    "price" => Some(F::Price),
                    "avgPrice" => Some(F::AveragePrice),
                    "origQty" => Some(F::Quantity),
                    "executedQty" => Some(F::FilledQuantity),
                    "reduceOnly" => Some(F::ReduceOnly),
                    "positionSide" => Some(F::PositionSide),
                    "updateTime" => Some(F::UpdateTime),
                    _ => None,
                }))
            }
        }
        d.deserialize_identifier(Name)
    }
}

struct Inspection {
    failure: Option<Detail>,
    code: Code,
}
struct Object(Scope);
impl<'de> Visitor<'de> for Object {
    type Value = Inspection;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("object")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Inspection, M::Error> {
        let mut seen = 0_u32;
        let mut failure = None;
        while let Some(Key(key)) = map.next_key::<Key>()? {
            let Some(field) = key.filter(|field| self.0.fields().iter().any(|(f, _)| f == field))
            else {
                map.next_value::<IgnoredAny>()?;
                continue;
            };
            let sample = map.next_value_seed(Scalar {
                scope: self.0,
                field,
            })?;
            if failure.is_none() {
                failure = sample.failure.or_else(|| {
                    (seen & (1 << field as u32) != 0)
                        .then(|| detail(self.0.reason(field, true), field, sample.actual))
                });
            }
            seen |= 1 << field as u32;
        }
        for &(field, required) in self.0.fields() {
            if required && seen & (1 << field as u32) == 0 {
                failure = Some(detail(self.0.reason(field, false), field, Actual::Missing));
                break;
            }
        }
        Ok(Inspection {
            failure,
            code: if seen & (1 << Field::Code as u32) == 0 {
                Code::Absent
            } else {
                Code::Present
            },
        })
    }
}

#[derive(Clone, Copy)]
struct Scalar {
    scope: Scope,
    field: Field,
}
struct Sample {
    actual: Actual,
    failure: Option<Detail>,
    code: Code,
}
impl Scalar {
    fn expected(self) -> Expected {
        HistoryDecode::expected(self.scope.schema(), self.field)
    }
    fn sample(self, actual: Actual, valid: bool, value_error: bool) -> Sample {
        Sample {
            actual,
            code: Code::Absent,
            failure: (!valid).then(|| {
                detail(
                    self.scope.reason(self.field, value_error),
                    self.field,
                    actual,
                )
            }),
        }
    }
    fn text(self, value: &str) -> Sample {
        let expected = self.expected();
        let valid_type = matches!(
            expected,
            Expected::String
                | Expected::StringOrNull
                | Expected::StringOrInteger
                | Expected::StringOrIntegerOrNull
                | Expected::NumberOrNumericString
                | Expected::BooleanOrStringOrNull
        );
        if !valid_type {
            return self.sample(Actual::String, false, false);
        }
        let valid = match expected {
            Expected::NumberOrNumericString => safe_cex_engine::num::parse_decimal(value).is_some(),
            Expected::StringOrInteger => !value.trim().is_empty(),
            Expected::BooleanOrStringOrNull => crate::account_wire::boolean_text(value).is_some(),
            _ => true,
        };
        let mut result = self.sample(Actual::String, valid, true);
        if valid && matches!(self.scope, Scope::OrdersRow) {
            let refusal = match self.field {
                Field::Side if super::side_value(value).is_none() => Some(Reason::Side),
                Field::OrderType if super::order_type(value).is_none() => Some(Reason::OrderType),
                Field::Status if super::order_status(value).is_none() => Some(Reason::OrderStatus),
                _ => None,
            };
            result.failure = refusal.map(|reason| detail(reason, self.field, Actual::String));
        }
        result
    }
    fn number(self, integer: bool, signed: bool) -> Sample {
        let expected = self.expected();
        let valid_type = matches!(
            expected,
            Expected::NumberOrNumericString
                | Expected::Integer
                | Expected::StringOrInteger
                | Expected::StringOrIntegerOrNull
        );
        let valid = match expected {
            Expected::Integer => signed,
            Expected::StringOrInteger | Expected::StringOrIntegerOrNull => integer,
            Expected::NumberOrNumericString => true,
            _ => false,
        };
        self.sample(Actual::Number, valid, valid_type)
    }
}
impl<'de> DeserializeSeed<'de> for Scalar {
    type Value = Sample;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Sample, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Scalar {
    type Value = Sample;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("field")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Sample, E> {
        Ok(self.text(value))
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Sample, E> {
        Ok(self.sample(
            Actual::Boolean,
            self.expected() == Expected::BooleanOrStringOrNull,
            false,
        ))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Sample, E> {
        Ok(self.sample(
            Actual::Null,
            matches!(
                self.expected(),
                Expected::BooleanOrStringOrNull
                    | Expected::StringOrNull
                    | Expected::StringOrIntegerOrNull
            ),
            false,
        ))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Sample, E> {
        Ok(self.number(true, true))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Sample, E> {
        Ok(self.number(true, i64::try_from(value).is_ok()))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Sample, E> {
        Ok(self.number(false, false))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Sample, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(self.sample(
            Actual::Array,
            matches!(self.expected(), Expected::Array | Expected::ArrayOrObject),
            false,
        ))
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Sample, M::Error> {
        if self.field == Field::Root {
            let parsed = Object(self.scope).visit_map(map)?;
            return Ok(Sample {
                actual: Actual::Object,
                failure: parsed.failure,
                code: parsed.code,
            });
        }
        if matches!(self.scope, Scope::OrdersEnvelope) && self.field == Field::Data {
            return Ok(Sample {
                actual: Actual::Object,
                failure: Object(Scope::OrdersData).visit_map(map)?.failure,
                code: Code::Absent,
            });
        }
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(self.sample(Actual::Object, false, false))
    }
}

fn inspect(scope: Scope, text: &str) -> Result<Inspection, Actual> {
    let mut decoder = serde_json::Deserializer::from_str(text);
    let parsed = Scalar {
        scope,
        field: Field::Root,
    }
    .deserialize(&mut decoder)
    .map_err(|_| Actual::InvalidJson)?;
    decoder.end().map_err(|_| Actual::InvalidJson)?;
    if parsed.actual != Actual::Object {
        return Err(parsed.actual);
    }
    Ok(Inspection {
        failure: parsed.failure,
        code: parsed.code,
    })
}

pub(super) fn row(kind: Kind, raw: &str, _error: &str) -> Option<Detail> {
    let scope = match kind {
        Kind::Income => Scope::IncomeRow,
        Kind::Orders => Scope::OrdersRow,
        _ => return None,
    };
    Some(match inspect(scope, raw) {
        Ok(parsed) => parsed
            .failure
            .unwrap_or_else(|| detail(Reason::RowShape, Field::Row, Actual::Object)),
        Err(actual) => detail(Reason::RowShape, Field::Row, actual),
    })
}
pub(super) fn failure(
    kind: Kind,
    row: Option<Detail>,
    body: Option<&str>,
) -> Option<HistoryDecode> {
    let scope = match kind {
        Kind::Income => Scope::IncomeEnvelope,
        Kind::Orders => Scope::OrdersEnvelope,
        _ => return None,
    };
    let (envelope, code) = match body.map(|text| inspect(scope, text)) {
        Some(Ok(parsed)) => (parsed.failure, parsed.code),
        failed => {
            let actual = failed.and_then(Result::err).unwrap_or(Actual::InvalidJson);
            (
                Some(detail(Reason::EnvelopeShape, Field::Root, actual)),
                if matches!(actual, Actual::InvalidJson | Actual::Object) {
                    Code::Unavailable
                } else {
                    Code::Absent
                },
            )
        }
    };
    let detail = row
        .or(envelope)
        .unwrap_or_else(|| detail(Reason::EnvelopeShape, Field::Root, Actual::Object));
    HistoryDecode::with_shape(
        scope.schema(),
        detail.reason,
        HistoryDecodeShape {
            field: detail.field,
            expected: HistoryDecode::expected(scope.schema(), detail.field),
            actual: detail.actual,
            envelope_code: code,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_inspection_validates_the_entire_json_document() {
        for text in [
            r#"{"code":0,"data":[]} trailing"#,
            r#"{"code":0,"data":[}"#,
            "",
        ] {
            let detail = failure(Kind::Income, None, Some(text)).unwrap();
            let wire = serde_json::to_value(detail).unwrap();
            assert_eq!(wire["shape"]["actual"], "invalidJson");
            assert_eq!(wire["shape"]["envelopeCode"], "unavailable");
        }
        for (text, actual) in [
            ("[]", "array"),
            ("null", "null"),
            ("false", "boolean"),
            ("7", "number"),
            (r#""text""#, "string"),
        ] {
            let detail = failure(Kind::Income, None, Some(text)).unwrap();
            let wire = serde_json::to_value(detail).unwrap();
            assert_eq!(wire["shape"]["actual"], actual);
            assert_eq!(wire["shape"]["envelopeCode"], "absent");
        }
        let parsed = inspect(Scope::IncomeEnvelope, "  {\"code\":0,\"data\":[]} ")
            .unwrap_or_else(|_| panic!("valid padded JSON"));
        assert!(parsed.failure.is_none());
        assert!(matches!(parsed.code, Code::Present));
    }

    #[test]
    fn duplicate_code_keeps_presence_and_a_closed_failure() {
        let detail = failure(Kind::Income, None, Some(r#"{"code":0,"code":0,"data":[]}"#)).unwrap();
        assert_eq!(
            serde_json::to_value(detail).unwrap(),
            serde_json::json!({
                "schema":"bingxIncome", "reason":"envelopeCode", "shape":{
                    "field":"code", "expected":"integer", "actual":"number", "envelopeCode":"present"
                }
            })
        );
    }

    #[test]
    fn escaped_known_keys_and_unknown_private_fields_remain_value_free() {
        let raw = r#"{"incomeType":"FUNDING_FEE","inco\u006de":null,"asset":"USDT","time":1702713615000,"tranId":"private-id-canary","unknown":{"apiKey":"private-key-canary"}}"#;
        let row = row(Kind::Income, raw, "").unwrap();
        let detail = failure(
            Kind::Income,
            Some(row),
            Some(&format!(r#"{{"code":0,"data":[{raw}]}}"#)),
        )
        .unwrap();
        let value = serde_json::to_value(detail).unwrap();
        assert_eq!(value["reason"], "fieldType");
        assert_eq!(value["shape"]["field"], "row.income");
        assert_eq!(value["shape"]["actual"], "null");
        assert!(!value.to_string().contains("private-"));
    }
}
