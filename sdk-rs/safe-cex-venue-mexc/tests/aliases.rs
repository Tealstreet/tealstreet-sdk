use safe_cex_venue_mexc::utils::{
    normalize_balance, normalize_market, normalize_order, normalize_plan_order, normalize_position,
    normalize_stop_order, normalize_ticker,
};
use serde_json::{Value, json};

#[test]
fn mexc_mapper_owned_snapshot_vs_live_alias() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/alias_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut raw = case["raw"].clone();
        let map = raw.as_object().unwrap();
        let market = normalize_market(&serde_json::Value::Object((map).clone()).into()).unwrap();
        let mut currency_info = None;
        let mut info = match name {
            "normalizeMarket" => market.info.unwrap(),
            "normalizeTicker" => normalize_ticker(map, &market, &|ms| ms)
                .unwrap()
                .info
                .unwrap(),
            "normalizePosition" => normalize_position(map, Some(&market), None, &|ms| ms)
                .unwrap()
                .unwrap()
                .info
                .unwrap(),
            "normalizeOrder" => normalize_order(map, Some(&market), &|ms| ms)
                .unwrap()
                .unwrap()
                .info
                .unwrap(),
            "normalizePlanOrder" => normalize_plan_order(map, Some(&market), &|ms| ms)
                .unwrap()
                .unwrap()
                .info
                .unwrap(),
            "normalizeStopOrder" => normalize_stop_order(map, Some(&market), &|ms| ms)
                .unwrap()
                .unwrap()
                .info
                .unwrap(),
            "normalizeBalance" => {
                let balance = normalize_balance(&json!([raw])).unwrap();
                currency_info = balance
                    .currencies
                    .unwrap()
                    .remove("USDT")
                    .unwrap()
                    .as_balance()
                    .unwrap()
                    .info
                    .clone();
                balance.info.unwrap().as_array().unwrap()[0].clone()
            }
            _ => panic!("unexpected mapper {name}"),
        };
        raw["probe"] = json!(2);
        raw["nested"]["probe"] = json!(2);
        assert_eq!(
            info.get("probe").unwrap().as_f64(),
            Some(1.0),
            "{name}: input mutation must leave owned output unchanged"
        );
        assert_eq!(
            info.get("nested").unwrap().get("probe").unwrap().as_f64(),
            Some(1.0),
            "{name}: nested input mutation must leave owned output unchanged"
        );
        let spread = matches!(name, "normalizePlanOrder" | "normalizeStopOrder");
        assert_eq!(case["afterInput"]["input"], 2, "{name}");
        assert_eq!(
            case["afterInput"]["output"],
            if spread { 1 } else { 2 },
            "{name}: source alias direction"
        );
        assert_eq!(case["afterInput"]["inputNested"], 2, "{name}");
        assert_eq!(
            case["afterInput"]["outputNested"], 2,
            "{name}: source nested alias"
        );
        *info.get_mut("probe").unwrap() = json!(3).into();
        *info.get_mut("nested").unwrap().get_mut("probe").unwrap() = json!(3).into();
        assert_eq!(
            raw["probe"], 2,
            "{name}: output mutation must leave owned input unchanged"
        );
        assert_eq!(
            raw["nested"]["probe"], 2,
            "{name}: nested output mutation must leave owned input unchanged"
        );
        assert_eq!(info.get("probe").unwrap().as_f64(), Some(3.0));
        assert_eq!(
            info.get("nested").unwrap().get("probe").unwrap().as_f64(),
            Some(3.0)
        );
        assert_eq!(
            case["afterOutput"]["input"],
            if spread { 2 } else { 3 },
            "{name}: source reverse alias"
        );
        assert_eq!(case["afterOutput"]["output"], 3, "{name}");
        assert_eq!(
            case["afterOutput"]["inputNested"], 3,
            "{name}: source nested reverse alias"
        );
        assert_eq!(case["afterOutput"]["outputNested"], 3, "{name}");
        if name == "normalizeBalance" {
            let currency = currency_info.unwrap();
            assert_eq!(
                currency.get("probe").unwrap().as_f64(),
                Some(1.0),
                "Rust currency info is an owned snapshot"
            );
            assert_eq!(
                currency
                    .get("nested")
                    .unwrap()
                    .get("probe")
                    .unwrap()
                    .as_f64(),
                Some(1.0)
            );
            assert_eq!(case["afterInput"]["currency"], 2);
            assert_eq!(case["afterOutput"]["currency"], 3);
            assert_eq!(case["afterInput"]["currencyNested"], 2);
            assert_eq!(case["afterOutput"]["currencyNested"], 3);
        }
    }
}

#[test]
fn mexc_mapper_alias_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_mapper_owned_snapshot_vs_live_alias",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "info-alias")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_mapper_owned_snapshot_vs_live_alias ... FAILED"),
        "{text}"
    );
    assert!(
        text.contains("normalizeMarket: source alias direction"),
        "{text}"
    );
}

#[test]
fn mexc_balance_prices_heterogeneous_prototype_reads() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/alias_oracle.mjs"
        ))
        .arg("--prototype")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let currencies = normalize_balance(&source["raw"])
        .unwrap()
        .currencies
        .unwrap();
    assert_eq!(currencies.keys().collect::<Vec<_>>(), vec!["constructor"]);
    assert_eq!(currencies["constructor"].as_balance().unwrap().free, 2.0);
    // Shared dictionary writes are routed: matching own keys does not recreate
    // the prototype-backed numeric read from this typed currency map.
    assert!(!currencies.contains_key("free"));
    assert!(!currencies.contains_key("__proto__"));
    assert_eq!(source["ownKeys"], json!(["constructor"]));
    assert_eq!(source["hasOwnFree"], false);
    assert_eq!(
        source["inheritedFree"], 7,
        "ts-bug: normalizeBalance prototype assignment changes inherited reads"
    );
    assert_eq!(source["prototypeTotal"], 9);
    assert_eq!(source["constructorFree"], 2);
}
