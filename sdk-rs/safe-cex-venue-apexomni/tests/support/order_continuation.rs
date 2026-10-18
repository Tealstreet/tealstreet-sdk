use super::*;

fn source_field<'a>(order: &'a Value, key: &str) -> &'a Value {
    &order["values"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pair| pair[0] == key)
        .unwrap()[1]
}

#[test]
fn order_coercions_and_clocks_precede_typed_admission() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-continuation.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-ORDER source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    let expectations = [
        ("valid", 3),
        ("type-function", 1),
        ("type-prototype", 1),
        ("type-missing", 1),
        ("status-function", 1),
        ("status-prototype", 1),
        ("type-before-price", 0),
        ("type-clock", 1),
        ("tp-before-sl-clock", 1),
        ("missing-market", 0),
        ("missing-market-bad-type", 0),
        ("numeric-side", 0),
        ("null-side", 3),
        ("numeric-id", 3),
        ("numeric-reduce", 3),
        ("array-type", 3),
        ("array-status", 3),
        ("bad-type-key", 0),
        ("bad-tif-key", 0),
        ("bad-status-key", 0),
        ("bad-base-trigger-key", 1),
        ("bad-tp-trigger-key", 3),
        ("bad-id-interpolation", 1),
        ("optional-tif-function", 1),
        ("optional-tif-prototype", 1),
        ("optional-trigger-function", 1),
        ("optional-trigger-prototype", 1),
        ("reduce-before-price", 0),
    ];
    assert_eq!(rows.len(), 28, "APEX-ORDER independent source floor");
    let mut refused = 0;
    let mut errors = 0;
    let mut positive = 0;
    for (row, (label, clocks)) in rows.iter().zip(expectations) {
        assert_eq!(row["label"], label, "APEX-ORDER authored order");
        let marker = format!("APEX-ORDER {label}");
        let host = Rc::new(Host::default());
        let venue = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
                fixture(&host),
                host.clone(),
                Default::default(),
            ),
            host.clone(),
        )
        .unwrap();
        host.operations.borrow_mut().clear();
        host.now_calls.set(0);
        let actual = venue.map_order(&native::input(&row["input"]));
        let refusal = match label {
            "type-function" | "type-clock" => {
                Some(("type", "function", "unknown venue order type"))
            }
            "type-prototype" => Some(("type", "object", "unknown venue order type")),
            "status-function" => Some(("status", "function", "unknown venue order status")),
            "status-prototype" => Some(("status", "object", "unknown venue order status")),
            "numeric-id" => Some(("id", "number", "non-string field")),
            _ => None,
        };
        if label == "numeric-reduce" {
            assert!(
                actual
                    .expect("per-row refusal is not a batch error")
                    .is_empty()
            );
            assert_eq!(row["result"]["ok"], true);
            let source = &row["result"]["value"]["values"];
            assert_eq!(source.as_array().unwrap().len(), 3);
            native::check(
                &n(17.),
                source_field(&source[0], "reduceOnly"),
                &format!("{marker} source keeps numeric field"),
            );
            assert_eq!(
                host.operations.borrow_mut().pop(),
                Some(J::Array(vec![
                    s("console"),
                    s("warn"),
                    s(
                        "[APEX OMNI] Refused order row: Order.reduceOnly must be boolean, got Number(17.0)"
                    ),
                    J::Undefined,
                ]))
            );
        } else if let Some((field, kind, reason)) = refusal {
            let error = actual.expect_err(&format!("{marker} native output refusal"));
            assert_eq!(error.name, "Error", "{marker} native refusal kind");
            assert!(
                error.message.ends_with(reason),
                "{marker} native refusal reason: {}",
                error.message
            );
            assert_eq!(
                row["result"]["ok"], true,
                "{marker} source returns unsupported record"
            );
            let source = &row["result"]["value"]["values"];
            assert_eq!(
                source.as_array().unwrap().len(),
                if clocks == 3 { 3 } else { 1 },
                "{marker} source result floor"
            );
            let value = source_field(&source[0], field);
            assert_eq!(value["kind"], kind, "{marker} source returned field kind");
            if kind == "number" {
                native::check(&n(17.), value, &format!("{marker} source raw field"));
            }
            if kind == "object" {
                assert!(
                    value["values"].as_array().unwrap().is_empty(),
                    "{marker} source prototype has no own entries"
                );
            }
            refused += 1;
        } else if row["result"]["ok"] == false {
            let error = actual.expect_err(&format!("{marker} native expected source failure"));
            assert_eq!(
                error.name,
                row["result"]["name"].as_str().unwrap(),
                "{marker} native error kind"
            );
            assert_eq!(
                error.message,
                row["result"]["message"].as_str().unwrap(),
                "{marker} native error precedence"
            );
            errors += 1;
        } else {
            let orders = actual
                .unwrap_or_else(|error| panic!("{marker} native unexpected failure: {error:?}"));
            let mut expected = row["result"]["value"].clone();
            if label.starts_with("optional-") {
                assert_eq!(orders.len(), 1, "{marker} native optional result floor");
                let field = if label.starts_with("optional-tif") {
                    assert_eq!(
                        orders[0].time_in_force, None,
                        "{marker} native omitted optional enum"
                    );
                    "timeInForce"
                } else {
                    assert_eq!(
                        orders[0].sl_trigger_type, None,
                        "{marker} native omitted optional enum"
                    );
                    "slTriggerType"
                };
                let source = source_field(&expected["values"][0], field);
                assert_eq!(
                    source["kind"],
                    if label.ends_with("function") {
                        "function"
                    } else {
                        "object"
                    },
                    "{marker} source inherited optional field"
                );
                if label.ends_with("prototype") {
                    assert!(
                        source["values"].as_array().unwrap().is_empty(),
                        "{marker} source prototype own-entry floor"
                    );
                }
                expected["values"][0]["values"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|pair| pair[0] != field);
            }
            native::check_fields(
                &J::Array(orders.iter().map(order).collect()),
                &expected,
                &format!("{marker} native orders"),
            );
            positive += 1;
        }
        assert_eq!(
            host.now_calls.get(),
            0,
            "{marker} virtual clock must not read the raw runtime"
        );
        assert_eq!(row["clocks"], clocks, "{marker} source clock count");
        native::check(
            &J::Array(host.operations.borrow().clone()),
            &row["operations"],
            &format!("{marker} native operations"),
        );
    }
    assert_eq!(
        refused, 6,
        "APEX-ORDER independent whole-result refusal floor"
    );
    assert_eq!(errors, 11, "APEX-ORDER independent matching-error floor");
    assert_eq!(positive, 10, "APEX-ORDER independent positive floor");
    // Every native result/refusal/error/clock assertion above runs before wire.
    let mut wire_count = 0;
    for (label, field) in [
        ("type-function", "type"),
        ("type-prototype", "type"),
        ("status-function", "status"),
        ("status-prototype", "status"),
        ("optional-tif-function", "timeInForce"),
        ("optional-tif-prototype", "timeInForce"),
        ("optional-trigger-function", "slTriggerType"),
        ("optional-trigger-prototype", "slTriggerType"),
    ] {
        let row = rows.iter().find(|row| row["label"] == label).unwrap();
        let wire: Value = serde_json::from_str(row["result"]["wire"].as_str().unwrap()).unwrap();
        let order = wire[0].as_object().unwrap();
        if label.ends_with("prototype") {
            assert_eq!(
                order.get(field),
                Some(&json!({})),
                "APEX-ORDER {label} source prototype survives wire"
            );
        } else {
            assert!(
                !order.contains_key(field),
                "APEX-ORDER {label} source function omitted from wire"
            );
        }
        wire_count += 1;
    }
    assert_eq!(wire_count, 8, "APEX-ORDER independent wire witness floor");
}
