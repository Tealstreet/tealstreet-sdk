use super::*;
use serde_json::json;
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
        Err(serde::de::Error::custom("captured enum inventory"))
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected enum inventory"))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
fn ts_literals(block: &str) -> std::collections::BTreeSet<String> {
    block
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n")
        .split('\'')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, value)| value.to_owned())
        .collect()
}
fn assert_rejected<T: serde::de::DeserializeOwned>(accepted: &std::collections::BTreeSet<String>) {
    let mut rejected = vec![
        json!(null),
        json!(1),
        json!(true),
        json!([]),
        json!({}),
        json!("unknown"),
        json!(""),
    ];
    for name in accepted {
        rejected.extend([
            json!({name:null}),
            json!([name]),
            json!(format!(" {name}")),
            json!(format!("{name}\u{feff}")),
        ]);
        for candidate in [
            name.to_uppercase(),
            name.to_lowercase(),
            name.replace(':', "_"),
            name.replace('-', "_"),
        ] {
            if !accepted.contains(&candidate) {
                rejected.push(json!(candidate));
            }
        }
    }
    for input in rejected {
        assert!(
            serde_json::from_value::<T>(input.clone()).is_err(),
            "accepted rejected literal {input}"
        );
    }
}
#[test]
fn derived_deserialization_inventory_equals_typescript_and_rejects_nonmembers() {
    let source = include_str!("../../../../packages/safe-cex/src/webworker/StoreWorkerPayload.ts");
    let source = source
        .lines()
        .map(|line| line.split("//").next().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    let incoming = ts_literals(
        source
            .split("export type StoreWorkerIncomingOp =")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap(),
    );
    let outgoing = ts_literals(
        source
            .split("export type StoreWorkerOutgoingOp =")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap(),
    );
    let combined = incoming.union(&outgoing).cloned().collect();
    let mut inventory = EnumInventory::default();
    // Inspect serde's generated helper, then test the public string-only Deserialize boundary.
    let _ = StoreWorkerIncomingOpWire::deserialize(&mut inventory);
    assert_eq!(
        inventory
            .0
            .unwrap()
            .iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        incoming
    );
    let _ = StoreWorkerOutgoingOpWire::deserialize(&mut inventory);
    assert_eq!(
        inventory
            .0
            .unwrap()
            .iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        outgoing
    );
    let _ = StoreWorkerOpWire::deserialize(&mut inventory);
    assert_eq!(
        inventory
            .0
            .unwrap()
            .iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        combined
    );
    assert_rejected::<StoreWorkerIncomingOp>(&incoming);
    assert_rejected::<StoreWorkerOutgoingOp>(&outgoing);
    assert_rejected::<StoreWorkerOp>(&combined);
    for input in &outgoing {
        if !incoming.contains(input) {
            assert!(serde_json::from_value::<StoreWorkerIncomingOp>(json!(input)).is_err());
        }
    }
    for input in &incoming {
        if !outgoing.contains(input) {
            assert!(serde_json::from_value::<StoreWorkerOutgoingOp>(json!(input)).is_err());
        }
    }
}
