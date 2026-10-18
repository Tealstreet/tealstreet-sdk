use safe_cex_venue_binance::{
    binance_types as futures, binancepm_types as pm, binancespot_types as spot,
};
use serde::Serialize;
use serde_json::{Value, json};

fn js_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().expect("finite source number")),
        Value::Array(items) => Value::Array(items.into_iter().map(js_numbers).collect()),
        Value::Object(items) => {
            Value::Object(items.into_iter().map(|(k, v)| (k, js_numbers(v))).collect())
        }
        other => other,
    }
}
fn object<T: Serialize>(rows: &[(&str, T)]) -> Value {
    rows.iter()
        .map(|(key, value)| (key.to_string(), serde_json::to_value(value).unwrap()))
        .collect()
}
fn hosts(rows: &[(&str, &[(&str, &str)])]) -> Value {
    rows.iter()
        .map(|(key, value)| (key.to_string(), object(value)))
        .collect()
}
macro_rules! constants {
    ($module:ident) => {
        json!({
            "RECV_WINDOW": $module::RECV_WINDOW,
            "BASE_WS_URL": hosts($module::BASE_WS_URL),
            "ENDPOINTS": object($module::ENDPOINTS),
            "PUBLIC_ENDPOINTS": $module::PUBLIC_ENDPOINTS,
            "GRAFANA_ENDPOINTS_EVERY": $module::GRAFANA_ENDPOINTS_EVERY,
            "GRAFANA_ENDPOINTS_PERIODIC": $module::GRAFANA_ENDPOINTS_PERIODIC,
            "ORDER_TYPE": object($module::ORDER_TYPE),
            "ORDER_SIDE": object($module::ORDER_SIDE),
            "POSITION_SIDE": object($module::POSITION_SIDE),
            "TIME_IN_FORCE": object($module::TIME_IN_FORCE),
        })
    };
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn compiled_tables_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!("{}/scripts/types.mjs", env!("CARGO_MANIFEST_DIR")))
        .arg("--oracle")
        .output()
        .expect("Node oracle");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = js_numbers(serde_json::from_slice(&out.stdout).unwrap());
    let mut usd = constants!(futures);
    usd["BASE_URL"] = object(futures::BASE_URL);
    usd["ORDER_TRIGGER_TYPE"] = object(futures::ORDER_TRIGGER_TYPE);
    let mut portfolio = constants!(pm);
    portfolio["BASE_PAPI_URL"] = object(pm::BASE_PAPI_URL);
    portfolio["BASE_FAPI_URL"] = object(pm::BASE_FAPI_URL);
    portfolio["ORDER_TRIGGER_TYPE"] = object(pm::ORDER_TRIGGER_TYPE);
    let mut spot = constants!(spot);
    spot["BASE_URL"] = object(spot::BASE_URL);
    spot["ORDER_TYPE_SPOT"] = object(spot::ORDER_TYPE_SPOT);
    spot["USER_DATA_RECV_WINDOW"] = json!(spot::USER_DATA_RECV_WINDOW);
    assert_eq!(
        json!({"binance":usd,"binancepm":portfolio,"binancespot":spot}),
        expected
    );
}
#[test]
fn optional_risk_fields_and_income_wire_spellings_roundtrip() {
    let minimal = json!({"symbol":"BTCUSDT","positionAmt":"0","entryPrice":"0","markPrice":"1","unRealizedProfit":"0","liquidationPrice":"0","isolatedMargin":"0","positionSide":"BOTH","notional":"0","isolatedWallet":"0","updateTime":0.0});
    let row: futures::GetPositionRiskResponseRow = serde_json::from_value(minimal.clone()).unwrap();
    assert!(row.leverage.is_none());
    assert_eq!(serde_json::to_value(row).unwrap(), minimal);
    assert!(serde_json::from_value::<pm::GetPositionRiskResponseRow>(minimal).is_err());
    let income = json!({"symbol":"BTCUSDT","incomeType":"DELIVERED_SETTELMENT","income":"1","asset":"USDT","time":123.0,"tranId":"9007199254740993"});
    let row: futures::BinanceIncomeRecord = serde_json::from_value(income.clone()).unwrap();
    assert_eq!(serde_json::to_value(row).unwrap(), income);
}

#[test]
fn optional_fields_distinguish_absence_from_present_null() {
    let minimal = json!({"symbol":"BTCUSDT","positionAmt":"0","entryPrice":"0","markPrice":"1","unRealizedProfit":"0","liquidationPrice":"0","isolatedMargin":"0","positionSide":"BOTH","notional":"0","isolatedWallet":"0","updateTime":0});
    for key in [
        "leverage",
        "maxNotionalValue",
        "marginType",
        "isAutoAddMargin",
        "isolated",
        "adl",
        "adlQuantile",
        "initialMargin",
        "positionInitialMargin",
        "openOrderInitialMargin",
    ] {
        let mut row = minimal.clone();
        row[key] = Value::Null;
        assert!(
            serde_json::from_value::<futures::GetPositionRiskResponseRow>(row).is_err(),
            "accepted null {key}"
        );
    }
    for key in ["symbol", "incomeType", "startTime", "endTime", "limit"] {
        assert!(serde_json::from_value::<futures::BinanceIncomeHistoryOptions>(json!({})).is_ok());
        let mut row = json!({});
        row[key] = Value::Null;
        assert!(
            serde_json::from_value::<futures::BinanceIncomeHistoryOptions>(row).is_err(),
            "accepted null {key}"
        );
    }
    for key in ["info", "tradeId"] {
        let mut row = json!({"symbol":"BTCUSDT","incomeType":"COMMISSION","income":"1","asset":"USDT","time":123,"tranId":"7"});
        assert!(serde_json::from_value::<futures::BinanceIncomeRecord>(row.clone()).is_ok());
        row[key] = Value::Null;
        assert!(
            serde_json::from_value::<futures::BinanceIncomeRecord>(row).is_err(),
            "accepted null {key}"
        );
    }
    let row =
        json!({"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","startTime":0,"endTime":1,"limit":1});
    assert!(serde_json::from_value::<futures::BinanceIncomeHistoryOptions>(row).is_ok());
}

#[test]
fn qualified_decoders_reject_non_object_records_and_non_string_unions() {
    fn rejects<T: serde::de::DeserializeOwned>(values: &[Value]) {
        for value in values {
            assert!(
                <T as serde::Deserialize>::deserialize(value.clone()).is_err(),
                "accepted {value}"
            );
        }
    }
    let invalid = [
        json!([]),
        json!(null),
        json!(false),
        json!(1),
        json!("record"),
    ];
    rejects::<futures::GetPositionRiskResponseRow>(&invalid);
    rejects::<pm::GetPositionRiskResponseRow>(&invalid);
    rejects::<futures::BinanceIncomeRecord>(&invalid);
    rejects::<futures::BinanceIncomeHistoryOptions>(&invalid);
    rejects::<futures::BinanceIncomeHistoryOptions>(&[json!([null, null, null, null, null])]);
    rejects::<futures::BinanceIncomeRecord>(&[json!([
        "BTCUSDT",
        "COMMISSION",
        "1",
        "USDT",
        123,
        "7"
    ])]);
    rejects::<futures::GetPositionRiskResponseRow>(&[json!([
        "BTCUSDT", "0", "0", "1", "0", "0", "0", "BOTH", "0", "0", 0
    ])]);
    rejects::<futures::BinanceIncomeType>(&[
        json!({"COMMISSION":null}),
        json!(["COMMISSION"]),
        json!(null),
        json!(1),
        json!(false),
        json!("commission"),
        json!(" COMMISSION"),
        json!("COMMISSION "),
        json!("\u{feff}COMMISSION"),
        json!("UNKNOWN"),
    ]);
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn income_union_accepted_set_matches_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!("{}/scripts/types.mjs", env!("CARGO_MANIFEST_DIR")))
        .arg("--schema")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let schema: Value = serde_json::from_slice(&out.stdout).unwrap();
    let values = schema["BinanceIncomeType"].as_array().unwrap();
    for value in values {
        let decoded =
            <futures::BinanceIncomeType as serde::Deserialize>::deserialize(value.clone()).unwrap();
        let name = value.as_str().unwrap();
        let native = match name {
            "TRANSFER" => futures::BinanceIncomeType::TRANSFER,
            "WELCOME_BONUS" => futures::BinanceIncomeType::WelcomeBonus,
            "REALIZED_PNL" => futures::BinanceIncomeType::RealizedPnl,
            "FUNDING_FEE" => futures::BinanceIncomeType::FundingFee,
            "COMMISSION" => futures::BinanceIncomeType::COMMISSION,
            "INSURANCE_CLEAR" => futures::BinanceIncomeType::InsuranceClear,
            "REFERRAL_KICKBACK" => futures::BinanceIncomeType::ReferralKickback,
            "COMMISSION_REBATE" => futures::BinanceIncomeType::CommissionRebate,
            "API_REBATE" => futures::BinanceIncomeType::ApiRebate,
            "CONTEST_REWARD" => futures::BinanceIncomeType::ContestReward,
            "CROSS_COLLATERAL_TRANSFER" => futures::BinanceIncomeType::CrossCollateralTransfer,
            "OPTIONS_PREMIUM_FEE" => futures::BinanceIncomeType::OptionsPremiumFee,
            "OPTIONS_SETTLE_PROFIT" => futures::BinanceIncomeType::OptionsSettleProfit,
            "INTERNAL_TRANSFER" => futures::BinanceIncomeType::InternalTransfer,
            "AUTO_EXCHANGE" => futures::BinanceIncomeType::AutoExchange,
            "DELIVERED_SETTELMENT" => futures::BinanceIncomeType::DeliveredSettelment,
            "COIN_SWAP_DEPOSIT" => futures::BinanceIncomeType::CoinSwapDeposit,
            "COIN_SWAP_WITHDRAW" => futures::BinanceIncomeType::CoinSwapWithdraw,
            "POSITION_LIMIT_INCREASE_FEE" => futures::BinanceIncomeType::PositionLimitIncreaseFee,
            _ => panic!("unclassified income union {name}"),
        };
        assert_eq!(
            decoded, native,
            "decoded income union native identity {name}"
        );
        assert_eq!(serde_json::to_value(decoded).unwrap(), *value);
        for wrong in [
            json!({name:null}),
            json!([name]),
            json!(name.to_lowercase()),
            json!(format!(" {name}")),
            json!(format!("{name} ")),
            json!(format!("\u{feff}{name}")),
        ] {
            assert!(
                <futures::BinanceIncomeType as serde::Deserialize>::deserialize(wrong.clone())
                    .is_err(),
                "accepted {wrong}"
            );
        }
    }
    // Serde's unknown_variant diagnostic inventories the private derive's accepted literals, including aliases.
    let error = <futures::BinanceIncomeType as serde::Deserialize>::deserialize(json!("unknown"))
        .unwrap_err()
        .to_string();
    let expected = values
        .iter()
        .map(|v| format!("`{}`", v.as_str().unwrap()))
        .collect::<Vec<_>>()
        .join(", ");
    assert_eq!(
        error,
        format!("unknown variant `unknown`, expected one of {expected}")
    );
}
