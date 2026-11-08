use safe_cex_types::{ExecutionPositionSide, OrderSide, OrderStatus, OrderType};
use serde::Deserialize;
use std::{fs, path::Path};
use ts_rs::{Config, TS};

#[path = "../src/history.rs"]
mod history;
#[path = "support/history_output.rs"]
mod output_fixture;
#[allow(dead_code)]
#[path = "../src/strict_decode.rs"]
mod strict_decode;

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[test]
fn native_history_records_preserve_missing_zero_and_distinct_money_fields() {
    let minimal: history::HistoricalOrder = serde_json::from_str(r#"{"id":"order-1","symbol":"BTC","side":"buy","type":"market","status":"closed","amount":2,"filled":0,"createdTime":1}"#).unwrap();
    assert!(minimal.price.is_none());
    assert!(minimal.realized_pnl.is_none());
    assert_eq!(minimal.filled, 0.);
    let json = serde_json::to_value(&minimal).unwrap();
    assert!(json.get("price").is_none());
    assert!(json.get("realizedPnl").is_none());
    let mut full = minimal.clone();
    full.price = Some(0.);
    full.realized_pnl = Some(12.);
    full.fees = Some(2.);
    full.rebate = Some(0.5);
    let json = serde_json::to_value(&full).unwrap();
    assert_eq!(json["price"], 0.);
    assert_eq!(json["realizedPnl"], 12.);
    assert_eq!(json["fees"], 2.);
    assert_eq!(json["rebate"], 0.5);
    for key in [
        "price",
        "averagePrice",
        "positionSide",
        "reduceOnly",
        "realizedPnl",
        "fees",
        "rebate",
        "feeAsset",
        "rebateAsset",
        "updatedTime",
    ] {
        let mut invalid = json.clone();
        invalid[key] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<history::HistoricalOrder>(invalid).is_err(),
            "null {key}"
        );
    }
    assert!(
        serde_json::from_value::<history::HistoricalOrder>(serde_json::json!([
            "order-1", "BTC", "buy", "market", "closed", 2, 0, 1
        ]))
        .is_err()
    );
}

#[test]
fn history_generated_bindings_match_the_owned_rust_module() {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/history/raw");
    let config = Config::new().with_out_dir(&raw);
    fn check<T: TS + 'static>(raw: &Path, config: &Config, name: &str) {
        let generated = T::export_to_string(config).unwrap();
        let normalized = generated
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_eq!(
            normalized,
            fs::read_to_string(raw.join(format!("{name}.ts"))).unwrap(),
            "regenerate {name}"
        );
    }
    check::<history::HistoricalOrder>(&raw, &config, "HistoricalOrder");
    check::<history::HistoricalTrade>(&raw, &config, "HistoricalTrade");
    check::<history::ClosedPnlEntry>(&raw, &config, "ClosedPnlEntry");
    check::<history::IncomeRecord>(&raw, &config, "IncomeRecord");
    check::<history::DataProvenance>(&raw, &config, "DataProvenance");
    check::<history::IncomeCategory>(&raw, &config, "IncomeCategory");
}

#[test]
fn trade_identity_is_optional_and_never_copied_from_order_identity() {
    let trade: history::HistoricalTrade = serde_json::from_str(r#"{"symbol":"BTC","orderId":"order-1","price":1,"qty":0,"positionSide":"both","side":"sell","time":2}"#).unwrap();
    assert_eq!(trade.id, None);
    assert_eq!(trade.order_id, "order-1");
    assert_eq!(serde_json::to_value(&trade).unwrap().get("id"), None);
    let mut json = serde_json::to_value(&trade).unwrap();
    json["id"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<history::HistoricalTrade>(json).is_err());
}

#[test]
fn income_retains_raw_type_signed_amount_and_absent_execution_identity() {
    let row: history::IncomeRecord = serde_json::from_str(r#"{"symbol":"","incomeType":"VENUE_FEE","income":-2,"asset":"USD","timestamp":0,"tranId":"bill-1","category":"commission"}"#).unwrap();
    assert_eq!(row.income_type, "VENUE_FEE");
    assert_eq!(row.income, -2.);
    assert_eq!(row.category, Some(history::IncomeCategory::Commission));
    assert_eq!(row.trade_id, None);
    let json = serde_json::to_value(&row).unwrap();
    assert!(json.get("tradeId").is_none());
    for key in ["tradeId", "category"] {
        let mut invalid = json.clone();
        invalid[key] = serde_json::Value::Null;
        assert!(serde_json::from_value::<history::IncomeRecord>(invalid).is_err());
    }
    assert!(serde_json::from_str::<history::IncomeCategory>(r#""gross""#).is_err());
    assert!(serde_json::from_str::<history::IncomeRecord>("[]").is_err());
}

#[test]
fn consumer_fixture_is_the_native_serializers_output() {
    let emitted = serde_json::to_string_pretty(&output_fixture::fixture()).unwrap() + "\n";
    let committed = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/history-output.json"),
    )
    .unwrap();
    assert_eq!(emitted, committed);
    let closed = output_fixture::fixture().closed;
    assert!(matches!(
        closed[0].fees,
        history::ClosedPnlEntryFees::Variant1(0.)
    ));
    assert!(
        matches!(&closed[1].fees, history::ClosedPnlEntryFees::Variant0(value) if value == "0.00")
    );
    assert!(serde_json::from_str::<history::ClosedPnlEntry>("[]").is_err());
    let mut invalid = serde_json::to_value(&closed[0]).unwrap();
    invalid.as_object_mut().unwrap().remove("provenance");
    assert!(serde_json::from_value::<history::ClosedPnlEntry>(invalid).is_err());
}
