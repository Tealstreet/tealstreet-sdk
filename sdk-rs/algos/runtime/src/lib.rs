//! The shared runtime for server-side algos (chaser, TWAP, exec).
//!
//! Every algo of an account runs on that account's `LocalSet`, so state is
//! `!Send` and per-account work is serial by construction. The server adapts its
//! account handle to [`AccountPort`]; algos see only this crate's surface.
#![cfg(not(target_arch = "wasm32"))]

pub mod executor;
#[cfg(feature = "host-executor")]
pub mod host_executor;
mod port;
mod registry;
mod scope;
mod store;

pub use port::{AccountPort, Change, CommandFuture, amend_command, cancel_command};
pub use registry::{AlgoFactory, Registry};
pub use scope::{AlgoCx, Scope};
pub use store::{AlgoStore, PrivateSocketHealth};
