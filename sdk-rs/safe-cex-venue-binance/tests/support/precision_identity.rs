use serde_json::{Map, Value, json};

pub const CASES: usize = 8;

fn expected_input(index: usize) -> Value {
    let (amount, step, price_step, max) = match index % 4 {
        0 => ("3.6", "1", json!(0.1), "3.8"),
        1 => ("3.6", "0.1", json!(1), "10"),
        2 => ("5", "1", json!(0.1), "1.6"),
        3 => ("5", "0.1", json!(1), "1.6"),
        _ => unreachable!(),
    };
    json!({"type":"limit","side":if index < 4 {"buy"} else {"sell"},
        "amount":amount,"price":"12.31","step":step,"priceStep":price_step,
        "max":max,"market":true,"precisionIdentity":true})
}

pub fn assert_input(case: &Value, index: usize) {
    assert!(index < CASES, "PRECISION-IDENTITY extra source case");
    let mut actual = case.as_object().expect("source object").clone();
    for output in ["expected", "minted", "emitted"] {
        assert!(
            actual.shift_remove(output).is_some(),
            "PRECISION-IDENTITY missing source output {output}"
        );
    }
    assert_eq!(
        Value::Object(actual),
        expected_input(index),
        "PRECISION-IDENTITY source content case {index}"
    );
}

pub fn assert_steps(index: usize, price: f64, amount: f64) {
    let expected = expected_input(index);
    assert_eq!(
        price,
        expected["priceStep"].as_f64().unwrap(),
        "PRECISION-IDENTITY native price step case {index}"
    );
    assert_eq!(
        amount,
        expected["step"].as_str().unwrap().parse::<f64>().unwrap(),
        "PRECISION-IDENTITY native amount step case {index}"
    );
    assert_ne!(
        price, amount,
        "PRECISION-IDENTITY distinct axes case {index}"
    );
}

pub fn assert_payloads(payloads: &[Map<String, Value>], index: usize) {
    let (price, quantities): (&str, &[&str]) = match index % 4 {
        0 => ("12.3", &["2", "2"]),
        1 => ("12", &["3.6"]),
        2 => ("12.3", &["1", "1", "1", "1"]),
        3 => ("12", &["1.2", "1.2", "1.2", "1.2", "0.2"]),
        _ => unreachable!(),
    };
    assert_eq!(
        payloads.len(),
        quantities.len(),
        "PRECISION-IDENTITY native payload count case {index}"
    );
    for (row, quantity) in quantities.iter().enumerate() {
        let payload = payloads
            .get(row)
            .expect("PRECISION-IDENTITY fixed native row");
        assert_eq!(
            payload.get("price").and_then(Value::as_str),
            Some(price),
            "PRECISION-IDENTITY native main price case {index} row {row}"
        );
        assert_eq!(
            payload.get("quantity").and_then(Value::as_str),
            Some(*quantity),
            "PRECISION-IDENTITY native quantity case {index} row {row}"
        );
        assert_eq!(
            payload.get("side").and_then(Value::as_str),
            Some(if index < 4 { "BUY" } else { "SELL" }),
            "PRECISION-IDENTITY native side case {index} row {row}"
        );
    }
}
