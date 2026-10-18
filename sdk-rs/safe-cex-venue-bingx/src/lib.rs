//! Function-by-function BingX port; see the per-source parity manifests.
#[cfg(test)]
extern crate self as safe_cex_venue_bingx;

pub mod account_reads;
pub mod amendment;
pub mod api;
pub mod constants;
pub mod data_reads;
pub mod errors;
pub mod exchange;
pub mod listen_key;
pub mod market_utils;
pub mod placement;
pub mod precision;
pub mod private_dispatch;
pub mod private_events;
pub mod public_stream;
pub mod read_history;
pub mod stream_subscriptions;
pub mod ticker;
pub mod types;
pub mod websocket;

pub mod binding;
pub use binding::{BingxDependencies, BingxExchange};
