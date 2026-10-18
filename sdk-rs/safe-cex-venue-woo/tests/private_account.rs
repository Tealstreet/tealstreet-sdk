use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::{PositionMode, PositionSide, object_map::JsObjectMap};
use safe_cex_venue_woo::v3_private::*;
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "PRIVATE-ACCOUNT source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value, key: &str) -> Option<JsNumberInput> {
    row[key].as_str().map(|s| js_from_str(s).unwrap())
}
fn number_named(actual: f64, e: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        e["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "{label}"
    );
}
fn number(actual: f64, e: &Value) {
    number_named(actual, e, "PRIVATE-ACCOUNT native number bits");
}
fn checked<T>(result: Result<T, String>, e: &Value, check: impl FnOnce(T, &Value)) {
    if let Some(error) = e.get("error") {
        assert!(result.is_err(), "PRIVATE-ACCOUNT native refusal");
        assert_eq!(
            result.err().unwrap(),
            error["message"].as_str().unwrap(),
            "PRIVATE-ACCOUNT native first error"
        );
    } else {
        check(result.unwrap(), &e["ok"]);
    }
}
fn captured(v: &JsNumberInput, e: &Value) {
    if e.get("$number").is_some() {
        number(v.clone().decode::<f64>().unwrap(), e);
        return;
    }
    match e {
        Value::Null => assert!(v.clone().decode::<()>().is_ok()),
        Value::Bool(b) => assert_eq!(v.clone().decode::<bool>().unwrap(), *b),
        Value::String(s) => assert_eq!(
            v.clone().decode::<String>().unwrap(),
            *s,
            "PRIVATE-ACCOUNT native captured text"
        ),
        Value::Array(rows) => {
            let a = v.clone().decode::<Vec<JsNumberInput>>().unwrap();
            assert_eq!(a.len(), rows.len());
            for (v, e) in a.iter().zip(rows) {
                captured(v, e);
            }
        }
        Value::Object(rows) => {
            let a = v.clone().decode::<JsObjectMap<JsNumberInput>>().unwrap();
            assert_eq!(a.len(), rows.len());
            for (k, e) in rows {
                captured(a.get(k).unwrap(), e);
            }
        }
        _ => panic!("untagged source number"),
    }
}
fn mode(e: &Value) -> WooV3AccountPositionMode {
    match e.as_str().unwrap() {
        "ONE_WAY" => WooV3AccountPositionMode::OneWay,
        "HEDGE_MODE" => WooV3AccountPositionMode::HedgeMode,
        _ => panic!("mode"),
    }
}
fn gateway_side(e: &Value) -> WooV3PositionSide {
    match e.as_str().unwrap() {
        "BOTH" => WooV3PositionSide::Both,
        "LONG" => WooV3PositionSide::Long,
        "SHORT" => WooV3PositionSide::Short,
        _ => panic!("side"),
    }
}
fn inspect_woo_v3_page_meta(v: &WooV3PageMeta, e: &Value) {
    number_named(v.total, &e["total"], "PRIVATE-ACCOUNT native total");
    number_named(
        v.records_per_page,
        &e["recordsPerPage"],
        "PRIVATE-ACCOUNT native recordsPerPage",
    );
    number_named(
        v.current_page,
        &e["currentPage"],
        "PRIVATE-ACCOUNT native currentPage",
    );
}
fn inspect_woo_v3_asset_balance(v: &WooV3AssetBalance, e: &Value) {
    assert_eq!(
        v.token,
        e["token"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native token"
    );
    assert_eq!(
        v.holding,
        e["holding"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native holding"
    );
    assert_eq!(
        v.frozen,
        e["frozen"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native frozen"
    );
    assert_eq!(
        v.staked,
        e["staked"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native staked"
    );
    assert_eq!(
        v.unbonding,
        e["unbonding"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native unbonding"
    );
    assert_eq!(
        v.vault,
        e["vault"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native vault"
    );
    assert_eq!(
        v.interest,
        e["interest"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native interest"
    );
    assert_eq!(
        v.earn,
        e["earn"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native earn"
    );
    assert_eq!(
        v.pending_short_qty,
        e["pendingShortQty"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native pendingShortQty"
    );
    assert_eq!(
        v.pending_long_qty,
        e["pendingLongQty"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native pendingLongQty"
    );
    assert_eq!(
        v.available_balance,
        e["availableBalance"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native availableBalance"
    );
    assert_eq!(
        v.average_open_price,
        e["averageOpenPrice"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native averageOpenPrice"
    );
    assert_eq!(
        v.mark_price,
        e["markPrice"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native markPrice"
    );
    assert_eq!(
        v.pnl24_h,
        e["pnl24H"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native pnl24H"
    );
    assert_eq!(
        v.fee24_h,
        e["fee24H"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native fee24H"
    );
    number_named(
        v.timestamp,
        &e["timestamp"],
        "PRIVATE-ACCOUNT native timestamp",
    );
}
fn inspect_woo_v3_asset_balance_read_model(v: &WooV3AssetBalanceReadModel, e: &Value) {
    assert_eq!(
        v.token,
        e["token"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native token"
    );
    number_named(
        v.trading_total,
        &e["tradingTotal"],
        "PRIVATE-ACCOUNT native tradingTotal",
    );
    number_named(
        v.trading_free,
        &e["tradingFree"],
        "PRIVATE-ACCOUNT native tradingFree",
    );
    number_named(
        v.trading_frozen,
        &e["tradingFrozen"],
        "PRIVATE-ACCOUNT native tradingFrozen",
    );
    number_named(v.staked, &e["staked"], "PRIVATE-ACCOUNT native staked");
    number_named(
        v.unbonding,
        &e["unbonding"],
        "PRIVATE-ACCOUNT native unbonding",
    );
    number_named(v.vault, &e["vault"], "PRIVATE-ACCOUNT native vault");
    number_named(
        v.interest,
        &e["interest"],
        "PRIVATE-ACCOUNT native interest",
    );
    number_named(v.earn, &e["earn"], "PRIVATE-ACCOUNT native earn");
    number_named(
        v.pending_short_quantity,
        &e["pendingShortQuantity"],
        "PRIVATE-ACCOUNT native pendingShortQuantity",
    );
    number_named(
        v.pending_long_quantity,
        &e["pendingLongQuantity"],
        "PRIVATE-ACCOUNT native pendingLongQuantity",
    );
    number_named(
        v.average_open_price,
        &e["averageOpenPrice"],
        "PRIVATE-ACCOUNT native averageOpenPrice",
    );
    number_named(
        v.mark_price,
        &e["markPrice"],
        "PRIVATE-ACCOUNT native markPrice",
    );
    number_named(v.pnl24_h, &e["pnl24H"], "PRIVATE-ACCOUNT native pnl24H");
    number_named(v.fee24_h, &e["fee24H"], "PRIVATE-ACCOUNT native fee24H");
    number_named(
        v.timestamp,
        &e["timestamp"],
        "PRIVATE-ACCOUNT native timestamp",
    );
}
fn inspect_woo_v3_futures_position(v: &WooV3FuturesPosition, e: &Value) {
    assert_eq!(
        v.symbol,
        e["symbol"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native symbol"
    );
    assert_eq!(
        v.holding,
        e["holding"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native holding"
    );
    assert_eq!(v.position_side, gateway_side(&e["positionSide"]));
    assert_eq!(
        v.pending_long_qty,
        e["pendingLongQty"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native pendingLongQty"
    );
    assert_eq!(
        v.pending_short_qty,
        e["pendingShortQty"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native pendingShortQty"
    );
    assert_eq!(
        v.settle_price,
        e["settlePrice"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native settlePrice"
    );
    assert_eq!(
        v.average_open_price,
        e["averageOpenPrice"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native averageOpenPrice"
    );
    assert_eq!(
        v.pnl24_h,
        e["pnl24H"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native pnl24H"
    );
    assert_eq!(
        v.fee24_h,
        e["fee24H"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native fee24H"
    );
    assert_eq!(
        v.mark_price,
        e["markPrice"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native markPrice"
    );
    assert_eq!(
        v.est_liq_price,
        e["estLiqPrice"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native estLiqPrice"
    );
    number_named(
        v.adl_quantile,
        &e["adlQuantile"],
        "PRIVATE-ACCOUNT native adlQuantile",
    );
    number_named(
        v.timestamp,
        &e["timestamp"],
        "PRIVATE-ACCOUNT native timestamp",
    );
}
fn inspect_woo_v3_position_read_model(v: &WooV3PositionReadModel, e: &Value) {
    assert_eq!(
        v.symbol,
        e["symbol"].as_str().unwrap(),
        "PRIVATE-ACCOUNT native symbol"
    );
    assert_eq!(
        v.side,
        if e["side"] == "long" {
            PositionSide::Long
        } else {
            assert_eq!(e["side"], "short");
            PositionSide::Short
        },
        "PRIVATE-ACCOUNT native position side"
    );
    assert_eq!(
        v.position_mode,
        if e["positionMode"] == "oneway" {
            PositionMode::OneWay
        } else {
            assert_eq!(e["positionMode"], "hedged");
            PositionMode::Hedged
        }
    );
    assert_eq!(
        v.gateway_position_side,
        gateway_side(&e["gatewayPositionSide"])
    );
    number_named(
        v.signed_holding,
        &e["signedHolding"],
        "PRIVATE-ACCOUNT native signedHolding",
    );
    number_named(
        v.contracts,
        &e["contracts"],
        "PRIVATE-ACCOUNT native contracts",
    );
    number_named(
        v.entry_price,
        &e["entryPrice"],
        "PRIVATE-ACCOUNT native entryPrice",
    );
    number_named(
        v.mark_price,
        &e["markPrice"],
        "PRIVATE-ACCOUNT native markPrice",
    );
    number_named(
        v.notional,
        &e["notional"],
        "PRIVATE-ACCOUNT native notional",
    );
    number(
        v.estimated_liquidation_price,
        &e["estimatedLiquidationPrice"],
    );
    number_named(
        v.pending_long_quantity,
        &e["pendingLongQuantity"],
        "PRIVATE-ACCOUNT native pendingLongQuantity",
    );
    number_named(
        v.pending_short_quantity,
        &e["pendingShortQuantity"],
        "PRIVATE-ACCOUNT native pendingShortQuantity",
    );
    number_named(v.pnl24_h, &e["pnl24H"], "PRIVATE-ACCOUNT native pnl24H");
    number_named(v.fee24_h, &e["fee24H"], "PRIVATE-ACCOUNT native fee24H");
    number_named(
        v.adl_quantile,
        &e["adlQuantile"],
        "PRIVATE-ACCOUNT native adlQuantile",
    );
    number_named(
        v.timestamp,
        &e["timestamp"],
        "PRIVATE-ACCOUNT native timestamp",
    );
}
#[test]
fn native_declarations() {
    let oracle = source();
    let rows = oracle["declarations"].as_array().unwrap();
    assert_eq!(rows.len(), 5);
    let expected = [
        "WooV3PageMeta",
        "WooV3AssetBalance",
        "WooV3AssetBalanceReadModel",
        "WooV3FuturesPosition",
        "WooV3PositionReadModel",
    ];
    for (row, name) in rows.iter().zip(expected) {
        assert_eq!(row["type"], name);
        let text = row["input"].as_str().unwrap();
        let e = &row["expected"];
        match name {
            "WooV3PageMeta" => {
                inspect_woo_v3_page_meta(&js_from_str::<WooV3PageMeta>(text).unwrap(), e)
            }
            "WooV3AssetBalance" => {
                inspect_woo_v3_asset_balance(&js_from_str::<WooV3AssetBalance>(text).unwrap(), e)
            }
            "WooV3AssetBalanceReadModel" => inspect_woo_v3_asset_balance_read_model(
                &js_from_str::<WooV3AssetBalanceReadModel>(text).unwrap(),
                e,
            ),
            "WooV3FuturesPosition" => inspect_woo_v3_futures_position(
                &js_from_str::<WooV3FuturesPosition>(text).unwrap(),
                e,
            ),
            "WooV3PositionReadModel" => inspect_woo_v3_position_read_model(
                &js_from_str::<WooV3PositionReadModel>(text).unwrap(),
                e,
            ),
            _ => unreachable!(),
        }
    }
    for (wire, expected) in [
        ("ONE_WAY", WooV3AccountPositionMode::OneWay),
        ("HEDGE_MODE", WooV3AccountPositionMode::HedgeMode),
    ] {
        assert_eq!(
            js_from_str::<WooV3AccountPositionMode>(&format!("\"{wire}\"")).unwrap(),
            expected
        );
    }
    for (wire, expected) in [
        ("BOTH", WooV3PositionSide::Both),
        ("LONG", WooV3PositionSide::Long),
        ("SHORT", WooV3PositionSide::Short),
    ] {
        assert_eq!(
            js_from_str::<WooV3PositionSide>(&format!("\"{wire}\"")).unwrap(),
            expected
        );
    }
    for bad in ["null", "0", "[]", "{}", "\"bad\""] {
        assert!(js_from_str::<WooV3AccountPositionMode>(bad).is_err());
        assert!(js_from_str::<WooV3PositionSide>(bad).is_err());
    }
    for bad in ["null", "[]", "{}"] {
        assert!(js_from_str::<WooV3PageMeta>(bad).is_err());
    }
    for bad in ["null", "[]", "{}"] {
        assert!(js_from_str::<WooV3AssetBalance>(bad).is_err());
    }
    for bad in ["null", "[]", "{}"] {
        assert!(js_from_str::<WooV3AssetBalanceReadModel>(bad).is_err());
    }
    for bad in ["null", "[]", "{}"] {
        assert!(js_from_str::<WooV3FuturesPosition>(bad).is_err());
    }
    for bad in ["null", "[]", "{}"] {
        assert!(js_from_str::<WooV3PositionReadModel>(bad).is_err());
    }
}
#[test]
fn native_helpers_and_numeric_mapping() {
    let o = source();
    let helpers = o["helpers"].as_array().unwrap();
    assert_eq!(helpers.len(), 250);
    let names = [
        "isRecord",
        "requireRecord",
        "requireArray",
        "requireString",
        "requireBoolean",
        "requireInteger",
        "requireFiniteNumber",
        "requireDecimalString",
        "optionalDecimalString",
        "requireEnum",
    ];
    for (i, row) in helpers.iter().enumerate() {
        let name = row["name"].as_str().unwrap();
        assert_eq!(name, names[i / 25]);
        let v = input(row, "input");
        match name {
            "isRecord" => assert_eq!(is_record(v.as_ref()), row["ok"].as_bool().unwrap()),
            "requireRecord" => checked(require_record(v.as_ref(), "probe"), row, |value, e| {
                let e = e.as_object().unwrap();
                assert_eq!(value.len(), e.len());
                for (k, e) in e {
                    captured(value.get(k).unwrap(), e);
                }
            }),
            "requireArray" => checked(require_array(v.as_ref(), "probe"), row, |values, e| {
                let e = e.as_array().unwrap();
                assert_eq!(values.len(), e.len());
                for (v, e) in values.iter().zip(e) {
                    captured(v, e);
                }
            }),
            "requireString" => checked(require_string(v.as_ref(), "probe", false), row, |v, e| {
                assert_eq!(v, e.as_str().unwrap())
            }),
            "requireBoolean" => checked(require_boolean(v.as_ref(), "probe"), row, |v, e| {
                assert_eq!(v, e.as_bool().unwrap())
            }),
            "requireInteger" => checked(require_integer(v.as_ref(), "probe", 0.0), row, number),
            "requireFiniteNumber" => {
                checked(require_finite_number(v.as_ref(), "probe"), row, number)
            }
            "requireDecimalString" => {
                checked(require_decimal_string(v.as_ref(), "probe"), row, |v, e| {
                    assert_eq!(v, e.as_str().unwrap())
                })
            }
            "optionalDecimalString" => {
                let result = optional_decimal_string(v.as_ref(), "probe");
                if row["undefined"] == true {
                    assert!(result.unwrap().is_none());
                } else {
                    checked(result, row, |v, e| assert_eq!(v.as_deref(), e.as_str()));
                }
            }
            "requireEnum" => checked(
                require_enum(v.as_ref(), &["ONE_WAY", "HEDGE_MODE"], "probe"),
                row,
                |v, e| assert_eq!(v, e.as_str().unwrap()),
            ),
            _ => unreachable!(),
        }
    }
    let rows = o["stringAllowEmpty"].as_array().unwrap();
    assert_eq!(rows.len(), 25);
    for row in rows {
        let v = input(row, "input");
        checked(require_string(v.as_ref(), "probe", true), row, |v, e| {
            assert_eq!(v, e.as_str().unwrap())
        });
    }
    let rows = o["decimalNumbers"].as_array().unwrap();
    assert_eq!(rows.len(), 39);
    for row in rows {
        let v = input(row, "input").unwrap().decode::<String>().unwrap();
        let min = input(row, "minimum").map(|v| v.decode::<f64>().unwrap());
        checked(decimal_to_number(&v, "probe", min), row, number);
    }
    let rows = o["symbolCases"].as_array().unwrap();
    assert_eq!(rows.len(), 9);
    for row in rows {
        let v = input(row, "input").unwrap().decode::<String>().unwrap();
        checked(normalize_perpetual_symbol(&v, "probe"), row, |v, e| {
            assert_eq!(v, e.as_str().unwrap())
        });
    }
    let rows = o["modeCases"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(
            map_position_mode(mode(&row["mode"])),
            if row["ok"] == "oneway" {
                PositionMode::OneWay
            } else {
                assert_eq!(row["ok"], "hedged");
                PositionMode::Hedged
            }
        );
    }
    let rows = o["sideCases"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        checked(
            assert_position_side_matches_mode(
                gateway_side(&row["side"]),
                mode(&row["mode"]),
                "probe",
            ),
            row,
            |(), e| assert_eq!(e, true),
        );
    }
}
#[test]
fn native_asset_reads_and_mapping() {
    let o = source();
    let rows = o["assets"].as_array().unwrap();
    assert_eq!(rows.len(), 9);
    for row in rows {
        let v = input(row, "input");
        checked(
            decode_woo_v3_asset_balances(v.as_ref()),
            row,
            |values, e| {
                let e = e.as_array().unwrap();
                assert_eq!(values.len(), e.len());
                for (v, e) in values.iter().zip(e) {
                    inspect_woo_v3_asset_balance(v, e);
                }
            },
        );
    }
    let rows = o["assetMaps"].as_array().unwrap();
    assert_eq!(rows.len(), 7);
    for row in rows {
        let v = js_from_str::<Vec<WooV3AssetBalance>>(row["input"].as_str().unwrap()).unwrap();
        checked(map_woo_v3_asset_balances(&v), row, |values, e| {
            let e = e.as_array().unwrap();
            assert_eq!(values.len(), e.len());
            for (v, e) in values.iter().zip(e) {
                inspect_woo_v3_asset_balance_read_model(v, e);
            }
        });
    }
}
#[test]
fn native_futures_reads_and_mapping() {
    let o = source();
    let rows = o["positions"].as_array().unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        let v = input(row, "input");
        checked(
            decode_woo_v3_futures_positions(v.as_ref()),
            row,
            |values, e| {
                let e = e.as_array().unwrap();
                assert_eq!(values.len(), e.len());
                for (v, e) in values.iter().zip(e) {
                    inspect_woo_v3_futures_position(v, e);
                }
            },
        );
    }
    let rows = o["positionMaps"].as_array().unwrap();
    assert_eq!(rows.len(), 15);
    for row in rows {
        let v = js_from_str::<Vec<WooV3FuturesPosition>>(row["input"].as_str().unwrap()).unwrap();
        checked(
            map_woo_v3_futures_positions(&v, mode(&row["mode"])),
            row,
            |values, e| {
                let e = e.as_array().unwrap();
                assert_eq!(values.len(), e.len());
                for (v, e) in values.iter().zip(e) {
                    inspect_woo_v3_position_read_model(v, e);
                }
            },
        );
    }
}
#[test]
fn native_envelopes_metadata_and_callback_order() {
    let o = source();
    let rows = o["envelopes"].as_array().unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        let v = input(row, "input");
        checked(decode_envelope(v.as_ref(), "probe"), row, |value, e| {
            let e = e.as_object().unwrap();
            assert_eq!(value.len(), e.len());
            for (k, e) in e {
                captured(value.get(k).unwrap(), e);
            }
        });
    }
    let rows = o["metas"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let v = input(row, "input");
        checked(decode_meta(v.as_ref(), "probe"), row, |v, e| {
            inspect_woo_v3_page_meta(&v, e)
        });
    }
    let rows = o["pages"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let v = input(row, "input");
        let mut calls = Vec::new();
        let result = decode_paginated_rows(v.as_ref(), "probe", |value, path| {
            calls.push(path.to_owned());
            require_finite_number(Some(value), path)
        });
        let expected = row["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            calls, expected,
            "PRIVATE-ACCOUNT native metadata-before-callback order"
        );
        checked(result, row, |v, e| {
            inspect_woo_v3_page_meta(&v.meta, &e["meta"]);
            let rows = e["rows"].as_array().unwrap();
            assert_eq!(v.rows.len(), rows.len());
            for (v, e) in v.rows.iter().zip(rows) {
                number(*v, e);
            }
        });
    }
}
