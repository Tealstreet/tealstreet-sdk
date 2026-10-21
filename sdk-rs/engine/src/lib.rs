//! Sans-I/O exchange engine core. See docs/plans/RUST_REWRITE.md.
pub mod backoff;
pub mod book;
pub mod capabilities;
pub mod command;
pub mod coverage;
pub mod credentials;
pub mod de;
pub mod engine;
pub mod error;
pub mod fence;
pub mod json;
pub mod model;
pub mod num;
#[cfg(feature = "order-locks")]
mod order_locks;
pub mod paging;
mod placement_acks;
pub mod public_feed;
pub mod socket;
pub mod startup;
pub mod store;
#[cfg(feature = "test-support")]
pub mod test_support;
pub mod time;
pub mod url;
pub mod wire;

pub use capabilities::{
    AccountLookup, CancelAll, Capabilities, LeverageQuery, MaxLeverage, QuantityUnits,
};
pub use credentials::ReadCredentials;
pub use engine::{
    Action, Command, CommandId, Cx, Engine, Event, Fill, HttpRequest, LogLevel, Method, Notice,
    PayloadSignRequest, Query, RequestId, RequestRoute, Response, SignatureTarget, SocketChannel,
    SocketContext, SocketId, SocketSpec, TimerId, Venue, VenueAction, unsupported_venue_action,
};
pub use error::{ErrorKind, Outcome, Partial, VenueError};
pub use model::{OrderIdentityKind, OrderIdentityReceipt};
pub use store::Store;

pub use public_feed::{PublicFeedPolicy, PublicFeedUpdate};
