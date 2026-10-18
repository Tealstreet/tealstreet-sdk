//! Child-module witness accesses the actual constructed receiver, without a test API.
#[allow(
    dead_code,
    reason = "primitive constructed fixture offers other workflow operations"
)]
mod support {
    use crate as safe_cex_venue_paradex;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));
}
use safe_cex_exchange::js_runtime::value::{Js, from_retained_input, to_typed};
use safe_cex_types::{Order, retained_json::RetainedJsonInput as Input};
#[test]
fn actual_map_order_receiver_retains_opaque_native_info() {
    let (venue, _, store, _) = support::fixture_with(
        serde_json::json!({"key":"","secret":"","publicDataOnly":true}),
        |_| {},
    );
    let mut raw:Input=serde_json::json!({"id":"direct-opaque","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","status":"OPEN","price":"101.5","size":"2","remaining_size":"1","created_at":1700000000000f64,"reduce_only":false}).into();
    raw.as_object_mut().unwrap().push((
        "opaque".encode_utf16().collect(),
        Input::Object(vec![
            ("unset".encode_utf16().collect(), Input::Undefined),
            ("text".encode_utf16().collect(), Input::Text(vec![0xd800])),
            (
                "number".encode_utf16().collect(),
                Input::Float(f64::INFINITY),
            ),
            ("negativeZero".encode_utf16().collect(), Input::Float(-0.)),
            (vec![0xdc00], Input::Text(vec![65])),
        ]),
    ));
    let result = venue
        .rt
        .this
        .call_method("mapOrder", vec![from_retained_input(raw)])
        .unwrap();
    let opaque = result.get("info").unwrap().get("opaque").unwrap();
    assert!(opaque.has_own("unset"));
    assert!(matches!(opaque.get("unset").unwrap(), Js::Undefined));
    assert_eq!(
        opaque.get("text").unwrap().to_js_string().unwrap().as_ref(),
        [0xd800]
    );
    assert_eq!(
        opaque.get("number").unwrap().to_number().unwrap(),
        f64::INFINITY
    );
    assert_eq!(
        opaque
            .get("negativeZero")
            .unwrap()
            .to_number()
            .unwrap()
            .to_bits(),
        (-0f64).to_bits()
    );
    let order: Order = to_typed(&result).unwrap();
    assert_eq!(order.id, "direct-opaque");
    assert_eq!(order.amount, 2.);
    assert_eq!(order.filled, 1.);
    store.add_or_update_order(order).unwrap();
    let orders = store.orders().unwrap();
    let opaque = orders[0].info.as_ref().unwrap().get("opaque").unwrap();
    assert!(matches!(opaque.get("unset"), Some(Input::Undefined)));
    assert_eq!(
        opaque.get("text").unwrap().as_text(),
        Some([0xd800].as_slice())
    );
    assert_eq!(opaque.get("number").unwrap().as_f64(), Some(f64::INFINITY));
    assert!(
        opaque
            .as_object()
            .unwrap()
            .iter()
            .any(|(key, value)| key == &[0xdc00] && value.as_text() == Some([65].as_slice()))
    );
}
