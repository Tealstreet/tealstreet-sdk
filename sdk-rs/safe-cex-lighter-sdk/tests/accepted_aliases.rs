//! Fixed native witnesses for the six reviewed public aliases; no generic-T claim.
use safe_cex_lighter_sdk::{
    api_reads::{Fields, ReadHost},
    client_runtime::NativeMethod,
    http_types::{CallbackReturn, ErrorInterceptor, RequestInterceptor, ResponseInterceptor},
    key_runtime::KeyFuture,
};
use serde_json::json;
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<T>(mut future: impl Future<Output = T> + Unpin) -> T {
    match std::pin::Pin::new(&mut future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixed ready-future witness unexpectedly suspended"),
    }
}

#[test]
fn fields_bool_decodes_actual_tuple_sequences_and_retains_utf16_units() {
    let fields: Fields<bool> = serde_json::from_value(json!([
        [[65], true],
        [[55357, 56832], false],
        [[55296], true]
    ]))
    .unwrap();
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0], (vec![65], true));
    assert_eq!(fields[1], (vec![55357, 56832], false));
    assert_eq!(
        fields[2],
        (vec![55296], true),
        "native lone surrogate survives as a unit"
    );
    let empty: Fields<bool> = serde_json::from_value(json!([])).unwrap();
    assert!(empty.is_empty());
    for invalid in [
        json!({}),
        json!(null),
        json!("text"),
        json!([["key", true]]),
        json!([[[-1], true]]),
        json!([[[65536], true]]),
        json!([[[1.5], true]]),
        json!([[[65], 1]]),
        json!([[[65], true, 9]]),
    ] {
        assert!(
            serde_json::from_value::<Fields<bool>>(invalid.clone()).is_err(),
            "refuse {invalid}"
        );
    }
}

#[test]
fn fixed_callback_and_future_aliases_execute_values_errors_and_promises() {
    let method: NativeMethod<dyn ReadHost<Value = bool, Error = String>> = Rc::new(|this, args| {
        if this {
            Ok(args[0])
        } else {
            Err("receiver refused".into())
        }
    });
    assert_eq!(method(true, vec![false]), Ok(false));
    assert_eq!(method(false, vec![true]), Err("receiver refused".into()));
    let request: RequestInterceptor<bool, String> = Rc::new(|value| {
        if value {
            Ok(CallbackReturn::Value(false))
        } else {
            Err("request refused".into())
        }
    });
    assert!(matches!(
        request(true).unwrap(),
        CallbackReturn::Value(false)
    ));
    assert!(matches!(request(false),Err(error) if error=="request refused"));
    let response: ResponseInterceptor<bool, String> =
        Rc::new(|value| Ok(CallbackReturn::Promise(Box::pin(async move { Ok(!value) }))));
    let CallbackReturn::Promise(future) = response(false).unwrap() else {
        panic!("expected promise")
    };
    assert_eq!(ready(future), Ok(true));
    let error: ErrorInterceptor<bool, String> = Rc::new(|value| {
        if value {
            Ok(false)
        } else {
            Err("error callback refused".into())
        }
    });
    assert_eq!(error(true), Ok(false));
    assert_eq!(error(false), Err("error callback refused".into()));
    let value: KeyFuture<bool, String> = Box::pin(async { Ok(true) });
    let error: KeyFuture<bool, String> = Box::pin(async { Err("future refused".into()) });
    assert_eq!(ready(value), Ok(true));
    assert_eq!(ready(error), Err("future refused".into()));
}
