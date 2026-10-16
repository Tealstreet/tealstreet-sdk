use safe_cex_venue_asterdex::{helpers::*, types::*};
use serde_json::{Value, json};

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn helpers_match_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/helpers-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in data["guards"].as_array().unwrap() {
        assert_eq!(
            json!(is_aster_dex_stp_mode(&case["input"])),
            case["stp"],
            "isAsterDEXStpMode {case}"
        );
        assert_eq!(
            json!(is_aster_dex_main_wallet_error(&case["input"])),
            case["wallet"],
            "isAsterDEXMainWalletError {case}"
        );
    }
    for case in data["checks"].as_array().unwrap() {
        let input = case["input"].as_str();
        assert_eq!(
            json!(trim_non_empty(input)),
            case["trim"],
            "trimNonEmpty {case}"
        );
        assert_eq!(
            json!(is_aster_dex_address(input)),
            case["address"],
            "isAsterDEXAddress {case}"
        );
        assert_eq!(
            json!(is_aster_dex_v3_private_key(input)),
            case["key"],
            "isAsterDEXV3PrivateKey {case}"
        );
    }
    for case in data["options"].as_array().unwrap() {
        let options = serde_json::from_value(case["input"].clone()).unwrap();
        assert_eq!(
            json!(resolve_aster_dex_v3_auth_context(&options)),
            case["context"],
            "resolveAsterDEXV3AuthContext {case}"
        );
        assert_eq!(
            json!(has_aster_dex_v3_auth_context(&options)),
            case["has"],
            "hasAsterDEXV3AuthContext {case}"
        );
        assert_eq!(
            json!(resolve_aster_dex_api_version(&options)),
            case["version"],
            "resolveAsterDEXApiVersion {case}"
        );
    }
    for case in data["paths"].as_array().unwrap() {
        let version = serde_json::from_value(case["version"].clone()).unwrap();
        assert_eq!(
            json!(get_aster_dex_fapi_path(
                case["endpoint"].as_str().unwrap(),
                &version
            )),
            case["output"],
            "getAsterDEXFapiPath {case}"
        );
    }
    for case in data["pnl"].as_array().unwrap() {
        let trades: Vec<AsterDEXUserTradeResponse> =
            serde_json::from_value(case["input"].clone()).unwrap();
        let actual = normalize_aster_dex_realized_pnl_deltas(&trades);
        let expected: Vec<Option<f64>> = serde_json::from_value(case["output"].clone()).unwrap();
        assert_eq!(
            actual, expected,
            "normalizeAsterDEXRealizedPnlDeltas {case}"
        );
        assert_eq!(
            json!(
                trades
                    .iter()
                    .map(get_aster_dex_realized_pnl_key)
                    .collect::<Vec<_>>()
            ),
            case["keys"],
            "getAsterDEXRealizedPnlKey {case}"
        );
    }
}
