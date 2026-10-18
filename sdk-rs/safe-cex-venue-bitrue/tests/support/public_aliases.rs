//! Actual source consumers of the reviewed executable/cache aliases.
use super::{Fixture, bits};
use crate::{collection_reads::ReadHost, public_bodies::PublicBodyHost, trading_rest::TradingHost};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, Thrown},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

type Host = crate::binding::Rt;

#[test]
fn public_and_collection_callbacks_retain_original_native_values_and_errors() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    let marker = Js::object(vec![
        ("ownUndefined", Js::Undefined),
        ("negativeZero", Js::Number(-0.0)),
    ]);
    let error = Js::object(vec![("source", Js::str("original rejection"))]);
    let original = marker.clone();
    let original_error = error.clone();
    let callback: crate::public_bodies::NativeCallback<Js, Thrown> = Rc::new(move |args| {
        assert!(args[0].same_reference(&original));
        assert_eq!(bits(args[1].clone()), (-0.0f64).to_bits());
        if args[2].truthy() {
            Err(Thrown::new(original_error.clone()))
        } else {
            Ok(args[0].clone())
        }
    });
    let function = PublicBodyHost::function(h, callback);
    let returned = function
        .call(
            Js::Undefined,
            vec![marker.clone(), Js::Number(-0.0), Js::Bool(false)],
        )
        .unwrap();
    assert!(
        returned.same_reference(&marker),
        "NativeCallback retains original referent"
    );
    assert!(returned.has_own("ownUndefined"));
    assert_eq!(
        bits(returned.get("negativeZero").unwrap()),
        (-0.0f64).to_bits()
    );
    assert!(
        function
            .call(
                Js::Undefined,
                vec![marker.clone(), Js::Number(-0.0), Js::Bool(true)]
            )
            .unwrap_err()
            .value
            .same_reference(&error)
    );
    let original = marker.clone();
    let original_error = error.clone();
    let callback: crate::collection_reads::Callback<Host> = Rc::new(move |args| {
        assert!(args[0].same_reference(&original));
        assert_eq!(bits(args[1].clone()), (-0.0f64).to_bits());
        if args[2].truthy() {
            Err(Thrown::new(original_error.clone()))
        } else {
            Ok(args[0].clone())
        }
    });
    let function = ReadHost::function(h, callback);
    assert!(
        function
            .call(
                Js::Undefined,
                vec![marker.clone(), Js::Number(-0.0), Js::Bool(false)]
            )
            .unwrap()
            .same_reference(&marker),
        "Callback<production Rt> retains original referent"
    );
    assert!(
        function
            .call(
                Js::Undefined,
                vec![marker, Js::Number(-0.0), Js::Bool(true)]
            )
            .unwrap_err()
            .value
            .same_reference(&error)
    );
}

#[test]
fn async_callback_and_body_keep_pending_resolution_and_rejection_identity() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    let ready = Rc::new(Cell::new(false));
    let calls = Rc::new(Cell::new(0));
    let wake = Rc::new(RefCell::new(None::<Waker>));
    let marker = Js::object(vec![("ownUndefined", Js::Undefined)]);
    let error = Js::object(vec![("source", Js::str("async rejection"))]);
    let gate = ready.clone();
    let invoked = calls.clone();
    let rejected = error.clone();
    let pending_wake = wake.clone();
    let callback: crate::trading_rest::AsyncCallback<Host> = Rc::new(move |args| {
        invoked.set(invoked.get() + 1);
        assert_eq!(bits(args[1].clone()), (-0.0f64).to_bits());
        let value = args[0].clone();
        let reject = args[2].truthy();
        let ready = gate.clone();
        let error = rejected.clone();
        let wake = pending_wake.clone();
        let future: crate::trading_rest::Body<Host> = Box::pin(std::future::poll_fn(move |cx| {
            if !ready.get() {
                *wake.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            } else if reject {
                Poll::Ready(Err(Thrown::new(error.clone())))
            } else {
                Poll::Ready(Ok(value.clone()))
            }
        }));
        future
    });
    let function = TradingHost::async_function(h, callback);
    let promise = function
        .call(
            Js::Undefined,
            vec![marker.clone(), Js::Number(-0.0), Js::Bool(false)],
        )
        .unwrap();
    assert_eq!(
        calls.get(),
        1,
        "actual async factory invokes original callback exactly once"
    );
    let runtime = h.exchange_runtime();
    let future = await_js(&runtime, promise);
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    assert!(
        future.as_mut().poll(&mut context).is_pending(),
        "Body<production Rt> really remains pending"
    );
    ready.set(true);
    wake.borrow_mut()
        .take()
        .expect("pending Body registered its waker")
        .wake();
    let value = (0..32)
        .find_map(|_| {
            f.runtime.pump();
            match future.as_mut().poll(&mut context) {
                Poll::Pending => None,
                Poll::Ready(result) => Some(result),
            }
        })
        .expect("released real Promise must settle")
        .expect("released real Promise must resolve");
    assert!(
        value.same_reference(&marker),
        "async callback/Body retains resolved object identity"
    );
    assert!(value.has_own("ownUndefined"));
    let promise = function
        .call(
            Js::Undefined,
            vec![marker, Js::Number(-0.0), Js::Bool(true)],
        )
        .unwrap();
    let future = await_js(&runtime, promise);
    let mut future = pin!(future);
    let value = (0..32)
        .find_map(|_| {
            f.runtime.pump();
            match future.as_mut().poll(&mut context) {
                Poll::Pending => None,
                Poll::Ready(result) => Some(result),
            }
        })
        .expect("actual Promise must settle")
        .expect_err("actual Promise must retain body rejection");
    assert!(value.value.same_reference(&error));
    assert_eq!(calls.get(), 2);
}

#[test]
fn metadata_handle_alias_retains_real_cache_aliases_and_all_fields() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    let runtime = h.exchange_runtime();
    let promise = h
        .this
        .call_method(
            "fetchPositionMetadata",
            vec![
                Js::array(vec![Js::str("E-BTC-USDT"), Js::str("E-BTC-USDT")]),
                Js::array(vec![Js::object(vec![
                    ("symbol", Js::str("E-BTC-USDT")),
                    ("side", Js::str("short")),
                    ("leverageLevel", Js::Number(7.25)),
                    ("positionType", Js::Number(2.0)),
                ])]),
            ],
        )
        .unwrap();
    let future = await_js(&runtime, promise);
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    let returned = (0..32)
        .find_map(|_| {
            f.runtime.pump();
            match future.as_mut().poll(&mut context) {
                Poll::Pending => None,
                Poll::Ready(value) => Some(value),
            }
        })
        .expect("actual metadata member settles")
        .unwrap()
        .array_values().expect("fixture array property reads")
        .unwrap();
    assert_eq!(returned.len(), 2);
    assert!(
        returned[0].same_reference(&returned[1]),
        "actual public metadata Map retains duplicate aliases"
    );
    let cache = &h.shared.extra.metadata;
    assert_eq!(
        cache.borrow().len(),
        1,
        "actual constructor's native metadata cache is populated by its maintained member"
    );
    let symbols = ["E-BTC-USDT".to_owned(), "E-BTC-USDT".to_owned()];
    let future=crate::position_metadata::fetch_position_metadata(cache,&symbols,None,||->Result<std::future::Ready<Result<Option<serde_json::Value>,safe_cex_exchange::runtime::ExchangeError>>,safe_cex_exchange::runtime::ExchangeError>{panic!("supplied native snapshots must not fetch")},||false);
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    let Poll::Ready(Ok(handles)) = future.as_mut().poll(&mut cx) else {
        panic!("supplied snapshot should resolve without IO")
    };
    let handle: crate::position_metadata::PositionMetadataHandle = handles[0].clone();
    assert_eq!(handles.len(), 2);
    assert!(Rc::ptr_eq(&handle, &handles[1]));
    assert!(
        Rc::ptr_eq(&handle, &cache.borrow()[0].1),
        "PositionMetadataHandle keeps actual source cache referent"
    );
    {
        let row = handle.borrow();
        assert_eq!(row.symbol, "E-BTC-USDT");
        assert_eq!(row.side, safe_cex_types::PositionSide::Short);
        assert_eq!(row.leverage.to_bits(), 7.25f64.to_bits());
        assert_eq!(row.position_mode, safe_cex_types::PositionMode::OneWay);
        assert_eq!(row.margin_type, safe_cex_types::MarginType::Isolated);
    }
    handle.borrow_mut().leverage = 13.5;
    assert_eq!(handles[1].borrow().leverage.to_bits(), 13.5f64.to_bits());
    assert_eq!(
        cache.borrow()[0].1.borrow().leverage.to_bits(),
        13.5f64.to_bits()
    );
}
