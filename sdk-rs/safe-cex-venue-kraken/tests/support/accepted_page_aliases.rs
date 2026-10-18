//! The concrete income pager aliases through the constructed production Scope.
use super::{
    Rt,
    accepted_read_receivers::terminal::{Harness, harness, options},
};
use crate::{
    private_accounts::{self, IncomeHost, PageCallback, PageFuture},
    rest_account::Handle,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_types::ExchangeName;
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn settle<T>(h: &Harness, mut future: Pin<Box<dyn Future<Output = T> + '_>>) -> T {
    for _ in 0..100 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        h.runtime.advance(50.0);
    }
    panic!("native page continuation did not settle");
}

fn params() -> Handle<Rt> {
    Rc::new(Js::object(vec![
        ("startTime", Js::Number(1_700_000_000_000.0)),
        ("limit", Js::Number(37.0)),
    ]))
}

#[test]
fn page_future_keeps_held_native_http_completion_and_exact_wire_params() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok(
            "GET",
            "/history/v3/account-log",
            serde_json::json!({"logs": [{
                "booking_uid": "page-row", "date": "2023-11-14T22:13:20.001Z", "asset": "usd",
                "info": "realized", "realized_pnl": "3.125", "realized_funding": "-2.75"
            }]}),
        );
        http.hold_responses.set(true);
    });
    let rt = &h.exchange.rt;
    let mut page: PageFuture<'_, Rt> =
        Box::pin(IncomeHost::account_log_page(rt, params()).unwrap());
    let independently_typed: &mut Pin<Box<dyn Future<Output = Result<Rc<Js>, Thrown>> + '_>> =
        &mut page;
    assert!(
        independently_typed
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    h.runtime.pump();
    assert!(
        page.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    let requests = h.http.calls("/history/v3/account-log");
    assert_eq!(requests.len(), 1, "one real signed page request");
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].url,
        "/history/v3/account-log?count=37&since=1700000000000"
    );
    assert!(
        requests[0].params.is_null(),
        "terminal adapter has serialized params into the URL"
    );
    h.http.hold_responses.set(false);
    for waker in h.http.response_wakers.borrow_mut().drain(..) {
        waker.wake();
    }
    let value = settle(&h, page).unwrap();
    let rows = value.array_values().expect("native page array");
    assert_eq!(
        rows.len(),
        2,
        "both independent ledger amounts survive completion"
    );
    assert_eq!(
        rows[0].get("income").unwrap().as_f64().unwrap().to_bits(),
        3.125f64.to_bits()
    );
    assert_eq!(
        rows[1].get("income").unwrap().as_f64().unwrap().to_bits(),
        (-2.75f64).to_bits()
    );
    assert_eq!(
        String::from_utf16(rows[1].get("incomeType").unwrap().as_str().unwrap()).unwrap(),
        "realized_funding"
    );
    assert_eq!(h.http.calls("/history/v3/account-log").len(), 1);
}

#[test]
fn page_callback_preserves_native_synchronous_error_and_future_rejection() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let callback: PageCallback<'_, Rt> =
        Box::new(move |params| Ok(Box::pin(IncomeHost::account_log_page(rt, params)?)));
    let original = Js::object(vec![
        ("units", Js::utf16(&[0xd800])),
        ("negativeZero", Js::Number(-0.0)),
    ]);
    let thrown = original.clone();
    rt.this
        .define_accessor(
            "_fetchAccountLogPage",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        )
        .unwrap();
    let error = match callback(params()) {
        Err(error) => error,
        Ok(_) => panic!("getter rejection must stay synchronous"),
    };
    assert!(error.value.strict_equals(&original));
    assert_eq!(
        error.value.get("units").unwrap().as_str().unwrap().as_ref(),
        &[0xd800]
    );
    assert_eq!(
        error
            .value
            .get("negativeZero")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );

    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok(
            "GET",
            "/history/v3/account-log",
            serde_json::json!({"status":"unauthorized", "reason":"page refused"}),
        );
    });
    let rt = &h.exchange.rt;
    let callback: PageCallback<'_, Rt> =
        Box::new(move |params| Ok(Box::pin(IncomeHost::account_log_page(rt, params)?)));
    let error = settle(&h, callback(params()).unwrap()).unwrap_err();
    assert!(
        error.message().contains("page refused"),
        "page refusal must not become an empty success"
    );
}

#[test]
fn income_range_uses_page_callback_with_source_window_and_page_limit() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok(
            "GET",
            "/history/v3/account-log",
            serde_json::json!({"logs": []}),
        );
    });
    let rt = &h.exchange.rt;
    let start = Rc::new(Js::Number(1_700_000_000_000.0));
    let end = Rc::new(Js::Number(1_700_000_001_000.0));
    let result = settle(
        &h,
        Box::pin(private_accounts::income_range(
            Rc::new(rt.clone()),
            start,
            end,
            Rc::new(Js::Undefined),
        )),
    )
    .unwrap();
    assert_eq!(result.array_values().unwrap().len(), 0);
    let requests = h.http.calls("/history/v3/account-log");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].url,
        "/history/v3/account-log?count=500&since=1700000000000"
    );
    assert!(
        requests[0].params.is_null(),
        "terminal adapter has serialized params into the URL"
    );
}
