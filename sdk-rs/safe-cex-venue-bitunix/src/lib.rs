//! Bitunix venue primitives and host-projected exchange operations.
pub mod api;
pub mod exchange_lifecycle;
pub mod history_mappers;
pub mod history_numbers;
pub mod history_reads;
pub mod history_rest;
pub mod ohlcv_subscription;
pub mod order_ids;
pub mod order_links;
pub mod order_models;
pub mod order_placement;
pub mod order_state;
pub mod position_reads;
pub mod response_helpers;
pub mod rest_reads;
pub mod rest_writes;
pub mod row_extraction;
pub mod session_control;
pub mod tpsl_helpers;
pub mod types;
pub mod utils;
pub mod value_helpers;
pub mod websocket_futures;
pub mod websocket_private;
pub mod websocket_public;
pub mod wire;

pub mod composition;
pub mod event_models;
pub mod order_cancellation;
pub mod orderbook_subscription;
pub mod position_adapter;

pub mod adapter;
mod adapter_exchange;
mod adapter_history;
mod adapter_orders;
mod adapter_rest;
mod native_http;
mod sockets;

#[cfg(test)]
extern crate self as safe_cex_venue_bitunix;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/support/venue_runtime.rs"]
mod native_query_fixture;
#[cfg(test)]
mod native_query_tests;
