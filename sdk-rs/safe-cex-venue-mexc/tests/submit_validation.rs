use safe_cex_venue_mexc::submit_validation::{
    MexcOrderSubmitError, assert_regular_order_batch_succeeded,
    assert_regular_order_submit_succeeded,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};

fn oracle() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/submit_validation_oracle.mjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["singles"].as_array().unwrap().len(),
            16,
            "independent single validation floor"
        );
        assert_eq!(
            value["batches"].as_array().unwrap().len(),
            20,
            "independent batch validation floor"
        );
        assert_eq!(value["singles"][15]["index"], 15);
        assert_eq!(value["batches"][19]["index"], 19, "last validation fixture");
        value
    })
}
fn bits(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().unwrap(), 16).unwrap()
}
fn check_error(
    error: &MexcOrderSubmitError,
    expected: &Value,
    input: Option<&Rc<RefCell<Value>>>,
    partial_count: usize,
    total_bits: u64,
) {
    assert_eq!(
        error.name,
        expected["name"].as_str().unwrap(),
        "MEXC native submit error name"
    );
    assert_eq!(
        error.message,
        expected["message"].as_str().unwrap(),
        "MEXC native submit error message"
    );
    if expected["responseIdentity"].as_bool().unwrap() {
        assert!(
            Rc::ptr_eq(error.response_data.as_ref().unwrap(), input.unwrap()),
            "MEXC native original response handle"
        );
        let response = error.response_data.as_ref().unwrap();
        response
            .borrow_mut()
            .as_object_mut()
            .unwrap()
            .insert("fromError".into(), Value::from(17));
        assert_eq!(
            input.unwrap().borrow()["fromError"],
            17,
            "MEXC native error to input alias"
        );
        input
            .unwrap()
            .borrow_mut()
            .as_object_mut()
            .unwrap()
            .insert("fromInput".into(), Value::from(23));
        assert_eq!(
            response.borrow()["fromInput"],
            23,
            "MEXC native input to error alias"
        );
    } else {
        assert!(error.response_data.is_none(), "MEXC native absent response");
    }
    assert_eq!(
        error.partial_order_ids.as_ref().map_or(0, Vec::len),
        partial_count,
        "MEXC native partial ID count"
    );
    assert_eq!(
        error.partial_order_count,
        (partial_count != 0).then_some(partial_count),
        "MEXC native partial count property"
    );
    if partial_count == 0 {
        assert!(
            error.partial_order_ids.is_none(),
            "MEXC native absent partial IDs"
        );
        assert!(
            error.total_order_count.is_none(),
            "MEXC native absent total count"
        );
        assert!(
            error.failed_order_index.is_none(),
            "MEXC native absent failure index"
        );
    } else {
        let ids = expected["partialIds"].as_array().unwrap();
        assert_eq!(
            ids.len(),
            partial_count,
            "independent source partial ID floor"
        );
        for (index, id) in error.partial_order_ids.as_ref().unwrap().iter().enumerate() {
            assert_eq!(
                id,
                ids[index].as_str().unwrap(),
                "MEXC native ordered partial ID"
            );
        }
        let actual = error.total_order_count.unwrap();
        let expected_number = f64::from_bits(total_bits);
        if expected_number.is_nan() {
            assert!(actual.is_nan(), "MEXC native total count NaN");
        } else {
            assert_eq!(actual.to_bits(), total_bits, "MEXC native total count bits");
        }
        assert_eq!(
            error.failed_order_index.unwrap() as u64,
            expected["failedIndex"].as_u64().unwrap(),
            "MEXC native first failure index"
        );
        assert_eq!(
            expected["partialCount"].as_u64().unwrap() as usize,
            partial_count,
            "independent source partial count property"
        );
    }
}

#[test]
fn mexc_single_submit_validation_matches_source() {
    for (index, case) in oracle()["singles"].as_array().unwrap().iter().enumerate() {
        assert_eq!(case["index"].as_u64().unwrap() as usize, index);
        let calls = Cell::new(0_u64);
        let now = || {
            calls.set(calls.get() + 1);
            1000.0
        };
        let mut cooldown = 0.0;
        let input = case
            .get("input")
            .map(|value| Rc::new(RefCell::new(value.clone())));
        let result = assert_regular_order_submit_succeeded(input.as_ref(), &mut cooldown, &now);
        assert_eq!(
            result.is_ok(),
            case["outcome"]["success"].as_bool().unwrap(),
            "MEXC native single submit outcome: {index}"
        );
        let throttle = [8, 9, 11].contains(&index);
        assert_eq!(
            calls.get(),
            u64::from(throttle),
            "MEXC native single submit clock calls"
        );
        assert_eq!(
            cooldown.to_bits(),
            if throttle {
                2500.0_f64.to_bits()
            } else {
                0.0_f64.to_bits()
            },
            "MEXC native single submit cooldown"
        );
        assert_eq!(
            cooldown.to_bits(),
            bits(&case["cooldown"]),
            "source cooldown witness"
        );
        if let Err(error) = result {
            check_error(&error, &case["outcome"], input.as_ref(), 0, 0);
        }
    }
}

#[test]
fn mexc_batch_submit_validation_matches_source() {
    const PARTIALS: [usize; 20] = [0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0, 2, 0, 1, 0, 1];
    for (index, case) in oracle()["batches"].as_array().unwrap().iter().enumerate() {
        assert_eq!(case["index"].as_u64().unwrap() as usize, index);
        assert_eq!(
            case["expectedPartials"].as_u64().unwrap() as usize,
            PARTIALS[index],
            "independent batch expected cardinality"
        );
        let total = bits(&case["totalBits"]);
        let input = case
            .get("input")
            .map(|value| Rc::new(RefCell::new(value.clone())));
        let result = assert_regular_order_batch_succeeded(input.as_ref(), f64::from_bits(total));
        assert_eq!(
            result.is_ok(),
            case["outcome"]["success"].as_bool().unwrap(),
            "MEXC native batch submit outcome: {index}"
        );
        match result {
            Ok(()) => assert_eq!(PARTIALS[index], 0),
            Err(error) => check_error(
                &error,
                &case["outcome"],
                input.as_ref(),
                PARTIALS[index],
                total,
            ),
        }
    }
}
