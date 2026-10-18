#![doc = include_str!("../CLAUDE.md")]
//! Shared exchange orchestration; behavioral sources are linked from CLAUDE.md.
pub mod build_records;
pub mod capabilities;
pub mod client_order_id;
pub mod delivery_timers;
pub mod eager;
pub mod error_reporting;
pub mod exchange;
pub mod external;
pub mod fold_position_adapter;
#[cfg(feature = "health")]
pub mod health;
pub mod http;
pub mod income_paging;
pub mod journal;
pub mod js_runtime;
pub use js_runtime::native_store;
pub mod lifecycle;
pub mod message_stats;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
pub mod order_operations;
pub mod position_intervals;
pub mod proxy_token;
pub mod runtime;
pub mod signing;
pub mod snapshots;
pub mod socket_lifecycle;
pub mod store_delivery;
pub mod store_payload;
pub mod throttle;
pub mod websocket;
pub mod worker;
pub mod ws_parent;

// Public host traits must be implementable without reaching through this crate
// merely to name the transport types appearing in their signatures.
pub use safe_cex_transport::{provider::TransportProvider, socket::TunnelHost};

// Public signer hooks must be nameable by hosts depending on exchange core alone.
pub use safe_cex_transport::signing::{
    HttpSigningRequest, HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse,
    SignedConfig, SigningConfig,
};
pub use safe_cex_types::ws_auth::{WsAuth, WsAuthRequest, WsAuthResponse};

mod subscribed_candle;
mod waiters;

mod owned_tasks;
pub use owned_tasks::{OwnedOperation, start_detached_timeout};

mod shared_operation;
pub use shared_operation::SharedOperation;

pub mod access_token;

pub mod closed_positions;
pub mod loop_util;

pub mod cross_trade;

pub mod fill_event_units;
pub mod how_facade;
