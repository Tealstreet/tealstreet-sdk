//! Reference witnesses only: shared dictionary writes still block fetchBalance.
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn whole_object_balance_write_replaces_only_the_dictionary_prototype() {
    let names = [
        "USD",
        "BTC",
        "constructor",
        "toString",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
        "__proto__",
    ];
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/balance-write.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&names).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(names.len(), 14, "KUCOIN-CARDINALITY balance input total");
    assert_eq!(
        actual["results"].as_array().unwrap().len(),
        14,
        "KUCOIN-CARDINALITY balance source total"
    );
    let mut comparisons = 0;
    for index in 0..14 {
        let name = names[index];
        let case = &actual["results"][index];
        assert_eq!(case["currency"], name);
        assert_eq!(case["errors"], json!([]));
        assert_eq!(
            case["calls"],
            json!([{"url":"/api/v1/accounts","options":{"params":{"type":"trade"}}}])
        );
        assert_eq!(
            case["info"],
            json!([{"currency":name,"available":"2","holds":"1","balance":"3"}])
        );
        for key in ["globalFree", "freshFree", "freshBase"] {
            assert_eq!(
                case[key], false,
                "whole-object write is not global pollution: {name}/{key}"
            );
        }
        for dictionary in ["currencies", "coins"] {
            let d = &case[dictionary];
            if name == "__proto__" {
                assert_eq!(
                    d["ownKeys"],
                    json!([]),
                    "whole-object prototype replacement must create no own balance key"
                );
                assert_eq!(d["ownCurrency"], false);
                assert_eq!(d["standardPrototype"], false);
                assert_eq!(d["wire"], "{}");
            } else {
                assert_eq!(d["ownKeys"], json!([name]));
                assert_eq!(d["ownCurrency"], true);
                assert_eq!(
                    d["standardPrototype"], true,
                    "ordinary balance dictionary must retain Object.prototype"
                );
                let wire: Value = serde_json::from_str(d["wire"].as_str().unwrap()).unwrap();
                assert!(
                    wire.get(name).is_some(),
                    "shadowing write must survive serialization"
                );
                if dictionary == "coins" {
                    let notional = if name == "USD" {
                        3
                    } else if name == "BTC" {
                        21
                    } else {
                        0
                    };
                    assert_eq!(
                        wire[name],
                        json!({"base":name,"amount":3,"notional":notional}),
                        "balance coin field identity: {name}"
                    );
                } else {
                    for (field, expected) in [("free", 2), ("used", 1), ("total", 3), ("upnl", 0)] {
                        assert_eq!(
                            wire[name][field], expected,
                            "balance currency field identity: {name}/{field}"
                        );
                    }
                }
            }
        }
        if name == "__proto__" {
            assert_eq!(case["currencies"]["prototype"]["free"], 2);
            assert_eq!(case["currencies"]["prototype"]["total"], 3);
            assert_eq!(case["coins"]["prototype"]["base"], "__proto__");
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 14, "KUCOIN-CARDINALITY balance compared cases");
    assert_eq!(
        actual["nested"],
        json!({"ownKeys":[],"global":true,"fresh":1}),
        "counterfactual nested write has a different blast radius"
    );
    assert_eq!(actual["cleaned"], true);
}

#[test]
fn null_prototype_substitution_fails_the_live_balance_witness() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "whole_object_balance_write_replaces_only_the_dictionary_prototype",
            "--nocapture",
        ])
        .env(
            "NODE_OPTIONS",
            concat!(
                "--require=",
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/oracle-controls.cjs"
            ),
        )
        .env("KUCOIN_ORACLE_MUTATION", "balance-prototype")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("ordinary balance dictionary must retain Object.prototype")
    );
}
