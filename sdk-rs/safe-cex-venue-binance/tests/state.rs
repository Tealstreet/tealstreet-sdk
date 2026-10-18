use safe_cex_num::{js_number_string, number_from_string};
use safe_cex_venue_binance::{reconciliation as state, spot_valuation};
use serde_json::Value;
fn spelling(n: f64) -> String {
    if n == 0.0 && n.is_sign_negative() {
        "-0".into()
    } else {
        js_number_string(n)
    }
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn valuation_and_reconciliation_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!("{}/scripts/state.mjs", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 7174, "SOURCE-CARDINALITY state total");
    let mut completed_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let number = |key: &str| number_from_string(case[key].as_str().unwrap());
        match case["kind"].as_str().unwrap() {
            "pause" => {
                let mut until = number("before");
                let now = number("now");
                state::pause_background_reconciliation(&mut until, now, number("duration"));
                assert_eq!(
                    spelling(until),
                    case["expected"].as_str().unwrap(),
                    "pause {index}: {case}"
                );
                assert_eq!(
                    state::is_background_reconciliation_paused(until, now),
                    case["paused"].as_bool().unwrap(),
                    "paused {index}: {case}"
                );
            }
            "version" => {
                let mut version = number("before");
                let mut actual = vec![spelling(state::get_position_state_version(version))];
                for _ in 0..3 {
                    state::mark_position_state_updated_by_websocket(&mut version);
                    actual.push(spelling(state::get_position_state_version(version)));
                }
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    case["expected"],
                    "version {index}"
                );
            }
            "price" => {
                let asset = case["asset"].as_str().unwrap();
                let mut calls = vec![];
                let actual = spot_valuation::get_asset_usd_price(asset, |symbol| {
                    calls.push(symbol.to_owned());
                    let at = ["USDT", "USDC", "USD"]
                        .iter()
                        .position(|q| symbol == format!("{asset}{q}"))
                        .unwrap();
                    let text = case["quotes"][at].as_str().unwrap();
                    Ok::<_, std::convert::Infallible>(
                        (text != "undefined").then(|| number_from_string(text)),
                    )
                })
                .unwrap();
                assert_eq!(
                    spelling(actual),
                    case["expected"].as_str().unwrap(),
                    "price {index}: {case}"
                );
                assert_eq!(
                    serde_json::to_value(calls).unwrap(),
                    case["calls"],
                    "lookup order {index}: {case}"
                );
            }
            _ => panic!("unknown case"),
        }
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 7174,
        "NATIVE-CARDINALITY state completed comparisons"
    );
    eprintln!("{} live valuation/state cases", cases.len());
}
