//! Market display policy shared by the packed producer and native fixtures.
use crate::model::Market;
use crate::wire::{FrameWriter, market_flag, section};

pub(crate) fn optional(value: Option<f64>) -> f64 {
    value.filter(|v| v.is_finite()).unwrap_or(f64::NAN)
}

pub(crate) fn prepare(market: &mut Market, units: u32) {
    if market.pretty_symbol.is_none()
        && units == crate::QuantityUnits::ContractCount as u32
        && !market.id.ends_with("-SWAP")
    {
        market.pretty_symbol = Some(format!("{}/{} XPERP", market.base, market.quote));
    }
}

pub(crate) fn numbers(m: &Market) -> [f64; 10] {
    [
        m.precision.amount,
        m.precision.price,
        m.limits.amount.min,
        m.limits.amount.max,
        m.limits.leverage.min,
        m.limits.leverage.max,
        optional(m.limits.price.map(|p| p.min)),
        optional(m.contract_size),
        optional(m.taker_fee),
        optional(m.maker_fee),
    ]
}

pub(crate) fn write<'a>(markets: &'a [Market], units: u32, w: &mut FrameWriter<'a>) {
    let pretty = markets.iter().any(|market| market.pretty_symbol.is_some());
    w.begin_revision(section::MARKETS, u16::from(pretty));
    w.u32(markets.len() as u32);
    for m in markets {
        let refs = [
            w.string(&m.id),
            w.string(&m.symbol),
            w.string(&m.base),
            w.string(&m.quote),
        ];
        w.u32s(refs);
    }
    w.u32s(markets.iter().map(|m| {
        [
            (Some(m.active), market_flag::ACTIVE),
            (m.is_inverse, market_flag::INVERSE),
            (m.is_spot, market_flag::SPOT),
            (m.is_perpetual, market_flag::PERPETUAL),
            (m.is_expiring, market_flag::EXPIRING),
            (m.is_read_only, market_flag::READ_ONLY),
        ]
        .iter()
        .filter(|(set, _)| *set == Some(true))
        .fold(units << market_flag::UNITS_SHIFT, |bits, (_, flag)| {
            bits | flag
        })
    }));
    w.pad();
    for column in 0..10 {
        w.f64s(markets.iter().map(|market| numbers(market)[column]));
    }
    if pretty {
        for market in markets {
            let reference = w.optional_string(market.pretty_symbol.as_deref());
            w.u32(reference);
        }
    }
    w.end();
}
