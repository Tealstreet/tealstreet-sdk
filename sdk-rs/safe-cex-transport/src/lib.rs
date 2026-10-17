//! Transport contracts and snapshot-generated routing.
//! Behavioral guidance: packages/safe-cex/CLAUDE.md, Proxy & Tunnel Architecture.
pub mod direct;
pub mod provider;
pub mod registry;
pub mod routing;
pub use direct::DirectTransport;
pub use provider::{
    NetworkFailure, PrivateProxy, RestAdapter, RestAdapterContext, RestAdapterRegion,
    RestAdapterVariant, RestAuthorizer, RestBody, RestExecution, RestFailure, RestFuture,
    RestRequest, RestResponse, RestResponseBody, RestTransport, RestTransportContext,
    TransportError, TransportFuture, TransportProvider, WsConnection,
};
pub use registry::TransportRegistry;
pub use routing::{
    HowEgress, ResolvedRouting, ResolvedWs, RoutingOptions, resolve_exchange_routing,
};
pub mod codec;
pub mod fallback;
#[path = "../../../infra/tradetunnel/src/frames.rs"]
mod frames;
pub mod how;
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub mod native;
pub mod relay;
pub mod socket;

pub mod polling;
pub mod rate_limit;
pub mod retry;
pub mod secondary;
pub mod signature_rejection;
pub mod signing;

pub mod how_config;
pub mod tunnel_endpoints;

pub mod how_diagnostics;

pub mod how_context;

pub mod query;

pub mod query_signing;
pub mod tealstreet;
pub mod tealstreet_time_source;
pub mod transport_resolution;
pub mod ws_relay;

#[cfg(test)]
mod accepted_frame_shapes;

mod native_data;
