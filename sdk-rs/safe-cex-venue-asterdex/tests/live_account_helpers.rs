use safe_cex_venue_asterdex::account_helpers::*;
use serde_json::{Value, json};
fn number(n: f64) -> Value {
    if n.is_nan() {
        json!({"number":"NaN"})
    } else if n == f64::INFINITY {
        json!({"number":"Infinity"})
    } else if n == f64::NEG_INFINITY {
        json!({"number":"-Infinity"})
    } else if n == 0.0 && n.is_sign_negative() {
        json!({"number":"-0"})
    } else {
        json!(n)
    }
}
fn outcome<T>(
    value: Result<T, safe_cex_num::JsCoercionError>,
    render: impl FnOnce(T) -> Value,
) -> Value {
    match value {
        Ok(v) => json!({"value":render(v)}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    }
}
fn same(a: &Value, b: &Value, context: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{context}"),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{context}");
            for (k, v) in a {
                same(
                    v,
                    b.get(k).unwrap_or_else(|| panic!("missing {k}: {context}")),
                    context,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{context}");
            for (a, b) in a.iter().zip(b) {
                same(a, b, context)
            }
        }
        _ => assert_eq!(a, b, "{context}"),
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_helpers_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for c in data["finite"].as_array().unwrap() {
        same(
            &json!({"value":parse_finite_number(c.get("input")).map(number)}),
            &c["output"],
            &format!("parseFiniteNumber {c}"),
        );
    }
    for c in data["times"].as_array().unwrap() {
        let mut calls = 0;
        let actual = outcome(
            get_order_update_time(&c["input"], || {
                calls += 1;
                1234.0
            }),
            number,
        );
        same(&actual, &c["output"], &format!("getOrderUpdateTime {c}"));
        assert_eq!(
            json!(calls),
            c["clockCalls"],
            "getOrderUpdateTime clock laziness {c}"
        );
    }
    for c in data["balances"].as_array().unwrap() {
        let fields = &c["fields"];
        let actual=parse_asset_balance_updates(c.get("input"),fields["total"].as_str(),fields["free"].as_str().unwrap(),fields["used"].as_str()).map(|updates|updates.into_iter().map(|u|json!({"asset":u.asset,"total":number(u.total),"free":number(u.free),"used":number(u.used)})).collect::<Vec<_>>());
        same(
            &json!({"value":actual}),
            &c["output"],
            &format!("parseAssetBalanceUpdates {c}"),
        );
    }

    let account_cases = data["accountUpdates"].as_array().unwrap();
    assert_eq!(
        account_cases.len(),
        15,
        "handleAccountUpdate source case count"
    );
    let mut account_comparisons = 0;
    for case in account_cases {
        let (output, effects) = match handle_account_update(&case["input"]) {
            Ok(effects) => {
                let effects = effects
                    .into_iter()
                    .map(|effect| match effect {
                        AccountUpdateEffect::ApplyBalances(balances) => json!([
                            "balances",
                            balances
                                .into_iter()
                                .map(|balance| json!({
                                    "asset": balance.asset,
                                    "total": number(balance.total),
                                    "free": number(balance.free),
                                    "used": number(balance.used),
                                }))
                                .collect::<Vec<_>>()
                        ]),
                        AccountUpdateEffect::EmitAccount(data) => {
                            json!(["emit", "account", data])
                        }
                    })
                    .collect::<Vec<_>>();
                (json!({"value":null}), effects)
            }
            Err(error) => (
                json!({"error":{"name":error.name,"message":error.message}}),
                Vec::new(),
            ),
        };
        same(
            &output,
            &case["output"],
            &format!("handleAccountUpdate result {:?}", case["input"]),
        );
        same(
            &json!(effects),
            &case["effects"],
            &format!("handleAccountUpdate ordered effects {:?}", case["input"]),
        );
        account_comparisons += 1;
    }
    assert_eq!(
        account_comparisons, 15,
        "handleAccountUpdate native comparison count"
    );
    for c in data["sides"].as_array().unwrap() {
        let amount = c["amount"].as_f64().unwrap_or(f64::NAN);
        same(
            &outcome(get_position_side(&c["input"], amount), |v| json!(v)),
            &c["output"],
            &format!("getPositionSide {c}"),
        );
    }

    for c in data["upnl"].as_array().unwrap() {
        let keys: Vec<String> = serde_json::from_value(c["keys"].clone()).unwrap();
        let previous: Vec<safe_cex_types::Position> = c["previous"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let mut position: safe_cex_types::Position = serde_json::from_value(json!({
                    "id":"", "symbol":p["symbol"], "side":p["side"], "entryPrice":0,
                    "notional":0,"leverage":1,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0
                }))
                .unwrap();
                position.unrealized_pnl = p["unrealizedPnl"].as_f64().unwrap_or(f64::INFINITY);
                position
            })
            .collect();
        let actual = get_account_update_upnl_deltas(c.get("input"), &previous, &keys).map(|v| {
            v.into_iter()
                .map(|(k, v)| json!([k, number(v)]))
                .collect::<Vec<_>>()
        });
        same(
            &json!({"value":actual}),
            &c["output"],
            &format!("getAccountUpdateUpnlDeltas {c}"),
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn production_store_currency_order_selects_first_margin_asset() {
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_store::{Store, StoreUpdate};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in data["storeUpnl"].as_array().unwrap() {
        let store = DefaultStore::new();
        let balance = serde_json::from_str(case["balanceJson"].as_str().unwrap()).unwrap();
        Store::update(
            &store,
            StoreUpdate {
                balance: Some(balance),
                ..Default::default()
            },
            false,
        )
        .expect("account helper store setup update");
        let mut keys: Vec<String> = store
            .balance()
            .unwrap()
            .currencies
            .as_ref()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        if std::env::var("ASTERDEX_STORE_ORDER_CONTROL").as_deref() == Ok("sort") {
            keys.sort();
        }
        let actual = get_account_update_upnl_deltas(
            Some(&case["input"]),
            &Store::positions(&store).unwrap(),
            &keys,
        )
        .unwrap();
        let expected: Vec<(String, f64)> =
            serde_json::from_value(case["output"]["value"].clone()).unwrap();
        assert_eq!(
            actual, expected,
            "getAccountUpdateUpnlDeltas production store first margin asset"
        );
        assert_eq!(
            json!(keys),
            case["keys"],
            "production store currency enumeration order"
        );
        if keys == ["USDT", "DT"] {
            // The exact former Balance representation selects a different margin asset.
            let raw: Value = serde_json::from_str(case["balanceJson"].as_str().unwrap()).unwrap();
            let legacy: std::collections::BTreeMap<String, safe_cex_types::Balance> =
                serde_json::from_value(raw["currencies"].clone()).unwrap();
            let old_keys: Vec<_> = legacy.keys().cloned().collect();
            let wrong =
                get_account_update_upnl_deltas(Some(&case["input"]), &[], &old_keys).unwrap();
            assert_eq!(
                wrong,
                vec![("DT".into(), 10.0)],
                "former BTreeMap wrong margin asset witness"
            );
            assert_ne!(wrong, expected, "currency repair flips wrong-asset witness");
        }
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn production_store_oracle_rejects_sorted_currency_keys() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "production_store_currency_order_selects_first_margin_asset",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_STORE_ORDER_CONTROL", "sort")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "currency sorting control: {diagnostics}"
    );
    assert!(
        diagnostics.contains("getAccountUpdateUpnlDeltas production store first margin asset"),
        "wrong-asset comparison must fail: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn mirror_cumulative_order_fill_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["fills"].as_array().unwrap().len(),
        14,
        "independent cumulative fill case floor"
    );
    for case in data["fills"].as_array().unwrap() {
        let mut clock_calls = 0;
        let result = mirror_cumulative_order_fill(&case["input"], || {
            clock_calls += 1;
            1234.0
        });
        let (actual, writes) = match result {
            Ok(fill) => {
                let writes = fill
                    .map(|fill| {
                        let filled = number(fill.filled);
                        let remaining = number(fill.remaining);
                        let last_update = number(fill.last_update);
                        json!({
                            "order": {"id": fill.order_id},
                            "patch": {
                                "status": fill.status,
                                "filled": filled,
                                "remaining": remaining,
                                "lastUpdate": last_update
                            }
                        })
                    })
                    .into_iter()
                    .collect::<Vec<_>>();
                (json!({"value": null}), writes)
            }
            Err(error) => (
                json!({"error":{"name":error.name,"message":error.message}}),
                Vec::new(),
            ),
        };
        same(
            &actual,
            &case["output"],
            &format!("mirrorCumulativeOrderFill result {}", case["input"]),
        );
        same(
            &json!(writes),
            &case["writes"],
            &format!(
                "mirrorCumulativeOrderFill update identity {}",
                case["input"]
            ),
        );
        assert_eq!(
            json!(clock_calls),
            case["clockCalls"],
            "mirrorCumulativeOrderFill clock calls {}",
            case["input"]
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn mirror_cumulative_order_fill_oracle_rejects_source_field_swap() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mirror_cumulative_order_fill_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_FILL_CONTROL", "swap")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("mirrorCumulativeOrderFill update identity"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn handle_execution_report_matches_typescript() {
    use safe_cex_num::TableHit;

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let known_ids = vec!["order-1".to_owned()];
    for case in data["executionReports"].as_array().unwrap() {
        let actual = handle_execution_report(&case["input"], &known_ids);
        let (actual_result, writes) = match actual {
            Ok(Some((order_id, update))) => {
                let (status_type, status_string) = match update.status {
                    TableHit::Mapped(status) => (
                        "string",
                        serde_json::to_value(status)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned(),
                    ),
                    TableHit::Inherited(member) => (
                        if member == safe_cex_num::ProtoMember::Prototype {
                            "object"
                        } else {
                            "function"
                        },
                        member.js_string().to_owned(),
                    ),
                    TableHit::Missing => unreachable!("status fallback is explicit"),
                };
                (
                    json!({"value":null}),
                    vec![json!({
                        "orderId":order_id,
                        "statusType":status_type,
                        "statusString":status_string,
                        "filled":number(update.filled),
                        "remaining":number(update.remaining)
                    })],
                )
            }
            Ok(None) => (json!({"value":null}), Vec::new()),
            Err(error) => (
                json!({"error":{"name":error.name,"message":error.message}}),
                Vec::new(),
            ),
        };
        same(
            &actual_result,
            &case["output"],
            &format!("handleExecutionReport result {}", case["input"]),
        );
        same(
            &json!(writes),
            &case["writes"],
            &format!("handleExecutionReport status identity {}", case["input"]),
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn handle_execution_report_oracle_rejects_source_status_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "handle_execution_report_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_EXECUTION_STATUS_CONTROL", "filled-open")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleExecutionReport status identity"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_update_oracle_rejects_source_effect_order_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "account_helpers_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_ACCOUNT_UPDATE_CONTROL", "emit-first")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleAccountUpdate ordered effects"),
        "{diagnostics}"
    );
}
