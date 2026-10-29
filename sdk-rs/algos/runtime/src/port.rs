use std::cell::Ref;
use std::future::Future;
use std::pin::Pin;

use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::VenueError;
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
    /// A wire reply to the client that started the algo, under an existing op
    /// name (`chaser:complete`, `warning`, `error`, `log`, ...) with JSON data.
    fn emit(&self, op: &str, data: String);
}
