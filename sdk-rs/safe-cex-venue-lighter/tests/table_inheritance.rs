//! Source witnesses for pending mapOrder; no Rust mapper parity claim.
use safe_cex_num::{ProtoMember, TableHit, js_table_lookup};
use serde_json::Value;
use std::process::Command;

fn inherited_name(member: ProtoMember) -> &'static str {
    match member {
        ProtoMember::Constructor => "constructor",
        ProtoMember::DefineGetter => "__defineGetter__",
        ProtoMember::DefineSetter => "__defineSetter__",
        ProtoMember::HasOwnProperty => "hasOwnProperty",
        ProtoMember::LookupGetter => "__lookupGetter__",
        ProtoMember::LookupSetter => "__lookupSetter__",
        ProtoMember::IsPrototypeOf => "isPrototypeOf",
        ProtoMember::PropertyIsEnumerable => "propertyIsEnumerable",
        ProtoMember::ToString => "toString",
        ProtoMember::ValueOf => "valueOf",
        ProtoMember::Prototype => "__proto__",
        ProtoMember::ToLocaleString => "toLocaleString",
    }
}

fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/table-inheritance-oracle.cjs"
    )
}

#[test]
#[ignore = "requires Node and the maintained TypeScript compiler"]
fn pending_map_order_preserves_inherited_fallback_witnesses() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 17, "mapOrder table witness inventory");
    let inherited_names: std::collections::BTreeSet<_> = cases[..12]
        .iter()
        .map(|case| case["raw"].as_str().unwrap())
        .collect();
    assert_eq!(
        inherited_names,
        std::collections::BTreeSet::from([
            "constructor",
            "__defineGetter__",
            "__defineSetter__",
            "hasOwnProperty",
            "__lookupGetter__",
            "__lookupSetter__",
            "isPrototypeOf",
            "propertyIsEnumerable",
            "toString",
            "valueOf",
            "__proto__",
            "toLocaleString",
        ]),
        "TABLE-CARDINALITY inherited member identities"
    );
    for case in &cases[..12] {
        // Independent Lighter source capture predates this shared primitive.
        // No numeric-code own keys match these twelve inherited names.
        let table: [(&str, Value); 0] = [];
        let hit = js_table_lookup(&table, case["raw"].as_str().unwrap());
        let actual = match hit {
            TableHit::Inherited(member) => {
                serde_json::json!({"kind":"Inherited","member":inherited_name(member),"truthy":hit.is_truthy(),"nullish":hit.is_nullish(),"present":hit.is_present(),"type":if member==ProtoMember::Prototype {"object"}else{"function"}})
            }
            TableHit::Missing => serde_json::json!({"kind":"Missing"}),
            TableHit::Mapped(_) => serde_json::json!({"kind":"Mapped"}),
        };
        for field in ["type", "timeInForce"] {
            assert_eq!(
                case["result"][field]["own"], false,
                "mapOrder inherited ownership {case}"
            );
            assert_eq!(
                case["result"][field]["matchesPrototype"], true,
                "mapOrder actual prototype identity {case}"
            );
            // ts-bug: mapOrder's raw numeric-code tables inherit Object.prototype.
            // Their truthy inherited result suppresses the explicit default.
            assert_eq!(
                case["result"][field]["kind"], "Inherited",
                "mapOrder {field} {case}"
            );
            assert_eq!(
                case["result"][field]["member"], case["raw"],
                "mapOrder inherited identity {case}"
            );
            assert_eq!(
                case["result"][field]["truthy"], true,
                "mapOrder inherited truthiness {case}"
            );
            assert_eq!(
                case["result"][field]["type"],
                if case["raw"] == "__proto__" {
                    "object"
                } else {
                    "function"
                },
                "mapOrder inherited mechanism {case}"
            );
            let expected = serde_json::json!({"kind":case["result"][field]["kind"],"member":case["result"][field]["member"],"truthy":case["result"][field]["truthy"],"nullish":false,"present":true,"type":case["result"][field]["type"]});
            assert_eq!(actual, expected, "mapOrder shared TableHit {field} {case}");
        }
    }
    for (case, (expected_type, expected_tif)) in cases[12..].iter().zip([
        ("limit", "GoodTillCancel"),
        ("limit", "ImmediateOrCancel"),
        ("market", "GoodTillCancel"),
        ("limit", "GoodTillCancel"),
        ("limit", "GoodTillCancel"),
    ]) {
        assert_eq!(
            case["result"]["type"]["value"], expected_type,
            "mapOrder ordinary type {case}"
        );
        assert_eq!(
            case["result"]["timeInForce"]["value"], expected_tif,
            "mapOrder ordinary timeInForce {case}"
        );
        for field in ["type", "timeInForce"] {
            assert_eq!(
                case["result"][field]["kind"], "Mapped",
                "mapOrder ordinary fallback {case}"
            );
        }
    }
}

#[test]
#[ignore = "requires Node and the maintained TypeScript compiler"]
fn own_only_table_lookup_mutation_is_detected() {
    for (mutation, name) in [
        (
            "duplicate-inherited",
            "TABLE-CARDINALITY inherited member identities",
        ),
        ("empty-cases", "mapOrder table witness inventory"),
        ("short-cases", "mapOrder table witness inventory"),
        ("own-only", "mapOrder actual prototype identity"),
        ("wrong-own-function", "mapOrder inherited ownership"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "pending_map_order_preserves_inherited_fallback_witnesses",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_TABLE_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(
            log.contains(name) && log.contains("assertion `left == right` failed"),
            "{log}"
        );
    }
}

#[test]
#[ignore = "requires Node"]
fn table_witness_requires_its_selected_compiler() {
    let output = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/missing-table-compiler"
            ),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}
