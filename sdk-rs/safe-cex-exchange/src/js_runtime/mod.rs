//! JS-semantics host runtime for host-trait-generic venue ports: values,
//! promises, builtins, an Axios-shaped client, a BaseWebSocket bridge and a
//! Store facade. Venue crates keep their own class assembly and host impls.
pub mod base_exchange;
pub mod builtins;
pub mod client;
pub mod globals;
pub mod library;
pub mod promise;
pub mod scope;
pub mod sockets;
pub mod store;
pub mod value;
