//! The production realm's JavaScript value model. Object handles are shared
//! `Rc` cells, so cloning a `Value` keeps its native identity.
use super::Realm;
use crate::spot_json::JsString;
use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
    task::Waker,
};

pub type Obj = Rc<RefCell<ObjectData>>;
pub type Thrown = Value;
pub type NativeFn = Rc<dyn Fn(&mut Realm, &Value, &[Value]) -> Result<Value, Thrown>>;
/// `new_target` is the constructor `new` was applied to (a subclass for `super`).
pub type NativeConstruct = Rc<dyn Fn(&mut Realm, &[Value], &Value) -> Result<Value, Thrown>>;

#[derive(Clone)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<[u16]>),
    Symbol(Rc<Symbol>),
    Object(Obj),
}

#[derive(Debug)]
pub struct Symbol {
    pub description: &'static str,
}

#[derive(Clone)]
pub enum PropKey {
    String(Rc<[u16]>),
    Symbol(Rc<Symbol>),
}

#[derive(Clone)]
pub enum Slot {
    Data(Value),
    Accessor {
        get: Option<Value>,
        set: Option<Value>,
    },
}

#[derive(Clone)]
pub struct Prop {
    pub key: PropKey,
    pub slot: Slot,
    pub enumerable: bool,
    pub writable: bool,
    pub configurable: bool,
}

pub struct FunctionData {
    pub name: JsString,
    pub length: u8,
    pub call: Option<NativeFn>,
    pub construct: Option<NativeConstruct>,
}

#[derive(Default)]
pub enum PromiseState {
    #[default]
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Default)]
pub struct PromiseCell {
    pub state: PromiseState,
    pub wakers: Vec<Waker>,
    pub handled: bool,
}

pub enum IterSource {
    Array { target: Obj, index: usize },
    MapEntries { target: Obj, index: usize },
    MapKeys { target: Obj, index: usize },
    MapValues { target: Obj, index: usize },
    SetValues { target: Obj, index: usize },
    String { units: Rc<[u16]>, index: usize },
    Keys { keys: Vec<Value>, index: usize },
    /// A user iterator object with `next` (and optionally `return`).
    Protocol { iterator: Value, next: Value },
    Done,
}

pub enum Kind {
    Ordinary,
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Set(Vec<Value>),
    Function(Rc<FunctionData>),
    Promise(Rc<RefCell<PromiseCell>>),
    Iterator(IterSource),
    Error,
    Date(f64),
    RegExp(Rc<super::regex::Pattern>),
    /// An opaque native resource (socket, client) the facades downcast.
    Host(Rc<dyn std::any::Any>),
}

pub struct ObjectData {
    pub proto: Option<Obj>,
    pub props: Vec<Prop>,
    pub kind: Kind,
    pub extensible: bool,
}

impl Value {
    pub fn str(text: &str) -> Self {
        Value::String(text.encode_utf16().collect::<Vec<_>>().into())
    }
    pub fn units(units: &[u16]) -> Self {
        Value::String(units.into())
    }
    pub fn js(text: &JsString) -> Self {
        Value::String(text.0.as_slice().into())
    }
    pub fn object(&self) -> Option<&Obj> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
    pub fn as_units(&self) -> Option<&[u16]> {
        match self {
            Value::String(u) => Some(u),
            _ => None,
        }
    }
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(u) => !u.is_empty(),
            Value::Symbol(_) | Value::Object(_) => true,
        }
    }
    /// `===`, including NaN !== NaN and +0 === -0.
    pub fn strict_equal(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => Rc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    /// SameValueZero (Map/Set keys, Array.prototype.includes).
    pub fn same_value_zero(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) if a.is_nan() && b.is_nan() => true,
            _ => self.strict_equal(other),
        }
    }
    pub fn type_of(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Null => "object",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Symbol(_) => "symbol",
            Value::Object(o) => match &o.borrow().kind {
                Kind::Function(_) => "function",
                _ => "object",
            },
        }
    }
    pub fn is_callable(&self) -> bool {
        self.function().is_some_and(|f| f.call.is_some())
    }
    pub fn function(&self) -> Option<Rc<FunctionData>> {
        match &self.object()?.borrow().kind {
            Kind::Function(f) => Some(f.clone()),
            _ => None,
        }
    }
    pub fn promise(&self) -> Option<Rc<RefCell<PromiseCell>>> {
        match &self.object()?.borrow().kind {
            Kind::Promise(p) => Some(p.clone()),
            _ => None,
        }
    }
    pub fn host<T: 'static>(&self) -> Option<Rc<T>> {
        match &self.object()?.borrow().kind {
            Kind::Host(any) => any.clone().downcast::<T>().ok(),
            _ => None,
        }
    }
    pub fn is_array(&self) -> bool {
        self.object()
            .is_some_and(|o| matches!(o.borrow().kind, Kind::Array(_)))
    }
}

impl PropKey {
    pub fn str(text: &str) -> Self {
        PropKey::String(text.encode_utf16().collect::<Vec<_>>().into())
    }
    pub fn same(&self, other: &PropKey) -> bool {
        match (self, other) {
            (PropKey::String(a), PropKey::String(b)) => a == b,
            (PropKey::Symbol(a), PropKey::Symbol(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    pub fn units(&self) -> Option<&[u16]> {
        match self {
            PropKey::String(u) => Some(u),
            PropKey::Symbol(_) => None,
        }
    }
    /// The canonical array index this key names, if any.
    pub fn array_index(&self) -> Option<usize> {
        let units = self.units()?;
        if units.is_empty() || (units.len() > 1 && units[0] == b'0' as u16) {
            return None;
        }
        let mut value = 0usize;
        for &unit in units {
            if !(b'0' as u16..=b'9' as u16).contains(&unit) {
                return None;
            }
            value = value.checked_mul(10)?.checked_add((unit - b'0' as u16) as usize)?;
        }
        (value < u32::MAX as usize).then_some(value)
    }
    pub fn to_value(&self) -> Value {
        match self {
            PropKey::String(u) => Value::String(u.clone()),
            PropKey::Symbol(s) => Value::Symbol(s.clone()),
        }
    }
}

impl ObjectData {
    pub fn new(kind: Kind, proto: Option<Obj>) -> Self {
        Self {
            proto,
            props: vec![],
            kind,
            extensible: true,
        }
    }
    pub fn own(&self, key: &PropKey) -> Option<&Prop> {
        self.props.iter().find(|p| p.key.same(key))
    }
    pub fn own_mut(&mut self, key: &PropKey) -> Option<&mut Prop> {
        self.props.iter_mut().find(|p| p.key.same(key))
    }
    /// Define or replace a writable, enumerable, configurable data property.
    pub fn put(&mut self, key: PropKey, value: Value, enumerable: bool) {
        if let Some(p) = self.own_mut(&key) {
            p.slot = Slot::Data(value);
            return;
        }
        self.props.push(Prop {
            key,
            slot: Slot::Data(value),
            enumerable,
            writable: true,
            configurable: true,
        });
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{}", safe_cex_num::js_number_string(*n)),
            Value::String(u) => write!(f, "{:?}", String::from_utf16_lossy(u)),
            Value::Symbol(s) => write!(f, "Symbol({})", s.description),
            Value::Object(o) => match o.try_borrow() {
                Ok(o) => match &o.kind {
                    Kind::Array(v) => write!(f, "{v:?}"),
                    Kind::Function(func) => {
                        write!(f, "[Function {}]", String::from_utf16_lossy(&func.name.0))
                    }
                    Kind::Error => {
                        let message = o.own(&PropKey::str("message")).map(|p| match &p.slot {
                            Slot::Data(v) => format!("{v:?}"),
                            _ => "?".into(),
                        });
                        write!(f, "[Error {}]", message.unwrap_or_default())
                    }
                    _ => write!(f, "[object]"),
                },
                Err(_) => write!(f, "[object (borrowed)]"),
            },
        }
    }
}
