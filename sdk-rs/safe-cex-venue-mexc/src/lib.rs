//! MEXC futures port; behavioral reference: packages/safe-cex/src/exchanges/mexc/CLAUDE.md.
pub mod api;
pub mod types;
pub mod utils;

pub mod client;
pub mod exchange_errors;
pub mod exchange_helpers;

pub mod exchange;

pub mod private_session;
pub mod public_subscriptions;
pub mod submit_validation;
pub mod symbol_config;
pub mod ws;
pub mod ws_lifecycle;
pub mod ws_private;
pub mod ws_public;

pub mod exchange_runtime;

pub mod order_workflow;

pub mod runtime_assembly;
