use safe_cex_venue_hyperliquid::types::*;
use serde::{Deserialize, de::Visitor};
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Case {
    name: String,
    accepted: Vec<String>,
    rejected: Vec<String>,
}
#[derive(Debug)]
struct InventoryError(Option<&'static [&'static str]>);
impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("literal metadata captured")
    }
}
impl std::error::Error for InventoryError {}
impl serde::de::Error for InventoryError {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self(None)
    }
    fn unknown_variant(_: &str, expected: &'static [&'static str]) -> Self {
        Self(Some(expected))
    }
}
struct Inventory;
impl<'de> serde::Deserializer<'de> for Inventory {
    type Error = InventoryError;
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _: &'static str,
        variants: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error> {
        Err(InventoryError(Some(variants)))
    }
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_str::<InventoryError>("__literal_inventory__")
    }
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.deserialize_str(visitor)
    }
    fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
        Err(InventoryError(None))
    }
    serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any}
}
fn check<T: for<'de> Deserialize<'de>>(case: &Case) -> (Vec<bool>, Vec<bool>) {
    let Err(metadata) = T::deserialize(Inventory) else {
        panic!("{} accepted inventory probe", case.name)
    };
    let mut actual: Vec<_> = metadata
        .0
        .expect("actual serde variant metadata required")
        .iter()
        .map(|v| (*v).to_owned())
        .collect();
    actual.sort();
    let mut expected = case.accepted.clone();
    expected.sort();
    assert_eq!(
        actual, expected,
        "{} actual deserializer accepted set",
        case.name
    );
    let accepted = case
        .accepted
        .iter()
        .map(|value| serde_json::from_str::<T>(&serde_json::to_string(value).unwrap()).is_ok())
        .collect();
    let rejected = case
        .rejected
        .iter()
        .map(|raw| serde_json::from_str::<T>(raw).is_err())
        .collect();
    for value in &case.accepted {
        assert!(
            case.rejected.contains(&format!(
                "{{{}:null}}",
                serde_json::to_string(value).unwrap()
            )),
            "externally tagged negative property required"
        );
    }
    (accepted, rejected)
}

fn source_is_current() {
    static SOURCE: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    let result = SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/literal-unions.cjs"
            ))
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    });
    assert!(
        result.is_ok(),
        "LITERAL-CONTENT actual source regeneration: {result:?}"
    );
}
fn expected_counts(name: &str) -> (usize, usize) {
    match name {
        "HyperliquidUserAbstractionMode" => (6, 52),
        "Tif" => (3, 31),
        "Tpsl" => (2, 21),
        "Grouping" => (3, 30),
        "WsUserNonFundingLedgerUpdateType" => (4, 35),
        other => panic!("LITERAL-CARDINALITY unknown root {other}"),
    }
}
#[test]
fn actual_serde_literal_sets_match_ts_and_reject_widening() {
    source_is_current();
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/literal-unions.json")).unwrap();
    let admitted: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/literal-unions.json")).unwrap();
    assert_eq!(cases.len(), 5, "LITERAL-CARDINALITY source roots");
    let required = [
        "HyperliquidUserAbstractionMode",
        "Tif",
        "Tpsl",
        "Grouping",
        "WsUserNonFundingLedgerUpdateType",
    ];
    let mut actual: Vec<_> = cases.iter().map(|case| case.name.as_str()).collect();
    actual.sort();
    let mut expected = required.to_vec();
    expected.sort();
    assert_eq!(
        actual, expected,
        "exact enum corpus ownership and uniqueness"
    );
    for name in required {
        assert!(
            cases.iter().any(|case| case.name == name),
            "missing accepted-set corpus for {name}"
        );
    }
    let mut comparisons = (0, 0, 0);
    for index in 0..5 {
        let case = cases
            .get(index)
            .expect("LITERAL-CARDINALITY native index coverage");
        assert_eq!(
            case,
            admitted.get(index).unwrap(),
            "LITERAL-CONTENT native input {index}"
        );
        let expected = expected_counts(&case.name);
        assert_eq!(
            (case.accepted.len(), case.rejected.len()),
            expected,
            "LITERAL-CARDINALITY source dimensions {}",
            case.name
        );
        macro_rules! dispatch {($($ty:ident),*)=>{match case.name.as_str() {$(stringify!($ty)=>{
            let qualified: Vec<bool> = case.rejected.iter().map(|raw| {
                let mut decoder = serde_json::Deserializer::from_str(raw);
                $ty::deserialize(&mut decoder).is_err()
            }).collect();
            let (accepted, rejected) = check::<$ty>(case);
            (accepted, rejected, qualified)
        },)* other=>panic!("unexamined literal enum {other}")}}}
        let (accepted, rejected, qualified) = dispatch!(
            HyperliquidUserAbstractionMode,
            Tif,
            Tpsl,
            Grouping,
            WsUserNonFundingLedgerUpdateType
        );
        assert_eq!(
            accepted.len(),
            expected.0,
            "LITERAL-NATIVE accepted receipt count {}",
            case.name
        );
        assert_eq!(
            rejected.len(),
            expected.1,
            "LITERAL-NATIVE rejected receipt count {}",
            case.name
        );
        assert_eq!(
            qualified.len(),
            expected.1,
            "LITERAL-NATIVE qualified receipt count {}",
            case.name
        );
        assert!(
            accepted.iter().all(|value| *value),
            "LITERAL-NATIVE accepted result {}",
            case.name
        );
        assert!(
            rejected.iter().all(|value| *value),
            "LITERAL-NATIVE rejected result {}",
            case.name
        );
        assert!(
            qualified.iter().all(|value| *value),
            "LITERAL-NATIVE qualified result {}",
            case.name
        );
        comparisons.0 += accepted.len();
        comparisons.1 += rejected.len();
        comparisons.2 += qualified.len();
    }
    assert_eq!(
        comparisons,
        (18, 169, 169),
        "LITERAL-CARDINALITY native receipt totals"
    );
}
#[test]
fn literal_corpus_matches_live_typescript() {
    source_is_current();
}

#[test]
fn surviving_rejection_pair_matches_actual_source_assignability() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/literal-rejection-equivalence.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "LITERAL-SOURCE pair checker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["type"], "HyperliquidUserAbstractionMode");
    assert_eq!(source["cases"].as_array().unwrap().len(), 3);
    for (index, raw) in [r#""""#, "null"].into_iter().enumerate() {
        assert!(
            serde_json::from_str::<HyperliquidUserAbstractionMode>(raw).is_err(),
            "LITERAL-NATIVE surviving rejected pair {raw}"
        );
        assert_eq!(source["cases"][index]["raw"], raw);
        assert_eq!(source["cases"][index]["accepted"], false);
    }
    let native: HyperliquidUserAbstractionMode =
        serde_json::from_str(r#""unifiedAccount""#).unwrap();
    assert_eq!(
        native,
        HyperliquidUserAbstractionMode::UnifiedAccount,
        "LITERAL-NATIVE distinguishing positive variant"
    );
    assert_eq!(source["cases"][2]["raw"], r#""unifiedAccount""#);
    assert_eq!(source["cases"][2]["accepted"], true);
}

#[test]
fn interface_inventories_refuse_inherited_numeric_members() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/heritage-control.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "inherited-number controls must fail at every heritage guard: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("7 inherited-number controls refused by name")
    );
}
