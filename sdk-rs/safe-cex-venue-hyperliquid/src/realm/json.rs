//! JSON.parse / JSON.stringify and the typed boundary to `serde_json::Value`.
use super::{
    Realm,
    value::{Kind, Obj, PropKey, Thrown, Value},
};
use crate::json_values::JsonValue;
use crate::spot_json::JsString;
use std::rc::Rc;

fn quote(units: &[u16], out: &mut Vec<u16>) {
    out.push(b'"' as u16);
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        let escape = |c: &str, out: &mut Vec<u16>| out.extend(c.encode_utf16());
        match u {
            0x22 => escape("\\\"", out),
            0x5c => escape("\\\\", out),
            0x08 => escape("\\b", out),
            0x0c => escape("\\f", out),
            0x0a => escape("\\n", out),
            0x0d => escape("\\r", out),
            0x09 => escape("\\t", out),
            u if u < 0x20 => escape(&format!("\\u{u:04x}"), out),
            0xd800..=0xdbff
                if units
                    .get(i + 1)
                    .is_some_and(|n| (0xdc00..0xe000).contains(n)) =>
            {
                out.push(u);
                out.push(units[i + 1]);
                i += 1;
            }
            // Well-formed JSON.stringify escapes lone surrogates.
            0xd800..=0xdfff => escape(&format!("\\u{u:04x}"), out),
            u => out.push(u),
        }
        i += 1;
    }
    out.push(b'"' as u16);
}

impl Realm {
    pub(crate) fn from_json(&self, value: JsonValue) -> Value {
        match value {
            JsonValue::Null => Value::Null,
            JsonValue::Bool(b) => Value::Bool(b),
            JsonValue::Number(n) => Value::Number(n),
            JsonValue::String(s) => Value::js(&s),
            JsonValue::Array(items) => {
                self.new_array(items.into_iter().map(|v| self.from_json(v)).collect())
            }
            JsonValue::Object(entries) => {
                let o = self.new_object();
                {
                    let mut data = o.object().unwrap().borrow_mut();
                    for (k, v) in entries.0 {
                        let key = PropKey::String(k.0.into());
                        let v = self.from_json(v);
                        data.put(key, v, true);
                    }
                }
                o
            }
        }
    }
    /// JSON.parse; a SyntaxError carries the parser's message.
    pub(crate) fn parse_json(&mut self, text: &[u16]) -> Result<Value, Thrown> {
        let text = String::from_utf16_lossy(text);
        match serde_json::from_str::<JsonValue>(&text) {
            Ok(v) => Ok(self.from_json(v)),
            Err(e) => Err(self.error_of("SyntaxError", &format!("Unexpected token in JSON: {e}"))),
        }
    }
    /// JSON.stringify(value) without replacer/space; None is a top-level undefined.
    pub(crate) fn stringify(&mut self, value: &Value) -> Result<Option<Vec<u16>>, Thrown> {
        let mut out = vec![];
        let mut stack = vec![];
        if self.write_json(value, &mut out, &mut stack, true)? {
            Ok(Some(out))
        } else {
            Ok(None)
        }
    }
    /// Returns false when the value serializes to nothing (undefined/function/symbol).
    fn write_json(
        &mut self,
        value: &Value,
        out: &mut Vec<u16>,
        stack: &mut Vec<Obj>,
        _top: bool,
    ) -> Result<bool, Thrown> {
        let mut value = value.clone();
        if value.object().is_some() || matches!(value, Value::Number(_) | Value::String(_)) {
            let to_json = match &value {
                Value::Object(_) => self.get_str(&value, "toJSON")?,
                _ => Value::Undefined,
            };
            if to_json.is_callable() {
                value = self.call(&to_json, &value, &[Value::str("")])?;
            }
        }
        match &value {
            Value::Undefined | Value::Symbol(_) => return Ok(false),
            Value::Null => out.extend("null".encode_utf16()),
            Value::Bool(b) => out.extend(if *b { "true" } else { "false" }.encode_utf16()),
            Value::Number(n) => {
                let text = if n.is_finite() {
                    safe_cex_num::js_number_string(*n)
                } else {
                    "null".into()
                };
                out.extend(text.encode_utf16());
            }
            Value::String(u) => quote(u, out),
            Value::Object(o) => {
                if value.is_callable() || matches!(o.borrow().kind, Kind::Function(_)) {
                    return Ok(false);
                }
                if stack.iter().any(|s| Rc::ptr_eq(s, o)) {
                    return Err(self.type_error("Converting circular structure to JSON"));
                }
                stack.push(o.clone());
                if value.is_array() {
                    let len = self.get_str(&value, "length")?;
                    let len = self.to_number(&len)? as usize;
                    out.push(b'[' as u16);
                    for i in 0..len {
                        if i > 0 {
                            out.push(b',' as u16);
                        }
                        let item = self.get(&value, &PropKey::str(&i.to_string()))?;
                        if !self.write_json(&item, out, stack, false)? {
                            out.extend("null".encode_utf16());
                        }
                    }
                    out.push(b']' as u16);
                } else {
                    out.push(b'{' as u16);
                    let mut first = true;
                    for key in self.own_keys(&value, true) {
                        let item = self.get(&value, &key)?;
                        let mut piece = vec![];
                        if !self.write_json(&item, &mut piece, stack, false)? {
                            continue;
                        }
                        if !first {
                            out.push(b',' as u16);
                        }
                        first = false;
                        quote(key.units().unwrap_or_default(), out);
                        out.push(b':' as u16);
                        out.extend(piece);
                    }
                    out.push(b'}' as u16);
                }
                stack.pop();
            }
        }
        Ok(true)
    }

    // ---- typed boundary ----
    pub(crate) fn from_serde(&self, value: &serde_json::Value) -> Value {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Number(n) => Value::Number(
                n.as_f64()
                    .unwrap_or_else(|| n.to_string().parse().unwrap_or(f64::NAN)),
            ),
            serde_json::Value::String(s) => Value::str(s),
            serde_json::Value::Array(items) => {
                self.new_array(items.iter().map(|v| self.from_serde(v)).collect())
            }
            serde_json::Value::Object(map) => {
                let o = self.new_object();
                {
                    let mut data = o.object().unwrap().borrow_mut();
                    for (k, v) in map {
                        data.put(PropKey::str(k), self.from_serde(v), true);
                    }
                }
                o
            }
        }
    }
    /// The JSON-shaped projection of a value: undefined members are omitted,
    /// non-finite numbers become null, as JSON.stringify would.
    pub(crate) fn to_serde(&mut self, value: &Value) -> Result<serde_json::Value, Thrown> {
        let Some(units) = self.stringify(value)? else {
            return Ok(serde_json::Value::Null);
        };
        serde_json::from_str(&String::from_utf16_lossy(&units))
            .map_err(|e| self.error(&format!("unserializable value: {e}")))
    }
    pub(crate) fn to_typed<T: serde::de::DeserializeOwned>(
        &mut self,
        value: &Value,
    ) -> Result<T, Thrown> {
        let value = self.capture_typed_value(value, 0)?;
        safe_cex_exchange::js_runtime::value::to_typed(&value)
            .map_err(|error| self.typed_error(error))
    }
    pub(crate) fn from_typed<T: serde::Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Value, Thrown> {
        let value = safe_cex_exchange::js_runtime::value::from_typed(value)
            .map_err(|error| self.typed_error(error))?;
        self.restore_typed_value(&value)
    }
    pub(crate) fn js_text(&self, text: &str) -> JsString {
        JsString(text.encode_utf16().collect())
    }
}

impl Realm {
    /// The signing view of a value: own enumerable data in key order, with
    /// `undefined` members retained (msgpack encodes them as nil; JSON drops them).
    pub(crate) fn to_js_json(
        &mut self,
        value: &Value,
    ) -> Result<safe_cex_num::JsJsonValue, Thrown> {
        use safe_cex_num::JsJsonValue as J;
        Ok(match value {
            Value::Undefined => J::Undefined,
            Value::Null => J::Null,
            Value::Bool(b) => J::Bool(*b),
            Value::Number(n) => J::Number(*n),
            Value::String(u) => J::String(String::from_utf16_lossy(u)),
            Value::Symbol(_) => J::Undefined,
            Value::Object(_) if value.is_array() => {
                let items = self.collect_iterable(value)?;
                J::Array(
                    items
                        .iter()
                        .map(|v| self.to_js_json(v))
                        .collect::<Result<_, _>>()?,
                )
            }
            Value::Object(_) => {
                let mut out = vec![];
                for (k, v) in self.own_entries(value)? {
                    if v.is_callable() {
                        continue;
                    }
                    out.push((
                        String::from_utf16_lossy(k.units().unwrap_or_default()),
                        self.to_js_json(&v)?,
                    ));
                }
                J::Object(out)
            }
        })
    }
}
