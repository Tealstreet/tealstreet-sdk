//! Serde decoders for venue numbers sent bare or as decimal strings.
//!
//! Every decoder accepts a JSON number or a numeric string and rejects
//! non-finite values. The name says what null, a blank string and a malformed
//! string become; nothing else differs between them.
use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, de::Error as _};
use std::fmt;

/// Strict: null, blank and malformed are errors.
pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    decode(d, Policy::STRICT)?.ok_or_else(|| D::Error::custom("missing number"))
}

/// Null is `None`; blank and malformed are errors.
pub fn optional_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    decode(d, Policy::OPTIONAL)
}

/// Null and blank are `None`; malformed is an error.
pub fn optional_number_or_blank<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    decode(d, Policy::OPTIONAL_OR_BLANK)
}

/// Null and blank read as 0; malformed is an error.
pub fn number_or_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(decode(d, Policy::OR_ZERO)?.unwrap_or(0.0))
}

/// Blank reads as 0 (TS `Number("")`); null and malformed are errors, so a
/// public discovery row with a bad field is skipped rather than zeroed.
pub fn number_or_blank_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    decode(d, Policy::BLANK_ZERO)?.ok_or_else(|| D::Error::custom("missing number"))
}

/// Lenient: a blank or malformed string reads as 0, so one bad field never
/// discards its row. Null is still an error.
pub fn lenient_number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    decode(d, Policy::LENIENT)?.ok_or_else(|| D::Error::custom("missing number"))
}

/// Lenient optional: null, blank and malformed are all `None`.
pub fn lenient_optional_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    decode(d, Policy::LENIENT_OPTIONAL)
}

/// A strict number for `Option<Number>`, `Vec<Number>` and tuple positions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Number(pub f64);

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        number(d).map(Self)
    }
}

/// What a non-number becomes: `Err` rejects, `Ok(v)` substitutes `v`.
type Outcome = Result<Option<f64>, &'static str>;

#[derive(Clone, Copy)]
struct Policy {
    null: Outcome,
    blank: Outcome,
    malformed: Outcome,
}

impl Policy {
    const STRICT: Self = Self {
        null: Err("null number"),
        blank: Err("blank number"),
        malformed: Err("invalid number"),
    };
    const OPTIONAL: Self = Self {
        null: Ok(None),
        ..Self::STRICT
    };
    const OPTIONAL_OR_BLANK: Self = Self {
        blank: Ok(None),
        ..Self::OPTIONAL
    };
    const OR_ZERO: Self = Self {
        null: Ok(Some(0.0)),
        blank: Ok(Some(0.0)),
        ..Self::STRICT
    };
    const BLANK_ZERO: Self = Self {
        blank: Ok(Some(0.0)),
        ..Self::STRICT
    };
    const LENIENT: Self = Self {
        blank: Ok(Some(0.0)),
        malformed: Ok(Some(0.0)),
        ..Self::STRICT
    };
    const LENIENT_OPTIONAL: Self = Self {
        null: Ok(None),
        blank: Ok(None),
        malformed: Ok(None),
    };
}

fn decode<'de, D: Deserializer<'de>>(d: D, policy: Policy) -> Result<Option<f64>, D::Error> {
    d.deserialize_any(Numeric(policy))
}

struct Numeric(Policy);

impl Numeric {
    fn finite<E: de::Error>(&self, value: f64) -> Result<Option<f64>, E> {
        if value.is_finite() {
            Ok(Some(value))
        } else {
            self.0.malformed.map_err(E::custom)
        }
    }
}

impl<'de> Visitor<'de> for Numeric {
    type Value = Option<f64>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a finite number or numeric string")
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        self.finite(value)
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Some(value as f64))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Some(value as f64))
    }
    fn visit_str<E: de::Error>(self, text: &str) -> Result<Self::Value, E> {
        if text.is_empty() {
            return self.0.blank.map_err(E::custom);
        }
        match crate::num::parse_decimal(text) {
            Some(value) => Ok(Some(value)),
            None => self.0.malformed.map_err(E::custom),
        }
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.null.map_err(E::custom)
    }
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.null.map_err(E::custom)
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Row {
        #[serde(default, deserialize_with = "number")]
        strict: f64,
        #[serde(default, deserialize_with = "optional_number")]
        optional: Option<f64>,
        #[serde(default, deserialize_with = "optional_number_or_blank")]
        or_blank: Option<f64>,
        #[serde(default, deserialize_with = "number_or_zero")]
        or_zero: f64,
        #[serde(default, deserialize_with = "number_or_blank_zero")]
        blank_zero: f64,
        #[serde(default, deserialize_with = "lenient_number")]
        lenient: f64,
        #[serde(default, deserialize_with = "lenient_optional_number")]
        lenient_optional: Option<f64>,
    }

    fn field(name: &str, raw: &str) -> Result<Row, serde_json::Error> {
        serde_json::from_str(&format!("{{\"{name}\":{raw}}}"))
    }

    #[test]
    fn every_policy_reads_numbers_and_numeric_strings() {
        for raw in ["1.5", "\"1.5\"", "\" 1.5 \"", "\"1.5e0\"", "\"+1.5\""] {
            let row: Row = serde_json::from_str(&format!(
                "{{\"strict\":{raw},\"optional\":{raw},\"or_blank\":{raw},\"or_zero\":{raw},\
                 \"blank_zero\":{raw},\"lenient\":{raw},\"lenient_optional\":{raw}}}"
            ))
            .unwrap();
            assert_eq!(row.strict, 1.5);
            assert_eq!(row.optional, Some(1.5));
            assert_eq!(row.or_blank, Some(1.5));
            assert_eq!(row.or_zero, 1.5);
            assert_eq!(row.blank_zero, 1.5);
            assert_eq!(row.lenient, 1.5);
            assert_eq!(row.lenient_optional, Some(1.5));
        }
        assert_eq!(field("strict", "-7").unwrap().strict, -7.0);
        assert_eq!(field("strict", "\"-0.25\"").unwrap().strict, -0.25);
        assert_eq!(
            field("strict", "\"\\u0031\"").unwrap().strict,
            1.0,
            "escaped strings decode"
        );
    }

    #[test]
    fn names_say_what_null_blank_and_malformed_become() {
        // (field, null, blank, malformed); None = rejected.
        type Case = (&'static str, Option<f64>, Option<f64>, Option<f64>);
        let read = |row: Row, name: &str| match name {
            "strict" => Some(row.strict),
            "or_zero" => Some(row.or_zero),
            "blank_zero" => Some(row.blank_zero),
            "lenient" => Some(row.lenient),
            "optional" => Some(row.optional.unwrap_or(f64::NAN)),
            "or_blank" => Some(row.or_blank.unwrap_or(f64::NAN)),
            _ => Some(row.lenient_optional.unwrap_or(f64::NAN)),
        };
        let absent = f64::NAN;
        let cases: [Case; 7] = [
            ("strict", None, None, None),
            ("optional", Some(absent), None, None),
            ("or_blank", Some(absent), Some(absent), None),
            ("or_zero", Some(0.0), Some(0.0), None),
            ("blank_zero", None, Some(0.0), None),
            ("lenient", None, Some(0.0), Some(0.0)),
            ("lenient_optional", Some(absent), Some(absent), Some(absent)),
        ];
        for (name, null, blank, malformed) in cases {
            for (raw, expected) in [
                ("null", null),
                ("\"\"", blank),
                ("\"abc\"", malformed),
                ("\"NaN\"", malformed),
                ("\"inf\"", malformed),
            ] {
                let got = field(name, raw).ok().and_then(|row| read(row, name));
                match expected {
                    None => assert!(got.is_none(), "{name} {raw} should reject, got {got:?}"),
                    Some(v) if v.is_nan() => {
                        assert!(got.is_some_and(f64::is_nan), "{name} {raw} should be None")
                    }
                    Some(v) => assert_eq!(got, Some(v), "{name} {raw}"),
                }
            }
        }
    }

    #[test]
    fn non_numeric_json_is_always_an_error() {
        for name in ["strict", "blank_zero", "lenient", "lenient_optional"] {
            for raw in ["true", "[]", "{}"] {
                assert!(field(name, raw).is_err(), "{name} {raw}");
            }
        }
    }

    #[test]
    fn decodes_inside_untagged_and_option_containers() {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Row {
                #[serde(deserialize_with = "optional_number")]
                value: Option<f64>,
            },
        }
        let Either::Row { value } = serde_json::from_str("{\"value\":null}").unwrap();
        assert_eq!(value, None);
        let Either::Row { value } = serde_json::from_str("{\"value\":\"2\"}").unwrap();
        assert_eq!(value, Some(2.0));
        let values: Vec<Option<Number>> = serde_json::from_str("[\"1\",null,3]").unwrap();
        assert_eq!(values, [Some(Number(1.0)), None, Some(Number(3.0))]);
        assert!(serde_json::from_str::<Vec<Number>>("[\"\"]").is_err());
    }
}
