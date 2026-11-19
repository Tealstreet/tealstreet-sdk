//! Structured command and lifecycle failures. `message` is the user-facing
//! text hosts have always shown; the other fields let consumers classify a
//! failure without matching that text.
#[cfg(feature = "bingx-write-diagnostics")]
#[path = "error/bingx_write_shape.rs"]
pub mod bingx_write_shape;

use serde::Serialize;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(serde::Deserialize))]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Rejected,
    InvalidInput,
    InsufficientBalance,
    AuthInvalid,
    AuthExpired,
    AuthPermission,
    RateLimited,
    Network,
    Timeout,
    VenueUnavailable,
    Unsupported,
    Internal,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rejected => "rejected",
            Self::InvalidInput => "invalidInput",
            Self::InsufficientBalance => "insufficientBalance",
            Self::AuthInvalid => "authInvalid",
            Self::AuthExpired => "authExpired",
            Self::AuthPermission => "authPermission",
            Self::RateLimited => "rateLimited",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::VenueUnavailable => "venueUnavailable",
            Self::Unsupported => "unsupported",
            Self::Internal => "internal",
        }
    }
}

/// Whether a write reached the venue. `None` on [`VenueError`] means the site
/// has not classified it, never that it is safe to resend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(serde::Deserialize))]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    NotSent,
    Rejected,
    Unknown,
}

/// Exact native order identities retained for partial or uncertain commands.
/// With Unknown, identities are reconciliation evidence, not proof of attributed
/// caller-leg success. Codecs unable to map rows to caller orders leave counts unset.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Partial {
    pub order_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_index: Option<usize>,
}

/// Definitive builder approval refusal; unknown/failed checks never carry this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuilderAuthorizationReason {
    Unauthorized,
    CapTooLow,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuilderAuthorization {
    pub code: &'static str,
    pub reason: BuilderAuthorizationReason,
    pub required_raw: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_raw: Option<u32>,
}

/// Closed history failure classifications; no raw venue text enters this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCause {
    HistoryRateLimited,
    HistoryAuthInvalid,
    HistoryAuthPermission,
    HistoryInvalidParams,
    HistoryVenueError,
    HistoryHttpError,
    HistoryDecode,
    HistoryCursorStalled,
    HistoryPageLimit,
    HistoryWindowMismatch,
    HistoryNetwork,
    HistoryTimeout,
}

/// Closed native row schemas; never includes a symbol, identity or response text.
#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HistorySchema {
    BitgetClassicFills,
    BitgetUtaFills,
    BitgetClassicOrders,
    BitgetUtaOrders,
    BitgetClassicIncome,
    BitgetUtaIncome,
    BitgetPublicTrades,
    KrakenFills,
    KrakenOrders,
    KrakenIncome,
    BingxIncome,
    BingxOrders,
}

#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryDecodeReason {
    RowShape,
    Identity,
    Symbol,
    Quantity,
    FilledQuantity,
    Timestamp,
    TimestampOrder,
    Side,
    OrderStatus,
    OrderType,
    Fee,
    Market,
    Economics,
    QuantityNegative,
    QuantityZero,
    FilledQuantityNegative,
    FilledQuantityExceedsQuantity,
    QuantityDecimal,
    FilledQuantityDecimal,
    TimestampDecimal,
    LastUpdateTimestampDecimal,
    LimitPriceDecimal,
    FieldType,
    FieldValue,
    EnvelopeShape,
    EnvelopeCode,
    DataShape,
}

#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum HistoryDecodeField {
    #[serde(rename = "root")]
    Root,
    #[serde(rename = "code")]
    Code,
    #[serde(rename = "msg")]
    Message,
    #[serde(rename = "data")]
    Data,
    #[serde(rename = "data.orders")]
    DataOrders,
    #[serde(rename = "row")]
    Row,
    #[serde(rename = "row.symbol")]
    Symbol,
    #[serde(rename = "row.incomeType")]
    IncomeType,
    #[serde(rename = "row.income")]
    Income,
    #[serde(rename = "row.asset")]
    Asset,
    #[serde(rename = "row.time")]
    Time,
    #[serde(rename = "row.tranId")]
    TranId,
    #[serde(rename = "row.tradeId")]
    TradeId,
    #[serde(rename = "row.orderId")]
    OrderId,
    #[serde(rename = "row.side")]
    Side,
    #[serde(rename = "row.type")]
    OrderType,
    #[serde(rename = "row.status")]
    Status,
    #[serde(rename = "row.price")]
    Price,
    #[serde(rename = "row.avgPrice")]
    AveragePrice,
    #[serde(rename = "row.origQty")]
    Quantity,
    #[serde(rename = "row.executedQty")]
    FilledQuantity,
    #[serde(rename = "row.reduceOnly")]
    ReduceOnly,
    #[serde(rename = "row.positionSide")]
    PositionSide,
    #[serde(rename = "row.updateTime")]
    UpdateTime,
}

#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryExpectedType {
    Object,
    Array,
    Integer,
    String,
    NumberOrNumericString,
    StringOrInteger,
    BooleanOrStringOrNull,
    StringOrNull,
    StringOrIntegerOrNull,
    ArrayOrObject,
}

#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryJsonType {
    Missing,
    InvalidJson,
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryEnvelopeCode {
    Present,
    Absent,
    Unavailable,
}

#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryDecodeShape {
    pub field: HistoryDecodeField,
    pub expected: HistoryExpectedType,
    pub actual: HistoryJsonType,
    pub envelope_code: HistoryEnvelopeCode,
}

// Preserve the original two-byte optional detail in every VenueError queue.
// BingX's expected type is fixed by its field/schema, never response content.
#[cfg(feature = "history-row-diagnostics")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryDecode(std::num::NonZeroU16);

#[cfg(feature = "history-row-diagnostics")]
impl HistoryDecode {
    pub fn new(schema: HistorySchema, reason: HistoryDecodeReason) -> Self {
        Self(
            std::num::NonZeroU16::new(((schema as u16) << 5 | reason as u16) + 1)
                .expect("closed schema/reason fits below the shape tag"),
        )
    }

    pub fn with_shape(
        schema: HistorySchema,
        reason: HistoryDecodeReason,
        shape: HistoryDecodeShape,
    ) -> Option<Self> {
        use HistorySchema as S;
        if !matches!(schema, S::BingxIncome | S::BingxOrders)
            || shape.expected != Self::expected(schema, shape.field)
        {
            return None;
        }
        let reason = Self::SHAPE_REASONS.iter().position(|r| *r == reason)? as u16;
        let bits = 0x8000
            | (u16::from(schema == S::BingxOrders) << 14)
            | reason << 10
            | (shape.envelope_code as u16) << 8
            | (shape.actual as u16) << 5
            | shape.field as u16;
        std::num::NonZeroU16::new(bits).map(Self)
    }

    const SHAPE_REASONS: [HistoryDecodeReason; 9] = {
        use HistoryDecodeReason as R;
        [
            R::RowShape,
            R::FieldType,
            R::FieldValue,
            R::EnvelopeShape,
            R::EnvelopeCode,
            R::DataShape,
            R::Side,
            R::OrderType,
            R::OrderStatus,
        ]
    };

    /// The fixed BingX schema determines the expected type, never response values.
    pub fn expected(schema: HistorySchema, field: HistoryDecodeField) -> HistoryExpectedType {
        use HistoryDecodeField as F;
        use HistoryExpectedType as E;
        match field {
            F::Root | F::Row => E::Object,
            F::Code => E::Integer,
            F::Data if schema == HistorySchema::BingxOrders => E::ArrayOrObject,
            F::Data | F::DataOrders => E::Array,
            F::TranId | F::OrderId => E::StringOrInteger,
            F::TradeId => E::StringOrIntegerOrNull,
            F::ReduceOnly => E::BooleanOrStringOrNull,
            F::PositionSide => E::StringOrNull,
            F::Income
            | F::Time
            | F::Price
            | F::AveragePrice
            | F::Quantity
            | F::FilledQuantity
            | F::UpdateTime => E::NumberOrNumericString,
            F::Message
            | F::Symbol
            | F::IncomeType
            | F::Asset
            | F::Side
            | F::OrderType
            | F::Status => E::String,
        }
    }
}

#[cfg(feature = "history-row-diagnostics")]
impl HistoryDecode {
    // One concrete JSON encoder avoids duplicating this cold detail across sinks.
    #[inline(never)]
    fn wire_json(&self) -> serde_json::Result<Box<serde_json::value::RawValue>> {
        use HistoryDecodeField as F;
        use HistorySchema as H;
        const SCHEMAS: &[&str] = &[
            "bitgetClassicFills",
            "bitgetUtaFills",
            "bitgetClassicOrders",
            "bitgetUtaOrders",
            "bitgetClassicIncome",
            "bitgetUtaIncome",
            "bitgetPublicTrades",
            "krakenFills",
            "krakenOrders",
            "krakenIncome",
            "bingxIncome",
            "bingxOrders",
        ];
        const REASONS: &[&str] = &[
            "rowShape",
            "identity",
            "symbol",
            "quantity",
            "filledQuantity",
            "timestamp",
            "timestampOrder",
            "side",
            "orderStatus",
            "orderType",
            "fee",
            "market",
            "economics",
            "quantityNegative",
            "quantityZero",
            "filledQuantityNegative",
            "filledQuantityExceedsQuantity",
            "quantityDecimal",
            "filledQuantityDecimal",
            "timestampDecimal",
            "lastUpdateTimestampDecimal",
            "limitPriceDecimal",
            "fieldType",
            "fieldValue",
            "envelopeShape",
            "envelopeCode",
            "dataShape",
        ];
        const FIELDS: &[F] = &[
            F::Root,
            F::Code,
            F::Message,
            F::Data,
            F::DataOrders,
            F::Row,
            F::Symbol,
            F::IncomeType,
            F::Income,
            F::Asset,
            F::Time,
            F::TranId,
            F::TradeId,
            F::OrderId,
            F::Side,
            F::OrderType,
            F::Status,
            F::Price,
            F::AveragePrice,
            F::Quantity,
            F::FilledQuantity,
            F::ReduceOnly,
            F::PositionSide,
            F::UpdateTime,
        ];
        const PATHS: &[&str] = &[
            "root",
            "code",
            "msg",
            "data",
            "data.orders",
            "row",
            "row.symbol",
            "row.incomeType",
            "row.income",
            "row.asset",
            "row.time",
            "row.tranId",
            "row.tradeId",
            "row.orderId",
            "row.side",
            "row.type",
            "row.status",
            "row.price",
            "row.avgPrice",
            "row.origQty",
            "row.executedQty",
            "row.reduceOnly",
            "row.positionSide",
            "row.updateTime",
        ];
        const EXPECTED: &[&str] = &[
            "object",
            "array",
            "integer",
            "string",
            "numberOrNumericString",
            "stringOrInteger",
            "booleanOrStringOrNull",
            "stringOrNull",
            "stringOrIntegerOrNull",
            "arrayOrObject",
        ];
        const ACTUAL: &[&str] = &[
            "missing",
            "invalidJson",
            "null",
            "boolean",
            "number",
            "string",
            "array",
            "object",
        ];
        const CODES: &[&str] = &["present", "absent", "unavailable"];
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Shape {
            field: &'static str,
            expected: &'static str,
            actual: &'static str,
            envelope_code: &'static str,
        }
        #[derive(Serialize)]
        struct Wire {
            schema: &'static str,
            reason: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            shape: Option<Shape>,
        }
        let bits = self.0.get();
        let wire = if bits & 0x8000 == 0 {
            let bits = bits - 1;
            Wire {
                schema: SCHEMAS[(bits >> 5) as usize],
                reason: REASONS[(bits & 31) as usize],
                shape: None,
            }
        } else {
            let schema = if bits & 0x4000 == 0 {
                H::BingxIncome
            } else {
                H::BingxOrders
            };
            let field_index = (bits & 31) as usize;
            Wire {
                schema: SCHEMAS[schema as usize],
                reason: REASONS[Self::SHAPE_REASONS[((bits >> 10) & 15) as usize] as usize],
                shape: Some(Shape {
                    field: PATHS[field_index],
                    expected: EXPECTED[Self::expected(schema, FIELDS[field_index]) as usize],
                    actual: ACTUAL[((bits >> 5) & 7) as usize],
                    envelope_code: CODES[((bits >> 8) & 3) as usize],
                }),
            }
        };
        serde_json::value::to_raw_value(&wire)
    }
}

#[cfg(feature = "history-row-diagnostics")]
impl Serialize for HistoryDecode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.wire_json()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

// A thin optional pointer offsets the closed diagnostic without growing native
// error queues or Result error variants. Its JSON and string view stay unchanged.
#[cfg(feature = "bingx-write-diagnostics")]
#[derive(Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct VenueMessage(Box<MessageText>);
#[cfg(feature = "bingx-write-diagnostics")]
#[derive(Clone, PartialEq, Serialize)]
#[serde(transparent)]
struct MessageText {
    text: String,
}
#[cfg(feature = "bingx-write-diagnostics")]
impl std::ops::Deref for VenueMessage {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0.text
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl VenueMessage {
    pub fn into_string(self) -> String {
        self.0.text
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl AsRef<str> for VenueMessage {
    fn as_ref(&self) -> &str {
        &self.0.text
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl fmt::Display for VenueMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.text, f)
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl fmt::Debug for VenueMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0.text, f)
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl From<String> for VenueMessage {
    fn from(value: String) -> Self {
        Self(Box::new(MessageText {
            text: value.into_boxed_str().into_string(),
        }))
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl From<&str> for VenueMessage {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}
#[cfg(feature = "bingx-write-diagnostics")]
impl From<Box<str>> for VenueMessage {
    fn from(value: Box<str>) -> Self {
        value.into_string().into()
    }
}

#[cfg(not(feature = "bingx-write-diagnostics"))]
pub type VenueMessage = Box<str>;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VenueError {
    #[cfg(feature = "bingx-write-diagnostics")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bingx_write_shape: Option<bingx_write_shape::BingxWriteShape>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ErrorKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venue_code: Option<String>,
    /// Closed, SDK-owned diagnostic classification, distinct from the venue's code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause_code: Option<ErrorCause>,
    #[cfg(feature = "history-row-diagnostics")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_decode: Option<HistoryDecode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venue_message: Option<VenueMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<Box<Partial>>,
    /// Indexed outcomes recorded by the venue's existing write path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements: Option<Box<crate::write_plan::ElementOutcomes>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_authorization: Option<Box<BuilderAuthorization>>,
    /// A stop or take-profit edit whose replacement was refused after its cancel landed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protection: Option<Protection>,
    /// A command's fallback whose words describe its outcome better than its first
    /// failed leg, without claiming `protection` (a put-back of unknown outcome).
    #[serde(skip)]
    pub presents: bool,
}

/// What a refused protective replacement left the position with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(serde::Deserialize))]
#[serde(rename_all = "camelCase")]
pub enum Protection {
    /// The old order was placed again, unchanged; only its id differs.
    Restored,
    /// Placing the old order again failed too: the position has no such order.
    Unprotected,
}

impl VenueError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::from_message(message.into())
    }

    // Out of line so call sites share one struct initialisation (wasm size budget).
    #[inline(never)]
    fn from_message(message: String) -> Self {
        Self {
            message,
            ..Self::default()
        }
    }

    pub fn builder_authorization(mut self, detail: BuilderAuthorization) -> Self {
        self.builder_authorization = Some(Box::new(detail));
        self
    }

    pub fn kind(mut self, kind: ErrorKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = Some(outcome);
        self
    }

    pub fn venue_code(mut self, code: impl Into<String>) -> Self {
        self.venue_code = Some(code.into());
        self
    }

    pub fn cause_code(mut self, code: ErrorCause) -> Self {
        self.cause_code = Some(code);
        self
    }

    #[cfg(feature = "history-row-diagnostics")]
    pub fn history_decode(mut self, schema: HistorySchema, reason: HistoryDecodeReason) -> Self {
        self.kind = Some(ErrorKind::Internal);
        self.cause_code = Some(ErrorCause::HistoryDecode);
        self.history_decode = Some(HistoryDecode::new(schema, reason));
        self
    }

    pub fn venue_message(mut self, message: impl Into<String>) -> Self {
        self.venue_message = Some(VenueMessage::from(message.into()));
        self
    }

    /// A non-finite delay is dropped rather than serialized as `null`.
    pub fn retry_after_ms(mut self, ms: f64) -> Self {
        self.retry_after_ms = ms.is_finite().then_some(ms);
        self
    }

    /// Records accepted orders; an empty list leaves the error whole.
    pub fn partial(self, order_ids: &[String], total: usize, failed_index: usize) -> Self {
        self.partial_with(order_ids, Some(total), Some(failed_index))
    }

    /// Records accepted orders whose position in the caller's batch is unknown.
    pub fn partial_ids(self, order_ids: &[String]) -> Self {
        self.partial_with(order_ids, None, None)
    }

    fn partial_with(
        mut self,
        order_ids: &[String],
        total: Option<usize>,
        failed_index: Option<usize>,
    ) -> Self {
        if !order_ids.is_empty() {
            self.partial = Some(Box::new(Partial {
                order_ids: order_ids.to_vec(),
                total,
                failed_index,
            }));
        }
        self
    }

    /// The `error` string hosts send today: the message, or for a partial
    /// batch the JSON object `host.mjs` unpacks into `partialOrderIds`.
    pub fn legacy_text(&self) -> String {
        let Some(partial) = &self.partial else {
            return self.message.clone();
        };
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Legacy<'a> {
            message: &'a str,
            partial_order_ids: &'a [String],
            partial_order_count: usize,
            #[serde(skip_serializing_if = "Option::is_none")]
            total_order_count: Option<usize>,
            #[serde(skip_serializing_if = "Option::is_none")]
            failed_order_index: Option<usize>,
        }
        serde_json::to_string(&Legacy {
            message: &self.message,
            partial_order_ids: &partial.order_ids,
            partial_order_count: partial.order_ids.len(),
            total_order_count: partial.total,
            failed_order_index: partial.failed_index,
        })
        .unwrap_or_else(|_| self.message.clone())
    }
}

/// Local operation failure: this call dispatched nothing new. `NotSent` never proves
/// the whole command unsent; the mutation journal overrides it from earlier
/// releases, and an earlier released read may still have reached the venue.
impl crate::request_context::Error {
    pub fn kind(self) -> ErrorKind {
        match self {
            Self::TimedOut => ErrorKind::Timeout,
            Self::Invalid | Self::Revoked | Self::Duplicate | Self::RetryForbidden => {
                ErrorKind::InvalidInput
            }
            Self::Capacity | Self::State | Self::Stale => ErrorKind::Internal,
        }
    }

    pub fn venue_error(self) -> VenueError {
        VenueError::new(self.to_string())
            .kind(self.kind())
            .outcome(Outcome::NotSent)
    }

    /// Read admission failures carry no financial outcome.
    pub fn read_venue_error(self) -> VenueError {
        VenueError::new(self.to_string()).kind(self.kind())
    }
}

impl fmt::Display for crate::request_context::Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::request_context::Error;
        f.write_str(match self {
            Error::Invalid => "This request couldn’t be prepared",
            Error::Capacity => "The account’s request queue is full. Try again shortly.",
            Error::Duplicate => "Request is already in flight",
            Error::State => "This request isn’t ready to send",
            Error::TimedOut => "This request timed out",
            Error::Stale => "This request is out of date",
            Error::Revoked => "This request was interrupted",
            Error::RetryForbidden => "This request can’t be sent again",
        })
    }
}

impl From<String> for VenueError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for VenueError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<&String> for VenueError {
    fn from(message: &String) -> Self {
        Self::new(message.as_str())
    }
}

/// What a refusal naming the API key's IP whitelist tells the user to do.
pub const IP_WHITELIST_HINT: &str = "The API key's IP whitelist does not include this server: add the new Tealstreet IPs to the key on the exchange";
/// For a key refusal whose text leaves the cause open (bad key, IP or permissions).
pub const IP_WHITELIST_MAYBE_HINT: &str =
    "If the API key has an IP whitelist, add the new Tealstreet IPs to it";
const IP_WHITELIST_WORDS: [&str; 10] = [
    "whitelist",
    "white list",
    "allowlist",
    "allow list",
    "bound ip",
    "unmatched ip",
    "ip not allowed",
    "ip is not allowed",
    "ip restriction",
    "invalid ip",
];

/// How plainly `text` blames the API key's IP whitelist, if it does.
fn ip_whitelist_refusal(text: &str, auth_kind: bool) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    // The site already says it (Bybit's own hint, GRVT, Bitget's WS login).
    if lower.contains("tealstreet ip") {
        return None;
    }
    if !lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == "ip")
    {
        return None;
    }
    let named = IP_WHITELIST_WORDS.iter().any(|word| lower.contains(word));
    // "Bad API key, API secret or incorrect IP whitelist" names no single cause.
    if named && !lower.contains(" or ") {
        Some(IP_WHITELIST_HINT)
    } else if named || auth_kind {
        Some(IP_WHITELIST_MAYBE_HINT)
    } else {
        None
    }
}

fn is_auth(kind: Option<ErrorKind>) -> bool {
    matches!(
        kind,
        Some(ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission)
    )
}

fn with_hint(message: &str, hint: &str) -> String {
    let message = message.trim_end().trim_end_matches('.');
    format!("{message}. {hint}")
}

/// `message` with the IP whitelist hint when it blames the key's IP whitelist,
/// and whether it names that as the cause, for a notice that has only text.
pub fn ip_whitelist_text(message: &str, kind: Option<ErrorKind>) -> Option<(String, bool)> {
    let hint = ip_whitelist_refusal(message, is_auth(kind))?;
    Some((with_hint(message, hint), hint == IP_WHITELIST_HINT))
}

impl VenueError {
    /// The venue's code for an IP the key's whitelist excludes, whatever its text says.
    pub fn ip_whitelist_refused(mut self) -> Self {
        self.kind = Some(ErrorKind::AuthPermission);
        if !self.message.to_ascii_lowercase().contains("tealstreet ip") {
            self.message = with_hint(&self.message, IP_WHITELIST_HINT);
        }
        self
    }

    /// A refusal blaming the key's IP whitelist says to add the new IPs, and
    /// one that names it as the cause is an auth permission refusal, whatever
    /// the site classified: retrying cannot pass it.
    pub fn with_ip_whitelist_hint(mut self) -> Self {
        let auth = is_auth(self.kind);
        let hint = ip_whitelist_refusal(&self.message, auth).or_else(|| {
            self.venue_message
                .as_deref()
                .and_then(|venue| ip_whitelist_refusal(venue, auth))
                .filter(|_| !self.message.to_ascii_lowercase().contains("tealstreet ip"))
        });
        if let Some(hint) = hint {
            if hint == IP_WHITELIST_HINT && !auth {
                self.kind = Some(ErrorKind::AuthPermission);
            }
            self.message = with_hint(&self.message, hint);
        }
        self
    }
}

impl fmt::Display for VenueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for VenueError {}

/// What a non-stop order's edit cost: its cancel landed and nothing replaced it.
pub fn order_gone_text(symbol: &str, unknown: bool) -> String {
    if unknown {
        format!(
            "Your {symbol} order was cancelled, but its replacement wasn’t confirmed. Check your open orders before trying again."
        )
    } else {
        format!("Your {symbol} order was cancelled and its replacement was declined")
    }
}

/// What a stop's edit cost: its cancel landed and nothing replaced it. One text for
/// every venue, because a vanished stop is what the user must act on.
pub fn stop_gone_text(
    symbol: &str,
    side: crate::model::OrderSide,
    kind: crate::model::OrderType,
    unknown: bool,
) -> String {
    use crate::model::{OrderSide, OrderType};
    let position = match side {
        OrderSide::Sell => "long",
        OrderSide::Buy => "short",
    };
    let order = if kind == OrderType::TakeProfit {
        "take profit"
    } else {
        "stop"
    };
    if unknown {
        format!(
            "Your {symbol} {position} position’s {order} was cancelled, but its replacement wasn’t confirmed. Check your open orders before adding protection."
        )
    } else {
        format!(
            "Your {symbol} {position} position has no {order}: the previous {order} was cancelled and its replacement was declined"
        )
    }
}

// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
#[cfg(test)]
mod tests {
    use super::*;

    /// Venue refusals naming the key's IP whitelist, as the venues word them.
    const NAMED: [&str; 9] = [
        "Unmatched IP, please check your API key's bound IP addresses.",
        "Your IP is not included in your API key's IP whitelist",
        "Current IP does not match the bound IP",
        "Invalid IP",
        "Ip not in whitelist",
        "Request IP is not whitelisted",
        "The IP address is not in the API whitelist",
        "IP_ILLEGAL: IP address not whitelisted",
        "403 IP is not whitelisted",
    ];

    #[test]
    fn a_refusal_naming_the_ip_whitelist_says_to_add_the_new_ips() {
        for text in NAMED {
            let error = VenueError::new(format!("Venue rejected the order: {text}"))
                .kind(ErrorKind::Rejected)
                .with_ip_whitelist_hint();
            assert!(
                error.message.ends_with(IP_WHITELIST_HINT),
                "{}",
                error.message
            );
            assert!(
                error.message.contains(text.trim_end_matches('.')),
                "{}",
                error.message
            );
            assert_eq!(error.kind, Some(ErrorKind::AuthPermission), "{text}");
        }
    }

    #[test]
    fn a_key_refusal_that_leaves_the_cause_open_only_suggests_the_whitelist() {
        for text in [
            "Invalid API-key, IP, or permissions for action.",
            "Bad API key, API secret or incorrect IP whitelist.",
        ] {
            let error = VenueError::new(text)
                .kind(ErrorKind::AuthInvalid)
                .with_ip_whitelist_hint();
            assert!(
                error.message.ends_with(IP_WHITELIST_MAYBE_HINT),
                "{}",
                error.message
            );
            assert_eq!(error.kind, Some(ErrorKind::AuthInvalid), "{text}");
        }
    }

    #[test]
    fn other_failures_and_a_site_that_already_says_it_are_left_alone() {
        for (text, kind) in [
            (
                "Way too much request weight used; IP banned until 1700000000000",
                ErrorKind::RateLimited,
            ),
            ("Too many requests from this IP", ErrorKind::RateLimited),
            ("Order price 10010.5 out of range", ErrorKind::Rejected),
            ("API key is invalid.", ErrorKind::AuthInvalid),
            (
                "Bybit refused this API key from this IP address: the key is restricted to other IPs. Add the current Tealstreet IPs to the key's IP allowlist on Bybit, or create a new key",
                ErrorKind::AuthPermission,
            ),
        ] {
            let error = VenueError::new(text).kind(kind).with_ip_whitelist_hint();
            assert_eq!(error.message, text);
            assert_eq!(error.kind, Some(kind));
        }
    }

    #[test]
    fn the_venue_message_counts_and_the_hint_is_added_once() {
        let error = VenueError::new("Bitunix account read failed: code 10004")
            .venue_message("The current ip is not in the apikey ip whitelist")
            .with_ip_whitelist_hint();
        assert!(
            error.message.ends_with(IP_WHITELIST_HINT),
            "{}",
            error.message
        );
        assert_eq!(error.clone().with_ip_whitelist_hint(), error);
        let coded = VenueError::new("KuCoin rejected the order (400006)").ip_whitelist_refused();
        assert_eq!(coded.kind, Some(ErrorKind::AuthPermission));
        assert_eq!(coded.clone().ip_whitelist_refused(), coded);
        assert_eq!(coded.clone().with_ip_whitelist_hint(), coded);
    }

    #[test]
    fn builder_approval_details_are_optional_and_keep_the_legacy_text() {
        let error = VenueError::new("Builder cap too low")
            .builder_authorization(BuilderAuthorization {
                code: "GRVT_BUILDER_UNAUTHORIZED",
                reason: BuilderAuthorizationReason::CapTooLow,
                required_raw: 300,
                authorized_raw: Some(100),
            })
            .kind(ErrorKind::InvalidInput)
            .outcome(Outcome::NotSent);
        assert_eq!(error.legacy_text(), "Builder cap too low");
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"Builder cap too low","kind":"invalidInput","outcome":"notSent","builderAuthorization":{"code":"GRVT_BUILDER_UNAUTHORIZED","reason":"cap-too-low","requiredRaw":300,"authorizedRaw":100}}"#
        );
    }

    #[test]
    fn plain_errors_keep_their_text_and_serialize_only_the_message() {
        let error = VenueError::from("Order would trigger immediately");
        assert_eq!(error.legacy_text(), "Order would trigger immediately");
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"Order would trigger immediately"}"#
        );
    }

    #[test]
    fn partial_batches_keep_the_legacy_json_text_byte_for_byte() {
        let ids = ["a".to_owned(), "b".to_owned()];
        let error = VenueError::new("leg 3 rejected").partial(&ids, 4, 2);
        assert_eq!(
            error.legacy_text(),
            r#"{"message":"leg 3 rejected","partialOrderIds":["a","b"],"partialOrderCount":2,"totalOrderCount":4,"failedOrderIndex":2}"#
        );
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"leg 3 rejected","partial":{"orderIds":["a","b"],"total":4,"failedIndex":2}}"#
        );
    }

    #[test]
    fn unmapped_partials_omit_the_counts_they_cannot_know() {
        let ids = ["a".to_owned()];
        let error = VenueError::new("leg rejected").partial_ids(&ids);
        assert_eq!(
            error.legacy_text(),
            r#"{"message":"leg rejected","partialOrderIds":["a"],"partialOrderCount":1}"#
        );
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"leg rejected","partial":{"orderIds":["a"]}}"#
        );
    }

    #[test]
    fn an_empty_partial_is_a_whole_failure() {
        let error = VenueError::new("rejected").partial(&[], 3, 0);
        assert_eq!(error.partial, None);
        assert_eq!(error.legacy_text(), "rejected");
    }

    #[test]
    fn a_non_finite_retry_delay_is_left_out() {
        let error = VenueError::new("busy").retry_after_ms(f64::NAN);
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"busy"}"#
        );
    }

    #[test]
    fn request_context_refusals_are_typed_not_sent_failures() {
        use crate::request_context::Error;
        let error = Error::TimedOut.venue_error();
        assert_eq!(error.message, "This request timed out");
        assert_eq!(error.kind, Some(ErrorKind::Timeout));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        for (reason, kind) in [
            (Error::Invalid, ErrorKind::InvalidInput),
            (Error::Revoked, ErrorKind::InvalidInput),
            (Error::Duplicate, ErrorKind::InvalidInput),
            (Error::RetryForbidden, ErrorKind::InvalidInput),
            (Error::Capacity, ErrorKind::Internal),
            (Error::State, ErrorKind::Internal),
            (Error::Stale, ErrorKind::Internal),
        ] {
            let error = reason.venue_error();
            assert_eq!(error.kind, Some(kind), "{reason:?}");
            assert_eq!(error.outcome, Some(Outcome::NotSent), "{reason:?}");
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn read_admission_refusals_keep_kind_and_text_without_financial_outcome() {
        use crate::request_context::Error;
        for reason in [
            Error::TimedOut,
            Error::Invalid,
            Error::Revoked,
            Error::Duplicate,
            Error::RetryForbidden,
            Error::Capacity,
            Error::State,
            Error::Stale,
        ] {
            let read = reason.read_venue_error();
            let generic = reason.venue_error();
            assert_eq!(read.message, generic.message);
            assert_eq!(read.kind, generic.kind);
            assert_eq!(read.outcome, None);
            assert_eq!(generic.outcome, Some(Outcome::NotSent));
        }
    }

    #[test]
    fn classified_fields_serialize_in_camel_case() {
        let error = VenueError::new("Too many requests")
            .kind(ErrorKind::RateLimited)
            .outcome(Outcome::NotSent)
            .venue_code("10006")
            .venue_message("Too many visits")
            .retry_after_ms(1000.0);
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"message":"Too many requests","kind":"rateLimited","outcome":"notSent","venueCode":"10006","venueMessage":"Too many visits","retryAfterMs":1000.0}"#
        );
    }

    #[test]
    fn a_closed_cause_serializes_separately_from_the_venue_code() {
        let error = VenueError::new("read refused")
            .kind(ErrorKind::RateLimited)
            .cause_code(ErrorCause::HistoryRateLimited)
            .venue_code("100410");
        let json = serde_json::to_value(error).unwrap();
        assert_eq!(json["causeCode"], "HISTORY_RATE_LIMITED");
        assert_eq!(json["venueCode"], "100410");
        assert!(std::mem::size_of::<VenueError>() < 128);
    }

    #[test]
    fn a_vanished_order_names_its_symbol() {
        assert_eq!(
            super::order_gone_text("BTCUSDT", false),
            "Your BTCUSDT order was cancelled and its replacement was declined"
        );
        assert!(
            super::order_gone_text("ETHUSDT", true)
                .contains("Your ETHUSDT order was cancelled, but its replacement wasn’t confirmed")
        );
    }

    #[test]
    fn a_vanished_stop_names_its_symbol_and_position() {
        use crate::model::{OrderSide, OrderType};
        assert_eq!(
            super::stop_gone_text("BTCUSDT", OrderSide::Sell, OrderType::StopLoss, false),
            "Your BTCUSDT long position has no stop: the previous stop was cancelled and its replacement was declined"
        );
        assert!(
            super::stop_gone_text("ETHUSDT", OrderSide::Buy, OrderType::TakeProfit, true)
                .starts_with("Your ETHUSDT short position’s take profit was cancelled, but its replacement wasn’t confirmed")
        );
    }
}

#[cfg(all(test, feature = "bingx-write-diagnostics"))]
mod venue_message_tests {
    use super::*;
    #[test]
    fn thin_message_keeps_wire_text_and_discards_spare_capacity() {
        let mut text = String::with_capacity(65_536);
        text.push_str("fixture venue message");
        let message = VenueMessage::from(text);
        assert_eq!(message.0.text.capacity(), message.len());
        assert_eq!(message.as_ref(), "fixture venue message");
        assert_eq!(
            serde_json::to_string(&message).unwrap(),
            r#""fixture venue message""#
        );
        assert_eq!(format!("{message:?}"), r#""fixture venue message""#);
        assert_eq!(message.clone().into_string(), "fixture venue message");
        assert_eq!(
            std::mem::size_of::<Option<VenueMessage>>(),
            std::mem::size_of::<usize>()
        );
        #[cfg(target_pointer_width = "64")]
        assert!(std::mem::size_of::<(Box<()>, VenueError)>() < 128);
    }
}
