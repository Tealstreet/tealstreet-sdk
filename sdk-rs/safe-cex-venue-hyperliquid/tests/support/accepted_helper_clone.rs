use super::start;
use crate::realm::{Realm, Value, promise::settle, test_runtime::TestRuntime};
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
        let mut realm = Realm::new(runtime.clone());
        let (promise, cell) = realm.new_promise();
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
        let expected = realm.object_from(vec![("marker", Value::str("native result"))]);
        settle(
            &cell,
            if rejected {
                Err(expected.clone())
            } else {
                Ok(expected.clone())
            },
        );
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
                Err(value) if rejected => value,
                _ => panic!("observer changed fulfillment/rejection identity"),
            };
            assert!(value.strict_equal(&expected));
        }
        realm
            .set_str(&expected, "marker", Value::str("retained identity"))
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
            Err(value) if rejected => value,
            _ => panic!("late Clone changed fulfillment/rejection identity"),
        };
        assert!(value.strict_equal(&expected));
        let marker = realm.get_str(&value, "marker").unwrap();
        assert_eq!(
            marker.as_units().unwrap(),
            "retained identity".encode_utf16().collect::<Vec<_>>()
        );
    }
}
