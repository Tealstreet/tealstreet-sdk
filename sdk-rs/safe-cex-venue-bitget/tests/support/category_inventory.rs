use super::BitgetUtaCategoryWire;
use serde_json::Value;

#[derive(Default)]
struct Inventory(Option<&'static [&'static str]>);
impl<'de> serde::Deserializer<'de> for &mut Inventory {
    type Error = serde::de::value::Error;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0 = Some(variants);
        Err(serde::de::Error::custom("inventory captured"))
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected derived enum metadata"))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
fn check(oracle: Value) {
    let mut inventory = Inventory::default();
    let _ = BitgetUtaCategoryWire::deserialize(&mut inventory);
    let mut actual = inventory.0.expect("category serde metadata").to_vec();
    actual.sort();
    let mut expected: Vec<&str> = oracle["acceptedSets"]["BITGET_UTA_CATEGORY"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    expected.sort();
    assert_eq!(
        actual, expected,
        "serde accepted-set inventory BITGET_UTA_CATEGORY"
    );
}
#[test]
fn frozen_category_inventory() {
    check(serde_json::from_str(include_str!("../types-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires Node and TypeScript"]
fn live_category_inventory() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/types-oracle.cjs"
        ))
        .output()
        .expect("execute Node");
    assert!(
        result.status.success(),
        "category oracle failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    check(serde_json::from_slice(&result.stdout).unwrap());
}
