//! Decimal coefficient/exponent arithmetic before the source's Number conversion.
use num_bigint::{BigInt, BigUint};
use num_traits::{Signed, Zero};
use safe_cex_num::{JsJsonValue as J, js_number_string};
#[derive(Clone)]
pub(crate) struct Decimal {
    coefficient: Option<BigInt>,
    exponent: i32,
    special: f64,
}
impl Decimal {
    pub(crate) fn parse(value: &str) -> Self {
        let value = safe_cex_num::js_trim(value);
        let invalid = || Self {
            coefficient: None,
            exponent: 0,
            special: f64::NAN,
        };
        if ["Infinity", "+Infinity", "-Infinity", "NaN"].contains(&value) {
            return Self {
                coefficient: None,
                exponent: 0,
                special: value.parse().unwrap_or(f64::NAN),
            };
        }
        let (negative, body) = value
            .strip_prefix('-')
            .map_or((false, value), |v| (true, v));
        let body = body.strip_prefix('+').unwrap_or(body);
        for (prefix, radix) in [
            ("0x", 16),
            ("0X", 16),
            ("0b", 2),
            ("0B", 2),
            ("0o", 8),
            ("0O", 8),
        ] {
            if let Some(body) = body.strip_prefix(prefix) {
                return BigInt::parse_bytes(body.as_bytes(), radix).map_or_else(
                    invalid,
                    |coefficient| Self {
                        coefficient: Some(if negative { -coefficient } else { coefficient }),
                        exponent: 0,
                        special: 0.,
                    },
                );
            }
        }
        let mut parts = body.split(['e', 'E']);
        let mantissa = parts.next().unwrap_or("");
        let exponent = match parts.next() {
            Some(v) => match v.parse::<i32>() {
                Ok(v) => v,
                Err(_) => return invalid(),
            },
            None => 0,
        };
        if parts.next().is_some() {
            return invalid();
        }
        let mut points = mantissa.split('.');
        let whole = points.next().unwrap_or("");
        let fraction = points.next().unwrap_or("");
        if points.next().is_some()
            || whole.len() + fraction.len() == 0
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|c| c.is_ascii_digit())
        {
            return invalid();
        }
        let Some(exponent) = exponent.checked_sub(fraction.len() as i32) else {
            return invalid();
        };
        let digits = format!("{whole}{fraction}");
        let Some(coefficient) = BigInt::parse_bytes(digits.as_bytes(), 10) else {
            return invalid();
        };
        Self {
            coefficient: Some(if negative { -coefficient } else { coefficient }),
            exponent,
            special: 0.,
        }
    }
    pub(crate) fn from_value(value: &J) -> Self {
        match value {
            J::Number(v) => Self::parse(&js_number_string(*v)),
            J::String(v) => Self::parse(v),
            _ => Self::parse("NaN"),
        }
    }
    pub(crate) fn multiply(&self, other: &Self) -> Self {
        if let (Some(a), Some(b)) = (&self.coefficient, &other.coefficient) {
            Self {
                coefficient: Some(a * b),
                exponent: self.exponent.saturating_add(other.exponent),
                special: 0.,
            }
        } else {
            Self::parse(&js_number_string(self.to_number() * other.to_number()))
        }
    }
    pub(crate) fn to_number(&self) -> f64 {
        self.coefficient.as_ref().map_or(self.special, |v| {
            format!("{v}e{}", self.exponent).parse().unwrap_or(f64::NAN)
        })
    }
    pub(crate) fn scaled_integer(&self, digits: i32) -> String {
        let Some(value) = &self.coefficient else {
            return js_number_string(self.special);
        };
        let exponent = self.exponent.saturating_add(digits);
        let result = if exponent >= 0 {
            value * BigInt::from(10u8).pow(exponent as u32)
        } else if -i64::from(exponent) > value.to_string().len() as i64 + 1 {
            BigInt::zero()
        } else {
            value / BigInt::from(10u8).pow((-exponent) as u32)
        };
        if result.is_zero() && value.is_negative() {
            "-0".into()
        } else {
            result.to_string()
        }
    }
    pub(crate) fn modulo_number(&self, modulus: u64) -> f64 {
        let Some(value) = &self.coefficient else {
            return f64::NAN;
        };
        let modulus = BigInt::from(modulus);
        if self.exponent >= 0 {
            let scale = BigInt::from(10u8).modpow(&BigInt::from(self.exponent), &modulus);
            ((value * scale) % modulus)
                .to_string()
                .parse()
                .unwrap_or(f64::NAN)
        } else if self.exponent < -400 {
            self.to_number()
        } else {
            let scale = BigInt::from(10u8).pow((-self.exponent) as u32);
            let remainder = value % (&modulus * &scale);
            format!("{remainder}e{}", self.exponent)
                .parse()
                .unwrap_or(f64::NAN)
        }
    }
}
pub(crate) fn rounded_ratio(numerator: BigUint, denominator: u64) -> f64 {
    let divisor = BigUint::from(denominator);
    let scaled = numerator * BigUint::from(10u8).pow(20);
    let mut quotient = &scaled / &divisor;
    if (&scaled % &divisor) * 2u8 >= divisor {
        quotient += 1u8;
    }
    format!("{quotient}e-20")
        .parse()
        .expect("finite decimal quotient")
}
