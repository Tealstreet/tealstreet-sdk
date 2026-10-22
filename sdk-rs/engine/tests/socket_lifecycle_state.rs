//! Staged engine state tests. Runtime wiring waits for the shared safe-ID core.
pub use safe_cex_engine::{ErrorKind, SocketChannel, SocketId, VenueError, socket};
#[path = "../src/socket_lifecycle.rs"]
pub mod staged;
