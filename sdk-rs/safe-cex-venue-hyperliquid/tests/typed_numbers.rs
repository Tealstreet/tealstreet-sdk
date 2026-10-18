use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
static MODE: AtomicU8 = AtomicU8::new(0);
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

// Observe native f64 at Serialize's callback, before serde_json could erase Infinity.
struct Probe {
    path: String,
    numbers: Rc<RefCell<BTreeMap<String, String>>>,
    strings: Rc<RefCell<BTreeMap<String, Vec<u16>>>>,
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
            strings: self.strings.clone(),
            index: 0,
            key: None,
        }
    }
    fn field<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<(), serde_json::Error> {
        value.serialize(self.child(key))
    }
    fn number(self, value: f64) -> Result<(), serde_json::Error> {
        self.numbers.borrow_mut().insert(
            self.path,
            format!(
                "{:016x}",
                if value.is_nan() {
                    0x7ff8000000000000
                } else {
                    value.to_bits()
                }
            ),
        );
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
    fn serialize_str(self, value: &str) -> Result<(), Self::Error> {
        self.strings
            .borrow_mut()
            .insert(self.path, value.encode_utf16().collect());
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
        variant: &'static str,
    ) -> Result<(), Self::Error> {
        serde::Serializer::serialize_str(self, variant)
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
fn capture<
    T: serde::de::DeserializeOwned + Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
>(
    raw: &str,
) -> Result<BTreeMap<String, String>, serde_json::Error> {
    let value: T = match MODE.load(Ordering::SeqCst) {
        0 => serde_json::from_str(raw)?,
        1 => serde_json::from_slice(raw.as_bytes())?,
        2 => serde_json::from_value(serde_json::from_str::<serde_json::Value>(raw)?)?,
        3 => T::deserialize(&serde_json::from_str::<serde_json::Value>(raw)?)?,
        4 => safe_cex_num::numeric_ingress::js_from_str(raw)?,
        5 => safe_cex_num::numeric_ingress::js_from_value(serde_json::from_str(raw)?)?,
        6 => safe_cex_num::numeric_ingress::js_from_value_ref(&serde_json::from_str(raw)?)?,
        _ => unreachable!(),
    };
    let numbers = Rc::new(RefCell::new(BTreeMap::new()));
    value.serialize(Probe {
        path: String::new(),
        numbers: numbers.clone(),
        strings: Rc::new(RefCell::new(BTreeMap::new())),
        index: 0,
        key: None,
    })?;
    Ok(numbers.borrow().clone())
}
include!("fixtures/typed-number-bindings.rs");

fn capture_strings<
    T: serde::de::DeserializeOwned + Serialize + safe_cex_num::numeric_ingress::JsNumberOutput,
>(
    raw: &str,
) -> Result<BTreeMap<String, Vec<u16>>, serde_json::Error> {
    let value: T = safe_cex_num::numeric_ingress::js_from_str(raw)?;
    let strings = Rc::new(RefCell::new(BTreeMap::new()));
    value.serialize(Probe {
        path: String::new(),
        numbers: Rc::new(RefCell::new(BTreeMap::new())),
        strings: strings.clone(),
        index: 0,
        key: None,
    })?;
    Ok(strings.borrow().clone())
}
include!("fixtures/typed-string-bindings.rs");

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
#[test]
fn all_numeric_types_match_node_bits_before_serialization() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/typed-numbers.json")).unwrap();
    assert_eq!(corpus.names.len(), 29, "NUMERIC-CARDINALITY source roots");
    assert_eq!(corpus.cases.len(), 680, "NUMERIC-CARDINALITY source cases");
    assert_eq!(
        corpus
            .cases
            .iter()
            .map(|case| case.bits.len())
            .sum::<usize>(),
        1420,
        "NUMERIC-CARDINALITY source numeric field total"
    );
    // Pin subcorpora separately without rewriting the source-first artifact.
    let root_counts = [
        ("TriggerOrderType", 20),
        ("OrderType", 40),
        ("ClearinghouseState", 30),
        ("UserFills", 30),
        ("WsTrade", 20),
        ("WsBook", 30),
        ("WsBbo", 70),
        ("WsLevel", 10),
        ("WsOrder", 30),
        ("WsFill", 30),
        ("WsUserFunding", 10),
        ("WsLiquidation", 10),
        ("WsNonUserCancel", 10),
        ("L2Book", 20),
        ("FundingHistoryEntry", 10),
        ("FundingHistory", 10),
        ("UserOpenOrder", 20),
        ("UserOpenOrders", 20),
        ("CancelOrderRequest", 10),
        ("CancelOrderRequests", 20),
        ("Signature", 10),
        ("Candle", 30),
        ("HyperliquidCandle", 30),
        ("WsUserFill", 30),
        ("WsUserFills", 30),
        ("WsUserFundings", 10),
        ("WsUserNonFundingLedgerUpdate", 40),
        ("WsUserNonFundingLedgerUpdates", 40),
        ("BuilderInfo", 10),
    ];
    assert_eq!(
        root_counts.len(),
        29,
        "NUMERIC-CARDINALITY native root inventory"
    );
    for (name, count) in root_counts {
        assert!(
            corpus.names.iter().any(|n| n == name),
            "NUMERIC-CARDINALITY root {name}"
        );
        assert_eq!(
            corpus.cases.iter().filter(|case| case.name == name).count(),
            count,
            "NUMERIC-CARDINALITY source subcorpus {name}"
        );
    }
    for name in &corpus.names {
        for lexeme in [
            "1e400",
            "-1e400",
            "9007199254740993",
            "-0",
            "5e-324",
            "-1e-400",
        ] {
            assert!(
                corpus
                    .cases
                    .iter()
                    .any(|case| &case.name == name && case.lexeme == lexeme),
                "per-type numeric edge coverage {name}/{lexeme}"
            );
        }
    }
    let mut comparisons = 0;
    let mut numeric_fields = 0;
    for mode in 0..7 {
        MODE.store(mode, Ordering::SeqCst);
        for index in 0..680 {
            let case = corpus
                .cases
                .get(index)
                .expect("NUMERIC-CARDINALITY native index coverage");
            assert!(
                !case.bits.is_empty(),
                "NUMERIC-CARDINALITY source numeric fields"
            );
            let actual = capture_typed(&case.name, &case.raw).unwrap_or_else(|error| {
                panic!(
                    "raw public typed ingress {}/{} {}: {error}",
                    case.name, case.path, case.lexeme
                )
            });
            numeric_fields += actual.len();
            assert_eq!(
                actual, case.bits,
                "native bits {}/{} {}",
                case.name, case.path, case.lexeme
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 4760, "NUMERIC-CARDINALITY native comparisons");
    assert_eq!(
        numeric_fields, 9940,
        "NUMERIC-CARDINALITY native numeric field total"
    );
}
#[test]
fn live_node_number_corpus_and_binding_inventory_regenerate() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/typed-numbers.cjs");
    let string_script = script.with_file_name("typed-string-boundaries.cjs");
    let string_output = std::process::Command::new("node")
        .arg(string_script)
        .output()
        .unwrap();
    assert!(
        string_output.status.success(),
        "shared string-boundary source oracle must execute: {}",
        String::from_utf8_lossy(&string_output.stderr)
    );
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Node number oracle and source binding inventory must execute: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn shared_utf8_refusal_and_scalar_unicode_remain_separate_from_numeric_flips() {
    {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/typed-string-boundaries.json")).unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        let required = 78;
        assert_eq!(cases.len(), required, "STRING-CARDINALITY source cases");
        assert_eq!(
            corpus["names"].as_array().unwrap().len(),
            26,
            "STRING-CARDINALITY source roots"
        );

        let expected_roots: &[&str] = &[
            "ClearinghouseState",
            "UserFills",
            "WsTrade",
            "WsBook",
            "WsBbo",
            "WsLevel",
            "WsOrder",
            "WsFill",
            "WsUserFunding",
            "WsLiquidation",
            "WsNonUserCancel",
            "L2Book",
            "FundingHistoryEntry",
            "FundingHistory",
            "UserOpenOrder",
            "UserOpenOrders",
            "CancelOrderRequest",
            "Signature",
            "Candle",
            "HyperliquidCandle",
            "WsUserFill",
            "WsUserFills",
            "WsUserFundings",
            "WsUserNonFundingLedgerUpdate",
            "WsUserNonFundingLedgerUpdates",
            "BuilderInfo",
        ];
        assert_eq!(
            corpus["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>(),
            expected_roots.iter().copied().collect(),
            "STRING-CARDINALITY exact roots"
        );
        for name in expected_roots {
            assert_eq!(
                cases.iter().filter(|case| case["name"] == *name).count(),
                3,
                "STRING-CARDINALITY root {name}"
            );
            assert_eq!(
                cases
                    .iter()
                    .filter(|case| case["name"] == *name && case["refusal"] == true)
                    .count(),
                2,
                "STRING-CARDINALITY root refusals {name}"
            );
        }
        for name in corpus["names"].as_array().unwrap() {
            assert!(
                cases
                    .iter()
                    .any(|c| &c["name"] == name && c["refusal"] == true),
                "each string-bearing numeric root needs refusal witness"
            );
            assert!(
                cases
                    .iter()
                    .any(|c| &c["name"] == name && c["refusal"] == false),
                "each string-bearing numeric root needs scalar positive witness"
            );
        }
        let mut comparisons = 0;
        for index in 0..required {
            let case = cases
                .get(index)
                .expect("STRING-CARDINALITY native index coverage");
            let capture = capture_typed_strings;
            let actual = capture(
                case["name"].as_str().unwrap(),
                case["raw"].as_str().unwrap(),
            );
            if case["refusal"] == true {
                assert!(
                    actual.is_err(),
                    "shared UTF8 representation refusal {}",
                    case["name"]
                );
                assert!(
                    case["sourceJson"].as_str().unwrap().contains("\\ud"),
                    "source retains valid escaped lone surrogate"
                );
            } else {
                assert_eq!(
                    actual.unwrap(),
                    serde_json::from_value::<BTreeMap<String, Vec<u16>>>(case["strings"].clone())
                        .unwrap(),
                    "scalar Unicode and astral positives {}",
                    case["name"]
                );
            }
            comparisons += 1;
        }
        assert_eq!(
            comparisons, required,
            "STRING-CARDINALITY native comparisons"
        );
    }
}
