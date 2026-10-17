//! Path-dependent fills reconstruction; missing/opening-window data remains explicit.
use indexmap::IndexMap;
use safe_cex_types::{
    Execution, ExecutionPositionSide, OrderSide, PositionRecordLeg, PositionSide,
};
use std::cmp::Ordering;

type LegResolver<'a> = &'a mut dyn FnMut(&Execution) -> Option<PositionRecordLeg>;

const FLAT_EPSILON: f64 = 1e-9;
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionInterval {
    pub symbol: String,
    pub position_side: PositionRecordLeg,
    pub side: PositionSide,
    pub opened_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<f64>,
    pub max_size: f64,
    pub closed_size: f64,
    pub complete: bool,
    pub closed_by_flip: bool,
    pub avg_entry_price: f64,
    pub avg_exit_price: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_pnl: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_pnl_partial: Option<bool>,
    pub fees: f64,
    pub fee_currencies: Vec<String>,
}
// Private metadata companion: decode only objects into the canonical interval.
#[derive(serde::Deserialize)]
#[serde(remote = "PositionInterval", rename_all = "camelCase")]
struct PositionIntervalWire {
    symbol: String,
    position_side: PositionRecordLeg,
    side: PositionSide,
    opened_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    closed_at: Option<f64>,
    max_size: f64,
    closed_size: f64,
    complete: bool,
    closed_by_flip: bool,
    avg_entry_price: f64,
    avg_exit_price: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    realized_pnl_partial: Option<bool>,
    fees: f64,
    fee_currencies: Vec<String>,
}
impl<'de> serde::Deserialize<'de> for PositionInterval {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
            deserializer,
        )?
        .decode_with(|decoder| {
            PositionIntervalWire::deserialize(safe_cex_types::strict_decode::object(decoder))
        })
        .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for PositionInterval {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {
            symbol, position_side, side, opened_at, closed_at, max_size, closed_size,
            complete, closed_by_flip, avg_entry_price, avg_exit_price, realized_pnl,
            realized_pnl_partial, fees, fee_currencies
        });
    }
}
#[derive(Default)]
pub struct PositionIntervalOptions<'a> {
    /// Caller supplies Object.entries order; ordinary symbol|leg keys are not integer keys.
    pub opening_size: IndexMap<String, f64>,
    pub resolve_leg: Option<LegResolver<'a>>,
}
struct RunningPosition {
    signed: f64,
    opened_at: f64,
    max_size: f64,
    closed_size: f64,
    complete: bool,
    entry_qty: f64,
    entry_notional: f64,
    exit_qty: f64,
    exit_notional: f64,
    realized_pnl: f64,
    saw_realized_pnl: bool,
    saw_missing_realized_pnl: bool,
    fees: f64,
    fee_currencies: Vec<String>,
}
fn fresh_position() -> RunningPosition {
    RunningPosition {
        signed: 0.,
        opened_at: 0.,
        max_size: 0.,
        closed_size: 0.,
        complete: true,
        entry_qty: 0.,
        entry_notional: 0.,
        exit_qty: 0.,
        exit_notional: 0.,
        realized_pnl: 0.,
        saw_realized_pnl: false,
        saw_missing_realized_pnl: false,
        fees: 0.,
        fee_currencies: vec![],
    }
}
fn accrue_fill(position: &mut RunningPosition, execution: &Execution, reduces: bool) {
    if let Some(commission) = execution.commission {
        position.fees += commission;
        if let Some(asset) = execution
            .commission_asset
            .as_ref()
            .filter(|s| !s.is_empty())
            && !position.fee_currencies.contains(asset)
        {
            position.fee_currencies.push(asset.clone());
        }
    }
    if let Some(pnl) = execution.realized_pnl {
        position.realized_pnl += pnl;
        position.saw_realized_pnl = true;
    } else if reduces {
        position.saw_missing_realized_pnl = true;
    }
}
fn finish(
    position: &RunningPosition,
    symbol: &str,
    leg: PositionRecordLeg,
    closed_at: Option<f64>,
) -> PositionInterval {
    PositionInterval {
        symbol: symbol.into(),
        position_side: leg,
        side: if position.signed > 0. {
            PositionSide::Long
        } else {
            PositionSide::Short
        },
        opened_at: position.opened_at,
        closed_at,
        max_size: position.max_size,
        closed_size: position.closed_size,
        complete: position.complete,
        closed_by_flip: false,
        avg_entry_price: if position.entry_qty > 0. {
            position.entry_notional / position.entry_qty
        } else {
            0.
        },
        avg_exit_price: if position.exit_qty > 0. {
            position.exit_notional / position.exit_qty
        } else {
            0.
        },
        realized_pnl: (position.saw_realized_pnl && !position.saw_missing_realized_pnl)
            .then_some(position.realized_pnl),
        realized_pnl_partial: Some(position.saw_realized_pnl && position.saw_missing_realized_pnl),
        fees: position.fees,
        fee_currencies: position.fee_currencies.clone(),
    }
}
fn leg_text(leg: PositionRecordLeg) -> &'static str {
    match leg {
        PositionRecordLeg::Long => "long",
        PositionRecordLeg::Short => "short",
        PositionRecordLeg::Both => "both",
    }
}
fn leg_key(symbol: &str, leg: PositionRecordLeg) -> String {
    format!("{symbol}|{}", leg_text(leg))
}
fn compare_execution_order(a: &Execution, b: &Execution) -> Ordering {
    if a.timestamp != b.timestamp {
        return (a.timestamp - b.timestamp)
            .partial_cmp(&0.)
            .unwrap_or(Ordering::Equal);
    }
    let left = a.id.as_deref().unwrap_or("");
    let right = b.id.as_deref().unwrap_or("");
    if left.is_empty()
        || right.is_empty()
        || !left.bytes().all(|b| b.is_ascii_digit())
        || !right.bytes().all(|b| b.is_ascii_digit())
    {
        return Ordering::Equal;
    }
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}
fn sign(number: f64) -> f64 {
    if number == 0. || number.is_nan() {
        number
    } else {
        number.signum()
    }
}

pub fn reconstruct_position_intervals(
    executions: &[Execution],
    options: &mut PositionIntervalOptions<'_>,
) -> Result<Vec<PositionInterval>, String> {
    let mut ordered = executions.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| compare_execution_order(a, b));
    let mut running = IndexMap::<String, RunningPosition>::new();
    let mut intervals = vec![];
    for (key, signed) in &options.opening_size {
        if signed.abs() <= FLAT_EPSILON {
            continue;
        }
        running.insert(
            key.clone(),
            RunningPosition {
                signed: *signed,
                max_size: signed.abs(),
                complete: false,
                ..fresh_position()
            },
        );
    }
    for execution in ordered {
        let leg = match &execution.position_side {
            Some(ExecutionPositionSide::Side(PositionSide::Long)) => PositionRecordLeg::Long,
            Some(ExecutionPositionSide::Side(PositionSide::Short)) => PositionRecordLeg::Short,
            Some(ExecutionPositionSide::Both(_)) => PositionRecordLeg::Both,
            None => options
                .resolve_leg
                .as_mut()
                .and_then(|resolve| resolve(execution))
                .unwrap_or(PositionRecordLeg::Both),
        };
        let key = leg_key(&execution.symbol, leg);
        let at = execution.timestamp * 1000.;
        let delta = if execution.side == OrderSide::Buy {
            execution.qty
        } else {
            -execution.qty
        };
        let price = execution.price;
        let current = running.get_mut(&key);
        if current
            .as_ref()
            .is_none_or(|c| c.signed.abs() <= FLAT_EPSILON)
        {
            let mut opened = RunningPosition {
                signed: delta,
                opened_at: current.as_ref().filter(|c| !c.complete).map_or(at, |c| {
                    if c.opened_at == 0. || c.opened_at.is_nan() {
                        at
                    } else {
                        c.opened_at
                    }
                }),
                max_size: delta.abs(),
                complete: current.as_ref().is_none_or(|c| c.complete),
                entry_qty: execution.qty,
                entry_notional: execution.qty * price,
                ..fresh_position()
            };
            accrue_fill(&mut opened, execution, false);
            running.insert(key, opened);
            continue;
        }
        let current = current.unwrap();
        if current.opened_at == 0. {
            current.opened_at = at;
        }
        let next = current.signed + delta;
        if sign(delta) == sign(current.signed) {
            current.signed = next;
            current.max_size = safe_cex_num::math_max(current.max_size, next.abs());
            current.entry_qty += execution.qty;
            current.entry_notional += execution.qty * price;
            accrue_fill(current, execution, false);
            continue;
        }
        let closed_now = safe_cex_num::math_min(delta.abs(), current.signed.abs());
        current.closed_size += closed_now;
        current.exit_qty += closed_now;
        current.exit_notional += closed_now * price;
        if next.abs() > FLAT_EPSILON && sign(next) == sign(current.signed) {
            current.signed = next;
            accrue_fill(current, execution, true);
            continue;
        }
        let crossed = next.abs() > FLAT_EPSILON;
        if !crossed {
            accrue_fill(current, execution, true);
            intervals.push(finish(current, &execution.symbol, leg, Some(at)));
            running.shift_remove(&key);
            continue;
        }
        let closed_share = closed_now / delta.abs();
        let mut closing = execution.clone();
        closing.commission = execution.commission.map(|fee| fee * closed_share);
        accrue_fill(current, &closing, true);
        let mut closed = finish(current, &execution.symbol, leg, Some(at));
        closed.closed_by_flip = true;
        intervals.push(closed);
        let opened_size = next.abs();
        let opened = RunningPosition {
            signed: next,
            opened_at: at,
            max_size: opened_size,
            entry_qty: opened_size,
            entry_notional: opened_size * price,
            fees: execution
                .commission
                .map_or(0., |fee| fee * (1. - closed_share)),
            fee_currencies: execution
                .commission_asset
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned()
                .into_iter()
                .collect(),
            ..fresh_position()
        };
        running.insert(key, opened);
    }
    for (key, current) in &running {
        if current.signed.abs() <= FLAT_EPSILON {
            continue;
        }
        // Source slice(0, lastIndexOf('|')) uses -1 when no delimiter exists.
        let (symbol, leg) = match key.rsplit_once('|') {
            Some(parts) => parts,
            None if matches!(key.as_str(), "long" | "short" | "both") => {
                (&key[..key.len() - 1], key.as_str())
            }
            None => {
                return Err(
                    "RepresentationError: openingSize leg is outside PositionRecordLeg".into(),
                );
            }
        };
        let leg = match leg {
            "long" => PositionRecordLeg::Long,
            "short" => PositionRecordLeg::Short,
            "both" => PositionRecordLeg::Both,
            _ => {
                return Err(
                    "RepresentationError: openingSize leg is outside PositionRecordLeg".into(),
                );
            }
        };
        intervals.push(finish(current, symbol, leg, None));
    }
    intervals.sort_by(|a, b| {
        (a.closed_at.unwrap_or(9007199254740991.) - b.closed_at.unwrap_or(9007199254740991.))
            .partial_cmp(&0.)
            .unwrap_or(Ordering::Equal)
    });
    Ok(intervals)
}

pub struct FundingRow {
    pub symbol: String,
    pub timestamp: f64,
    pub income: f64,
}
pub struct FundingAttribution {
    pub funding: f64,
    pub attributed: bool,
}
pub fn attribute_funding(
    interval: &PositionInterval,
    funding_rows: &[FundingRow],
    hedged: bool,
) -> FundingAttribution {
    let Some(closed_at) = interval.closed_at.filter(|_| interval.complete) else {
        return FundingAttribution {
            funding: 0.,
            attributed: false,
        };
    };
    let mut funding = 0.;
    for row in funding_rows {
        if row.symbol != interval.symbol {
            continue;
        }
        if row.timestamp < interval.opened_at || row.timestamp > closed_at {
            continue;
        }
        funding += row.income;
    }
    FundingAttribution {
        funding,
        attributed: !hedged,
    }
}
