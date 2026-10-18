//! Bybit V5 data and request machinery; variants remain inside this crate.
mod hyro_configuration;
pub mod rate_limits;
pub mod spot_types;
pub mod types;

pub mod response;

pub mod api;
pub mod api_factory;
mod api_request;
pub mod site;

pub mod exchange_state;

pub mod order_params;

pub mod order_format;

pub mod exchange_errors;

pub mod ws_public;

pub mod ws_private;

pub mod fill_event;

#[cfg(test)]
mod union_audit;

mod market_mapping;

mod candle_mapping;

mod book_mapping;

pub mod history_mapping;

pub mod journal_fold;

pub mod rest_markets;

mod primary_fills;

mod rest_fills;

pub mod inverse_position;

mod order_mapping;

mod rest_positions;

mod submission;

pub mod binding;
pub mod exchange;
pub use binding::BybitDependencies;
pub use exchange::BybitExchange;

mod json_field;

pub mod exchange_helpers;

pub mod fee_overlay;
pub mod ledger_mapping;

pub mod variant_accessors;

pub mod failure_diagnostics;

pub mod execution_guards;

pub mod account_readers;

pub mod private_ws_lifecycle;

pub mod api_validation;

pub mod public_ticker_topics;

pub mod public_ws_lifecycle;

pub mod position_controls;

pub mod stream_delegation;

pub mod ticker_readers;

pub mod hyro_private_session;

pub mod primary_order_lifecycle;

pub mod variant_account_refresh;

pub mod public_data_recovery;

pub mod core_snapshot_readers;

pub mod hyro_capabilities;

pub mod ledger_history;

pub mod hyro_private_reads;

pub mod hyro_snapshot;
pub mod spot_order_writes;

pub mod hyro_order_publication;

pub mod inverse_read_surface;

pub mod spot_exchange_reads;

pub mod private_snapshot_refresh;

pub mod local_order_reconciliation;
pub mod primary_local_order_writes;

pub mod primary_startup;

pub mod inverse_symbol;
pub mod options;

#[cfg(test)]
extern crate self as safe_cex_venue_bybit;

#[cfg(test)]
#[path = "../tests/support/binding_harness.rs"]
mod retained_config_binding_harness;
