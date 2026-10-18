//! Typed boundaries use the production Promise driver and native scheduler.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    eager::start_eager,
    js_runtime::value::Js,
    native::native_runtime,
    runtime::{ExchangeError, ExchangeErrorOrigin, ExchangeRuntime},
};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<F: Future<Output = ()>>(body: impl FnOnce(Rc<dyn ExchangeRuntime>) -> F) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async { body(native_runtime()).await });
}

#[test]
fn synchronous_prefix_and_continuation_survive_dropping_the_waiter() {
    run(|runtime| async move {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let seen = trace.clone();
        let pending = start_eager(&runtime, async move {
            seen.borrow_mut().push("prefix");
            tokio::task::yield_now().await;
            seen.borrow_mut().push("continuation");
            Ok(())
        });
        assert_eq!(*trace.borrow(), ["prefix"]);
        drop(pending);
        for _ in 0..8 {
            if trace.borrow().len() == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(*trace.borrow(), ["prefix", "continuation"]);
    });
}

#[test]
fn a_nonclone_nonserializable_result_moves_once_with_native_number_bits() {
    struct NativeOnly {
        identity: Box<u64>,
        nan: f64,
        negative_zero: f64,
    }
    run(|runtime| async move {
        let value = NativeOnly {
            identity: Box::new(173),
            nan: f64::from_bits(0x7ff8_0000_0000_0042),
            negative_zero: -0.0,
        };
        let address = value.identity.as_ref() as *const u64;
        let result = start_eager(&runtime, async move { Ok(value) })
            .await
            .unwrap();
        assert_eq!(result.identity.as_ref() as *const u64, address);
        assert_eq!(result.nan.to_bits(), 0x7ff8_0000_0000_0042);
        assert_eq!(result.negative_zero.to_bits(), (-0.0_f64).to_bits());
    });
}

#[test]
fn native_error_metadata_and_origin_identity_survive_rejection() {
    run(|runtime| async move {
        let mut error = ExchangeError::new("native failure");
        error.name = "RangeError".into();
        error.code = Some(Js::object(vec![("bits", Js::Number(-0.0))]));
        error.context = Some(serde_json::json!({"request":"one"}));
        error.store_subscriber_ids = vec![3, 7];
        error.origin = Some(ExchangeErrorOrigin::new(std::io::Error::other("original")));
        let expected = error.clone();
        let result = start_eager(&runtime, async move { Err::<(), _>(error) })
            .await
            .unwrap_err();
        assert_eq!(result, expected);
        assert!(
            result
                .code
                .as_ref()
                .unwrap()
                .strict_equals(expected.code.as_ref().unwrap())
        );
    });
}

#[test]
fn a_ready_result_keeps_exactly_one_promise_await_boundary() {
    run(|runtime| async move {
        let mut pending = start_eager(&runtime, async { Ok(41_u32) });
        let mut context = Context::from_waker(Waker::noop());
        assert!(pending.as_mut().poll(&mut context).is_pending());
        assert!(matches!(
            pending.as_mut().poll(&mut context),
            Poll::Ready(Ok(41))
        ));
    });
}
