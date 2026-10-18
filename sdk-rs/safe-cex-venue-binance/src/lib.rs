//! Binance family implementation; shared behavioral contracts live in exchange-core.
#[path = "generated/binance.rs"]
pub mod binance_types;
#[path = "generated/binanceinverse.rs"]
pub mod binanceinverse_types;
#[path = "generated/binancepm.rs"]
pub mod binancepm_types;
#[path = "generated/binancespot.rs"]
pub mod binancespot_types;

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub mod inverse;

pub mod account_fetch;
pub mod account_leverage_overlay;
pub mod account_validation;
pub mod cancel_symbol_orders;
pub mod closed_pnl;
pub mod closed_position_reader;
pub mod exchange_dispose;
pub mod exchange_error;
pub mod futures_orders;
pub mod futures_position_mode;
pub mod leverage;
pub mod listen_key;
pub mod margin_mode;
pub mod order_history;
pub mod order_history_native;
pub mod order_logic;
pub mod orderbook;
pub mod orders_retry;
pub mod pm_order_logic;
pub mod pm_orders;
pub mod pm_position_mode;
pub mod position_mode;
pub mod position_refresh;
pub mod private_helpers;
pub mod private_open;
pub mod private_ping;
pub mod protocol_types;
pub mod public_connect;
pub mod public_dispose;
pub mod public_open;
pub mod public_start;
pub mod public_stats;
pub mod public_subscriptions;
pub mod public_ticker_events;
pub mod reconciliation;
pub mod request_weight;
pub mod response_message;
pub mod response_shape;
pub mod signing;
pub mod spot_open;
pub mod spot_orders;
pub mod spot_protocol;
pub mod spot_ticker_events;
pub mod spot_valuation;
pub mod trailing_orders;
pub mod utils;

pub mod public_rest;

pub mod private_event_value;
pub mod private_events;

pub mod private_rest;

pub mod lifecycle;

pub mod api_factory;

pub mod rest_history;

pub mod order_operations;

pub mod order_batches;

pub mod public_listeners;

pub mod private_session;

pub mod position_snapshots;
pub mod public_dispatch;

pub mod public_classes;

pub mod class_jobs;

pub mod private_classes;

pub mod class_flags;

pub mod ticker_capture;

pub mod class_decode;
pub mod exchange_classes;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native_host;
