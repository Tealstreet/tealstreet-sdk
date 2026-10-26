//! get-key.ts: presence-based lookup on JSON own data properties and stock Object.prototype.
use crate::{TableHit, js_table_lookup, lodash_camel_case, lodash_snake_case};
use serde_json::{Map, Value};

/// Borrowed object fields. Entries additionally represent own `undefined` values.
#[derive(Debug, Clone, Copy)]
pub enum GetKeyObject<'a> {
    Json(&'a Map<String, Value>),
    Entries(&'a [(String, Option<Value>)]),
}
/// `undefined` is distinct from an own JSON null and from an absent property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GetKeyValue<'a> {
    Undefined,
    Json(&'a Value),
}

impl<'a> GetKeyObject<'a> {
    fn lookup(self, key: &str) -> TableHit<GetKeyValue<'a>> {
        let own = match self {
            Self::Json(fields) => fields.get(key).map(GetKeyValue::Json),
            Self::Entries(fields) => {
                fields
                    .iter()
                    .rev()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| {
                        value
                            .as_ref()
                            .map_or(GetKeyValue::Undefined, GetKeyValue::Json)
                    })
            }
        };
        if let Some(value) = own {
            return TableHit::Mapped(value);
        }
        // Delegate the prototype chain to the shared operation; never a local name table.
        match js_table_lookup::<&str, GetKeyValue<'a>>(&[], key) {
            TableHit::Mapped(value) => TableHit::Mapped(*value),
            TableHit::Inherited(member) => TableHit::Inherited(member),
            TableHit::Missing => TableHit::Missing,
        }
    }
}

// Memoization is omitted: both dependency functions are deterministic; private caches affect timing only.
pub use crate::lodash_camel_case as c;
pub use crate::lodash_snake_case as s;

/// Exact, snake then camel lookup. An own undefined hit stops this search too.
pub fn v<'a>(object: Option<GetKeyObject<'a>>, key: &str) -> TableHit<GetKeyValue<'a>> {
    let Some(object) = object else {
        return TableHit::Missing;
    };
    let exact = object.lookup(key);
    if exact.is_present() {
        return exact;
    }
    let snake = object.lookup(&lodash_snake_case(key));
    if snake.is_present() {
        return snake;
    }
    object.lookup(&lodash_camel_case(key))
}

/// Try each alias until a value is not undefined. Null, false, zero and empty string stop.
pub fn vn<'a>(object: Option<GetKeyObject<'a>>, keys: &[&str]) -> TableHit<GetKeyValue<'a>> {
    for key in keys {
        match v(object, key) {
            TableHit::Missing | TableHit::Mapped(GetKeyValue::Undefined) => {}
            hit => return hit,
        }
    }
    TableHit::Missing
}

/// Identity of a stock Array.prototype callable, distinct from Object.prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrayMember(&'static str);
impl ArrayMember {
    /// Property name on the stock array prototype.
    pub fn name(self) -> &'static str {
        self.0
    }
    /// V8 native function interpolation, retaining the Array constructor identity.
    pub fn js_string(self) -> String {
        let name = if self.0 == "constructor" {
            "Array"
        } else {
            self.0
        };
        format!("function {name}() {{ [native code] }}")
    }
}

/// Result for unknown JSON inputs; arrays have computed length and a distinct prototype.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GetKeyUnknownValue<'a> {
    Undefined,
    Json(&'a Value),
    Length(usize),
    ObjectInherited(crate::ProtoMember),
    ArrayInherited(ArrayMember),
    ArrayPrototype,
}
impl GetKeyUnknownValue<'_> {
    /// Source Boolean conversion for guards; null is present but false.
    pub fn is_truthy(self) -> bool {
        match self {
            Self::Undefined => false,
            Self::Json(value) => crate::js_truthy(Some(value)),
            Self::Length(length) => length != 0,
            Self::ObjectInherited(_) | Self::ArrayInherited(_) | Self::ArrayPrototype => true,
        }
    }
}

/// Dynamic V8 in-operator error, including the exact key and primitive RHS text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetKeyTypeError {
    pub name: &'static str,
    pub message: String,
}
impl std::fmt::Display for GetKeyTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}
impl std::error::Error for GetKeyTypeError {}

const ARRAY_METHODS: &[&str] = &[
    "constructor",
    "at",
    "concat",
    "copyWithin",
    "fill",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "lastIndexOf",
    "pop",
    "push",
    "reverse",
    "shift",
    "unshift",
    "slice",
    "sort",
    "splice",
    "includes",
    "indexOf",
    "join",
    "keys",
    "entries",
    "values",
    "forEach",
    "filter",
    "flat",
    "flatMap",
    "map",
    "every",
    "some",
    "reduce",
    "reduceRight",
    "toReversed",
    "toSorted",
    "toSpliced",
    "with",
    "toLocaleString",
    "toString",
];

fn unknown_object_hit(hit: TableHit<GetKeyValue<'_>>) -> GetKeyUnknownValue<'_> {
    match hit {
        TableHit::Missing | TableHit::Mapped(GetKeyValue::Undefined) => {
            GetKeyUnknownValue::Undefined
        }
        TableHit::Mapped(GetKeyValue::Json(value)) => GetKeyUnknownValue::Json(value),
        TableHit::Inherited(member) => GetKeyUnknownValue::ObjectInherited(member),
    }
}
/// Exact property lookup on a dense JSON array with the stock prototype.
/// Unlike v_unknown this does not retry snake/camel aliases.
pub fn array_lookup<'a>(array: &'a [Value], key: &str) -> GetKeyUnknownValue<'a> {
    if key == "length" {
        return GetKeyUnknownValue::Length(array.len());
    }
    if let Some(value) = crate::js_array_index(key).and_then(|index| array.get(index as usize)) {
        return GetKeyUnknownValue::Json(value);
    }
    if key == "__proto__" {
        return GetKeyUnknownValue::ArrayPrototype;
    }
    if let Some(name) = ARRAY_METHODS.iter().find(|name| **name == key) {
        return GetKeyUnknownValue::ArrayInherited(ArrayMember(name));
    }
    unknown_object_hit(GetKeyObject::Entries(&[]).lookup(key))
}

/// Actual get-key on unknown JSON data. None/null short-circuit; primitives throw.
/// Arrays are dense JSON arrays with stock prototypes, without custom properties.
pub fn v_unknown<'a>(
    value: Option<&'a Value>,
    key: &str,
) -> Result<GetKeyUnknownValue<'a>, GetKeyTypeError> {
    match value {
        None | Some(Value::Null) => Ok(GetKeyUnknownValue::Undefined),
        Some(Value::Object(fields)) => {
            Ok(unknown_object_hit(v(Some(GetKeyObject::Json(fields)), key)))
        }
        Some(Value::Array(array)) => {
            for spelling in [
                key.to_owned(),
                lodash_snake_case(key),
                lodash_camel_case(key),
            ] {
                let hit = array_lookup(array, &spelling);
                if hit != GetKeyUnknownValue::Undefined {
                    return Ok(hit);
                }
            }
            Ok(GetKeyUnknownValue::Undefined)
        }
        Some(Value::Bool(value)) => Err(primitive_rhs_error(key, value)),
        Some(Value::String(value)) => Err(primitive_rhs_error(key, value)),
        Some(Value::Number(value)) => Err(primitive_rhs_error(
            key,
            crate::js_number_string(crate::number_from_string(&value.to_string())),
        )),
    }
}

/// Alias walk over unknown JSON data. An empty alias list never inspects the RHS.
pub fn vn_unknown<'a>(
    value: Option<&'a Value>,
    keys: &[&str],
) -> Result<GetKeyUnknownValue<'a>, GetKeyTypeError> {
    for key in keys {
        let hit = v_unknown(value, key)?;
        if hit != GetKeyUnknownValue::Undefined {
            return Ok(hit);
        }
    }
    Ok(GetKeyUnknownValue::Undefined)
}

fn primitive_rhs_error(key: &str, rhs: impl std::fmt::Display) -> GetKeyTypeError {
    GetKeyTypeError {
        name: "TypeError",
        message: format!("Cannot use 'in' operator to search for '{key}' in {rhs}"),
    }
}
