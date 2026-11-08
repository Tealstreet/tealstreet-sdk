//! Pure checks over native filters and the exact serialized order parameters.
use super::Params;
use crate::wire::Filter;
use safe_cex_engine::model::{Market, Ticker};
use safe_cex_engine::num;
use std::cmp::Ordering;

#[derive(Clone, Copy, Default)]
struct Lot {
    min: f64,
    max: f64,
    step: f64,
}

#[derive(Clone, Copy)]
struct Notional {
    min: f64,
    max: f64,
    min_market: bool,
    max_market: bool,
}

#[derive(Clone, Default)]
pub(crate) struct NativeRules {
    lot: Option<Lot>,
    market_lot: Option<Lot>,
    price: Lot,
    notionals: Vec<Notional>,
}

fn metadata(text: Option<&str>) -> Result<f64, String> {
    let Some(text) = text else { return Ok(0.0) };
    let value = num::parse_decimal(text)
        .filter(|v| *v >= 0.0)
        .ok_or("Invalid Binance numeric filter")?;
    if Decimal::parse(text)?.compare(Decimal::parse(&num::format(value))?)? != Ordering::Equal {
        return Err("Binance filter exceeds supported decimal precision".into());
    }
    Ok(value)
}

impl NativeRules {
    pub(crate) fn from_wire(filters: &[Filter<'_>]) -> Result<Self, String> {
        let mut rules = Self::default();
        for filter in filters {
            match filter.filter_type.as_ref() {
                "LOT_SIZE" | "MARKET_LOT_SIZE" => {
                    let lot = Lot {
                        min: metadata(filter.min_qty.as_deref())?,
                        max: metadata(filter.max_qty.as_deref())?,
                        step: metadata(filter.step_size.as_deref())?,
                    };
                    if lot.max > 0.0 && lot.min > lot.max {
                        return Err("Invalid Binance quantity bounds".into());
                    }
                    if filter.filter_type == "LOT_SIZE" {
                        rules.lot = Some(lot);
                    } else {
                        rules.market_lot = Some(lot);
                    }
                }
                "PRICE_FILTER" => {
                    rules.price = Lot {
                        min: metadata(filter.min_price.as_deref())?,
                        max: metadata(filter.max_price.as_deref())?,
                        step: metadata(filter.tick_size.as_deref())?,
                    }
                }
                "MIN_NOTIONAL" | "NOTIONAL" => rules.notionals.push(Notional {
                    min: metadata(filter.notional.as_deref())?,
                    max: metadata(filter.max_notional.as_deref())?,
                    min_market: filter
                        .apply_min_to_market
                        .or(filter.apply_to_market)
                        .unwrap_or(true),
                    max_market: filter.apply_max_to_market.unwrap_or(true),
                }),
                _ => {}
            }
        }
        Ok(rules)
    }

    fn valuation(
        &self,
        params: &Params,
        market: &Market,
        ticker: Option<&Ticker>,
    ) -> Result<String, String> {
        if market.is_inverse == Some(true) {
            return positive(market.contract_size.unwrap_or(0.0)).map(num::format);
        }
        if let Some(price) = params.get("price").or_else(|| {
            (market.is_spot == Some(true))
                .then(|| params.get("stopPrice"))
                .flatten()
        }) {
            positive(parse(&price)?)?;
            return Ok(price);
        }
        let value = ticker.map_or(0.0, |t| {
            if market.is_spot == Some(true) {
                t.last
            } else {
                t.mark
            }
        });
        positive(value).map(num::format)
    }

    fn notional_bounds(&self, params: &Params) -> (f64, f64) {
        let is_market = params.get("type").as_deref() == Some("MARKET");
        let reduce = params.get("reduceOnly").as_deref() == Some("true");
        let mut min: f64 = 0.0;
        let mut max = f64::INFINITY;
        for rule in &self.notionals {
            if !reduce && (!is_market || rule.min_market) {
                min = min.max(rule.min);
            }
            if rule.max > 0.0 && (!is_market || rule.max_market) {
                max = max.min(rule.max);
            }
        }
        (min, max)
    }

    pub(super) fn quantities(
        &self,
        params: &Params,
        market: &Market,
        ticker: Option<&Ticker>,
        amount: f64,
        max: f64,
        step: f64,
    ) -> Result<Vec<f64>, String> {
        let mut min = market.limits.amount.min.max(0.0);
        let mut max = if max.is_finite() && max > 0.0 {
            max
        } else {
            amount
        };
        let mut step = positive(step)?;
        for lot in self.applicable_lots(params) {
            min = min.max(lot.min);
            if lot.max > 0.0 {
                max = max.min(lot.max);
            }
            if lot.step > 0.0 {
                let larger = step.max(lot.step);
                exact_units(larger, step.min(lot.step))?;
                step = larger;
            }
        }
        let total = exact_units(positive(amount)?, step)?;
        let mut minimum = ceil_units(min, step)?.max(1.0);
        let mut maximum = floor_units(max, step)?;
        let (min_notional, max_notional) = self.notional_bounds(params);
        if min_notional > 0.0 || max_notional.is_finite() {
            let unit_value = Decimal::parse(&num::format(step))?
                .product(Decimal::parse(&self.valuation(params, market, ticker)?)?)?;
            minimum = minimum
                .max(Decimal::parse(&num::format(min_notional))?.whole_units(unit_value, true)?);
            if max_notional.is_finite() {
                maximum = maximum.min(
                    Decimal::parse(&num::format(max_notional))?.whole_units(unit_value, false)?,
                );
            }
        }
        if maximum < minimum || total < minimum {
            return Err("Binance quantity cannot form valid native lots".into());
        }
        let mut remaining = total;
        let mut lots = Vec::new();
        while remaining > 0.0 {
            if lots.len() >= 10_000 {
                return Err("Binance order requires too many native lots".into());
            }
            let lot = remaining.min(maximum);
            lots.push(lot);
            remaining -= lot;
        }
        if let Some(tail) = lots.last().copied().filter(|last| *last < minimum) {
            let mut deficit = minimum - tail;
            let last = lots.len() - 1;
            for previous in lots[..last].iter_mut().rev() {
                let taken = (*previous - minimum).max(0.0).min(deficit);
                *previous -= taken;
                deficit -= taken;
                if deficit == 0.0 {
                    break;
                }
            }
            if deficit != 0.0 {
                return Err("Binance split leaves an invalid quantity tail".into());
            }
            lots[last] = minimum;
        }
        if lots.iter().sum::<f64>() != total {
            return Err("Binance split does not conserve quantity".into());
        }
        Ok(lots
            .into_iter()
            .map(|units| num::adjust(units * step, step))
            .collect())
    }

    fn applicable_lots(&self, params: &Params) -> impl Iterator<Item = Lot> {
        let market = params.get("type").as_deref() == Some("MARKET");
        self.lot
            .into_iter()
            .chain(self.market_lot.filter(|_| market))
    }

    pub(super) fn validate(
        &self,
        params: &Params,
        market: &Market,
        ticker: Option<&Ticker>,
    ) -> Result<(), String> {
        for name in [
            "price",
            "stopPrice",
            "triggerPrice",
            "activationPrice",
            "activatePrice",
        ] {
            if let Some(text) = params.get(name) {
                let value = positive(parse(&text)?)?;
                if self.price.step > 0.0 {
                    exact_units(value, self.price.step)?;
                }
                if value < self.price.min || (self.price.max > 0.0 && value > self.price.max) {
                    return Err("Binance price is outside native bounds".into());
                }
            }
        }
        if params.get("closePosition").as_deref() == Some("true") {
            return Ok(());
        }
        let quantity = params
            .get("quantity")
            .ok_or("Binance order has no explicit quantity")?;
        let value = positive(parse(&quantity)?)?;
        exact_units(value, market.precision.amount)?;
        if value < market.limits.amount.min
            || (market.limits.amount.max > 0.0 && value > market.limits.amount.max)
        {
            return Err("Binance quantity is outside market bounds".into());
        }
        for lot in self.applicable_lots(params) {
            if value < lot.min || (lot.max > 0.0 && value > lot.max) {
                return Err("Binance quantity is outside native bounds".into());
            }
            if lot.step > 0.0 {
                exact_units(value, lot.step)?;
            }
        }
        let (min, max) = self.notional_bounds(params);
        if min > 0.0 || max.is_finite() {
            let notional = Decimal::parse(&quantity)?
                .product(Decimal::parse(&self.valuation(params, market, ticker)?)?)?;
            if notional.compare(Decimal::parse(&num::format(min))?)? == Ordering::Less
                || (max.is_finite()
                    && notional.compare(Decimal::parse(&num::format(max))?)? == Ordering::Greater)
            {
                return Err(
                    "Binance lot is outside the native notional bounds at its wire price".into(),
                );
            }
        }
        Ok(())
    }
}

fn parse(text: &str) -> Result<f64, String> {
    num::parse_decimal(text).ok_or_else(|| "Invalid Binance wire number".into())
}
fn positive(value: f64) -> Result<f64, String> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err("Binance wire value must be finite and positive".into())
    }
}
fn exact_units(value: f64, step: f64) -> Result<f64, String> {
    positive(step)?;
    let units = (value / step).round();
    if !units.is_finite()
        || units.abs() > 9_007_199_254_740_991.0
        || num::adjust(value, step) != value
    {
        return Err("Binance wire value is not on its native grid".into());
    }
    Ok(units)
}
fn floor_units(value: f64, step: f64) -> Result<f64, String> {
    let rounded = num::adjust(value, step);
    let units = exact_units(rounded, step)?;
    Ok(if rounded > value { units - 1.0 } else { units })
}
fn ceil_units(value: f64, step: f64) -> Result<f64, String> {
    let rounded = num::adjust(value, step);
    let units = exact_units(rounded, step)?;
    Ok(if rounded < value { units + 1.0 } else { units })
}

#[derive(Clone, Copy)]
struct Decimal {
    units: u128,
    scale: u32,
}
impl Decimal {
    fn parse(text: &str) -> Result<Self, String> {
        let mut out = Self { units: 0, scale: 0 };
        let mut fraction = false;
        let mut digits = false;
        for byte in text.trim().bytes() {
            if byte == b'.' && !fraction {
                fraction = true;
                continue;
            }
            if !byte.is_ascii_digit() {
                return Err("Unsupported Binance decimal filter".into());
            }
            digits = true;
            out.units = out
                .units
                .checked_mul(10)
                .and_then(|n| n.checked_add(u128::from(byte - b'0')))
                .ok_or("Binance decimal exceeds validation capacity")?;
            if fraction {
                out.scale = out
                    .scale
                    .checked_add(1)
                    .ok_or("Binance decimal scale exceeds validation capacity")?;
            }
        }
        if !digits {
            return Err("Empty Binance decimal".into());
        }
        Ok(out.normalized())
    }
    fn normalized(mut self) -> Self {
        while self.scale > 0 && self.units.is_multiple_of(10) {
            self.units /= 10;
            self.scale -= 1;
        }
        self
    }
    fn product(self, other: Self) -> Result<Self, String> {
        Ok(Self {
            units: self
                .units
                .checked_mul(other.units)
                .ok_or("Binance notional exceeds validation capacity")?,
            scale: self
                .scale
                .checked_add(other.scale)
                .ok_or("Binance notional scale exceeds validation capacity")?,
        }
        .normalized())
    }
    fn whole_units(self, unit_value: Self, round_up: bool) -> Result<f64, String> {
        let scale = self.scale.max(unit_value.scale);
        let align = |value: Self| {
            value
                .units
                .checked_mul(10u128.checked_pow(scale - value.scale)?)
        };
        let numerator = align(self).ok_or("Binance notional exceeds validation capacity")?;
        let denominator = align(unit_value)
            .filter(|v| *v > 0)
            .ok_or("Invalid Binance unit value")?;
        let mut units = numerator / denominator;
        if round_up && !numerator.is_multiple_of(denominator) {
            units = units
                .checked_add(1)
                .ok_or("Binance lot count exceeds validation capacity")?;
        }
        let value = parse(&units.to_string())?;
        if value > 9_007_199_254_740_991.0 {
            return Err("Binance lot count exceeds validation capacity".into());
        }
        Ok(value)
    }

    fn compare(self, other: Self) -> Result<Ordering, String> {
        let scale = self.scale.max(other.scale);
        let align = |value: Self| {
            value
                .units
                .checked_mul(10u128.checked_pow(scale - value.scale)?)
        };
        Ok(align(self)
            .ok_or("Binance notional exceeds validation capacity")?
            .cmp(&align(other).ok_or("Binance notional exceeds validation capacity")?))
    }
}
