//! Source operator spelling adapter over canonical scalar operations.
use super::Host;
use safe_cex_exchange::js_runtime::{ops, value::{Js, Thrown}};
impl Host {
    pub(crate) fn add(&mut self, a: &Js, b: &Js) -> Result<Js,Thrown> { ops::add(a,b) }
    pub(crate) fn binary(&mut self, op: &str, a: &Js, b: &Js) -> Result<Js,Thrown> {
        match op {
            "+" | "plus" | "add" => ops::add(a,b),
            "-" | "minus" | "subtract" => ops::subtract(a,b),
            "*" | "multiply" => ops::multiply(a,b),
            "/" | "divide" => ops::divide(a,b),
            "%" => ops::remainder(a,b),
            "<" | "less" => ops::less_than(a,b).map(Js::Bool),
            ">" | "greater" => ops::greater_than(a,b).map(Js::Bool),
            "<=" | "lessEqual" => ops::less_equal(a,b).map(Js::Bool),
            ">=" | "greaterEqual" => ops::greater_equal(a,b).map(Js::Bool),
            "===" => Ok(Js::Bool(a.strict_equals(b))),
            "!==" => Ok(Js::Bool(!a.strict_equals(b))),
            "||" => Ok(if a.truthy() { a.clone() } else { b.clone() }),
            "&&" => Ok(if a.truthy() { b.clone() } else { a.clone() }),
            "??" => Ok(if a.is_nullish() { b.clone() } else { a.clone() }),
            "in" => self.has_property(b,a).map(Js::Bool),
            "instanceof" => self.instance_of(a,b).map(Js::Bool),
            other => Err(Thrown::error("Error", &format!("Unsupported HL operator {other}"))),
        }
    }
    pub(crate) fn unary(&mut self, op: &str, value: &Js) -> Result<Js,Thrown> {
        match op {
            "+" | "plus" => value.to_number().map(Js::Number),
            "-" | "minus" => ops::negate(value),
            "!" => Ok(Js::Bool(!value.truthy())),
            "typeof" => Ok(Js::str(value.type_of())),
            other => Err(Thrown::error("Error", &format!("Unsupported HL unary operator {other}"))),
        }
    }
}
