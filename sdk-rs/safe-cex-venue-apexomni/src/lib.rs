//! ApeX Omni protocol data and strict wire models.
pub mod api;
pub mod api_runtime;
pub mod config_records;
mod data;
mod decimal;
pub mod exchange;
pub mod exchange_utils;
pub mod executions;
pub mod historical_fills;
pub mod interface;
pub mod lifecycle;
pub mod order_records;
pub mod private_records;
pub mod protocol;
pub mod public_records;
mod response;
pub mod signer_records;
pub mod signer_utils;
pub mod socket_host;
pub mod trading;
pub mod transaction_records;
pub mod transaction_signing;
pub mod utils;
pub mod ws_private;
pub mod ws_public;
pub mod zklink_host;

pub mod binding;
pub use binding::{ApexDependencies, ApexOmniExchange};

#[cfg(test)]
#[path = "../tests/support/accepted_scalar_helper_support.rs"]
mod accepted_scalar_helper_support;
