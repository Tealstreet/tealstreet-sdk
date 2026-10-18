use super::{Realm, facade_rest, test_runtime::TestRuntime};
use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown},
    runtime::{ExchangeError, ExchangeErrorOrigin},
};
use std::{cell::Cell, fmt, rc::Rc};

#[derive(Debug)]
struct OriginalCause {
    value: Js,
    displays: Rc<Cell<usize>>,
}
impl fmt::Display for OriginalCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.displays.set(self.displays.get() + 1);
        f.write_str("opaque cause must not be formatted")
    }
}
impl std::error::Error for OriginalCause {}

#[test]
fn legacy_native_error_keeps_original_origin_without_reading_the_cause() {
    let mut realm = Realm::new(Rc::new(TestRuntime::default()));
    let value = Js::object(vec![]);
    value.define_key(&[0xd800], Js::Number(-0.0)).unwrap();
    let reads = Rc::new(Cell::new(0));
    let getter_reads = reads.clone();
    value
        .define_accessor(
            "message",
            Some(Rc::new(move |_| {
                getter_reads.set(getter_reads.get() + 1);
                Err(Thrown::new(Js::utf16(&[0xdc00])))
            })),
            None,
        )
        .unwrap();
    let displays = Rc::new(Cell::new(0));
    let origin = ExchangeErrorOrigin::new(OriginalCause {
        value: value.clone(),
        displays: displays.clone(),
    });
    let mut failure = ExchangeError::new("Store property access failed");
    failure.origin = Some(origin.clone());
    let thrown = facade_rest::thrown_from(&mut realm, failure);
    let recovered = realm.exchange_error(&thrown);
    assert_eq!(
        recovered.origin.as_ref(),
        Some(&origin),
        "HL_STORE_ORIGIN preserve the original opaque failure across the actual Realm boundary"
    );
    let cause = recovered
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<OriginalCause>()
        .unwrap();
    assert!(cause.value.same_reference(&value));
    assert_eq!(cause.value.get_key(&[0xd800]).unwrap().as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    assert_eq!(reads.get(), 0);
    assert_eq!(displays.get(), 0);
}
