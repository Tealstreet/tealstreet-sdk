//! KuCoin futures and spot venue family.
pub mod account_mode;
pub mod broker;
pub mod sizing;
pub mod spot_types;
pub mod types;

pub mod api_helpers;

pub mod ws_helpers;

pub mod spot_helpers;

pub mod api;

pub mod spot_topics;

pub mod message_routing;
pub mod position_metadata;
pub mod private_dispose;
pub mod private_refresh;
pub mod private_socket;
pub mod public_lifecycle;
pub mod public_subscriptions;
pub mod socket_setup;
pub mod ws_ping;

pub mod socket_boundaries;

pub mod rest_reads;

pub mod public_streams;

pub mod order_control;

pub mod bootstrap_reads;

pub mod spot_bodies;

pub mod order_lifecycle;

pub mod class_construction;

pub mod spot_private_class;

pub mod futures_private_class;

pub mod spot_public_class;

pub mod futures_public_class;

pub mod spot_native_accounts;

pub mod spot_native_policy;

pub mod spot_native_order_reads;

pub mod spot_exchange_class;

pub mod futures_native_helpers;

pub mod futures_native_accounts;

pub mod futures_native_order_reads;

pub mod futures_native_formatters;
pub mod futures_native_mapping;

mod binding;

pub mod kucoin_exchange;
