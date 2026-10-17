//! Bitget venue family. See the linked TypeScript guidance in CLAUDE.md.
#[cfg(test)]
extern crate self as safe_cex_venue_bitget;
pub mod account_positions;
pub mod account_predicates;
pub mod account_read;
pub mod api;
pub mod cancellation;
pub mod family;
pub mod fills;
pub mod history_reads;
pub mod income;
pub mod inverse_logic;
pub mod market;
pub mod normalization;
pub mod numeric;
pub mod order_lifecycle;
pub mod order_logic;
pub mod order_placement;
pub mod order_preparation;
pub mod order_reads;
pub mod orders;
pub mod pnl;
pub mod positioning;
pub mod positions;
pub mod private_recovery;
pub mod private_throttles;
pub mod private_topics;
pub mod profile;
pub mod protocol;
pub mod public_data;
pub mod rest_actions;
pub mod runtime_orchestration;
pub mod secondary_public;
pub mod socket_ping;
pub mod socket_runtime;
pub mod stream_lifecycle;
pub mod stream_positions;
pub mod stream_predicates;
pub mod stream_timers;
pub mod subscriptions;
pub mod ticker;
pub mod types;

pub mod assembly;
pub mod inverse_positions;

pub mod binding;
pub use binding::exchange::{BitgetDependencies, BitgetExchange};
