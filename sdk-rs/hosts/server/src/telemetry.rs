//! Sentry error capture for the native exchange server, with the semantics of
//! `packages/service-telemetry` (`initServiceSentry`): off unless
//! `EXCHANGE_SERVER_SENTRY_DSN` is set; `server_name` and a `box` tag from
//! `TEALSTREET_BOX`; `release` from `TEALSTREET_RELEASE`, omitted when unset or
//! a placeholder; no region and no SDK inference of host, release or
//! environment.
//!
//! This process holds users' exchange credentials, so every event is rebuilt
//! from an allowlist in `before_send`: a scrubbed message or exception, a
//! fingerprint, and the tags and extras named below. Nothing else (request
//! data, user, contexts, breadcrumbs) leaves. Events are throttled per
//! fingerprint and globally, and sent from Sentry's own transport thread
//! through a bounded queue that drops when full, so a failure storm can never
//! slow a trading path.
use sentry::protocol::{Event, Exception, Level, Value, Values};
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

/// A credential named by its key (`api_key=…`, `"secret": "…"`,
/// `Authorization: Bearer …`) loses its value.
fn redact_named(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut word = String::new();
    while let Some((_, c)) = chars.next() {
        out.push(c);
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
            word.push(c);
            continue;
        }
        let named = sensitive_key(&word);
        word.clear();
        if !named || !matches!(c, '=' | ':' | '"') {
            continue;
        }
        // Past the separator, quotes and spaces, and an auth scheme word.
        while let Some(&(_, next)) = chars.peek() {
            if matches!(next, '=' | ':' | '"' | '\'' | ' ') {
                out.push(next);
                chars.next();
            } else {
                break;
            }
        }
        let value: String = chars
            .clone()
            .map(|(_, c)| c)
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '&' | ',' | ';' | '}'))
            .collect();
        let scheme = ["bearer", "basic"].contains(&value.to_ascii_lowercase().as_str());
        let mut skip = value.chars().count();
        if scheme {
            out.push_str(&value);
            out.push(' ');
            skip += 1;
            let token: String = chars
                .clone()
                .skip(skip)
                .map(|(_, c)| c)
                .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ',' | ';'))
                .collect();
            skip += token.chars().count();
        }
        if skip > 0 {
            for _ in 0..skip {
                chars.next();
            }
            out.push_str("[redacted]");
        }
    }
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
                ty: exception.ty,
                value: exception.value.as_deref().map(scrub),
                stacktrace: exception.stacktrace,
                mechanism: exception.mechanism,
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    );
    Event {
        event_id: event.event_id,
        timestamp: event.timestamp,
        level: event.level,
        platform: event.platform,
        fingerprint: event.fingerprint,
        message: event.message.as_deref().map(scrub),
        exception,
        tags,
        extra,
        release: identity.release.clone().map(Into::into),
        server_name: identity.box_label.clone().map(Into::into),
        ..Default::default()
    }
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
/// breadcrumbs or stack attachment), the panic hook, and the allowlist.
pub fn client_options(dsn: &str, identity: Identity) -> Option<sentry::ClientOptions> {
    let dsn = dsn.trim().parse().ok()?;
    let throttle = Arc::new(Mutex::new(Throttle::default()));
    let send = identity.clone();
    let mut options = sentry::ClientOptions::default();
    options.dsn = Some(dsn);
    options.release = identity.release.clone().map(Into::into);
    options.server_name = identity.box_label.clone().map(Into::into);
    options.default_integrations = false;
    options.integrations = vec![Arc::new(
        sentry::integrations::panic::PanicIntegration::default(),
    )];
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
/// flushes for up to two seconds.
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
/// name, URL or venue payload (those go in the journal line only).
#[derive(Default)]
pub struct Report<'a> {
    pub kind: &'static str,
    pub summary: &'a str,
    pub user_tag: Option<String>,
    pub venue: Option<&'a str>,
    pub target: Option<&'a str>,
    pub error_kind: Option<&'a str>,
    pub venue_code: Option<&'a str>,
    /// Raw error text; scrubbed before it leaves.
    pub detail: Option<&'a str>,
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
        ] {
            if let Some(value) = value {
                tags.insert(key.to_owned(), value.to_owned());
            }
        }
        let mut extra = BTreeMap::new();
        if let Some(user_tag) = &self.user_tag {
            extra.insert("user_tag".to_owned(), Value::String(user_tag.clone()));
        }
        let message = match self.detail {
            Some(detail) => format!("{}: {detail}", self.summary),
            None => self.summary.to_owned(),
        };
        // One issue per condition and target, not per venue message.
        let fingerprint: Vec<String> = [Some(self.kind), self.target, self.error_kind]
            .into_iter()
            .flatten()
            .map(|part| scrub(part).chars().take(120).collect())
            .collect();
        Event {
            level: Level::Error,
            message: Some(message),
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
    use safe_cex_native::report::ErrorKind;
    // The label is `user_tag account venue`; the account name stays in the journal.
    let words: Vec<&str> = report
        .label
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let summary = match report.kind {
        ErrorKind::AccountStartFailed => "account start failed",
        ErrorKind::SocketOpenFailed => "venue, relay or tunnel socket did not open",
        ErrorKind::RequestFailed => "venue request failed in transport",
    };
    capture(&Report {
        kind: report.kind.as_str(),
        summary,
        user_tag: (words.len() >= 2).then(|| words[0].to_owned()),
        venue: (words.len() >= 2).then(|| words[words.len() - 1]),
        target: report.target,
        error_kind: report.error_kind,
        venue_code: report.venue_code,
        detail: Some(report.detail),
    });
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
                detail: Some("POST https://api.okx.com/x?sig=S failed: secret=s3cr3t"),
                ..Default::default()
            });
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
            Some(
                "account start failed: POST https://api.okx.com/x?[redacted] failed: secret=[redacted]"
            )
        );
        assert!(!event.tags.contains_key("account"));
        assert_eq!(event.extra.keys().collect::<Vec<_>>(), ["user_tag"]);
        assert!(event.user.is_none() && event.request.is_none());
        assert_eq!(event.fingerprint.as_ref(), ["account.start_failed"]);
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
    fn a_panic_is_captured_scrubbed() {
        let events = captured(Identity::default(), || {
            let result = std::panic::catch_unwind(|| {
                panic!("venue refused secret=hunter2 at https://api.x.com/a?sign=s");
            });
            assert!(result.is_err());
        });
        assert_eq!(events.len(), 1, "{events:?}");
        let exception = &events[0].exception.values[0];
        assert_eq!(
            exception.value.as_deref(),
            Some("venue refused secret=[redacted] at https://api.x.com/a?[redacted]")
        );
        assert_eq!(events[0].level, Level::Fatal);
    }
}
