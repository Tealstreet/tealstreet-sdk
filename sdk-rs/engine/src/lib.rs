//! Sans-I/O exchange engine core. See docs/plans/RUST_REWRITE.md.
pub mod backoff;
pub mod book;
pub mod book_output;
pub mod candle_output;
pub mod capabilities;
pub mod classify;
pub mod command;
mod control_frame;
pub mod coverage;
pub mod credential_selection;
pub mod credentials;
pub mod de;
pub mod engine;
pub mod entropy;
pub mod error;
pub mod execution;
pub mod execution_group;
pub mod fence;
pub mod fresh_positions;
pub mod history;
pub mod history_output;
mod id_map;
pub(crate) use id_map::IdMap;
mod balance_output;
mod fill_output;
pub mod json;
pub mod log;
mod market_output;
#[cfg(feature = "metadata-freshness")]
pub mod metadata;
pub mod model;
pub mod nonce;
pub mod num;
#[cfg(feature = "order-locks")]
mod order_locks;
mod order_output;
pub mod order_read_cover;
pub mod paging;
mod placement_acks;
mod position_output;
pub mod protective_amend;
mod protocol_clock;
pub mod public_feed;
pub mod read_scheduler;
#[cfg(feature = "repair-mutations")]
pub mod repair;
pub mod request_budget;
pub mod request_context;
pub mod resource_ids;
pub mod selected_read;
#[cfg(feature = "sign-meta")]
pub mod sign_meta;
pub mod signing;
pub mod signing_capture;
mod signing_dispatch;
pub mod signing_headers;
pub mod signing_policy;
mod signing_release;
mod store_output;
pub mod text;
pub use signing_dispatch::{DispatchContext, SignStep};
pub use signing_release::{SigningRelease, StampFloor};
pub mod frame_fed;
pub mod ingest;
pub mod setting_holds;
pub mod socket;
#[cfg(feature = "managed-sockets")]
mod socket_driver;
#[cfg(feature = "managed-sockets")]
pub mod socket_lifecycle;
pub mod startup;
mod startup_causes;
pub mod store;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod time;
pub mod url;
pub mod wire;
pub mod write_plan;

pub use capabilities::{
    AccountLookup, CancelAll, Capabilities, ClosedPositionFormat, FillHistoryTime, LeverageQuery,
    MaxLeverage, PositionHistory, QuantityUnits,
};
pub use control_frame::ControlFrame;
pub use credentials::{READ_CREDENTIALS_INVALID, ReadCredentials};
pub use engine::{
    AccountReadScope, Action, AuthOutcome, Command, CommandId, CommandWait, CredentialSlot, Cx,
    DEFAULT_COMMAND_WAIT_MS, Engine, EngineUnknownOutcome, Event, Fill, FillFinancials,
    HttpReadClass, HttpRelease, HttpRequest, MAX_COMMAND_WAIT_MS, MergedScopeAuth, Method, Notice,
    PollTimerBounds, Query, ROUTE_EXHAUSTED_CLOSE, RequestId, RequestRoute, Response,
    SigningTarget, SocketChannel, SocketContext, SocketId, SocketSpec, SubscriptionTopic, TimerId,
    UNKNOWN_HTTP_OUTCOME_TEXT, UNKNOWN_OUTCOME_TEXT, UnknownOutcomeTelemetry, Venue, VenueAction,
    WebSocketCommit, WebSocketHandoff, WebSocketWrite, WebSocketWriteBinding,
    unsupported_venue_action,
};
pub use error::{
    BuilderAuthorization, BuilderAuthorizationReason, ErrorCause, ErrorKind, Outcome, Partial,
    VenueError,
};
#[cfg(feature = "history-row-diagnostics")]
pub use error::{HistoryDecode, HistoryDecodeReason, HistorySchema};
pub use log::{Health, Level, LogEvent, Redacted, Value};
pub use model::{OrderIdentityKind, OrderIdentityReceipt};
pub use store::{OrderSnapshotToken, Store};

pub use public_feed::{PublicFeedPolicy, PublicFeedUpdate};

#[cfg(feature = "address-nonce")]
pub use engine::{AddressSetupCleanup, AddressSetupProgress, SetupHostSupport};
pub use engine::{AddressSetupSlots, SetupRetirement};
