use super::Host;
use safe_cex_exchange::js_runtime::{
    globals::stringify_json,
    value::{Js, Thrown},
};
use std::rc::Rc;

#[derive(Clone)]
struct Parts {
    r: String,
    s: String,
    v: f64,
}

fn diagnostic(value: &Js, seen: &mut Vec<Js>) -> Result<String, Thrown> {
    if value.is_nullish() {
        return Ok("null".into());
    }
    if value.is_object() {
        if seen.iter().any(|old| old.same_reference(value)) {
            return Ok("[Circular]".into());
        }
        seen.push(value.clone());
    }
    if matches!(value, Js::Array(_)) {
        let len = value.get("length")?.to_number()? as usize;
        let mut cells = Vec::new();
        for i in 0..len {
            let key = i.to_string();
            cells.push(
                if value.has_property(&key.encode_utf16().collect::<Vec<_>>()) {
                    diagnostic(&value.get(&key)?, seen)?
                } else {
                    String::new()
                },
            );
        }
        return Ok(format!("[ {} ]", cells.join(", ")));
    }
    if value.is_object() {
        let json = value.get("toJSON")?;
        if json.is_callable() {
            return diagnostic(&json.call(value.clone(), vec![])?, seen);
        }
        let mut keys = value.own_keys();
        keys.sort();
        let mut fields = Vec::new();
        for key in keys {
            fields.push(format!(
                "{}: {}",
                diagnostic(&Js::String(key.clone()), seen)?,
                diagnostic(&value.get_key(&key)?, seen)?
            ));
        }
        return Ok(format!("{{ {} }}", fields.join(", ")));
    }
    if matches!(value, Js::String(_)) {
        return Ok(String::from_utf16_lossy(
            &stringify_json(value)?.expect("string JSON"),
        ));
    }
    if matches!(value, Js::Function(_) | Js::Builtin(_)) {
        return Ok("[ COULD NOT SERIALIZE ]".into());
    }
    Ok(value.to_display())
}

fn error(short: &str, code: &str, fields: Vec<(&str, Js)>) -> Thrown {
    let make = || -> Result<Thrown, Thrown> {
        let mut details = Vec::new();
        for (name, value) in &fields {
            details.push(format!("{name}={}", diagnostic(value, &mut vec![])?));
        }
        details.push(format!("code={code}"));
        details.push("version=6.17.0".into());
        let thrown = Thrown::error(
            if code == "INVALID_ARGUMENT" {
                "TypeError"
            } else {
                "RangeError"
            },
            &format!("{short} ({})", details.join(", ")),
        );
        thrown.value.define_property(
            &"code".encode_utf16().collect::<Vec<_>>(),
            &Js::object(vec![
                ("value", Js::str(code)),
                ("enumerable", Js::Bool(true)),
            ]),
        )?;
        for (name, value) in &fields {
            thrown.value.define(name, value.clone())?;
        }
        thrown.value.define_property(
            &"shortMessage".encode_utf16().collect::<Vec<_>>(),
            &Js::object(vec![
                ("value", Js::str(short)),
                ("enumerable", Js::Bool(true)),
            ]),
        )?;
        Ok(thrown)
    };
    make().unwrap_or_else(std::convert::identity)
}
fn invalid(short: &str, argument: &str, value: &Js) -> Thrown {
    error(
        short,
        "INVALID_ARGUMENT",
        vec![("argument", Js::str(argument)), ("value", value.clone())],
    )
}
fn bigint(host: &mut Host, value: &Js, argument: &str) -> Result<Js, Thrown> {
    match value {
        Js::BigInt(_) => return Ok(value.clone()),
        Js::Number(n) => {
            if !n.is_finite() || n.fract() != 0.0 {
                return Err(invalid("underflow", argument, value));
            }
            if n.abs() > 9007199254740991.0 {
                return Err(invalid("overflow", argument, value));
            }
        }
        Js::String(text) => {
            if text.is_empty() {
                return Err(invalid(
                    "invalid BigNumberish string: empty string",
                    argument,
                    value,
                ));
            }
        }
        _ => return Err(invalid("invalid BigNumberish value", argument, value)),
    }
    let mut input = value.clone();
    let mut negative = false;
    if let Js::String(text) = value
        && text.first() == Some(&45)
        && text.get(1) != Some(&45)
    {
        input = Js::utf16(&text[1..]);
        negative = true;
    }
    let parsed = match input {
        Js::String(ref units) => safe_cex_exchange::js_runtime::ops::string_to_bigint(units)
            .map(|value| Js::BigInt(Rc::new(value))),
        Js::Number(n) => safe_cex_num::js_bigint_primitive(&safe_cex_num::JsPrimitive::Number(n))
            .ok()
            .map(|value| Js::BigInt(Rc::new(value))),
        _ => unreachable!(),
    }
    .ok_or(());
    let parsed = parsed.map_err(|_| {
        invalid(
            &format!(
                "invalid BigNumberish string: Cannot convert {} to a BigInt",
                value.to_display()
            ),
            argument,
            value,
        )
    })?;
    if negative {
        host.unary("-", &parsed)
    } else {
        Ok(parsed)
    }
}
fn number(value: &Js, argument: &str) -> Result<f64, Thrown> {
    match value {
        Js::Number(n) => {
            if !n.is_finite() || n.fract() != 0.0 {
                return Err(invalid("underflow", argument, value));
            }
            if n.abs() > 9007199254740991.0 {
                return Err(invalid("overflow", argument, value));
            }
            Ok(*n)
        }
        Js::BigInt(n) => {
            let converted: f64 = n.to_string().parse().expect("BigInt decimal spelling");
            if converted.abs() > 9007199254740991.0 {
                return Err(invalid("overflow", argument, value));
            }
            Ok(converted)
        }
        Js::String(units) => {
            let parsed = if units.is_empty() {
                Err(Thrown::error("Error", "empty string"))
            } else {
                safe_cex_exchange::js_runtime::ops::string_to_bigint(units)
                    .ok_or_else(|| {
                        Thrown::error(
                            "SyntaxError",
                            &format!("Cannot convert {} to a BigInt", value.to_display()),
                        )
                    })
                    .and_then(|n| number(&Js::BigInt(Rc::new(n)), argument))
            };
            parsed.map_err(|cause| {
                invalid(
                    &format!(
                        "invalid numeric string: {}",
                        cause
                            .value
                            .get("message")
                            .expect("native parser error message")
                            .to_display()
                    ),
                    argument,
                    value,
                )
            })
        }
        _ => Err(invalid("invalid numeric value", argument, value)),
    }
}
fn uint256(host: &mut Host, value: &Js) -> Result<String, Thrown> {
    let Js::BigInt(number) = bigint(host, value, "value")? else {
        unreachable!()
    };
    let hex = number.to_str_radix(16);
    if hex.starts_with('-') {
        return Err(error(
            "unsigned value cannot be negative",
            "NUMERIC_FAULT",
            vec![
                ("fault", Js::str("overflow")),
                ("operation", Js::str("getUint")),
                ("value", value.clone()),
            ],
        ));
    }
    if hex.len() > 64 {
        let size = hex.len().div_ceil(2);
        return Err(error(
            "padding exceeds data length",
            "BUFFER_OVERRUN",
            vec![
                (
                    "buffer",
                    Js::str(&format!("0x{hex:0>width$}", width = size * 2)),
                ),
                ("length", Js::Number(32.0)),
                ("offset", Js::Number((size - 32) as f64)),
            ],
        ));
    }
    Ok(format!("0x{hex:0>64}"))
}
fn normalized_v(host: &mut Host, value: &Js) -> Result<f64, Thrown> {
    let Js::BigInt(number) = bigint(host, value, "value")? else {
        unreachable!()
    };
    let text = number.to_string();
    match text.as_str() {
        "0" | "27" => Ok(27.0),
        "1" | "28" => Ok(28.0),
        _ if !text.starts_with('-') && (text.len() > 2 || text.as_str() >= "35") => Ok(
            if text.as_bytes().last().expect("integer spelling") % 2 == 1 {
                27.0
            } else {
                28.0
            },
        ),
        _ => Err(invalid("invalid v", "v", &Js::BigInt(number.clone()))),
    }
}
fn compact(value: &Js, signature: &Js) -> Result<String, Thrown> {
    let Js::String(units) = value else {
        return Err(invalid("invalid yParityAndS", "signature", signature));
    };
    let text = String::from_utf16_lossy(units);
    if text.len() != 66
        || !text.starts_with("0x")
        || !text[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid("invalid yParityAndS", "signature", signature));
    }
    Ok(text)
}
fn materialize(host: &Host, parts: Parts) -> Js {
    let object = Js::host_object(Rc::new(parts.clone()));
    for (name, text) in [("r", parts.r.clone()), ("s", parts.s.clone())] {
        let getter = host.native(name, 0, move |_, _, _| {
            if name == "s" && text.as_bytes()[2] >= b'8' {
                return Err(invalid("non-canonical s; use ._s", "s", &Js::str(&text)));
            }
            Ok(Js::str(&text))
        });
        host.define_getter(&object, Js::str(name), getter);
    }
    host.define_getter(
        &object,
        Js::str("v"),
        host.native("v", 0, move |_, _, _| Ok(Js::Number(parts.v))),
    );
    object
}

pub(super) fn from(host: &mut Host, signature: &Js) -> Result<Js, Thrown> {
    let parts = if signature.is_nullish() {
        Parts {
            r: format!("0x{}", "0".repeat(64)),
            s: format!("0x{}", "0".repeat(64)),
            v: 27.0,
        }
    } else if let Js::String(units) = signature {
        let text = String::from_utf16_lossy(units);
        let bytes = crate::signing::address_to_bytes(&text)
            .map_err(|_| invalid("invalid BytesLike value", "signature", signature))?;
        if !matches!(bytes.len(), 64 | 65) {
            return Err(invalid(
                "invalid raw signature length",
                "signature",
                signature,
            ));
        }
        let r = bytes[..32]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut s = bytes[32..64].to_vec();
        let v = if bytes.len() == 64 {
            let v = 27.0 + f64::from(s[0] >> 7);
            s[0] &= 127;
            v
        } else {
            normalized_v(host, &Js::Number(f64::from(bytes[64])))?
        };
        Parts {
            r: format!("0x{r}"),
            s: format!(
                "0x{}",
                s.iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ),
            v,
        }
    } else if let Some(parts) = signature.resource::<Parts>() {
        (*parts).clone()
    } else {
        let raw_r = signature.get("r")?;
        if raw_r.is_nullish() {
            return Err(invalid("missing r", "signature", signature));
        }
        let r = uint256(host, &raw_r)?;
        let raw_s = signature.get("s")?;
        let raw_compact = signature.get("yParityAndS")?;
        let s = if !raw_s.is_nullish() {
            uint256(host, &raw_s)?
        } else if !raw_compact.is_nullish() {
            let text = compact(&raw_compact, signature)?;
            let first = u8::from_str_radix(&text[2..4], 16).expect("checked hex") & 127;
            format!("0x{first:02x}{}", text[4..].to_ascii_lowercase())
        } else {
            return Err(invalid("missing s", "signature", signature));
        };
        let raw_v = signature.get("v")?;
        let raw_compact = signature.get("yParityAndS")?;
        let raw_parity = signature.get("yParity")?;
        let v = if !raw_v.is_nullish() {
            normalized_v(host, &raw_v)?
        } else if !raw_compact.is_nullish() {
            let text = compact(&raw_compact, signature)?;
            27.0 + f64::from(u8::from_str_radix(&text[2..4], 16).expect("checked hex") >> 7)
        } else if !raw_parity.is_nullish() {
            let n = number(&raw_parity, "sig.yParity")?;
            match n {
                0.0 => 27.0,
                1.0 => 28.0,
                _ => return Err(invalid("invalid yParity", "signature", signature)),
            }
        } else {
            return Err(invalid("missing v", "signature", signature));
        };
        if !signature.get("yParity")?.is_nullish() {
            let parity = signature.get("yParity")?;
            let n = number(&parity, "sig.yParity")?;
            if n != if v == 27.0 { 0.0 } else { 1.0 } {
                return Err(invalid("yParity mismatch", "signature", signature));
            }
        }
        if !signature.get("yParityAndS")?.is_nullish() {
            let actual = signature.get("yParityAndS")?;
            if s.as_bytes()[2] >= b'8' {
                return Err(invalid("non-canonical s; use ._s", "s", &Js::str(&s)));
            }
            let first = u8::from_str_radix(&s[2..4], 16).expect("normalized scalar")
                | if v == 28.0 { 128 } else { 0 };
            let expected = Js::str(&format!("0x{first:02x}{}", &s[4..]));
            if !actual.strict_equals(&expected) {
                return Err(invalid("yParityAndS mismatch", "signature", signature));
            }
        }
        Parts { r, s, v }
    };
    Ok(materialize(host, parts))
}

#[cfg(test)]
mod clone_tests {
    use super::Parts;

    #[test]
    fn signature_parts_clone_preserves_bits_and_owns_distinct_strings() {
        for v in [27.0_f64, 28.0, -0.0, f64::from_bits(0x7ff8_0000_0000_0042)] {
            let original = Parts {
                r: format!("0x{:064x}", 1),
                s: format!("0x{:064x}", 2),
                v,
            };
            let mut clone = original.clone();
            assert_eq!(original.r, clone.r);
            assert_eq!(original.s, clone.s);
            assert_ne!(original.r.as_ptr(), clone.r.as_ptr());
            assert_ne!(original.s.as_ptr(), clone.s.as_ptr());
            assert_eq!(original.v.to_bits(), clone.v.to_bits());
            clone.r.push('3');
            clone.s.push('4');
            assert_eq!(original.r, format!("0x{:064x}", 1));
            assert_eq!(original.s, format!("0x{:064x}", 2));
        }
    }
}
