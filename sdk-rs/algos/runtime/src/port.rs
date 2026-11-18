use std::cell::Ref;
use std::future::Future;
use std::pin::Pin;

use safe_cex_engine::Capabilities;
use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::CancelOrderRef;
use tokio::sync::mpsc;

use crate::AlgoStore;

/// A command's completion: the `CommandDone` result JSON, or the venue error.
pub type CommandFuture = Pin<Box<dyn Future<Output = Result<String, VenueError>>>>;

/// What one flush frame changed. Ticker-only frames set only `tickers`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    pub orders: bool,
    pub positions: bool,
    pub tickers: bool,
    pub fills: Vec<Fill>,
    /// The feed rejoined: fills sent while it was down are lost, so algos
    /// reconcile fills over REST before acting on the remainder again.
    pub resync: bool,
}

/// One account as an algo sees it, implemented by the server over its account
/// handle. Commands follow the engine's rules: no retry, and an uncertain
/// outcome is reported, never replayed.
pub trait AccountPort {
    /// The latest decoded state; already updated when a [`Change`] is delivered.
    /// Never hold the borrow across an `.await`: the port applies frames between polls.
    fn store(&self) -> Ref<'_, AlgoStore>;
    /// One [`Change`] per applied flush frame, from the moment of subscribing.
    /// Unbounded so no fill is ever dropped for a busy algo; dropping the
    /// receiver unsubscribes.
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change>;
    fn next_command_id(&self) -> CommandId;
    fn command(&self, command: Command) -> CommandFuture;
    /// The venue's capability record: which cancel and amend forms it accepts.
    fn capabilities(&self) -> Capabilities;
    /// A wire reply to the client that started the algo, under an existing op
    /// name (`chaser:complete`, `warning`, `error`, `log`, ...) with JSON data.
    fn emit(&self, op: &str, data: String);
}

/// A cancel of `orders` in the form the venue accepts: symbol-routed where its
/// engine declares `cancel_with_symbols`, else by id (Bybit refuses the
/// symbol-routed form outright, so the order would stay live).
pub fn cancel_command(
    capabilities: &Capabilities,
    command: CommandId,
    orders: Vec<CancelOrderRef>,
) -> Command {
    if capabilities.cancel_with_symbols {
        Command::CancelOrdersWithSymbols { command, orders }
    } else {
        Command::CancelOrders {
            command,
            ids: orders.into_iter().map(|order| order.id).collect(),
        }
    }
}

/// An amend in the form the venue accepts: with its symbol where the engine
/// declares `amend_with_symbol`, else by id.
pub fn amend_command(
    capabilities: &Capabilities,
    command: CommandId,
    id: String,
    symbol: String,
    price: Option<f64>,
    amount: Option<f64>,
) -> Command {
    if capabilities.amend_with_symbol {
        Command::AmendOrderWithSymbol {
            command,
            id,
            symbol,
            price,
            amount,
        }
    } else {
        Command::AmendOrder {
            command,
            id,
            price,
            amount,
        }
    }
}
