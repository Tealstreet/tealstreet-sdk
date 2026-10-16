// Reuse the native host and direct comparators, not re-encoded Rust output.
include!("order_adapters.rs");

const PROFILE_LENGTHS: [usize; 32] = [
    0, 1, 2, 3, 15, 16, 17, 31, 32, 33, 63, 64, 65, 66, 127, 128, 129, 255, 256, 257, 511, 512,
    513, 1023, 1024, 1025, 47, 48, 49, 95, 96, 97,
];
const PROFILE_PRICES: [&str; 32] = [
    "0",
    "-0",
    "1",
    "-1",
    ".5",
    "1e-4000",
    "-1e-4000",
    "1e309",
    "-1e309",
    "NaN",
    "Infinity",
    "-Infinity",
    "0x10",
    "0.125tail",
    "",
    " ",
    "\u{a0}0.25tail",
    "1e-323",
    "5e-324",
    "2.2250738585072014e-308",
    "1.7976931348623157e308",
    "9007199254740991",
    "9007199254740992",
    "9007199254740993",
    "1e2x",
    ".",
    "+",
    "1e-2",
    "0.0000000000000001",
    "-0tail",
    "3.4028234663852886e38",
    "1.0000000000000002",
];
fn generated_host(venue: &'static str, name: &'static str, iteration: usize) -> Rc<Host> {
    let i = iteration % 16 % 12;
    let generation = iteration / 16;
    let host = Rc::new(Host::new(venue, name, i));
    let V::Object(input) = &host.input else {
        unreachable!()
    };
    let mut fields = input.fields.borrow_mut();
    fields.insert("id", text(&"x".repeat(PROFILE_LENGTHS[generation])));
    fields.insert("asset_id", text(&format!("asset-{iteration}")));
    if ![2, 3].contains(&i) {
        fields.insert(
            "side",
            text(if generation.is_multiple_of(2) {
                "BUY"
            } else {
                "SELL"
            }),
        );
    }
    if ![4, 7, 10].contains(&i) && !([6, 11].contains(&i) && name != "handleOrderUpdate") && i != 11
    {
        fields.insert("price", text(PROFILE_PRICES[generation]));
    }
    if i != 8 {
        fields.insert("original_size", text(&format!("{}tail", 3 + generation)));
    }
    fields.insert("size_matched", text(&(1 + generation % 3).to_string()));
    drop(fields);
    host
}
fn resolution_run(family: &str, venue: &'static str, name: &'static str) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/resolution_cost.cjs"
        ))
        .arg(family)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "resolution source runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 512, "resolution_independent_case_floor");
    let (mut successes, mut failures, mut stored, mut fills, mut boundary65) = (0, 0, 0, 0, 0);
    for iteration in 0..512 {
        let case = &cases[iteration];
        let i = iteration % 16 % 12;
        let generation = iteration / 16;
        assert_eq!(case["iteration"], iteration, "resolution_iteration");
        assert_eq!(case["generation"], generation, "resolution_generation");
        assert_eq!(case["index"], i, "resolution_exception_profile");
        assert_eq!(case["venue"], venue);
        assert_eq!(case["name"], name);
        let host = generated_host(venue, name, iteration);
        if PROFILE_LENGTHS[generation] == 65 {
            boundary65 += 1;
        }
        let result = if name == "mapOrder" {
            let native = if venue == "exchange" {
                map_exchange_order(&*host, &host.input)
            } else {
                map_private_order(&*host, &host.input)
            };
            match native {
                Ok(order) => {
                    // Independent native witnesses precede all source result comparisons.
                    let V::Text(id) = &order.id else {
                        panic!("resolution_native_id_kind")
                    };
                    assert_eq!(
                        *id,
                        vec![120_u16; PROFILE_LENGTHS[generation]],
                        "resolution_native_id:{family}:{iteration}"
                    );
                    assert_eq!(
                        order.amount.to_bits(),
                        (if i == 8 {
                            8.0_f64
                        } else {
                            (3 + generation) as f64
                        })
                        .to_bits(),
                        "resolution_native_amount:{family}:{iteration}"
                    );
                    if ![2, 3].contains(&i) {
                        let V::Text(side) = &order.side else {
                            panic!("resolution_native_side_kind")
                        };
                        assert_eq!(
                            *side,
                            u(if generation.is_multiple_of(2) {
                                "buy"
                            } else {
                                "sell"
                            }),
                            "resolution_native_side:{family}:{iteration}"
                        );
                    }
                    order_assert(&order, &case["output"], &host.input);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        } else {
            handle_order_update(&*host, &host.input)
        };
        if result.is_ok() {
            successes += 1;
        } else {
            failures += 1;
        }
        assert_eq!(
            result.is_err(),
            match family {
                "exchange" => [5, 6, 10, 11].contains(&i),
                "private" => [5, 6, 11].contains(&i),
                "update" => i == 11,
                _ => unreachable!(),
            },
            "resolution_native_outcome:{family}:{iteration}"
        );
        error(result, &case["error"], &host);
        if let Some(order) = host.stored.borrow().as_ref() {
            stored += 1;
            let order = order.borrow();
            let V::Text(id) = &order.id else {
                panic!("resolution_native_update_id_kind")
            };
            assert_eq!(
                *id,
                if i == 8 {
                    u("changed").into_iter().chain([0xd800]).collect()
                } else {
                    vec![120_u16; PROFILE_LENGTHS[generation]]
                },
                "resolution_native_update_id:{iteration}"
            );
            order_assert(&order, &case["output"], &host.input);
        } else if name == "handleOrderUpdate" {
            assert!(case["output"].is_null());
        }
        if let Some(fill) = host.fill.borrow().as_ref() {
            fills += 1;
            assert_eq!(
                fill.amount.to_bits(),
                (if i == 8 {
                    42.0_f64
                } else {
                    (1 + generation % 3) as f64
                })
                .to_bits(),
                "resolution_native_fill_amount:{iteration}"
            );
            value(&fill.id, &case["fill"]["id"], "resolution_native_fill_id");
            value(
                &fill.side,
                &case["fill"]["side"],
                "resolution_native_fill_side",
            );
            value(
                &fill.symbol,
                &case["fill"]["symbol"],
                "resolution_native_fill_symbol",
            );
            bits(
                fill.price,
                &case["fill"]["price"],
                "resolution_native_fill_price",
            );
            bits(
                fill.amount,
                &case["fill"]["amount"],
                "resolution_native_fill_amount_source",
            );
        } else {
            assert!(case["fill"].is_null());
        }
        let expected = expected_tags(venue, name, i);
        let events = host.events.borrow();
        assert_eq!(
            events.len(),
            expected.len(),
            "resolution_native_event_floor:{family}:{iteration}"
        );
        assert_eq!(
            case["trace"].as_array().unwrap().len(),
            expected.len(),
            "resolution_source_event_floor"
        );
        for (index, (event, tag)) in events.iter().zip(expected).enumerate() {
            assert_eq!(
                event.tag, tag,
                "resolution_native_event_order:{family}:{iteration}"
            );
            event_assert(event, &case["trace"][index]);
        }
    }
    assert_eq!(
        (successes, failures, stored, fills, boundary65),
        match family {
            "exchange" => (384, 128, 0, 0, 16),
            "private" => (416, 96, 0, 0, 16),
            "update" => (480, 32, 480, 96, 16),
            _ => unreachable!(),
        },
        "resolution_native_independent_totals:{family}"
    );
    eprintln!("resolution {family}:512 instances;32 text lengths,max1025;65 at192..207;stop511");
}
#[test]
fn resolution_exchange_map_native() {
    resolution_run("exchange", "exchange", "mapOrder")
}
#[test]
fn resolution_private_map_native() {
    resolution_run("private", "private", "mapOrder")
}
#[test]
fn resolution_update_native() {
    resolution_run("update", "private", "handleOrderUpdate")
}
