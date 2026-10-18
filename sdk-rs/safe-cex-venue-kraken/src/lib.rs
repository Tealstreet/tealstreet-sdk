//! Kraken Futures and Spot source ports, bound to the Exchange trait by `binding`.
#[cfg(test)]
extern crate self as safe_cex_venue_kraken;
pub mod feed_stats;
pub mod fills;
pub mod futures_candles;
pub mod helpers;
pub mod income;
pub mod lifecycle;
pub mod orders;
pub mod periodic_sync;
pub mod private_auth;
pub mod public_subscriptions;
pub mod reads;
pub mod spot_balance;
pub mod spot_cancel;
pub mod spot_dispatch;
pub mod spot_reads;
pub mod spot_types;
pub mod startup;
pub mod stream_adapters;
pub mod stream_updates;
pub mod tickers;
pub mod types;
pub mod validation;
mod wire;
pub mod ws_helpers;

pub mod message_dispatch;
pub mod private_control;

pub mod book_listeners;
pub mod orderbooks;

pub mod rest_account;

pub mod trading;

pub mod api;

pub mod private_accounts;

pub mod construction;
pub mod ohlcv_polling;

pub mod classes;

pub mod binding;
pub use binding::exchange::{KrakenDependencies, KrakenExchange, spot_client_ids};

#[cfg(test)]
#[path = "../tests/support/accepted_record_shapes.rs"]
mod accepted_record_shapes_support;
