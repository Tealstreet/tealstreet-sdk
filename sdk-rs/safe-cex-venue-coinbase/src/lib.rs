//! Coinbase Advanced Trade and inert Starbase parity port.
mod binding;
pub mod decimal;
pub mod exchange_lifecycle;
pub mod json_key_scanner;
pub mod market_position_reads;
pub mod order_create;
pub mod orders;
pub mod portfolios_balances;
pub mod private_health;
pub mod private_order_snapshot;
pub mod private_stream_dispatch;
pub mod protocol;
pub mod public_data;
pub mod public_market_listeners;
pub mod public_market_reads;
pub mod public_stream_dispatch;
pub mod rest_reads;
pub mod starbase;
pub mod starbase_auth;
pub mod starbase_cutover;
pub mod starbase_orders;
pub mod starbase_private;
pub mod starbase_public;
pub mod starbase_public_frames;
pub mod starbase_rpc;
pub mod starbase_session;
pub mod starbase_ws_controller;
pub mod tables;
pub mod types;
pub mod utils;

pub mod cdp_signing;
pub mod exchange_tick;
pub mod exchange_ws_auth;
pub mod order_sync;

pub mod api_factory;
pub mod classes;
pub mod order_edit;

pub mod class_methods;

pub use binding::exchange::{CoinbaseDependencies, CoinbaseExchange};
