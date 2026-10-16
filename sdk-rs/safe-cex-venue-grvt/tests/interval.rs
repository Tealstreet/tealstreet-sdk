#![cfg(not(target_arch = "wasm32"))]
#[path = "../../safe-cex-num/tests/support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{ProtoMember, TableHit};
use safe_cex_venue_grvt::{order_semantics::convert_interval, types::GrvtCandlestickInterval};

#[test]
fn interval_matches_actual_source_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/interval.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 93, "interval corpus census");
    for row in rows {
        let input = row["input"].as_str().unwrap();
        let expected = if let Some(mapped) = row["mapped"].as_str() {
            TableHit::Mapped(match mapped {
                "CI_1_M" => GrvtCandlestickInterval::Ci1M,
                "CI_3_M" => GrvtCandlestickInterval::Ci3M,
                "CI_5_M" => GrvtCandlestickInterval::Ci5M,
                "CI_15_M" => GrvtCandlestickInterval::Ci15M,
                "CI_30_M" => GrvtCandlestickInterval::Ci30M,
                "CI_1_H" => GrvtCandlestickInterval::Ci1H,
                "CI_2_H" => GrvtCandlestickInterval::Ci2H,
                "CI_4_H" => GrvtCandlestickInterval::Ci4H,
                "CI_6_H" => GrvtCandlestickInterval::Ci6H,
                "CI_12_H" => GrvtCandlestickInterval::Ci12H,
                "CI_1_D" => GrvtCandlestickInterval::Ci1D,
                "CI_1_W" => GrvtCandlestickInterval::Ci1W,
                other => panic!("unexpected source interval {other}"),
            })
        } else {
            let member = match row["inherited"].as_str().unwrap() {
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
                other => panic!("unexpected inherited member {other}"),
            };
            assert_eq!(
                member.js_string(),
                row["string"].as_str().unwrap(),
                "INTERVAL inherited string"
            );
            assert_eq!(
                member.length().map(u64::from),
                row["length"].as_u64(),
                "INTERVAL inherited length"
            );
            assert_eq!(
                row["type"],
                if member == ProtoMember::Prototype {
                    "object"
                } else {
                    "function"
                },
                "INTERVAL inherited kind"
            );
            TableHit::Inherited(member)
        };
        assert_eq!(
            convert_interval(input),
            expected,
            "INTERVAL native identity {input:?}"
        );
    }
    source_cardinality::for_each(
        corpus["refusals"].as_array().unwrap(),
        3,
        "grvt-interval-refusals",
        |row| {
            assert!(
                serde_json::from_str::<String>(row["raw"].as_str().unwrap()).is_err(),
                "INTERVAL UTF8 input refusal"
            );
            assert_eq!(
                row["output"], "CI_1_M",
                "INTERVAL surrogate source fallback"
            );
        },
    );
}

#[test]
fn own_only_source_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "interval_matches_actual_source_identity",
            "--nocapture",
        ])
        .env("GRVT_INTERVAL_CONTROL", "own-only")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "source control must fail: {text}"
    );
    assert!(
        text.contains("INTERVAL native identity"),
        "named source control: {text}"
    );
}
