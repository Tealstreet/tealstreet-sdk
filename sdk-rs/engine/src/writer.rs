//! `writesOnly` engines: a stateless order writer.
//!
//! Three runtimes host it side by side: the browser (the user's key signs
//! locally), hosted remote signing (the server's algo orders keep the sign-req
//! round trip), and hosted keyed (a native server writer). The contract is the
//! same in each: the writer opens no socket, makes no account read, loads only
//! its venue's public instrument metadata, and emits no store state. Every write
//! carries a [`WriterContext`] built from the host's own store; the writer
//! replaces its scratch rows with it and keeps nothing between commands.
use crate::model::{
    MarginType, Order, OrderSide, OrderStatus, OrderType, Position, PositionMode, PositionSide,
};
use serde::Deserialize;
use std::collections::BTreeMap;

/// The host's view of the account, as minimal fields in command units.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriterContext {
    #[serde(default)]
    pub orders: Vec<ContextOrder>,
    #[serde(default)]
    pub positions: Vec<ContextPosition>,
    #[serde(default)]
    pub settings: BTreeMap<String, SymbolSettings>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextOrder {
    pub id: String,
    pub symbol: String,
    pub side: OrderSide,
    #[serde(rename = "type")]
    pub kind: OrderType,
    pub price: f64,
    pub amount: f64,
    pub remaining: f64,
    pub reduce_only: bool,
    #[serde(default)]
    pub client_order_id: Option<String>,
    #[serde(default)]
    pub trigger_price: Option<f64>,
    /// Bybit: a TP/SL bound to its position (`tpslMode` Full or Partial). An
    /// amend of a Bybit stop is refused when this is absent.
    #[serde(default)]
    pub position_bound: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextPosition {
    pub symbol: String,
    pub side: PositionSide,
    pub contracts: f64,
}

/// Account settings for one symbol that a venue's write path reads.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SymbolSettings {
    #[serde(default)]
    pub hedged: Option<bool>,
    #[serde(default)]
    pub position_idx: Option<u8>,
    #[serde(default)]
    pub margin_mode: Option<MarginType>,
}

impl WriterContext {
    pub(crate) fn orders(&self) -> Vec<Order> {
        self.orders
            .iter()
            .map(|order| Order {
                id: order.id.clone(),
                parent_id: None,
                status: OrderStatus::Open,
                symbol: order.symbol.clone(),
                kind: order.kind,
                side: order.side,
                // A trigger order rests at its trigger, as the store keeps it.
                price: order.trigger_price.unwrap_or(order.price),
                amount: order.amount,
                filled: (order.amount - order.remaining).max(0.0),
                remaining: order.remaining,
                reduce_only: order.reduce_only,
                close: None,
                client_order_id: order.client_order_id.clone(),
                time_in_force: None,
                last_update: 0.0,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            })
            .collect()
    }

    pub(crate) fn positions(&self) -> Vec<Position> {
        self.positions
            .iter()
            .map(|position| {
                let settings = self.settings.get(&position.symbol);
                Position {
                    id: format!("{}-{:?}", position.symbol, position.side),
                    symbol: position.symbol.clone(),
                    side: position.side,
                    entry_price: 0.0,
                    // Command units: contracts, or notional on an inverse market.
                    notional: position.contracts,
                    leverage: 0.0,
                    margin_type: settings.and_then(|settings| settings.margin_mode),
                    position_mode: settings.and_then(|settings| settings.hedged).map(|hedged| {
                        if hedged {
                            PositionMode::Hedged
                        } else {
                            PositionMode::Oneway
                        }
                    }),
                    unrealized_pnl: 0.0,
                    realized_pnl: None,
                    contracts: position.contracts,
                    liquidation_price: 0.0,
                    margin_size: None,
                    last_update: None,
                }
            })
            .collect()
    }
}

/// Refusal for a write that reaches a writer before its venue metadata loads.
pub const MARKETS_NOT_LOADED: &str = "markets not loaded";
/// Refusal for any non-write command sent to a writer.
pub const WRITES_ONLY: &str = "a writesOnly engine accepts only write commands";

/// Why a writer refuses `command`, if it does: a refreshMarkets reload is the
/// one non-write it runs.
pub fn refusal(command: &crate::Command, markets_loaded: bool) -> Option<&'static str> {
    if is_write(command) {
        return (!markets_loaded).then_some(MARKETS_NOT_LOADED);
    }
    match command {
        crate::Command::Venue {
            action: crate::VenueAction::RefreshMarkets,
            ..
        } => None,
        _ => Some(WRITES_ONLY),
    }
}

/// The commands a writer executes.
pub fn is_write(command: &crate::Command) -> bool {
    use crate::Command;
    matches!(
        command,
        Command::PlaceOrders { .. }
            | Command::CancelOrders { .. }
            | Command::CancelOrdersWithSymbols { .. }
            | Command::AmendOrder { .. }
            | Command::AmendOrderWithSymbol { .. }
            | Command::CancelSymbolOrders { .. }
            | Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. }
    )
}
