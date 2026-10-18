//! Mutable builtin referents shared by ordinary objects within one runtime realm.
//! This models property access used by the socket statistics and table lookups;
//! it does not execute arbitrary JavaScript functions.
use crate::{ProtoMember, TableHit, js_table_lookup};
use indexmap::IndexMap;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone)]
pub enum PrototypeValue {
    Undefined,
    Null,
    String(String),
    Number(f64),
    Boolean(bool),
    Reference(BuiltinReference),
}
impl PrototypeValue {
    pub fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::String(value) => !value.is_empty(),
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::Boolean(value) => *value,
            Self::Reference(_) => true,
        }
    }
    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Undefined | Self::Null)
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PrototypeScalar {
    #[default]
    Undefined,
    Null,
    String(String),
    Number(f64),
    Boolean(bool),
}
impl serde::Serialize for PrototypeScalar {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Undefined | Self::Null => serializer.serialize_unit(),
            Self::String(value) => serializer.serialize_str(value),
            Self::Boolean(value) => serializer.serialize_bool(*value),
            Self::Number(value) if value.is_finite() => {
                // JSON views use JavaScript's numeric spelling; native consumers
                // keep the original f64. Containing objects omit Undefined.
                let number: serde_json::Number =
                    serde_json::from_str(&crate::js_number_string(*value))
                        .map_err(serde::ser::Error::custom)?;
                serde::Serialize::serialize(&number, serializer)
            }
            Self::Number(_) => serializer.serialize_unit(),
        }
    }
}
impl From<Option<usize>> for PrototypeScalar {
    fn from(value: Option<usize>) -> Self {
        value.map_or(Self::Undefined, |value| Self::Number(value as f64))
    }
}
impl From<PrototypeScalar> for PrototypeValue {
    fn from(value: PrototypeScalar) -> Self {
        match value {
            PrototypeScalar::Undefined => Self::Undefined,
            PrototypeScalar::Null => Self::Null,
            PrototypeScalar::String(v) => Self::String(v),
            PrototypeScalar::Number(v) => Self::Number(v),
            PrototypeScalar::Boolean(v) => Self::Boolean(v),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinIdentity {
    Object(ProtoMember),
    FunctionPrototype,
    FunctionConstructor,
    FunctionMethod(FunctionMethod),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FunctionMethod {
    ToString,
    Apply,
    Bind,
    Call,
}
impl FunctionMethod {
    pub fn name(self) -> &'static str {
        match self {
            Self::ToString => "toString",
            Self::Apply => "apply",
            Self::Bind => "bind",
            Self::Call => "call",
        }
    }
    pub fn length(self) -> u8 {
        match self {
            Self::ToString => 0,
            Self::Apply => 2,
            Self::Bind | Self::Call => 1,
        }
    }
}
#[derive(Debug, Default)]
pub struct PrototypeRealm {
    // Statistics write primitive fields only. Keeping primitive storage also
    // prevents the realm from retaining itself through one of its referents.
    fields: RefCell<IndexMap<BuiltinIdentity, IndexMap<String, PrototypeScalar>>>,
}
#[derive(Debug, Clone)]
pub struct BuiltinReference {
    realm: Rc<PrototypeRealm>,
    member: BuiltinIdentity,
}
impl PartialEq for BuiltinReference {
    fn eq(&self, other: &Self) -> bool {
        self.member == other.member && Rc::ptr_eq(&self.realm, &other.realm)
    }
}
impl Eq for BuiltinReference {}
impl BuiltinReference {
    pub fn is_callable(&self) -> bool {
        self.member != BuiltinIdentity::Object(ProtoMember::Prototype)
    }
    pub fn js_string(&self) -> String {
        match self.member {
            BuiltinIdentity::Object(member) => member.js_string().into(),
            BuiltinIdentity::FunctionConstructor => "function Function() { [native code] }".into(),
            BuiltinIdentity::FunctionPrototype => "function () { [native code] }".into(),
            BuiltinIdentity::FunctionMethod(method) => {
                format!("function {}() {{ [native code] }}", method.name())
            }
        }
    }
    pub fn member(&self) -> BuiltinIdentity {
        self.member
    }
    pub fn realm(&self) -> &Rc<PrototypeRealm> {
        &self.realm
    }
    /// Actual parent identity, independent of shadowable string properties.
    pub fn prototype(&self) -> Option<Self> {
        let member = match self.member {
            BuiltinIdentity::Object(ProtoMember::Prototype) => return None,
            BuiltinIdentity::FunctionPrototype => BuiltinIdentity::Object(ProtoMember::Prototype),
            _ => BuiltinIdentity::FunctionPrototype,
        };
        Some(Self {
            realm: self.realm.clone(),
            member,
        })
    }
    pub fn get(&self, key: &str) -> PrototypeValue {
        self.realm.reference_get(self.member, key)
    }
    pub fn set(&self, key: &str, value: PrototypeScalar) {
        self.realm
            .fields
            .borrow_mut()
            .entry(self.member)
            .or_default()
            .insert(key.into(), value);
    }
    pub fn own_properties(&self) -> Vec<(String, bool)> {
        let stock: &[&str] = match self.member {
            BuiltinIdentity::Object(ProtoMember::Prototype) => &[
                "constructor",
                "__defineGetter__",
                "__defineSetter__",
                "hasOwnProperty",
                "__lookupGetter__",
                "__lookupSetter__",
                "isPrototypeOf",
                "propertyIsEnumerable",
                "toString",
                "valueOf",
                "__proto__",
                "toLocaleString",
            ],
            BuiltinIdentity::Object(ProtoMember::Constructor) => &[
                "length",
                "name",
                "prototype",
                "assign",
                "getOwnPropertyDescriptor",
                "getOwnPropertyDescriptors",
                "getOwnPropertyNames",
                "getOwnPropertySymbols",
                "hasOwn",
                "is",
                "preventExtensions",
                "seal",
                "create",
                "defineProperties",
                "defineProperty",
                "freeze",
                "getPrototypeOf",
                "setPrototypeOf",
                "isExtensible",
                "isFrozen",
                "isSealed",
                "keys",
                "entries",
                "fromEntries",
                "values",
                "groupBy",
            ],
            BuiltinIdentity::FunctionConstructor => &["length", "name", "prototype"],
            BuiltinIdentity::FunctionPrototype => &[
                "length",
                "name",
                "arguments",
                "caller",
                "constructor",
                "apply",
                "bind",
                "call",
                "toString",
            ],
            _ => &["length", "name"],
        };
        let fields = self.own_fields();
        let keys: Vec<&str> = stock
            .iter()
            .copied()
            .chain(
                fields
                    .keys()
                    .map(String::as_str)
                    .filter(|key| !stock.contains(key)),
            )
            .collect();
        crate::js_own_keys(keys)
            .into_iter()
            .map(|key| (key.to_owned(), !stock.contains(&key)))
            .collect()
    }
    pub fn own_fields(&self) -> IndexMap<String, PrototypeScalar> {
        self.realm
            .fields
            .borrow()
            .get(&self.member)
            .cloned()
            .unwrap_or_default()
    }
}
fn default_realm_slot<T>(f: impl FnOnce(&RefCell<Rc<PrototypeRealm>>) -> T) -> T {
    // Function-local static storage is still shared by every accessor on this thread.
    thread_local! {
        static DEFAULT_REALM: RefCell<Rc<PrototypeRealm>> = RefCell::new(Rc::default());
    }
    DEFAULT_REALM.with(f)
}
/// Lexical host binding; saved values and sockets retain their original realm.
/// Bindings must be nested in stack order and stay on this thread.
pub struct PrototypeRealmBinding {
    previous: Rc<PrototypeRealm>,
}
impl Drop for PrototypeRealmBinding {
    fn drop(&mut self) {
        default_realm_slot(|current| *current.borrow_mut() = self.previous.clone());
    }
}
impl PrototypeRealm {
    pub fn current() -> Rc<Self> {
        default_realm_slot(|current| current.borrow().clone())
    }
    pub fn bind(self: &Rc<Self>) -> PrototypeRealmBinding {
        let previous = default_realm_slot(|current| current.replace(self.clone()));
        PrototypeRealmBinding { previous }
    }
    pub fn reference(self: &Rc<Self>, member: ProtoMember) -> BuiltinReference {
        BuiltinReference {
            realm: self.clone(),
            member: BuiltinIdentity::Object(member),
        }
    }
    /// Read through an ordinary empty object's Object.prototype.
    pub fn inherited(self: &Rc<Self>, key: &str) -> PrototypeValue {
        if let Some(value) = self
            .fields
            .borrow()
            .get(&BuiltinIdentity::Object(ProtoMember::Prototype))
            .and_then(|v| v.get(key))
            .cloned()
        {
            return value.into();
        }
        match js_table_lookup::<&str, ()>(&[], key) {
            TableHit::Inherited(member) => PrototypeValue::Reference(self.reference(member)),
            _ => PrototypeValue::Undefined,
        }
    }
    pub fn enumerable_keys(&self) -> Vec<String> {
        let fields = self.fields.borrow();
        fields
            .get(&BuiltinIdentity::Object(ProtoMember::Prototype))
            .map(|fields| {
                crate::js_own_keys(fields.keys().map(String::as_str))
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
    fn reference_get(self: &Rc<Self>, member: BuiltinIdentity, key: &str) -> PrototypeValue {
        if let Some(value) = self
            .fields
            .borrow()
            .get(&member)
            .and_then(|fields| fields.get(key))
            .cloned()
        {
            return value.into();
        }
        match member {
            BuiltinIdentity::Object(ProtoMember::Prototype) => {
                if key == "__proto__" {
                    return PrototypeValue::Null;
                }
            }
            BuiltinIdentity::Object(value) => {
                if key == "constructor" {
                    return PrototypeValue::Reference(BuiltinReference {
                        realm: self.clone(),
                        member: BuiltinIdentity::FunctionConstructor,
                    });
                }
                if key == "length" {
                    return value
                        .length()
                        .map(|n| PrototypeValue::Number(f64::from(n)))
                        .unwrap_or(PrototypeValue::Undefined);
                }
                if key == "name" {
                    return PrototypeValue::String(
                        value
                            .js_string()
                            .trim_start_matches("function ")
                            .split('(')
                            .next()
                            .unwrap_or("")
                            .into(),
                    );
                }
                if key == "__proto__" {
                    return PrototypeValue::Reference(BuiltinReference {
                        realm: self.clone(),
                        member: BuiltinIdentity::FunctionPrototype,
                    });
                }
                if value == ProtoMember::Constructor && key == "prototype" {
                    return PrototypeValue::Reference(self.reference(ProtoMember::Prototype));
                }
            }
            BuiltinIdentity::FunctionMethod(method) => {
                if key == "length" {
                    return PrototypeValue::Number(f64::from(method.length()));
                }
                if key == "name" {
                    return PrototypeValue::String(method.name().into());
                }
            }
            BuiltinIdentity::FunctionConstructor => {
                if key == "length" {
                    return PrototypeValue::Number(1.0);
                }
                if key == "name" {
                    return PrototypeValue::String("Function".into());
                }
                if key == "prototype" || key == "__proto__" {
                    return PrototypeValue::Reference(BuiltinReference {
                        realm: self.clone(),
                        member: BuiltinIdentity::FunctionPrototype,
                    });
                }
            }
            BuiltinIdentity::FunctionPrototype => {
                if key == "length" {
                    return PrototypeValue::Number(0.0);
                }
                if key == "name" {
                    return PrototypeValue::String(String::new());
                }
                if key == "constructor" {
                    return PrototypeValue::Reference(BuiltinReference {
                        realm: self.clone(),
                        member: BuiltinIdentity::FunctionConstructor,
                    });
                }
                if key == "__proto__" {
                    return PrototypeValue::Reference(self.reference(ProtoMember::Prototype));
                }
            }
        }
        if member != BuiltinIdentity::Object(ProtoMember::Prototype) {
            let function_member = match key {
                "toString" => Some(FunctionMethod::ToString),
                "apply" => Some(FunctionMethod::Apply),
                "bind" => Some(FunctionMethod::Bind),
                "call" => Some(FunctionMethod::Call),
                _ => None,
            };
            if let Some(method) = function_member {
                return PrototypeValue::Reference(BuiltinReference {
                    realm: self.clone(),
                    member: BuiltinIdentity::FunctionMethod(method),
                });
            }
            if key == "constructor" {
                return PrototypeValue::Reference(BuiltinReference {
                    realm: self.clone(),
                    member: BuiltinIdentity::FunctionConstructor,
                });
            }
            if key == "__proto__" {
                return PrototypeValue::Reference(BuiltinReference {
                    realm: self.clone(),
                    member: BuiltinIdentity::FunctionPrototype,
                });
            }
        }
        self.inherited(key)
    }
    pub fn get(
        self: &Rc<Self>,
        value: &PrototypeValue,
        key: &str,
    ) -> Result<PrototypeValue, String> {
        match value {
            PrototypeValue::Null => {
                Err(format!("Cannot read properties of null (reading '{key}')"))
            }
            PrototypeValue::Undefined => Err(format!(
                "Cannot read properties of undefined (reading '{key}')"
            )),
            PrototypeValue::Reference(reference) => Ok(reference.get(key)),
            PrototypeValue::String(value) if key == "length" => {
                Ok(PrototypeValue::Number(value.encode_utf16().count() as f64))
            }
            PrototypeValue::String(_) | PrototypeValue::Number(_) | PrototypeValue::Boolean(_) => {
                Ok(self.inherited(key))
            }
        }
    }
    pub fn set(
        &self,
        value: &PrototypeValue,
        key: &str,
        new_value: PrototypeScalar,
    ) -> Result<(), String> {
        match value {
            PrototypeValue::Reference(reference) => {
                reference.set(key, new_value);
                Ok(())
            }
            PrototypeValue::Null => Err(format!("Cannot set properties of null (setting '{key}')")),
            PrototypeValue::Undefined => Err(format!(
                "Cannot set properties of undefined (setting '{key}')"
            )),
            PrototypeValue::String(v) => {
                Err(format!("Cannot create property '{key}' on string '{v}'"))
            }
            PrototypeValue::Number(n) => Err(format!(
                "Cannot create property '{key}' on number '{}'",
                crate::js_number_string(*n)
            )),
            PrototypeValue::Boolean(v) => {
                Err(format!("Cannot create property '{key}' on boolean '{v}'"))
            }
        }
    }
}

/// Reenter the originating runtime realm for each poll, and restore the caller
/// realm before returning Pending. A suspended future never holds a binding.
pub struct PrototypeRealmFuture<'a, T> {
    realm: Rc<PrototypeRealm>,
    future: std::pin::Pin<Box<dyn std::future::Future<Output = T> + 'a>>,
}
impl PrototypeRealm {
    pub fn future<'a, T>(
        self: &Rc<Self>,
        future: impl std::future::Future<Output = T> + 'a,
    ) -> PrototypeRealmFuture<'a, T> {
        PrototypeRealmFuture {
            realm: self.clone(),
            future: Box::pin(future),
        }
    }
}
impl<T> std::future::Future for PrototypeRealmFuture<'_, T> {
    type Output = T;
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<T> {
        let this = self.get_mut();
        let _binding = this.realm.bind();
        this.future.as_mut().poll(cx)
    }
}
