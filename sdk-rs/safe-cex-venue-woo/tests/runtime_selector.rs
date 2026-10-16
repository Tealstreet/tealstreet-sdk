use safe_cex_venue_woo::runtime_selector::*;
use safe_cex_venue_woo::v3_contracts::{WOO_V3_MIGRATION_ROUTES, WooV3HttpMethod};
use serde_json::{Value, json};
#[path = "support/number_bits.rs"]
mod capture_bits;

fn assert_native(actual: &Value, expected: &Value, path: &str) {
    match actual {
        Value::Number(_) => assert_eq!(
            capture_bits::number_bits(
                safe_cex_num::numeric_ingress::js_value_as_f64(actual).unwrap()
            ),
            expected["numberBits"].as_str().unwrap(),
            "route native number bits {path}"
        ),
        Value::Array(items) => {
            assert_eq!(
                items.len(),
                expected.as_array().unwrap().len(),
                "route array length {path}"
            );
            for (i, item) in items.iter().enumerate() {
                assert_native(item, &expected[i], &format!("{path}[{i}]"));
            }
        }
        Value::Object(fields) => {
            assert_eq!(
                fields.len(),
                expected.as_object().unwrap().len(),
                "route own field count {path}"
            );
            for (key, value) in fields {
                assert_native(value, expected.get(key).unwrap(), &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "route native field {path}"),
    }
}

#[test]
fn runtime_routes_match_actual_source_before_any_serialization() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/runtime-selector-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(source["capture"]["architecture"].is_string());
    assert!(source["capture"]["node"].is_string());
    assert_eq!(
        source["routes"].as_array().unwrap().len(),
        WOO_V3_MIGRATION_ROUTES.len()
    );
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        (WOO_V3_MIGRATION_ROUTES.len() + 6) * 17
    );
    for route in source["routes"].as_array().unwrap() {
        let actual = require_route(route["id"].as_str().unwrap()).unwrap();
        assert_eq!(actual.id, route["id"], "registry native id");
        assert_eq!(
            actual.legacy_path, route["legacyPath"],
            "registry native legacy path"
        );
        assert_eq!(
            actual.consumer, route["consumer"],
            "registry native consumer"
        );
    }
    for case in source["cases"].as_array().unwrap() {
        let id = case["routeId"].as_str().unwrap();
        let mode: Value = serde_json::from_str(case["mode"].as_str().unwrap()).unwrap();
        let result = select_woo_v3_runtime_route(id, &mode);
        if let Some(error) = case.get("error") {
            assert_eq!(error["name"], "Error");
            assert_eq!(
                result.unwrap_err(),
                error["message"],
                "route error precedes mode handling"
            );
        } else {
            let actual = result.unwrap();
            assert_eq!(
                actual.get("blocker"),
                case["ok"].get("blocker"),
                "route blocker identity {id} {mode}"
            );
            assert_native(&actual, &case["ok"], id);
        }
    }
    for case in source["targets"].as_array().unwrap() {
        let method = match case["method"].as_str().unwrap() {
            "GET" => WooV3HttpMethod::Get,
            "POST" => WooV3HttpMethod::Post,
            "PUT" => WooV3HttpMethod::Put,
            "DELETE" => WooV3HttpMethod::Delete,
            _ => panic!("unexpected actual-source method"),
        };
        assert_native(
            &target(
                case["generation"].as_str().unwrap(),
                method,
                case["path"].as_str().unwrap(),
            ),
            &case["result"],
            "standalone target",
        );
    }
    assert_eq!(source["routeAlias"]["after"], "/mutated-source-route");
    assert_eq!(
        require_route("markets").unwrap().legacy_path,
        source["routeAlias"]["before"],
        "chosen immutable registry view"
    );
    assert_eq!(source["modeAlias"]["after"], "after");
    assert_eq!(source["excluded"]["undefinedMode"], true);
    assert_eq!(source["excluded"]["nanMode"], "7ff8000000000000");
    assert_eq!(source["excluded"]["openMethod"], "PATCH");
    assert!(
        serde_json::from_str::<String>(source["representation"]["input"].as_str().unwrap())
            .is_err(),
        "UTF-8 ingress representation refusal"
    );
}

#[test]
fn runtime_route_owned_modes_and_mutable_plans_are_declared() {
    let mut mode = json!({"version": "before"});
    let selected = select_woo_v3_runtime_route("markets", &mode).unwrap();
    mode["version"] = json!("after");
    assert_eq!(
        selected["requestedMode"]["version"], "before",
        "owned mode snapshot"
    );
    let mut plan = select_woo_v3_runtime_route("markets", &json!("legacy")).unwrap();
    plan["authoritative"]["path"] = json!("/local-only");
    assert_eq!(
        plan["authoritative"]["path"], "/local-only",
        "chosen mutable plan"
    );
    assert_eq!(
        require_route("markets").unwrap().legacy_path,
        "/v1/public/info",
        "plan mutation does not edit registry"
    );
}

fn native_target_fields(actual: &WooV3RuntimeRouteTarget, expected: &Value, label: &str) {
    let generation = match expected["generation"].as_str().unwrap() {
        "legacy" => WooV3RuntimeGeneration::Legacy,
        "v3" => WooV3RuntimeGeneration::V3,
        _ => panic!("source generation outside declaration"),
    };
    let method = match expected["method"].as_str().unwrap() {
        "GET" => WooV3HttpMethod::Get,
        "POST" => WooV3HttpMethod::Post,
        "PUT" => WooV3HttpMethod::Put,
        "DELETE" => WooV3HttpMethod::Delete,
        _ => panic!("source method outside declaration"),
    };
    assert_eq!(
        actual.generation(),
        generation,
        "typed native {label} generation"
    );
    assert_eq!(actual.method(), method, "typed native {label} method");
    assert_eq!(actual.path(), expected["path"], "typed native {label} path");
}

fn native_plan_fields(actual: &WooV3RuntimeRouteSelection, expected: &Value) {
    // All reads below are native enums, private-record accessors and native Values.
    // No serialization of the actual plan occurs in this function.
    macro_rules! common {
        ($plan:expr) => {{
            let p = $plan;
            assert_eq!(p.status(), expected["status"], "typed native status");
            assert_eq!(p.route_id().as_str(), expected["routeId"], "typed native route ID");
            let gate = match expected["gate"].as_str().unwrap() {
                "publicReadParity" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::PublicReadParity,
                "privateReadParity" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::PrivateReadParity,
                "regularWriteDemoLifecycle" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::RegularWriteDemoLifecycle,
                "algoWriteDemoLifecycle" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::AlgoWriteDemoLifecycle,
                "cancelAllAfterDemoLifecycle" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::CancelAllAfterDemoLifecycle,
                "accountWriteDemoLifecycle" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::AccountWriteDemoLifecycle,
                "publicWebsocketParity" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::PublicWebsocketParity,
                "privateWebsocketParity" => safe_cex_venue_woo::v3_contracts::WooV3MigrationGate::PrivateWebsocketParity,
                _ => panic!("source gate outside declaration"),
            };
            assert_eq!(p.gate(), gate, "typed native gate");
            assert_eq!(p.store_authority(), expected["storeAuthority"], "typed native store authority");
        }};
    }
    macro_rules! mode {
        ($plan:expr) => {
            assert_eq!(
                $plan.requested_mode().as_str(),
                expected["requestedMode"],
                "typed native requested mode"
            );
        };
    }
    match actual {
        WooV3RuntimeRouteSelection::Legacy(p) => {
            common!(p);
            mode!(p);
            assert_eq!(
                p.requested_mode(),
                WooV3RuntimeMode::Legacy,
                "typed native legacy discriminant"
            );
            native_target_fields(
                p.authoritative(),
                &expected["authoritative"],
                "legacy authoritative",
            );
        }
        WooV3RuntimeRouteSelection::ShadowRead(p) => {
            common!(p);
            mode!(p);
            assert_eq!(
                p.requested_mode(),
                WooV3RuntimeMode::ShadowV3,
                "typed native shadow discriminant"
            );
            native_target_fields(
                p.authoritative(),
                &expected["authoritative"],
                "shadow authoritative",
            );
            native_target_fields(p.observer(), &expected["observer"], "shadow observer");
            assert_eq!(
                p.mutation_dispatch(),
                expected["mutationDispatch"],
                "typed native mutation dispatch"
            );
        }
        WooV3RuntimeRouteSelection::V3(p) => {
            common!(p);
            mode!(p);
            assert_eq!(
                p.requested_mode(),
                WooV3RuntimeMode::V3,
                "typed native v3 discriminant"
            );
            native_target_fields(
                p.authoritative(),
                &expected["authoritative"],
                "v3 authoritative",
            );
        }
        WooV3RuntimeRouteSelection::AlreadyV3(p) => {
            common!(p);
            mode!(p);
            assert_ne!(
                p.requested_mode(),
                WooV3RuntimeMode::ShadowV3,
                "typed native already-v3 excludes shadow"
            );
            native_target_fields(
                p.authoritative(),
                &expected["authoritative"],
                "already-v3 authoritative",
            );
        }
        WooV3RuntimeRouteSelection::Blocked(p) => {
            common!(p);
            mode!(p);
            assert_eq!(p.blocker(), expected["blocker"], "typed native blocker");
        }
        WooV3RuntimeRouteSelection::InvalidMode(p) => {
            common!(p);
            assert_eq!(
                p.blocker(),
                "invalid-runtime-mode",
                "typed native invalid-mode discriminant"
            );
            assert_eq!(
                p.blocker(),
                expected["blocker"],
                "typed native invalid blocker"
            );
            assert_native(
                p.requested_mode(),
                &expected["requestedMode"],
                "typed invalid mode",
            );
        }
    }
}

#[test]
fn typed_route_plans_match_source_native_fields_before_wire() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/runtime-selector-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["routes"].as_array().unwrap().len(),
        26,
        "independent declared route floor"
    );
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        544,
        "independent route-mode floor"
    );
    let mut native = Vec::new();
    for case in source["cases"].as_array().unwrap() {
        let id = case["routeId"].as_str().unwrap();
        let mode: Value = serde_json::from_str(case["mode"].as_str().unwrap()).unwrap();
        let actual = select_woo_v3_runtime_route_native(id, &mode);
        if let Some(error) = case.get("error") {
            assert_eq!(
                actual.unwrap_err(),
                error["message"],
                "typed native lookup before mode"
            );
        } else {
            let actual = actual.unwrap();
            native_plan_fields(&actual, &case["ok"]);
            native.push((actual, &case["ok"]));
        }
    }
    // Every native case has been asserted before the first wire projection.
    for (actual, expected) in native {
        let wire = serde_json::to_value(actual).unwrap();
        assert_eq!(
            wire.as_object().unwrap().keys().collect::<Vec<_>>(),
            expected.as_object().unwrap().keys().collect::<Vec<_>>(),
            "typed plan wire property order",
        );
        assert_native(&wire, expected, "typed plan wire");
    }
}

#[test]
fn typed_route_and_mode_domains_and_invalid_owned_snapshot() {
    use WooV3MigrationRouteId::*;
    // Fixed source expectation, not derived from enum serde/as_str metadata.
    let ids = [
        ("markets", Markets),
        ("tickers", Tickers),
        ("orderbook", Orderbook),
        ("recent-klines", RecentKlines),
        ("historical-klines", HistoricalKlines),
        ("place-order", PlaceOrder),
        ("cancel-order", CancelOrder),
        ("fetch-orders", FetchOrders),
        ("cancel-symbol-orders", CancelSymbolOrders),
        ("cancel-all-orders", CancelAllOrders),
        ("amend-order", AmendOrder),
        ("place-algo-order", PlaceAlgoOrder),
        ("amend-algo-order", AmendAlgoOrder),
        ("cancel-algo-order", CancelAlgoOrder),
        ("fetch-algo-orders", FetchAlgoOrders),
        ("cancel-all-algo-orders", CancelAllAlgoOrders),
        ("cancel-all-after", CancelAllAfter),
        ("recent-trades", RecentTrades),
        ("historical-trades", HistoricalTrades),
        ("transaction-history", TransactionHistory),
        ("account-info", AccountInfo),
        ("balances", Balances),
        ("positions", Positions),
        ("position-mode", PositionMode),
        ("fetch-futures-leverage", FetchFuturesLeverage),
        ("set-futures-leverage", SetFuturesLeverage),
    ];
    for (text, expected) in ids {
        let actual: WooV3MigrationRouteId = serde_json::from_value(json!(text)).unwrap();
        assert_eq!(
            actual, expected,
            "typed native decoded route ID discriminant {text}"
        );
    }
    for (text, expected) in ids {
        assert_eq!(expected.as_str(), text, "typed native route ID text");
    }

    for (text, expected) in [
        ("legacy", WooV3RuntimeMode::Legacy),
        ("shadow-v3", WooV3RuntimeMode::ShadowV3),
        ("v3", WooV3RuntimeMode::V3),
    ] {
        let actual: WooV3RuntimeMode = serde_json::from_value(json!(text)).unwrap();
        assert_eq!(actual, expected, "typed native decoded mode");
        assert_eq!(actual.as_str(), text, "typed native mode text");
    }
    for mode in [json!("LEGACY"), json!(null), json!(1), json!({})] {
        assert!(
            serde_json::from_value::<WooV3RuntimeMode>(mode).is_err(),
            "typed mode rejects outside union"
        );
    }
    for id in ["", "constructor", "__proto__", "unknown"] {
        assert!(
            serde_json::from_value::<WooV3MigrationRouteId>(json!(id)).is_err(),
            "typed ID rejects outside union"
        );
    }
    let mut mode = json!({"version": "before"});
    let WooV3RuntimeRouteSelection::InvalidMode(p) =
        select_woo_v3_runtime_route_native("markets", &mode).unwrap()
    else {
        panic!("typed native invalid mode variant")
    };
    mode["version"] = json!("after");
    assert_eq!(
        p.requested_mode()["version"],
        "before",
        "typed native chosen owned mode snapshot"
    );
}

#[test]
fn declared_record_domains_observe_surrogates_and_native_whole_record_refusal() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/runtime-plan-domain-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source domain oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["gatesClosed"], true,
        "source fixture does not open a runtime gate"
    );
    let expected = [
        ("target", vec!["path"]),
        ("legacy", vec!["authoritative.path"]),
        ("shadow", vec!["authoritative.path", "observer.path"]),
        ("v3", vec!["authoritative.path"]),
        ("already-v3", vec!["authoritative.path"]),
        ("invalid-mode", vec!["requestedMode"]),
    ];
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "independent declared refusal floor");
    for (row, (name, slots)) in rows.iter().zip(expected) {
        assert_eq!(row["name"], name, "source domain fixture identity");
        assert_eq!(
            row["slots"],
            json!(slots),
            "source observable declaration slots"
        );
        assert_eq!(
            row["units"].as_array().unwrap().len(),
            slots.len(),
            "independent observed unit count {name}"
        );
        for unit in row["units"].as_array().unwrap() {
            assert_eq!(
                unit,
                &json!(0xd800),
                "source declared observable native UTF-16 unit"
            );
        }
        // Valid source JSON, refused as a whole by the shared native Value/String surface.
        // No native record exists to assert in this representation-refusal case.
        assert!(
            serde_json::from_str::<Value>(row["wire"].as_str().unwrap()).is_err(),
            "native whole-record surrogate representation refusal {name}"
        );
    }
    let positives = source["positives"].as_array().unwrap();
    assert_eq!(positives.len(), 3, "independent native positive floor");
    for (input, expected) in positives.iter().zip(["ordinary", "😀", "\\ud800"]) {
        let decoded: String = serde_json::from_str(input.as_str().unwrap()).unwrap();
        assert_eq!(
            decoded, expected,
            "decoded native scalar/pair/literal path exact"
        );
    }
}
