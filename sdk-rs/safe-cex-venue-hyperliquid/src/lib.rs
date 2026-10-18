//! Hyperliquid types and REST API; exchange/WS implementations remain pending.
pub mod agent;
pub mod balance_state;
pub mod binding;
pub mod browser_approval_flows;
pub mod builder_fee;
pub mod constants;
pub mod exchange_helpers;
pub mod lifecycle_delegates;
pub mod market_data_selectors;
pub mod order_request_leaves;
pub(crate) mod host;
pub mod spot_constants;
pub mod spot_json;
pub mod spot_models;
pub mod spot_types;
pub mod types;

pub mod signing;

pub mod api;
pub mod order_response;
pub mod rate_limit;

pub mod cloid;
pub mod order_history;
pub mod order_inference;

// Exchange transport and websocket composition is still incomplete.
pub mod exchange;

pub mod json_boundaries;
pub mod json_models;
pub mod json_values;

pub mod account_position_leaves;
pub mod api_errors;
pub mod cache_orders;
pub mod cross_cache;
pub mod market_helpers;
pub mod private_position_helpers;
pub mod ws_emission;
pub mod ws_ping_lifecycle;
pub mod ws_transport_leaves;

pub mod cache_runtime;
pub mod private_constructor;
pub mod private_message_dispatch;

pub mod core_account_snapshots;
pub mod core_position_order_state;
pub mod core_rest_reads;
pub mod helper_tasks;
pub mod income;
pub mod market_startup;
pub mod spot_ws_dispatch;

pub mod candle_listeners;
pub mod spot_account_reads;

pub mod spot_client;
pub mod spot_public_subscriptions;

pub mod cancellation;
pub mod core_public_message;
pub mod hip3_position_refresh;
pub mod limit_orders;
pub mod market_bootstrap;
pub mod order_reads;
pub mod order_retry;
pub mod order_update;
pub mod placement_dispatch;
pub mod private_class;
pub mod public_constructor;
pub mod spot_market_bootstrap;
pub mod spot_runtime;
pub mod spot_trading;
pub mod spot_user_event;
pub mod startup_account_resolution;
pub mod startup_lifecycle;
pub mod startup_timeout;

pub mod core_book_listener;

pub mod public_class;

pub mod core_exchange_class;
pub mod core_exchange_leaves;

pub mod spot_binding;

#[cfg(test)]
#[path = "../tests/support/accepted_projection_aliases.rs"]
mod accepted_projection_aliases;

#[cfg(test)]
#[path = "../tests/support/accepted_future_aliases.rs"]
mod accepted_future_aliases;
