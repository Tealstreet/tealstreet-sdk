//! Native private-event data: f64, typed enums and shallow/reference identity.
//! JSON data is decoded once. Own undefined and stock inherited table members
//! remain distinct; executable getters/custom callables/iterators are not data.
use crate::request_weight::{Failure, type_error};
use safe_cex_num::{ProtoMember, TableHit, js_number_string, js_table_lookup, parse_float};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_types::{
    MarginType, OrderRejectReason, OrderSide, OrderStatus, OrderStopTriggerType, OrderType,
    PositionMode, PositionSide,
};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrivateEnum {
    Status(OrderStatus),
    Type(OrderType),
    Side(OrderSide),
    Trigger(OrderStopTriggerType),
    Reject(OrderRejectReason),
    PositionSide(PositionSide),
    PositionMode(PositionMode),
    Margin(MarginType),
    Income(&'static safe_cex_types::IncomeCategory),
    Execution(&'static safe_cex_types::ExecutionPositionSide),
}
// All variants contain closed categorical enums; equality is reflexive,
// including canonical history enums whose shared types expose only PartialEq.
impl Eq for PrivateEnum {}
impl PrivateEnum {
    pub fn wire(self) -> Value {
        match self {
            Self::Status(v) => serde_json::to_value(v),
            Self::Type(v) => serde_json::to_value(v),
            Self::Side(v) => serde_json::to_value(v),
            Self::Trigger(v) => serde_json::to_value(v),
            Self::Reject(v) => serde_json::to_value(v),
            Self::PositionSide(v) => serde_json::to_value(v),
            Self::PositionMode(v) => serde_json::to_value(v),
            Self::Margin(v) => serde_json::to_value(v),
            Self::Income(v) => serde_json::to_value(v),
            Self::Execution(v) => serde_json::to_value(v),
        }
        .expect("native enum string")
    }
}
pub type EventArray = Rc<RefCell<Vec<EventValue>>>;
pub type EventObject = Rc<RefCell<EventRecord>>;
#[derive(Clone, Debug)]
pub enum EventValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(EventArray),
    Object(EventObject),
    Enum(PrivateEnum),
    Inherited(ProtoMember),
    Builtin(safe_cex_num::BuiltinReference),
}
#[derive(Clone, Debug)]
pub struct EventRecord {
    pub fields: JsObjectMap<EventValue>,
    pub prototype: Option<EventValue>,
    pub realm: Rc<safe_cex_num::PrototypeRealm>,
}
impl Default for EventRecord {
    fn default() -> Self {
        Self {
            fields: Default::default(),
            prototype: None,
            realm: safe_cex_num::PrototypeRealm::current(),
        }
    }
}
impl EventValue {
    pub fn from_prototype(value: safe_cex_num::PrototypeValue) -> Self {
        use safe_cex_num::PrototypeValue as P;
        match value {
            P::Undefined => Self::Undefined,
            P::Null => Self::Null,
            P::String(v) => Self::String(v),
            P::Number(v) => Self::Number(v),
            P::Boolean(v) => Self::Bool(v),
            P::Reference(v) => Self::Builtin(v),
        }
    }
    pub fn from_json(v: Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::Bool(v),
            Value::Number(v) => Self::Number(v.as_f64().expect("JS number")),
            Value::String(v) => Self::String(v),
            Value::Array(v) => Self::Array(Rc::new(RefCell::new(
                v.into_iter().map(Self::from_json).collect(),
            ))),
            Value::Object(v) => Self::object(
                v.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }
    pub fn object(fields: JsObjectMap<Self>) -> Self {
        Self::object_in_realm(safe_cex_num::PrototypeRealm::current(), fields)
    }
    pub fn object_in_realm(
        realm: Rc<safe_cex_num::PrototypeRealm>,
        fields: JsObjectMap<Self>,
    ) -> Self {
        Self::Object(Rc::new(RefCell::new(EventRecord {
            fields,
            prototype: None,
            realm,
        })))
    }
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(v) => *v,
            Self::Number(v) => *v != 0.0 && !v.is_nan(),
            Self::String(v) => !v.is_empty(),
            _ => true,
        }
    }
    pub fn nullish(&self) -> bool {
        matches!(self, Self::Undefined | Self::Null)
    }
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => Rc::ptr_eq(a, b),
            (Self::Object(a), Self::Object(b)) => Rc::ptr_eq(a, b),
            (Self::Enum(a), Self::Enum(b)) => a == b,
            (Self::Enum(a), Self::String(b)) | (Self::String(b), Self::Enum(a)) => {
                a.wire().as_str() == Some(b)
            }
            (Self::Inherited(a), Self::Inherited(b)) => a == b,
            (Self::Builtin(a), Self::Builtin(b)) => a == b,
            _ => false,
        }
    }
    pub fn get(&self, key: &str) -> Result<Self, Failure> {
        match self {
            Self::Undefined | Self::Null => Err(type_error(format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(self, Self::Undefined) {
                    "undefined"
                } else {
                    "null"
                }
            ))),
            Self::Object(o) => {
                let o = o.borrow();
                if let Some(v) = o.fields.get(key) {
                    return Ok(v.clone());
                }
                if key == "__proto__" {
                    return Ok(match &o.prototype {
                        Some(Self::Null) => Self::Undefined,
                        Some(v) => v.clone(),
                        None => Self::Builtin(o.realm.reference(ProtoMember::Prototype)),
                    });
                }
                if let Some(proto) = &o.prototype {
                    if matches!(proto, Self::Null) {
                        return Ok(Self::Undefined);
                    }
                    return proto.get(key);
                }
                Ok(Self::from_prototype(o.realm.inherited(key)))
            }
            Self::Array(a) => {
                if key == "length" {
                    return Ok(Self::Number(a.borrow().len() as f64));
                }
                if let Ok(i) = key.parse::<usize>() {
                    if i.to_string() == key {
                        return Ok(a.borrow().get(i).cloned().unwrap_or(Self::Undefined));
                    }
                }
                Ok(stock(key))
            }
            Self::Builtin(reference) => Ok(Self::from_prototype(reference.get(key))),
            Self::String(s) => {
                if key == "length" {
                    return Ok(Self::Number(s.encode_utf16().count() as f64));
                }
                Ok(stock(key))
            }
            Self::Enum(value) if key == "length" => Ok(Self::Number(
                value
                    .wire()
                    .as_str()
                    .expect("enum string")
                    .encode_utf16()
                    .count() as f64,
            )),
            Self::Number(_) | Self::Bool(_) | Self::Enum(_) => Ok(stock(key)),
            Self::Inherited(member) => Ok(Self::from_prototype(
                safe_cex_num::PrototypeRealm::current()
                    .reference(*member)
                    .get(key),
            )),
        }
    }
    /// Data property definition (object spread/object literal), without invoking setters.
    pub fn define(&self, key: &str, v: Self) {
        let Self::Object(o) = self else {
            panic!("native object definition")
        };
        o.borrow_mut().fields.insert(key.into(), v);
    }
    /// Source assignment into the fresh Spot currency/coin dictionaries. Preserve
    /// the stock __proto__ setter unless an own data property shadows it.
    pub fn assign(&self, key: &str, v: Self) {
        let Self::Object(o) = self else {
            panic!("native dictionary")
        };
        let mut o = o.borrow_mut();
        if key == "__proto__" && !o.fields.contains_key(key) {
            if matches!(
                v,
                Self::Null
                    | Self::Object(_)
                    | Self::Array(_)
                    | Self::Builtin(_)
                    | Self::Inherited(_)
            ) {
                o.prototype = Some(v)
            }
        } else {
            o.fields.insert(key.into(), v);
        }
    }
    pub fn spread(&self) -> JsObjectMap<Self> {
        match self {
            Self::Object(o) => o.borrow().fields.clone(),
            Self::Builtin(reference) => reference
                .own_fields()
                .into_iter()
                .map(|(key, value)| (key, Self::from_prototype(value.into())))
                .collect(),
            Self::Array(a) => a
                .borrow()
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
            Self::String(s) => s
                .chars()
                .enumerate()
                .map(|(i, c)| (i.to_string(), Self::String(c.to_string())))
                .collect(),
            _ => JsObjectMap::new(),
        }
    }
    pub fn text(&self) -> Result<String, Failure> {
        self.text_in(&mut Vec::new())
    }
    fn text_in(&self, active: &mut Vec<usize>) -> Result<String, Failure> {
        Ok(match self {
            Self::Undefined => "undefined".into(),
            Self::Null => "null".into(),
            Self::Bool(v) => v.to_string(),
            Self::Number(v) => js_number_string(*v),
            Self::String(v) => v.clone(),
            Self::Enum(v) => v.wire().as_str().expect("enum string").into(),
            Self::Inherited(v) => v.js_string().into(),
            Self::Builtin(v) => v.js_string(),
            Self::Array(a) => {
                let identity = Rc::as_ptr(a) as usize;
                if active.contains(&identity) {
                    return Ok(String::new());
                }
                active.push(identity);
                let values = a
                    .borrow()
                    .iter()
                    .map(|v| {
                        if v.nullish() {
                            Ok(String::new())
                        } else {
                            v.text_in(active)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>();
                active.pop();
                values?.join(",")
            }
            Self::Object(o) => {
                if o.borrow().fields.contains_key("toString") {
                    return Err(type_error("Cannot convert object to primitive value"));
                }
                "[object Object]".into()
            }
        })
    }
    pub fn number(&self) -> Result<f64, Failure> {
        match self {
            Self::Undefined => Ok(f64::NAN),
            Self::Null => Ok(0.0),
            Self::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
            Self::Number(v) => Ok(*v),
            _ => safe_cex_num::js_number(Some(&Value::String(self.text()?)))
                .map_err(|e| type_error(e.message)),
        }
    }
    pub fn float(&self) -> Result<f64, Failure> {
        Ok(parse_float(&self.text()?))
    }
    /// Optional explicit .toString() call, not String(value) coercion. Data-property
    /// overrides on decoded JSON objects are non-callable and throw at this site.
    pub fn optional_id(&self, expression: &str) -> Result<Self, Failure> {
        if self.nullish() {
            return Ok(Self::Undefined);
        }
        if let Self::Object(o) = self {
            if o.borrow().fields.contains_key("toString") {
                return Err(type_error(format!(
                    "{expression}?.toString is not a function"
                )));
            }
            return Ok(Self::String("[object Object]".into()));
        }
        Ok(Self::String(self.text()?))
    }
}
fn stock(key: &str) -> EventValue {
    EventValue::from_prototype(safe_cex_num::PrototypeRealm::current().inherited(key))
}

pub fn record(fields: impl IntoIterator<Item = (&'static str, EventValue)>) -> EventValue {
    EventValue::object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn number(v: f64) -> EventValue {
    EventValue::Number(v)
}
pub fn string(v: impl Into<String>) -> EventValue {
    EventValue::String(v.into())
}
pub fn fallback(v: EventValue, default: EventValue) -> EventValue {
    if v.truthy() { v } else { default }
}
pub fn nullish(v: EventValue, default: EventValue) -> EventValue {
    if v.nullish() { default } else { v }
}
pub fn table<T: Copy>(
    entries: &[(&str, T)],
    key: EventValue,
    wrap: impl FnOnce(T) -> PrivateEnum,
) -> Result<EventValue, Failure> {
    let key = key.text()?;
    Ok(match js_table_lookup(entries, &key) {
        TableHit::Mapped(v) => EventValue::Enum(wrap(*v)),
        TableHit::Inherited(_) | TableHit::Missing => {
            EventValue::from_prototype(safe_cex_num::PrototypeRealm::current().inherited(&key))
        }
    })
}
/// Array.forEach snapshots length, rereads present entries, and ignores appends.
pub fn for_each(
    array: &EventArray,
    mut visit: impl FnMut(EventValue) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let len = array.borrow().len();
    for i in 0..len {
        let value = array.borrow().get(i).cloned();
        if let Some(value) = value {
            visit(value)?
        }
    }
    Ok(())
}
pub fn array_for_each(
    value: EventValue,
    expression: &str,
    visit: impl FnMut(EventValue) -> Result<(), Failure>,
) -> Result<(), Failure> {
    if value.nullish() {
        return Ok(());
    }
    let EventValue::Array(a) = value else {
        return Err(type_error(format!("{expression} is not a function")));
    };
    for_each(&a, visit)
}
/// Array.find visits each initial slot, including absent entries after removals.
pub fn find(
    array: &EventArray,
    mut predicate: impl FnMut(&EventValue) -> Result<bool, Failure>,
) -> Result<EventValue, Failure> {
    let len = array.borrow().len();
    for i in 0..len {
        let value = array
            .borrow()
            .get(i)
            .cloned()
            .unwrap_or(EventValue::Undefined);
        if predicate(&value)? {
            return Ok(value);
        }
    }
    Ok(EventValue::Undefined)
}

/// Source JSON.stringify input over the native value graph.
pub fn native_json(v: &EventValue) -> safe_cex_num::JsJsonValue {
    use safe_cex_num::JsJsonValue as J;
    match v {
        EventValue::Undefined => J::Undefined,
        EventValue::Null => J::Null,
        EventValue::Bool(v) => J::Bool(*v),
        EventValue::String(v) => J::String(v.clone()),
        EventValue::Number(v) => J::Number(*v),
        EventValue::Enum(v) => J::String(v.wire().as_str().expect("categorical enum").into()),
        EventValue::Array(v) => J::Array(v.borrow().iter().map(native_json).collect()),
        EventValue::Object(v) => J::Object(
            v.borrow()
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), native_json(v)))
                .collect(),
        ),
        EventValue::Inherited(safe_cex_num::ProtoMember::Prototype) => J::Object(vec![]),
        EventValue::Inherited(_) => J::Undefined,
        EventValue::Builtin(reference) => {
            if reference.is_callable() {
                J::Undefined
            } else {
                J::Object(
                    reference
                        .own_fields()
                        .into_iter()
                        .map(|(key, value)| {
                            (key, native_json(&EventValue::from_prototype(value.into())))
                        })
                        .collect(),
                )
            }
        }
    }
}
