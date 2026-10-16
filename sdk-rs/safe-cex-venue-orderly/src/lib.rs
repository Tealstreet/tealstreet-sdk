//! Orderly source bodies and concrete shared-runtime Exchange composition.
pub mod account_order_reads;
pub mod api;
pub mod classes;
pub mod data_mappers;
pub mod encryption;
pub mod errors;
pub mod margin;
pub mod public_feeds;
pub mod runtime_actions;
pub mod socket_classes;
pub mod stream_reports;
pub mod trading_actions;
pub mod types;
pub mod utils;
pub mod wire;
pub mod ws_controls;

mod binding;
pub mod native_exchange;

#[cfg(test)]
extern crate self as safe_cex_venue_orderly;
#[cfg(test)]
#[path = "../tests/support/exchange_runtime.rs"]
mod binding_test_support;
