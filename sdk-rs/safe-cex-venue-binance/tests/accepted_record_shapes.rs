//! Independently authored objects and complete arrays in TS/Serde field order.
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_binance::{binance_types as futures, binancepm_types as pm};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

#[test]
fn futures_risk_vector_alias_preserves_native_rows() {
    let raw = r#"[{"symbol":"BTCUSDT","positionAmt":"2","entryPrice":"3","markPrice":"4","unRealizedProfit":"5","liquidationPrice":"6","leverage":"7","maxNotionalValue":"8","marginType":"cross","isolatedMargin":"10","isAutoAddMargin":"false","positionSide":"LONG","notional":"13","isolatedWallet":"14","updateTime":-0,"isolated":false,"adl":-0,"adlQuantile":18.5,"initialMargin":"19","positionInitialMargin":"20","openOrderInitialMargin":"21"},{"symbol":"ADAUSDT","positionAmt":"-22","entryPrice":"23","markPrice":"24","unRealizedProfit":"25","liquidationPrice":"26","isolatedMargin":"30","positionSide":"SHORT","notional":"33","isolatedWallet":"34","updateTime":2}]"#;
    let rows = serde_json::from_str::<futures::GetPositionRiskResponse>(raw)
        .expect("actual Futures vector alias decodes source rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].symbol.as_str(), rows[1].symbol.as_str()),
        ("BTCUSDT", "ADAUSDT")
    );
    assert_eq!(
        (rows[0].position_amt.as_str(), rows[1].position_amt.as_str()),
        ("2", "-22")
    );
    assert_eq!(
        (
            rows[0].position_side.as_str(),
            rows[1].position_side.as_str()
        ),
        ("LONG", "SHORT")
    );
    assert_eq!(rows[0].update_time.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(rows[0].adl.unwrap().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(rows[0].leverage.as_deref(), Some("7"));
    assert_eq!(rows[1].leverage, None);
    assert_eq!(rows[1].adl, None);
    assert!(
        serde_json::from_str::<futures::GetPositionRiskResponse>("[]")
            .unwrap()
            .is_empty()
    );
    for forbidden in ["{}", "null", "[{}]", "[null]", "[[]]"] {
        assert!(serde_json::from_str::<futures::GetPositionRiskResponse>(forbidden).is_err());
    }
}

#[test]
fn pm_risk_vector_alias_preserves_native_rows() {
    let raw = r#"[{"symbol":"ETHUSDT","positionAmt":"22","entryPrice":"23","markPrice":"24","unRealizedProfit":"25","liquidationPrice":"26","leverage":"27","maxNotionalValue":"28","marginType":"isolated","isolatedMargin":"30","isAutoAddMargin":"true","positionSide":"SHORT","notional":"33","isolatedWallet":"34","updateTime":-0,"isolated":true,"adlQuantile":-0},{"symbol":"BNBUSDT","positionAmt":"42","entryPrice":"43","markPrice":"44","unRealizedProfit":"45","liquidationPrice":"46","leverage":"47","maxNotionalValue":"48","marginType":"cross","isolatedMargin":"50","isAutoAddMargin":"false","positionSide":"LONG","notional":"53","isolatedWallet":"54","updateTime":7,"isolated":false,"adlQuantile":8}]"#;
    let rows = serde_json::from_str::<pm::GetPositionRiskResponse>(raw)
        .expect("actual PM vector alias decodes source rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].symbol.as_str(), rows[1].symbol.as_str()),
        ("ETHUSDT", "BNBUSDT")
    );
    assert_eq!(
        (rows[0].position_amt.as_str(), rows[1].position_amt.as_str()),
        ("22", "42")
    );
    assert_eq!(
        (rows[0].leverage.as_str(), rows[1].leverage.as_str()),
        ("27", "47")
    );
    assert_eq!(rows[0].update_time.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(rows[0].adl_quantile.to_bits(), (-0.0_f64).to_bits());
    assert!(!rows[1].isolated);
    assert!(
        serde_json::from_str::<pm::GetPositionRiskResponse>("[]")
            .unwrap()
            .is_empty()
    );
    for forbidden in ["{}", "null", "[{}]", "[null]", "[[]]"] {
        assert!(serde_json::from_str::<pm::GetPositionRiskResponse>(forbidden).is_err());
    }
}

fn decode<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    route: usize,
) -> Result<T, serde_json::Error> {
    match route {
        0 => serde_json::from_str(raw),
        1 => serde_json::from_slice(raw.as_bytes()),
        2 => serde_json::from_value(serde_json::from_str::<Value>(raw).unwrap()),
        3 => T::deserialize(&serde_json::from_str::<Value>(raw).unwrap()),
        4 => js_from_str(raw),
        5 => js_from_value(serde_json::from_str::<Value>(raw).unwrap()),
        6 => js_from_value_ref(&serde_json::from_str::<Value>(raw).unwrap()),
        _ => unreachable!(),
    }
}

fn record<T: DeserializeOwned + Serialize + JsNumberOutput>(
    object: &str,
    complete_array: &str,
    inspect: impl Fn(T),
) {
    for route in 0..7 {
        inspect(decode::<T>(object, route).unwrap());
        for forbidden in [complete_array, "[]", "null", "false", "42", r#""record""#] {
            assert!(
                decode::<T>(forbidden, route).is_err(),
                "complete valid positional array or non-object accepted: route={route} input={forbidden}"
            );
        }
    }
}

#[test]
fn futures_risk_rejects_complete_positional_array() {
    record::<futures::GetPositionRiskResponseRow>(
        r#"{"symbol":"BTCUSDT","positionAmt":"2","entryPrice":"3","markPrice":"4","unRealizedProfit":"5","liquidationPrice":"6","leverage":"7","maxNotionalValue":"8","marginType":"cross","isolatedMargin":"10","isAutoAddMargin":"false","positionSide":"LONG","notional":"13","isolatedWallet":"14","updateTime":-0,"isolated":false,"adl":-0,"adlQuantile":18.5,"initialMargin":"19","positionInitialMargin":"20","openOrderInitialMargin":"21"}"#,
        r#"["BTCUSDT","2","3","4","5","6","7","8","cross","10","false","LONG","13","14",-0,false,-0,18.5,"19","20","21"]"#,
        |r| {
            assert_eq!(
                (
                    r.symbol.as_str(),
                    r.position_amt.as_str(),
                    r.entry_price.as_str(),
                    r.mark_price.as_str(),
                    r.un_realized_profit.as_str(),
                    r.liquidation_price.as_str()
                ),
                ("BTCUSDT", "2", "3", "4", "5", "6")
            );
            assert_eq!(
                (
                    r.leverage.as_deref(),
                    r.max_notional_value.as_deref(),
                    r.margin_type.as_deref(),
                    r.is_auto_add_margin.as_deref()
                ),
                (Some("7"), Some("8"), Some("cross"), Some("false"))
            );
            assert_eq!(
                (
                    r.isolated_margin.as_str(),
                    r.position_side.as_str(),
                    r.notional.as_str(),
                    r.isolated_wallet.as_str()
                ),
                ("10", "LONG", "13", "14")
            );
            assert_eq!(r.update_time.to_bits(), (-0.0_f64).to_bits());
            assert_eq!(r.isolated, Some(false));
            assert_eq!(r.adl.unwrap().to_bits(), (-0.0_f64).to_bits());
            assert_eq!(r.adl_quantile, Some(18.5));
            assert_eq!(
                (
                    r.initial_margin.as_deref(),
                    r.position_initial_margin.as_deref(),
                    r.open_order_initial_margin.as_deref()
                ),
                (Some("19"), Some("20"), Some("21"))
            );
        },
    );
}

#[test]
fn pm_risk_rejects_complete_positional_array() {
    record::<pm::GetPositionRiskResponseRow>(
        r#"{"symbol":"ETHUSDT","positionAmt":"22","entryPrice":"23","markPrice":"24","unRealizedProfit":"25","liquidationPrice":"26","leverage":"27","maxNotionalValue":"28","marginType":"isolated","isolatedMargin":"30","isAutoAddMargin":"true","positionSide":"SHORT","notional":"33","isolatedWallet":"34","updateTime":-0,"isolated":true,"adlQuantile":-0}"#,
        r#"["ETHUSDT","22","23","24","25","26","27","28","isolated","30","true","SHORT","33","34",-0,true,-0]"#,
        |r| {
            assert_eq!(
                (
                    r.symbol.as_str(),
                    r.position_amt.as_str(),
                    r.entry_price.as_str(),
                    r.mark_price.as_str(),
                    r.un_realized_profit.as_str(),
                    r.liquidation_price.as_str()
                ),
                ("ETHUSDT", "22", "23", "24", "25", "26")
            );
            assert_eq!(
                (
                    r.leverage.as_str(),
                    r.max_notional_value.as_str(),
                    r.margin_type.as_str(),
                    r.isolated_margin.as_str(),
                    r.is_auto_add_margin.as_str()
                ),
                ("27", "28", "isolated", "30", "true")
            );
            assert_eq!(
                (
                    r.position_side.as_str(),
                    r.notional.as_str(),
                    r.isolated_wallet.as_str()
                ),
                ("SHORT", "33", "34")
            );
            assert_eq!(r.update_time.to_bits(), (-0.0_f64).to_bits());
            assert!(r.isolated);
            assert_eq!(r.adl_quantile.to_bits(), (-0.0_f64).to_bits());
        },
    );
}

#[test]
fn income_record_rejects_complete_positional_array() {
    record::<futures::BinanceIncomeRecord>(
        r#"{"symbol":"SOLUSDT","incomeType":"FUNDING_FEE","income":"-1.5","asset":"USDT","info":"note","time":-0,"tranId":"9007199254740993","tradeId":"trade-8"}"#,
        r#"["SOLUSDT","FUNDING_FEE","-1.5","USDT","note",-0,"9007199254740993","trade-8"]"#,
        |r| {
            assert_eq!(
                (
                    r.symbol.as_str(),
                    r.income.as_str(),
                    r.asset.as_str(),
                    r.tran_id.as_str()
                ),
                ("SOLUSDT", "-1.5", "USDT", "9007199254740993")
            );
            assert_eq!(r.income_type, futures::BinanceIncomeType::FundingFee);
            assert_eq!(r.info.as_deref(), Some("note"));
            assert_eq!(r.trade_id.as_deref(), Some("trade-8"));
            assert_eq!(r.time.to_bits(), (-0.0_f64).to_bits());
        },
    );
}

#[test]
fn income_options_rejects_complete_positional_array() {
    record::<futures::BinanceIncomeHistoryOptions>(
        r#"{"symbol":"XRPUSDT","incomeType":"COMMISSION","startTime":-0,"endTime":123.5,"limit":17.25}"#,
        r#"["XRPUSDT","COMMISSION",-0,123.5,17.25]"#,
        |r| {
            assert_eq!(r.symbol.as_deref(), Some("XRPUSDT"));
            assert_eq!(r.income_type, Some(futures::BinanceIncomeType::COMMISSION));
            assert_eq!(r.start_time.unwrap().to_bits(), (-0.0_f64).to_bits());
            assert_eq!(r.end_time, Some(123.5));
            assert_eq!(r.limit, Some(17.25));
        },
    );
    for route in 0..7 {
        let empty: futures::BinanceIncomeHistoryOptions = decode("{}", route).unwrap();
        assert_eq!(
            (
                empty.symbol,
                empty.income_type,
                empty.start_time,
                empty.end_time,
                empty.limit
            ),
            (None, None, None, None, None)
        );
        for field in ["symbol", "incomeType", "startTime", "endTime", "limit"] {
            assert!(
                decode::<futures::BinanceIncomeHistoryOptions>(
                    &format!(r#"{{"{field}":null}}"#),
                    route
                )
                .is_err(),
                "present null is not absent: {field} route={route}"
            );
        }
    }
}
