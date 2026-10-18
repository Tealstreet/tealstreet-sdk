use safe_cex_types::*;
use safe_cex_venue_bybit::{fee_overlay::*, ledger_mapping::*};
use serde_json::Value;
fn project_category(value: Option<&Value>) -> Result<IncomeCategory, String> {
    use safe_cex_exchange::js_runtime::value::{Js, from_json, to_typed};
    bybit_transaction_category(&value.map(from_json).unwrap_or(Js::Undefined))
        .and_then(|v| to_typed(&v))
        .map_err(|e| e.to_exchange_error().message)
}
fn number(actual: f64, expected: &Value, name: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "LEDGER native {name}"
    );
}
fn optional(actual: Option<f64>, expected: &Value, name: &str) {
    if expected.is_null() || expected["undefined"] == true {
        assert!(actual.is_none(), "LEDGER native absent {name}");
    } else {
        number(actual.expect(name), expected, name);
    }
}
fn outcome<T, E: AsRef<str>>(actual: Result<T, E>, expected: &Value) -> Option<T> {
    if let Some(message) = expected["message"].as_str() {
        assert_eq!(
            actual.err().map(|e| e.as_ref().to_owned()),
            Some(message.to_owned()),
            "LEDGER native coercion error"
        );
        None
    } else if expected["refusal"] == true {
        assert!(actual.is_err(), "LEDGER native typed refusal: {expected}");
        None
    } else {
        Some(actual.unwrap_or_else(|e| panic!("{}", e.as_ref())))
    }
}
fn fixtures() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ledger-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn ledger_native_source_parity() {
    let f = fixtures();
    assert_eq!(
        f["scalars"].as_array().unwrap().len(),
        102,
        "LEDGER independent scalar floor"
    );
    for c in f["scalars"].as_array().unwrap() {
        let value = c["raw"]
            .as_str()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        let e = &c["result"];
        match c["name"].as_str().unwrap() {
            "toNumber" => {
                let native = value
                    .as_ref()
                    .map(safe_cex_exchange::js_runtime::value::from_json)
                    .unwrap_or(safe_cex_exchange::js_runtime::value::Js::Undefined);
                if let Some(v) = outcome(
                    to_number(&native).map_err(|e| e.to_exchange_error().message),
                    e,
                ) {
                    number(v, &e["ok"], "toNumber");
                }
            }
            "toOptionalNumber" => {
                if let Some(v) = outcome(
                    to_optional_number(
                        &value
                            .as_ref()
                            .map(safe_cex_exchange::js_runtime::value::from_json)
                            .unwrap_or(safe_cex_exchange::js_runtime::value::Js::Undefined),
                    )
                    .map(|v| {
                        if v.is_undefined() {
                            None
                        } else {
                            Some(v.to_number().unwrap())
                        }
                    })
                    .map_err(|e| e.to_exchange_error().message),
                    e,
                ) {
                    optional(v, &e["ok"], "toOptionalNumber");
                }
            }
            "toTimestamp" => {
                let native = value
                    .as_ref()
                    .map(safe_cex_exchange::js_runtime::value::from_json)
                    .unwrap_or(safe_cex_exchange::js_runtime::value::Js::Undefined);
                if let Some(v) = outcome(
                    closed_timestamp(&native).map_err(|e| e.to_exchange_error().message),
                    e,
                ) {
                    number(v, &e["ok"], "toTimestamp");
                }
            }
            "bybitTransactionCategory" => {
                if let Some(v) = outcome(project_category(value.as_ref()), e) {
                    assert_eq!(
                        v,
                        serde_json::from_value::<IncomeCategory>(e["ok"].clone()).unwrap(),
                        "LEDGER native category"
                    );
                }
            }
            _ => panic!("unrecognized source helper"),
        }
    }
    assert_eq!(
        f["categories"].as_array().unwrap().len(),
        9,
        "LEDGER independent category floor"
    );
    for c in f["categories"].as_array().unwrap() {
        let e = &c["result"];
        if let Some(v) = outcome(project_category(Some(&c["type"])), e) {
            assert_eq!(
                v,
                serde_json::from_value::<IncomeCategory>(e["ok"].clone()).unwrap(),
                "LEDGER native named category"
            );
        }
    }
    assert!(
        project_category(Some(&Value::String("constructor".into()))).is_err(),
        "LEDGER native inherited category refusal"
    );
    assert_eq!(f["inherited"]["refusal"], true);
    assert_eq!(
        f["orders"].as_array().unwrap().len(),
        25,
        "LEDGER independent order floor"
    );
    for c in f["orders"].as_array().unwrap() {
        let e = &c["result"];
        let native =
            map_bybit_historical_order(&safe_cex_exchange::js_runtime::value::from_json(&c["row"]));
        if e["ok"]["positionSide"]["inherited"] == true {
            assert!(
                native
                    .as_ref()
                    .unwrap()
                    .get("positionSide")
                    .unwrap()
                    .is_callable(),
                "native mapper must retain the inherited optional function before projection"
            );
        }
        let Some(actual) = outcome(
            native
                .and_then(|v| {
                    if v.is_nullish() {
                        Ok(None)
                    } else {
                        safe_cex_exchange::js_runtime::value::to_typed::<HistoricalOrder>(&v)
                            .map(Some)
                    }
                })
                .map_err(|e| e.to_exchange_error().message),
            e,
        ) else {
            continue;
        };
        let e = &e["ok"];
        if e.is_null() {
            assert!(actual.is_none(), "LEDGER native rejected order");
            continue;
        }
        let r = actual.unwrap();
        assert_eq!(r.id, e["id"].as_str().unwrap(), "LEDGER native order id");
        assert_eq!(
            r.symbol,
            e["symbol"].as_str().unwrap(),
            "LEDGER native order symbol"
        );
        assert_eq!(
            r.side,
            serde_json::from_value::<OrderSide>(e["side"].clone()).unwrap(),
            "LEDGER native order side"
        );
        assert_eq!(
            r.r#type,
            serde_json::from_value::<OrderType>(e["type"].clone()).unwrap(),
            "LEDGER native order type"
        );
        assert_eq!(
            r.status,
            serde_json::from_value::<OrderStatus>(e["status"].clone()).unwrap(),
            "LEDGER native order status"
        );
        optional(r.price, &e["price"], "order price");
        optional(r.average_price, &e["averagePrice"], "order averagePrice");
        number(r.amount, &e["amount"], "order amount");
        number(r.filled, &e["filled"], "order filled");
        assert_eq!(
            r.reduce_only,
            Some(e["reduceOnly"].as_bool().unwrap()),
            "LEDGER native order reduceOnly"
        );
        let position = if e["positionSide"]["undefined"] == true
            || e["positionSide"]["inherited"] == true
        {
            None
        } else {
            Some(
                serde_json::from_value::<ExecutionPositionSide>(e["positionSide"].clone()).unwrap(),
            )
        };
        assert_eq!(
            r.position_side, position,
            "LEDGER native order positionSide"
        );
        number(r.created_time, &e["createdTime"], "order createdTime");
        optional(r.updated_time, &e["updatedTime"], "order updatedTime");
        assert!(
            r.realized_pnl.is_none()
                && r.fees.is_none()
                && r.fee_asset.is_none()
                && r.rebate.is_none()
                && r.rebate_asset.is_none(),
            "LEDGER native omitted order fields"
        );
    }
    assert_eq!(
        f["closedRows"].as_array().unwrap().len(),
        24,
        "LEDGER independent closed floor"
    );
    for c in f["closedRows"].as_array().unwrap() {
        let e = &c["result"];
        let Some(actual) = outcome(
            map_bybit_closed_pnl(
                &safe_cex_exchange::js_runtime::value::from_json(&c["row"]),
                safe_cex_exchange::js_runtime::value::Js::str("account-A"),
                c["inverse"].as_bool().unwrap(),
            )
            .map(|v| {
                if v.is_nullish() {
                    None
                } else {
                    Some(
                        safe_cex_exchange::js_runtime::value::to_typed::<ClosedPnlEntry>(&v)
                            .unwrap(),
                    )
                }
            })
            .map_err(|e| e.to_exchange_error().message),
            e,
        ) else {
            continue;
        };
        let e = &e["ok"];
        if e.is_null() {
            assert!(actual.is_none(), "LEDGER native rejected closed row");
            continue;
        }
        let r = actual.unwrap();
        assert_eq!(
            r.provenance,
            DataProvenance::Venue,
            "LEDGER native provenance"
        );
        assert_eq!(
            r.symbol,
            e["symbol"].as_str().unwrap(),
            "LEDGER native closed symbol"
        );
        assert_eq!(
            r.account,
            e["account"].as_str().unwrap(),
            "LEDGER native closed account"
        );
        assert_eq!(
            r.side,
            e["side"].as_str().unwrap(),
            "LEDGER native closed side"
        );
        let ClosedPnlEntryNotional::Variant1(v) = r.notional else {
            panic!("native numeric notional")
        };
        number(v, &e["notional"], "closed notional");
        let ClosedPnlEntryContracts::Variant1(v) = r.contracts else {
            panic!("native numeric contracts")
        };
        number(v, &e["contracts"], "closed contracts");
        let ClosedPnlEntryEntryPrice::Variant1(v) = r.entry_price else {
            panic!("native numeric entry")
        };
        number(v, &e["entryPrice"], "closed entryPrice");
        let ClosedPnlEntryExitPrice::Variant1(v) = r.exit_price else {
            panic!("native numeric exit")
        };
        number(v, &e["exitPrice"], "closed exitPrice");
        let ClosedPnlEntryUnrealizedPnl::Variant1(v) = r.unrealized_pnl else {
            panic!("native numeric pnl")
        };
        number(v, &e["unrealizedPnl"], "closed pnl");
        let ClosedPnlEntryFees::Variant1(v) = r.fees else {
            panic!("native numeric fees")
        };
        number(v, &e["fees"], "closed fees");
        let ClosedPnlEntryLeverage::Variant0(v) = r.leverage else {
            panic!("native numeric leverage")
        };
        number(v, &e["leverage"], "closed leverage");
        number(r.time, &e["time"], "closed time");
        number(r.closed_time, &e["closedTime"], "closed closedTime");
    }
    assert_eq!(
        f["incomes"].as_array().unwrap().len(),
        23,
        "LEDGER independent income floor"
    );
    for c in f["incomes"].as_array().unwrap() {
        let e = &c["result"];
        let Some(records) = outcome(
            map_bybit_transaction_to_income_records(
                &safe_cex_exchange::js_runtime::value::from_json(&c["row"]),
            )
            .and_then(|v| safe_cex_exchange::js_runtime::value::to_typed::<Vec<IncomeRecord>>(&v))
            .map_err(|e| e.to_exchange_error().message),
            e,
        ) else {
            continue;
        };
        let expected = e["ok"].as_array().unwrap();
        assert_eq!(
            records.len(),
            expected.len(),
            "LEDGER native income record count from source"
        );
        for (r, e) in records.iter().zip(expected) {
            assert_eq!(
                r.symbol,
                e["symbol"].as_str().unwrap(),
                "LEDGER native income symbol"
            );
            assert_eq!(
                r.asset,
                e["asset"].as_str().unwrap(),
                "LEDGER native income asset"
            );
            assert_eq!(
                r.income_type,
                e["incomeType"].as_str().unwrap(),
                "LEDGER native incomeType"
            );
            number(r.income, &e["income"], "income amount");
            number(r.timestamp, &e["timestamp"], "income timestamp");
            assert_eq!(
                r.tran_id,
                e["tranId"].as_str().unwrap(),
                "LEDGER native income tranId"
            );
            assert_eq!(
                r.trade_id.as_deref(),
                e["tradeId"].as_str(),
                "LEDGER native income tradeId"
            );
            assert_eq!(
                r.category,
                Some(serde_json::from_value::<IncomeCategory>(e["category"].clone()).unwrap()),
                "LEDGER native income category"
            );
        }
    }
    assert_eq!(
        f["overlays"].as_array().unwrap().len(),
        3,
        "LEDGER independent overlay floor"
    );
    for c in f["overlays"].as_array().unwrap() {
        let fees: Vec<BybitFeeRate> = serde_json::from_value(c["fees"].clone()).unwrap();
        for (fee, e) in fees.iter().zip(c["fees"].as_array().unwrap()) {
            assert_eq!(
                fee.symbol,
                e["symbol"].as_str().unwrap(),
                "LEDGER native fee symbol"
            );
            assert_eq!(
                fee.taker_fee_rate,
                e["takerFeeRate"].as_str().unwrap(),
                "LEDGER native fee taker input"
            );
            assert_eq!(
                fee.maker_fee_rate,
                e["makerFeeRate"].as_str().unwrap(),
                "LEDGER native fee maker input"
            );
        }
        let markets: Vec<Market> = serde_json::from_value(c["markets"].clone()).unwrap();
        assert_eq!(markets.len(), 2);
        assert_eq!(
            (
                &*markets[0].id,
                &*markets[0].symbol,
                markets[0].taker_fee,
                markets[0].maker_fee
            ),
            ("id-A", "BTCUSDT", Some(0.01), Some(-0.02)),
            "LEDGER native market ingress"
        );
        let overlay = bybit_fee_overlay(&fees);
        let e = &c["result"]["ok"];
        if e.is_null() {
            assert!(overlay.is_none(), "LEDGER native empty overlay");
            continue;
        }
        let result = overlay.unwrap()(&markets);
        assert_eq!(result.len(), 2, "LEDGER native overlay count");
        for (i, r) in result.iter().enumerate() {
            optional(r.taker_fee, &e[i]["takerFee"], "overlay takerFee");
            optional(r.maker_fee, &e[i]["makerFee"], "overlay makerFee");
            let mut unchanged = r.clone();
            unchanged.taker_fee = markets[i].taker_fee;
            unchanged.maker_fee = markets[i].maker_fee;
            assert_eq!(
                unchanged, markets[i],
                "LEDGER native overlay preserves other fields"
            );
        }
    }
}
