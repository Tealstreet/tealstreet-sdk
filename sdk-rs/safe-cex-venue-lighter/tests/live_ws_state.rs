use safe_cex_exchange::js_runtime::{
    globals::install_builtins,
    value::{Js, from_json, to_json},
};
use safe_cex_venue_lighter::{ws_private as private, ws_public as public};
use serde_json::Value;
use std::process::Command;
fn number(value: &Value) -> f64 {
    match value.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        None => value.as_f64().unwrap(),
        other => panic!("unknown number {other:?}"),
    }
}
fn script() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/ws-state-oracle.cjs")
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_ws_state_helpers_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [
        ("flags", 4),
        ("deadlines", 196),
        ("toasts", 2744),
        ("recent", 2028),
        ("hydration", 212),
        ("flatten", 530),
        ("flattenBoundaries", 12),
        ("nativeFlatten", 4),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "{name} corpus inventory"
        );
    }
    assert_eq!(
        public::should_enforce_pong_miss_reconnect(),
        oracle["publicPong"].as_bool().unwrap(),
        "public shouldEnforcePongMissReconnect"
    );
    assert_eq!(
        public::get_watchdog_stale_threshold_ms(),
        number(&oracle["publicWatchdog"]),
        "public getWatchdogStaleThresholdMs"
    );
    for case in oracle["flags"].as_array().unwrap() {
        let readonly = case["readonly"].as_bool().unwrap();
        let ack = case["ack"].as_bool().unwrap();
        assert_eq!(
            private::should_enforce_pong_miss_reconnect(),
            case["pong"].as_bool().unwrap(),
            "private shouldEnforcePongMissReconnect"
        );
        assert_eq!(
            private::is_readonly_fallback_active(readonly),
            case["active"].as_bool().unwrap(),
            "isReadonlyFallbackActive {case}"
        );
        assert_eq!(
            private::should_use_rest_order_reconciliation(ack, readonly),
            case["reconcile"].as_bool().unwrap(),
            "shouldUseRestOrderReconciliation {case}"
        );
        assert_eq!(
            private::get_ping_interval_ms(readonly),
            number(&case["ping"]),
            "getPingIntervalMs {case}"
        );
        assert_eq!(
            private::get_idle_ping_threshold_ms(readonly),
            number(&case["idlePing"]),
            "getIdlePingThresholdMs {case}"
        );
        assert_eq!(
            private::get_idle_reconnect_threshold_ms(readonly),
            number(&case["idleReconnect"]),
            "getIdleReconnectThresholdMs {case}"
        );
        assert_eq!(
            private::get_watchdog_stale_threshold_ms(readonly),
            number(&case["watchdog"]),
            "private getWatchdogStaleThresholdMs {case}"
        );
    }
    for case in oracle["deadlines"].as_array().unwrap() {
        assert_eq!(
            private::is_ws_rate_limited(number(&case["now"]), number(&case["until"])),
            case["private"].as_bool().unwrap(),
            "isWsRateLimited {case}"
        );
        assert_eq!(
            public::is_ticker_rest_fallback_cooling_down(
                number(&case["now"]),
                number(&case["until"])
            ),
            case["public"].as_bool().unwrap(),
            "isTickerRestFallbackCoolingDown {case}"
        );
    }
    let flatten_cases = oracle["flatten"].as_array().unwrap();
    for (field, total) in [("expected", 704), ("orders", 840)] {
        assert_eq!(
            flatten_cases
                .iter()
                .map(|case| case[field].as_array().unwrap().len())
                .sum::<usize>(),
            total,
            "FLATTEN-CARDINALITY source {field} total"
        );
    }
    let mut compared_items = [0_usize; 2];
    for case in flatten_cases {
        let payload = case["present"]
            .as_bool()
            .unwrap()
            .then(|| from_json(&case["payload"]));
        let include = case["include"].as_bool().unwrap();
        let flattened = private::flatten_by_market_payload(payload.as_ref(), include).unwrap();
        compared_items[0] += flattened
            .array_values()
            .expect("fixture array property reads")
            .unwrap()
            .len();
        assert_eq!(
            flattened
                .array_values()
                .expect("fixture array property reads")
                .unwrap()
                .len(),
            case["expected"].as_array().unwrap().len(),
            "FLATTEN-CARDINALITY native payload count"
        );
        assert_eq!(
            to_json(&flattened).unwrap(),
            to_json(&from_json(&case["expected"])).unwrap(),
            "flattenByMarketPayload {case}"
        );
        let orders = private::flatten_orders_by_market(payload.as_ref()).unwrap();
        compared_items[1] += orders
            .array_values()
            .expect("fixture array property reads")
            .unwrap()
            .len();
        assert_eq!(
            orders
                .array_values()
                .expect("fixture array property reads")
                .unwrap()
                .len(),
            case["orders"].as_array().unwrap().len(),
            "FLATTEN-CARDINALITY native orders count"
        );
        assert_eq!(
            to_json(&orders).unwrap(),
            to_json(&from_json(&case["orders"])).unwrap(),
            "flattenOrdersByMarket {case}"
        );
    }
    assert_eq!(
        compared_items,
        [704, 840],
        "FLATTEN-CARDINALITY compared native items"
    );
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let json = globals.get("JSON").unwrap();
    for case in oracle["flattenBoundaries"].as_array().unwrap() {
        let payload = json
            .call_method("parse", vec![Js::str(case["inputJson"].as_str().unwrap())])
            .unwrap();
        for (value, expected) in [
            (
                private::flatten_by_market_payload(
                    Some(&payload),
                    case["include"].as_bool().unwrap(),
                )
                .unwrap(),
                "expectedJson",
            ),
            (
                private::flatten_orders_by_market(Some(&payload)).unwrap(),
                "ordersJson",
            ),
        ] {
            let actual = json
                .call_method("stringify", vec![value])
                .unwrap()
                .to_display();
            assert_eq!(
                actual,
                case[expected].as_str().unwrap(),
                "native UTF-16 flatten {case}"
            );
        }
    }
    for case in oracle["nativeFlatten"].as_array().unwrap() {
        let row = Js::object(vec![
            ("price", Js::Number(f64::from_bits(0x7ff8000000000042))),
            ("amount", Js::Number(f64::INFINITY)),
            ("zero", Js::Number(-0.0)),
            ("label", Js::String(vec![0xd800].into())),
            ("missing", Js::Undefined),
        ]);
        row.define("cycle", row.clone()).unwrap();
        let values = Js::holey_array(vec![
            None,
            Some(row.clone()),
            Some(Js::Undefined),
            Some(Js::Null),
        ]);
        let payload = if case["kind"] == "object" {
            Js::object(vec![("2", values), ("1", row.clone())])
        } else {
            let function = Js::function("input", |_, _| Ok(Js::Undefined));
            function.define("rows", values).unwrap();
            function
        };
        let flattened =
            private::flatten_by_market_payload(Some(&payload), case["include"].as_bool().unwrap())
                .unwrap();
        let items = flattened
            .array_values()
            .expect("fixture array property reads")
            .unwrap();
        let tags: Vec<_> = items
            .iter()
            .map(|value| {
                if value.same_reference(&row) {
                    "row"
                } else if value.is_undefined() {
                    "undefined"
                } else if matches!(value, Js::Null) {
                    "null"
                } else {
                    "other"
                }
            })
            .collect();
        let actual = serde_json::json!({
            "tags": tags,
            "dense": flattened.array_items().expect("fixture array property reads").unwrap().iter().all(Option::is_some),
            "rowIdentity": items.iter().filter(|value| !value.is_nullish()).all(|value| value.same_reference(&row)),
            "nanBits":format!("{:016x}",row.get("price").unwrap().as_f64().unwrap().to_bits()),
            "zeroBits":format!("{:016x}",row.get("zero").unwrap().as_f64().unwrap().to_bits()),
            "amount":row.get("amount").unwrap().to_display(),
            "units":row.get("label").unwrap().as_str().unwrap().as_ref(),
            "ownUndefined":row.has_own("missing") && row.get("missing").unwrap().is_undefined(),
            "cycle":row.get("cycle").unwrap().same_reference(&row),
            "ordersLength":private::flatten_orders_by_market(Some(&payload)).unwrap().array_values().expect("fixture array property reads").unwrap().len(),
        });
        assert_eq!(actual, case["expected"], "native flatten values {case}");
        row.delete(&"cycle".encode_utf16().collect::<Vec<_>>())
            .unwrap();
    }
    for case in oracle["recent"].as_array().unwrap() {
        assert_eq!(
            private::is_recent_stream_event(
                number(&case["timestamp"]),
                number(&case["wall"]),
                number(&case["virtual"]),
                number(&case["connected"])
            ),
            case["expected"].as_bool().unwrap(),
            "isRecentStreamEvent {case}"
        );
    }
    for case in oracle["hydration"].as_array().unwrap() {
        let tickers: Vec<safe_cex_types::Ticker> = case["tickers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let mut clean = value.clone();
                for field in ["mark", "last", "index", "bid", "ask"] {
                    clean[field] = serde_json::json!(0);
                }
                let mut ticker: safe_cex_types::Ticker = serde_json::from_value(clean).unwrap();
                ticker.mark = number(&value["mark"]);
                ticker.last = number(&value["last"]);
                ticker.index = number(&value["index"]);
                ticker.bid = number(&value["bid"]);
                ticker.ask = number(&value["ask"]);
                ticker
            })
            .collect();
        assert_eq!(
            public::has_hydrated_ticker_price(&tickers),
            case["expected"].as_bool().unwrap(),
            "hasHydratedTickerPrice {case}"
        );
    }
    for case in oracle["toasts"].as_array().unwrap() {
        assert_eq!(
            private::should_force_toast_private_stream_rate_limit(
                number(&case["now"]),
                number(&case["count"]),
                number(&case["first"])
            ),
            case["expected"].as_bool().unwrap(),
            "shouldForceToastPrivateStreamRateLimit {case}"
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn ws_state_source_mutations_fail_by_name() {
    for (mutation, name) in [
        ("private-ping", "getPingIntervalMs"),
        ("flatten-order", "flattenByMarketPayload"),
        // Removing single entries changes the pinned source total first.
        (
            "flatten-single",
            "FLATTEN-CARDINALITY source expected total",
        ),
        ("recent-clock", "isRecentStreamEvent"),
        ("hydration-field", "hasHydratedTickerPrice"),
        ("toast-boundary", "shouldForceToastPrivateStreamRateLimit"),
        ("reconciliation", "shouldUseRestOrderReconciliation"),
        ("public-cooldown", "isTickerRestFallbackCoolingDown"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lighter_ws_state_helpers_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_WS_STATE_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(
            log.contains(name) && log.contains("assertion `left == right` failed"),
            "{log}"
        );
    }
}
#[test]
#[ignore = "requires Node"]
fn ws_state_requires_selected_compiler() {
    let output = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/missing-ws-state-compiler"
            ),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn source_cardinality_controls_fail_by_name() {
    for mutation in ["empty", "subset"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lighter_ws_state_helpers_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_WS_CARDINALITY_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains("flatten corpus inventory"), "{log}");
    }
}
