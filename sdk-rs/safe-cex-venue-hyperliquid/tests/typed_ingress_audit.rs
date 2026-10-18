use std::sync::atomic::{AtomicU8, Ordering};
static MODE: AtomicU8 = AtomicU8::new(0);
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

// Observe native f64 at Serialize's callback, before serde_json could erase Infinity.
struct Probe {
    path: String,
    numbers: Rc<RefCell<BTreeMap<String, String>>>,
    index: usize,
    key: Option<String>,
}
impl Probe {
    fn child(&self, key: &str) -> Self {
        Self {
            path: format!(
                "{}/{}",
                self.path,
                key.replace('~', "~0").replace('/', "~1")
            ),
            numbers: self.numbers.clone(),
            index: 0,
            key: None,
        }
    }
    fn field<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<(), serde_json::Error> {
        value.serialize(self.child(key))
    }
    fn number(self, value: f64) -> Result<(), serde_json::Error> {
        self.numbers
            .borrow_mut()
            .insert(self.path, format!("{:016x}", value.to_bits()));
        Ok(())
    }
}
impl serde::Serializer for Probe {
    type Ok = ();
    type Error = serde_json::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;
    fn serialize_f64(self, v: f64) -> Result<(), Self::Error> {
        self.number(v)
    }
    fn serialize_f32(self, v: f32) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_i8(self, v: i8) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_i16(self, v: i16) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_i32(self, v: i32) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_i64(self, v: i64) -> Result<(), Self::Error> {
        self.number(v as f64)
    }
    fn serialize_u8(self, v: u8) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_u16(self, v: u16) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_u32(self, v: u32) -> Result<(), Self::Error> {
        self.number(f64::from(v))
    }
    fn serialize_u64(self, v: u64) -> Result<(), Self::Error> {
        self.number(v as f64)
    }
    fn serialize_bool(self, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_char(self, _: char) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_unit(self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Self::Error> {
        v.serialize(self)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        v: &T,
    ) -> Result<(), Self::Error> {
        self.field(name, v)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Self, Self::Error> {
        Ok(self.child(name))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Self, Self::Error> {
        Ok(self.child(name))
    }
}
impl SerializeSeq for Probe {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        let i = self.index;
        self.index += 1;
        self.field(&i.to_string(), v)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}
macro_rules! sequence {
    ($trait:ident,$method:ident) => {
        impl $trait for Probe {
            type Ok = ();
            type Error = serde_json::Error;
            fn $method<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
                SerializeSeq::serialize_element(self, v)
            }
            fn end(self) -> Result<(), Self::Error> {
                Ok(())
            }
        }
    };
}
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);
impl SerializeMap for Probe {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        let value = serde_json::to_value(key)?;
        self.key = Some(
            value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string()),
        );
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Self::Error> {
        self.field(
            self.key.as_deref().ok_or_else(|| {
                <serde_json::Error as serde::ser::Error>::custom("map value without key")
            })?,
            v,
        )
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}
macro_rules! record {
    ($trait:ident) => {
        impl $trait for Probe {
            type Ok = ();
            type Error = serde_json::Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                v: &T,
            ) -> Result<(), Self::Error> {
                self.field(key, v)
            }
            fn end(self) -> Result<(), Self::Error> {
                Ok(())
            }
        }
    };
}
record!(SerializeStruct);
record!(SerializeStructVariant);
fn capture<T: serde::de::DeserializeOwned + Serialize>(
    raw: &str,
) -> Result<BTreeMap<String, String>, serde_json::Error> {
    let value: T = match MODE.load(Ordering::SeqCst) {
        0 => serde_json::from_str(raw)?,
        1 => serde_json::from_value(serde_json::from_str::<serde_json::Value>(raw)?)?,
        2 => serde_json::from_value(serde_json::from_str::<serde_json::Value>(
            &raw.replace("-0", "-0.0"),
        )?)?,
        3 => {
            let mut value = serde_json::from_str::<serde_json::Value>(raw)?;
            native_zero(&mut value);
            serde_json::from_value(value)?
        }
        _ => unreachable!(),
    };
    let numbers = Rc::new(RefCell::new(BTreeMap::new()));
    value.serialize(Probe {
        path: String::new(),
        numbers: numbers.clone(),
        index: 0,
        key: None,
    })?;
    Ok(numbers.borrow().clone())
}
include!("fixtures/typed-number-bindings.rs");
#[derive(Deserialize)]
struct Corpus {
    names: Vec<String>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    path: String,
    lexeme: String,
    raw: String,
    bits: BTreeMap<String, String>,
}

fn native_zero(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(number)
            if number
                .as_f64()
                .is_some_and(|v| v.to_bits() == 0x8000000000000000) =>
        {
            *value = serde_json::Value::Number(serde_json::Number::from_f64(-0.0).unwrap())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                native_zero(value)
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                native_zero(value)
            }
        }
        _ => {}
    }
}
#[test]
fn deep_public_ingress_repairs_existing_findings_in_four_forms() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/typed-numbers.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "independent source numeric inventory must regenerate: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/typed-numbers.json")).unwrap();
    assert_eq!(corpus.names.len(), 29, "ZERO-CARDINALITY source roots");
    let zeros: Vec<_> = corpus
        .cases
        .iter()
        .filter(|case| case.lexeme == "-0")
        .collect();
    assert_eq!(zeros.len(), 68, "ZERO-CARDINALITY source cases");

    let expected_roots = std::collections::BTreeMap::from([
        ("TriggerOrderType", 2),
        ("OrderType", 4),
        ("ClearinghouseState", 3),
        ("UserFills", 3),
        ("WsTrade", 2),
        ("WsBook", 3),
        ("WsBbo", 7),
        ("WsLevel", 1),
        ("WsOrder", 3),
        ("WsFill", 3),
        ("WsUserFunding", 1),
        ("WsLiquidation", 1),
        ("WsNonUserCancel", 1),
        ("L2Book", 2),
        ("FundingHistoryEntry", 1),
        ("FundingHistory", 1),
        ("UserOpenOrder", 2),
        ("UserOpenOrders", 2),
        ("CancelOrderRequest", 1),
        ("CancelOrderRequests", 2),
        ("Signature", 1),
        ("Candle", 3),
        ("HyperliquidCandle", 3),
        ("WsUserFill", 3),
        ("WsUserFills", 3),
        ("WsUserFundings", 1),
        ("WsUserNonFundingLedgerUpdate", 4),
        ("WsUserNonFundingLedgerUpdates", 4),
        ("BuilderInfo", 1),
    ]);
    assert_eq!(
        corpus
            .names
            .iter()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        expected_roots.keys().copied().collect(),
        "ZERO-CARDINALITY exact roots"
    );
    for (name, required) in expected_roots {
        assert_eq!(
            zeros.iter().filter(|case| case.name == name).count(),
            required,
            "ZERO-CARDINALITY root {name}"
        );
    }
    for name in &corpus.names {
        assert!(
            corpus
                .cases
                .iter()
                .any(|case| &case.name == name && case.lexeme == "-0"),
            "every source numeric type requires signed-zero witness {name}"
        );
    }
    let mut comparisons = 0;
    for mode in 0..4 {
        MODE.store(mode, Ordering::SeqCst);
        for index in 0..68 {
            let case = zeros
                .get(index)
                .expect("ZERO-CARDINALITY native index coverage");
            let actual = capture_typed(&case.name, &case.raw);
            assert_eq!(
                actual.unwrap(),
                case.bits,
                "deep public ingress mode={mode} {}/{}",
                case.name,
                case.path
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 272, "ZERO-CARDINALITY native comparisons");
}
