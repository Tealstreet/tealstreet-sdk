//! Backpack venue: native method bodies and owned exchange/socket adapters.
pub mod api;
pub mod constants;
pub mod types;

pub mod signing;

pub mod order_utils;

pub mod account_reads;

pub mod private_order_utils;

pub mod batch_orders;

pub mod public_control;

pub mod orderbook_state;
pub mod private_control;

pub mod private_auth;
pub mod socket_lifecycle;

pub mod socket_events;

pub mod private_events;

pub mod startup_recovery;

pub mod rest_reads;

pub mod order_actions;

pub mod api_factory;

pub mod public_listeners;

pub mod native_value;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native_host;
pub mod native_http;

pub mod adapter;

pub mod public_adapter;
pub mod socket_parent;

pub mod private_adapter;

pub mod adapter_exchange;
pub mod adapter_orders;
pub mod adapter_runtime;
pub mod model_bridge;
pub mod weak_listener;
