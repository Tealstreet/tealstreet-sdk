use safe_cex_store::{account_environment::*, config::*, instrument_class::*, telemetry::*};
use safe_cex_types::{
    Market,
    telemetry::{TelemetryContext, TelemetrySink},
};
use serde_json::{Value, json};
use std::{any::Any, cell::RefCell, collections::HashMap, process::Command, rc::Rc};

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/root_utils_oracle.cjs"
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
fn root_config_matches_native_values_and_live_source_initializers() {
    for (actual, expected) in [
        (WS_MAX_RECONNECTION_DELAY, 5000.0_f64),
        (WS_BACKOFF_BASE_DELAY, 2000.0),
        (WS_BACKOFF_MAX_DELAY, 30000.0),
        (WS_BACKOFF_FACTOR, 2.0),
        (WS_FAILURE_STREAK_PAUSE_MS, 120000.0),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "ROOT-UTILS config native constant identity"
        );
    }
    assert_eq!(WS_FAILURE_STREAK_PAUSE_THRESHOLD, 8);
    let reconnecting_enabled = ENABLE_RECONNECTING_WEBSOCKET;
    assert!(!reconnecting_enabled, "ROOT-UTILS reconnecting native flag");
    let cases = [
        (0.25, Some("0x10"), Some("NaN"), 1500.0_f64, true, false),
        (
            0.75,
            Some("Infinity"),
            Some("\u{feff}-0\u{feff}"),
            2500.0,
            true,
            false,
        ),
        (0.0, None, None, 1000.0, false, false),
        (0.5, None, Some("0x10"), 2000.0, false, true),
    ];
    let mut native = Vec::new();
    for (random, simulate, debug, expected_min, expected_simulate, expected_debug) in cases {
        let tuning = reconnecting_websocket_tuning(random);
        assert_eq!(
            ws_min_reconnection_delay(random).to_bits(),
            expected_min.to_bits()
        );
        assert_eq!(
            tuning.min_reconnection_delay.to_bits(),
            expected_min.to_bits()
        );
        assert_eq!(
            tuning.max_reconnection_delay.to_bits(),
            5000.0_f64.to_bits()
        );
        assert_eq!(simulate_http_over_websocket(simulate), expected_simulate);
        assert_eq!(
            enable_ws_lifecycle_debug_logs(debug),
            expected_debug,
            "ROOT-UTILS lifecycle debug native flag"
        );
        native.push((
            tuning,
            simulate_http_over_websocket(simulate),
            enable_ws_lifecycle_debug_logs(debug),
        ));
    }
    // Source JSON is only compared after native field identities above.
    let source = oracle();
    assert_eq!(source["config"].as_array().unwrap().len(), 4);
    for (index, (tuning, simulate, debug)) in native.iter().enumerate() {
        let row = &source["config"][index];
        for (key, actual) in [
            ("WS_MAX_RECONNECTION_DELAY", WS_MAX_RECONNECTION_DELAY),
            ("WS_MIN_RECONNECTION_DELAY", tuning.min_reconnection_delay),
            ("WS_BACKOFF_BASE_DELAY", WS_BACKOFF_BASE_DELAY),
            ("WS_BACKOFF_MAX_DELAY", WS_BACKOFF_MAX_DELAY),
            ("WS_BACKOFF_FACTOR", WS_BACKOFF_FACTOR),
            ("WS_FAILURE_STREAK_PAUSE_MS", WS_FAILURE_STREAK_PAUSE_MS),
        ] {
            assert_eq!(
                actual.to_bits(),
                row[key].as_f64().unwrap().to_bits(),
                "ROOT-UTILS live config {key}"
            );
        }
        assert_eq!(
            row["RECONNECTING_WEBSOCKET_TUNING"]["maxReconnectionDelay"].as_f64(),
            Some(tuning.max_reconnection_delay)
        );
        assert_eq!(
            row["RECONNECTING_WEBSOCKET_TUNING"]["minReconnectionDelay"].as_f64(),
            Some(tuning.min_reconnection_delay)
        );
        assert_eq!(row["WS_FAILURE_STREAK_PAUSE_THRESHOLD"], 8);
        assert_eq!(row["ENABLE_RECONNECTING_WEBSOCKET"], false);
        assert_eq!(
            row["SIMULATE_HTTP_OVER_WEBSOCKET"].as_bool(),
            Some(*simulate)
        );
        assert_eq!(
            row["ENABLE_WS_LIFECYCLE_DEBUG_LOGS"].as_bool(),
            Some(*debug)
        );
    }
}

#[test]
fn root_account_helpers_keep_field_identity_fallback_and_read_order() {
    let env: HashMap<&str, &str> = [
        ("NEXT_PUBLIC_KUCOIN_BROKER_PARTNER", " global partner "),
        ("NEXT_PUBLIC_KUCOIN_BROKER_KEY", " global key "),
        ("NEXT_PUBLIC_KUCOIN_BROKER_NAME", " global name "),
        ("NEXT_PUBLIC_KUCOIN_BROKER_VERIFY", "yes"),
        (
            "NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_PARTNER",
            "\u{feff} futures partner \u{feff}",
        ),
        ("NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_KEY", " "),
        ("NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_NAME", "futures name"),
        ("NEXT_PUBLIC_KUCOIN_SPOT_BROKER_PARTNER", "spot partner"),
        ("NEXT_PUBLIC_KUCOIN_SPOT_BROKER_VERIFY", "unexpected"),
    ]
    .into_iter()
    .collect();
    let lookup = |key: &str| env.get(key).map(|value| (*value).to_owned());
    assert_eq!(
        get_public_env("NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_PARTNER", &lookup).as_deref(),
        Some("futures partner")
    );
    assert_eq!(
        get_public_env("NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_KEY", &lookup),
        None
    );
    assert_eq!(get_public_env("absent", &lookup), None);
    assert_eq!(
        get_kucoin_broker_env("NEXT_PUBLIC_KUCOIN_FUTURES_BROKER", "KEY", &lookup).as_deref(),
        Some("global key"),
        "ROOT-UTILS broker fallback identity"
    );
    let futures = resolve_public_kucoin_broker_config("kucoin", &lookup).unwrap();
    let spot = resolve_public_kucoin_broker_config("kucoinspot", &lookup).unwrap();
    for (native, partner, key, name, verify) in [
        (
            &futures,
            "futures partner",
            "global key",
            "futures name",
            Some(true),
        ),
        (&spot, "spot partner", "global key", "global name", None),
    ] {
        assert_eq!(
            native.partner, partner,
            "ROOT-UTILS broker partner identity"
        );
        assert_eq!(native.broker_key, key, "ROOT-UTILS broker key identity");
        assert_eq!(native.broker_name, name, "ROOT-UTILS broker name identity");
        assert_eq!(native.verify, verify, "ROOT-UTILS broker verify identity");
    }
    assert_eq!(
        resolve_public_kucoin_broker_config("lighter", &lookup),
        None
    );
    let inputs = [
        None,
        Some(""),
        Some(" TRUE "),
        Some("1"),
        Some("Yes"),
        Some("false"),
        Some(" 0 "),
        Some("NO"),
        Some("unknown"),
        Some("\u{feff}true\u{feff}"),
    ];
    let expected = [
        None,
        None,
        Some(true),
        Some(true),
        Some(true),
        Some(false),
        Some(false),
        Some(false),
        None,
        Some(true),
    ];
    for (input, expected) in inputs.into_iter().zip(expected) {
        assert_eq!(
            parse_kucoin_broker_verify(input),
            expected,
            "ROOT-UTILS native verify branch"
        );
    }
    let lighter = ["lighter", "lighter-robinhood", "kucoin", "LIGHTER"].map(is_lighter_exchange);
    assert_eq!(lighter, [true, true, false, false]);
    let reads = RefCell::new(Vec::new());
    assert_eq!(
        resolve_public_kucoin_broker_config("kucoin", &|name| {
            reads.borrow_mut().push(name.to_owned());
            None
        }),
        None
    );
    let expected_reads = [
        "NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_PARTNER",
        "NEXT_PUBLIC_KUCOIN_BROKER_PARTNER",
        "NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_KEY",
        "NEXT_PUBLIC_KUCOIN_BROKER_KEY",
        "NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_NAME",
        "NEXT_PUBLIC_KUCOIN_BROKER_NAME",
    ];
    assert_eq!(
        *reads.borrow(),
        expected_reads,
        "ROOT-UTILS native required reads before refusal"
    );
    let source = oracle();
    for (index, native) in [&futures, &spot].into_iter().enumerate() {
        let row = &source["brokers"][index];
        assert_eq!(native.partner, row["partner"].as_str().unwrap());
        assert_eq!(native.broker_key, row["brokerKey"].as_str().unwrap());
        assert_eq!(native.broker_name, row["brokerName"].as_str().unwrap());
        assert_eq!(native.verify, row.get("verify").and_then(Value::as_bool));
    }
    assert_eq!(source["brokers"].as_array().unwrap().len(), 3);
    assert_eq!(source["brokers"][2], Value::Null);
    assert_eq!(source["verifies"], json!(expected));
    assert_eq!(source["lighter"], json!(lighter));
    assert_eq!(source["reads"], json!(expected_reads));
}

struct RecordingSink {
    trace: Rc<RefCell<Vec<String>>>,
    data: Rc<dyn Any>,
    context: Rc<TelemetryContext>,
    error: Rc<dyn Any>,
    replace: Option<Rc<dyn TelemetrySink>>,
    fail: bool,
}
impl TelemetrySink for RecordingSink {
    fn event(&self, name: &str, data: Rc<dyn Any>) -> Result<(), Rc<dyn Any>> {
        assert!(
            Rc::ptr_eq(&data, &self.data),
            "ROOT-UTILS telemetry native data identity"
        );
        if self.fail {
            return Err(self.error.clone());
        }
        self.trace.borrow_mut().push(name.to_owned());
        if let Some(next) = &self.replace {
            set_telemetry_sink(next.clone());
        }
        Ok(())
    }
    fn capture_exception(
        &self,
        error: Rc<dyn Any>,
        context: Rc<TelemetryContext>,
    ) -> Result<(), Rc<dyn Any>> {
        assert!(
            Rc::ptr_eq(&error, &self.error),
            "ROOT-UTILS telemetry native error identity"
        );
        assert!(
            Rc::ptr_eq(&context, &self.context),
            "ROOT-UTILS telemetry native context identity"
        );
        if self.fail {
            return Err(self.error.clone());
        }
        self.trace.borrow_mut().push("exception".into());
        Ok(())
    }
}

#[test]
fn root_telemetry_retains_native_identity_reentrancy_and_failure() {
    struct Empty;
    impl TelemetrySink for Empty {}
    set_telemetry_sink(Rc::new(Empty));
    let data: Rc<dyn Any> = Rc::new(41_u32);
    let error: Rc<dyn Any> = Rc::new("opaque");
    let tags: Rc<dyn Any> = Rc::new("example");
    let extra: Rc<dyn Any> = Rc::new(f64::INFINITY);
    let context = Rc::new(TelemetryContext {
        tags: Some(tags.clone()),
        extra: Some(extra.clone()),
    });
    assert!(Rc::ptr_eq(context.tags.as_ref().unwrap(), &tags));
    assert!(Rc::ptr_eq(context.extra.as_ref().unwrap(), &extra));
    assert_eq!(
        *context
            .extra
            .as_ref()
            .unwrap()
            .downcast_ref::<f64>()
            .unwrap(),
        f64::INFINITY
    );
    assert!(report_event("empty", data.clone()).is_ok());
    assert!(report_exception(error.clone(), context.clone()).is_ok());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let sink = |replace, fail| {
        Rc::new(RecordingSink {
            trace: trace.clone(),
            data: data.clone(),
            context: context.clone(),
            error: error.clone(),
            replace,
            fail,
        })
    };
    let next: Rc<dyn TelemetrySink> = sink(None, false);
    set_telemetry_sink(sink(Some(next), false));
    assert!(report_event("first", data.clone()).is_ok());
    assert!(report_event("next", data.clone()).is_ok());
    assert!(report_exception(error.clone(), context.clone()).is_ok());
    assert_eq!(
        *trace.borrow(),
        ["first", "next", "exception"],
        "ROOT-UTILS telemetry native callback trace"
    );
    set_telemetry_sink(sink(None, true));
    let Err(failure) = report_event("throws", data) else {
        panic!("ROOT-UTILS telemetry native failure propagation");
    };
    assert!(
        Rc::ptr_eq(&failure, &error),
        "ROOT-UTILS telemetry native failure identity"
    );
    let Err(failure) = report_exception(error.clone(), context) else {
        panic!("ROOT-UTILS telemetry native exception failure propagation");
    };
    assert!(
        Rc::ptr_eq(&failure, &error),
        "ROOT-UTILS telemetry native exception failure identity"
    );
    assert_eq!(oracle()["trace"], json!(*trace.borrow()));
    set_telemetry_sink(Rc::new(Empty));
}

#[test]
fn root_instrument_registry_keeps_four_native_entries_and_classifiers() {
    assert_eq!(VENUE_INSTRUMENT_CLASS.len(), 4);
    assert_eq!(
        venues_with_instrument_class_fn(),
        [
            InstrumentClassVenue::Bybit,
            InstrumentClassVenue::Bybitdemo,
            InstrumentClassVenue::Bybitinverse,
            InstrumentClassVenue::Bybitspot
        ]
    );
    let market: Market = serde_json::from_value(json!({"id":"X","symbol":"X","base":"X","quote":"USDT",
        "active":true,"precision":{"amount":1,"price":0.01},"limits":{"amount":{"min":1,"max":100},
        "leverage":{"min":1,"max":10}},"info":{"symbolType":"stock","underlyingTicker":" X ","marketRegion":" US "}})).unwrap();
    for (_, classify) in VENUE_INSTRUMENT_CLASS {
        assert_eq!(
            classify(&market),
            InstrumentClassResult::Classified {
                asset_class: AssetClass::Equity,
                venue_label: "stock".into(),
                underlying_ticker: Some("X".into()),
                market_region: Some("US".into()),
            },
            "ROOT-UTILS registry native classifier identity"
        );
    }
}
