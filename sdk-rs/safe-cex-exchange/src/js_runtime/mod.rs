//! JS-semantics host runtime for host-trait-generic venue ports: values,
//! promises, builtins, an Axios-shaped client, a BaseWebSocket bridge and a
//! Store facade. Venue crates keep their own class assembly and host impls.
pub mod builtins;
pub mod client;
pub mod promise;
pub mod sockets;
pub mod store;
pub mod value;
