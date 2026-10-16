include!("exchange.rs");

// Observe the native tagged values directly. Never project native values back
// into the interchange encoding to establish result agreement.
fn class_cost_compare(actual: &V, expected: &J, path: &str, nan_classes: &mut usize) {
    match (actual, expected["kind"].as_str().unwrap()) {
        (V::U, "undefined") | (V::Null, "null") => {}
        (V::Bool(a), "bool") => assert_eq!(
            Some(*a),
            expected["value"].as_bool(),
            "class_cost_native:{path}"
        ),
        (V::Text(a), "text") => {
            let b: Vec<u16> = serde_json::from_value(expected["units"].clone()).unwrap();
            assert_eq!(*a, b, "class_cost_native:{path}");
        }
        (V::Number(a), "number") => {
            let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
            if a.is_nan() && f64::from_bits(bits).is_nan() {
                *nan_classes += 1;
            } else {
                assert_eq!(a.to_bits(), bits, "class_cost_native:{path}");
            }
        }
        (V::Object(_, fields), "object") => {
            let source = expected["fields"].as_array().unwrap();
            let native = fields.borrow();
            assert_eq!(
                native.len(),
                source.len(),
                "class_cost_native_fields:{path}"
            );
            for ((key, value), entry) in native.iter().zip(source) {
                assert_eq!(
                    key,
                    entry[0].as_str().unwrap(),
                    "class_cost_native_key:{path}"
                );
                class_cost_compare(value, &entry[1], &format!("{path}.{key}"), nan_classes);
            }
        }
        (V::Array(_, slots), "array") => {
            let source = expected["items"].as_array().unwrap();
            let native = slots.borrow();
            assert_eq!(
                native.len(),
                source.len(),
                "class_cost_native_length:{path}"
            );
            for (index, (value, entry)) in native.iter().zip(source).enumerate() {
                match value {
                    Some(value) => {
                        class_cost_compare(value, entry, &format!("{path}[{index}]"), nan_classes)
                    }
                    None => assert_eq!(entry["kind"], "hole", "class_cost_native_hole:{path}"),
                }
            }
        }
        _ => panic!("class_cost_native_type:{path}: {actual:?} vs {expected}"),
    }
}
fn class_cost_run_venue(family: &str) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/class_cost_venue.cjs"
        ))
        .arg(family)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "generated venue source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: J = serde_json::from_slice(&output.stdout).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 512, "venue independent case floor");
    let mut nan_classes = 0;
    let mut native_rows = 0;
    let mut successes = 0;
    let mut errors = 0;
    for iteration in 0..512 {
        let case = &cases[iteration];
        let generation = iteration / 16;
        assert_eq!(case["iteration"], iteration, "venue iteration inventory");
        assert_eq!(case["generation"], generation, "venue generation inventory");
        assert_eq!(case["i"], iteration % 16, "venue profile inventory");
        assert_eq!(case["name"], family, "venue family inventory");
        let host = Host::new(case);
        num(
            &host.value("generation"),
            generation as f64,
            "independent generated receiver",
        );
        if family == "markets" {
            let gamma = host.value("gamma");
            len(
                &host,
                &gamma,
                if iteration % 16 == 9 {
                    0
                } else if iteration % 16 == 8 {
                    2
                } else {
                    1
                },
                "independent Gamma input count",
            );
            for item in 0..host.length(&gamma).unwrap() {
                let row = host.at(&gamma, item).unwrap().unwrap();
                txt(
                    &raw_field(&row, "slug"),
                    &format!("{}-{generation}", if item == 0 { "event" } else { "new" }),
                    "independent Gamma slug",
                );
                txt(
                    &raw_field(&row, "question"),
                    &format!("Binary? {generation}"),
                    "independent Gamma question",
                );
                txt(
                    &raw_field(&row, "orderMinSize"),
                    &safe_cex_num::js_number_string(1. + generation as f64 / 4.),
                    "independent Gamma minimum",
                );
            }
        }
        if family == "place" {
            let profile = iteration % 16;
            let opts = host.value("opts");
            if profile == 10 {
                assert!(
                    numeric(&raw_field(&opts, "price")).is_nan(),
                    "independent invalid price"
                );
            } else {
                num(
                    &raw_field(&opts, "price"),
                    if generation == 0 && profile < 2 {
                        0.4376
                    } else if profile == 12 {
                        1.
                    } else {
                        0.1 + (generation % 20) as f64 / 40.
                    },
                    "independent generated price",
                );
            }
            num(
                &raw_field(&opts, "amount"),
                if generation == 0 && profile < 2 {
                    12.34567
                } else if profile == 11 {
                    0.
                } else {
                    5. + generation as f64 / 8.
                },
                "independent generated amount",
            );
            txt(
                &raw_field(&host.value("response"), "orderID"),
                &if generation == 0 {
                    "order-17".to_owned()
                } else {
                    format!("order-{generation}")
                },
                "independent generated order ID",
            );
        }
        if family == "tickers" {
            let count = if iteration % 16 == 0 {
                [0, 1, 99, 100, 101, 201, 257, 513][generation % 8]
            } else {
                generation % 4
            };
            let map = raw_field(&host.receiver, "tokenToSymbol");
            for k in 0..count {
                field_set(&map, &(17 + k).to_string(), text(&format!("event-{k}")));
            }
        }
        let result = match family {
            "markets" => finish(&host, fetch_markets(host.clone())),
            "tickers" => finish(&host, fetch_tickers(host.clone())),
            "place" => finish(&host, place_order(host.clone(), host.value("opts"))),
            _ => unreachable!(),
        };
        if let Ok(V::Array(_, slots)) = &result {
            native_rows += slots.borrow().len();
        }
        if result.is_ok() {
            successes += 1;
        } else {
            errors += 1;
        }
        match result {
            Ok(value) => class_cost_compare(
                &value,
                &case["result"]["value"],
                &format!("{family}/{iteration}/result"),
                &mut nan_classes,
            ),
            Err(error) => {
                assert_eq!(
                    error.marker.map(u64::from),
                    case["result"]["error"]["marker"].as_u64(),
                    "class_cost_native_error_marker"
                );
                class_cost_compare(
                    &error.message,
                    &case["result"]["error"]["message"],
                    &format!("{family}/{iteration}/error"),
                    &mut nan_classes,
                );
                if host.fail("sign") {
                    assert!(
                        Rc::ptr_eq(&error, &host.error),
                        "class_cost_native_signing_error_identity"
                    );
                }
            }
        }
        let events = host.events.borrow();
        let expected = case["events"].as_array().unwrap();
        assert_eq!(
            events.len(),
            expected.len(),
            "class_cost_native_event_count:{family}/{iteration}"
        );
        for (event, source) in events.iter().zip(expected) {
            assert_eq!(
                event.0, source[0],
                "class_cost_native_event_name:{family}/{iteration}"
            );
            let arguments = source[1].as_array().unwrap();
            assert_eq!(
                event.1.len(),
                arguments.len(),
                "class_cost_native_argument_count"
            );
            for (index, (value, want)) in event.1.iter().zip(arguments).enumerate() {
                class_cost_compare(
                    value,
                    want,
                    &format!("{family}/{iteration}/event/{index}"),
                    &mut nan_classes,
                );
            }
        }
        for (key, state) in [
            ("marketMetadata", "metadata"),
            ("tokenToSymbol", "tokens"),
            ("symbolToToken", "symbols"),
        ] {
            class_cost_compare(
                &raw_field(&host.receiver, key),
                &case["state"][state],
                &format!("{family}/{iteration}/state/{key}"),
                &mut nan_classes,
            );
        }
        let logs = host.logs.borrow();
        let source = case["logs"].as_array().unwrap();
        assert_eq!(logs.len(), source.len(), "class_cost_native_log_count");
        for ((message, severity), expected) in logs.iter().zip(source) {
            let wanted: Vec<u16> = expected[0].as_str().unwrap().encode_utf16().collect();
            assert_eq!(
                message.encode_utf16().collect::<Vec<_>>(),
                wanted,
                "class_cost_native_log_message"
            );
            assert_eq!(*severity, expected[1], "class_cost_native_log_severity");
        }
    }
    assert_eq!(
        (native_rows, successes, errors),
        match family {
            "markets" => (864, 512, 0),
            "tickers" => (5760, 512, 0),
            "place" => (64, 64, 448),
            _ => unreachable!(),
        },
        "class_cost_native_independent_outcome_totals:{family}"
    );
    eprintln!(
        "{family}: 512 generated native outcomes; NaN-class (not payload-bit) comparisons: {nan_classes}; stop iteration511"
    );
}
#[test]
fn class_cost_markets_native() {
    class_cost_run_venue("markets")
}
#[test]
fn class_cost_tickers_native() {
    class_cost_run_venue("tickers")
}
#[test]
fn class_cost_place_native() {
    class_cost_run_venue("place")
}
