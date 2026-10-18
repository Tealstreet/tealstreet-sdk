//! Polymarket source mirrors, added in manifest-scoped slices.
pub mod constants;
pub mod disposal;
pub mod types;

pub mod market_records;
pub mod order_adapters;
pub mod price_reads;
pub mod public_reads;
pub mod rest_forwarding;
pub mod spot_contracts;
pub mod utilities;
pub mod ws_dispatch;
pub mod ws_lifecycle;
pub mod ws_records;
pub mod ws_timers;

pub mod account_reads;

pub mod state_sync;

pub mod auth_init;

pub mod exchange;

pub mod api_factory;
pub mod classes;

pub mod binding;

#[cfg(test)]
extern crate self as safe_cex_venue_polymarket;
