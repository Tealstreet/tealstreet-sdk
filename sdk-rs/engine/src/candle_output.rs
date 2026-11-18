//! Native candle output; timestamps and prices are never rescaled at this boundary.
use crate::{
    model::Candle,
    wire::{FrameWriter, section},
};

pub fn write<'a>(writer: &mut FrameWriter<'a>, rows: &[(u32, &'a str, Candle)]) {
    if rows.is_empty() {
        return;
    }
    writer.begin(section::CANDLES);
    writer.u32(rows.len() as u32);
    writer.u32s(rows.iter().map(|(market, ..)| *market));
    let intervals: Vec<u32> = rows.iter().map(|(_, i, _)| writer.string(i)).collect();
    writer.u32s(intervals);
    writer.pad();
    for (_, _, c) in rows {
        writer.f64s([c.timestamp, c.open, c.high, c.low, c.close, c.volume]);
    }
    writer.end();
}
