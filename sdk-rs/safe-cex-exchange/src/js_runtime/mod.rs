//! JS-semantics host runtime for host-trait-generic venue ports: values,
//! promises, builtins, an Axios-shaped client, a BaseWebSocket bridge and a
//! Store facade. Venue crates keep their own class assembly and host impls.
pub mod base_exchange;
pub mod builtins;
mod ccxt;
pub mod client;
pub mod globals;
pub mod iterator;
pub(crate) mod jobs;
pub mod library;
mod number_radix;
pub mod ops;
pub mod promise;
pub mod regexp;
pub mod scope;
pub mod sockets;
mod sorting;
pub mod store;
mod throttle;
pub mod value;

pub mod collections;

#[cfg(test)]
mod iterator_record_tests;
