//! Native JS value model the concrete binding hands to every ported body.
//! Strings are UTF-16, arrays keep holes, objects keep own-key order and accessors.
use indexmap::IndexMap;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, js_number_string, parse_float};
use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
};

pub type Str = Rc<[u16]>;
pub type NativeFn = Rc<dyn Fn(Js, Vec<Js>) -> Result<Js, Thrown>>;
pub type Getter = Rc<dyn Fn(&Js) -> Result<Js, Thrown>>;
pub type Setter = Rc<dyn Fn(&Js, Js) -> Result<(), Thrown>>;

pub fn utf16(text: &str) -> Str {
    text.encode_utf16().collect::<Vec<_>>().into()
}

#[derive(Clone)]
pub enum Prop {
    Data(Js),
    Accessor(Option<Getter>, Option<Setter>),
}

pub struct ObjectData {
    pub class: &'static str,
    pub props: RefCell<IndexMap<Str, Prop>>,
    pub proto: RefCell<Option<Js>>,
}

pub struct FunctionData {
    pub name: Str,
    pub call: NativeFn,
    pub props: RefCell<IndexMap<Str, Prop>>,
    pub constructor: bool,
}

#[derive(Clone)]
pub enum Js {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Str),
    Array(Rc<RefCell<Vec<Option<Js>>>>),
    Object(Rc<ObjectData>),
    Function(Rc<FunctionData>),
    Promise(super::promise::Promise),
    Map(Rc<RefCell<Vec<(Js, Js)>>>),
    Set(Rc<RefCell<Vec<Js>>>),
}

/// A thrown JS value. Native failures keep their ExchangeError metadata.
#[derive(Clone)]
pub struct Thrown {
    pub value: Js,
    pub native: Option<Rc<ExchangeError>>,
}

impl Thrown {
    pub fn new(value: Js) -> Self {
        Self {
            value,
            native: None,
        }
    }
    pub fn error(name: &str, message: &str) -> Self {
        Self::new(error_object(name, message))
    }
    pub fn type_error(message: &str) -> Self {
        Self::error("TypeError", message)
    }
    pub fn message(&self) -> String {
        match &self.value {
            Js::Object(_) | Js::Function(_) => self
                .value
                .get("message")
                .ok()
                .map(|m| m.to_display())
                .unwrap_or_default(),
            other => other.to_display(),
        }
    }
    pub fn to_exchange_error(&self) -> ExchangeError {
        if let Some(native) = &self.native {
            return (**native).clone();
        }
        let mut error = ExchangeError::new(self.message());
        if let Ok(name) = self.value.get("name")
            && let Js::String(name) = name
        {
            error.name = String::from_utf16_lossy(&name);
        }
        error
    }
}

impl fmt::Debug for Thrown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Thrown({})", self.message())
    }
}

impl From<ExchangeError> for Thrown {
    fn from(error: ExchangeError) -> Self {
        let value = error_object(&error.name, &error.message);
        Self {
            value,
            native: Some(Rc::new(error)),
        }
    }
}

impl From<JsCoercionError> for Thrown {
    fn from(error: JsCoercionError) -> Self {
        Self::error(error.name, &error.message)
    }
}

impl From<Thrown> for ExchangeError {
    fn from(thrown: Thrown) -> Self {
        thrown.to_exchange_error()
    }
}

pub fn error_object(name: &str, message: &str) -> Js {
    let object = Js::object_with_class("Error", vec![]);
    let _ = object.define("message", Js::str(message));
    let _ = object.define("name", Js::str(name));
    object
}

impl fmt::Debug for Js {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Js::Undefined => write!(f, "undefined"),
            Js::Null => write!(f, "null"),
            Js::Bool(b) => write!(f, "{b}"),
            Js::Number(n) => write!(f, "{}", js_number_string(*n)),
            Js::String(s) => write!(f, "{:?}", String::from_utf16_lossy(s)),
            Js::Array(a) => f.debug_list().entries(a.borrow().iter()).finish(),
            Js::Object(o) => {
                let props = o.props.borrow();
                let mut map = f.debug_map();
                for (k, v) in props.iter() {
                    match v {
                        Prop::Data(v) => map.entry(&String::from_utf16_lossy(k), v),
                        Prop::Accessor(..) => map.entry(&String::from_utf16_lossy(k), &"<accessor>"),
                    };
                }
                map.finish()
            }
            Js::Function(func) => write!(f, "[Function {}]", String::from_utf16_lossy(&func.name)),
            Js::Promise(_) => write!(f, "[Promise]"),
            Js::Map(m) => write!(f, "Map({})", m.borrow().len()),
            Js::Set(s) => write!(f, "Set({})", s.borrow().len()),
        }
    }
}

impl Js {
    pub fn str(text: &str) -> Js {
        Js::String(utf16(text))
    }
    pub fn utf16(text: &[u16]) -> Js {
        Js::String(text.into())
    }
    pub fn array(items: Vec<Js>) -> Js {
        Js::Array(Rc::new(RefCell::new(items.into_iter().map(Some).collect())))
    }
    pub fn holey_array(items: Vec<Option<Js>>) -> Js {
        Js::Array(Rc::new(RefCell::new(items)))
    }
    pub fn object(fields: Vec<(&str, Js)>) -> Js {
        Js::object_with_class("Object", fields)
    }
    pub fn object_with_class(class: &'static str, fields: Vec<(&str, Js)>) -> Js {
        Js::Object(Rc::new(ObjectData {
            class,
            props: RefCell::new(
                fields
                    .into_iter()
                    .map(|(k, v)| (utf16(k), Prop::Data(v)))
                    .collect(),
            ),
            proto: RefCell::new(None),
        }))
    }
    pub fn function(name: &str, call: impl Fn(Js, Vec<Js>) -> Result<Js, Thrown> + 'static) -> Js {
        Js::Function(Rc::new(FunctionData {
            name: utf16(name),
            call: Rc::new(call),
            props: RefCell::new(IndexMap::new()),
            constructor: false,
        }))
    }
    pub fn constructor(
        name: &str,
        call: impl Fn(Js, Vec<Js>) -> Result<Js, Thrown> + 'static,
    ) -> Js {
        Js::Function(Rc::new(FunctionData {
            name: utf16(name),
            call: Rc::new(call),
            props: RefCell::new(IndexMap::new()),
            constructor: true,
        }))
    }
    pub fn new_map() -> Js {
        Js::Map(Rc::new(RefCell::new(vec![])))
    }
    pub fn new_set() -> Js {
        Js::Set(Rc::new(RefCell::new(vec![])))
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Js::Undefined)
    }
    pub fn is_nullish(&self) -> bool {
        matches!(self, Js::Undefined | Js::Null)
    }
    pub fn is_array(&self) -> bool {
        matches!(self, Js::Array(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(self, Js::String(_))
    }
    pub fn is_object(&self) -> bool {
        !matches!(
            self,
            Js::Undefined | Js::Null | Js::Bool(_) | Js::Number(_) | Js::String(_)
        )
    }
    pub fn is_callable(&self) -> bool {
        matches!(self, Js::Function(_))
    }
    pub fn type_of(&self) -> &'static str {
        match self {
            Js::Undefined => "undefined",
            Js::Null => "object",
            Js::Bool(_) => "boolean",
            Js::Number(_) => "number",
            Js::String(_) => "string",
            Js::Function(_) => "function",
            _ => "object",
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Js::Undefined | Js::Null => false,
            Js::Bool(b) => *b,
            Js::Number(n) => *n != 0.0 && !n.is_nan(),
            Js::String(s) => !s.is_empty(),
            _ => true,
        }
    }
    /// Strict equality (===).
    pub fn strict_equals(&self, other: &Js) -> bool {
        match (self, other) {
            (Js::Undefined, Js::Undefined) | (Js::Null, Js::Null) => true,
            (Js::Bool(a), Js::Bool(b)) => a == b,
            (Js::Number(a), Js::Number(b)) => a == b,
            (Js::String(a), Js::String(b)) => a == b,
            _ => self.same_reference(other),
        }
    }
    pub fn same_value_zero(&self, other: &Js) -> bool {
        match (self, other) {
            (Js::Number(a), Js::Number(b)) if a.is_nan() && b.is_nan() => true,
            _ => self.strict_equals(other),
        }
    }
    pub fn same_reference(&self, other: &Js) -> bool {
        match (self, other) {
            (Js::Array(a), Js::Array(b)) => Rc::ptr_eq(a, b),
            (Js::Object(a), Js::Object(b)) => Rc::ptr_eq(a, b),
            (Js::Function(a), Js::Function(b)) => Rc::ptr_eq(a, b),
            (Js::Promise(a), Js::Promise(b)) => a.ptr_eq(b),
            (Js::Map(a), Js::Map(b)) => Rc::ptr_eq(a, b),
            (Js::Set(a), Js::Set(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// ToString for primitives; objects use their default `[object X]` text.
    pub fn to_js_string(&self) -> Result<Str, Thrown> {
        Ok(match self {
            Js::Undefined => utf16("undefined"),
            Js::Null => utf16("null"),
            Js::Bool(b) => utf16(if *b { "true" } else { "false" }),
            Js::Number(n) => utf16(&js_number_string(*n)),
            Js::String(s) => s.clone(),
            Js::Array(items) => {
                let items = items.borrow().clone();
                let mut out: Vec<u16> = vec![];
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',' as u16);
                    }
                    if let Some(item) = item
                        && !item.is_nullish()
                    {
                        out.extend_from_slice(&item.to_js_string()?);
                    }
                }
                out.into()
            }
            Js::Object(o) => {
                if o.class == "Error" {
                    let name = self.get("name")?;
                    let message = self.get("message")?;
                    let name = if name.is_undefined() { utf16("Error") } else { name.to_js_string()? };
                    let message = if message.is_undefined() { utf16("") } else { message.to_js_string()? };
                    if message.is_empty() {
                        name
                    } else {
                        let mut out = name.to_vec();
                        out.extend(": ".encode_utf16());
                        out.extend_from_slice(&message);
                        out.into()
                    }
                } else {
                    utf16("[object Object]")
                }
            }
            Js::Function(f) => {
                let mut out: Vec<u16> = "function ".encode_utf16().collect();
                out.extend_from_slice(&f.name);
                out.extend("() { [native code] }".encode_utf16());
                out.into()
            }
            Js::Promise(_) => utf16("[object Promise]"),
            Js::Map(_) => utf16("[object Map]"),
            Js::Set(_) => utf16("[object Set]"),
        })
    }
    pub fn to_display(&self) -> String {
        self.to_js_string()
            .map(|s| String::from_utf16_lossy(&s))
            .unwrap_or_else(|_| "[unprintable]".into())
    }
    /// ToNumber (unary +).
    pub fn to_number(&self) -> Result<f64, Thrown> {
        Ok(match self {
            Js::Undefined => f64::NAN,
            Js::Null => 0.0,
            Js::Bool(b) => f64::from(u8::from(*b)),
            Js::Number(n) => *n,
            Js::String(s) => string_to_number(&String::from_utf16_lossy(s)),
            Js::Array(_) | Js::Object(_) => {
                string_to_number(&String::from_utf16_lossy(&self.to_js_string()?))
            }
            _ => f64::NAN,
        })
    }
    pub fn parse_float(&self) -> Result<f64, Thrown> {
        Ok(parse_float(&String::from_utf16_lossy(&self.to_js_string()?)))
    }

    fn own_props(&self) -> Option<&RefCell<IndexMap<Str, Prop>>> {
        match self {
            Js::Object(o) => Some(&o.props),
            Js::Function(f) => Some(&f.props),
            _ => None,
        }
    }

    /// [[Get]] including array/string length and index, accessors and prototype.
    pub fn get(&self, key: &str) -> Result<Js, Thrown> {
        self.get_key(&utf16(key))
    }
    pub fn get_key(&self, key: &[u16]) -> Result<Js, Thrown> {
        match self {
            Js::Undefined | Js::Null => {
                return Err(Thrown::type_error(&format!(
                    "Cannot read properties of {} (reading '{}')",
                    self.to_display(),
                    String::from_utf16_lossy(key)
                )));
            }
            Js::String(s) => {
                if key == utf16("length").as_ref() {
                    return Ok(Js::Number(s.len() as f64));
                }
                if let Some(i) = array_index(key) {
                    return Ok(s.get(i).map(|c| Js::utf16(&[*c])).unwrap_or(Js::Undefined));
                }
                return Ok(super::builtins::member(self, key).unwrap_or(Js::Undefined));
            }
            Js::Array(items) => {
                if key == utf16("length").as_ref() {
                    return Ok(Js::Number(items.borrow().len() as f64));
                }
                if let Some(i) = array_index(key) {
                    return Ok(items.borrow().get(i).cloned().flatten().unwrap_or(Js::Undefined));
                }
                return Ok(super::builtins::member(self, key).unwrap_or(Js::Undefined));
            }
            Js::Map(m) if key == utf16("size").as_ref() => {
                return Ok(Js::Number(m.borrow().len() as f64));
            }
            Js::Set(s) if key == utf16("size").as_ref() => {
                return Ok(Js::Number(s.borrow().len() as f64));
            }
            Js::Map(_) | Js::Set(_) | Js::Promise(_) | Js::Number(_) | Js::Bool(_) => {
                return Ok(super::builtins::member(self, key).unwrap_or(Js::Undefined));
            }
            _ => {}
        }
        let Some(props) = self.own_props() else {
            return Ok(Js::Undefined);
        };
        let prop = props.borrow().get(key).cloned();
        match prop {
            Some(Prop::Data(v)) => Ok(v),
            Some(Prop::Accessor(Some(getter), _)) => getter(self),
            Some(Prop::Accessor(None, _)) => Ok(Js::Undefined),
            None => {
                if let Js::Object(o) = self
                    && let Some(proto) = o.proto.borrow().clone()
                {
                    return proto.get_key(key);
                }
                Ok(Js::Undefined)
            }
        }
    }
    pub fn has_own(&self, key: &str) -> bool {
        match self {
            Js::Array(items) => {
                array_index(&utf16(key)).is_some_and(|i| matches!(items.borrow().get(i), Some(Some(_))))
            }
            _ => self
                .own_props()
                .is_some_and(|p| p.borrow().contains_key(utf16(key).as_ref())),
        }
    }
    /// Ordinary [[Set]]: runs setters, otherwise writes or appends an own data property.
    pub fn set(&self, key: &str, value: Js) -> Result<(), Thrown> {
        self.set_key(&utf16(key), value)
    }
    pub fn set_key(&self, key: &[u16], value: Js) -> Result<(), Thrown> {
        if let Js::Array(items) = self {
            if let Some(i) = array_index(key) {
                let mut items = items.borrow_mut();
                if i >= items.len() {
                    items.resize(i + 1, None);
                }
                items[i] = Some(value);
                return Ok(());
            }
            if key == utf16("length").as_ref() {
                let len = value.to_number()? as usize;
                items.borrow_mut().resize(len, None);
                return Ok(());
            }
            return Ok(());
        }
        let Some(props) = self.own_props() else {
            if self.is_nullish() {
                return Err(Thrown::type_error(&format!(
                    "Cannot set properties of {} (setting '{}')",
                    self.to_display(),
                    String::from_utf16_lossy(key)
                )));
            }
            return Ok(());
        };
        let existing = props.borrow().get(key).cloned();
        match existing {
            Some(Prop::Accessor(_, Some(setter))) => setter(self, value),
            Some(Prop::Accessor(_, None)) => Ok(()),
            _ => {
                props.borrow_mut().insert(key.into(), Prop::Data(value));
                Ok(())
            }
        }
    }
    /// CreateDataProperty: replaces accessors, keeps an existing key's slot.
    pub fn define(&self, key: &str, value: Js) -> Result<(), Thrown> {
        self.define_key(&utf16(key), value)
    }
    pub fn define_key(&self, key: &[u16], value: Js) -> Result<(), Thrown> {
        if let Js::Array(_) = self {
            return self.set_key(key, value);
        }
        if let Some(props) = self.own_props() {
            props.borrow_mut().insert(key.into(), Prop::Data(value));
        }
        Ok(())
    }
    pub fn define_accessor(&self, key: &str, getter: Option<Getter>, setter: Option<Setter>) {
        if let Some(props) = self.own_props() {
            props
                .borrow_mut()
                .insert(utf16(key), Prop::Accessor(getter, setter));
        }
    }
    pub fn delete(&self, key: &[u16]) {
        match self {
            Js::Array(items) => {
                if let Some(i) = array_index(key)
                    && let Some(slot) = items.borrow_mut().get_mut(i)
                {
                    *slot = None;
                }
            }
            _ => {
                if let Some(props) = self.own_props() {
                    props.borrow_mut().shift_remove(key);
                }
            }
        }
    }
    /// Enumerable own string keys in OrdinaryOwnPropertyKeys order.
    pub fn own_keys(&self) -> Vec<Str> {
        match self {
            Js::Array(items) => items
                .borrow()
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_some())
                .map(|(i, _)| utf16(&i.to_string()))
                .collect(),
            Js::String(s) => (0..s.len()).map(|i| utf16(&i.to_string())).collect(),
            _ => {
                let Some(props) = self.own_props() else {
                    return vec![];
                };
                let keys: Vec<Str> = props.borrow().keys().cloned().collect();
                let mut indexed: Vec<(usize, Str)> = keys
                    .iter()
                    .filter_map(|k| array_index(k).map(|i| (i, k.clone())))
                    .collect();
                indexed.sort_by_key(|(i, _)| *i);
                let mut out: Vec<Str> = indexed.into_iter().map(|(_, k)| k).collect();
                out.extend(keys.into_iter().filter(|k| array_index(k).is_none()));
                out
            }
        }
    }
    /// Object spread / Object.assign source read: own enumerable values in key order.
    pub fn own_entries(&self) -> Result<Vec<(Str, Js)>, Thrown> {
        self.own_keys()
            .into_iter()
            .map(|k| {
                let v = self.get_key(&k)?;
                Ok((k, v))
            })
            .collect()
    }
    /// `{ ...value }`: nullish spreads nothing, strings spread indexed characters.
    pub fn spread_object(&self) -> Result<Js, Thrown> {
        let out = Js::object(vec![]);
        if !self.is_nullish() {
            for (k, v) in self.own_entries()? {
                out.define_key(&k, v)?;
            }
        }
        Ok(out)
    }
    pub fn array_items(&self) -> Option<Vec<Option<Js>>> {
        match self {
            Js::Array(items) => Some(items.borrow().clone()),
            _ => None,
        }
    }
    pub fn array_values(&self) -> Option<Vec<Js>> {
        self.array_items()
            .map(|items| items.into_iter().map(|v| v.unwrap_or(Js::Undefined)).collect())
    }
    pub fn push(&self, value: Js) -> Result<(), Thrown> {
        match self {
            Js::Array(items) => {
                items.borrow_mut().push(Some(value));
                Ok(())
            }
            _ => Err(Thrown::type_error("push is not a function")),
        }
    }
    /// Invoke as a method with `this`; non-callables throw the V8 TypeError.
    pub fn call(&self, this: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        match self {
            Js::Function(f) => (f.call)(this, args),
            other => {
                if std::env::var("BITGET_DEBUG_CALLS").is_ok() {
                    eprintln!("NONCALL {:?}\n{}", other, std::backtrace::Backtrace::force_capture());
                }
                Err(Thrown::type_error(&format!("{} is not a function", other.to_display())))
            }
        }
    }
    pub fn call_method(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let method = self.get(name)?;
        if !method.is_callable() {
            return Err(Thrown::type_error(&format!(
                "{}.{name} is not a function",
                self.type_of()
            )));
        }
        method.call(self.clone(), args)
    }
    pub fn as_str(&self) -> Option<&Str> {
        match self {
            Js::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Js::Number(n) => Some(*n),
            _ => None,
        }
    }
}

pub fn array_index(key: &[u16]) -> Option<usize> {
    if key.is_empty() || key.len() > 10 || (key.len() > 1 && key[0] == b'0' as u16) {
        return None;
    }
    let mut out: usize = 0;
    for &c in key {
        if !(b'0' as u16..=b'9' as u16).contains(&c) {
            return None;
        }
        out = out.checked_mul(10)?.checked_add(usize::from(c - b'0' as u16))?;
    }
    (out < u32::MAX as usize).then_some(out)
}

/// StringToNumber per ECMAScript (whitespace trim, hex/octal/binary, Infinity).
pub fn string_to_number(text: &str) -> f64 {
    let t = safe_cex_num::js_trim(text);
    if t.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = t.strip_prefix(prefix) {
            return u128::from_str_radix(digits, radix)
                .map(|v| v as f64)
                .unwrap_or(f64::NAN);
        }
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let valid = t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
    if !valid || t.contains("inf") {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// serde_json::Value -> fresh native value (objects/arrays are new allocations).
pub fn from_json(value: &serde_json::Value) -> Js {
    match value {
        serde_json::Value::Null => Js::Null,
        serde_json::Value::Bool(b) => Js::Bool(*b),
        serde_json::Value::Number(n) => Js::Number(
            safe_cex_num::numeric_ingress::js_value_as_f64(value).unwrap_or_else(|| n.as_f64().unwrap_or(f64::NAN)),
        ),
        serde_json::Value::String(s) => Js::str(s),
        serde_json::Value::Array(items) => Js::array(items.iter().map(from_json).collect()),
        serde_json::Value::Object(fields) => {
            let out = Js::object(vec![]);
            for (k, v) in fields {
                let _ = out.define(k, from_json(v));
            }
            out
        }
    }
}

/// JSON projection used at typed boundaries. Lone surrogates are refused.
pub fn to_json(value: &Js) -> Result<serde_json::Value, Thrown> {
    Ok(match value {
        Js::Undefined | Js::Null => serde_json::Value::Null,
        Js::Bool(b) => serde_json::Value::Bool(*b),
        Js::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Js::String(s) => serde_json::Value::String(
            String::from_utf16(s).map_err(|_| Thrown::type_error("lone UTF-16 surrogate at a typed boundary"))?,
        ),
        Js::Array(items) => serde_json::Value::Array(
            items
                .borrow()
                .iter()
                .map(|v| v.as_ref().map(to_json).transpose().map(|v| v.unwrap_or(serde_json::Value::Null)))
                .collect::<Result<_, _>>()?,
        ),
        Js::Object(_) => {
            let mut out = serde_json::Map::new();
            for (k, v) in value.own_entries()? {
                if matches!(v, Js::Undefined | Js::Function(_)) {
                    continue;
                }
                out.insert(String::from_utf16_lossy(&k), to_json(&v)?);
            }
            serde_json::Value::Object(out)
        }
        Js::Function(_) | Js::Promise(_) => serde_json::Value::Null,
        Js::Map(_) | Js::Set(_) => serde_json::Value::Object(serde_json::Map::new()),
    })
}

/// Typed boundary helpers: serde types cross through the JSON projection.
pub fn to_typed<T: serde::de::DeserializeOwned>(value: &Js) -> Result<T, Thrown> {
    serde_json::from_value(to_json(value)?).map_err(|e| Thrown::type_error(&e.to_string()))
}
pub fn from_typed<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, Thrown> {
    Ok(from_json(
        &serde_json::to_value(value).map_err(|e| Thrown::type_error(&e.to_string()))?,
    ))
}
