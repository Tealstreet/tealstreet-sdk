use serde::Deserializer;
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, Visitor};
use serde_json::{Map, Value, json};
use std::fmt;

type Error = de::value::Error;
type Result<T> = std::result::Result<T, Error>;

struct Sample<'a> {
    value: &'a mut Value,
    depth: usize,
}
impl Sample<'_> {
    fn nested<'a>(&self, value: &'a mut Value) -> Result<Sample<'a>> {
        if self.depth > 32 {
            return Err(de::Error::custom(
                "recursive type requires an explicit fixture",
            ));
        }
        Ok(Sample {
            value,
            depth: self.depth + 1,
        })
    }
}
struct Fields<'a> {
    fields: std::slice::Iter<'static, &'static str>,
    key: Option<&'static str>,
    sample: Sample<'a>,
}
impl<'de> MapAccess<'de> for Fields<'_> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        self.key = self.fields.next().copied();
        self.key
            .map(|key| seed.deserialize(de::value::StrDeserializer::<Error>::new(key)))
            .transpose()
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let mut value = Value::Null;
        let result = seed.deserialize(self.sample.nested(&mut value)?)?;
        self.sample
            .value
            .as_object_mut()
            .unwrap()
            .insert(self.key.unwrap().into(), value);
        Ok(result)
    }
}
macro_rules! number {
    ($method:ident, $visit:ident, $ty:ty) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            *self.value = json!(1);
            visitor.$visit(1 as $ty)
        }
    };
}
impl<'de> Deserializer<'de> for Sample<'_> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = Value::Null;
        visitor.visit_unit()
    }
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = json!(false);
        visitor.visit_bool(false)
    }
    number!(deserialize_i8, visit_i8, i8);
    number!(deserialize_i16, visit_i16, i16);
    number!(deserialize_i32, visit_i32, i32);
    number!(deserialize_i64, visit_i64, i64);
    number!(deserialize_u8, visit_u8, u8);
    number!(deserialize_u16, visit_u16, u16);
    number!(deserialize_u32, visit_u32, u32);
    number!(deserialize_u64, visit_u64, u64);
    number!(deserialize_f32, visit_f32, f32);
    number!(deserialize_f64, visit_f64, f64);
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = json!("sample");
        visitor.visit_string("sample".into())
    }
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = json!("sample");
        visitor.visit_borrowed_str("sample")
    }
    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = json!("x");
        visitor.visit_char('x')
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = Value::Null;
        visitor.visit_none()
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = json!([]);
        visitor.visit_seq(de::value::SeqDeserializer::<_, Error>::new(
            std::iter::empty::<String>(),
        ))
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        *self.value = json!({});
        visitor.visit_map(Fields {
            fields: [].iter(),
            key: None,
            sample: self,
        })
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        *self.value = json!({});
        visitor.visit_map(Fields {
            fields: fields.iter(),
            key: None,
            sample: self,
        })
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        let variant = variants
            .first()
            .ok_or_else(|| de::Error::custom("empty enum"))?;
        *self.value = json!(variant);
        visitor.visit_enum(de::value::StrDeserializer::<Error>::new(variant))
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }
    serde::forward_to_deserialize_any! { bytes byte_buf unit unit_struct tuple tuple_struct identifier ignored_any }
}

#[derive(Debug)]
struct InventoryError(Option<&'static [&'static str]>);
impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("inventory")
    }
}
impl std::error::Error for InventoryError {}
impl de::Error for InventoryError {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self(None)
    }
    fn unknown_variant(_: &str, variants: &'static [&'static str]) -> Self {
        Self(Some(variants))
    }
}
struct EnumInventory;
impl<'de> Deserializer<'de> for EnumInventory {
    type Error = InventoryError;
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        _: V,
    ) -> std::result::Result<V::Value, Self::Error> {
        Err(InventoryError(Some(variants)))
    }
    fn deserialize_any<V: Visitor<'de>>(
        self,
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error> {
        visitor.visit_string("__ACCEPTED_SET_UNKNOWN__".into())
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any }
}

struct StructInventory;
impl<'de> Deserializer<'de> for StructInventory {
    type Error = InventoryError;
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _: &'static str,
        fields: &'static [&'static str],
        _: V,
    ) -> std::result::Result<V::Value, Self::Error> {
        Err(InventoryError(Some(fields)))
    }
    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> std::result::Result<V::Value, Self::Error> {
        Err(InventoryError(None))
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any }
}

fn check_adapters(
    contract: &Value,
    alternate: &impl Fn(&Value) -> Vec<(&'static str, bool)>,
    failures: &mut Vec<String>,
) {
    if let Some(entries) = contract["entrypoints"].as_array() {
        for (index, entry) in entries
            .iter()
            .filter(|entry| {
                !matches!(
                    entry["input"].as_str(),
                    Some("typed" | "observed" | "non-decoder")
                )
            })
            .enumerate()
        {
            for positive in entry["positive"].as_array().unwrap() {
                let label = entry["trait"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("entrypoint {index}"));
                if !alternate(positive)
                    .iter()
                    .any(|(name, accepted)| *name == label && *accepted)
                {
                    failures.push(format!(
                        "{label}: positive adapter witness rejected {positive}"
                    ));
                }
            }
            if let (Some(label), Some(negative)) =
                (entry["trait"].as_str(), entry["negative"].as_array())
            {
                for value in negative {
                    if alternate(value)
                        .iter()
                        .any(|(name, accepted)| *name == label && *accepted)
                    {
                        failures.push(format!(
                            "{label}: forbidden adapter witness accepted {value}"
                        ));
                    }
                }
            }
        }
    }
}
// A source/package scope receipt, deliberately independent of a Rust T.
pub fn audit_tooling_alias(name: &str, contract: &str) {
    let contract: Value = serde_json::from_str(contract).expect("tooling exemption JSON");
    assert_eq!(
        name, "safe_cex_parity::Result",
        "unreviewed tooling alias owner"
    );
    assert_eq!(
        contract["category"], "tooling-exemption",
        "tooling scope required"
    );
    assert!(
        contract["exemption"]
            .as_str()
            .is_some_and(|reason| !reason.trim().is_empty())
    );
    emit(
        &json!({"name":name,"classificationOnly":true,"category":"tooling-exemption","exemption":contract["exemption"],"failures":[]}),
    );
}

/// Run all decoded-field expectations before the owner's existing wire audit.
/// On failure emit one receipt before panicking; success leaves the ordinary audit
/// responsible for its single receipt. Do not turn this into a second green receipt.
pub fn audit_native_observations(name: &str, observation: &str, observe: impl Fn(&Value) -> Value) {
    let observation: Value = serde_json::from_str(observation).expect("native observation JSON");
    let mut failures = Vec::new();
    for case in observation["cases"]
        .as_array()
        .expect("native observations")
    {
        let observed = observe(&case["input"]);
        if observed != case["output"] {
            failures.push(format!(
                "native observation mismatch for {}: expected {}, observed {}",
                case["input"], case["output"], observed
            ));
        }
    }
    if !failures.is_empty() {
        emit(&json!({"name":name,"nativeOnly":true,"failures":failures}));
        assert!(
            failures.is_empty(),
            "{name}: accepted-set assertion: {}",
            failures.join("; ")
        );
    }
}

pub fn audit_function(name: &str, contract: &str, observe: impl Fn(&Value) -> Value) {
    let contract: Value = serde_json::from_str(contract).expect("function contract JSON");
    let mut failures = Vec::new();
    let classification_only = contract["category"] == "non-decoder";
    if !classification_only {
        for case in contract["cases"].as_array().expect("function witnesses") {
            let observed = observe(&case["input"]);
            if observed != case["output"] {
                failures.push(format!(
                    "function output mismatch for {}: expected {}, observed {}",
                    case["input"], case["output"], observed
                ));
            }
        }
    }
    emit(
        &json!({"name":name,"classificationOnly":classification_only,"category":contract["category"],"reviewedMethods":contract["reviewedMethods"],"failures":failures}),
    );
    assert!(
        failures.is_empty(),
        "{name}: accepted-set assertion: {}",
        failures.join("; ")
    );
}

pub fn audit_alternate(
    name: &str,
    contract: &str,
    alternate: impl Fn(&Value) -> Vec<(&'static str, bool)>,
) {
    let contract: Value = serde_json::from_str(contract).expect("contract JSON");
    let mut failures = Vec::new();
    check_adapters(&contract, &alternate, &mut failures);
    let mut negatives = contract["negative"].as_array().cloned().unwrap_or_default();
    if contract["shape"] == "string" {
        negatives.extend([
            json!(null),
            json!(true),
            json!(false),
            json!(0),
            json!(1),
            json!([]),
            json!({}),
            json!(""),
            json!("__ACCEPTED_SET_UNKNOWN__"),
        ]);
        let literals = contract["literals"].as_array().unwrap();
        for literal in literals {
            let spelling = literal.as_str().unwrap();
            negatives.push(json!({spelling:null}));
            for changed in [
                spelling.to_uppercase(),
                spelling.to_lowercase(),
                format!(" {spelling}"),
                format!("{spelling} "),
                format!("\u{feff}{spelling}"),
                format!("{spelling}\u{feff}"),
                format!("\u{85}{spelling}"),
            ] {
                if !literals.contains(&json!(changed)) {
                    negatives.push(json!(changed));
                }
            }
        }
    }
    {
        let negatives = &negatives;
        for value in negatives {
            for (path, accepted) in alternate(value) {
                if accepted {
                    failures.push(format!("{path} accepted {value}"));
                }
            }
        }
    }
    let classification_only = contract["entrypoints"].as_array().is_some_and(|entries| {
        entries
            .iter()
            .all(|entry| matches!(entry["input"].as_str(), Some("typed" | "non-decoder")))
    });
    emit(
        &json!({"name":name,"alternateOnly":true,"classificationOnly":classification_only,"category":if classification_only { "reviewed-api" } else { "alternate-decoder" },"failures":failures}),
    );
    assert!(
        failures.is_empty(),
        "{name}: accepted-set assertion: {}",
        failures.join("; ")
    );
}

pub fn audit<T: DeserializeOwned>(
    name: &str,
    kind: &str,
    contract: Option<&str>,
    qualified: impl Fn(Value) -> bool,
    alternate: impl Fn(&Value) -> Vec<(&'static str, bool)>,
    observe: impl Fn(&Value) -> Value,
) {
    audit_with_identity::<T>(
        name,
        name,
        kind,
        contract,
        qualified,
        alternate,
        observe,
        Vec::new(),
    );
}

fn audit_with_identity<T: DeserializeOwned>(
    name: &str,
    canonical: &str,
    kind: &str,
    contract: Option<&str>,
    qualified: impl Fn(Value) -> bool,
    alternate: impl Fn(&Value) -> Vec<(&'static str, bool)>,
    observe: impl Fn(&Value) -> Value,
    native_failures: Vec<String>,
) {
    let contract: Option<Value> =
        contract.map(|text| serde_json::from_str(text).expect("contract JSON"));
    let mut failures = native_failures;
    let identity = std::any::type_name::<T>();
    if identity.split('<').next() != Some(canonical) {
        failures.push(format!("compiler owner identity mismatch: {identity}"));
    }
    if let Some(observation) = contract.as_ref().and_then(|value| value.get("observation")) {
        for case in observation["cases"]
            .as_array()
            .expect("Deserialize output witnesses")
        {
            let observed = observe(&case["input"]);
            if observed != case["output"] {
                failures.push(format!(
                    "Deserialize output mismatch for {}: expected {}, observed {}",
                    case["input"], case["output"], observed
                ));
            }
        }
    }
    if kind == "alias" {
        let actual: std::collections::BTreeSet<_> = alternate(&Value::Null)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let expected: std::collections::BTreeSet<_> = contract.as_ref().unwrap()["entrypoints"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| entry["trait"].as_str().unwrap())
            .collect();
        if actual != expected {
            failures.push(format!("unclassified or missing alias alternate traits: actual {actual:?}, expected {expected:?}"));
        }
    }
    let mut sample = Value::Null;
    let generated = T::deserialize(Sample {
        value: &mut sample,
        depth: 0,
    });
    if let Some(fixture) = contract.as_ref().and_then(|value| value.get("sample")) {
        sample = fixture.clone();
    }
    let variants = T::deserialize(EnumInventory)
        .err()
        .and_then(|error| error.0);
    let mut negatives = vec![
        json!(null),
        json!(true),
        json!(false),
        json!(0),
        json!(1),
        json!([]),
    ];
    let shape = contract.as_ref().and_then(|value| value["shape"].as_str());
    if shape == Some("uninhabited") {
        negatives.extend([json!({}), json!("sample"), json!([1]), json!({"sample":1})]);
    } else if shape == Some("json") {
        // This is an explicit open JSON contract, with a fixed shape floor.
        // It makes no claim that JSON has a forbidden JSON-shaped value.
        for positive in [
            json!(null),
            json!(false),
            json!(7),
            json!("text"),
            json!([]),
            json!({}),
            json!([1, {"a":true}]),
            json!({"a":[null,2]}),
        ] {
            if serde_json::from_value::<T>(positive.clone()).is_err()
                || !qualified(positive.clone())
            {
                failures.push(format!("open JSON fixture rejected: {positive}"));
            }
        }
        negatives.clear();
    } else if shape == Some("union") {
        let value = contract.as_ref().unwrap();
        for positive in value["positive"].as_array().unwrap() {
            if serde_json::from_value::<T>(positive.clone()).is_err()
                || !qualified(positive.clone())
            {
                failures.push(format!("positive fixture rejected: {positive}"));
            }
        }
        negatives = value["negative"].as_array().unwrap().clone();
    } else if shape == Some("string") || (shape.is_none() && variants.is_some()) {
        let expected: Vec<&str> = if let Some(value) = &contract {
            value["literals"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect()
        } else {
            variants.unwrap().to_vec()
        };
        if let Some(actual) = variants {
            let actual: std::collections::BTreeSet<_> = actual.iter().copied().collect();
            let expected: std::collections::BTreeSet<_> = expected.iter().copied().collect();
            if actual != expected {
                failures.push(format!(
                    "accepted variant metadata differs: {actual:?} != {expected:?}"
                ));
            }
        } else {
            failures.push("missing accepted variant metadata".into());
        }
        let variants = expected;
        for spelling in &variants {
            let valid = json!(spelling);
            if serde_json::from_value::<T>(valid.clone()).is_err() || !qualified(valid) {
                failures.push(format!("advertised spelling rejected: {spelling}"));
            }
            let mut tagged = Map::new();
            tagged.insert((*spelling).into(), Value::Null);
            negatives.push(Value::Object(tagged));
            for value in [
                spelling.to_uppercase(),
                spelling.to_lowercase(),
                format!(" {spelling}"),
                format!("{spelling} "),
                format!("\u{feff}{spelling}"),
                format!("{spelling}\u{feff}"),
                format!("\u{85}{spelling}"),
            ] {
                if !variants.contains(&value.as_str()) {
                    negatives.push(json!(value));
                }
            }
        }
        negatives.extend([json!({}), json!(""), json!("__ACCEPTED_SET_UNKNOWN__")]);
    } else if (shape == Some("map") || kind == "record")
        && sample.is_object()
        && (generated.is_ok()
            || contract
                .as_ref()
                .is_some_and(|value| value.get("sample").is_some()))
    {
        if serde_json::from_value::<T>(sample.clone()).is_err() || !qualified(sample.clone()) {
            failures.push("positive record sample is not accepted JSON".into());
        }
        let fields = T::deserialize(StructInventory)
            .err()
            .and_then(|error| error.0);
        if let Some(fields) = fields {
            let missing: Vec<_> = fields
                .iter()
                .filter(|field| {
                    sample.get(**field).is_none()
                        && !contract
                            .as_ref()
                            .and_then(|c| c["uninhabitedFields"].as_array())
                            .is_some_and(|names| {
                                names.iter().any(|name| name.as_str() == Some(**field))
                            })
                })
                .collect();
            if !missing.is_empty() {
                failures.push(format!(
                    "positive record sample missing positional fields: {missing:?}"
                ));
            }
            negatives.push(Value::Array(
                fields
                    .iter()
                    .map(|field| sample.get(*field).cloned().unwrap_or(Value::Null))
                    .collect(),
            ));
        } else {
            negatives.push(Value::Array(
                sample.as_object().unwrap().values().cloned().collect(),
            ));
        }
        negatives.push(json!("sample"));
    } else {
        failures.push(format!(
            "explicit positive fixture required: {}",
            generated.err().map(|e| e.to_string()).unwrap_or_default()
        ));
        negatives.clear();
    }
    if let Some(contract) = &contract {
        check_adapters(contract, &alternate, &mut failures);
    }
    for value in negatives {
        for (path, accepted) in alternate(&value) {
            if accepted {
                failures.push(format!("{path} accepted {value}"));
            }
        }
        if serde_json::from_value::<T>(value.clone()).is_ok() {
            failures.push(format!("Deserialize accepted {value}"));
        }
        if qualified(value.clone()) {
            failures.push(format!("qualified decoder accepted {value}"));
        }
    }
    emit(
        &json!({"name":name,"sample":sample,"variants":variants,"monomorphization":contract.as_ref().and_then(|value| value.get("monomorphization")),"failures":failures}),
    );
    assert!(
        failures.is_empty(),
        "{name}: accepted-set assertion: {}",
        failures.join("; ")
    );
}

fn emit(value: &Value) {
    #[cfg(not(target_arch = "wasm32"))]
    println!("ACCEPTED_SET_RESULT {value}");
    #[cfg(target_arch = "wasm32")]
    {
        #[link(wasm_import_module = "accepted_set")]
        unsafe extern "C" {
            fn report(ptr: *const u8, len: usize);
        }
        let text = value.to_string();
        // The synchronous host copies the bytes before the string is dropped.
        unsafe {
            report(text.as_ptr(), text.len());
        }
    }
}

#[allow(dead_code)]
pub fn unclassified(name: &str, problem: &str) {
    emit(&json!({"name":name,"sample":null,"variants":null,"failures":[problem]}));
    panic!("{problem}");
}

pub struct AliasAlternate<T: ?Sized>(pub std::marker::PhantomData<T>);
macro_rules! alias_static_trait {
    ($trait:ident, $method:ident, $bound:path) => {
        pub trait $trait {
            fn $method(self) -> bool;
        }
        impl<T: ?Sized> $trait for &AliasAlternate<T> {
            fn $method(self) -> bool {
                false
            }
        }
        impl<T: $bound> $trait for &&AliasAlternate<T> {
            fn $method(self) -> bool {
                true
            }
        }
    };
}
alias_static_trait!(AliasStaticFrom, has_static_from, From<&'static str>);
alias_static_trait!(AliasStaticTry, has_static_try, TryFrom<&'static str>);
macro_rules! alias_alternate {
    ($trait:ident, $method:ident, $native:ident, ($($bound:tt)+), $call:expr) => {
        pub trait $trait<T: ?Sized> {
            fn $method(self, value: &Value) -> Option<bool>;
            fn $native(self, value: &Value, project: &dyn Fn(&T) -> Value) -> Option<std::result::Result<Value, ()>>;
        }
        impl<T: ?Sized> $trait<T> for &AliasAlternate<T> {
            fn $method(self, _: &Value) -> Option<bool> { None }
            fn $native(self, _: &Value, _: &dyn Fn(&T) -> Value) -> Option<std::result::Result<Value, ()>> { None }
        }
        impl<T> $trait<T> for &&AliasAlternate<T> where T: $($bound)+ {
            fn $method(self, value: &Value) -> Option<bool> { Some(($call)(value).is_some()) }
            fn $native(self, value: &Value, project: &dyn Fn(&T) -> Value) -> Option<std::result::Result<Value, ()>> {
                Some(($call)(value).map(|decoded| project(&decoded)).ok_or(()))
            }
        }
    };
}
alias_alternate!(
    AliasFromStr,
    probe_from_str,
    native_from_str,
    (std::str::FromStr),
    |value: &Value| value
        .as_str()
        .and_then(|input| <T as std::str::FromStr>::from_str(input).ok())
);
alias_alternate!(
    AliasFromString,
    probe_from_string,
    native_from_string,
    (From<String>),
    |value: &Value| value
        .as_str()
        .map(|input| <T as From<String>>::from(input.to_owned()))
);
alias_alternate!(
    AliasFromStrRef,
    probe_from_str_ref,
    native_from_str_ref,
    (for<'a> From<&'a str>),
    |value: &Value| value.as_str().map(|input| <T as From<&str>>::from(input))
);
alias_alternate!(
    AliasFromValue,
    probe_from_value,
    native_from_value,
    (TryFrom<Value>),
    |value: &Value| <T as TryFrom<Value>>::try_from(value.clone()).ok()
);
alias_alternate!(
    AliasFromMap,
    probe_from_map,
    native_from_map,
    (TryFrom<Map<String, Value>>),
    |value: &Value| value
        .as_object()
        .and_then(|input| <T as TryFrom<Map<String, Value>>>::try_from(input.clone()).ok())
);
alias_alternate!(
    AliasTryString,
    probe_try_string,
    native_try_string,
    (TryFrom<String>),
    |value: &Value| value
        .as_str()
        .and_then(|input| <T as TryFrom<String>>::try_from(input.to_owned()).ok())
);
alias_alternate!(
    AliasTryStrRef,
    probe_try_str_ref,
    native_try_str_ref,
    (for<'a> TryFrom<&'a str>),
    |value: &Value| value
        .as_str()
        .and_then(|input| <T as TryFrom<&str>>::try_from(input).ok())
);

fn alias_native_checks(
    contract: &Value,
    native: &impl Fn(&str, &Value) -> Option<std::result::Result<Value, ()>>,
) -> Vec<String> {
    let mut failures = Vec::new();
    for entry in contract["entrypoints"].as_array().into_iter().flatten() {
        let path = entry["trait"].as_str().expect("native adapter trait");
        for case in entry["nativeCases"]
            .as_array()
            .expect("native adapter cases")
        {
            match native(path, &case["input"]) {
                None => failures.push(format!("{path}: native adapter is missing")),
                Some(Err(())) => failures.push(format!(
                    "{path}: native adapter witness rejected {}",
                    case["input"]
                )),
                Some(Ok(observed)) if observed != case["output"] => failures.push(format!(
                    "{path}: native output mismatch for {}: expected {}, observed {}",
                    case["input"], case["output"], observed
                )),
                Some(Ok(_)) => {}
            }
        }
    }
    failures
}

pub struct AliasProbe<T: ?Sized>(pub std::marker::PhantomData<T>);
pub trait DetectAlias {
    fn probe(
        self,
        label: &str,
        canonical: &str,
        contract: &str,
        observe: impl Fn(&Value) -> Value,
        _native: impl Fn(&str, &Value) -> Option<std::result::Result<Value, ()>>,
        alternate: impl Fn(&Value) -> Vec<(&'static str, bool)>,
    );
}
impl<T: ?Sized> DetectAlias for &AliasProbe<T> {
    fn probe(
        self,
        label: &str,
        canonical: &str,
        contract: &str,
        _: impl Fn(&Value) -> Value,
        _native: impl Fn(&str, &Value) -> Option<std::result::Result<Value, ()>>,
        alternate: impl Fn(&Value) -> Vec<(&'static str, bool)>,
    ) {
        let contract: Value = serde_json::from_str(contract).expect("alias contract JSON");
        if contract["category"] != "non-decoder" {
            unclassified(
                label,
                "classified alias does not implement DeserializeOwned",
            );
        }
        if std::any::type_name::<T>().split('<').next() != Some(canonical) {
            unclassified(label, "non-decoder alias compiler root identity mismatch");
        }
        if !alternate(&Value::Null).is_empty() {
            unclassified(
                label,
                "non-decoder alias exposes raw alternate traits requiring production coverage",
            );
        }
        let mut receipt = json!({"name":label,"classificationOnly":true,"category":"non-decoder-alias","deserialize":false,"failures":[]});
        if let Some(scope) = contract.get("monomorphization") {
            receipt["monomorphization"] = scope.clone();
        }
        emit(&receipt);
    }
}
impl<T: serde::Deserialize<'static>> DetectAlias for &&AliasProbe<T> {
    fn probe(
        self,
        label: &str,
        _: &str,
        _: &str,
        _: impl Fn(&Value) -> Value,
        _: impl Fn(&str, &Value) -> Option<std::result::Result<Value, ()>>,
        _: impl Fn(&Value) -> Vec<(&'static str, bool)>,
    ) {
        unclassified(
            label,
            "borrowed alias Deserialize requires an explicit production probe",
        );
    }
}
impl<T: DeserializeOwned> DetectAlias for &&&AliasProbe<T> {
    fn probe(
        self,
        label: &str,
        canonical: &str,
        contract: &str,
        observe: impl Fn(&Value) -> Value,
        native: impl Fn(&str, &Value) -> Option<std::result::Result<Value, ()>>,
        alternate: impl Fn(&Value) -> Vec<(&'static str, bool)>,
    ) {
        let parsed: Value = serde_json::from_str(contract).expect("alias contract JSON");
        if parsed["category"] != "decoder" {
            unclassified(
                label,
                "Deserialize alias cannot be classified as a non-decoder",
            );
        }
        audit_with_identity::<T>(
            label,
            canonical,
            "alias",
            Some(contract),
            |value| <T as serde::Deserialize>::deserialize(value).is_ok(),
            alternate,
            observe,
            alias_native_checks(&parsed, &native),
        );
    }
}

pub struct LocalProbe<T>(pub std::marker::PhantomData<T>);
pub trait DetectLocal {
    fn probe(self, label: &str, contract: Option<&str>);
}
impl<T> DetectLocal for &LocalProbe<T> {
    fn probe(self, label: &str, _: Option<&str>) {
        emit(&json!({"name":label,"local":true,"deserialize":false,"failures":[]}));
    }
}
impl<T: serde::Deserialize<'static>> DetectLocal for &&LocalProbe<T> {
    fn probe(self, label: &str, _: Option<&str>) {
        unclassified(
            label,
            &format!(
                "{label}: accepted-set assertion: borrowed local Deserialize requires an explicit production probe"
            ),
        );
    }
}
impl<T: DeserializeOwned> DetectLocal for &&&LocalProbe<T> {
    fn probe(self, label: &str, _: Option<&str>) {
        unclassified(
            label,
            &format!(
                "{label}: accepted-set assertion: unclassified function-local Deserialize; move it to module scope for concrete API coverage"
            ),
        );
    }
}
macro_rules! local_decoder {
    ($trait:ident, $method:ident, $bound:path) => {
        pub trait $trait { fn $method(self, label: &str); }
        impl<T> $trait for &LocalProbe<T> { fn $method(self, _: &str) {} }
        impl<T: $bound> $trait for &&LocalProbe<T> {
            fn $method(self, label: &str) { unclassified(label, &format!("{label}: accepted-set assertion: local alternate decoder requires module-scope API coverage")); }
        }
    };
}
local_decoder!(LocalFromStr, check_from_str, std::str::FromStr);
local_decoder!(LocalFromString, check_from_string, From<String>);
local_decoder!(LocalFromStrRef, check_from_str_ref, From<&'static str>);
local_decoder!(LocalFromValue, check_from_value, TryFrom<Value>);
local_decoder!(LocalFromMap, check_from_map, TryFrom<Map<String, Value>>);
