//! Abstract conversions and operators (ToPrimitive, ToString, ToNumber,
//! relational/arithmetic operators and loose equality).
use super::{
    Realm,
    value::{Kind, PropKey, Thrown, Value},
};
use std::rc::Rc;

fn units(text: &str) -> Rc<[u16]> {
    text.encode_utf16().collect::<Vec<_>>().into()
}

/// StringToNumber over UTF-16 (whitespace-trimmed decimal, hex/octal/binary, Infinity).
pub(crate) fn string_to_number(units: &[u16]) -> f64 {
    safe_cex_num::number_from_string(&String::from_utf16_lossy(units))
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Hint {
    Default,
    Number,
    String,
}

impl Realm {
    pub(crate) fn to_primitive(&mut self, v: &Value, hint: Hint) -> Result<Value, Thrown> {
        let Value::Object(o) = v else {
            return Ok(v.clone());
        };
        if let Kind::Date(t) = &o.borrow().kind
            && hint == Hint::Number
        {
            return Ok(Value::Number(*t));
        }
        let order = if hint == Hint::String {
            ["toString", "valueOf"]
        } else {
            ["valueOf", "toString"]
        };
        for name in order {
            let f = self.get_str(v, name)?;
            if f.is_callable() {
                let result = self.call(&f, v, &[])?;
                if result.object().is_none() {
                    return Ok(result);
                }
            }
        }
        Err(self.type_error("Cannot convert object to primitive value"))
    }
    pub(crate) fn to_string(&mut self, v: &Value) -> Result<Rc<[u16]>, Thrown> {
        Ok(match v {
            Value::Undefined => units("undefined"),
            Value::Null => units("null"),
            Value::Bool(b) => units(if *b { "true" } else { "false" }),
            Value::Number(n) => units(&safe_cex_num::js_number_string(*n)),
            Value::String(u) => u.clone(),
            Value::Symbol(_) => {
                return Err(self.type_error("Cannot convert a Symbol value to a string"));
            }
            Value::Object(_) => {
                let p = self.to_primitive(v, Hint::String)?;
                return self.to_string(&p);
            }
        })
    }
    pub(crate) fn to_rust_string(&mut self, v: &Value) -> Result<String, Thrown> {
        Ok(String::from_utf16_lossy(&self.to_string(v)?))
    }
    pub(crate) fn to_number(&mut self, v: &Value) -> Result<f64, Thrown> {
        Ok(match v {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => f64::from(u8::from(*b)),
            Value::Number(n) => *n,
            Value::String(u) => string_to_number(u),
            Value::Symbol(_) => {
                return Err(self.type_error("Cannot convert a Symbol value to a number"));
            }
            Value::Object(_) => {
                let p = self.to_primitive(v, Hint::Number)?;
                return self.to_number(&p);
            }
        })
    }
    pub(crate) fn to_property_key(&mut self, v: &Value) -> Result<PropKey, Thrown> {
        Ok(match v {
            Value::Symbol(s) => PropKey::Symbol(s.clone()),
            Value::String(u) => PropKey::String(u.clone()),
            other => {
                let p = self.to_primitive(other, Hint::String)?;
                match p {
                    Value::Symbol(s) => PropKey::Symbol(s),
                    p => PropKey::String(self.to_string(&p)?),
                }
            }
        })
    }
    pub(crate) fn index(&mut self, o: &Value, key: &Value) -> Result<Value, Thrown> {
        if o.is_nullish() {
            let name = self.to_rust_string(key).unwrap_or_default();
            let what = if matches!(o, Value::Null) {
                "null"
            } else {
                "undefined"
            };
            return Err(self.type_error(&format!(
                "Cannot read properties of {what} (reading '{name}')"
            )));
        }
        let key = self.to_property_key(key)?;
        self.get(o, &key)
    }
    pub(crate) fn set_index(&mut self, o: &Value, key: &Value, value: Value) -> Result<(), Thrown> {
        let key = self.to_property_key(key)?;
        self.set(o, &key, value)
    }
    /// IsLooselyEqual (`==`).
    pub(crate) fn loose_equal(&mut self, a: &Value, b: &Value) -> Result<bool, Thrown> {
        Ok(match (a, b) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Number(_), Value::String(_)) | (Value::String(_), Value::Number(_)) => {
                self.to_number(a)? == self.to_number(b)?
            }
            (Value::Bool(_), _) => {
                let n = Value::Number(self.to_number(a)?);
                return self.loose_equal(&n, b);
            }
            (_, Value::Bool(_)) => {
                let n = Value::Number(self.to_number(b)?);
                return self.loose_equal(a, &n);
            }
            (Value::Object(_), Value::Object(_)) => a.strict_equal(b),
            (Value::Object(_), _) => {
                let p = self.to_primitive(a, Hint::Default)?;
                return self.loose_equal(&p, b);
            }
            (_, Value::Object(_)) => {
                let p = self.to_primitive(b, Hint::Default)?;
                return self.loose_equal(a, &p);
            }
            _ => a.strict_equal(b),
        })
    }
    /// Abstract relational comparison `a < b`; None is undefined (NaN involved).
    pub(crate) fn less_than(
        &mut self,
        a: &Value,
        b: &Value,
        left_first: bool,
    ) -> Result<Option<bool>, Thrown> {
        let (pa, pb) = if left_first {
            let pa = self.to_primitive(a, Hint::Number)?;
            (pa, self.to_primitive(b, Hint::Number)?)
        } else {
            let pb = self.to_primitive(b, Hint::Number)?;
            (self.to_primitive(a, Hint::Number)?, pb)
        };
        if let (Value::String(x), Value::String(y)) = (&pa, &pb) {
            return Ok(Some(x[..] < y[..]));
        }
        let (x, y) = (self.to_number(&pa)?, self.to_number(&pb)?);
        if x.is_nan() || y.is_nan() {
            return Ok(None);
        }
        Ok(Some(x < y))
    }
    pub(crate) fn add(&mut self, a: &Value, b: &Value) -> Result<Value, Thrown> {
        let pa = self.to_primitive(a, Hint::Default)?;
        let pb = self.to_primitive(b, Hint::Default)?;
        if matches!(pa, Value::String(_)) || matches!(pb, Value::String(_)) {
            let mut out = self.to_string(&pa)?.to_vec();
            out.extend_from_slice(&self.to_string(&pb)?);
            return Ok(Value::String(out.into()));
        }
        Ok(Value::Number(self.to_number(&pa)? + self.to_number(&pb)?))
    }
    /// Binary operators by source spelling.
    pub(crate) fn binary(&mut self, op: &str, a: &Value, b: &Value) -> Result<Value, Thrown> {
        // Some ported bodies name operators instead of spelling them.
        let op = match op {
            "plus" | "add" => "+",
            "minus" | "subtract" => "-",
            "multiply" => "*",
            "divide" => "/",
            "less" => "<",
            "lessEqual" => "<=",
            "greater" => ">",
            "greaterEqual" => ">=",
            other => other,
        };
        let numeric = |realm: &mut Realm, f: fn(f64, f64) -> f64| -> Result<Value, Thrown> {
            let x = realm.to_number(a)?;
            let y = realm.to_number(b)?;
            Ok(Value::Number(f(x, y)))
        };
        Ok(match op {
            "+" => return self.add(a, b),
            "-" => return numeric(self, |x, y| x - y),
            "*" => return numeric(self, |x, y| x * y),
            "/" => return numeric(self, |x, y| x / y),
            "%" => return numeric(self, |x, y| x % y),
            "**" => return numeric(self, f64::powf),
            "<" => Value::Bool(self.less_than(a, b, true)? == Some(true)),
            ">" => Value::Bool(self.less_than(b, a, false)? == Some(true)),
            "<=" => Value::Bool(self.less_than(b, a, false)? == Some(false)),
            ">=" => Value::Bool(self.less_than(a, b, true)? == Some(false)),
            "===" => Value::Bool(a.strict_equal(b)),
            "!==" => Value::Bool(!a.strict_equal(b)),
            "==" => Value::Bool(self.loose_equal(a, b)?),
            "!=" => Value::Bool(!self.loose_equal(a, b)?),
            "&&" => {
                if a.truthy() {
                    b.clone()
                } else {
                    a.clone()
                }
            }
            "||" => {
                if a.truthy() {
                    a.clone()
                } else {
                    b.clone()
                }
            }
            "??" => {
                if a.is_nullish() {
                    b.clone()
                } else {
                    a.clone()
                }
            }
            "in" => {
                let key = self.to_property_key(a)?;
                Value::Bool(self.has_property(b, &key)?)
            }
            "instanceof" => Value::Bool(self.instance_of(a, b)?),
            "|" | "&" | "^" | "<<" | ">>" | ">>>" => {
                let x = self.to_number(a)?;
                let y = self.to_number(b)?;
                let (xi, yi) = (to_int32(x), to_int32(y));
                Value::Number(match op {
                    "|" => (xi | yi) as f64,
                    "&" => (xi & yi) as f64,
                    "^" => (xi ^ yi) as f64,
                    "<<" => xi.wrapping_shl(yi as u32 & 31) as f64,
                    ">>" => (xi >> (yi as u32 & 31)) as f64,
                    _ => ((xi as u32) >> (yi as u32 & 31)) as f64,
                })
            }
            other => return Err(self.error(&format!("unsupported operator {other}"))),
        })
    }
    pub(crate) fn unary(&mut self, op: &str, v: &Value) -> Result<Value, Thrown> {
        Ok(match op {
            "+" | "plus" => Value::Number(self.to_number(v)?),
            "-" | "minus" => Value::Number(-self.to_number(v)?),
            "!" => Value::Bool(!v.truthy()),
            "typeof" => Value::str(v.type_of()),
            "~" => Value::Number(!to_int32(self.to_number(v)?) as f64),
            other => return Err(self.error(&format!("unsupported unary operator {other}"))),
        })
    }
    /// `${value}` (ToString, symbols throw).
    pub(crate) fn template(&mut self, v: &Value) -> Result<Rc<[u16]>, Thrown> {
        self.to_string(v)
    }
}

pub(crate) fn to_int32(n: f64) -> i32 {
    if !n.is_finite() || n == 0.0 {
        return 0;
    }
    let int = n.trunc();
    let modulo = int.rem_euclid(4294967296.0);
    (if modulo >= 2147483648.0 {
        modulo - 4294967296.0
    } else {
        modulo
    }) as i32
}
