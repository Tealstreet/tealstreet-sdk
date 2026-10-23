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
    /// Exact prefix of the native message field, only for code-less venues.
    MessagePrefix(&'static str),
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
            (Self::MessagePrefix(prefix), NativeCode::None) => native.message.starts_with(prefix),
            _ => false,
        }
    }
}

impl CodeTable {
    /// Exact-intent rows first, then rows for every intent, then `unlisted`.
    pub fn verdict(&self, native: &Native<'_>, intent: Intent) -> Verdict {
        let scoped = |only: Option<Intent>| {
            self.rows
                .iter()
                .find(|row| row.only == only && row.key.matches(native))
        };
        let verdict = scoped(Some(intent))
            .or_else(|| scoped(None))
            .map_or(self.unlisted, |row| row.verdict);
        match verdict {
            Verdict::Satisfied(satisfied) if satisfied.intent() != intent => Verdict::Unknown(None),
            verdict => verdict,
        }
    }

    fn status_verdict(&self, status: u16) -> Option<Verdict> {
        self.status
            .iter()
            .find(|(documented, _)| *documented == status)
            .map(|(_, verdict)| *verdict)
    }
}

fn status_kind(status: u16) -> Option<ErrorKind> {
    match status {
        200..=299 => Some(ErrorKind::Internal),
        401 => Some(ErrorKind::AuthInvalid),
        403 => Some(ErrorKind::AuthPermission),
        418 | 429 => Some(ErrorKind::RateLimited),
        _ => None,
    }
}

/// Engine precedence, rules 2-6. Call only for a failed or unproven answer; a
/// 2xx without a native error means the success ACK was malformed.
pub fn verdict(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
) -> Verdict {
    let response = match released {
        Released::Transport => return Verdict::Unknown(Some(ErrorKind::Network)),
        Released::Response(response) => response,
    };
    match response.status {
        408 => return Verdict::Unknown(Some(ErrorKind::Timeout)),
        500.. => return Verdict::Unknown(Some(ErrorKind::VenueUnavailable)),
        _ => {}
    }
    if let Some(native) = native {
        return table.verdict(native, intent);
    }
    match table.status_verdict(response.status) {
        Some(verdict) if !(200..300).contains(&response.status) => verdict,
        _ => Verdict::Unknown(status_kind(response.status)),
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

/// Legacy (unmanaged) lookup: transitional, deleted with the request bridge.
/// It never closes a P0 aggregation row and grants no resend.
pub fn write(
    table: &CodeTable,
    intent: Intent,
    released: Released<'_>,
    native: Option<&Native<'_>>,
    message: impl Into<String>,
) -> (Verdict, VenueError) {
    let verdict = verdict(table, intent, released, native);
    (verdict, error(verdict, intent, released, native, message))
}

/// One acknowledgement row of a batch write, in caller order.
#[derive(Clone, Copy, Debug)]
pub enum BatchItem<'a> {
    /// Native ID the venue proves it accepted (or, for cancels, echoed).
    Accepted(&'a str),
    Native(Native<'a>),
}

/// Engine projection of one batch acknowledgement: count check, accepted-ID
/// join and the Accepted / Rejected / KnownPartial / Failed choice.
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
    let (index, native, verdict) = failures
        .iter()
        .find(|(_, _, verdict)| matches!(verdict, Verdict::Unknown(_)))
        .copied()
        .unwrap_or(first);
    let error = self::error(verdict, intent, released, Some(native), message);
    let error = if ids.is_empty() {
        error
    } else {
        error.partial(&ids, expected, index)
    };
    Ok(if matches!(verdict, Verdict::Unknown(_)) {
        WriteResult::Failed(error)
    } else if effected {
        WriteResult::KnownPartial(error)
    } else {
        WriteResult::Rejected(error)
    })
}

/// Credential evidence from one table; public reads never produce any.
pub fn auth_outcome(
    table: &CodeTable,
    credentialed: bool,
    native: &Native<'_>,
) -> Option<AuthOutcome> {
    if !credentialed {
        return None;
    }
    matches!(
        table.verdict(native, Intent::Read),
        Verdict::Refused(
            ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission
        )
    )
    .then_some(AuthOutcome::Rejected)
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
        !matches!(table.unlisted, Verdict::Satisfied(_)),
        "unlisted codes cannot be Satisfied"
    );
    for (status, verdict) in table.status {
        assert!(
            (400..500).contains(status) && *status != 408,
            "status row {status}: only documented 4xx other than 408"
        );
        assert!(
            !matches!(verdict, Verdict::Satisfied(_)),
            "status row {status} cannot be Satisfied"
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
            (Key::MessagePrefix(a), Key::MessagePrefix(b)) => a.starts_with(b) || b.starts_with(a),
            _ => false,
        },
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
