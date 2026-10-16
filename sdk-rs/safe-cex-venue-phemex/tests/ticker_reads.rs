use safe_cex_exchange::runtime::ExchangeError;

use safe_cex_venue_phemex::ticker_reads::fetch_tickers_raw;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug)]
enum Failure {
    Input(Rc<Value>),
    Core(ExchangeError),
}
impl From<ExchangeError> for Failure {
    fn from(e: ExchangeError) -> Self {
        Self::Core(e)
    }
}
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
#[test]
fn ticker_read_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/ticker_reads_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual ticker oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("ticker capture profile: {}", corpus["profile"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        23,
        "ticker case census"
    );
    let mut comparisons = 0;
    let mut raw_lexeme_comparisons = 0;
    for case_index in 0..23 {
        let case = corpus["cases"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed cases case");
        let events = RefCell::new(Vec::<Value>::new());
        let read_failure = case.get("readFailure").cloned().map(Rc::new);
        let emit_failure = case.get("emitFailure").cloned().map(Rc::new);
        let result = ready(fetch_tickers_raw::<Failure, _>(
            |url| {
                events
                    .borrow_mut()
                    .push(json!({"kind":"read","args":[url]}));
                let response = if let Some(e) = &read_failure {
                    Err(Failure::Input(e.clone()))
                } else {
                    Ok(case["raw"]
                        .as_str()
                        .map(|raw| serde_json::from_str::<Value>(raw).unwrap()))
                };
                async move { response }
            },
            |message| {
                events
                    .borrow_mut()
                    .push(json!({"kind":"emit","value":capture(message)}));
                emit_failure
                    .as_ref()
                    .map_or(Ok(()), |e| Err(Failure::Input(e.clone())))
            },
        ));
        let expected = &case["expected"];
        match result {
            Ok(value) => {
                assert_eq!(expected["ok"], true, "ticker outcome: {case}");
                assert_eq!(
                    capture(Some(&value)),
                    expected["value"],
                    "native ticker result: {case}"
                );
                if case["raw"]
                    .as_str()
                    .is_some_and(|raw| raw.contains("\"error\":null"))
                {
                    let input: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
                    assert_eq!(value, input, "ticker raw lexemes retained");
                    raw_lexeme_comparisons += 1;
                    assert_eq!(
                        expected["sameInput"], true,
                        "source ticker reference retained"
                    );
                }
            }
            Err(error) => {
                assert_eq!(expected["ok"], false, "ticker outcome: {case}");
                let (name, message, read_identity, emit_identity) = match &error {
                    Failure::Input(e) => (
                        e["name"].as_str().unwrap(),
                        e["message"].as_str().unwrap(),
                        read_failure.as_ref().is_some_and(|x| Rc::ptr_eq(x, e)),
                        emit_failure.as_ref().is_some_and(|x| Rc::ptr_eq(x, e)),
                    ),
                    Failure::Core(e) => (e.name.as_str(), e.message.as_str(), false, false),
                };
                assert_eq!(
                    name,
                    expected["name"].as_str().unwrap(),
                    "ticker error name"
                );
                assert_eq!(
                    message,
                    expected["message"].as_str().unwrap(),
                    "ticker error message"
                );
                assert_eq!(
                    read_identity,
                    expected["readIdentity"].as_bool().unwrap(),
                    "ticker read error identity"
                );
                assert_eq!(
                    emit_identity,
                    expected["emitIdentity"].as_bool().unwrap(),
                    "ticker emitter error identity"
                );
            }
        }
        assert_eq!(
            *events.borrow(),
            *case["events"].as_array().unwrap(),
            "ticker request and emit order: {case}"
        );
        comparisons += 1;
    }
    assert_eq!(
        raw_lexeme_comparisons, 2,
        "PHEMEX-GATED ticker raw lexeme comparison count"
    );
    assert_eq!(
        comparisons, 23,
        "PHEMEX-CARDINALITY ticker_reads native comparisons"
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
