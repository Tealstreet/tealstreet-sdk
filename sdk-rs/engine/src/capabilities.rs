//! What a venue answers beyond the commands every venue implements. Hosts
//! route their client surface from this record instead of venue names.

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
    /// `CancelSymbolOrders` with an empty symbol cancels every open order.
    pub cancel_all: bool,
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
        cancel_all: false,
        leverage_query: LeverageQuery::Settings,
        max_leverage: MaxLeverage::Market,
        account_leverage: false,
        account_margin_type: false,
        cancel_with_symbols: false,
        amend_with_symbol: false,
        order_book_query: false,
        options: false,
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
