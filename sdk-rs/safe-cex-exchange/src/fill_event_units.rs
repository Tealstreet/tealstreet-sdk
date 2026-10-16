//! Source fill-unit table, including the ordinary-object prototype lookup boundary.
use safe_cex_num::{TableHit, js_table_lookup};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillEventUnit {
    Native,
    Base,
}
const UNITS: &[(&str, FillEventUnit)] = &[
    ("bybit", FillEventUnit::Native),
    ("bybitdemo", FillEventUnit::Native),
    ("bybitinverse", FillEventUnit::Native),
    ("bybitspot", FillEventUnit::Native),
    ("hyrotrader", FillEventUnit::Native),
    ("bitrue", FillEventUnit::Native),
    ("binance", FillEventUnit::Native),
    ("binancepm", FillEventUnit::Native),
    ("binancespot", FillEventUnit::Native),
    ("hyperliquid", FillEventUnit::Native),
    ("hyperliquidspot", FillEventUnit::Native),
    ("grvt", FillEventUnit::Native),
    ("asterdex", FillEventUnit::Native),
    ("weex", FillEventUnit::Native),
    ("backpack", FillEventUnit::Native),
    ("okx", FillEventUnit::Base),
    ("okxeu", FillEventUnit::Base),
    ("okxdemo", FillEventUnit::Base),
    ("okxeudemo", FillEventUnit::Base),
    ("kucoin", FillEventUnit::Base),
    ("blofin", FillEventUnit::Base),
    ("mexc", FillEventUnit::Base),
];
pub const FILL_EVENT_UNIT_VENUES: &[&str] = &[
    "bybit",
    "bybitdemo",
    "bybitinverse",
    "bybitspot",
    "hyrotrader",
    "bitrue",
    "binance",
    "binancepm",
    "binancespot",
    "hyperliquid",
    "hyperliquidspot",
    "grvt",
    "asterdex",
    "weex",
    "backpack",
    "okx",
    "okxeu",
    "okxdemo",
    "okxeudemo",
    "kucoin",
    "blofin",
    "mexc",
];
pub fn fill_event_units(exchange: &str) -> TableHit<FillEventUnit> {
    match js_table_lookup(UNITS, exchange) {
        TableHit::Mapped(unit) => TableHit::Mapped(*unit),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    }
}
