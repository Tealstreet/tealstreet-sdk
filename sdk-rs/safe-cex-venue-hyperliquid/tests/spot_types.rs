use safe_cex_venue_hyperliquid::spot_types::*;
use serde_json::Value;

fn source() -> &'static Value {
    static SOURCE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-types.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "SPOT source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

#[test]
fn native_spot_tables() {
    let source = source();
    let mut failures = Vec::new();
    let mut roots = 0;
    macro_rules! table {
        ($name:ident, $count:literal) => {{
            roots += 1;
            if std::panic::catch_unwind(|| {
                let expected = source["constants"][stringify!($name)].as_array().unwrap();
                assert_eq!(
                    expected.len(),
                    $count,
                    "SPOT source table floor {}",
                    stringify!($name)
                );
                let expected: Vec<_> = expected
                    .iter()
                    .map(|pair| (pair[0].as_str().unwrap(), pair[1].as_str().unwrap()))
                    .collect();
                assert_eq!($name, expected, "SPOT native table {}", stringify!($name));
            })
            .is_err()
            {
                failures.push(stringify!($name));
            }
        }};
    }
    macro_rules! array {
        ($name:ident) => {{
            roots += 1;
            if std::panic::catch_unwind(|| {
                let expected = source["constants"][stringify!($name)].as_array().unwrap();
                assert_eq!(
                    expected.len(),
                    1,
                    "SPOT source array floor {}",
                    stringify!($name)
                );
                let expected: Vec<_> = expected
                    .iter()
                    .map(|value| value.as_str().unwrap())
                    .collect();
                assert_eq!($name, expected, "SPOT native table {}", stringify!($name));
            })
            .is_err()
            {
                failures.push(stringify!($name));
            }
        }};
    }
    table!(BASE_URLS, 2);
    table!(WSS_URLS, 2);
    table!(ENDPOINTS, 2);
    array!(GRAFANA_ENDPOINTS_EVERY);
    array!(GRAFANA_ENDPOINTS_PERIODIC);
    table!(INFO_TYPES, 18);
    table!(EXCHANGE_TYPES, 14);
    table!(WEBSOCKET, 2);
    assert_eq!(roots, 8, "SPOT table root iteration stop");
    assert!(
        failures.is_empty(),
        "SPOT named native table failures: {failures:?}"
    );
}

fn tif(value: &Value) -> Tif {
    match value.as_str().unwrap() {
        "Alo" => Tif::Alo,
        "Ioc" => Tif::Ioc,
        "Gtc" => Tif::Gtc,
        other => panic!("unexpected source Tif {other}"),
    }
}
fn tpsl(value: &Value) -> Tpsl {
    match value.as_str().unwrap() {
        "tp" => Tpsl::Tp,
        "sl" => Tpsl::Sl,
        other => panic!("unexpected source Tpsl {other}"),
    }
}
fn trigger(native: &TriggerOrderType, expected: &Value, index: usize, owner: &str) {
    assert_eq!(
        native.trigger_px.to_bits(),
        u64::from_str_radix(expected["priceBits"].as_str().unwrap(), 16).unwrap(),
        "SPOT native {owner} price bits case {index}"
    );
    assert_eq!(
        native.is_market,
        expected["isMarket"].as_bool().unwrap(),
        "SPOT native {owner} market flag case {index}"
    );
    assert_eq!(
        native.tpsl,
        tpsl(&expected["tpsl"]),
        "SPOT native {owner} TP/SL case {index}"
    );
}

// Native variants, members, optional presence and binary64 bits are asserted
// directly. No encode/roundtrip assertion supplies the expected identity.
macro_rules! cases {
    ($test:ident, $type:ident, $count:literal, $assertion:expr) => {
        #[test]
        fn $test() {
            let source = source();
            assert_eq!(
                source["iterationStop"], 47,
                "SPOT whole source case iteration stop"
            );
            assert_eq!(
                source["closedDeclarations"].as_array().unwrap().len(),
                6,
                "SPOT source declaration floor"
            );
            let rows: Vec<_> = source["cases"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, row)| row["type"] == stringify!($type))
                .collect();
            assert_eq!(
                rows.len(),
                $count,
                "SPOT source {} case floor",
                stringify!($type)
            );
            for (index, row) in rows {
                let decoded = serde_json::from_str::<$type>(row["input"].as_str().unwrap());
                if row["accepted"].as_bool().unwrap() {
                    let native = decoded.unwrap_or_else(|error| {
                        panic!(
                            "SPOT native {} accepted case {index}: {error}",
                            stringify!($type)
                        )
                    });
                    ($assertion)(&native, &row["native"], index);
                } else {
                    assert!(
                        decoded.is_err(),
                        "SPOT native {} rejected case {index}: {decoded:?}",
                        stringify!($type)
                    );
                }
            }
        }
    };
}
cases!(
    native_spot_tif,
    Tif,
    7,
    |native: &Tif, expected: &Value, index| {
        assert_eq!(
            *native,
            tif(expected),
            "SPOT native Tif variant case {index}"
        );
    }
);
cases!(
    native_spot_tpsl,
    Tpsl,
    6,
    |native: &Tpsl, expected: &Value, index| {
        assert_eq!(
            *native,
            tpsl(expected),
            "SPOT native Tpsl variant case {index}"
        );
    }
);
cases!(
    native_spot_grouping,
    Grouping,
    7,
    |native: &Grouping, expected: &Value, index| {
        let expected = match expected.as_str().unwrap() {
            "na" => Grouping::Na,
            "normalTpsl" => Grouping::NormalTpsl,
            "positionTpsl" => Grouping::PositionTpsl,
            other => panic!("unexpected source Grouping {other}"),
        };
        assert_eq!(
            *native, expected,
            "SPOT native Grouping variant case {index}"
        );
    }
);
cases!(
    native_spot_limit,
    LimitOrderType,
    7,
    |native: &LimitOrderType, expected: &Value, index| {
        assert_eq!(
            native.tif,
            tif(expected),
            "SPOT native LimitOrderType TIF case {index}"
        );
    }
);
cases!(
    native_spot_trigger,
    TriggerOrderType,
    9,
    |native: &TriggerOrderType, expected: &Value, index| {
        trigger(native, expected, index, "TriggerOrderType");
    }
);
cases!(
    native_spot_order,
    OrderType,
    11,
    |native: &OrderType, expected: &Value, index| {
        assert_eq!(
            native.limit.is_some(),
            !expected["limit"].is_null(),
            "SPOT native OrderType limit presence case {index}"
        );
        if let Some(limit) = &native.limit {
            assert_eq!(
                limit.tif,
                tif(&expected["limit"]),
                "SPOT native OrderType limit TIF case {index}"
            );
        }
        assert_eq!(
            native.trigger.is_some(),
            !expected["trigger"].is_null(),
            "SPOT native OrderType trigger presence case {index}"
        );
        if let Some(value) = &native.trigger {
            trigger(value, &expected["trigger"], index, "OrderType");
        }
    }
);
