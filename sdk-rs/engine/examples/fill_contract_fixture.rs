//! Real native FILLS writer fixture consumed by the SDK's fillFrames test.
pub use safe_cex_engine::{engine, wire};
#[path = "../src/fill_output.rs"]
mod fill_output;

use engine::{Fill, FillFinancials};
use safe_cex_engine::model::OrderSide;
use serde_json::json;

fn main() {
    let fills = vec![
        (
            Fill {
                id: Some("native-fill-1".into()),
                order_id: "order-1".into(),
                symbol: "BTC-PERP".into(),
                side: OrderSide::Buy,
                price: 100.0,
                amount: 0.25,
            },
            Some(FillFinancials {
                realized_pnl: Some(0.0),
                fee: Some(0.0),
            }),
        ),
        (
            Fill {
                id: None,
                order_id: "order-1".into(),
                symbol: "BTC-PERP".into(),
                side: OrderSide::Buy,
                price: 101.0,
                amount: 0.75,
            },
            None,
        ),
        (
            Fill {
                id: Some("native-fill-3".into()),
                order_id: "order-2".into(),
                symbol: "BTC-PERP".into(),
                side: OrderSide::Sell,
                price: 0.0,
                amount: 0.0,
            },
            Some(FillFinancials {
                realized_pnl: Some(f64::INFINITY),
                fee: Some(f64::NAN),
            }),
        ),
    ];
    let mut writer = wire::FrameWriter::new(Vec::new(), 1700000000125.0);
    fill_output::write_fills(&mut writer, &fills, |_| None);
    let events: Vec<_> = fills.iter().map(|(fill, values)| {
        let mut row = json!({"id": fill_output::event_id(fill), "orderId":fill.order_id,
            "symbol":fill.symbol, "side":match fill.side { OrderSide::Buy => "buy", OrderSide::Sell => "sell" },
            "price":fill.price, "amount":fill.amount});
        if let Some(value) = fill_output::financial(values.and_then(|v| v.realized_pnl)) { row["realizedPnl"] = json!(value); }
        if let Some(value) = fill_output::financial(values.and_then(|v| v.fee)) { row["fee"] = json!(value); }
        row
    }).collect();
    serde_json::to_writer(
        std::io::stdout(),
        &json!({"frame":writer.finish(), "events":events}),
    )
    .unwrap();
}
