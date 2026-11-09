//! Native output policy for live fills; the TS reader only decodes this layout.
use crate::engine::{Fill, FillFinancials};
use crate::wire::{FrameWriter, NONE, section};

pub fn event_id(fill: &Fill) -> &str {
    fill.id.as_deref().unwrap_or(&fill.order_id)
}

pub fn financial(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

/// Revision 2 always carries the public event identity and NaN optional sentinels.
pub fn write_fills<'a>(
    writer: &mut FrameWriter<'a>,
    fills: &'a [(Fill, Option<FillFinancials>)],
    market_index: impl Fn(&str) -> Option<u32>,
) {
    if fills.is_empty() {
        return;
    }
    writer.begin_revision(section::FILLS, 2);
    writer.u32(fills.len() as u32);
    let orders: Vec<u32> = fills
        .iter()
        .map(|(fill, _)| writer.string(&fill.order_id))
        .collect();
    writer.u32s(orders);
    writer.u32s(
        fills
            .iter()
            .map(|(fill, _)| market_index(&fill.symbol).unwrap_or(NONE)),
    );
    writer.u32s(fills.iter().map(|(fill, _)| fill.side as u32));
    writer.pad();
    for (fill, _) in fills {
        writer.f64s([fill.price, fill.amount]);
    }
    for (fill, _) in fills {
        let symbol = writer.string(&fill.symbol);
        writer.u32(symbol);
    }
    for (fill, _) in fills {
        let id = writer.string(event_id(fill));
        writer.u32(id);
    }
    writer.pad();
    writer.f64s(
        fills
            .iter()
            .map(|(_, values)| financial(values.and_then(|v| v.realized_pnl)).unwrap_or(f64::NAN)),
    );
    writer.f64s(
        fills
            .iter()
            .map(|(_, values)| financial(values.and_then(|v| v.fee)).unwrap_or(f64::NAN)),
    );
    writer.end();
}
