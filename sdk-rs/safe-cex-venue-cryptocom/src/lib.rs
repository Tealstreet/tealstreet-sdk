//! Crypto.com source mirrors. See the venue manifest for witnessed scope.
pub mod api;
pub mod api_envelope_body;
pub mod api_guards;
pub mod binding;
pub mod order_mappings;
pub mod private_order_mappings;
pub mod signing;
pub mod types;
pub mod wire;
pub mod ws_private;
pub mod ws_public;

pub mod public_subscriptions;

pub mod ws_payloads;

pub mod api_error;

pub mod rest_reads;

pub mod order_writes;
pub mod socket_classes;
pub mod socket_handlers;

pub mod runtime;

pub mod native_bindings;

mod class_admin;

mod class_socket;

mod class_exchange;

mod class_mappings;

pub mod public_endpoints;
pub mod signing_value;

mod wire_record;

#[cfg(test)]
mod accepted_test_support;

#[cfg(test)]
extern crate self as safe_cex_venue_cryptocom;
