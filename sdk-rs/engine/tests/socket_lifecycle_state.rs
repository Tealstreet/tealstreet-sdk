//! Staged engine state tests. Runtime wiring waits for the shared safe-ID core.
pub use safe_cex_engine::{Cx, ErrorKind, SocketChannel, SocketId, TimerId, VenueError, socket};
#[path = "../src/socket_driver.rs"]
pub mod socket_driver;
#[path = "../src/socket_lifecycle.rs"]
pub mod socket_lifecycle;
