use safe_cex_num::js_number_string;
use safe_cex_types::*;
use safe_cex_venue_binance::closed_pnl::derive_binance_closed_positions;
use serde_json::{Value, json};

fn number(value: f64) -> Value {
    json!({"number":if value == 0.0 && value.is_sign_negative() { "-0".into() } else { js_number_string(value) }})
}
fn normalize(entry: ClosedPnlEntry) -> Value {
    assert_eq!(
        entry.provenance,
        DataProvenance::Derived,
        "native closed PnL provenance"
    );
    macro_rules! numeric {
        ($field:ident, $ty:ident, $variant:ident) => {
            match entry.$field {
                $ty::$variant(value) => number(value),
                _ => panic!("expected numeric {}", stringify!($field)),
            }
        };
    }
    json!({
        "provenance":entry.provenance,"symbol":entry.symbol,"account":entry.account,
        "side":entry.side,"time":number(entry.time),"closedTime":number(entry.closed_time),
        "notional":numeric!(notional,ClosedPnlEntryNotional,Variant1),
        "contracts":numeric!(contracts,ClosedPnlEntryContracts,Variant1),
        "entryPrice":numeric!(entry_price,ClosedPnlEntryEntryPrice,Variant1),
        "exitPrice":numeric!(exit_price,ClosedPnlEntryExitPrice,Variant1),
        "unrealizedPnl":numeric!(unrealized_pnl,ClosedPnlEntryUnrealizedPnl,Variant1),
        "fees":numeric!(fees,ClosedPnlEntryFees,Variant1),
        "leverage":numeric!(leverage,ClosedPnlEntryLeverage,Variant0)
    })
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn closed_positions_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/closed-pnl.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 7239, "SOURCE-CARDINALITY closed_pnl total");
    let mut completed_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let rows: Vec<serde_json::Map<String, Value>> =
            serde_json::from_str(case["rows"].as_str().unwrap()).unwrap();
        let actual = match derive_binance_closed_positions(&rows, case["account"].as_str().unwrap())
        {
            Ok(entries) => json!({"value":entries.into_iter().map(normalize).collect::<Vec<_>>()}),
            Err(error) => json!({"error":error.name}),
        };
        assert_eq!(actual, case["expected"], "case {index}: {case}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 7239,
        "NATIVE-CARDINALITY closed_pnl completed comparisons"
    );
    eprintln!("{} live closed-position comparisons", cases.len());
}
