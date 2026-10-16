//! Accepted-set tests consume TypeScript AST fixtures, including rejected values.
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::OnceLock;
fn fixtures() -> &'static Value {
    static FIXTURES: OnceLock<Value> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/check-unions.mjs"
            ))
            .output()
            .expect("Node and workspace TypeScript required");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
pub(crate) fn check<T: DeserializeOwned, Wire: DeserializeOwned>(name: &str) {
    let mut inventory = EnumInventory::default();
    let _ = Wire::deserialize(&mut inventory);
    let mut actual: Vec<_> = inventory
        .0
        .expect("canonical serde metadata")
        .iter()
        .map(|s| Value::String((*s).into()))
        .collect();
    let mut expected = fixtures()["members"][name].as_array().unwrap().clone();
    actual.sort_by_key(Value::to_string);
    expected.sort_by_key(Value::to_string);
    assert_eq!(
        actual, expected,
        "{name}: serde accepted member inventory differs from TS"
    );
    check_cases::<T>(name);
}
fn check_cases<T: DeserializeOwned>(name: &str) {
    let count = match name {
        "BybitTransactionType" => 175,
        "BybitVipLevel" => 93,
        "BybitProductType" => 31,
        "BybitAccountType" | "BybitBboSideType" => 27,
        "BybitSiteId" => 19,
        "BybitBboLevel" | "BybitOrderExecutionParams" => 38,
        "BybitTransactionAccountType" => 25,
        "BybitTransactionCategory" => 37,
        "BybitResponseMeta" => 28,
        _ => panic!("unregistered union corpus {name}"),
    };
    assert_eq!(
        fixtures()["cases"][name].as_array().unwrap().len(),
        count,
        "UNION-CARDINALITY source {name}"
    );

    for case in fixtures()["cases"][name].as_array().unwrap() {
        assert_eq!(
            serde_json::from_value::<T>(case["value"].clone()).is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{name}: unexpected acceptance for {}",
            case["value"]
        );
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn numeric_and_execution_unions_reject_outside_values() {
    check_cases::<crate::order_params::BybitBboLevel>("BybitBboLevel");
    check_cases::<crate::order_params::BybitOrderExecutionParams>("BybitOrderExecutionParams");
    check_cases::<crate::response::BybitResponseMeta>("BybitResponseMeta");
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn shared_site_union_matches_source_and_rejects_object_tags() {
    use crate::site::BybitSiteId;
    let _exhaustive = |v: BybitSiteId| match v {
        BybitSiteId::BraBtl => (),
    };
    use serde::Deserialize;
    // Go through the public strict path. Its private wire decoder forwards
    // serde's canonical unknown_variant names after requiring a string.
    let error = BybitSiteId::deserialize(
        serde::de::value::StringDeserializer::<AcceptedNames>::new("__inventory__".into()),
    )
    .unwrap_err();
    let actual: Vec<_> = error
        .0
        .expect("canonical serde names")
        .iter()
        .map(|s| Value::String((*s).into()))
        .collect();
    assert_eq!(
        actual,
        *fixtures()["members"]["BybitSiteId"].as_array().unwrap()
    );
    check_cases::<BybitSiteId>("BybitSiteId");
    for name in actual {
        let object = serde_json::json!({name.as_str().unwrap(): null});
        assert!(
            BybitSiteId::deserialize(object).is_err(),
            "public qualified decoder must reject object tags"
        );
    }
}
#[derive(Default)]
struct EnumInventory(Option<&'static [&'static str]>);
impl<'de> serde::Deserializer<'de> for &mut EnumInventory {
    type Error = serde::de::value::Error;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0 = Some(variants);
        Err(serde::de::Error::custom(
            "inventory captured; no value requested",
        ))
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected derived enum metadata"))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

#[derive(Debug)]
struct AcceptedNames(Option<&'static [&'static str]>);
impl std::fmt::Display for AcceptedNames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("accepted spelling inventory")
    }
}
impl std::error::Error for AcceptedNames {}
impl serde::de::Error for AcceptedNames {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self(None)
    }
    fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
        Self(Some(expected))
    }
}
