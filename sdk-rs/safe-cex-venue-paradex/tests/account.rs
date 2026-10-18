use safe_cex_types::PositionMode;
use safe_cex_venue_paradex::{
    account::{get_account, get_position_mode},
    types::AccountResponse,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

type Error = Rc<&'static str>;
struct Response {
    settled: Rc<Cell<bool>>,
    outcome: Option<Result<AccountResponse, Error>>,
}
impl Future for Response {
    type Output = Result<AccountResponse, Error>;
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.settled.get() {
            Poll::Ready(self.outcome.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
#[test]
fn actual_typescript_account_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "account_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        s => s,
    };
    assert_eq!(corpus["header"]["arch"], arch, "capture_cpu_architecture");
    assert!(
        corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "capture_node_major"
    );
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        19,
        "account_corpus_census"
    );
    println!("capture_header {}", corpus["header"]);
    let mut cx = Context::from_waker(Waker::noop());
    let mut mode = Box::pin(get_position_mode());
    let Poll::Ready(mode) = mode.as_mut().poll(&mut cx) else {
        panic!("position_mode_must_resolve")
    };
    let mode = match mode {
        PositionMode::OneWay => "oneway",
        PositionMode::Hedged => "hedged",
    };
    assert_eq!(mode, corpus["positionMode"], "position_mode_native");
    let mut compared = 0;
    for c in corpus["cases"].as_array().unwrap() {
        let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
        let settled = Rc::new(Cell::new(false));
        let original = Rc::new("original");
        let coercion = Rc::new("coercion");
        let logger = Rc::new("logger");
        let mode = c["mode"].as_str().unwrap();
        let original_for_get = original.clone();
        let mut future = Box::pin(get_account(
            |url| {
                trace.borrow_mut().push(json!(["get", url]));
                if mode == "sync-error" {
                    return Err(original_for_get.clone());
                }
                Ok(Response {
                    settled: settled.clone(),
                    outcome: Some(if mode == "success" {
                        Ok(serde_json::from_str(&c["data"].to_string()).unwrap())
                    } else {
                        Err(original_for_get.clone())
                    }),
                })
            },
            |error| {
                assert!(
                    Rc::ptr_eq(error, &original),
                    "account_formatter_original_identity"
                );
                trace.borrow_mut().push(json!(["stringify"]));
                if mode == "coercion-error" {
                    Err(coercion.clone())
                } else {
                    Ok(c["message"].as_str().unwrap().to_owned())
                }
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if mode == "logger-error" {
                    Err(logger.clone())
                } else {
                    Ok(())
                }
            },
        ));
        let first = future.as_mut().poll(&mut cx);
        assert_eq!(
            json!(*trace.borrow()),
            c["before"],
            "account_before_settlement {}",
            c["label"]
        );
        let result = match first {
            Poll::Ready(result) => {
                assert_eq!(mode, "sync-error", "account_await_boundary");
                result
            }
            Poll::Pending => {
                assert_ne!(mode, "sync-error", "account_sync_error_boundary");
                settled.set(true);
                let Poll::Ready(result) = future.as_mut().poll(&mut cx) else {
                    panic!("account_settlement")
                };
                result
            }
        };
        match result {
            Ok(account) => {
                assert_eq!(c["outcome"]["ok"], true);
                assert_eq!(
                    account.user_id, c["outcome"]["userId"],
                    "account_native_user_id {}",
                    c["label"]
                );
                let affiliate = match c["outcome"].get("affiliateId") {
                    Some(Value::Null) => Some(None),
                    Some(Value::String(value)) => Some(Some(value.clone())),
                    None => None,
                    other => panic!("account affiliate domain changed: {other:?}"),
                };
                assert_eq!(
                    account.affiliate_id, affiliate,
                    "account_native_affiliate_null"
                );
                assert_eq!(account.sub_id, None, "account_native_sub_absent");
                assert_eq!(
                    c["outcome"]["keys"],
                    json!(["userId", "affiliateId"]),
                    "account_output_shape"
                );
            }
            Err(error) => {
                assert_eq!(c["outcome"]["ok"], false);
                let expected = match c["outcome"]["identity"].as_str().unwrap() {
                    "original" => &original,
                    "coercion" => &coercion,
                    "logger" => &logger,
                    _ => panic!("unexpected oracle error identity"),
                };
                assert!(
                    Rc::ptr_eq(&error, expected),
                    "account_rethrow_identity {}",
                    c["label"]
                );
            }
        }
        assert_eq!(
            json!(*trace.borrow()),
            c["trace"],
            "account_effect_trace {}",
            c["label"]
        );
        compared += 1;
    }
    assert_eq!(compared, 19, "account_completed_comparisons");
    assert_eq!(corpus["surrogateCodeUnit"], 0xd800);
    typed_utf16::assert_projection(
        corpus["surrogateRaw"].as_str().unwrap(),
        &(serde_json::from_str::<AccountResponse>(corpus["surrogateRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    assert_eq!(corpus["uncheckedType"], "undefined");
    assert!(
        serde_json::from_str::<AccountResponse>(corpus["uncheckedRaw"].as_str().unwrap()).is_err(),
        "account_declared_ingress_refusal"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
