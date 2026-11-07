//! Sentry error capture for the native exchange server, with the semantics of
//! `packages/service-telemetry` (`initServiceSentry`): off unless
//! `EXCHANGE_SERVER_SENTRY_DSN` is set; `server_name` and a `box` tag from
//! `TEALSTREET_BOX`; `release` from `TEALSTREET_RELEASE`, omitted when unset or
//! a placeholder; no region and no SDK inference of host, release or
//! environment.
//!
//! This process holds users' exchange credentials, so every event is rebuilt
//! from an allowlist in `before_send`: fixed summaries (never error text or a
//! panic payload), a fingerprint, stack frames' code locations, and the tags
//! and extras named below. Nothing else (request data, user, contexts,
//! breadcrumbs, frame variables) leaves. Events are throttled per fingerprint
//! and globally, and sent from Sentry's own transport thread through a bounded
//! queue that drops when full; nothing flushes inline, so a failure storm or a
//! stalled Sentry can never slow a trading path.
use sentry::protocol::{Event, Exception, Frame, Level, Mechanism, Stacktrace, Value, Values};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SERVICE: &str = "exchange-server";
/// One event per fingerprint per window, carrying how many were suppressed.
const PER_KEY_WINDOW: Duration = Duration::from_secs(60);
/// At most this many events per minute across every fingerprint.
const GLOBAL_PER_MINUTE: u32 = 30;
/// A long-lived process must not grow a map entry per distinct fingerprint.
const MAX_TRACKED_KEYS: usize = 500;
const TAGS: &[&str] = &[
    "service",
    "box",
    "kind",
    "error_kind",
    "venue_code",
    "venue",
    "target",
    "startup_unloaded",
    "startup_refused",
];
const EXTRAS: &[&str] = &["user_tag", "suppressed_since_last_event"];

static ENABLED: AtomicBool = AtomicBool::new(false);

/// The identity every event carries, from the box's environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub box_label: Option<String>,
    pub release: Option<String>,
}

/// A value, trimmed, or None when empty or a placeholder.
fn label(value: Option<String>) -> Option<String> {
    let trimmed = value?.trim().to_owned();
    let placeholder = ["undefined", "null", "unknown"].contains(&trimmed.to_lowercase().as_str());
    (!trimmed.is_empty() && !placeholder).then_some(trimmed)
}

/// `^[a-zA-Z0-9][a-zA-Z0-9._-]{0,127}$`, as service-telemetry checks a box label.
fn valid_box(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && value.len() <= 128
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

impl Identity {
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        Self {
            box_label: label(var("TEALSTREET_BOX")).filter(|value| valid_box(value)),
            release: label(var("TEALSTREET_RELEASE")),
        }
    }
}

/// Per-fingerprint and global event limits.
#[derive(Default)]
pub struct Throttle {
    keys: HashMap<String, (Instant, u32)>,
    minute: Option<(Instant, u32)>,
}

impl Throttle {
    /// Whether an event with `key` may go now; when it may, how many with that
    /// key were suppressed since the last one sent.
    pub fn claim(&mut self, key: &str, now: Instant) -> Option<u32> {
        if let Some((sent, suppressed)) = self.keys.get_mut(key)
            && now.duration_since(*sent) < PER_KEY_WINDOW
        {
            *suppressed += 1;
            return None;
        }
        let minute = self.minute.get_or_insert((now, 0));
        if now.duration_since(minute.0) >= Duration::from_secs(60) {
            *minute = (now, 0);
        }
        if minute.1 >= GLOBAL_PER_MINUTE {
            // Counted against its key, so the next event that goes reports it.
            self.keys.entry(key.to_owned()).or_insert((now, 0)).1 += 1;
            self.prune(now);
            return None;
        }
        minute.1 += 1;
        let suppressed = self
            .keys
            .insert(key.to_owned(), (now, 0))
            .map_or(0, |(_, suppressed)| suppressed);
        self.prune(now);
        Some(suppressed)
    }

    fn prune(&mut self, now: Instant) {
        if self.keys.len() <= MAX_TRACKED_KEYS {
            return;
        }
        self.keys
            .retain(|_, (sent, _)| now.duration_since(*sent) < PER_KEY_WINDOW);
        while self.keys.len() > MAX_TRACKED_KEYS {
            let oldest = self
                .keys
                .iter()
                .min_by_key(|(_, (sent, _))| *sent)
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest {
                self.keys.remove(&key);
            }
        }
    }
}

/// Key names whose value is a credential, whatever follows them.
const SENSITIVE_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "secret",
    "passphrase",
    "password",
    "privatekey",
    "private_key",
    "apikey",
    "api_key",
    "api-key",
    "apisecret",
    "proxykey",
    "proxytoken",
    "token",
    "mnemonic",
    "seed",
    "signature",
];
const MAX_TEXT: usize = 512;

fn sensitive_key(word: &str) -> bool {
    let word = word.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|key| word.contains(key))
}

fn key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+')
}

/// 32+ characters of unbroken base64/hex is a key or a JWT segment, not prose.
/// A run with `/` is redacted whole only when it mixes cases and digits, so a
/// file path stays readable.
fn redact_run(run: &str, out: &mut String) {
    let mixed = run.chars().any(|c| c.is_ascii_lowercase())
        && run.chars().any(|c| c.is_ascii_uppercase())
        && run.chars().any(|c| c.is_ascii_digit());
    if run.contains('/') && run.len() >= 32 && mixed {
        out.push_str("[redacted]");
        return;
    }
    for (i, segment) in run.split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        let body = segment.trim_end_matches('=');
        if body.len() >= 32 && body.chars().all(key_char) {
            out.push_str("[redacted]");
        } else {
            out.push_str(segment);
        }
    }
}

/// Everything after a URL's path (query and fragment) is dropped: a signed
/// venue request carries its key and signature there.
fn strip_query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = ["https://", "http://", "wss://", "ws://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        out.push_str(&rest[..at]);
        let url = &rest[at..];
        let end = url
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ')' | '>' | ']' | ','))
            .unwrap_or(url.len());
        let (url, after) = url.split_at(end);
        match url.find(['?', '#']) {
            Some(query) => {
                out.push_str(&url[..query]);
                out.push_str("?[redacted]");
            }
            None => out.push_str(url),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-')
}

/// A value runs to whitespace or a delimiter.
fn value_len(text: &str) -> usize {
    text.find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '&' | ',' | ';' | '}'))
        .unwrap_or(text.len())
}

fn blank_len(text: &str) -> usize {
    text.find(|c: char| !matches!(c, ' ' | '\t'))
        .unwrap_or(text.len())
}

/// A credential named by its key (`api_key = …`, `"secret": "…"`,
/// `Authorization:\tBearer …`) loses its value. Quotes, spaces and tabs may
/// surround the `=` or `:`; an auth scheme word needs no separator.
fn redact_named(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(word_char) {
        let end = rest[start..]
            .find(|c: char| !word_char(c))
            .map_or(rest.len(), |len| start + len);
        out.push_str(&rest[..end]);
        let named = sensitive_key(&rest[start..end]);
        rest = &rest[end..];
        if !named {
            continue;
        }
        let gap = rest
            .find(|c: char| !matches!(c, '=' | ':' | '"' | '\'' | ' ' | '\t'))
            .unwrap_or(rest.len());
        let after = &rest[gap..];
        let scheme_len = value_len(after);
        let scheme =
            ["bearer", "basic"].contains(&after[..scheme_len].to_ascii_lowercase().as_str());
        if !scheme && !rest[..gap].contains(['=', ':']) {
            continue;
        }
        out.push_str(&rest[..gap]);
        rest = after;
        if scheme {
            let spaced = scheme_len + blank_len(&rest[scheme_len..]);
            out.push_str(&rest[..spaced]);
            rest = &rest[spaced..];
        }
        let value = value_len(rest);
        if value > 0 {
            out.push_str("[redacted]");
            rest = &rest[value..];
        }
    }
    out.push_str(rest);
    out
}

/// The scrubber for every string an event carries.
pub fn scrub(text: &str) -> String {
    let text = redact_named(&strip_query(text));
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for c in text.chars() {
        if key_char(c) || c == '/' || c == '=' {
            run.push(c);
            continue;
        }
        redact_run(&run, &mut out);
        run.clear();
        out.push(c);
    }
    redact_run(&run, &mut out);
    if out.len() > MAX_TEXT {
        let mut cut = MAX_TEXT;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push('…');
    }
    out
}

/// The event that leaves: allowlisted fields only, identity set here rather
/// than inferred by the SDK.
fn allowlisted(event: Event<'static>, identity: &Identity, suppressed: u32) -> Event<'static> {
    let mut tags: BTreeMap<String, String> = event
        .tags
        .into_iter()
        .filter(|(key, _)| TAGS.contains(&key.as_str()) && key != "box")
        .map(|(key, value)| (key, scrub(&value).chars().take(200).collect()))
        .collect();
    tags.insert("service".into(), SERVICE.into());
    if let Some(label) = &identity.box_label {
        tags.insert("box".into(), label.clone());
    }
    let mut extra: BTreeMap<String, Value> = event
        .extra
        .into_iter()
        .filter(|(key, _)| EXTRAS.contains(&key.as_str()))
        .map(|(key, value)| match value {
            Value::String(text) => (key, Value::String(scrub(&text))),
            other @ Value::Number(_) => (key, other),
            _ => (key, Value::Null),
        })
        .collect();
    if suppressed > 0 {
        extra.insert("suppressed_since_last_event".into(), suppressed.into());
    }
    let exception = Values::from(
        event
            .exception
            .values
            .into_iter()
            .map(|exception| Exception {
                ty: scrub(&exception.ty),
                value: exception.value.as_deref().map(scrub),
                stacktrace: exception.stacktrace.map(code_locations),
                mechanism: exception.mechanism.map(|mechanism| Mechanism {
                    ty: scrub(&mechanism.ty),
                    handled: mechanism.handled,
                    ..Default::default()
                }),
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    );
    let fingerprint: Vec<_> = event
        .fingerprint
        .iter()
        .map(|part| scrub(part).chars().take(200).collect::<String>().into())
        .collect();
    Event {
        event_id: event.event_id,
        timestamp: event.timestamp,
        level: event.level,
        platform: event.platform,
        fingerprint: fingerprint.into(),
        message: event.message.as_deref().map(scrub),
        exception,
        tags,
        extra,
        release: identity.release.clone().map(Into::into),
        server_name: identity.box_label.clone().map(Into::into),
        ..Default::default()
    }
}

/// A stack trace's code locations only: no variables, source context,
/// registers or addresses.
fn code_locations(stacktrace: Stacktrace) -> Stacktrace {
    Stacktrace {
        frames: stacktrace
            .frames
            .into_iter()
            .map(|frame| Frame {
                function: frame.function,
                module: frame.module,
                filename: frame.filename,
                lineno: frame.lineno,
                colno: frame.colno,
                in_app: frame.in_app,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

/// Captures a panic without flushing: the panicking thread may be an engine
/// thread every account on it waits for, so delivery stays on the transport
/// thread. The payload can carry a venue body or user ID; only its location leaves.
struct PanicCapture;

impl sentry::Integration for PanicCapture {
    fn name(&self) -> &'static str {
        "panic-capture"
    }

    fn setup(&self, _: &mut sentry::ClientOptions) {
        static HOOK: std::sync::Once = std::sync::Once::new();
        HOOK.call_once(|| {
            let next = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                sentry::Hub::with_active(|hub| hub.capture_event(panic_event(info)));
                next(info);
            }));
        });
    }
}

fn panic_event(info: &std::panic::PanicHookInfo<'_>) -> Event<'static> {
    let location = info
        .location()
        .map(|at| format!("{}:{}", at.file(), at.line()))
        .unwrap_or_default();
    let mut event =
        sentry::integrations::panic::PanicIntegration::new().event_from_panic_info(info);
    for exception in &mut event.exception.values {
        exception.value = Some(format!("panic at {location}"));
    }
    event.fingerprint = vec!["panic".into(), location.into()].into();
    event
}

/// The throttle key: an explicit fingerprint, else the exception (a panic) or message.
fn throttle_key(event: &Event<'static>) -> String {
    let explicit: Vec<&str> = event.fingerprint.iter().map(|part| part.as_ref()).collect();
    if explicit != ["{{ default }}"] {
        return explicit.join("|");
    }
    let detail = match event.exception.values.first() {
        Some(exception) => format!(
            "{}:{}",
            exception.ty,
            exception.value.as_deref().unwrap_or_default()
        ),
        None => event.message.clone().unwrap_or_default(),
    };
    scrub(&detail).chars().take(120).collect()
}

/// Client options for `dsn`: no default integrations (no host contexts,
/// breadcrumbs or stack attachment), the non-flushing panic hook, and the allowlist.
pub fn client_options(dsn: &str, identity: Identity) -> Option<sentry::ClientOptions> {
    let dsn = dsn.trim().parse().ok()?;
    let throttle = Arc::new(Mutex::new(Throttle::default()));
    let send = identity.clone();
    let mut options = sentry::ClientOptions::default();
    options.dsn = Some(dsn);
    options.release = identity.release.clone().map(Into::into);
    options.server_name = identity.box_label.clone().map(Into::into);
    options.default_integrations = false;
    options.integrations = vec![Arc::new(PanicCapture)];
    options.send_default_pii = false;
    options.attach_stacktrace = false;
    options.max_breadcrumbs = 0;
    options.auto_session_tracking = false;
    options.shutdown_timeout = Duration::from_secs(2);
    options.before_breadcrumb = Some(Arc::new(|_| None));
    options.before_send = Some(Arc::new(move |event| {
        let key = throttle_key(&event);
        let suppressed = throttle.lock().ok()?.claim(&key, Instant::now())?;
        Some(allowlisted(event, &send, suppressed))
    }));
    Some(options)
}

/// Starts capture when `EXCHANGE_SERVER_SENTRY_DSN` is set; otherwise None and
/// every capture is a no-op. Keep the guard for the process's life: dropping it
/// at shutdown is the only flush, bounded to two seconds.
pub fn init_from_env() -> Option<sentry::ClientInitGuard> {
    let dsn = std::env::var("EXCHANGE_SERVER_SENTRY_DSN")
        .ok()
        .filter(|dsn| !dsn.trim().is_empty())?;
    let identity = Identity::from_vars(|name| std::env::var(name).ok());
    let Some(options) = client_options(&dsn, identity) else {
        crate::state::Log::warn("EXCHANGE_SERVER_SENTRY_DSN is not a valid DSN; Sentry is off");
        return None;
    };
    // The reqwest transport uses the process's rustls provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let guard = sentry::init(options);
    ENABLED.store(true, Ordering::Relaxed);
    safe_cex_native::report::set_error_sink(native_report);
    Some(guard)
}

/// One error condition for Sentry. `summary` is fixed text: never an account
/// name, URL, error text or venue payload (those go in the journal line only).
#[derive(Default)]
pub struct Report<'a> {
    pub kind: &'static str,
    pub summary: &'a str,
    pub user_tag: Option<String>,
    pub venue: Option<&'a str>,
    pub target: Option<&'a str>,
    pub error_kind: Option<&'a str>,
    pub venue_code: Option<&'a str>,
    /// A start failure's first unloaded section (`markets`..`positions`): what had
    /// not loaded, never which read failed.
    pub startup_unloaded: Option<&'static str>,
    pub startup_refused: Option<bool>,
}

impl Report<'_> {
    fn event(&self) -> Event<'static> {
        let mut tags = BTreeMap::new();
        tags.insert("kind".to_owned(), self.kind.to_owned());
        for (key, value) in [
            ("venue", self.venue),
            ("target", self.target),
            ("error_kind", self.error_kind),
            ("venue_code", self.venue_code),
            ("startup_unloaded", self.startup_unloaded),
            (
                "startup_refused",
                self.startup_refused
                    .map(|refused| if refused { "true" } else { "false" }),
            ),
        ] {
            if let Some(value) = value {
                tags.insert(key.to_owned(), value.to_owned());
            }
        }
        let mut extra = BTreeMap::new();
        if let Some(user_tag) = &self.user_tag {
            extra.insert("user_tag".to_owned(), Value::String(user_tag.clone()));
        }
        // One issue per condition and target, not per venue message.
        let fingerprint: Vec<String> = [Some(self.kind), self.target, self.error_kind]
            .into_iter()
            .flatten()
            .map(|part| scrub(part).chars().take(120).collect())
            .collect();
        Event {
            level: Level::Error,
            message: Some(self.summary.to_owned()),
            fingerprint: fingerprint
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
            tags,
            extra,
            ..Default::default()
        }
    }
}

/// Captures `report`; a no-op when Sentry is off. Never blocks or fails the caller.
pub fn capture(report: &Report<'_>) {
    if ENABLED.load(Ordering::Relaxed) {
        sentry::capture_event(report.event());
    }
}

/// The native host's account-start and transport failures.
fn native_report(report: &safe_cex_native::report::ErrorReport<'_>) {
    capture(&from_native(report));
}

/// A native report as a Sentry report: the transport's venue when it named one,
/// else the label's. The label is `user_tag account venue`; the account name
/// stays in the journal.
fn from_native<'a>(report: &safe_cex_native::report::ErrorReport<'a>) -> Report<'a> {
    use safe_cex_native::report::ErrorKind;
    let words: Vec<&'a str> = report
        .label
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let summary = match report.kind {
        ErrorKind::AccountStartFailed => "account start failed",
        ErrorKind::SocketOpenFailed => "venue, relay or tunnel socket did not open",
        ErrorKind::RequestFailed => "venue request failed in transport",
    };
    Report {
        kind: report.kind.as_str(),
        summary,
        user_tag: (words.len() >= 2).then(|| words[0].to_owned()),
        venue: report
            .venue
            .or_else(|| (words.len() >= 2).then(|| words[words.len() - 1])),
        target: report.target,
        error_kind: report.error_kind,
        venue_code: report.venue_code,
        startup_unloaded: report.startup_unloaded,
        startup_refused: report.startup_refused,
    }
}

#[cfg(test)]
pub(crate) fn report_event(report: &Report<'_>) -> Event<'static> {
    report.event()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::test::with_captured_events_options;

    const DSN: &str = "https://public@o1.ingest.us.sentry.io/2";

    fn identity(box_label: &str, release: &str) -> Identity {
        Identity::from_vars(|name| match name {
            "TEALSTREET_BOX" => Some(box_label.to_owned()),
            "TEALSTREET_RELEASE" => Some(release.to_owned()),
            _ => None,
        })
    }

    fn captured(identity: Identity, send: impl FnOnce()) -> Vec<Event<'static>> {
        with_captured_events_options(send, client_options(DSN, identity).unwrap())
    }

    #[test]
    fn no_dsn_is_a_no_op() {
        assert!(std::env::var("EXCHANGE_SERVER_SENTRY_DSN").is_err());
        assert!(init_from_env().is_none());
        assert!(!ENABLED.load(Ordering::Relaxed));
        // Off: capture does nothing and cannot fail the caller.
        capture(&Report {
            kind: "drain.deadline_passed",
            summary: "drain passed its deadline",
            ..Default::default()
        });
        assert!(client_options("", Identity::default()).is_none());
        assert!(client_options("not a dsn", Identity::default()).is_none());
    }

    #[test]
    fn identity_is_the_box_label_and_image_release_only() {
        let id = identity(" exchange-02 ", " fb652156d52 ");
        assert_eq!(id.box_label.as_deref(), Some("exchange-02"));
        assert_eq!(id.release.as_deref(), Some("fb652156d52"));
        for placeholder in ["", " ", "undefined", "null", "unknown", "UNDEFINED"] {
            assert_eq!(identity(placeholder, placeholder), Identity::default());
        }
        assert_eq!(identity("not a box\nlabel", "r").box_label, None);

        let events = captured(id, || {
            let mut event = report_event(&Report {
                kind: "signer.failing",
                summary: "signer keeps failing",
                venue: Some("bybit"),
                ..Default::default()
            });
            event.server_name = Some("ephemeral-container".into());
            event.release = Some("undefined".into());
            event.environment = Some("production".into());
            event.tags.insert("region".into(), "eu".into());
            event.tags.insert("box".into(), "old".into());
            sentry::capture_event(event);
        });
        let event = &events[0];
        assert_eq!(event.server_name.as_deref(), Some("exchange-02"));
        assert_eq!(event.release.as_deref(), Some("fb652156d52"));
        assert_eq!(event.environment, None);
        let tags: Vec<_> = event
            .tags
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            tags,
            [
                ("box", "exchange-02"),
                ("kind", "signer.failing"),
                ("service", "exchange-server"),
                ("venue", "bybit")
            ]
        );

        let events = captured(Identity::default(), || {
            let mut event = report_event(&Report {
                kind: "signer.failing",
                summary: "signer keeps failing",
                ..Default::default()
            });
            event.server_name = Some("container".into());
            event.release = Some("undefined".into());
            sentry::capture_event(event);
        });
        assert_eq!(events[0].server_name, None);
        assert_eq!(events[0].release, None);
        assert!(!events[0].tags.contains_key("box"));
    }

    #[test]
    fn scrubs_signed_urls_auth_headers_and_key_shaped_strings() {
        assert_eq!(
            scrub(
                "GET https://api.bybit.com/v5/order/create?api_key=XYZ&timestamp=1&sign=abc failed"
            ),
            "GET https://api.bybit.com/v5/order/create?[redacted] failed"
        );
        assert_eq!(
            scrub("wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?ts-relay=wss%3A%2F%2Fx&token=t"),
            "wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel?[redacted]"
        );
        assert_eq!(
            scrub("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.e30.sig rejected"),
            "Authorization: Bearer [redacted] rejected"
        );
        assert_eq!(
            scrub(r#"{"secret":"hunter2","x":1}"#),
            r#"{"secret":"[redacted]","x":1}"#
        );
        assert_eq!(
            scrub("api_key=abc123 passphrase: p"),
            "api_key=[redacted] passphrase: [redacted]"
        );
        assert_eq!(scrub("api_key = shortkey"), "api_key = [redacted]");
        assert_eq!(
            scrub("Authorization:\tBearer shortkey"),
            "Authorization:\tBearer [redacted]"
        );
        assert_eq!(
            scrub("authorization\tBasic shortkey"),
            "authorization\tBasic [redacted]"
        );
        assert_eq!(scrub("token refused"), "token refused");
        assert_eq!(
            scrub("key 0123456789abcdef0123456789abcdef0123 refused"),
            "key [redacted] refused"
        );
        assert_eq!(
            scrub("x-api-sig aB3dE6gH9jK2mN5pQ8rS1tU4vW7yZ0bC/dE3fG6hJ9 end"),
            "x-api-sig [redacted] end"
        );
        // A file path or a plain message stays readable.
        assert_eq!(
            scrub("at /app/services/exchange-server/src/remote-signer.rs:62"),
            "at /app/services/exchange-server/src/remote-signer.rs:62"
        );
        assert_eq!(
            scrub("signing_mode=hosted_cosign"),
            "signing_mode=hosted_cosign"
        );
        assert!(scrub(&"a ".repeat(400)).chars().count() <= MAX_TEXT + 1);
    }

    #[test]
    fn only_allowlisted_fields_leave_and_text_is_scrubbed() {
        let events = captured(Identity::default(), || {
            let mut event = report_event(&Report {
                kind: "account.start_failed",
                summary: "account start failed",
                user_tag: Some("a1b2c3d4e5f6".into()),
                ..Default::default()
            });
            event.message = Some("POST https://api.okx.com/x?sig=S failed: secret=s3cr3t".into());
            event.fingerprint = vec!["api_key=fingerprint-key".into()].into();
            let mut frame = Frame {
                function: Some("engine::step".into()),
                lineno: Some(7),
                ..Default::default()
            };
            frame.vars.insert("secret".into(), "local-key".into());
            frame.context_line = Some("let key = \"local-key\";".into());
            event.exception = vec![Exception {
                ty: "panic".into(),
                stacktrace: Some(Stacktrace {
                    frames: vec![frame],
                    ..Default::default()
                }),
                mechanism: Some(Mechanism {
                    ty: "panic".into(),
                    description: Some("api_key=k".into()),
                    data: [("body".to_owned(), "{}".into())].into_iter().collect(),
                    ..Default::default()
                }),
                ..Default::default()
            }]
            .into();
            event.tags.insert("account".into(), "Main account".into());
            event
                .extra
                .insert("payload".into(), "{\"orders\":[]}".into());
            event.user = Some(sentry::User {
                id: Some("raw-user-id".into()),
                ..Default::default()
            });
            event.request = Some(sentry::protocol::Request {
                url: "https://x/y?key=k".parse().ok(),
                ..Default::default()
            });
            sentry::capture_event(event);
        });
        let event = &events[0];
        assert_eq!(
            event.message.as_deref(),
            Some("POST https://api.okx.com/x?[redacted] failed: secret=[redacted]")
        );
        assert!(!event.tags.contains_key("account"));
        assert_eq!(event.extra.keys().collect::<Vec<_>>(), ["user_tag"]);
        assert!(event.user.is_none() && event.request.is_none());
        assert_eq!(event.fingerprint.as_ref(), ["api_key=[redacted]"]);
        let exception = &event.exception.values[0];
        let frame = &exception.stacktrace.as_ref().unwrap().frames[0];
        assert_eq!(frame.function.as_deref(), Some("engine::step"));
        assert_eq!(frame.lineno, Some(7));
        assert!(frame.vars.is_empty() && frame.context_line.is_none());
        let mechanism = exception.mechanism.as_ref().unwrap();
        assert!(mechanism.description.is_none() && mechanism.data.is_empty());
    }

    #[test]
    fn the_tag_allowlist_is_exactly_this_closed_set() {
        assert_eq!(
            TAGS,
            [
                "service",
                "box",
                "kind",
                "error_kind",
                "venue_code",
                "venue",
                "target",
                "startup_unloaded",
                "startup_refused",
            ]
        );
        assert_eq!(EXTRAS, ["user_tag", "suppressed_since_last_event"]);
    }

    #[test]
    fn a_start_failure_carries_its_cause_tags_and_nothing_else_leaves() {
        use safe_cex_native::report::{ErrorKind, ErrorReport};
        let events = captured(Identity::default(), || {
            let mut event = from_native(&ErrorReport {
                kind: ErrorKind::AccountStartFailed,
                label: Some("a1b2c3 Main account binance"),
                target: None,
                error_kind: Some("authInvalid"),
                venue_code: Some("-2015"),
                venue: None,
                startup_unloaded: Some("balance"),
                startup_refused: Some(true),
            })
            .event();
            for (key, value) in [
                ("account", "Main account"),
                ("user_id", "auth0|opaque-user"),
                ("http_status", "401"),
                ("message", "Invalid API-key, IP, or permissions for action"),
                ("api_key", "k3y"),
            ] {
                event.tags.insert(key.into(), value.into());
            }
            event.extra.insert("account".into(), "Main account".into());
            sentry::capture_event(event);
        });
        let tags: Vec<_> = events[0]
            .tags
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            tags,
            [
                ("error_kind", "authInvalid"),
                ("kind", "account.start_failed"),
                ("service", "exchange-server"),
                ("startup_refused", "true"),
                ("startup_unloaded", "balance"),
                ("venue", "binance"),
                ("venue_code", "-2015"),
            ]
        );
        assert_eq!(events[0].extra.keys().collect::<Vec<_>>(), ["user_tag"]);
        assert_eq!(events[0].message.as_deref(), Some("account start failed"));
    }

    #[test]
    fn a_report_carries_its_summary_never_error_text() {
        let event = report_event(&Report {
            kind: "account.start_failed",
            summary: "account start failed",
            error_kind: Some("auth"),
            venue_code: Some("10003"),
            ..Default::default()
        });
        assert_eq!(event.message.as_deref(), Some("account start failed"));
        assert_eq!(event.fingerprint.as_ref(), ["account.start_failed", "auth"]);
    }

    #[test]
    fn throttles_per_fingerprint_and_globally() {
        let mut throttle = Throttle::default();
        let start = Instant::now();
        assert_eq!(throttle.claim("a", start), Some(0));
        assert_eq!(throttle.claim("a", start + Duration::from_secs(1)), None);
        assert_eq!(throttle.claim("a", start + Duration::from_secs(2)), None);
        // The next one after the window reports what was held back.
        assert_eq!(throttle.claim("a", start + PER_KEY_WINDOW), Some(2));

        let mut throttle = Throttle::default();
        for i in 0..GLOBAL_PER_MINUTE {
            assert!(throttle.claim(&format!("k{i}"), start).is_some());
        }
        assert_eq!(throttle.claim("storm", start), None);
        assert_eq!(
            throttle.claim("storm", start + Duration::from_secs(61)),
            Some(1)
        );

        // A storm of distinct fingerprints past the global limit stays bounded.
        let mut throttle = Throttle::default();
        for i in 0..2_000 {
            let _ = throttle.claim(&format!("condition-{i}"), start);
        }
        assert!(throttle.keys.len() <= MAX_TRACKED_KEYS);

        // Through the client: a repeat inside the window never reaches the transport.
        let events = captured(Identity::default(), || {
            for _ in 0..5 {
                sentry::capture_event(report_event(&Report {
                    kind: "drain.deadline_passed",
                    summary: "drain passed its deadline",
                    ..Default::default()
                }));
            }
        });
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn a_panic_sends_its_location_never_its_payload() {
        let events = captured(Identity::default(), || {
            let result = std::panic::catch_unwind(|| {
                panic!(
                    r#"request body: {{"userId":"auth0|user","orders":[{{"symbol":"BTCUSDT"}}]}}"#
                );
            });
            assert!(result.is_err());
        });
        assert_eq!(events.len(), 1, "{events:?}");
        let exception = &events[0].exception.values[0];
        let value = exception.value.as_deref().unwrap();
        assert!(
            value.starts_with("panic at ") && value.contains("telemetry.rs:"),
            "{value}"
        );
        assert!(!value.contains("auth0") && !value.contains("BTCUSDT"));
        assert_eq!(events[0].fingerprint[0], "panic");
        assert_eq!(events[0].level, Level::Fatal);
    }

    #[test]
    fn a_caught_panic_never_flushes_on_the_panicking_thread() {
        use std::sync::atomic::AtomicUsize;
        static FLUSHES: AtomicUsize = AtomicUsize::new(0);
        struct StalledTransport;
        impl sentry::Transport for StalledTransport {
            fn send_envelope(&self, _: sentry::Envelope) {}
            fn flush(&self, _: Duration) -> bool {
                FLUSHES.fetch_add(1, Ordering::SeqCst);
                false
            }
        }
        let mut options = client_options(DSN, Identity::default()).unwrap();
        options.transport = Some(Arc::new(Arc::new(StalledTransport)));
        let hub = Arc::new(sentry::Hub::new(
            Some(Arc::new(sentry::Client::from(options))),
            Arc::new(Default::default()),
        ));
        sentry::Hub::run(hub, || {
            let _ = std::panic::catch_unwind(|| panic!("caught on an engine thread"));
        });
        assert_eq!(FLUSHES.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transport_reports_carry_their_venue_and_route_but_no_url() {
        use safe_cex_native::report::{ErrorKind, ErrorReport};
        let events = captured(Identity::default(), || {
            for (venue, target) in [
                (Some("bybitdemo"), "api-demo.bybit.com direct"),
                (None, "ws.okx.com"),
            ] {
                sentry::capture_event(
                    from_native(&ErrorReport {
                        kind: ErrorKind::RequestFailed,
                        label: None,
                        target: Some(target),
                        error_kind: None,
                        venue_code: None,
                        venue,
                        startup_unloaded: None,
                        startup_refused: None,
                    })
                    .event(),
                );
            }
        });
        let tags = |event: &Event<'static>| {
            event
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            tags(&events[0]),
            [
                ("kind".to_owned(), "transport.request_failed".to_owned()),
                ("service".to_owned(), "exchange-server".to_owned()),
                ("target".to_owned(), "api-demo.bybit.com direct".to_owned()),
                ("venue".to_owned(), "bybitdemo".to_owned()),
            ]
        );
        assert!(!events[1].tags.contains_key("venue"));
        // An account's start failure still takes its venue from the label.
        let start = from_native(&ErrorReport {
            kind: ErrorKind::AccountStartFailed,
            label: Some("a1b2c3 Main account kraken"),
            target: None,
            error_kind: Some("auth"),
            venue_code: None,
            venue: None,
            startup_unloaded: None,
            startup_refused: None,
        });
        assert_eq!(start.venue, Some("kraken"));
        assert_eq!(start.user_tag.as_deref(), Some("a1b2c3"));
    }
}
