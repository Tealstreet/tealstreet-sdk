//! Phemex source-by-source port; see CLAUDE.md for current coverage.
#[cfg(test)]
extern crate self as safe_cex_venue_phemex;
pub mod account;
pub mod constants;
pub mod ticker_reads;
pub mod types;
pub mod utilities;

pub mod validation;

pub mod position_control;
pub mod rest_account_control;
pub mod rest_order_control;
pub mod session_control;
pub mod ticker_parser;

pub mod public_routing;

pub mod api_transport;
pub mod private_session;

pub mod rest_data;
pub mod start_order_formatting;
pub mod websocket_data;

pub mod binding;
