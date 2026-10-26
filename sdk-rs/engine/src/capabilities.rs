//! What a venue answers beyond the commands every venue implements. Hosts
//! route their client surface from this record instead of venue names. Snapshot
//! frames carry it (`section::CAPABILITIES`), so frame consumers learn it too.

use crate::wire::{FrameWriter, section};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub account: AccountLookup,
    /// `FindOrderByClientOrderId` asks the venue rather than the local store.
    pub client_order_lookup: bool,
    pub fill_history: bool,
    pub order_history: bool,
    pub income_history: bool,
    /// `FetchFills` filters by `orderId`.
    pub fills_by_order_id: bool,
    pub cancel_all: CancelAll,
    pub leverage_query: LeverageQuery,
    pub max_leverage: MaxLeverage,
    /// `SetLeverage` with an empty symbol sets the whole account.
    pub account_leverage: bool,
    /// `SetMarginType` with an empty symbol sets the whole account.
    pub account_margin_type: bool,
    /// Cancels need each order's symbol: `CancelOrdersWithSymbols`.
    pub cancel_with_symbols: bool,
    /// Amendments need the order's symbol: `AmendOrderWithSymbol`.
    pub amend_with_symbol: bool,
    /// `FetchOrderBook` answers a REST snapshot.
    pub order_book_query: bool,
    /// Option chains, greeks and IV-priced orders.
    pub options: bool,
    /// The quantity convention of every market's orders and positions.
    pub quantity_units: QuantityUnits,
    /// `FetchAccountTier` / `SetAccountTier` venue actions (Lighter).
    pub account_tier: bool,
    /// The `FetchClosedPositions` venue action: venue-reported closed-position rows.
    pub closed_positions: bool,
    /// The `FetchBalanceInfo` venue action: the raw account payload for `balance.info`.
    pub balance_info: bool,
    /// Explicit public catalogue refresh through VenueAction::RefreshMarkets.
    pub market_refresh: bool,
    /// Host journal adapter and whether fill folding is account- or symbol-scoped.
    pub position_history: PositionHistory,
    pub closed_position_format: ClosedPositionFormat,
    /// Legacy SDK time-bound normalization expected by FetchFills.
    pub fill_history_time: FillHistoryTime,
    pub stp_controls: bool,
    pub bybit_execution_controls: bool,
    pub reject_client_slippage: bool,
    pub finite_command_numbers: bool,
    pub finite_history_bounds: bool,
    pub reject_order_history_filters: bool,
    pub public_catalogue_validation: bool,
    pub isolated_book_subscriptions: bool,
    pub inverse_closed_positions: bool,
    /// Signed account-wide absolute cancellation scheduling (Hyperliquid perps).
    pub schedule_cancel: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionHistory {
    Unsupported,
    FillsAccount,
    FillsSymbol,
    Bybit,
    Okx,
    Blofin,
    WooOrders,
    Bitunix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClosedPositionFormat {
    Unsupported,
    Bybit,
    Okx,
    Blofin,
    Bitunix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FillHistoryTime {
    AsProvided,
    SecondsOrMilliseconds,
    NonnegativeSecondsOrMilliseconds,
}

/// How a market's order and position quantities relate to its placement units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QuantityUnits {
    Base,
    /// Placement is in contracts of `contractSize` base; frames stay in base.
    NativePlacement,
    /// Inverse quantities are native quote contracts (Bitget inverse).
    InverseNative,
    /// Placement is in contract counts of `ctVal` (OKX).
    ContractCount,
    /// Inverse contracts of a fixed USD `contractSize` (Binance COIN-M).
    InverseContractSized,
}

/// How the host cancels every open order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelAll {
    /// `CancelSymbolOrders` for each symbol with an open order.
    EachOpenSymbol,
    /// Also each symbol a placement was attempted on, since frames can lag the ACK.
    EachPlacedSymbol,
    /// `CancelSymbolOrders` with an empty symbol cancels every open order.
    Native,
}

/// How `FetchAccount` answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountLookup {
    Unsupported,
    Query,
    /// The identity is discovered during startup; ask once the account loads.
    AfterLoad,
}

/// When `FetchLeverage` reads the venue; otherwise rails come from settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LeverageQuery {
    Settings,
    OnRead,
    /// Rails are read when a symbol's metadata refreshes, then served from settings.
    OnMetadata,
    /// Explicit reads are fresh, and selecting a symbol also refreshes its metadata.
    OnReadAndMetadata,
}

/// Where a symbol's maximum leverage comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaxLeverage {
    Market,
    /// `FetchMaxLeverage` on private accounts; the market limit otherwise.
    Query,
    /// The larger side of `FetchLeverage`.
    Rails,
    Unsupported,
}

impl Capabilities {
    pub const BASE: Self = Self {
        account: AccountLookup::Unsupported,
        client_order_lookup: false,
        fill_history: true,
        order_history: true,
        income_history: true,
        fills_by_order_id: false,
        cancel_all: CancelAll::EachOpenSymbol,
        leverage_query: LeverageQuery::Settings,
        max_leverage: MaxLeverage::Market,
        account_leverage: false,
        account_margin_type: false,
        cancel_with_symbols: false,
        amend_with_symbol: false,
        order_book_query: false,
        options: false,
        quantity_units: QuantityUnits::Base,
        account_tier: false,
        closed_positions: false,
        balance_info: false,
        market_refresh: false,
        position_history: PositionHistory::Unsupported,
        closed_position_format: ClosedPositionFormat::Unsupported,
        fill_history_time: FillHistoryTime::AsProvided,
        stp_controls: false,
        bybit_execution_controls: false,
        reject_client_slippage: false,
        finite_command_numbers: false,
        finite_history_bounds: false,
        reject_order_history_filters: false,
        public_catalogue_validation: false,
        isolated_book_subscriptions: false,
        inverse_closed_positions: false,
        schedule_cancel: false,
    };

    /// No history axis: fills, orders and income are all unavailable.
    pub const NO_HISTORY: Self = Self {
        fill_history: false,
        order_history: false,
        income_history: false,
        ..Self::BASE
    };
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::BASE
    }
}

impl Capabilities {
    /// Two words: boolean flags, then four-bit enum fields (layout in wire.mjs).
    pub fn write(&self, w: &mut FrameWriter<'_>) {
        let flags = [
            self.client_order_lookup,
            self.fill_history,
            self.order_history,
            self.income_history,
            self.fills_by_order_id,
            self.account_leverage,
            self.account_margin_type,
            self.cancel_with_symbols,
            self.amend_with_symbol,
            self.order_book_query,
            self.options,
            self.account_tier,
            self.closed_positions,
            self.balance_info,
            false, // Bit 14 reserved: retired per-venue host-clock policy.
            self.market_refresh,
            self.stp_controls,
            self.bybit_execution_controls,
            self.reject_client_slippage,
            self.finite_command_numbers,
            self.finite_history_bounds,
            self.reject_order_history_filters,
            self.public_catalogue_validation,
            self.isolated_book_subscriptions,
            self.inverse_closed_positions,
            self.schedule_cancel,
        ]
        .iter()
        .enumerate()
        .fold(0u32, |bits, (bit, set)| bits | (u32::from(*set) << bit));
        let enums = self.account as u32
            | (self.cancel_all as u32) << 4
            | (self.leverage_query as u32) << 8
            | (self.max_leverage as u32) << 12
            | (self.quantity_units as u32) << 16
            | (self.position_history as u32) << 20
            | (self.closed_position_format as u32) << 24
            | (self.fill_history_time as u32) << 28;
        w.begin(section::CAPABILITIES);
        w.u32s([flags, enums]);
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{FrameWriter, sections};

    #[test]
    fn the_wire_record_packs_flags_then_enum_fields() {
        let record = Capabilities {
            account: AccountLookup::AfterLoad,
            client_order_lookup: true,
            options: true,
            closed_positions: true,
            market_refresh: true,
            cancel_all: CancelAll::EachPlacedSymbol,
            leverage_query: LeverageQuery::OnMetadata,
            max_leverage: MaxLeverage::Unsupported,
            quantity_units: QuantityUnits::InverseContractSized,
            ..Capabilities::NO_HISTORY
        };
        let mut w = FrameWriter::new(vec![], 0.0);
        record.write(&mut w);
        let frame = w.finish();
        let (kind, payload) = sections(&frame).next().unwrap();
        assert_eq!(kind, section::CAPABILITIES);
        let word = |at: usize| u32::from_le_bytes(payload[at..at + 4].try_into().unwrap());
        assert_eq!(word(0), 1 | 1 << 10 | 1 << 12 | 1 << 15);
        assert_eq!(word(4), 2 | 1 << 4 | 2 << 8 | 3 << 12 | 4 << 16);
        assert_eq!(payload.len(), 8);
        assert_eq!(word(0) & (1 << 14), 0);
        // Append the combined policy without changing any existing wire discriminant.
        assert_eq!(LeverageQuery::Settings as u32, 0);
        assert_eq!(LeverageQuery::OnRead as u32, 1);
        assert_eq!(LeverageQuery::OnMetadata as u32, 2);
        assert_eq!(LeverageQuery::OnReadAndMetadata as u32, 3);
    }

    #[test]
    fn bridge_policies_append_to_the_existing_two_words() {
        let record = Capabilities {
            position_history: PositionHistory::WooOrders,
            closed_position_format: ClosedPositionFormat::Blofin,
            fill_history_time: FillHistoryTime::NonnegativeSecondsOrMilliseconds,
            stp_controls: true,
            bybit_execution_controls: true,
            reject_client_slippage: true,
            finite_command_numbers: true,
            finite_history_bounds: true,
            reject_order_history_filters: true,
            public_catalogue_validation: true,
            isolated_book_subscriptions: true,
            inverse_closed_positions: true,
            schedule_cancel: true,
            ..Capabilities::NO_HISTORY
        };
        let mut w = FrameWriter::new(vec![], 0.0);
        record.write(&mut w);
        let frame = w.finish();
        let (kind, payload) = sections(&frame).next().unwrap();
        assert_eq!(kind, section::CAPABILITIES);
        assert_eq!(payload.len(), 8);
        assert_eq!(
            u32::from_le_bytes(payload[..4].try_into().unwrap()),
            (0x1ff << 16) | (1 << 25)
        );
        assert_eq!(
            u32::from_le_bytes(payload[4..].try_into().unwrap()),
            6 << 20 | 3 << 24 | 2 << 28
        );
    }

    #[test]
    fn bitunix_journal_and_closed_format_append_new_discriminants() {
        let record = Capabilities {
            closed_positions: true,
            position_history: PositionHistory::Bitunix,
            closed_position_format: ClosedPositionFormat::Bitunix,
            ..Capabilities::BASE
        };
        let mut w = FrameWriter::new(vec![], 0.0);
        record.write(&mut w);
        let frame = w.finish();
        let (_, payload) = sections(&frame).next().unwrap();
        // Existing discriminants keep their wire values; Bitunix appends to each.
        assert_eq!(PositionHistory::WooOrders as u32, 6);
        assert_eq!(ClosedPositionFormat::Blofin as u32, 3);
        let enums = u32::from_le_bytes(payload[4..8].try_into().unwrap());
        assert_eq!((enums >> 20) & 0xf, 7);
        assert_eq!((enums >> 24) & 0xf, 4);
        assert_ne!(
            u32::from_le_bytes(payload[..4].try_into().unwrap()) & (1 << 12),
            0
        );
        let json = serde_json::to_value(record).unwrap();
        assert_eq!(json["positionHistory"], "bitunix");
        assert_eq!(json["closedPositionFormat"], "bitunix");
    }
}
