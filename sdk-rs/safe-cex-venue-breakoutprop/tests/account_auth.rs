use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_venue_breakoutprop::account_auth::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    process::Command,
    sync::LazyLock,
    task::{Context, Poll, Waker},
};
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_auth_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
});
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(x) => x,
        Poll::Pending => panic!("fixture pending"),
    }
}
#[test]
fn actual_account_read() {
    let names = [
        "single",
        "multi-first",
        "none-valid",
        "no-accounts",
        "no-users",
        "missing-unused",
        "missing-later-user",
        "cached",
        "cached-empty",
        "no-subaccount",
        "empty-subaccount",
        "not-found",
        "session",
        "http",
        "logger",
        "object-msg",
        "empty-msg",
        "surrogate",
    ];
    assert_eq!(
        ORACLE["cases"].as_array().unwrap().len(),
        18,
        "ACCOUNT-SOURCE fixed cases"
    );
    let mut comparisons = 0;
    for (index, name) in names.iter().enumerate() {
        let c = &ORACLE["cases"][index];
        assert_eq!(c["name"], *name, "ACCOUNT-SOURCE identity");
        let users = c["inputAccounts"].as_array().unwrap();
        assert_eq!(
            users.len(),
            if *name == "no-users" {
                0
            } else if *name == "missing-later-user" {
                2
            } else {
                1
            },
            "ACCOUNT-SOURCE user count"
        );
        if *name != "no-users" {
            let candidates = users[0].as_array().unwrap();
            let expected = if *name == "no-accounts" {
                vec![]
            } else if [
                "multi-first",
                "no-subaccount",
                "empty-subaccount",
                "not-found",
            ]
            .contains(name)
            {
                vec!["disabled", "default:321", "other:321"]
            } else {
                vec!["disabled", "default:321"]
            };
            assert_eq!(
                candidates.len(),
                expected.len(),
                "ACCOUNT-SOURCE candidate count"
            );
            assert_eq!(
                candidates
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect::<Vec<_>>(),
                expected,
                "ACCOUNT-SOURCE ordered candidate identities"
            );
        }
        let input = js_from_str::<JsNumberInput>(c["raw"].as_str().unwrap());
        if *name == "surrogate" {
            assert_eq!(c["expected"]["ok"], "default:321");
            let retained: safe_cex_types::retained_json::RetainedJsonInput =
                input.as_ref().unwrap().clone().decode().unwrap();
            assert_eq!(
                retained.get("userDetails").unwrap().as_array().unwrap()[0]
                    .get("fullName")
                    .unwrap()
                    .as_text(),
                Some([0xd800].as_slice())
            );
        }
        let cache = RefCell::new(c["cache"].as_str().map(str::to_owned));
        let errors = RefCell::new(Vec::<Value>::new());
        let trace = RefCell::new(Vec::<Value>::new());
        let mut original = ExchangeError::new("original-rejection");
        original = original.with_retained_response((json!({"data":{"msg":if *name=="object-msg"{json!({"detail":"object"})}else if *name=="empty-msg"{json!("")}else{json!("venue-message")}}})).into());
        original.store_subscriber_ids = vec![17, 23];
        let mut logger_error = ExchangeError::new("logger-rejection");
        logger_error.store_subscriber_ids = vec![31, 41];
        let logger = |e| {
            errors.borrow_mut().push(e);
            if *name == "logger" {
                Err(logger_error.clone())
            } else {
                Ok(())
            }
        };
        let f = get_account_code(
            &cache,
            c["options"]["key"].as_str().unwrap().into(),
            c["options"]["subaccount"].as_str().map(str::to_owned),
            || {
                trace.borrow_mut().push(json!("session"));
                ready(if *name == "session" {
                    Err(original.clone())
                } else {
                    Ok("token".into())
                })
            },
            |r| {
                trace
                    .borrow_mut()
                    .push(json!({"method":r.method,"url":r.url,"params":r.params}));
                ready(
                    if ["http", "logger", "object-msg", "empty-msg"].contains(name) {
                        Err(original.clone())
                    } else {
                        Ok(input.unwrap())
                    },
                )
            },
            &logger,
        );
        assert_eq!(
            trace.borrow().len(),
            usize::from(*name != "cached"),
            "ACCOUNT-EAGER cached bypass"
        );
        let result = run(f);
        assert_eq!(
            json!(*trace.borrow()),
            c["trace"],
            "ACCOUNT-REQUEST ordered calls"
        );
        match result {
            Ok(value) => assert_eq!(
                value, c["expected"]["ok"],
                "ACCOUNT-NATIVE selected account"
            ),
            Err(e) => {
                assert_eq!(e.message, c["expected"]["error"], "ACCOUNT-ERROR message");
                assert_eq!(e.name, c["expected"]["name"], "ACCOUNT-ERROR name");
                if ["session", "http", "object-msg", "empty-msg", "logger"].contains(name) {
                    assert_eq!(
                        e,
                        if *name == "logger" {
                            logger_error
                        } else {
                            original
                        },
                        "ACCOUNT-ERROR original metadata"
                    );
                }
            }
        }
        assert_eq!(
            json!(*cache.borrow()),
            c["after"],
            "ACCOUNT-CACHE committed value"
        );
        assert_eq!(
            json!(*errors.borrow()),
            c["expected"]["errors"],
            "ACCOUNT-LOG selected message"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 18, "ACCOUNT-COMPARISONS fixed total");
}
#[test]
fn actual_validation_and_invalidation() {
    let names = [
        "success", "message", "both", "empty", "falsy", "object", "logger", "array",
    ];
    assert_eq!(
        ORACLE["validations"].as_array().unwrap().len(),
        8,
        "VALIDATE-SOURCE fixed total"
    );
    let mut comparisons = 0;
    for (i, name) in names.iter().enumerate() {
        let c = &ORACLE["validations"][i];
        assert_eq!(c["name"], *name, "VALIDATE-SOURCE identity");
        let mut original = ExchangeError::new(c["message"].as_str().unwrap());
        original = original.with_retained_response(c["response"].clone().into());
        let errors = RefCell::new(Vec::<Value>::new());
        let mut logger_error = ExchangeError::new("logger-rejection");
        logger_error.store_subscriber_ids = vec![47, 59];
        let logger = |e| {
            errors.borrow_mut().push(e);
            if *name == "logger" {
                Err(logger_error.clone())
            } else {
                Ok(())
            }
        };
        let result = run(validate_account(
            || {
                ready(if *name == "success" {
                    Ok("token".into())
                } else {
                    Err(original)
                })
            },
            &logger,
        ));
        match result {
            Ok(value) => assert_eq!(value, c["expected"]["ok"], "VALIDATE-NATIVE error object"),
            Err(e) => assert_eq!(e, logger_error, "VALIDATE-ERROR reporter metadata"),
        };
        assert_eq!(
            json!(*errors.borrow()),
            c["expected"]["errors"],
            "VALIDATE-LOG message differs from msg"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 8, "VALIDATE-COMPARISONS fixed total");
    assert_eq!(
        ORACLE["invalidations"].as_array().unwrap().len(),
        3,
        "INVALIDATE-SOURCE fixed total"
    );
    for (i, before) in [Some("token".to_string()), None, Some(String::new())]
        .into_iter()
        .enumerate()
    {
        let c = &ORACLE["invalidations"][i];
        assert_eq!(json!(before), c["before"]);
        let mut token = before;
        let mut expiry = 9123.0;
        invalidate_session(&mut token, &mut expiry);
        assert!(token.is_none(), "INVALIDATE-NATIVE token cleared");
        assert_eq!(
            expiry.to_bits(),
            0.0_f64.to_bits(),
            "INVALIDATE-NATIVE expiry positive zero"
        );
        assert_eq!(c["token"], Value::Null);
        assert_eq!(c["expires"], 0);
        assert_eq!(
            c["pendingRetained"], true,
            "INVALIDATE-SOURCE pending request retained"
        );
    }
}

#[test]
fn account_auth_owned_and_lifecycle_boundaries() {
    use std::cell::Cell;
    assert_eq!(
        ORACLE["owned"],
        json!({"posted":"LATE","calls":2}),
        "ACCOUNT-SOURCE mutable options and discarded continuation"
    );
    assert_eq!(
        ORACLE["validationCalls"], 2,
        "VALIDATE-SOURCE discarded continuation"
    );
    assert_eq!(
        ORACLE["alias"],
        json!({"error":{"value":"after"}}),
        "VALIDATE-SOURCE error alias"
    );
    let mut original = ExchangeError::new("alias");
    original = original.with_retained_response((json!({"data":{"msg":{"value":"before"}}})).into());
    let result = run(validate_account(|| ready(Err(original.clone())), &|_| {
        Ok(())
    }))
    .unwrap();
    let mut changed = original.response_json_view().unwrap();
    changed["data"]["msg"]["value"] = json!("after");
    original = original.with_retained_response(changed.into());
    assert_eq!(
        original.response_json_view().unwrap()["data"]["msg"]["value"],
        "after"
    );
    assert_eq!(
        result,
        json!({"error":{"value":"before"}}),
        "VALIDATE-OWNED error snapshot"
    );
    let cache = RefCell::new(None);
    let logger = |_| Ok(());
    let result = run(get_account_code(
        &cache,
        "ORIGINAL".into(),
        None,
        || ready(Ok("".into())),
        |r| {
            assert_eq!(
                r.params.unwrap()["username"],
                "ORIGINAL",
                "ACCOUNT-OWNED options snapshot"
            );
            ready(Ok(js_from_str::<JsNumberInput>(
                ORACLE["cases"][0]["raw"].as_str().unwrap(),
            )
            .unwrap()))
        },
        &logger,
    ))
    .unwrap();
    assert_eq!(result, "default:321");
    struct Pending<'a>(&'a Cell<usize>);
    impl Future for Pending<'_> {
        type Output = Result<String, ExchangeError>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for Pending<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Cell::new(0);
    for poll in [false, true] {
        let f = validate_account(|| Pending(&drops), &logger);
        let mut f = Box::pin(f);
        if poll {
            assert!(
                f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        drop(f);
        let cache = RefCell::new(None);
        let f = get_account_code(
            &cache,
            "".into(),
            None,
            || Pending(&drops),
            |_| ready(Err(ExchangeError::new("unreachable"))),
            &logger,
        );
        let mut f = Box::pin(f);
        if poll {
            assert!(
                f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        drop(f);
    }
    assert_eq!(drops.get(), 4, "ACCOUNT-AUTH-DROP pending child release");
}

#[test]
fn validation_logger_argument_snapshot() {
    assert_eq!(
        ORACLE["loggerMutation"],
        json!({"error":{"value":"logged"}}),
        "VALIDATE-SOURCE logger aliases return value"
    );
    let mut error = ExchangeError::new("alias");
    error = error.with_retained_response(
        (json!({"data":{"message":{"value":"before"},"msg":{"value":"before"}}})).into(),
    );
    let logger = |mut argument: Value| {
        argument["value"] = json!("logged");
        Ok(())
    };
    let result = run(validate_account(|| ready(Err(error)), &logger)).unwrap();
    assert_eq!(
        result,
        json!({"error":{"value":"before"}}),
        "VALIDATE-OWNED logger argument snapshot"
    );
}

#[test]
fn account_consumed_fields_match_source() {
    let cases = ORACLE["consumed"].as_array().unwrap();
    let names = [
        "minimal",
        "poisoned-unused",
        "ignored-later",
        "ignored-disabled",
        "arraylike-users",
        "null-user",
        "null-accounts",
        "scalar-accounts",
        "null-account",
        "nonstring-status",
        "missing-status",
        "bad-code-before-second-match",
    ];
    assert_eq!(cases.len(), names.len(), "ACCOUNT-CONSUMED source census");
    for (case, name) in cases.iter().zip(names) {
        assert_eq!(case["name"], name);
        let cache = RefCell::new(None);
        let errors = RefCell::new(Vec::<Value>::new());
        let logger = |value| {
            errors.borrow_mut().push(value);
            Ok(())
        };
        let input = js_from_str::<JsNumberInput>(case["raw"].as_str().unwrap()).unwrap();
        let result = run(get_account_code(
            &cache,
            "k".into(),
            Some("A".into()),
            || ready(Ok("token".into())),
            |_| ready(Ok(input)),
            &logger,
        ));
        match result {
            Ok(account) => assert_eq!(
                json!({"ok":account,"errors":*errors.borrow()}),
                case["expected"],
                "{name}"
            ),
            Err(error) => assert_eq!(
                json!({"error":error.message,"name":error.name,"errors":*errors.borrow()}),
                case["expected"],
                "{name}"
            ),
        }
        assert_eq!(json!(*cache.borrow()), case["after"], "{name}: cache");
    }
}
