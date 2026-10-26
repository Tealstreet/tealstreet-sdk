//! One error classifier. The engine owns precedence (transport, unproven
//! status, native code, documented status, malformed reply); venues supply
//! only a const [`CodeTable`] of native codes. Contract:
//! `docs/plans/RUST_ERROR_CLASSIFICATION.md`.
use crate::{AuthOutcome, ErrorKind, Outcome, Response, VenueError};

/// What the caller asked the venue to do with this request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Place,
    Cancel,
    Amend,
    Setting,
    Read,
}

/// Intents whose requested state can already hold. Never a placement or read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SatisfiedIntent {
    Cancel,
    Amend,
    Setting,
}

impl SatisfiedIntent {
    fn intent(self) -> Intent {
        match self {
            Self::Cancel => Intent::Cancel,
            Self::Amend => Intent::Amend,
            Self::Setting => Intent::Setting,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Proven pre-execution refusal: a write is `rejected`.
    Refused(ErrorKind),
    /// Proven refusal that permits one re-signed resend.
    RefusedResend(ErrorKind),
    /// The answer does not prove the outcome: a write is `unknown`.
    Unknown(Option<ErrorKind>),
    /// "Not open / already in that state" for one intent. Not a store transition.
    Satisfied(SatisfiedIntent),
}

impl Verdict {
    pub fn kind(self) -> Option<ErrorKind> {
        match self {
            Self::Refused(kind) | Self::RefusedResend(kind) => Some(kind),
            Self::Unknown(kind) => kind,
            Self::Satisfied(_) => None,
        }
    }

    pub fn is_refusal(self) -> bool {
        matches!(self, Self::Refused(_) | Self::RefusedResend(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Int(i64),
    /// Inclusive.
    Range(i64, i64),
    Text(&'static str),
    /// The exact native message, only for code-less venues. The venue codec
    /// strips only its documented suffix grammar before building `Native`.
    Message(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: Key,
    pub verdict: Verdict,
    pub only: Option<Intent>,
}

/// One table per venue family, shared by writes, reads, startup and WS login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeTable {
    pub rows: &'static [Row],
    /// The venue's success codes. Handed to the classifier they prove nothing,
    /// so a success code on a failure path is a malformed acknowledgement.
    pub success: &'static [Key],
    /// Documented default for codes the table does not list.
    pub unlisted: Verdict,
    /// Documented pre-execution 4xx answers without a native code, each cited.
    pub status: &'static [(u16, Verdict)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCode<'a> {
    Int(i64),
    Text(&'a str),
    /// The venue's error envelope carries no code (message-only venues).
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Native<'a> {
    pub code: NativeCode<'a>,
    pub message: &'a str,
}

/// How far a failed request got. Unreleased work uses the preparation and
/// signing refusals (`request_context::Error::venue_error`), never this.
#[derive(Clone, Copy, Debug)]
pub enum Released<'a> {
    Transport,
    Response(&'a Response<'a>),
}

/// Strict native integer: an exact decimal integer, optionally as one quoted
/// JSON string. Fractions, exponents, signs other than `-` and padding fail.
pub fn decode_int(token: &str) -> Option<i64> {
    let digits = token
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(token);
    let unsigned = digits.strip_prefix('-').unwrap_or(digits);
    let canonical = !unsigned.is_empty()
        && unsigned.bytes().all(|b| b.is_ascii_digit())
        && (unsigned == "0" || !unsigned.starts_with('0'))
        && digits != "-0";
    canonical.then(|| digits.parse().ok()).flatten()
}

impl Key {
    fn matches(self, native: &Native<'_>) -> bool {
        match (self, native.code) {
            (Self::Int(key), NativeCode::Int(code)) => key == code,
            (Self::Range(low, high), NativeCode::Int(code)) => (low..=high).contains(&code),
            (Self::Text(key), NativeCode::Text(code)) => key == code,
            (Self::Message(message), NativeCode::None) => native.message == message,
            _ => false,
        }
    }
}

impl CodeTable {
    /// Exact-intent rows first, then rows for every intent, then `unlisted`.
    pub fn verdict(&self, native: &Native<'_>, intent: Intent) -> Verdict {
        if self.success.iter().any(|key| key.matches(native)) {
            return Verdict::Unknown(Some(ErrorKind::Internal));
        }
        let verdict = self
            .row(native, intent)
            .map_or(self.unlisted, |row| row.verdict);
        scoped(verdict, intent)
    }

    fn row(&self, native: &Native<'_>, intent: Intent) -> Option<&Row> {
        let scoped = |only: Option<Intent>| {
            self.rows
                .iter()
                .find(|row| row.only == only && row.key.matches(native))
        };
        scoped(Some(intent)).or_else(|| scoped(None))
    }

    fn status_verdict(&self, status: u16) -> Option<Verdict> {
        self.status
            .iter()
            .find(|(documented, _)| *documented == status)
            .map(|(_, verdict)| *verdict)
    }
}

/// A `Satisfied` verdict only ever applies to its own intent.
fn scoped(verdict: Verdict, intent: Intent) -> Verdict {
    match verdict {
        Verdict::Satisfied(satisfied) if satisfied.intent() != intent => Verdict::Unknown(None),
        verdict => verdict,
    }
}

fn status_kind(response: &Response<'_>) -> Option<ErrorKind> {
    match response.status {
        200..=299 => Some(ErrorKind::Internal),
        401 | 403 if cdn_refusal(response) => Some(ErrorKind::RateLimited),
        401 => Some(ErrorKind::AuthInvalid),
        403 => Some(ErrorKind::AuthPermission),
        418 | 429 => Some(ErrorKind::RateLimited),
        _ => None,
    }
}

/// A CDN refusing the egress address answers before the venue sees the
/// request, so it says nothing about the key. Only CloudFront's own error-page
/// marker counts: a venue auth error may mention a CloudFront host.
fn cdn_refusal(response: &Response<'_>) -> bool {
    const SCAN: usize = 64 * 1024;
    const MARKER: &[u8] = b"generated by cloudfront (cloudfront)";
    let body = response.body.as_bytes();
    body[..body.len().min(SCAN)]
        .windows(MARKER.len())
        .any(|window| window.eq_ignore_ascii_case(MARKER))
}

/// Engine precedence, rules 2-6. Call only for a failed or unproven answer; a
/// 2xx without a native error means the success ACK was malformed.
pub fn verdict(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
) -> Verdict {
    verdict_for_delivery(table, intent, released, native, false)
}

fn verdict_for_delivery(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    status_exempt: bool,
) -> Verdict {
    let response = match released {
        Released::Transport => return Verdict::Unknown(Some(ErrorKind::Network)),
        Released::Response(response) => response,
    };
    if !status_exempt && let Some(verdict) = unproven_status(response) {
        return verdict;
    }
    // A code-less envelope that matches no message row is no native evidence.
    let native = native
        .filter(|native| native.code != NativeCode::None || table.row(native, intent).is_some());
    if let Some(native) = native {
        return table.verdict(native, intent);
    }
    match table.status_verdict(response.status) {
        Some(verdict) if !(200..300).contains(&response.status) => scoped(verdict, intent),
        _ => Verdict::Unknown(status_kind(response)),
    }
}

/// Rule 3, shared with the managed-write guard: 408 and 5xx prove nothing.
fn unproven_status(response: &Response<'_>) -> Option<Verdict> {
    match crate::request_context::ResponseProof::from_status(response.status) {
        crate::request_context::ResponseProof::Unproven(kind) => Some(Verdict::Unknown(Some(kind))),
        crate::request_context::ResponseProof::NativeCodec => None,
    }
}

/// Header-proven delay only; never a default. Any status may carry it.
pub fn retry_after_ms(response: &Response<'_>) -> Option<f64> {
    response
        .header("retry-after")
        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .map(|seconds| (seconds * 1_000.0).min(crate::startup::MAX_RATE_LIMIT_PAUSE_MS))
}

/// The classified error for a released request. `message` is the venue's
/// existing user-facing text; native evidence goes in the venue fields.
pub fn error(
    verdict: Verdict,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    message: impl Into<String>,
) -> VenueError {
    let mut error = VenueError::new(message);
    error.kind = verdict.kind();
    if intent != Intent::Read {
        error.outcome = match verdict {
            Verdict::Refused(_) | Verdict::RefusedResend(_) => Some(Outcome::Rejected),
            Verdict::Unknown(_) => Some(Outcome::Unknown),
            Verdict::Satisfied(_) => None,
        };
    }
    if let Some(native) = native {
        match native.code {
            NativeCode::Int(code) => error.venue_code = Some(code.to_string()),
            NativeCode::Text(code) => error.venue_code = Some(code.to_owned()),
            NativeCode::None => {}
        }
        if !native.message.is_empty() {
            error.venue_message = Some(native.message.to_owned());
        }
    }
    if let (Some(ErrorKind::RateLimited), Released::Response(response)) = (error.kind, released)
        && let Some(ms) = retry_after_ms(response)
    {
        error = error.retry_after_ms(ms);
    }
    error
}

/// A classified write failure. Only the classifier creates one, so a venue
/// cannot turn status or exception text into a refusal fact.
#[derive(Clone, Debug, PartialEq)]
pub struct Fact {
    verdict: Verdict,
    error: VenueError,
    /// The managed attempt whose reply was classified.
    attempt: Option<crate::RequestId>,
}

impl Fact {
    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    pub fn error(&self) -> &VenueError {
        &self.error
    }

    /// Native order IDs this reply carried: reconciliation evidence only.
    pub fn with_order_ids(mut self, order_ids: &[String]) -> Self {
        self.error = self.error.partial_ids(order_ids);
        self
    }

    pub(crate) fn for_attempt(mut self, attempt: crate::RequestId) -> Self {
        self.attempt = Some(attempt);
        self
    }

    /// The codec cannot prove this reply. Only ever weakens the fact.
    pub fn unproven(mut self) -> Self {
        self.verdict = Verdict::Unknown(self.verdict.kind());
        self.error.outcome = Some(Outcome::Unknown);
        self
    }
}

/// A released write's classified answer (rules 2-6).
pub fn fact(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    message: impl Into<String>,
) -> Fact {
    fact_for_delivery(table, intent, released, native, message, false)
}

pub(crate) fn fact_for_delivery(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    message: impl Into<String>,
    status_exempt: bool,
) -> Fact {
    let verdict = verdict_for_delivery(table, intent, released, native, status_exempt);
    Fact {
        verdict,
        error: error(verdict, intent, released, native, message),
        attempt: None,
    }
}

/// A 2xx whose success envelope or acknowledgement does not prove the write.
pub fn unproven_reply(message: impl Into<String>) -> Fact {
    Fact {
        verdict: Verdict::Unknown(Some(ErrorKind::Internal)),
        error: VenueError::new(message)
            .kind(ErrorKind::Internal)
            .outcome(Outcome::Unknown),
        attempt: None,
    }
}

/// One write attempt's native answer, for `Cx::http_write_fact`.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteFact {
    Accepted {
        order_ids: Vec<String>,
    },
    /// `known_effects`: the codec proves part of this request took effect
    /// (for example a partially cancelled batch).
    Failed {
        fact: Fact,
        known_effects: bool,
    },
}

impl WriteFact {
    pub(crate) fn into_result(self) -> crate::request_context::WriteResult {
        use crate::request_context::WriteResult;
        match self {
            Self::Accepted { order_ids } => WriteResult::Accepted { order_ids },
            Self::Failed {
                fact,
                known_effects,
            } => match fact.verdict {
                Verdict::Satisfied(_) if !known_effects => WriteResult::Accepted {
                    order_ids: Vec::new(),
                },
                Verdict::Unknown(_) if known_effects => WriteResult::PartialUnknown(fact.error),
                Verdict::Unknown(_) => WriteResult::Failed(fact.error),
                _ if known_effects => WriteResult::KnownPartial(fact.error),
                _ => WriteResult::Rejected(fact.error),
            },
        }
    }

    /// A refusal must be the classifier's fact about this exact attempt; an
    /// attempt-free fact can only be an unproven (Unknown) answer.
    pub(crate) fn belongs_to(&self, attempt: crate::RequestId) -> bool {
        match self {
            Self::Accepted { .. } => true,
            Self::Failed { fact, .. } => match fact.attempt {
                Some(own) => own == attempt,
                None => matches!(fact.verdict, Verdict::Unknown(_)),
            },
        }
    }

    /// Only a whole, effects-free refusal can earn a resend.
    pub(crate) fn resendable(&self) -> bool {
        matches!(self, Self::Failed { fact, known_effects: false }
            if matches!(fact.verdict, Verdict::RefusedResend(_)) && fact.error.partial.is_none())
    }
}

/// Legacy (unmanaged) lookup: transitional, deleted with the request bridge.
/// It never closes a P0 aggregation row and grants no resend.
pub fn write(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    message: impl Into<String>,
) -> (Verdict, VenueError) {
    let fact = fact(table, intent, released, native, message);
    (fact.verdict, fact.error)
}

/// One acknowledgement row of a batch write, in caller order.
#[derive(Clone, Copy, Debug)]
pub enum BatchItem<'a> {
    /// Native ID the venue proves it accepted (or, for cancels, echoed).
    Accepted(&'a str),
    Native(Native<'a>),
}

/// Engine projection of one batch acknowledgement: count check, accepted-ID
/// join and the Accepted / Rejected / KnownPartial / PartialUnknown / Failed choice.
pub(crate) fn batch(
    table: &CodeTable,
    intent: Intent,
    response: &Response<'_>,
    expected: usize,
    items: &[BatchItem<'_>],
    message: impl Into<String>,
) -> Result<crate::request_context::WriteResult, crate::request_context::Error> {
    use crate::request_context::WriteResult;
    if expected == 0 || intent == Intent::Read {
        return Err(crate::request_context::Error::Invalid);
    }
    let released = Released::Response(response);
    if let Some(verdict) = unproven_status(response) {
        let error = self::error(verdict, intent, released, None, message);
        return Ok(WriteResult::Failed(error));
    }
    let ids: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            BatchItem::Accepted(id) if !id.is_empty() => Some((*id).to_owned()),
            _ => None,
        })
        .collect();
    let message = message.into();
    if items.len() != expected
        || items
            .iter()
            .any(|item| matches!(item, BatchItem::Accepted(id) if id.is_empty()))
    {
        let error = VenueError::new(message)
            .kind(ErrorKind::Internal)
            .outcome(Outcome::Unknown)
            .partial_ids(&ids);
        return Ok(WriteResult::Failed(error));
    }
    let mut failures = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            BatchItem::Accepted(_) => None,
            BatchItem::Native(native) => match table.verdict(native, intent) {
                Verdict::Satisfied(_) => None,
                verdict => Some((index, native, verdict)),
            },
        });
    let Some(first) = failures.next() else {
        return Ok(WriteResult::Accepted { order_ids: ids });
    };
    let failures: Vec<_> = std::iter::once(first).chain(failures).collect();
    let effected = failures.len() < items.len();
    // The first non-accepted position is the failed index; an unknown item
    // anywhere supplies the error kind and text.
    let index = first.0;
    let (_, native, verdict) = failures
        .iter()
        .find(|(_, _, verdict)| matches!(verdict, Verdict::Unknown(_)))
        .copied()
        .unwrap_or(first);
    let error = self::error(verdict, intent, released, Some(native), message);
    let partial_unknown = matches!(verdict, Verdict::Unknown(_)) && effected;
    let error = if ids.is_empty() {
        error
    } else {
        error.partial(&ids, expected, index)
    };
    Ok(if partial_unknown {
        WriteResult::PartialUnknown(error)
    } else if matches!(verdict, Verdict::Unknown(_)) {
        WriteResult::Failed(error)
    } else if effected {
        WriteResult::KnownPartial(error)
    } else {
        WriteResult::Rejected(error)
    })
}

/// Credential rejection evidence from the one table, for a venue's
/// `account_auth_outcome`. The engine consults that hook only for credentialed
/// account reads, so public reads never produce credential evidence.
pub fn auth_outcome(table: &CodeTable, native: &Native<'_>) -> Option<AuthOutcome> {
    matches!(
        table.verdict(native, Intent::Read),
        Verdict::Refused(
            ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission
        )
    )
    .then_some(AuthOutcome::Rejected)
}

/// The engine's wait for a rate limit that names no Retry-After (rule 5).
pub const DEFAULT_RATE_LIMIT_DEFERRAL_MS: f64 = 10_000.0;

/// How a failed read may continue. A read carries no money effect, so this
/// is retry evidence only, separate from signing and transport budgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadRetry {
    /// A proven refusal that permits one re-signed resend.
    NativeResend,
    /// The venue said when to come back (Retry-After header).
    After(f64),
    /// The answer proves nothing: eligible under `RetryPolicy::ReadFailure`.
    ReadFailure,
}

/// A classified read failure: kind and native evidence, never an `Outcome`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadFailure {
    pub error: VenueError,
    pub retry: Option<ReadRetry>,
}

/// Rules 2-6 for a read. Code 0 or a 2xx status never proves a body; only the
/// venue codec does, so call this only for a failed or unproven answer.
pub fn read(
    table: &CodeTable,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    message: impl Into<String>,
) -> ReadFailure {
    let verdict = verdict(table, Intent::Read, released, native);
    let error = error(verdict, Intent::Read, released, native, message);
    let header = match released {
        Released::Response(response) => retry_after_ms(response),
        Released::Transport => None,
    };
    let retry = match (verdict, verdict.kind()) {
        (Verdict::RefusedResend(_), _) => Some(ReadRetry::NativeResend),
        (_, Some(ErrorKind::RateLimited)) => Some(ReadRetry::After(
            header.unwrap_or(DEFAULT_RATE_LIMIT_DEFERRAL_MS),
        )),
        (_, Some(ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission)) => {
            None
        }
        (Verdict::Unknown(_), _) => Some(header.map_or(ReadRetry::ReadFailure, ReadRetry::After)),
        (Verdict::Refused(_) | Verdict::Satisfied(_), _) => None,
    };
    ReadFailure { error, retry }
}

/// Venue table invariants, for each venue's own table test.
pub fn assert_table(table: &CodeTable) {
    let satisfied_ok = |row: &Row| match row.verdict {
        Verdict::Satisfied(satisfied) => {
            row.only == Some(satisfied.intent()) && !matches!(row.key, Key::Range(..))
        }
        _ => true,
    };
    for (index, row) in table.rows.iter().enumerate() {
        assert!(
            satisfied_ok(row),
            "row {index}: Satisfied needs a matching `only`"
        );
        assert!(
            row.verdict != Verdict::RefusedResend(ErrorKind::RateLimited),
            "row {index}: a rate limit waits; it never resends"
        );
        if let Key::Range(low, high) = row.key {
            assert!(low <= high, "row {index}: empty range");
        }
        for (other_index, other) in table.rows.iter().enumerate().skip(index + 1) {
            assert!(
                row.only != other.only || !keys_overlap(row.key, other.key),
                "rows {index} and {other_index} overlap for the same intent scope"
            );
        }
    }
    assert!(
        !matches!(
            table.unlisted,
            Verdict::Satisfied(_) | Verdict::RefusedResend(_)
        ),
        "unlisted codes cannot be Satisfied or RefusedResend"
    );
    let prefixes = table
        .rows
        .iter()
        .filter(|row| matches!(row.key, Key::Message(_)))
        .count();
    assert!(
        prefixes == 0 || prefixes == table.rows.len(),
        "Message rows are only for code-less venues"
    );
    assert!(
        !table
            .rows
            .iter()
            .any(|row| matches!(row.key, Key::Message(""))),
        "an empty Message row is no evidence"
    );
    for (index, (status, verdict)) in table.status.iter().enumerate() {
        assert!(
            (400..500).contains(status) && *status != 408,
            "status row {status}: only documented 4xx other than 408"
        );
        assert!(
            matches!(verdict, Verdict::Refused(_) | Verdict::Unknown(_)),
            "status row {status} cannot be Satisfied or RefusedResend"
        );
        assert!(
            !table.status[..index]
                .iter()
                .any(|(other, _)| other == status),
            "duplicate status row {status}"
        );
    }
}

fn keys_overlap(a: Key, b: Key) -> bool {
    let range = |key| match key {
        Key::Int(code) => Some((code, code)),
        Key::Range(low, high) => Some((low, high)),
        _ => None,
    };
    match (range(a), range(b)) {
        (Some((a_low, a_high)), Some((b_low, b_high))) => a_low <= b_high && b_low <= a_high,
        _ => match (a, b) {
            (Key::Text(a), Key::Text(b)) => a == b,
            (Key::Message(a), Key::Message(b)) => a == b,
            _ => false,
        },
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
