//! Ordinary-object tables with an unmodified Object.prototype.

/// Identity of a member inherited from the standard Object.prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtoMember {
    Constructor,
    DefineGetter,
    DefineSetter,
    HasOwnProperty,
    LookupGetter,
    LookupSetter,
    IsPrototypeOf,
    PropertyIsEnumerable,
    ToString,
    ValueOf,
    /// The accessor returns Object.prototype itself, not a callable function.
    Prototype,
    ToLocaleString,
}

impl ProtoMember {
    /// The inherited value's .length: function arity, or absent for the prototype object.
    pub fn length(self) -> Option<u8> {
        match self {
            Self::Prototype => None,
            Self::ToString | Self::ValueOf | Self::ToLocaleString => Some(0),
            Self::DefineGetter | Self::DefineSetter => Some(2),
            Self::Constructor
            | Self::HasOwnProperty
            | Self::IsPrototypeOf
            | Self::PropertyIsEnumerable
            | Self::LookupGetter
            | Self::LookupSetter => Some(1),
        }
    }

    /// String(member) on stock V8 builtins; the prototype accessor returns an object.
    pub fn js_string(self) -> &'static str {
        match self {
            Self::Constructor => "function Object() { [native code] }",
            Self::DefineGetter => "function __defineGetter__() { [native code] }",
            Self::DefineSetter => "function __defineSetter__() { [native code] }",
            Self::HasOwnProperty => "function hasOwnProperty() { [native code] }",
            Self::LookupGetter => "function __lookupGetter__() { [native code] }",
            Self::LookupSetter => "function __lookupSetter__() { [native code] }",
            Self::IsPrototypeOf => "function isPrototypeOf() { [native code] }",
            Self::PropertyIsEnumerable => "function propertyIsEnumerable() { [native code] }",
            Self::ToString => "function toString() { [native code] }",
            Self::ValueOf => "function valueOf() { [native code] }",
            Self::Prototype => "[object Object]",
            Self::ToLocaleString => "function toLocaleString() { [native code] }",
        }
    }
}

/// Result of a protocol-table lookup, preserving inherited values without widening T.
/// Presence, truthiness and nullishness are distinct queries.
/// Inherited records identity, not function execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableHit<T> {
    Mapped(T),
    Inherited(ProtoMember),
    Missing,
}

impl<T> TableHit<T> {
    /// Whether lookup found an own or inherited value, even null or undefined.
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Missing)
    }

    /// JavaScript Boolean(result), using the caller's mapped-value coercion.
    pub fn is_truthy_by(&self, mapped: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Self::Mapped(value) => mapped(value),
            Self::Inherited(_) => true,
            Self::Missing => false,
        }
    }

    /// Whether ?? takes its fallback, using the caller's null/undefined predicate.
    pub fn is_nullish_by(&self, mapped: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Self::Mapped(value) => mapped(value),
            Self::Inherited(_) => false,
            Self::Missing => true,
        }
    }
}

impl<T: std::borrow::Borrow<serde_json::Value>> TableHit<T> {
    /// JavaScript Boolean(result), delegated to the shared JSON truthiness primitive.
    pub fn is_truthy(&self) -> bool {
        self.is_truthy_by(|value| crate::js_truthy(Some(value.borrow())))
    }

    /// Whether ?? takes its fallback; an own null is present but nullish.
    pub fn is_nullish(&self) -> bool {
        self.is_nullish_by(|value| value.borrow().is_null())
    }
}

/// Look up own entries first, then the standard Object.prototype members.
/// Entries denote own data properties (including an own __proto__), last duplicate wins.
/// Mutated/custom/null prototypes are outside this table contract.
pub fn js_table_lookup<'a, K: AsRef<str>, T>(
    table: &'a [(K, T)],
    wire_key: &str,
) -> TableHit<&'a T> {
    if let Some((_, value)) = table.iter().rev().find(|(key, _)| key.as_ref() == wire_key) {
        return TableHit::Mapped(value);
    }
    let member = match wire_key {
        "constructor" => ProtoMember::Constructor,
        "__defineGetter__" => ProtoMember::DefineGetter,
        "__defineSetter__" => ProtoMember::DefineSetter,
        "hasOwnProperty" => ProtoMember::HasOwnProperty,
        "__lookupGetter__" => ProtoMember::LookupGetter,
        "__lookupSetter__" => ProtoMember::LookupSetter,
        "isPrototypeOf" => ProtoMember::IsPrototypeOf,
        "propertyIsEnumerable" => ProtoMember::PropertyIsEnumerable,
        "toString" => ProtoMember::ToString,
        "valueOf" => ProtoMember::ValueOf,
        "__proto__" => ProtoMember::Prototype,
        "toLocaleString" => ProtoMember::ToLocaleString,
        _ => return TableHit::Missing,
    };
    TableHit::Inherited(member)
}
