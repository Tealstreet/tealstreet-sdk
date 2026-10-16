//! Exhaustive canonical journal record eligibility for numeric ingress.
use super::JsNumberOutput;
use safe_cex_types::{
    PositionCloseReason, PositionProvenance, PositionRecord, PositionRecordLeg,
    PositionRecordState, PositionSourceAxis,
};
impl JsNumberOutput for PositionCloseReason {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum {User, Liquidation, Adl, Settlement});
    }
}
impl JsNumberOutput for PositionProvenance {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum {Reported, Derived});
    }
}
impl JsNumberOutput for PositionSourceAxis {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum {ClosedPositions, ClosedPositionsOrders, Fills, FillsOrders, Orders});
    }
}
impl JsNumberOutput for PositionRecordLeg {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum {Long, Short, Both});
    }
}
impl JsNumberOutput for PositionRecordState {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum {Open, Closed});
    }
}
impl JsNumberOutput for PositionRecord {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct {key, account, symbol, settle_currency, side, position_side, state, opened_at, closed_at, closed_size, max_size, leverage, closed_notional, venue_id, avg_entry_price, avg_exit_price, realized_pnl, fees, funding, funding_attributed, close_reason, provenance, source_axis, close_count});
    }
}
