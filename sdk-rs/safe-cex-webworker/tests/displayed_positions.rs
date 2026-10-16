#[path = "../../safe-cex-exchange/tests/support/store.rs"]
mod test_store;
use safe_cex_types::ExchangeName;
use safe_cex_webworker::displayed_positions::DisplayedPositionsPostprocessor;
use serde_json::{Value, json};
fn assert_json(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}"
        ),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect(),
                "{path}"
            );
            for (key, value) in a {
                assert_json(value, &b[key], &format!("{path}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(
                a.len(),
                b.len(),
                "DISPLAYED-CARDINALITY native array {path}"
            );
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_json(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn displayed_positions_match_live_typescript_including_fake_keys_and_pnl_eviction() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/displayed_positions_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scenarios: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(scenarios.len(), 199, "DISPLAYED-CARDINALITY source total");
    assert_eq!(
        scenarios
            .iter()
            .filter(|s| s["clockIdentity"] == true)
            .count(),
        6,
        "PNL-CLOCK case inventory"
    );
    assert_eq!(
        scenarios.iter().filter(|s| s["identity"] == true).count(),
        1,
        "DISPLAYED-IDENTITY case inventory"
    );
    assert_eq!(
        scenarios
            .iter()
            .filter(|s| s["divergence"] == "absent-function-fake")
            .count(),
        11,
        "all Object.prototype callable names must be witnessed"
    );
    let mut compared_steps = 0;
    for i in 0..199 {
        let scenario = scenarios
            .get(i)
            .expect("DISPLAYED-CARDINALITY missing fixed scenario");
        let step_count = match i {
            151 => 9,
            152 => 5,
            153 | 155 | 156 => 3,
            154 => 6,
            _ => 1,
        };
        let steps = scenario["steps"].as_array().unwrap();
        assert_eq!(
            steps.len(),
            step_count,
            "DISPLAYED-CARDINALITY steps per scenario {i}"
        );
        let mut options = json!({"key":"", "secret":""});
        if let Some(extra) = scenario.get("options").and_then(Value::as_object) {
            options.as_object_mut().unwrap().extend(extra.clone());
        }
        let mut processor =
            DisplayedPositionsPostprocessor::new("acct".into(), ExchangeName::Bybit);
        let mut force = scenario["force"].as_bool().unwrap_or(false);
        for j in 0..step_count {
            let step = steps
                .get(j)
                .expect("DISPLAYED-CARDINALITY missing fixed step");
            let mut value =
                serde_json::to_value(test_store::TestStore::new().data.into_inner()).unwrap();
            value
                .as_object_mut()
                .unwrap()
                .extend(step["store"].as_object().unwrap().clone());
            let mut store: safe_cex_types::StoreData = serde_json::from_value(value).unwrap();
            if let Some(special) = step.get("special") {
                let value = match special["number"].as_str().unwrap() {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ => unreachable!(),
                };
                match special["field"].as_str().unwrap() {
                    "entryPrice" => store.positions[0].entry_price = value,
                    "contracts" => store.positions[0].contracts = value,
                    "unrealizedPnl" => store.positions[0].unrealized_pnl = value,
                    _ => unreachable!(),
                }
            }
            if let Some(extra) = step.get("options").and_then(Value::as_object) {
                options.as_object_mut().unwrap().extend(extra.clone());
            }
            if let Some(value) = step["force"].as_bool() {
                force = value;
            }
            let mut clock_calls = 0;
            let mut clock = || {
                let index = clock_calls;
                clock_calls += 1;
                if let Some(clocks) = step["clocks"].as_array() {
                    let value = clocks.get(index).expect("PNL-CLOCK unexpected Rust read");
                    if value == "NaN" {
                        f64::NAN
                    } else {
                        value.as_f64().unwrap()
                    }
                } else {
                    step["now"].as_f64().unwrap()
                }
            };
            processor.process(
                &mut store,
                &mut clock,
                &serde_json::from_value(options.clone()).unwrap(),
                force,
            );
            if scenario["divergence"].is_null() {
                assert_eq!(
                    clock_calls as u64,
                    step["clockCalls"].as_u64().unwrap(),
                    "PNL-CLOCK source call count"
                );
            }
            compared_steps += 1;
            if scenario["clockIdentity"] == true {
                assert_eq!(
                    store.displayed_positions.len(),
                    step["expected"].as_array().unwrap().len(),
                    "DISPLAYED-CARDINALITY clock rows"
                );
                for (row, expected) in store
                    .displayed_positions
                    .iter()
                    .zip(step["expected"].as_array().unwrap())
                {
                    assert_eq!(
                        row.five_min_upnl,
                        expected["fiveMinUpnl"].as_f64(),
                        "PNL-CLOCK native five-minute baseline"
                    );
                    assert_eq!(
                        row.one_min_upnl,
                        expected["oneMinUpnl"].as_f64(),
                        "PNL-CLOCK native one-minute baseline"
                    );
                }
            }
            match scenario["divergence"].as_str() {
                Some("absent-prototype-fake") => {
                    let expected = step["expected"].as_array().unwrap();
                    assert_eq!(expected.len(), 1);
                    assert_eq!(expected[0]["ticker"], json!({}));
                    assert_eq!(expected[0]["market"], json!({}));
                    assert!(store.displayed_positions.is_empty());
                    continue;
                }
                Some("absent-function-fake") => {
                    assert_eq!(
                        step["expected"].as_array().unwrap().len(),
                        1,
                        "prototype function passes fake-position guard"
                    );
                    assert_eq!(step["lookupKinds"], json!([["function", "function"]]));
                    let serialized = step["expected"][0].as_object().unwrap();
                    assert!(!serialized.contains_key("ticker"));
                    assert!(!serialized.contains_key("market"));
                    assert!(
                        serialized.contains_key("id"),
                        "source retains the whole fake row"
                    );
                    assert!(
                        store.displayed_positions.is_empty(),
                        "CHOSEN typed output refuses the whole inherited-function fake row"
                    );
                    continue;
                }
                Some("inherited-scalar-ticker") => {
                    assert_eq!(step["expected"][0]["ticker"], json!(110));
                    assert_eq!(step["expected"][0]["unrealizedPnl"], Value::Null);
                    assert!(store.displayed_positions[0].ticker.is_none());
                    assert_eq!(store.displayed_positions[0].position.unrealized_pnl, 0.0);
                    continue;
                }
                None => {}
                Some(other) => panic!("unknown divergence {other}"),
            }
            if scenario["identity"] == true {
                let row = &store.displayed_positions[0];
                let expected = &step["expected"][0];
                macro_rules! check {
                    ($v:expr,$e:expr; $($wire:literal=>$field:ident),+ $(,)?) => {
                        $(assert_json(&json!(&$v.$field), &$e[$wire], concat!("DISPLAYED-IDENTITY native ", $wire));)+
                    };
                }
                check!(&row.position,expected; "id"=>id,"symbol"=>symbol,"entryPrice"=>entry_price,"contracts"=>contracts,"notional"=>notional,"leverage"=>leverage,"unrealizedPnl"=>unrealized_pnl,"liquidationPrice"=>liquidation_price);
                let ticker = row.ticker.as_ref().unwrap();
                check!(ticker,expected["ticker"]; "id"=>id,"symbol"=>symbol,"bid"=>bid,"ask"=>ask,"last"=>last,"mark"=>mark,"index"=>index,"percentage"=>percentage,"openInterest"=>open_interest,"fundingRate"=>funding_rate,"fundingTime"=>funding_time,"volume"=>volume,"quoteVolume"=>quote_volume);
                let market = row.market.as_ref().unwrap();
                check!(market,expected["market"]; "id"=>id,"symbol"=>symbol,"base"=>base,"quote"=>quote,"active"=>active);
                check!(&market.precision,expected["market"]["precision"]; "amount"=>amount,"price"=>price);
                check!(&market.limits.amount,expected["market"]["limits"]["amount"]; "min"=>min,"max"=>max);
                check!(&market.limits.leverage,expected["market"]["limits"]["leverage"]; "min"=>min,"max"=>max);
            }
            assert_json(
                &serde_json::to_value(store.displayed_positions).unwrap(),
                &step["expected"],
                &format!("scenario {i} step {j}"),
            );
        }
    }
    assert_eq!(compared_steps, 222, "DISPLAYED-CARDINALITY compared steps");
}
