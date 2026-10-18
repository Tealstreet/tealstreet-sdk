//! Serde's native data model at the typed boundary, before JSON wire projection.
use super::{Js, Thrown};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::de;

fn fault(error: impl std::fmt::Display) -> Thrown {
    Thrown::type_error(&error.to_string())
}

pub(super) fn encode<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, Thrown> {
    safe_cex_num::numeric_ingress::js_capture_output(value)
        .map(from_input)
        .map_err(fault)
}

pub(super) fn from_input(value: Input) -> Js {
    match value {
        Input::Undefined => Js::Undefined,
        Input::Null => Js::Null,
        Input::Bool(value) => Js::Bool(value),
        Input::Text(value) => Js::utf16(&value),
        Input::Float(value) => Js::Number(value),
        Input::Lexeme(value) => Js::Number(value.parse().expect("captured numeric lexeme")),
        Input::Array(values) => Js::array(values.into_iter().map(from_input).collect()),
        Input::Object(values) => {
            let object = Js::object(vec![]);
            for (key, value) in values {
                object
                    .define_key(&key, from_input(value))
                    .expect("fresh object data property");
            }
            object
        }
    }
}

pub(super) fn capture(
    value: &Js,
    depth: usize,
    present_fields: bool,
    data_only: bool,
) -> Result<Input, Thrown> {
    if depth >= 128 {
        return Err(Thrown::type_error("typed value recursion limit exceeded"));
    }
    if present_fields && value.is_callable() {
        return Err(Thrown::type_error("callable cannot enter a declared DTO"));
    }
    Ok(match value {
        Js::SymbolIterator => return Err(fault("Cannot decode a Symbol as typed data")),
        Js::Undefined => Input::Undefined,
        Js::Null | Js::Function(_) | Js::Promise(_) => Input::Null,
        Js::Bool(value) => Input::Bool(*value),
        Js::Number(value) => Input::Float(*value),
        Js::BigInt(_) if data_only => {
            return Err(Thrown::type_error("Do not know how to serialize a BigInt"));
        }
        Js::BigInt(value) => Input::Text(value.to_string().encode_utf16().collect()),
        Js::String(value) => Input::Text(value.to_vec()),
        Js::Array(values) => {
            let length = values.len();
            let mut out = Vec::new();
            for index in 0..length {
                out.push(capture(
                    &value.get(&index.to_string())?,
                    depth + 1,
                    present_fields,
                    data_only,
                )?);
            }
            Input::Array(out)
        }
        Js::Builtin(reference) if reference.is_callable() => Input::Null,
        Js::Object(_) | Js::Builtin(_) => {
            let entries = value.own_entries()?;
            let mut fields = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                if !present_fields && (matches!(value, Js::Function(_)) || value.is_callable()) {
                    continue;
                }
                fields.push((
                    key.to_vec(),
                    capture(&value, depth + 1, present_fields, data_only)?,
                ));
            }
            Input::Object(fields)
        }
        Js::Map(_) | Js::Set(_) => Input::Object(vec![]),
    })
}

pub(super) fn decode<T: de::DeserializeOwned>(
    value: &Js,
    present_fields: bool,
) -> Result<T, Thrown> {
    capture(value, 0, present_fields, false)?
        .decode_native(present_fields)
        .map_err(fault)
}
