//! Decoded private-event operations over the canonical shared runtime graph.
//! JSON data is decoded once. Own undefined and stock inherited table members
//! remain distinct; executable getters/custom callables/iterators are not data.
use crate::request_weight::{Failure, type_error};
pub use safe_cex_exchange::js_runtime::value::Js as EventValue;
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
pub type EventArray = Rc<RefCell<Vec<Option<EventValue>>>>;

/// Legacy decoded-data profile over the canonical runtime graph. No second heap.
pub trait EventOps: Sized {
    fn from_json(value: Value) -> Self;
    fn record_object(fields: JsObjectMap<Self>) -> Self;
    fn object_in_realm(realm: Rc<safe_cex_num::PrototypeRealm>, fields: JsObjectMap<Self>) -> Self;
    fn text_value(value: String) -> Self;
    fn enum_value(value: PrivateEnum) -> Self;
    fn inherited(member: ProtoMember) -> Self;
    fn nullish(&self) -> bool;
    fn same(&self, other: &Self) -> bool;
    fn read(&self, key: &str) -> Result<Self, Failure>;
    fn write(&self, key: &str, value: Self);
    fn assign(&self, key: &str, value: Self);
    fn spread(&self) -> JsObjectMap<Self>;
    fn text(&self) -> Result<String, Failure>;
    fn number(&self) -> Result<f64, Failure>;
    fn float(&self) -> Result<f64, Failure>;
    fn optional_id(&self, expression: &str) -> Result<Self, Failure>;
}
impl EventOps for EventValue {
    fn from_json(value: Value) -> Self {
        in_current_realm(safe_cex_exchange::js_runtime::value::from_json(&value))
    }
    fn record_object(fields: JsObjectMap<Self>) -> Self {
        Self::object_in_realm(safe_cex_num::PrototypeRealm::current(), fields)
    }
    fn object_in_realm(realm: Rc<safe_cex_num::PrototypeRealm>, fields: JsObjectMap<Self>) -> Self {
        let value = Self::object(vec![]);
        let Self::Object(object) = &value else {
            unreachable!()
        };
        *object.proto.borrow_mut() = Some(Self::Builtin(realm.reference(ProtoMember::Prototype)));
        for (key, item) in fields {
            value.define(&key, item).expect("fresh object");
        }
        value
    }
    fn text_value(value: String) -> Self {
        Self::str(&value)
    }
    fn enum_value(value: PrivateEnum) -> Self {
        Self::str(value.wire().as_str().expect("categorical enum"))
    }
    fn inherited(member: ProtoMember) -> Self {
        Self::Builtin(safe_cex_num::PrototypeRealm::current().reference(member))
    }
    fn nullish(&self) -> bool {
        self.is_nullish()
    }
    fn same(&self, other: &Self) -> bool {
        self.strict_equals(other)
    }
    fn read(&self, key: &str) -> Result<Self, Failure> {
        // Dense JSON-era primitives observe the existing current native realm.
        // Objects retain their captured realm through their canonical prototype.
        match self {
            Self::Array(_)
                if key != "length"
                    && key
                        .parse::<usize>()
                        .ok()
                        .is_none_or(|i| i.to_string() != key) =>
            {
                Ok(stock(key))
            }
            Self::String(_) if key != "length" => Ok(stock(key)),
            Self::Number(_) | Self::Bool(_) => Ok(stock(key)),
            _ => self.get(key).map_err(Into::into),
        }
    }
    fn write(&self, key: &str, value: Self) {
        self.define(key, value).expect("native data property");
    }
    fn assign(&self, key: &str, value: Self) {
        self.set(key, value).expect("native dictionary assignment");
    }
    fn spread(&self) -> JsObjectMap<Self> {
        self.own_entries()
            .expect("decoded own data entries")
            .into_iter()
            .map(|(key, value)| {
                (
                    String::from_utf16(&key).expect("native UTF-8 object key boundary"),
                    value,
                )
            })
            .collect()
    }
    fn text(&self) -> Result<String, Failure> {
        fn text_in(value: &EventValue, active: &mut Vec<usize>) -> Result<String, Failure> {
            Ok(match value {
                EventValue::Array(items) => {
                    let identity = Rc::as_ptr(items) as usize;
                    if active.contains(&identity) {
                        return Ok(String::new());
                    }
                    active.push(identity);
                    let values = items.borrow().clone();
                    let text = values
                        .iter()
                        .map(|value| match value {
                            None | Some(EventValue::Undefined | EventValue::Null) => {
                                Ok(String::new())
                            }
                            Some(value) => text_in(value, active),
                        })
                        .collect::<Result<Vec<_>, _>>();
                    active.pop();
                    text?.join(",")
                }
                EventValue::Object(_) => {
                    if value.has_own("toString") {
                        return Err(type_error("Cannot convert object to primitive value"));
                    }
                    "[object Object]".into()
                }
                EventValue::Undefined => "undefined".into(),
                EventValue::Null => "null".into(),
                EventValue::Bool(value) => value.to_string(),
                EventValue::Number(value) => js_number_string(*value),
                EventValue::String(value) => String::from_utf16(value)
                    .map_err(|error| type_error(format!("Native UTF-8 boundary: {error}")))?,
                EventValue::Builtin(value) => value.js_string(),
                _ => return Err(type_error("Value is outside the decoded-data boundary")),
            })
        }
        text_in(self, &mut vec![])
    }
    fn number(&self) -> Result<f64, Failure> {
        match self {
            Self::Undefined => Ok(f64::NAN),
            Self::Null => Ok(0.0),
            Self::Bool(value) => Ok(f64::from(u8::from(*value))),
            Self::Number(value) => Ok(*value),
            _ => safe_cex_num::js_number(Some(&Value::String(self.text()?)))
                .map_err(|error| type_error(error.message)),
        }
    }
    fn float(&self) -> Result<f64, Failure> {
        Ok(parse_float(&self.text()?))
    }
    fn optional_id(&self, expression: &str) -> Result<Self, Failure> {
        if self.nullish() {
            return Ok(Self::Undefined);
        }
        if matches!(self, Self::Object(_)) && self.has_own("toString") {
            return Err(type_error(format!(
                "{expression}?.toString is not a function"
            )));
        }
        Ok(Self::str(&self.text()?))
    }
}

fn stock(key: &str) -> EventValue {
    EventValue::from_prototype(safe_cex_num::PrototypeRealm::current().inherited(key))
}

pub fn record(fields: impl IntoIterator<Item = (&'static str, EventValue)>) -> EventValue {
    EventValue::record_object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
pub fn number(v: f64) -> EventValue {
    EventValue::Number(v)
}
pub fn string(v: impl Into<String>) -> EventValue {
    EventValue::str(&v.into())
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
        TableHit::Mapped(v) => EventValue::enum_value(wrap(*v)),
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
        let value = array.borrow().get(i).cloned().flatten();
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
            .flatten()
            .unwrap_or(EventValue::Undefined);
        if predicate(&value)? {
            return Ok(value);
        }
    }
    Ok(EventValue::Undefined)
}

/// Source JSON.stringify input over the native value graph.
pub fn native_json(value: &EventValue) -> safe_cex_num::JsJsonValue {
    safe_cex_exchange::js_runtime::value::to_js_json(value).expect("decoded JSON graph")
}

/// Values yielded by a source array iterator; holes yield undefined.
pub fn array_values(array: &EventArray) -> Vec<EventValue> {
    EventValue::Array(array.clone())
        .array_values()
        .expect("native array")
}

/// Attach the existing captured native prototype realm to a freshly decoded graph.
pub(crate) fn in_current_realm(value: EventValue) -> EventValue {
    fn attach(value: &EventValue, realm: &Rc<safe_cex_num::PrototypeRealm>) {
        match value {
            EventValue::Object(object) => {
                *object.proto.borrow_mut() =
                    Some(EventValue::Builtin(realm.reference(ProtoMember::Prototype)));
                for (_, value) in value.own_entries().expect("decoded data properties") {
                    attach(&value, realm);
                }
            }
            EventValue::Array(values) => {
                for value in values.borrow().iter().flatten() {
                    attach(value, realm);
                }
            }
            _ => {}
        }
    }
    attach(&value, &safe_cex_num::PrototypeRealm::current());
    value
}
