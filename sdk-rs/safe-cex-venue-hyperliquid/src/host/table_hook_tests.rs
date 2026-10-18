use super::{facade_rest, test_runtime::TestRuntime};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeErrorOrigin};
use std::{cell::Cell, fmt, rc::Rc};

#[test]
fn table_view_enumerates_native_keys_and_projects_only_its_reached_row() {
    use safe_cex_exchange::{js_runtime::value::from_json, native_store::DefaultStore};

    let store = Rc::new(DefaultStore::new());
    let native = store.facade().unwrap();
    let ticker = |symbol: &str| {
        from_json(&serde_json::json!({
            "id":symbol,"symbol":symbol,"bid":1,"ask":2,"last":3,"mark":4,"index":5,
            "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0
        }))
    };
    let poison = ticker("POISON");
    native
        .call_method(
            "update",
            vec![Js::object(vec![(
                "tickers",
                Js::array(vec![ticker("SELECTED"), poison.clone()]),
            )])],
        )
        .unwrap();
    let cause = Js::object(vec![("code", Js::utf16(&[0xd800]))]);
    let thrown = cause.clone();
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    poison
        .define_accessor(
            "bid",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Err(Thrown::new(thrown.clone()))
            })),
            None,
        )
        .unwrap();
    let mut realm = crate::host::test_runtime::host(Rc::new(TestRuntime::default()));
    let legacy = super::facade_store::store_object(&realm, store).unwrap();
    let view = realm.get_str(&legacy, "tickersBySymbol").unwrap();
    assert_eq!(realm.own_keys(&view, true).unwrap().len(), 2);
    assert_eq!(reads.get(), 0, "key enumeration must not read DTO fields");
    let row = realm.get_str(&view, "SELECTED").unwrap();
    assert!(
        realm
            .get_str(&row, "bid")
            .unwrap()
            .strict_equals(&Js::Number(1.0))
    );
    assert_eq!(
        reads.get(),
        0,
        "selected projection must not touch an unrelated row"
    );
    let selected = realm.get_str(&view, "POISON").unwrap();
    assert!(selected.same_reference(&poison));
    let error = realm.get_str(&selected, "bid").unwrap_err();
    assert!(error.value.same_reference(&cause));
    assert_eq!(reads.get(), 1, "the reached getter runs exactly once");
}

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
    let mut realm = crate::host::test_runtime::host(Rc::new(TestRuntime::default()));
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
    let weak = match &value {
        Js::Object(data) => Rc::downgrade(data),
        _ => unreachable!(),
    };
    let displays = Rc::new(Cell::new(0));
    let origin = ExchangeErrorOrigin::new(OriginalCause {
        value: value.clone(),
        displays: displays.clone(),
    });
    let mut failure = ExchangeError::new("Store property access failed");
    failure.origin = Some(origin.clone());
    let thrown = facade_rest::thrown_from(&mut realm, failure);
    let projection_reads = Rc::new(Cell::new(0));
    for key in ["message", "code", "response"] {
        let count = projection_reads.clone();
        let poison = realm.native("poison diagnostic", 0, move |_, _, _| {
            count.set(count.get() + 1);
            Err(Thrown::new(Js::utf16(&[0xd800])))
        });
        realm.define_getter(&thrown.value, Js::str(key), poison);
    }
    let recovered = realm.exchange_error(&thrown);
    assert_eq!(
        recovered.origin.as_ref(),
        Some(&origin),
        "HL_STORE_ORIGIN preserve the original opaque failure across the actual Host boundary"
    );
    let cause = recovered
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<OriginalCause>()
        .unwrap();
    assert!(cause.value.same_reference(&value));
    assert_eq!(
        cause
            .value
            .get_key(&[0xd800])
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(reads.get(), 0);
    assert_eq!(displays.get(), 0);
    assert_eq!(projection_reads.get(), 0);
    drop(recovered);
    drop(origin);
    drop(value);
    assert!(weak.upgrade().is_some());
    drop(thrown);
    assert!(
        weak.upgrade().is_none(),
        "opaque origin must release with its Host wrapper"
    );
}

#[test]
fn common_error_delegate_retains_native_response_and_opaque_origin() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let mut realm = crate::host::test_runtime::host(Rc::new(TestRuntime::default()));
    let response = Input::Object(vec![
        (vec![0xd800], Input::Float(-0.0)),
        ("ownUndefined".encode_utf16().collect(), Input::Undefined),
    ]);
    let error = ExchangeError::new("retained response").with_retained_response(response.clone());
    let origin = error.origin.clone();
    let thrown = facade_rest::thrown_from(&mut realm, error);
    let projected = realm.get_str(&thrown.value, "response").unwrap();
    let number = realm
        .get(&projected, &Js::String(vec![0xd800].into()))
        .unwrap();
    let Js::Number(number) = number else {
        panic!("retained response number")
    };
    assert_eq!(number.to_bits(), (-0.0f64).to_bits());
    assert!(projected.has_own("ownUndefined"));
    assert!(matches!(
        realm.get_str(&projected, "ownUndefined").unwrap(),
        Js::Undefined
    ));
    let recovered = realm.exchange_error(&thrown);
    assert_eq!(recovered.origin, origin);
    assert_eq!(recovered.retained_response_input(), Some(response));
}
