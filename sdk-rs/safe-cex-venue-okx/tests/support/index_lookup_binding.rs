//! Index selection through the constructed OKX receiver and canonical Store rows.
use super::binding_harness::*;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
use safe_cex_types::ExchangeName;
use std::{cell::Cell, rc::Rc};

fn fixture() -> Harness {
    harness(ExchangeName::Okx, options(), |_| {})
}

#[test]
fn index_lookup_reads_only_source_fields_and_returns_original_rows() {
    let row = Js::object(vec![
        ("id", Js::str("BTC-USDT-SWAP")),
        ("info", Js::object(vec![("uly", Js::str("BTC-USDT"))])),
    ]);
    let reads = Rc::new(Cell::new(0));
    let calls = reads.clone();
    row.define_accessor(
        "limits",
        Some(Rc::new(move |_| {
            calls.set(calls.get() + 1);
            Err(Thrown::type_error("unrelated limits must stay unread"))
        })),
        None,
    )
    .unwrap();
    let h = fixture();
    h.store
        .native_value("markets")
        .unwrap()
        .call_method("push", vec![row.clone()])
        .unwrap();
    let selected = h
        .exchange
        .scope
        .this
        .call_method("getMarketsForIndexTickerInstId", vec![Js::str("BTC-USDT")])
        .expect("index lookup must not decode whole Market");
    assert_eq!(reads.get(), 0);
    assert!(selected.get("0").unwrap().strict_equals(&row));
    selected
        .get("0")
        .unwrap()
        .set("marker", Js::Bool(true))
        .unwrap();
    assert!(row.get("marker").unwrap().strict_equals(&Js::Bool(true)));
}

#[test]
fn index_lookup_uses_live_method_strict_comparison_and_filter_length() {
    let row = Js::object(vec![("marker", Js::Number(1.))]);
    let h = fixture();
    let rows = h.store.native_value("markets").unwrap();
    rows.set("length", Js::Number(3.)).unwrap();
    rows.set("0", row.clone()).unwrap();
    rows.set("2", Js::Null).unwrap();
    let receiver = h.exchange.scope.this.clone();
    let expected = receiver.clone();
    let source_rows = rows.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    receiver
        .set(
            "getIndexTickerInstId",
            Js::function("lookup", move |this, a| {
                assert!(this.strict_equals(&expected));
                let at = count.get();
                count.set(at + 1);
                if at == 0 {
                    source_rows.call_method("push", vec![Js::str("not visited")])?;
                    expected.set(
                        "getIndexTickerInstId",
                        Js::function("next", |_, _| Ok(Js::Number(7.))),
                    )?;
                }
                arg(&a, 0).get("marker")
            }),
        )
        .unwrap();
    let selected = receiver
        .call_method("getMarketsForIndexTickerInstId", vec![Js::Number(7.)])
        .unwrap();
    assert_eq!(
        calls.get(),
        1,
        "method is looked up again on next present row"
    );
    assert_eq!(selected.get("length").unwrap().to_number().unwrap(), 1.);
    assert!(selected.get("0").unwrap().strict_equals(&Js::Null));
    let none = receiver
        .call_method("getMarketsForIndexTickerInstId", vec![Js::str("7")])
        .unwrap();
    assert_eq!(none.get("length").unwrap().to_number().unwrap(), 0.);
}

#[test]
fn index_id_keeps_utf16_and_original_replace_receiver() {
    let h = fixture();
    let receiver = &h.exchange.scope.this;
    let surrogate = Js::utf16(&[0xd800]);
    let row = Js::object(vec![("info", Js::object(vec![("uly", surrogate.clone())]))]);
    assert!(
        receiver
            .call_method("getIndexTickerInstId", vec![row])
            .unwrap()
            .strict_equals(&surrogate)
    );
    let id = Js::object(vec![]);
    let expected = id.clone();
    id.set(
        "replace",
        Js::function("replace", move |this, a| {
            assert!(this.strict_equals(&expected));
            assert!(arg(&a, 1).strict_equals(&Js::str("")));
            assert!(arg(&a, 0).get("source")?.strict_equals(&Js::str("-SWAP$")));
            Ok(Js::str("custom"))
        }),
    )
    .unwrap();
    let row = Js::object(vec![("id", id), ("info", Js::Null)]);
    assert!(
        receiver
            .call_method("getIndexTickerInstId", vec![row])
            .unwrap()
            .strict_equals(&Js::str("custom"))
    );
    for suffix in ["", "\n", "\r", "\r\n", "\u{2028}", "\u{2029}"] {
        let mut id = vec![0xd800];
        id.extend("-SWAP".encode_utf16());
        id.extend(suffix.encode_utf16());
        let expected = if suffix.is_empty() {
            Js::utf16(&[0xd800])
        } else {
            Js::utf16(&id)
        };
        let row = Js::object(vec![("id", Js::utf16(&id))]);
        assert!(
            receiver
                .call_method("getIndexTickerInstId", vec![row])
                .unwrap()
                .strict_equals(&expected)
        );
    }
}

#[test]
fn index_lookup_keeps_original_throw_and_stops_after_selected_row() {
    let original = Js::object(vec![("cause", Js::str("opaque"))]);
    let thrown = original.clone();
    let row = Js::object(vec![]);
    row.define_accessor(
        "info",
        Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
        None,
    )
    .unwrap();
    let h = fixture();
    h.store
        .native_value("markets")
        .unwrap()
        .call_method("push", vec![row, Js::Null])
        .unwrap();
    let error = h
        .exchange
        .scope
        .this
        .call_method("getMarketsForIndexTickerInstId", vec![Js::str("x")])
        .unwrap_err();
    assert!(error.value.strict_equals(&original));
}

#[test]
fn index_lookup_invokes_the_array_filter_with_original_receiver() {
    let h = fixture();
    let rows = h.store.native_value("markets").unwrap();
    rows.call_method("push", vec![Js::Null]).unwrap();
    let original = rows.clone();
    let sentinel = Js::object(vec![]);
    let result = sentinel.clone();
    rows.set(
        "filter",
        Js::function("filter", move |this, a| {
            assert!(this.strict_equals(&original));
            assert!(arg(&a, 0).is_callable());
            Ok(result.clone())
        }),
    )
    .unwrap();
    assert!(
        h.exchange
            .scope
            .this
            .call_method("getMarketsForIndexTickerInstId", vec![Js::Null])
            .unwrap()
            .strict_equals(&sentinel)
    );
}

#[test]
#[ignore = "executes maintained TypeScript source with the isolated compiler"]
fn maintained_source_index_lookup_observations() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/index-lookup-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
