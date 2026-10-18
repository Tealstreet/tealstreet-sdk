//! Native JS value model handed to host-trait-generic ported venue bodies.
//! Strings are UTF-16, arrays keep holes, objects keep own-key order and accessors.
pub mod owned;
pub use owned::OwnedJsValue;

use crate::runtime::ExchangeError;
use indexmap::IndexMap;
use safe_cex_num::{JsCoercionError, js_number_string, parse_float};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};
mod typed;

pub type Str = Rc<[u16]>;
pub type NativeFn = Rc<dyn Fn(Js, Vec<Js>) -> Result<Js, Thrown>>;
pub type Getter = Rc<dyn Fn(&Js) -> Result<Js, Thrown>>;
pub type Setter = Rc<dyn Fn(&Js, Js) -> Result<(), Thrown>>;
pub type WriteObserver = Rc<dyn Fn(&Js, &[u16], &Js) -> Result<(), Thrown>>;

/// The `i`th call argument, `undefined` when absent.
pub fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}

pub fn utf16(text: &str) -> Str {
    text.encode_utf16().collect::<Vec<_>>().into()
}

#[derive(Clone)]
pub enum Prop {
    Data(Js),
    DataDescriptor {
        value: Js,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    },
    Accessor {
        get: Option<Js>,
        set: Option<Js>,
        enumerable: bool,
        configurable: bool,
    },
}

fn same_optional_value(left: &Option<Js>, right: &Option<Js>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => same_value(left, right),
        _ => false,
    }
}

pub struct ObjectData {
    write_observer: RefCell<Option<WriteObserver>>,
    resource: Option<Rc<dyn Any>>,
    extensible: Cell<bool>,
    pub class: &'static str,
    pub(crate) iterator_next: RefCell<Option<NativeFn>>,
    pub(crate) symbol_iterator: RefCell<Option<Prop>>,
    pub props: RefCell<IndexMap<Str, Prop>>,
    /// None is an explicit null prototype; ordinary allocations store their realm parent.
    pub proto: RefCell<Option<Js>>,
}

pub struct FunctionData {
    extensible: Cell<bool>,
    pub name: Str,
    pub call: NativeFn,
    pub props: RefCell<IndexMap<Str, Prop>>,
    pub proto: RefCell<Option<Js>>,
    construct: Option<Construct>,
    realm: Rc<safe_cex_num::PrototypeRealm>,
}

enum Construct {
    Native(NativeFn),
    Bound { target: Js, args: Vec<Js> },
}

#[derive(Clone)]
pub enum Js {
    Undefined,
    /// The well-known iterator symbol; distinct from every UTF-16 string key.
    SymbolIterator,
    Null,
    Bool(bool),
    Number(f64),
    BigInt(Rc<num_bigint::BigInt>),
    /// Retained native intrinsic referent shared with core socket statistics.
    /// Function call/bind/toString and the empty Function prototype execute here.
    Builtin(safe_cex_num::BuiltinReference),
    String(Str),
    Array(Rc<RefCell<Vec<Option<Js>>>>),
    Object(Rc<ObjectData>),
    Function(Rc<FunctionData>),
    Promise(super::promise::Promise),
    Map(Rc<RefCell<super::collections::Collection<(Js, Js)>>>),
    Set(Rc<RefCell<super::collections::Collection<Js>>>),
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
    fn error_fields(&self) -> ExchangeError {
        if let Some(native) = &self.native {
            return (**native).clone();
        }
        let message = match &self.value {
            Js::Object(_) | Js::Function(_) => self.value.get("message").ok(),
            other => Some(other.clone()),
        };
        let mut error =
            ExchangeError::new(message.as_ref().map(Js::to_display).unwrap_or_default());
        let name = self.value.get("name").ok();
        if let Some(Js::String(name)) = &name {
            error.name = String::from_utf16_lossy(name);
        }
        let fields: Vec<_> = [("name", name), ("message", message)]
            .into_iter()
            .filter_map(|(key, value)| match value {
                Some(Js::String(units)) => Some((key, OwnedJsValue::String(units.to_vec()))),
                _ => None,
            })
            .collect();
        if fields.iter().any(|(_, value)| {
            matches!(value, OwnedJsValue::String(units) if String::from_utf16(units).is_err())
        }) {
            error.origin = Some(crate::runtime::ExchangeErrorOrigin::new(
                owned::OwnedCallbackError::new(OwnedJsValue::object(fields)),
            ));
        }
        error
    }
    pub fn to_exchange_error(&self) -> ExchangeError {
        let mut original = self.clone();
        let mut error = self.error_fields();
        if self.value.is_object() {
            let code = self
                .value
                .get("code")
                .map(|code| (self.value.has_own("code") || !code.is_undefined()).then_some(code));
            match code {
                Ok(code) => error.code = code,
                Err(cause) => {
                    error = cause.error_fields();
                    original = cause;
                }
            }
        }
        if original.native.is_none() {
            // The existing native slot records the diagnostic projection before
            // adding its local origin. This is acyclic and needs no user reads.
            original.native = Some(Rc::new(error.clone()));
            error.origin = Some(crate::runtime::ExchangeErrorOrigin::new(original));
        }
        error
    }
}

impl fmt::Debug for Thrown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Thrown({})", self.message())
    }
}

impl fmt::Display for Thrown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            self.native
                .as_ref()
                .map_or("native JavaScript throw", |error| error.message.as_str()),
        )
    }
}

impl std::error::Error for Thrown {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.native
            .as_deref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

impl From<ExchangeError> for Thrown {
    fn from(error: ExchangeError) -> Self {
        // Store read failures carry arbitrary native throws below this layer.
        // Recover before creating diagnostics or reading any cause properties.
        if let Some(origin) = &error.origin {
            let read = origin
                .error()
                .downcast_ref::<safe_cex_types::StoreReadError>()
                .or_else(|| {
                    match origin
                        .error()
                        .downcast_ref::<safe_cex_store::StoreMutationError>()
                    {
                        Some(safe_cex_store::StoreMutationError::Access(error)) => Some(error),
                        _ => None,
                    }
                });
            if let Some(cause) = read.and_then(|read| read.native_cause().downcast_ref::<Thrown>())
            {
                return cause.clone();
            }
        }
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            if let Some(original) = cause.downcast_ref::<Thrown>()
                && let Some(snapshot) = &original.native
                && error.retained_response_input().is_none()
            {
                let mut projection = error.clone();
                projection.origin = snapshot.origin.clone();
                if projection == **snapshot {
                    return Self {
                        value: original.value.clone(),
                        native: Some(Rc::new(error)),
                    };
                }
            }
            source = cause.source();
        }
        let value = error_object(&error.name, &error.message);
        let mut source = std::error::Error::source(&error);
        while let Some(original) = source {
            if let Some(cause) = original.downcast_ref::<owned::OwnedCallbackError>() {
                for (field, projected) in [("name", &error.name), ("message", &error.message)] {
                    if let Some(OwnedJsValue::String(units)) = cause.value().get(field)
                        && String::from_utf16(units).is_err()
                        && String::from_utf16_lossy(units) == *projected
                    {
                        value
                            .set(field, Js::utf16(units))
                            .expect("fresh error metadata property");
                    }
                }
                for field in ["stack", "cause", "isAxiosError"] {
                    if let Some(cell) = cause.value().get(field) {
                        value
                            .define(field, cell.to_js())
                            .expect("fresh retained error property");
                    }
                }
                break;
            }
            source = original.source();
        }
        if let Some(response) = error.retained_response_input() {
            value
                .define("response", from_retained_input(response))
                .expect("fresh retained response property");
        }
        if let Some(code) = &error.code {
            value
                .define("code", code.clone())
                .expect("fresh error code property");
        }
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

impl From<Thrown> for Rc<ExchangeError> {
    fn from(thrown: Thrown) -> Self {
        thrown
            .native
            .clone()
            .unwrap_or_else(|| Rc::new(thrown.to_exchange_error()))
    }
}

impl From<Thrown> for ExchangeError {
    fn from(thrown: Thrown) -> Self {
        thrown.to_exchange_error()
    }
}

fn error_data_property(object: &Js, key: &str, value: Js, enumerable: bool) {
    let Js::Object(data) = object else {
        unreachable!("fresh error/prototype object")
    };
    data.props.borrow_mut().insert(
        utf16(key),
        Prop::DataDescriptor {
            value,
            writable: true,
            enumerable,
            configurable: true,
        },
    );
}

// Shared stock prototypes carry inherited names; no constructor/runtime captures
// or reference cycles are retained by this immutable thread-local lookup table.
fn error_prototype(name: &str) -> (Js, bool) {
    thread_local! {
        static ERROR_PROTOTYPES: IndexMap<&'static str, Js> = {
            let base = Js::object(vec![]);
            error_data_property(&base, "name", Js::str("Error"), false);
            error_data_property(&base, "message", Js::str(""), false);
            error_data_property(&base, "toString", Js::function("toString", |this, _| {
                error_string(&this).map(Js::String)
            }), false);
            let mut prototypes = IndexMap::new();
            prototypes.insert("Error", base.clone());
            for name in ["EvalError", "RangeError", "ReferenceError", "SyntaxError", "TypeError", "URIError", "AggregateError"] {
                let prototype = Js::object(vec![]);
                let Js::Object(data) = &prototype else { unreachable!() };
                *data.proto.borrow_mut() = Some(base.clone());
                error_data_property(&prototype, "name", Js::str(name), false);
                error_data_property(&prototype, "message", Js::str(""), false);
                prototypes.insert(name, prototype);
            }
            prototypes
        };
    }
    ERROR_PROTOTYPES.with(|prototypes| match prototypes.get(name) {
        Some(prototype) => (prototype.clone(), false),
        None => (prototypes["Error"].clone(), true),
    })
}

fn make_error_object(name: &str, message: Option<Str>) -> Js {
    let (prototype, custom_name) = error_prototype(name);
    let object = Js::object_with_class("Error", vec![]);
    let Js::Object(data) = &object else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(prototype);
    if let Some(message) = message {
        // Native Axios diagnostics assign their own name/message; standard
        // Error constructors define a non-enumerable own message instead.
        error_data_property(&object, "message", Js::String(message), custom_name);
    }
    if custom_name {
        error_data_property(&object, "name", Js::str(name), true);
    }
    object
}

pub fn error_object(name: &str, message: &str) -> Js {
    make_error_object(name, Some(utf16(message)))
}

pub(super) fn error_constructor(name: &'static str) -> Js {
    let body: NativeFn = Rc::new(move |_, args| {
        let message = arg(&args, 0);
        let message = if message.is_undefined() {
            None
        } else {
            Some(message.to_js_string()?)
        };
        Ok(make_error_object(name, message))
    });
    let call = body.clone();
    let constructor = Js::constructor(name, Some(call), move |target, args| {
        let prototype = target.get("prototype")?;
        let error = body(Js::Undefined, args)?;
        if prototype.is_object()
            && let Js::Object(data) = &error
        {
            *data.proto.borrow_mut() = Some(prototype);
        }
        Ok(error)
    });
    let Js::Function(function) = &constructor else {
        unreachable!()
    };
    function.props.borrow_mut().insert(
        utf16("prototype"),
        Prop::DataDescriptor {
            value: error_prototype(name).0,
            writable: false,
            enumerable: false,
            configurable: false,
        },
    );
    constructor
}

fn error_string(error: &Js) -> Result<Str, Thrown> {
    if !error.is_object() {
        return Err(Thrown::type_error(
            "Error.prototype.toString requires an object",
        ));
    }
    let name = error.get("name")?;
    let name = if name.is_undefined() {
        utf16("Error")
    } else {
        name.to_js_string()?
    };
    let message = error.get("message")?;
    let message = if message.is_undefined() {
        utf16("")
    } else {
        message.to_js_string()?
    };
    if name.is_empty() {
        return Ok(message);
    }
    if message.is_empty() {
        return Ok(name);
    }
    let mut out = name.to_vec();
    out.extend(": ".encode_utf16());
    out.extend_from_slice(&message);
    Ok(out.into())
}

impl fmt::Debug for Js {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Js::Undefined => write!(f, "undefined"),
            Js::SymbolIterator => write!(f, "Symbol(Symbol.iterator)"),
            Js::Null => write!(f, "null"),
            Js::Bool(b) => write!(f, "{b}"),
            Js::Number(n) => write!(f, "{}", js_number_string(*n)),
            Js::BigInt(n) => write!(f, "{n}n"),
            Js::Builtin(reference) => write!(f, "{}", reference.js_string()),
            Js::String(s) => write!(f, "{:?}", String::from_utf16_lossy(s)),
            Js::Array(a) => f.debug_list().entries(a.borrow().iter()).finish(),
            Js::Object(o) => {
                let props = o.props.borrow();
                let mut map = f.debug_map();
                for (k, v) in props.iter() {
                    match v {
                        Prop::Data(v) | Prop::DataDescriptor { value: v, .. } => {
                            map.entry(&String::from_utf16_lossy(k), v)
                        }
                        Prop::Accessor { .. } => {
                            map.entry(&String::from_utf16_lossy(k), &"<accessor>")
                        }
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
    /// Preserve a live intrinsic referent instead of copying its current fields.
    pub fn from_prototype(value: safe_cex_num::PrototypeValue) -> Js {
        use safe_cex_num::PrototypeValue as P;
        match value {
            P::Undefined => Js::Undefined,
            P::Null => Js::Null,
            P::Boolean(value) => Js::Bool(value),
            P::Number(value) => Js::Number(value),
            P::String(value) => Js::str(&value),
            P::Reference(reference) => Js::Builtin(reference),
        }
    }
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
        Self::object_with_resource(class, fields, None)
    }
    pub fn host_object(resource: Rc<dyn Any>) -> Js {
        Self::object_with_resource("Object", vec![], Some(resource))
    }
    pub fn resource<T: Any>(&self) -> Option<Rc<T>> {
        let Js::Object(object) = self else {
            return None;
        };
        object.resource.as_ref()?.clone().downcast::<T>().ok()
    }
    fn object_with_resource(
        class: &'static str,
        fields: Vec<(&str, Js)>,
        resource: Option<Rc<dyn Any>>,
    ) -> Js {
        Js::Object(Rc::new(ObjectData {
            write_observer: RefCell::new(None),
            resource,
            extensible: Cell::new(true),
            class,
            iterator_next: RefCell::new(None),
            symbol_iterator: RefCell::new(None),
            props: RefCell::new(
                fields
                    .into_iter()
                    .map(|(k, v)| (utf16(k), Prop::Data(v)))
                    .collect(),
            ),
            proto: RefCell::new(Some(Js::Builtin(
                safe_cex_num::PrototypeRealm::current()
                    .reference(safe_cex_num::ProtoMember::Prototype),
            ))),
        }))
    }
    pub fn function(name: &str, call: impl Fn(Js, Vec<Js>) -> Result<Js, Thrown> + 'static) -> Js {
        let realm = safe_cex_num::PrototypeRealm::current();
        Js::Function(Rc::new(FunctionData {
            extensible: Cell::new(true),
            name: utf16(name),
            call: Rc::new(call),
            props: RefCell::new(IndexMap::from([
                (
                    utf16("length"),
                    Prop::DataDescriptor {
                        value: Js::Number(0.),
                        writable: false,
                        enumerable: false,
                        configurable: true,
                    },
                ),
                (
                    utf16("name"),
                    Prop::DataDescriptor {
                        value: Js::str(name),
                        writable: false,
                        enumerable: false,
                        configurable: true,
                    },
                ),
            ])),
            proto: RefCell::new(
                realm
                    .reference(safe_cex_num::ProtoMember::Constructor)
                    .prototype()
                    .map(Js::Builtin),
            ),
            construct: None,
            realm,
        }))
    }
    pub fn constructor(
        name: &str,
        call: Option<NativeFn>,
        construct: impl Fn(Js, Vec<Js>) -> Result<Js, Thrown> + 'static,
    ) -> Js {
        let class_name = name.to_owned();
        let mut value = Self::function(name, move |this, args| match &call {
            Some(call) => call(this, args),
            None => Err(Thrown::type_error(&format!(
                "Class constructor {class_name} cannot be invoked without 'new'"
            ))),
        });
        let Js::Function(function) = &mut value else {
            unreachable!()
        };
        Rc::get_mut(function)
            .expect("unpublished constructor")
            .construct = Some(Construct::Native(Rc::new(construct)));
        value
    }

    pub fn is_constructor(&self) -> bool {
        matches!(self, Js::Function(function) if function.construct.is_some())
    }

    /// GetSuperConstructor reads metadata, without the shadowable `__proto__` property.
    pub fn super_constructor(&self) -> Result<Js, Thrown> {
        if !self.is_callable() {
            return Err(Thrown::type_error("Active class must be a function"));
        }
        Ok(self.prototype().unwrap_or(Js::Null))
    }

    /// The native construct body receives original new.target, then arguments.
    pub fn construct(&self, args: Vec<Js>, new_target: Option<&Js>) -> Result<Js, Thrown> {
        let Js::Function(function) = self else {
            return Err(Thrown::type_error("Value is not a constructor"));
        };
        let Some(construct) = &function.construct else {
            return Err(Thrown::type_error("Value is not a constructor"));
        };
        let new_target = new_target.unwrap_or(self);
        if !new_target.is_constructor() {
            return Err(Thrown::type_error("new.target is not a constructor"));
        }
        let value = match construct {
            Construct::Native(body) => body(new_target.clone(), args)?,
            Construct::Bound {
                target,
                args: prefix,
            } => {
                let mut combined = prefix.clone();
                combined.extend(args);
                let new_target = if new_target.same_reference(self) {
                    target
                } else {
                    new_target
                };
                target.construct(combined, Some(new_target))?
            }
        };
        if !value.is_object() {
            return Err(Thrown::type_error(
                "Constructor returned a non-object value",
            ));
        }
        Ok(value)
    }

    /// OrdinaryCreateFromConstructor for native class bodies allocating `this`.
    pub fn allocate_instance(&self) -> Result<Js, Thrown> {
        let Js::Function(function) = self else {
            return Err(Thrown::type_error("new.target is not a constructor"));
        };
        if !self.is_constructor() {
            return Err(Thrown::type_error("new.target is not a constructor"));
        }
        let prototype = self.get("prototype")?;
        let object = Js::object(vec![]);
        let Js::Object(data) = &object else {
            unreachable!()
        };
        *data.proto.borrow_mut() = Some(if prototype.is_object() {
            prototype
        } else {
            Js::Builtin(
                function
                    .realm
                    .reference(safe_cex_num::ProtoMember::Prototype),
            )
        });
        Ok(object)
    }
    pub fn new_map() -> Js {
        Js::Map(Rc::default())
    }
    pub fn new_set() -> Js {
        Js::Set(Rc::default())
    }

    /// Computed property access preserving the well-known iterator symbol.
    pub fn get_property(&self, key: &Js) -> Result<Js, Thrown> {
        if matches!(key, Js::SymbolIterator) {
            if self.is_nullish() {
                return self.get("Symbol(Symbol.iterator)");
            }
            return super::collections::iterator_method(self);
        }
        self.get_key(&key.to_js_string()?)
    }
    /// GetIterator for iterable spread and for-of consumers.
    pub fn iterator(&self) -> Result<Js, Thrown> {
        if self.is_nullish() {
            return Err(super::globals::not_iterable(self));
        }
        let method = self.get_property(&Js::SymbolIterator)?;
        if !method.is_callable() {
            return Err(super::globals::not_iterable(self));
        }
        let iterator = method.call(self.clone(), vec![])?;
        if !iterator.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        Ok(iterator)
    }
    /// Consume the shared iterator protocol for spread/Array.from adapters.
    pub fn iterable_values(&self) -> Result<Vec<Js>, Thrown> {
        let iterator = super::iterator::IteratorRecord::get(self)?;
        let mut values = vec![];
        while let Some(value) = iterator.step_value()? {
            values.push(value);
        }
        Ok(values)
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
            Js::Undefined
                | Js::SymbolIterator
                | Js::Null
                | Js::Bool(_)
                | Js::Number(_)
                | Js::BigInt(_)
                | Js::String(_)
        )
    }
    pub fn is_callable(&self) -> bool {
        matches!(self, Js::Function(_))
            || matches!(self, Js::Builtin(reference) if reference.is_callable())
    }
    pub fn type_of(&self) -> &'static str {
        match self {
            Js::Undefined => "undefined",
            Js::SymbolIterator => "symbol",
            Js::Null => "object",
            Js::Bool(_) => "boolean",
            Js::Number(_) => "number",
            Js::BigInt(_) => "bigint",
            Js::String(_) => "string",
            Js::Function(_) => "function",
            Js::Builtin(reference) if reference.is_callable() => "function",
            _ => "object",
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Js::Undefined | Js::Null => false,
            Js::Bool(b) => *b,
            Js::Number(n) => *n != 0.0 && !n.is_nan(),
            Js::BigInt(n) => **n != num_bigint::BigInt::default(),
            Js::String(s) => !s.is_empty(),
            _ => true,
        }
    }
    /// Strict equality (===).
    pub fn strict_equals(&self, other: &Js) -> bool {
        match (self, other) {
            (Js::Undefined, Js::Undefined)
            | (Js::Null, Js::Null)
            | (Js::SymbolIterator, Js::SymbolIterator) => true,
            (Js::Bool(a), Js::Bool(b)) => a == b,
            (Js::Number(a), Js::Number(b)) => a == b,
            (Js::BigInt(a), Js::BigInt(b)) => a == b,
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
            (Js::Builtin(a), Js::Builtin(b)) => a == b,
            (Js::Function(a), Js::Function(b)) => Rc::ptr_eq(a, b),
            (Js::Promise(a), Js::Promise(b)) => a.ptr_eq(b),
            (Js::Map(a), Js::Map(b)) => Rc::ptr_eq(a, b),
            (Js::Set(a), Js::Set(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// ToString, with OrdinaryToPrimitive's string hint for live objects.
    pub fn to_js_string(&self) -> Result<Str, Thrown> {
        if matches!(self, Js::Object(_) | Js::Function(_)) {
            return self
                .ordinary_to_primitive(["toString", "valueOf"])?
                .to_js_string();
        }
        self.intrinsic_string()
    }
    pub(super) fn ordinary_to_primitive(&self, names: [&str; 2]) -> Result<Js, Thrown> {
        for name in names {
            let method = self.get(name)?;
            if !method.is_callable() {
                continue;
            }
            let primitive = match &method {
                Js::Builtin(reference)
                    if reference.member()
                        == safe_cex_num::BuiltinIdentity::Object(
                            safe_cex_num::ProtoMember::ToString,
                        ) =>
                {
                    let tag = match self {
                        Js::Function(_) => "Function",
                        Js::Object(object) if object.class == "Buffer" => "Uint8Array",
                        Js::Object(object)
                            if matches!(
                                object.class,
                                "Error"
                                    | "Blob"
                                    | "ArrayBuffer"
                                    | "Map Iterator"
                                    | "Set Iterator"
                                    | "Array Iterator"
                                    | "String Iterator"
                            ) =>
                        {
                            object.class
                        }
                        Js::Object(_) => "Object",
                        _ => unreachable!(),
                    };
                    Js::str(&format!("[object {tag}]"))
                }
                Js::Builtin(reference)
                    if reference.member()
                        == safe_cex_num::BuiltinIdentity::Object(
                            safe_cex_num::ProtoMember::ValueOf,
                        ) =>
                {
                    self.clone()
                }
                _ => method.call(self.clone(), vec![])?,
            };
            if !primitive.is_object() {
                return Ok(primitive);
            }
        }
        Err(Thrown::type_error(
            "Cannot convert object to primitive value",
        ))
    }
    /// Stock intrinsic conversion after primitive coercion dispatch.
    fn intrinsic_string(&self) -> Result<Str, Thrown> {
        Ok(match self {
            Js::Undefined => utf16("undefined"),
            Js::SymbolIterator => {
                return Err(Thrown::type_error(
                    "Cannot convert a Symbol value to a string",
                ));
            }
            Js::Null => utf16("null"),
            Js::Bool(b) => utf16(if *b { "true" } else { "false" }),
            Js::Number(n) => utf16(&js_number_string(*n)),
            Js::BigInt(n) => utf16(&n.to_string()),
            Js::Builtin(reference) => utf16(&reference.js_string()),
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
                if matches!(
                    o.class,
                    "Blob"
                        | "ArrayBuffer"
                        | "Map Iterator"
                        | "Set Iterator"
                        | "Array Iterator"
                        | "String Iterator"
                ) {
                    utf16(&format!("[object {}]", o.class))
                } else if o.class == "Error" {
                    error_string(self)?
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
            Js::SymbolIterator => {
                return Err(Thrown::type_error(
                    "Cannot convert a Symbol value to a number",
                ));
            }
            Js::Null => 0.0,
            Js::Bool(b) => f64::from(u8::from(*b)),
            Js::Number(n) => *n,
            Js::BigInt(_) => {
                return Err(Thrown::type_error(
                    "Cannot convert a BigInt value to a number",
                ));
            }
            Js::String(s) => string_to_number(&String::from_utf16_lossy(s)),
            Js::Builtin(reference) => string_to_number(&reference.js_string()),
            Js::Object(_) | Js::Function(_) => self
                .ordinary_to_primitive(["valueOf", "toString"])?
                .to_number()?,
            Js::Array(_) => string_to_number(&String::from_utf16_lossy(&self.to_js_string()?)),
            _ => f64::NAN,
        })
    }
    pub fn parse_float(&self) -> Result<f64, Thrown> {
        Ok(parse_float(&String::from_utf16_lossy(
            &self.to_js_string()?,
        )))
    }

    fn own_props(&self) -> Option<&RefCell<IndexMap<Str, Prop>>> {
        match self {
            Js::Object(o) => Some(&o.props),
            Js::Function(f) => Some(&f.props),
            _ => None,
        }
    }

    pub(super) fn prototype(&self) -> Option<Js> {
        match self {
            Js::Object(object) => object.proto.borrow().clone(),
            Js::Function(function) => function.proto.borrow().clone(),
            Js::Builtin(reference) => reference.prototype().map(Js::Builtin),
            _ => None,
        }
    }

    /// Ordinary [[SetPrototypeOf]], shared by the legacy setter and Object facade.
    /// The target's own properties and opaque native resource remain untouched.
    pub(super) fn set_prototype(&self, value: Js) -> Result<Js, Thrown> {
        if self.is_nullish() {
            return Err(Thrown::type_error(
                "Cannot convert undefined or null to object",
            ));
        }
        if !value.is_object() && !matches!(value, Js::Null) {
            return Err(Thrown::type_error(
                "Object prototype may only be an Object or null",
            ));
        }
        if !self.is_object() {
            return Ok(self.clone());
        }
        let parent = (!matches!(value, Js::Null)).then_some(value);
        let target = match self {
            Js::Object(object) => &object.proto,
            Js::Function(function) => &function.proto,
            Js::Builtin(_) if same_optional_value(&self.prototype(), &parent) => {
                return Ok(self.clone());
            }
            _ => {
                return Err(Thrown::type_error(
                    "Prototype mutation requires an ordinary object or function",
                ));
            }
        };
        if same_optional_value(&target.borrow(), &parent) {
            return Ok(self.clone());
        }
        if !self.is_extensible()? {
            return Err(Thrown::type_error(
                "Cannot change prototype of a non-extensible object",
            ));
        }
        let mut candidate = parent.clone();
        while let Some(ancestor) = candidate {
            if ancestor.same_reference(self) {
                return Err(Thrown::type_error("Cyclic __proto__ value"));
            }
            candidate = ancestor.prototype();
        }
        *target.borrow_mut() = parent;
        Ok(self.clone())
    }

    /// [[Get]] including array/string length and index, accessors and prototype.
    pub fn get(&self, key: &str) -> Result<Js, Thrown> {
        self.get_key(&utf16(key))
    }
    pub fn get_key(&self, key: &[u16]) -> Result<Js, Thrown> {
        self.get_key_receiver(key, self)
    }
    fn get_key_receiver(&self, key: &[u16], receiver: &Js) -> Result<Js, Thrown> {
        match self {
            Js::Builtin(reference) => {
                let Ok(key) = String::from_utf16(key) else {
                    return Ok(Js::Undefined);
                };
                // Object.prototype's legacy accessor observes the original receiver.
                if key == "__proto__" && !reference.own_fields().contains_key(&key) {
                    return Ok(receiver.prototype().unwrap_or(Js::Null));
                }
                return Ok(Js::from_prototype(reference.get(&key)));
            }
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
                    return Ok(items
                        .borrow()
                        .get(i)
                        .cloned()
                        .flatten()
                        .unwrap_or(Js::Undefined));
                }
                return Ok(super::builtins::member(self, key).unwrap_or(Js::Undefined));
            }
            Js::Map(m) if key == utf16("size").as_ref() => {
                return Ok(Js::Number(m.borrow().len() as f64));
            }
            Js::Set(s) if key == utf16("size").as_ref() => {
                return Ok(Js::Number(s.borrow().len() as f64));
            }
            Js::Map(_)
            | Js::Set(_)
            | Js::Promise(_)
            | Js::Number(_)
            | Js::BigInt(_)
            | Js::Bool(_) => {
                return Ok(super::builtins::member(self, key).unwrap_or(Js::Undefined));
            }
            _ => {}
        }
        if let Js::Object(data) = self
            && key == [b'n' as u16, b'e' as u16, b'x' as u16, b't' as u16]
            && data.iterator_next.borrow().is_some()
            && !data.props.borrow().contains_key(key)
        {
            return Ok(super::collections::iterator_next_method(data.class));
        }
        let Some(props) = self.own_props() else {
            return Ok(Js::Undefined);
        };
        let prop = props.borrow().get(key).cloned();
        match prop {
            Some(Prop::Data(v) | Prop::DataDescriptor { value: v, .. }) => Ok(v),
            Some(Prop::Accessor {
                get: Some(getter), ..
            }) => getter.call(receiver.clone(), vec![]),
            Some(Prop::Accessor { get: None, .. }) => Ok(Js::Undefined),
            None => {
                if let Some(proto) = self.prototype() {
                    return proto.get_key_receiver(key, receiver);
                }
                Ok(Js::Undefined)
            }
        }
    }
    pub fn has_own(&self, key: &str) -> bool {
        if let Js::Builtin(reference) = self {
            return reference.own_fields().contains_key(key);
        }
        match self {
            Js::Array(items) => array_index(&utf16(key))
                .is_some_and(|i| matches!(items.borrow().get(i), Some(Some(_)))),
            _ => self
                .own_props()
                .is_some_and(|p| p.borrow().contains_key(utf16(key).as_ref())),
        }
    }
    /// Ordinary [[Set]]: runs setters, otherwise writes or appends an own data property.
    pub fn set(&self, key: &str, value: Js) -> Result<(), Thrown> {
        self.set_key(&utf16(key), value)
    }
    pub fn set_write_observer(&self, observer: Option<WriteObserver>) -> Result<(), Thrown> {
        let Js::Object(object) = self else {
            return Err(Thrown::type_error(
                "Write observers require an ordinary object",
            ));
        };
        let previous = object.write_observer.replace(observer);
        drop(previous);
        Ok(())
    }
    pub fn set_key(&self, key: &[u16], value: Js) -> Result<(), Thrown> {
        self.set_property(key, value.clone())?;
        let observer = match self {
            Js::Object(object) => object.write_observer.borrow().clone(),
            _ => None,
        };
        if let Some(observer) = observer {
            observer(self, key, &value)?;
        }
        Ok(())
    }
    fn set_property(&self, key: &[u16], value: Js) -> Result<(), Thrown> {
        if let Js::Builtin(reference) = self {
            let key = String::from_utf16(key).map_err(|_| {
                Thrown::type_error("lone UTF-16 surrogate at an intrinsic boundary")
            })?;
            let value = match value {
                Js::Undefined => safe_cex_num::PrototypeScalar::Undefined,
                Js::Null => safe_cex_num::PrototypeScalar::Null,
                Js::Bool(value) => safe_cex_num::PrototypeScalar::Boolean(value),
                Js::Number(value) => safe_cex_num::PrototypeScalar::Number(value),
                Js::String(value) => {
                    safe_cex_num::PrototypeScalar::String(String::from_utf16(&value).map_err(
                        |_| Thrown::type_error("lone UTF-16 surrogate at an intrinsic boundary"),
                    )?)
                }
                _ => {
                    return Err(Thrown::type_error(
                        "intrinsic referent fields require native primitive data",
                    ));
                }
            };
            reference.set(&key, value);
            return Ok(());
        }
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
            return Err(Thrown::type_error(&format!(
                "js_runtime arrays carry no named properties (setting '{}')",
                String::from_utf16_lossy(key)
            )));
        }
        let Some(props) = self.own_props() else {
            if self.is_nullish() {
                return Err(Thrown::type_error(&format!(
                    "Cannot set properties of {} (setting '{}')",
                    self.to_display(),
                    String::from_utf16_lossy(key)
                )));
            }
            if let Js::String(units) = self
                && (key == utf16("length").as_ref()
                    || array_index(key).is_some_and(|index| index < units.len()))
            {
                return Err(Thrown::type_error(&format!(
                    "Cannot assign to read only property '{}' of string '{}'",
                    String::from_utf16_lossy(key),
                    self.to_display()
                )));
            }
            // Strict-mode [[Set]] on a primitive throws; Map/Set/Promise expandos are unsupported.
            return Err(Thrown::type_error(&format!(
                "Cannot create property '{}' on {} '{}'",
                String::from_utf16_lossy(key),
                self.type_of(),
                self.to_display()
            )));
        };
        let existing = props.borrow().get(key).cloned();
        // Inherited setters receive the original receiver; inherited read-only data
        // properties refuse an own shadow, as ordinary strict-mode [[Set]] does.
        if existing.is_none() {
            let mut ancestor = self.prototype();
            while let Some(parent) = ancestor {
                let inherited = parent
                    .own_props()
                    .and_then(|p| p.borrow().get(key).cloned());
                match inherited {
                    Some(Prop::Accessor {
                        set: Some(setter), ..
                    }) => return setter.call(self.clone(), vec![value]).map(|_| ()),
                    Some(Prop::Accessor { set: None, .. })
                    | Some(Prop::DataDescriptor {
                        writable: false, ..
                    }) => {
                        return Err(Thrown::type_error("Cannot assign to read only property"));
                    }
                    Some(_) => break,
                    None => {}
                }
                if let Js::Builtin(reference) = &parent {
                    if String::from_utf16(key)
                        .is_ok_and(|key| reference.own_fields().contains_key(&key))
                    {
                        break;
                    }
                    if key == utf16("__proto__").as_ref()
                        && reference.member()
                            == safe_cex_num::BuiltinIdentity::Object(
                                safe_cex_num::ProtoMember::Prototype,
                            )
                    {
                        if !value.is_object() && !matches!(value, Js::Null) {
                            return Ok(());
                        }
                        self.set_prototype(value)?;
                        return Ok(());
                    }
                }
                ancestor = parent.prototype();
            }
        }
        if existing.is_none() && !self.is_extensible()? {
            return Err(Thrown::type_error(
                "Cannot add a property to a non-extensible object",
            ));
        }
        match existing {
            Some(Prop::Accessor {
                set: Some(setter), ..
            }) => setter.call(self.clone(), vec![value]).map(|_| ()),
            Some(Prop::Accessor { set: None, .. })
            | Some(Prop::DataDescriptor {
                writable: false, ..
            }) => Err(Thrown::type_error("Cannot assign to read only property")),
            Some(Prop::DataDescriptor {
                enumerable,
                configurable,
                ..
            }) => {
                props.borrow_mut().insert(
                    key.into(),
                    Prop::DataDescriptor {
                        value,
                        writable: true,
                        enumerable,
                        configurable,
                    },
                );
                Ok(())
            }
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
            if matches!(
                props.borrow().get(key),
                Some(
                    Prop::DataDescriptor {
                        configurable: false,
                        ..
                    } | Prop::Accessor {
                        configurable: false,
                        ..
                    }
                )
            ) {
                return Err(Thrown::type_error("Cannot redefine property"));
            }
            if !props.borrow().contains_key(key) && !self.is_extensible()? {
                return Err(Thrown::type_error(
                    "Cannot define a property on a non-extensible object",
                ));
            }
            props.borrow_mut().insert(key.into(), Prop::Data(value));
        }
        Ok(())
    }
    fn extensible_flag(&self) -> Option<&Cell<bool>> {
        match self {
            Js::Object(object) => Some(&object.extensible),
            Js::Function(function) => Some(&function.extensible),
            _ => None,
        }
    }
    pub(super) fn is_extensible(&self) -> Result<bool, Thrown> {
        match self.extensible_flag() {
            Some(flag) => Ok(flag.get()),
            None if !self.is_object() => Ok(false),
            None => Err(Thrown::type_error(
                "Extensibility requires an ordinary object or function",
            )),
        }
    }
    pub(super) fn prevent_extensions(&self) -> Result<Js, Thrown> {
        match self.extensible_flag() {
            Some(flag) => flag.set(false),
            None if !self.is_object() => return Ok(self.clone()),
            None => {
                return Err(Thrown::type_error(
                    "Extensibility requires an ordinary object or function",
                ));
            }
        }
        Ok(self.clone())
    }
    pub(super) fn freeze(&self) -> Result<Js, Thrown> {
        self.set_integrity(true)
    }
    pub(super) fn seal(&self) -> Result<Js, Thrown> {
        self.set_integrity(false)
    }
    fn set_integrity(&self, frozen: bool) -> Result<Js, Thrown> {
        self.prevent_extensions()?;
        if let Some(props) = self.own_props() {
            for property in props.borrow_mut().values_mut() {
                match property {
                    Prop::Data(value) => {
                        *property = Prop::DataDescriptor {
                            value: value.clone(),
                            writable: !frozen,
                            enumerable: true,
                            configurable: false,
                        }
                    }
                    Prop::DataDescriptor {
                        writable,
                        configurable,
                        ..
                    } => {
                        if frozen {
                            *writable = false;
                        }
                        *configurable = false;
                    }
                    Prop::Accessor { configurable, .. } => *configurable = false,
                }
            }
        }
        Ok(self.clone())
    }
    pub(super) fn is_frozen(&self) -> Result<bool, Thrown> {
        if !self.is_object() {
            return Ok(true);
        }
        if self.is_extensible()? {
            return Ok(false);
        }
        Ok(self
            .own_props()
            .expect("ordinary property storage")
            .borrow()
            .values()
            .all(|property| match property {
                Prop::Data(_) => false,
                Prop::DataDescriptor {
                    writable,
                    configurable,
                    ..
                } => !writable && !configurable,
                Prop::Accessor { configurable, .. } => !configurable,
            }))
    }
    pub(super) fn own_property_descriptor(&self, key: &[u16]) -> Result<Js, Thrown> {
        let props = self.own_props().ok_or_else(|| {
            Thrown::type_error("Descriptor reflection requires an ordinary object or function")
        })?;
        Ok(match props.borrow().get(key) {
            None => Js::Undefined,
            Some(Prop::Data(value)) => Js::object(vec![
                ("value", value.clone()),
                ("writable", Js::Bool(true)),
                ("enumerable", Js::Bool(true)),
                ("configurable", Js::Bool(true)),
            ]),
            Some(Prop::DataDescriptor {
                value,
                writable,
                enumerable,
                configurable,
            }) => Js::object(vec![
                ("value", value.clone()),
                ("writable", Js::Bool(*writable)),
                ("enumerable", Js::Bool(*enumerable)),
                ("configurable", Js::Bool(*configurable)),
            ]),
            Some(Prop::Accessor {
                get,
                set,
                enumerable,
                configurable,
            }) => Js::object(vec![
                ("get", get.clone().unwrap_or(Js::Undefined)),
                ("set", set.clone().unwrap_or(Js::Undefined)),
                ("enumerable", Js::Bool(*enumerable)),
                ("configurable", Js::Bool(*configurable)),
            ]),
        })
    }
    /// ToPropertyDescriptor observes fields in order and retains accessor identities.
    pub(super) fn define_property(&self, key: &[u16], descriptor: &Js) -> Result<(), Thrown> {
        let props = self.own_props().ok_or_else(|| {
            Thrown::type_error(
                "Object.defineProperty target must be an ordinary object or function",
            )
        })?;
        if !descriptor.is_object() {
            return Err(Thrown::type_error("Property description must be an object"));
        }
        let read = |name: &str| -> Result<Option<Js>, Thrown> {
            if descriptor.has_property(&utf16(name)) {
                Ok(Some(descriptor.get(name)?))
            } else {
                Ok(None)
            }
        };
        let enumerable = read("enumerable")?.map(|value| value.truthy());
        let configurable = read("configurable")?.map(|value| value.truthy());
        let value = read("value")?;
        let writable = read("writable")?.map(|value| value.truthy());
        let get = read("get")?;
        if get
            .as_ref()
            .is_some_and(|value| !value.is_undefined() && !value.is_callable())
        {
            return Err(Thrown::type_error("Getter must be a function"));
        }
        let set = read("set")?;
        if set
            .as_ref()
            .is_some_and(|value| !value.is_undefined() && !value.is_callable())
        {
            return Err(Thrown::type_error("Setter must be a function"));
        }
        let accessor = get.is_some() || set.is_some();
        let data = value.is_some() || writable.is_some();
        if accessor && data {
            return Err(Thrown::type_error(
                "Invalid mixed data and accessor descriptor",
            ));
        }
        let existing = props.borrow().get(key).cloned();
        if existing.is_none() && !self.is_extensible()? {
            return Err(Thrown::type_error(
                "Cannot define a property on a non-extensible object",
            ));
        }
        let (old_enumerable, old_configurable) = match &existing {
            Some(Prop::Data(_)) => (true, true),
            Some(
                Prop::DataDescriptor {
                    enumerable,
                    configurable,
                    ..
                }
                | Prop::Accessor {
                    enumerable,
                    configurable,
                    ..
                },
            ) => (*enumerable, *configurable),
            None => (false, false),
        };
        let old_accessor = matches!(existing, Some(Prop::Accessor { .. }));
        if existing.is_some() && !old_configurable {
            if configurable == Some(true)
                || enumerable.is_some_and(|value| value != old_enumerable)
                || (accessor && !old_accessor)
                || (data && old_accessor)
            {
                return Err(Thrown::type_error("Cannot redefine property"));
            }
            match &existing {
                Some(Prop::DataDescriptor {
                    value: previous,
                    writable: false,
                    ..
                }) if writable == Some(true)
                    || value
                        .as_ref()
                        .is_some_and(|value| !same_value(value, previous)) =>
                {
                    return Err(Thrown::type_error("Cannot redefine property"));
                }
                Some(Prop::Accessor {
                    get: previous_get,
                    set: previous_set,
                    ..
                }) => {
                    let getter_changed = get.as_ref().is_some_and(|get| {
                        !same_value(get, previous_get.as_ref().unwrap_or(&Js::Undefined))
                    });
                    let setter_changed = set.as_ref().is_some_and(|set| {
                        !same_value(set, previous_set.as_ref().unwrap_or(&Js::Undefined))
                    });
                    if getter_changed || setter_changed {
                        return Err(Thrown::type_error("Cannot redefine property"));
                    }
                }
                _ => {}
            }
        }
        let next = if accessor || (old_accessor && !data) {
            let (previous_get, previous_set) = match existing {
                Some(Prop::Accessor { get, set, .. }) => (get, set),
                _ => (None, None),
            };
            Prop::Accessor {
                get: get
                    .map(|value| (!value.is_undefined()).then_some(value))
                    .unwrap_or(previous_get),
                set: set
                    .map(|value| (!value.is_undefined()).then_some(value))
                    .unwrap_or(previous_set),
                enumerable: enumerable.unwrap_or(old_enumerable),
                configurable: configurable.unwrap_or(old_configurable),
            }
        } else {
            let (previous_value, previous_writable) = match existing {
                Some(Prop::Data(value)) => (value, true),
                Some(Prop::DataDescriptor {
                    value, writable, ..
                }) => (value, writable),
                _ => (Js::Undefined, false),
            };
            Prop::DataDescriptor {
                value: value.unwrap_or(previous_value),
                writable: writable.unwrap_or(previous_writable),
                enumerable: enumerable.unwrap_or(old_enumerable),
                configurable: configurable.unwrap_or(old_configurable),
            }
        };
        props.borrow_mut().insert(key.into(), next);
        Ok(())
    }
    pub(super) fn has_property(&self, key: &[u16]) -> bool {
        if self
            .own_props()
            .is_some_and(|p| p.borrow().contains_key(key))
        {
            return true;
        }
        match self {
            Js::Array(items) => {
                key == utf16("length").as_ref()
                    || array_index(key)
                        .is_some_and(|i| matches!(items.borrow().get(i), Some(Some(_))))
            }
            Js::Builtin(reference) => String::from_utf16(key).is_ok_and(|key| {
                reference.own_fields().contains_key(&key)
                    || !matches!(reference.get(&key), safe_cex_num::PrototypeValue::Undefined)
            }),
            _ => self
                .prototype()
                .is_some_and(|parent| parent.has_property(key)),
        }
    }
    pub fn define_accessor(
        &self,
        key: &str,
        getter: Option<Getter>,
        setter: Option<Setter>,
    ) -> Result<(), Thrown> {
        let get = getter.map_or(Js::Undefined, |get| {
            Js::function("get", move |this, _| get(&this))
        });
        let set = setter.map_or(Js::Undefined, |set| {
            Js::function("set", move |this, args| {
                set(&this, arg(&args, 0))?;
                Ok(Js::Undefined)
            })
        });
        let descriptor = Js::object(vec![
            ("get", get),
            ("set", set),
            ("enumerable", Js::Bool(true)),
            ("configurable", Js::Bool(true)),
        ]);
        descriptor.set_prototype(Js::Null)?;
        self.define_property(&utf16(key), &descriptor)
    }
    pub fn delete(&self, key: &[u16]) -> Result<(), Thrown> {
        if self.delete_property(key)? {
            Ok(())
        } else {
            Err(Thrown::type_error(
                "Cannot delete a non-configurable property",
            ))
        }
    }
    pub fn reflect_delete(&self, key: &[u16]) -> Result<bool, Thrown> {
        if !self.is_object() {
            return Err(Thrown::type_error(
                "Reflect.deleteProperty requires an object",
            ));
        }
        self.delete_property(key)
    }
    fn delete_property(&self, key: &[u16]) -> Result<bool, Thrown> {
        if self.is_nullish() {
            return Err(Thrown::type_error(
                "Cannot convert undefined or null to object",
            ));
        }
        if let Some(props) = self.own_props() {
            if matches!(
                props.borrow().get(key),
                Some(
                    Prop::DataDescriptor {
                        configurable: false,
                        ..
                    } | Prop::Accessor {
                        configurable: false,
                        ..
                    }
                )
            ) {
                return Ok(false);
            }
            let removed = props.borrow_mut().shift_remove(key);
            drop(removed);
            return Ok(true);
        }
        match self {
            Js::Array(items) => {
                if key == utf16("length").as_ref() {
                    return Ok(false);
                }
                if let Some(i) = array_index(key) {
                    let removed = items.borrow_mut().get_mut(i).and_then(Option::take);
                    drop(removed);
                }
                Ok(true)
            }
            Js::String(units) => Ok(key != utf16("length").as_ref()
                && !array_index(key).is_some_and(|index| index < units.len())),
            Js::Builtin(reference)
                if reference
                    .own_properties()
                    .iter()
                    .any(|(name, _)| name.encode_utf16().eq(key.iter().copied())) =>
            {
                Err(Thrown::type_error(
                    "Intrinsic property deletion is unsupported",
                ))
            }
            _ => Ok(true),
        }
    }
    /// Enumerable own string keys in OrdinaryOwnPropertyKeys order.
    pub fn own_keys(&self) -> Vec<Str> {
        match self {
            Js::Builtin(reference) => {
                let fields = reference.own_fields();
                safe_cex_num::js_own_keys(fields.keys().map(String::as_str))
                    .into_iter()
                    .map(utf16)
                    .collect()
            }
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
                Self::enumerable_property_indices(props)
                    .into_iter()
                    .map(|(key, _)| key)
                    .collect()
            }
        }
    }
    fn enumerable_property_indices(props: &RefCell<IndexMap<Str, Prop>>) -> Vec<(Str, usize)> {
        let mut keys: Vec<_> = props
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, (_, prop))| {
                !matches!(
                    prop,
                    Prop::DataDescriptor {
                        enumerable: false,
                        ..
                    } | Prop::Accessor {
                        enumerable: false,
                        ..
                    }
                )
            })
            .map(|(index, (key, _))| (key.clone(), index))
            .collect();
        keys.sort_by_key(|(key, _)| array_index(key).unwrap_or(usize::MAX));
        keys
    }
    /// Object.getOwnPropertyNames: own string keys, enumerable or not, in
    /// OrdinaryOwnPropertyKeys order; array and string `length` follows the indices.
    pub fn own_property_names(&self) -> Result<Vec<Str>, Thrown> {
        Ok(match self {
            Js::Array(_) | Js::String(_) => {
                let mut keys = self.own_keys();
                keys.push(utf16("length"));
                keys
            }
            Js::Object(_) | Js::Function(_) => {
                let keys: Vec<Str> = self
                    .own_props()
                    .expect("ordinary property storage")
                    .borrow()
                    .keys()
                    .cloned()
                    .collect();
                let mut indexed: Vec<(usize, Str)> = keys
                    .iter()
                    .filter_map(|k| array_index(k).map(|i| (i, k.clone())))
                    .collect();
                indexed.sort_by_key(|(i, _)| *i);
                let mut out: Vec<Str> = indexed.into_iter().map(|(_, k)| k).collect();
                out.extend(keys.into_iter().filter(|k| array_index(k).is_none()));
                out
            }
            Js::Builtin(_) => {
                return Err(Thrown::type_error(
                    "Object.getOwnPropertyNames does not model intrinsic keys",
                ));
            }
            _ => vec![],
        })
    }
    pub(super) fn for_in_own_names(&self) -> Result<Vec<Str>, Thrown> {
        match self {
            Js::Builtin(reference) => Ok(reference
                .own_properties()
                .into_iter()
                .map(|(name, _)| utf16(&name))
                .collect()),
            _ => self.own_property_names(),
        }
    }
    pub(super) fn own_enumerability(&self, key: &[u16]) -> Result<Option<bool>, Thrown> {
        match self {
            Js::Builtin(reference) => Ok(reference
                .own_properties()
                .into_iter()
                .find(|(name, _)| name.encode_utf16().eq(key.iter().copied()))
                .map(|(_, enumerable)| enumerable)),
            Js::Array(_) | Js::String(_) => {
                if key == utf16("length").as_ref() {
                    return Ok(Some(false));
                }
                Ok(self
                    .own_keys()
                    .iter()
                    .any(|name| name.as_ref() == key)
                    .then_some(true))
            }
            Js::Object(_) | Js::Function(_) => {
                let descriptor = self.own_property_descriptor(key)?;
                if descriptor.is_undefined() {
                    Ok(None)
                } else {
                    Ok(Some(descriptor.get("enumerable")?.truthy()))
                }
            }
            _ => Ok(None),
        }
    }
    /// Object spread / Object.assign source read: own enumerable values in key order.
    pub fn own_entries(&self) -> Result<Vec<(Str, Js)>, Thrown> {
        if let Some(props) = self.own_props() {
            let keys = Self::enumerable_property_indices(props);
            let mut entries = Vec::with_capacity(keys.len());
            let mut indices_valid = true;
            for (key, index) in keys {
                let value = if indices_valid {
                    match &props
                        .borrow()
                        .get_index(index)
                        .expect("unmodified property index")
                        .1
                    {
                        Prop::Data(value) | Prop::DataDescriptor { value, .. } => {
                            Some(value.clone())
                        }
                        Prop::Accessor { .. } => None,
                    }
                } else {
                    None
                };
                let value = match value {
                    Some(value) => value,
                    None => {
                        // A getter can reorder or replace later properties.
                        // Subsequent reads must use their live keys.
                        indices_valid = false;
                        self.get_key(&key)?
                    }
                };
                entries.push((key, value));
            }
            return Ok(entries);
        }
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
        self.array_items().map(|items| {
            items
                .into_iter()
                .map(|v| v.unwrap_or(Js::Undefined))
                .collect()
        })
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
            Js::Builtin(reference) => match reference.member() {
                safe_cex_num::BuiltinIdentity::FunctionPrototype => Ok(Js::Undefined),
                safe_cex_num::BuiltinIdentity::FunctionMethod(
                    safe_cex_num::FunctionMethod::Call,
                ) => {
                    if !this.is_callable() {
                        return Err(Thrown::type_error(
                            "Function.prototype.call requires a callable receiver",
                        ));
                    }
                    this.call(arg(&args, 0), args.into_iter().skip(1).collect())
                }
                safe_cex_num::BuiltinIdentity::FunctionMethod(
                    safe_cex_num::FunctionMethod::Bind,
                ) => {
                    if !this.is_callable() {
                        return Err(Thrown::type_error("Bind must be called on a function"));
                    }
                    let proto = this.prototype();
                    let bound_this = arg(&args, 0);
                    let bound_args: Vec<Js> = args.into_iter().skip(1).collect();
                    let mut length = 0.;
                    if this.own_enumerability(&utf16("length"))?.is_some()
                        && let Js::Number(target_length) = this.get("length")?
                    {
                        length = if target_length.is_nan() {
                            0.
                        } else {
                            safe_cex_num::math_max(
                                target_length.trunc() - bound_args.len() as f64,
                                0.,
                            )
                        };
                    }
                    let target_name = this.get("name")?;
                    let mut name: Vec<u16> = "bound ".encode_utf16().collect();
                    if let Js::String(target_name) = target_name {
                        name.extend_from_slice(&target_name);
                    }
                    let construct = this.is_constructor().then(|| Construct::Bound {
                        target: this.clone(),
                        args: bound_args.clone(),
                    });
                    let realm = match &this {
                        Js::Function(function) => function.realm.clone(),
                        Js::Builtin(reference) => reference.realm().clone(),
                        _ => unreachable!(),
                    };
                    let name: Str = name.into();
                    let props = IndexMap::from([
                        (
                            utf16("length"),
                            Prop::DataDescriptor {
                                value: Js::Number(length),
                                writable: false,
                                enumerable: false,
                                configurable: true,
                            },
                        ),
                        (
                            utf16("name"),
                            Prop::DataDescriptor {
                                value: Js::String(name.clone()),
                                writable: false,
                                enumerable: false,
                                configurable: true,
                            },
                        ),
                    ]);
                    Ok(Js::Function(Rc::new(FunctionData {
                        extensible: Cell::new(true),
                        name,
                        call: Rc::new(move |_, args| {
                            let mut combined = bound_args.clone();
                            combined.extend(args);
                            this.call(bound_this.clone(), combined)
                        }),
                        props: RefCell::new(props),
                        proto: RefCell::new(proto),
                        construct,
                        realm,
                    })))
                }
                safe_cex_num::BuiltinIdentity::FunctionMethod(
                    safe_cex_num::FunctionMethod::ToString,
                ) => {
                    if !this.is_callable() {
                        return Err(Thrown::type_error(
                            "Function.prototype.toString requires a callable receiver",
                        ));
                    }
                    this.intrinsic_string().map(Js::String)
                }
                _ => Err(Thrown::type_error(
                    "Intrinsic function execution is not bound",
                )),
            },
            other => Err(Thrown::type_error(&format!(
                "{} is not a function",
                other.to_display()
            ))),
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
        out = out
            .checked_mul(10)?
            .checked_add(usize::from(c - b'0' as u16))?;
    }
    (out < u32::MAX as usize).then_some(out)
}

/// StringToNumber per ECMAScript (whitespace trim, hex/octal/binary, Infinity).
pub fn string_to_number(text: &str) -> f64 {
    safe_cex_num::number_from_string(text)
}

/// serde_json::Value -> fresh native value (objects/arrays are new allocations).
pub fn from_json(value: &serde_json::Value) -> Js {
    match value {
        serde_json::Value::Null => Js::Null,
        serde_json::Value::Bool(b) => Js::Bool(*b),
        serde_json::Value::Number(n) => Js::Number(
            safe_cex_num::numeric_ingress::js_value_as_f64(value)
                .unwrap_or_else(|| n.as_f64().unwrap_or(f64::NAN)),
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
        Js::Undefined | Js::SymbolIterator | Js::Null => serde_json::Value::Null,
        Js::Bool(b) => serde_json::Value::Bool(*b),
        Js::Number(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Js::BigInt(n) => serde_json::Value::String(n.to_string()),
        Js::String(s) => serde_json::Value::String(
            String::from_utf16(s)
                .map_err(|_| Thrown::type_error("lone UTF-16 surrogate at a typed boundary"))?,
        ),
        Js::Array(items) => serde_json::Value::Array(
            items
                .borrow()
                .iter()
                .map(|v| {
                    v.as_ref()
                        .map(to_json)
                        .transpose()
                        .map(|v| v.unwrap_or(serde_json::Value::Null))
                })
                .collect::<Result<_, _>>()?,
        ),
        Js::Builtin(reference) if reference.is_callable() => serde_json::Value::Null,
        Js::Object(_) | Js::Builtin(_) => {
            let mut out = serde_json::Map::new();
            for (k, v) in value.own_entries()? {
                if matches!(v, Js::Undefined | Js::Function(_)) || v.is_callable() {
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

/// Typed boundary helpers retain native numbers; JSON wire projection is separate.
pub fn to_typed<T: serde::de::DeserializeOwned>(value: &Js) -> Result<T, Thrown> {
    typed::decode(value, false)
}
/// Declared DTO profile: retain own undefined fields as native unit values.
/// Unlike the JSON-shaped profile, callable fields are refused rather than omitted.
pub fn to_typed_preserving_undefined<T: serde::de::DeserializeOwned>(
    value: &Js,
) -> Result<T, Thrown> {
    typed::decode(value, true)
}
pub fn from_typed<T: serde::Serialize + ?Sized>(value: &T) -> Result<Js, Thrown> {
    typed::encode(value)
}

/// Capture data-only adapter config before JSON projection, retaining own
/// undefined fields and native number bits through transport/signing/retry.
pub fn to_retained_input(
    value: &Js,
) -> Result<safe_cex_types::retained_json::RetainedJsonInput, Thrown> {
    typed::capture(value, 0, false, true)
}
/// Restore the canonical retained data carrier without a JSON roundtrip.
pub fn from_retained_input(value: safe_cex_types::retained_json::RetainedJsonInput) -> Js {
    typed::from_input(value)
}

/// The JSON-shaped projection a typed wire body takes, keeping undefined and BigInt.
pub fn to_js_json(value: &Js) -> Result<safe_cex_num::JsJsonValue, Thrown> {
    use safe_cex_num::JsJsonValue as J;
    Ok(match value {
        Js::Builtin(reference) if reference.is_callable() => J::Undefined,
        Js::Undefined | Js::Function(_) => J::Undefined,
        Js::Null => J::Null,
        Js::Bool(b) => J::Bool(*b),
        Js::Number(n) => J::Number(*n),
        Js::BigInt(n) => J::BigInt((**n).clone()),
        Js::String(s) => J::String(String::from_utf16_lossy(s)),
        Js::Array(_) => J::Array(
            value
                .array_values()
                .unwrap_or_default()
                .iter()
                .map(to_js_json)
                .collect::<Result<_, _>>()?,
        ),
        _ => J::Object(
            value
                .own_entries()?
                .into_iter()
                .map(|(k, v)| Ok((String::from_utf16_lossy(&k), to_js_json(&v)?)))
                .collect::<Result<_, Thrown>>()?,
        ),
    })
}

// SameValue differs from === for NaN and the sign of zero.
fn same_value(a: &Js, b: &Js) -> bool {
    match (a, b) {
        (Js::Number(a), Js::Number(b)) => (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits(),
        _ => a.strict_equals(b),
    }
}
