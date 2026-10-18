use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
enum External {
    Unit,
    Number(f64),
    Pair(f64, f64),
    Record { left: f64, right: f64 },
}

impl JsNumberOutput for External {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum {
            Unit, Number(n), Pair(left, right), Record { left, right }
        });
    }
}

#[test]
fn external_tags_reach_enum_visitors_and_keep_native_number_bits() {
    for (raw, tag, expected) in [
        (r#""Unit""#, "Unit", vec![]),
        (r#"{"Unit":null}"#, "Unit", vec![]),
        (r#"{"Number":-0}"#, "Number", vec![(-0.0_f64).to_bits()]),
        (
            r#"{"Number":1e400}"#,
            "Number",
            vec![f64::INFINITY.to_bits()],
        ),
        (
            r#"{"Pair":[-0,1.25]}"#,
            "Pair",
            vec![(-0.0_f64).to_bits(), 1.25_f64.to_bits()],
        ),
        (
            r#"{"Record":{"left":2.5,"right":-0}}"#,
            "Record",
            vec![2.5_f64.to_bits(), (-0.0_f64).to_bits()],
        ),
    ] {
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        // Ordinary finite/tagged forms must also be valid under Serde's own
        // JSON enum decoder. Numeric ingress additionally permits JS overflow.
        let reference = if raw.contains("1e400") {
            None
        } else {
            Some(serde_json::from_str::<External>(raw).unwrap())
        };
        let decoded = [
            js_from_str::<External>(raw).unwrap(),
            js_from_value::<External>(value.clone()).unwrap(),
            js_from_value_ref::<External>(&value).unwrap(),
        ];
        for actual in decoded.into_iter().chain(reference) {
            let (actual_tag, numbers) = match actual {
                External::Unit => ("Unit", vec![]),
                External::Number(n) => ("Number", vec![n.to_bits()]),
                External::Pair(left, right) => ("Pair", vec![left.to_bits(), right.to_bits()]),
                External::Record { left, right } => {
                    ("Record", vec![left.to_bits(), right.to_bits()])
                }
            };
            assert_eq!(actual_tag, tag, "native external tag {raw}");
            assert_eq!(numbers, expected, "native external numeric bits {raw}");
        }
    }
}

#[test]
fn external_tag_maps_require_one_variant_and_keep_string_union_guards() {
    for raw in [
        "{}",
        r#"{"Unit":null,"Number":1}"#,
        r#"{"Number":1,"Unit":null}"#,
        r#"{"Unknown":null}"#,
        r#"{"Pair":[1]}"#,
        r#"{"Pair":[1,2,3]}"#,
        r#"{"Record":{"left":1}}"#,
    ] {
        assert!(
            js_from_str::<External>(raw).is_err(),
            "invalid external enum {raw}"
        );
    }
    assert!(js_from_str::<safe_cex_types::PositionSide>(r#"{"long":null}"#).is_err());
    assert_eq!(
        js_from_str::<safe_cex_types::PositionSide>(r#""long""#).unwrap(),
        safe_cex_types::PositionSide::Long
    );
}
