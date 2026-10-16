use safe_cex_venue_phemex::validation::validate_account;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
fn number(value: f64) -> Value {
    json!({"kind":"number","bits":if value.is_nan(){"NaN".into()}else{format!("{:016x}",value.to_bits())}})
}
#[test]
fn validation_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/validation_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual validation oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("validation capture profile: {}", corpus["profile"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        62,
        "PHEMEX-VALIDATION source total"
    );
    let calls = RefCell::new((0, 0));
    let (mut balances, mut absent, mut rejected, mut fields, mut comparisons) = (0, 0, 0, 0, 0);
    for index in 0..62 {
        let case = corpus["cases"]
            .get(index)
            .expect("PHEMEX-VALIDATION fixed source case");
        let events = RefCell::new(Vec::<Value>::new());
        let emit_failure = case.get("emitFailure").cloned().map(Rc::new);
        let result = ready(validate_account(
            |url, params| {
                calls.borrow_mut().0 += 1;
                events
                    .borrow_mut()
                    .push(json!({"kind":"read","url":url,"config":{"params":params}}));
                let result = if let Some(error) = case.get("error") {
                    Err(serde_json::from_str::<Value>(error.as_str().unwrap()).unwrap())
                } else {
                    Ok(case["raw"]
                        .as_str()
                        .map(|raw| serde_json::from_str::<Value>(raw).unwrap()))
                };
                async move { result }
            },
            |message| {
                calls.borrow_mut().1 += 1;
                events
                    .borrow_mut()
                    .push(json!({"kind":"emit","value":capture(message)}));
                emit_failure.as_ref().map_or(Ok(()), |e| Err(e.clone()))
            },
        ));
        let expected = &case["expected"];
        match result {
            Ok(value) => {
                assert_eq!(expected["ok"], true, "PHEMEX-VALIDATION outcome: {case}");
                assert_eq!(
                    capture(Some(value.error.as_value())),
                    expected["error"],
                    "PHEMEX-VALIDATION native error: {case}"
                );
                assert_eq!(
                    value.balance.is_some(),
                    expected["balance"]["kind"] == "object",
                    "PHEMEX-VALIDATION native balance presence: {case}"
                );
                if let Some(balance) = value.balance {
                    balances += 1;
                    let entries = expected["balance"]["entries"].as_array().unwrap();
                    assert_eq!(entries.len(), 4, "PHEMEX-VALIDATION source balance fields");
                    let field = |key: &str| {
                        &entries
                            .iter()
                            .find(|row| row[0] == key)
                            .expect("PHEMEX-VALIDATION fixed source field")[1]
                    };
                    assert_eq!(
                        &number(balance.total),
                        field("total"),
                        "PHEMEX-VALIDATION native balance.total: {case}"
                    );
                    assert_eq!(
                        &number(balance.used),
                        field("used"),
                        "PHEMEX-VALIDATION native balance.used: {case}"
                    );
                    assert_eq!(
                        &number(balance.free),
                        field("free"),
                        "PHEMEX-VALIDATION native balance.free: {case}"
                    );
                    assert_eq!(
                        &number(balance.upnl),
                        field("upnl"),
                        "PHEMEX-VALIDATION native balance.upnl: {case}"
                    );
                    assert!(
                        balance.info.is_none()
                            && balance.currencies.is_none()
                            && balance.coins.is_none(),
                        "PHEMEX-VALIDATION absent balance metadata"
                    );
                    fields += 4;
                } else {
                    absent += 1;
                }
            }
            Err(error) => {
                rejected += 1;
                assert_eq!(expected["ok"], false, "PHEMEX-VALIDATION rejection outcome");
                assert!(
                    emit_failure.as_ref().is_some_and(|e| Rc::ptr_eq(e, &error)),
                    "PHEMEX-VALIDATION native emitter identity"
                );
                assert_eq!(
                    expected["emitIdentity"], true,
                    "PHEMEX-VALIDATION source emitter identity"
                );
            }
        }
        assert_eq!(
            *events.borrow(),
            *case["events"].as_array().unwrap(),
            "PHEMEX-VALIDATION request and emission order: {case}"
        );
        comparisons += 1;
    }
    assert_eq!(
        *calls.borrow(),
        (62, 24),
        "PHEMEX-VALIDATION callback execution census"
    );
    assert_eq!(
        (balances, absent, rejected, fields, comparisons),
        (23, 37, 2, 92, 62),
        "PHEMEX-VALIDATION native branch and comparison census"
    );
}
fn capture(value: Option<&Value>) -> Value {
    match value {
        None => json!({"kind":"undefined"}),
        Some(Value::Null) => json!({"kind":"null"}),
        Some(Value::Bool(v)) => json!({"kind":"boolean","value":v}),
        Some(Value::String(v)) => json!({"kind":"string","value":v}),
        Some(v @ Value::Number(_)) => {
            let n = safe_cex_num::numeric_ingress::js_value_as_f64(v).unwrap();
            json!({"kind":"number","bits":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
        }
        Some(Value::Array(items)) => {
            json!({"kind":"array","items":items.iter().map(|v|capture(Some(v))).collect::<Vec<_>>()})
        }
        Some(Value::Object(fields)) => {
            json!({"kind":"object","entries":fields.iter().map(|(k,v)|json!([k,capture(Some(v))])).collect::<Vec<_>>()})
        }
    }
}
