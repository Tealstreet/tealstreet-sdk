use safe_cex_types::PositionMode;
use safe_cex_venue_okx::market_policy::*;
use serde_json::{Map, Value, json};
use std::{
    cell::Cell,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn eu_market_and_worker_policy_native_before_source() {
    let instruments = [
        None,
        Some(json!("SWAP")),
        Some(json!("FUTURES")),
        Some(json!("SPOT")),
        Some(json!(false)),
        Some(json!(["FUTURES"])),
    ];
    let contracts = [
        None,
        Some(json!("linear")),
        Some(json!("inverse")),
        Some(json!("LINEAR")),
        Some(Value::Null),
        Some(json!({})),
    ];
    let rules = [
        None,
        Some(json!("xperp")),
        Some(json!("normal")),
        Some(json!(false)),
    ];
    let mut markets = vec![];
    let mut expected_markets = vec![];
    for (i, instrument) in instruments.iter().enumerate() {
        for (c, contract) in contracts.iter().enumerate() {
            for (r, rule) in rules.iter().enumerate() {
                let mut market = Map::new();
                for (name, value) in [
                    ("instType", instrument),
                    ("ctType", contract),
                    ("ruleType", rule),
                ] {
                    if let Some(value) = value {
                        market.insert(name.into(), value.clone());
                    }
                }
                let expected = matches!(c, 1 | 2) && (i == 1 || (i == 2 && r == 1));
                let actual = is_supported_eu_market_instrument(&market);
                assert_eq!(actual, expected, "OKX-MARKET native EU support {i}/{c}/{r}");
                markets.push(Value::Object(market));
                expected_markets.push(actual);
            }
        }
    }
    let methods = [
        "changePositionMode".encode_utf16().collect::<Vec<_>>(),
        "fetchBalance".encode_utf16().collect(),
        vec![],
        vec![0xd800],
    ];
    let mut workers = vec![];
    let mut expected_workers = vec![];
    for initial in [None, Some(PositionMode::OneWay), Some(PositionMode::Hedged)] {
        for (index, method) in methods.iter().enumerate() {
            for first in [None, Some(false), Some(true)] {
                let mode = Cell::new(initial);
                after_method_called_in_worker(&mode, method, first);
                let expected = if index == 0 {
                    Some(if first == Some(true) {
                        PositionMode::Hedged
                    } else {
                        PositionMode::OneWay
                    })
                } else {
                    initial
                };
                assert_eq!(
                    mode.get(),
                    expected,
                    "OKX-WORKER native mode {initial:?}/{index}/{first:?}"
                );
                let mut case = json!({"initial":initial,"method":method});
                if let Some(first) = first {
                    case["first"] = json!(first);
                }
                workers.push(case);
                expected_workers.push(mode.get());
            }
        }
    }
    assert!(
        !is_supported_eu_market_instrument(&Map::new()),
        "OKX-MARKET native inherited JSON view boundary"
    );
    assert_eq!(markets.len(), 144, "declared EU market profile floor");
    assert_eq!(workers.len(), 36, "declared worker profile floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/eu-worker-policy-oracle.mjs"
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
        .write_all(&serde_json::to_vec(&json!({"markets":markets,"workers":workers})).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["markets"].as_array().unwrap().len(),
        144,
        "source market floor"
    );
    assert_eq!(
        source["workers"].as_array().unwrap().len(),
        36,
        "source worker floor"
    );
    assert_eq!(
        source["markets"],
        json!(expected_markets),
        "live EU inheritance after native identities"
    );
    assert_eq!(
        source["workers"],
        json!(expected_workers),
        "live worker policy after native mode identities"
    );
    assert_eq!(
        source["inheritedSupported"], true,
        "source inherited-record witness"
    );
    assert_eq!(
        source["jsonView"],
        json!({}),
        "source chosen own-record projection"
    );
}
