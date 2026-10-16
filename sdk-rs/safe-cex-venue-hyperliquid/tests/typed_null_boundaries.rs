use serde_json::Value;
fn decode(name: &str, raw: &str) -> Result<(), serde_json::Error> {
    match name {
        "OrderType" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::OrderType>(raw).map(|_| ())
        }
        "Meta" => serde_json::from_str::<safe_cex_venue_hyperliquid::types::Meta>(raw).map(|_| ()),
        "OrderResponseStatus" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::OrderResponseStatus>(raw)
                .map(|_| ())
        }
        "OrderResponse" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::OrderResponse>(raw)
                .map(|_| ())
        }
        "SpotClearinghouseBalance" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::SpotClearinghouseBalance>(raw)
                .map(|_| ())
        }
        "SpotClearinghouseState" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::SpotClearinghouseState>(raw)
                .map(|_| ())
        }
        "AssetCtx" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::AssetCtx>(raw).map(|_| ())
        }
        "MetaAndAssetCtxs" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::MetaAndAssetCtxs>(raw)
                .map(|_| ())
        }
        "SpotToken" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::SpotToken>(raw).map(|_| ())
        }
        "SpotMeta" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::SpotMeta>(raw).map(|_| ())
        }
        "SpotAssetCtx" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::SpotAssetCtx>(raw).map(|_| ())
        }
        "SpotMetaAndAssetCtxs" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::SpotMetaAndAssetCtxs>(raw)
                .map(|_| ())
        }
        "OrderRequest" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::OrderRequest>(raw).map(|_| ())
        }
        "OrderWire" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::OrderWire>(raw).map(|_| ())
        }
        "ModifyRequest" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::ModifyRequest>(raw)
                .map(|_| ())
        }
        "ModifyWire" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::ModifyWire>(raw).map(|_| ())
        }
        "ScheduleCancelAction" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::ScheduleCancelAction>(raw)
                .map(|_| ())
        }
        "PerpDex" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::PerpDex>(raw).map(|_| ())
        }
        "PerpDexs" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::PerpDexs>(raw).map(|_| ())
        }
        "Hip3MarketInfo" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::Hip3MarketInfo>(raw)
                .map(|_| ())
        }
        "BuilderFeeCheckResult" => {
            serde_json::from_str::<safe_cex_venue_hyperliquid::types::BuilderFeeCheckResult>(raw)
                .map(|_| ())
        }
        _ => panic!("unknown source null-boundary type {name}"),
    }
}
#[test]
fn compiler_rejected_null_and_missing_fields_are_rejected_by_public_decoders() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/typed-null-boundaries.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 150, "NULL-CARDINALITY source total");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["kind"] == "optional-null")
            .count(),
        138,
        "NULL-CARDINALITY optional-null total"
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["kind"] == "required-nullable-missing")
            .count(),
        12,
        "NULL-CARDINALITY required-nullable total"
    );
    assert!(
        cases
            .iter()
            .any(|c| c["name"] == "OrderType" && c["kind"] == "optional-null")
    );
    assert!(
        cases
            .iter()
            .any(|c| c["kind"] == "required-nullable-missing")
    );
    let mut comparisons = 0;
    for index in 0..150 {
        let case = cases
            .get(index)
            .expect("NULL-CARDINALITY native index coverage");
        assert_eq!(
            case["sourceAccepts"], false,
            "actual source rejects the input"
        );
        assert!(
            decode(
                case["name"].as_str().unwrap(),
                case["raw"].as_str().unwrap()
            )
            .is_err(),
            "public decoder accepted source-rejected {}/{} {}",
            case["name"],
            case["path"],
            case["kind"]
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 150, "NULL-CARDINALITY native comparisons");
}
#[test]
fn actual_source_null_checker_executes() {
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/typed-null-boundaries.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source null checker must execute {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn widened_source_context_cannot_make_rejection_oracle_green() {
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/typed-null-boundaries.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .arg("--control-widened-context")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "widened context must fail rather than pass trivially"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("OrderType optional null must be measured rejected"),
        "control must fail the named accepted-set assertion: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
