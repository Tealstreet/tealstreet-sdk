//! Bitunix USDT futures: sans-I/O public and account state machines.
mod account;
mod error;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use safe_cex_engine::startup::{DiscoveryRead, PublicRetries};
