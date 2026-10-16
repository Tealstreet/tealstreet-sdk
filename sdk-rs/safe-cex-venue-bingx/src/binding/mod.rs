//! Production assembly over the shared JS HTTP, socket and intrinsic runtime.
mod exchange;
mod hosts;
mod http;
mod sockets;
pub use exchange::{BingxDependencies, BingxExchange};
