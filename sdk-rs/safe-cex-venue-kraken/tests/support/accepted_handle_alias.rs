//! Handle<Rt> from the ReadHost retained by actual constructed exchanges.
use super::Rt;
use super::accepted_read_receivers::terminal::{harness, options};
use crate::rest_account::{Handle, ReadHost};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_types::ExchangeName;
use std::{cell::Cell, rc::Rc};

#[test]
fn handle_alias_retains_constructed_scope_values_identity_and_effects() {
    for name in [ExchangeName::Kraken, ExchangeName::Krakenspot] {
        let h = harness(name, options(), |_| {});
        let rt = &h.exchange.rt;
        let value = Js::object(vec![
            ("units", Js::utf16(&[0xd800, b'X' as u16, 0xdc00])),
            ("negativeZero", Js::Number(-0.0)),
            ("overflow", Js::Number(f64::INFINITY)),
        ]);
        let calls = Rc::new(Cell::new(0));
        let seen = calls.clone();
        let returned = value.clone();
        let row = Js::object(vec![]);
        row.define_accessor(
            "cell",
            Some(Rc::new(move |_| {
                seen.set(seen.get() + 1);
                Ok(returned.clone())
            })),
            None,
        )
        .unwrap();
        let handle: Handle<Rt> = ReadHost::keyed(rt, &row, "cell").unwrap();
        let independently_typed: Rc<Js> = handle.clone();
        assert!(Rc::ptr_eq(&handle, &independently_typed));
        assert!(handle.strict_equals(&value));
        assert_eq!(calls.get(), 1);
        assert_eq!(
            handle
                .get("units")
                .unwrap()
                .as_str()
                .map(|units| units.as_ref()),
            Some(&[0xd800, b'X' as u16, 0xdc00][..])
        );
        assert_eq!(
            handle
                .get("negativeZero")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            handle.get("overflow").unwrap().as_f64(),
            Some(f64::INFINITY)
        );
        value.set("later", Js::Bool(true)).unwrap();
        assert!(
            handle.get("later").unwrap().truthy(),
            "alias retains the original referent"
        );
        let missing: Handle<Rt> = ReadHost::keyed(rt, &row, "missing").unwrap();
        assert!(missing.is_undefined());
        let error = Js::object(vec![("message", Js::utf16(&[0xd800]))]);
        let original = error.clone();
        row.define_accessor(
            "failure",
            Some(Rc::new(move |_| Err(Thrown::new(original.clone())))),
            None,
        )
        .unwrap();
        assert!(
            ReadHost::keyed(rt, &row, "failure")
                .unwrap_err()
                .value
                .strict_equals(&error)
        );
    }
}

#[test]
fn read_result_alias_keeps_typed_success_and_original_thrown_value() {
    use crate::rest_account::ReadResult;
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let success: ReadResult<Rt, bool> =
        ReadHost::includes(&h.exchange.rt, &Js::str("native result"), "result");
    assert!(success.unwrap());
    let error = Js::object(vec![("marker", Js::utf16(&[0xdc00]))]);
    let original = error.clone();
    let row = Js::object(vec![]);
    row.define_accessor(
        "failure",
        Some(Rc::new(move |_| Err(Thrown::new(original.clone())))),
        None,
    )
    .unwrap();
    let result: ReadResult<Rt, Handle<Rt>> = ReadHost::field(
        &h.exchange.rt,
        &row,
        &"failure".encode_utf16().collect::<Vec<_>>(),
        true,
    );
    assert!(result.unwrap_err().value.strict_equals(&error));
    let absent: ReadResult<Rt, Handle<Rt>> = ReadHost::field(&h.exchange.rt, &Js::Null, &[], true);
    assert!(absent.unwrap().is_undefined());
}

#[test]
fn entries_alias_keeps_own_key_order_utf16_and_original_native_cells() {
    use crate::rest_account::{Entries, ReadResult};
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    let value = Js::object(vec![("marker", Js::Bool(true))]);
    let row = Js::object(vec![("first", value.clone())]);
    row.define_key(&[0xd800], Js::Number(-0.0)).unwrap();
    row.define("last", Js::Number(f64::INFINITY)).unwrap();
    let result: ReadResult<Rt, Entries<Rt>> = ReadHost::entries(&h.exchange.rt, &row);
    let entries = result.unwrap();
    let independently_typed: &Vec<(Vec<u16>, Rc<Js>)> = &entries;
    assert_eq!(independently_typed.len(), 3);
    assert_eq!(entries[0].0, "first".encode_utf16().collect::<Vec<_>>());
    assert!(entries[0].1.strict_equals(&value));
    assert_eq!(entries[1].0, [0xd800]);
    assert_eq!(
        entries[1].1.as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(entries[2].0, "last".encode_utf16().collect::<Vec<_>>());
    assert_eq!(entries[2].1.as_f64(), Some(f64::INFINITY));
}
