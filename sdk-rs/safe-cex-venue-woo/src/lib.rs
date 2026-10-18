//! WOO legacy venue and inert V3 migration contracts.
pub mod constants;
pub mod journal;
pub mod types;
pub mod v3_contracts;

pub mod v3_public;

pub mod api;

pub mod legacy_fills;

pub mod order_history;

pub mod utils;

pub mod pricing;

pub mod response;
pub mod response_class;

pub mod factory;

pub mod private_socket;
pub mod public_socket;

pub mod orders;

pub mod runtime_selector;

pub mod transaction;

pub mod history_fetch;

pub mod historical_fills;

pub mod order_pages;

pub mod v3_request;
pub mod v3_ticker;
pub mod v3_ws_quotes;

pub mod v3_ws_topics;

pub mod v3_ws_commands;

pub mod v3_ws_session;

pub mod v3_ws_public;

pub mod v3_ws_state;

pub mod v3_ws_private;

pub mod v3_account_writes;

pub mod v3_private;

pub mod v3_private_orders;

pub mod v3_regular_writes;

pub mod legacy_metadata;

pub mod legacy_socket_control;

pub mod v3_algo_writes;

pub mod legacy_market_reads;

pub mod legacy_order_control;

pub mod legacy_data_reads;

pub mod v3_position_mode;

pub mod legacy_fill_recovery;
pub mod public_parity;

pub mod legacy_order_writes;

pub mod public_canonical;

pub mod legacy_lifecycle;

pub mod public_streams;

pub mod class_construction;

pub mod socket_classes;

pub mod exchange_class;

pub mod binding;
pub use binding::exchange::{WooDependencies, WooExchange};

#[cfg(test)]
extern crate self as safe_cex_venue_woo;

#[cfg(test)]
#[path = "../tests/support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_test_harness;
