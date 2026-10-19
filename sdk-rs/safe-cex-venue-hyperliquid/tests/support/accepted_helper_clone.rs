use super::start;
use crate::host::test_runtime::TestRuntime;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::{
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

#[derive(Default)]
struct Wakes(AtomicUsize);
impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn helper_observer_clone_shares_pending_completion_and_original_values() {
    for rejected in [false, true] {
        let runtime = Rc::new(TestRuntime::default());
        let mut realm = crate::host::test_runtime::host(runtime.clone());
        let cell = safe_cex_exchange::js_runtime::promise::Promise::pending(&realm.runtime());
        let promise = Js::Promise(cell.clone());
        let mut original = start(&realm, realm.await_value(promise));
        let mut cloned = original.clone();
        assert!(
            Rc::ptr_eq(&original.state, &cloned.state),
            "Clone detached the pending completion state"
        );
        let first = Arc::new(Wakes::default());
        let second = Arc::new(Wakes::default());
        let first_waker = Waker::from(first.clone());
        let second_waker = Waker::from(second.clone());
        assert!(
            Pin::new(&mut original)
                .poll(&mut Context::from_waker(&first_waker))
                .is_pending()
        );
        assert!(
            Pin::new(&mut cloned)
                .poll(&mut Context::from_waker(&second_waker))
                .is_pending()
        );
        let expected = realm.object_from(vec![("marker", Js::str("native result"))]);
        cell.settle(if rejected {
            Err(Thrown::new(expected.clone()))
        } else {
            Ok(expected.clone())
        });
        runtime.drain();
        assert_eq!(first.0.load(Ordering::SeqCst), 1);
        assert_eq!(second.0.load(Ordering::SeqCst), 1);
        for observer in [&mut original, &mut cloned] {
            let Poll::Ready(result) =
                Pin::new(observer).poll(&mut Context::from_waker(Waker::noop()))
            else {
                panic!("observer did not see the shared completion");
            };
            let value = match result {
                Ok(value) if !rejected => value,
                Err(value) if rejected => value.value,
                _ => panic!("observer changed fulfillment/rejection identity"),
            };
            assert!(value.strict_equals(&expected));
        }
        realm
            .set_str(&expected, "marker", Js::str("retained identity"))
            .unwrap();
        let mut late_clone = cloned.clone();
        assert!(Rc::ptr_eq(&cloned.state, &late_clone.state));
        let Poll::Ready(result) =
            Pin::new(&mut late_clone).poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("late Clone lost the completed state");
        };
        let value = match result {
            Ok(value) if !rejected => value,
            Err(value) if rejected => value.value,
            _ => panic!("late Clone changed fulfillment/rejection identity"),
        };
        assert!(value.strict_equals(&expected));
        let marker = realm.get_str(&value, "marker").unwrap();
        assert_eq!(
            marker.as_str().unwrap().as_ref(),
            "retained identity".encode_utf16().collect::<Vec<_>>()
        );
    }
}
