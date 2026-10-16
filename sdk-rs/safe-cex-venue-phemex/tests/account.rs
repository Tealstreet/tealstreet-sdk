use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsCoercionError;
use safe_cex_venue_phemex::account::get_account;
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
    Coercion(JsCoercionError),
}
impl From<ExchangeError> for Failure {
    fn from(e: ExchangeError) -> Self {
        Self::Core(e)
    }
}
impl From<JsCoercionError> for Failure {
    fn from(e: JsCoercionError) -> Self {
        Self::Coercion(e)
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
fn account_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/account_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual account oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("account capture profile: {}", corpus["profile"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        30,
        "account case census"
    );
    let mut comparisons = 0;
    for case_index in 0..30 {
        let case = corpus["cases"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed cases case");
        let events = RefCell::new(Vec::<Value>::new());
        let read_failure = case.get("readFailure").cloned().map(Rc::new);
        let emit_failure = case.get("emitFailure").cloned().map(Rc::new);
        let result = ready(get_account::<Failure, _>(
            |url, params| {
                events
                    .borrow_mut()
                    .push(json!({"kind":"read","url":url,"config":{"params":params}}));
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
                    .push(json!({"kind":"emit","message":message}));
                emit_failure
                    .as_ref()
                    .map_or(Ok(()), |e| Err(Failure::Input(e.clone())))
            },
        ));
        let expected = &case["expected"];
        match result {
            Ok(account) => {
                assert_eq!(expected["ok"], true, "account outcome: {case}");
                assert_eq!(
                    account.user_id,
                    expected["userId"].as_str().unwrap(),
                    "native account userId: {case}"
                );
                assert!(
                    account.sub_id.is_none() && account.affiliate_id.is_none(),
                    "native account optional fields absent"
                );
                assert_eq!(
                    expected["keys"],
                    json!(["userId"]),
                    "source account field census"
                );
            }
            Err(error) => {
                assert_eq!(expected["ok"], false, "account outcome: {case}");
                let (name, message, read_identity, emit_identity) = match &error {
                    Failure::Input(e) => (
                        e["name"].as_str().unwrap(),
                        e["message"].as_str().unwrap(),
                        read_failure.as_ref().is_some_and(|x| Rc::ptr_eq(x, e)),
                        emit_failure.as_ref().is_some_and(|x| Rc::ptr_eq(x, e)),
                    ),
                    Failure::Core(e) => (e.name.as_str(), e.message.as_str(), false, false),
                    Failure::Coercion(e) => (e.name, e.message, false, false),
                };
                assert_eq!(
                    name,
                    expected["name"].as_str().unwrap(),
                    "account error name"
                );
                assert_eq!(
                    message,
                    expected["message"].as_str().unwrap(),
                    "account error message"
                );
                assert_eq!(
                    read_identity,
                    expected["readIdentity"].as_bool().unwrap(),
                    "account read error identity"
                );
                assert_eq!(
                    emit_identity,
                    expected["emitIdentity"].as_bool().unwrap(),
                    "account emitter error identity"
                );
            }
        }
        assert_eq!(
            *events.borrow(),
            *case["events"].as_array().unwrap(),
            "account request and emit order: {case}"
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 30,
        "PHEMEX-CARDINALITY account native comparisons"
    );
}
