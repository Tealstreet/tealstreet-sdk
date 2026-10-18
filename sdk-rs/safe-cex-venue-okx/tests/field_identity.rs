use safe_cex_num::JsCoercionError;
#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_types::Market;
use safe_cex_venue_okx::{closed_pnl, income, order_history};
use serde_json::{Value, json};
fn canonical(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            serde_json::from_str(&safe_cex_num::js_number_string(n.as_f64().unwrap())).unwrap()
        }
        Value::Array(a) => Value::Array(a.into_iter().map(canonical).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, canonical(v))).collect()),
        v => v,
    }
}
#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn field_identity_supplement_distinguishes_mapper_fields() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/field-identity-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4);
    cardinality::assert_groups(
        &cases,
        "field_identity operations",
        |case| case["op"].as_str().unwrap().to_owned(),
        &[("history", 1), ("closed", 1), ("income", 1), ("journal", 1)],
    );
    for case in cases {
        let row = case["row"].as_object().unwrap();
        let market: Market = serde_json::from_value(case["market"].clone()).unwrap();
        let actual = match case["op"].as_str().unwrap() {
            "history" => {
                let model =
                    order_history::map_okx_historical_order(row, Some(&market), |n, m, _| {
                        Ok::<_, JsCoercionError>(n * m.contract_size.unwrap())
                    })
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    model.amount.to_bits(),
                    140_f64.to_bits(),
                    "native history amount identity"
                );
                assert_eq!(
                    model.filled.to_bits(),
                    90_f64.to_bits(),
                    "native history filled identity"
                );
                assert_eq!(
                    model.price.unwrap().to_bits(),
                    0.081_f64.to_bits(),
                    "native history price identity"
                );
                assert_eq!(
                    model.average_price.unwrap().to_bits(),
                    0.072_f64.to_bits(),
                    "native history average price identity"
                );
                assert_eq!(
                    model.created_time.to_bits(),
                    1735689600000_f64.to_bits(),
                    "native history created time identity"
                );
                assert_eq!(
                    model.updated_time.unwrap().to_bits(),
                    1735689660123_f64.to_bits(),
                    "native history updated time identity"
                );
                json!(model)
            }
            "closed" => {
                let model = closed_pnl::map_okx_closed_position(
                    row,
                    Some(&market),
                    |n, m, _| Ok::<_, JsCoercionError>(n * m.contract_size.unwrap()),
                    "IDENTITY-ACCOUNT",
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    model.time.to_bits(),
                    1735689600000_f64.to_bits(),
                    "native closed open time identity"
                );
                assert_eq!(
                    model.closed_time.to_bits(),
                    1735689660123_f64.to_bits(),
                    "native closed close time identity"
                );
                match model.entry_price {
                    safe_cex_types::ClosedPnlEntryEntryPrice::Variant1(n) => assert_eq!(
                        n.to_bits(),
                        0.082_f64.to_bits(),
                        "native closed entry price identity"
                    ),
                    _ => panic!("expected native closed entry price"),
                };
                match model.exit_price {
                    safe_cex_types::ClosedPnlEntryExitPrice::Variant1(n) => assert_eq!(
                        n.to_bits(),
                        0.071_f64.to_bits(),
                        "native closed exit price identity"
                    ),
                    _ => panic!("expected native closed exit price"),
                };
                json!(model)
            }
            "income" => {
                let records = income::map_okx_bill_to_income_records(row, |_| {
                    Ok::<_, JsCoercionError>(Some(market.symbol.clone()))
                })
                .unwrap();
                assert_eq!(
                    records[0].income.to_bits(),
                    0.91_f64.to_bits(),
                    "native income pnl identity"
                );
                assert_eq!(
                    records[1].income.to_bits(),
                    (-0.017_f64).to_bits(),
                    "native income fee identity"
                );
                assert_eq!(
                    records[0].trade_id.as_deref(),
                    Some("TRADE-2"),
                    "native income trade id identity"
                );
                assert_eq!(
                    records[0].tran_id, "BILL-1:pnl",
                    "native income bill id identity"
                );
                json!(records)
            }
            "journal" => {
                let deps = safe_cex_venue_okx::position_adapter::OkxPositionAdapterDeps {
                    account: "IDENTITY-ACCOUNT",
                    to_base: &|n, _| Ok::<_, JsCoercionError>(n * 1000.),
                    resolve_symbol: &|_| Ok::<_, JsCoercionError>(Some("DOGEUSDC".into())),
                    resolve_settle_currency: &|_| Ok::<_, JsCoercionError>(Some("USDC".into())),
                };
                let native =
                    safe_cex_venue_okx::position_adapter::map_okx_position_history(row, &deps)
                        .unwrap()
                        .unwrap();
                let closed_size = match &native["closedSize"] {
                    safe_cex_num::JsJsonValue::Number(value) => *value,
                    _ => panic!("OKX journal native closedSize numeric kind"),
                };
                assert_eq!(
                    closed_size.to_bits(),
                    420.0f64.to_bits(),
                    "OKX journal native closedSize identity"
                );
                let value = safe_cex_num::JsJsonValue::Object(native.into_iter().collect());
                serde_json::from_str(&safe_cex_num::js_json_stringify(&value).unwrap().unwrap())
                    .unwrap()
            }
            _ => panic!("unknown identity supplement"),
        };
        assert_eq!(
            canonical(actual),
            canonical(case["expected"].clone()),
            "field identity supplement {}",
            case["op"]
        );
    }
}

#[test]
fn decoded_native_field_identity_cannot_cancel_through_serialization() {
    use safe_cex_venue_okx::{
        exchange::TickerMarkInput,
        types::{OkxInstrumentType, OkxRegion},
    };
    let ticker: TickerMarkInput =
        serde_json::from_str(r#"{"id":"VENUE-ID","symbol":"DISPLAY-SYMBOL","mark":1.25}"#).unwrap();
    assert_eq!(
        serde_json::to_value(&ticker).unwrap(),
        json!({"id":"VENUE-ID","symbol":"DISPLAY-SYMBOL","mark":1.25}),
        "roundtrip alone is not native identity"
    );
    assert_eq!(
        ticker.id.as_deref(),
        Some("VENUE-ID"),
        "native ticker id identity"
    );
    assert_eq!(
        ticker.symbol.as_deref(),
        Some("DISPLAY-SYMBOL"),
        "native ticker symbol identity"
    );
    assert_eq!(
        ticker.mark.unwrap().to_bits(),
        1.25_f64.to_bits(),
        "native ticker mark identity"
    );
    assert_eq!(
        serde_json::from_str::<OkxRegion>("\"okx\"").unwrap(),
        OkxRegion::Okx,
        "native global region identity"
    );
    assert_eq!(
        serde_json::from_str::<OkxRegion>("\"okxeu\"").unwrap(),
        OkxRegion::Okxeu,
        "native EEA region identity"
    );
    assert_eq!(
        serde_json::from_str::<OkxInstrumentType>("\"SWAP\"").unwrap(),
        OkxInstrumentType::Swap,
        "native swap instrument identity"
    );
    assert_eq!(
        serde_json::from_str::<OkxInstrumentType>("\"FUTURES\"").unwrap(),
        OkxInstrumentType::Futures,
        "native futures instrument identity"
    );
}
