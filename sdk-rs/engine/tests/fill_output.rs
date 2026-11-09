pub use safe_cex_engine::{engine, wire};
#[path = "../src/fill_output.rs"]
mod fill_output;

use engine::Fill;
use safe_cex_engine::model::OrderSide;

#[test]
fn native_writer_owns_event_identity_without_changing_order_identity_or_size() {
    let mut fills = vec![(
        Fill {
            id: None,
            order_id: "order".into(),
            symbol: "BTC".into(),
            side: OrderSide::Sell,
            price: 10.0,
            amount: 0.25,
        },
        None,
    )];
    for id in [None, Some("native-id"), Some("")] {
        fills[0].0.id = id.map(str::to_owned);
        let mut writer = wire::FrameWriter::new(Vec::new(), 1.0);
        fill_output::write_fills(&mut writer, &fills, |_| None);
        let rows = wire::fills(&writer.finish()).unwrap();
        assert_eq!(rows[0].id.as_deref(), Some(id.unwrap_or("order")));
        assert_eq!(rows[0].order_id, "order");
        assert_eq!(rows[0].amount, 0.25);
    }
}

#[test]
fn native_financial_policy_preserves_zero_and_negative_values_and_omits_nonfinite() {
    for value in [
        None,
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(f64::NEG_INFINITY),
    ] {
        assert_eq!(fill_output::financial(value), None);
    }
    for value in [0.0, -0.0, -1.25, 4.5] {
        assert_eq!(fill_output::financial(Some(value)), Some(value));
    }
    let mut writer = wire::FrameWriter::new(Vec::new(), 1.0);
    fill_output::write_fills(&mut writer, &[], |_| None);
    assert_eq!(wire::fills(&writer.finish()), Some(vec![]));
}
