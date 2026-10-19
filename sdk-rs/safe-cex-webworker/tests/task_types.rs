use safe_cex_webworker::task_types::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn numeric(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numeric).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numeric(v))).collect()),
        other => other,
    }
}
fn record<T: DeserializeOwned + Serialize>(
    schema: &Value,
    fields: &[&str],
    optional: &[&str],
) -> (usize, usize) {
    assert_eq!(
        schema["fields"],
        json!(fields),
        "RECORD-CARDINALITY source fields"
    );
    assert_eq!(
        schema["optional"],
        json!(optional),
        "RECORD-CARDINALITY source optional"
    );
    let mut field_checks = 0;
    let mut optional_checks = 0;
    let sample = &schema["sample"];
    let parsed: T = serde_json::from_value(sample.clone()).unwrap();
    assert_eq!(
        numeric(serde_json::to_value(parsed).unwrap()),
        numeric(sample.clone())
    );
    for shape in [
        json!([]),
        json!([1, 2]),
        Value::Null,
        json!(false),
        json!("x"),
    ] {
        assert!(T::deserialize(shape).is_err());
    }
    for &key in optional {
        assert!(
            fields.contains(&key),
            "RECORD-CARDINALITY optional field membership {key}"
        );
        assert!(
            sample.get(key).is_some(),
            "RECORD-CARDINALITY optional sample key {key}"
        );
        let mut input = sample.clone();
        input[key] = Value::Null;
        assert!(T::deserialize(input).is_err(), "nullable optional {key}");
        optional_checks += 1;
    }
    for &key in fields {
        let mut input = sample.clone();
        assert!(
            input.as_object_mut().unwrap().remove(key).is_some(),
            "RECORD-CARDINALITY requiredness sample key {key}"
        );
        let optional = optional.contains(&key);
        assert_eq!(
            T::deserialize(input).is_ok(),
            optional,
            "requiredness {key}"
        );
        field_checks += 1;
    }
    (field_checks, optional_checks)
}
#[test]
fn task_records_follow_source_fields_and_reject_positional_and_optional_null_inputs() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/task_types_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema: Value = serde_json::from_slice(&output.stdout).unwrap();
    macro_rules! qualified_record {
        ($ty:ident) => {
            assert!(
                $ty::deserialize(json!([])).is_err(),
                "public qualified decoder for {}",
                stringify!($ty)
            );
        };
    }
    qualified_record!(ChaserTerminate);
    qualified_record!(ChaserOrderOptions);
    qualified_record!(ChaserOrderState);
    qualified_record!(TwapOptions);
    qualified_record!(TwapState);
    for kind in [
        json!("other"),
        json!("PRICE"),
        json!({"price":null}),
        json!(["price"]),
        json!(null),
    ] {
        let mut value = schema["records"]["ChaserTerminate"]["sample"].clone();
        value["type"] = kind;
        assert!(ChaserTerminate::deserialize(value).is_err());
    }
    let mut numeric_fields = std::collections::BTreeSet::new();
    let tokens = ["-0", "-0.0", "0", "1e400", "-1e400", "1e-400", "-1e-400"];
    let source_tokens = schema["ingressNumbers"].as_object().unwrap();
    assert_eq!(source_tokens.len(), 7, "INGRESS-CARDINALITY token total");
    let mut numeric_comparisons = 0;
    let mut lexical_zero_checks = 0;
    macro_rules! probe_ingress {
        ($ty:ident, $key:literal, $read:expr) => {
            numeric_fields.insert((stringify!($ty).to_owned(), $key.to_owned()));
            for token in tokens {
                let bits = source_tokens
                    .get(token)
                    .expect("INGRESS-CARDINALITY missing fixed token");
                let expected = u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap();
                let mut sample = schema["records"][stringify!($ty)]["sample"].clone();
                sample[$key] = json!("PROBE_NUMBER");
                let raw = serde_json::to_string(&sample)
                    .unwrap()
                    .replace("\"PROBE_NUMBER\"", token);
                let direct: $ty = serde_json::from_str(&raw).unwrap();
                assert_eq!(
                    ($read)(&direct).to_bits(),
                    expected,
                    "RAW-INGRESS {}.{} {token}",
                    stringify!($ty),
                    $key
                );
                numeric_comparisons += 1;
                let buffered_input: Value = serde_json::from_str(&raw).unwrap();
                if token == "-0" {
                    assert_eq!(
                        buffered_input[$key].as_f64().unwrap().to_bits(),
                        expected,
                        "VALUE-BEFORE-DECODER {}.{} {token}",
                        stringify!($ty),
                        $key
                    );
                    lexical_zero_checks += 1;
                }
                let buffered: $ty = serde_json::from_value(buffered_input).unwrap();
                assert_eq!(
                    ($read)(&buffered).to_bits(),
                    expected,
                    "REPAIRED VALUE-INGRESS {}.{} {token}",
                    stringify!($ty),
                    $key
                );
                numeric_comparisons += 1;
                let mut value = schema["records"][stringify!($ty)]["sample"].clone();
                value[$key] = if expected == (-0.0_f64).to_bits() {
                    json!(-0.0)
                } else {
                    serde_json::from_str(token).unwrap()
                };
                let buffered: $ty = serde_json::from_value(value).unwrap();
                assert_eq!(
                    ($read)(&buffered).to_bits(),
                    expected,
                    "VALUE-INGRESS {}.{} {token}",
                    stringify!($ty),
                    $key
                );
                numeric_comparisons += 1;
            }
        };
    }
    probe_ingress!(ChaserTerminate, "value", |v: &ChaserTerminate| v.value);
    probe_ingress!(ChaserTerminate, "anchorPrice", |v: &ChaserTerminate| v
        .anchor_price);
    probe_ingress!(ChaserOrderOptions, "size", |v: &ChaserOrderOptions| v.size);
    probe_ingress!(
        ChaserOrderOptions,
        "maxAttempts",
        |v: &ChaserOrderOptions| v.max_attempts
    );
    probe_ingress!(ChaserOrderOptions, "delay", |v: &ChaserOrderOptions| v
        .delay);
    probe_ingress!(ChaserOrderState, "attempts", |v: &ChaserOrderState| v
        .attempts);
    probe_ingress!(TwapOptions, "size", |v: &TwapOptions| v.size);
    probe_ingress!(TwapOptions, "duration", |v: &TwapOptions| v.duration);
    probe_ingress!(TwapOptions, "ordersCount", |v: &TwapOptions| v.orders_count);
    probe_ingress!(TwapOptions, "chaserDelay", |v: &TwapOptions| v.chaser_delay);
    probe_ingress!(TwapOptions, "randomnessPercentage", |v: &TwapOptions| v
        .randomness_percentage);
    probe_ingress!(TwapOptions, "timeoutPct", |v: &TwapOptions| v
        .timeout_pct
        .unwrap());
    probe_ingress!(TwapState, "size", |v: &TwapState| v.size);
    probe_ingress!(TwapState, "ordersCount", |v: &TwapState| v.orders_count);
    probe_ingress!(TwapState, "ordersExecuted", |v: &TwapState| v
        .orders_executed);
    probe_ingress!(TwapState, "nextOrderAt", |v: &TwapState| v.next_order_at);
    probe_ingress!(TwapState, "createdAt", |v: &TwapState| v
        .created_at
        .unwrap());
    probe_ingress!(TwapState, "startedAt", |v: &TwapState| v
        .started_at
        .unwrap());
    assert_eq!(
        lexical_zero_checks, 18,
        "INGRESS-CARDINALITY lexical negative-zero inputs"
    );
    assert_eq!(
        numeric_comparisons, 378,
        "INGRESS-CARDINALITY native bit comparisons"
    );
    assert_eq!(
        numeric_fields.len(),
        18,
        "INGRESS-CARDINALITY native field total"
    );
    let source_numeric_fields: std::collections::BTreeSet<_> = schema["records"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(name, _)| name.as_str() != "ChaserCompleteEvent")
        .flat_map(|(name, record)| {
            record["numericFields"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |field| (name.clone(), field.as_str().unwrap().to_owned()))
        })
        .collect();
    assert_eq!(
        numeric_fields, source_numeric_fields,
        "INGRESS-COVERAGE every source scalar numeric task field"
    );
    // Independent source declarations pin execution even if an oracle inventory shrinks.
    let record_checks = [
        record::<ChaserTerminate>(
            &schema["records"]["ChaserTerminate"],
            &["type", "value", "anchorPrice"],
            &[],
        ),
        record::<ChaserOrderOptions>(
            &schema["records"]["ChaserOrderOptions"],
            &[
                "symbol",
                "size",
                "side",
                "maxAttempts",
                "reduceOnly",
                "delay",
                "origin",
                "accountName",
                "terminate",
                "clientId",
            ],
            &[
                "reduceOnly",
                "origin",
                "accountName",
                "terminate",
                "clientId",
            ],
        ),
        record::<ChaserOrderState>(
            &schema["records"]["ChaserOrderState"],
            &["attempts", "isStopped", "stopReason", "currentOrdersIds"],
            &[],
        ),
        record::<TwapOptions>(
            &schema["records"]["TwapOptions"],
            &[
                "accountName",
                "activeMarket",
                "size",
                "duration",
                "ordersCount",
                "side",
                "chaserDelay",
                "reduceOnly",
                "randomnessPercentage",
                "useLimitOrders",
                "useChaserOrders",
                "timeoutPct",
                "timeoutAction",
                "failAction",
                "origin",
            ],
            &["timeoutPct", "timeoutAction", "failAction", "origin"],
        ),
        record::<TwapState>(
            &schema["records"]["TwapState"],
            &[
                "accountName",
                "activeMarket",
                "size",
                "ordersSizes",
                "side",
                "status",
                "ordersCount",
                "ordersExecuted",
                "nextOrderAt",
                "markForCleanup",
                "origin",
                "createdAt",
                "startedAt",
            ],
            &["markForCleanup", "origin", "createdAt", "startedAt"],
        ),
    ];
    assert_eq!(
        record_checks,
        [(3, 0), (10, 5), (4, 0), (15, 4), (13, 4)],
        "RECORD-CARDINALITY executed field and optional checks"
    );
    assert_eq!(CHASER_KEPT_ORDER_STOP_REASON, schema["kept"]);
    assert_eq!(
        schema["unions"]["ChaserTerminateKind"],
        json!(["price", "percent", "usd"]),
        "RECORD-CARDINALITY source termination literals"
    );
    let mut union_checks = 0;
    let termination_literals = [
        ("price", ChaserTerminateKind::Price),
        ("percent", ChaserTerminateKind::Percent),
        ("usd", ChaserTerminateKind::Usd),
    ];
    // Complete native identity checks before any wire check can use the serializer.
    for (name, expected) in termination_literals {
        let kind = ChaserTerminateKind::deserialize(json!(name)).unwrap();
        assert_eq!(kind, expected, "RECORD-IDENTITY native termination {name}");
        union_checks += 1;
    }
    for (name, expected) in termination_literals {
        assert_eq!(
            serde_json::to_value(expected).unwrap(),
            json!(name),
            "RECORD-WIRE native termination {name}"
        );
        union_checks += 1;
        for rejected in [
            json!({name:null}),
            json!([name]),
            json!(name.to_uppercase()),
            json!(format!(" {name}")),
            json!(format!("{name}\u{feff}")),
        ] {
            assert!(ChaserTerminateKind::deserialize(rejected).is_err());
            union_checks += 1;
        }
    }
    assert_eq!(
        union_checks, 21,
        "RECORD-CARDINALITY executed termination checks"
    );
    for rejected in [
        Value::Null,
        json!(1),
        json!(true),
        json!({}),
        json!([]),
        json!("unknown"),
    ] {
        assert!(ChaserTerminateKind::deserialize(rejected).is_err());
    }
    let mut chaser = schema["records"]["ChaserOrderOptions"]["sample"].clone();
    chaser["side"] = json!({"buy":null});
    assert!(ChaserOrderOptions::deserialize(chaser).is_err());
    let mut twap = schema["records"]["TwapOptions"]["sample"].clone();
    twap["activeMarket"]["precision"] = json!([0.01, 0.01]);
    assert!(TwapOptions::deserialize(twap).is_err());
    let mut state = schema["records"]["TwapState"]["sample"].clone();
    state["status"] = json!({"running":null});
    assert!(TwapState::deserialize(state).is_err());
    // P4 strict decoders (3bbc52657f) repair these previously accepted tagged forms.
    assert!(ChaserOrigin::deserialize(json!({"web":null})).is_err());
    assert!(TwapOrigin::deserialize(json!({"cli":null})).is_err());
    assert!(TwapTimeoutAction::deserialize(json!({"market":null})).is_err());
    assert!(TwapFailAction::deserialize(json!({"next":null})).is_err());
    assert!(TwapStatus::deserialize(json!({"running":null})).is_err());
}
