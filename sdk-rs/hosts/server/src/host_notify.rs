//! Box -> gateway host notifications for DELEGATED residents (AGT-37).
//! Every serialized field is typed or shape-checked, so keys, tokens, order ids,
//! prices, sizes, balances, labels and venue text cannot reach a notification.
//! See `DELEGATED_RETENTION.md` "Host notification lane".
use serde::Serialize;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Oldest notifications are dropped past this many queued.
pub const MAX_OUTBOX: usize = 1000;
pub const MAX_BATCH: usize = 50;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const FIRST_BACKOFF: Duration = Duration::from_secs(1);
pub const MAX_BACKOFF: Duration = Duration::from_secs(600);
/// Drain/SIGTERM waits at most this long for queued notifications.
pub const RESTART_FLUSH: Duration = Duration::from_secs(3);
/// `delegated.shutdownWarning` fires this long before the retention deadline.
pub const WARNING_LEAD: Duration = Duration::from_secs(60 * 60);

const ACCOUNT_DOMAIN: &[u8] = b"tealstreet/host-notify/account/v1";
const MAX_LABEL: usize = 64;

/// Startup-only environment. An unset URL turns the whole lane off.
#[derive(Clone)]
pub struct Config {
    url: reqwest::Url,
    token: String,
    box_id: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("url", &self.url.as_str())
            .field("token", &"<redacted>")
            .field("box_id", &self.box_id)
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Option<Self>, String> {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self::parse(
            var("TEALSTREET_HOST_NOTIFY_URL").as_deref(),
            var("TEALSTREET_HOST_NOTIFY_TOKEN").as_deref(),
            var("TEALSTREET_BOX").as_deref(),
            var("TEALSTREET_ROUTER_BOX_ID").as_deref(),
        )
    }

    pub fn parse(
        url: Option<&str>,
        token: Option<&str>,
        box_label: Option<&str>,
        router_box: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let Some(url) = url else {
            return Ok(None);
        };
        let invalid_url = || {
            "TEALSTREET_HOST_NOTIFY_URL must be an https URL (http only for loopback)".to_owned()
        };
        let url = reqwest::Url::parse(url).map_err(|_| invalid_url())?;
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        if !(url.scheme() == "https" || url.scheme() == "http" && loopback)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid_url());
        }
        let token = token
            .filter(|token| {
                token.len() <= 4096 && token.bytes().all(|byte| byte.is_ascii_graphic())
            })
            .ok_or("TEALSTREET_HOST_NOTIFY_TOKEN must be set to a visible-ASCII token when TEALSTREET_HOST_NOTIFY_URL is set")?;
        let box_id = box_label
            .or(router_box)
            .and_then(label)
            .ok_or("TEALSTREET_BOX or TEALSTREET_ROUTER_BOX_ID must name this box when TEALSTREET_HOST_NOTIFY_URL is set")?;
        Ok(Some(Self {
            url,
            token: token.to_owned(),
            box_id,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    #[serde(rename = "fill")]
    Fill,
    #[serde(rename = "orderRejected")]
    OrderRejected,
    #[serde(rename = "algoStopped")]
    AlgoStopped,
    #[serde(rename = "delegated.shutdownWarning")]
    ShutdownWarning,
    #[serde(rename = "delegated.shutdown")]
    Shutdown,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::OrderRejected => "orderRejected",
            Self::AlgoStopped => "algoStopped",
            Self::ShutdownWarning => "delegated.shutdownWarning",
            Self::Shutdown => "delegated.shutdown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verb {
    Filled,
    PartiallyFilled,
    Rejected,
    Stopped,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Algo {
    Chase,
    Twap,
    Exec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Cause {
    #[serde(rename = "noClient48h")]
    NoClient48h,
    #[serde(rename = "proxyTokenExpired")]
    ProxyTokenExpired,
    #[serde(rename = "serverRestart")]
    ServerRestart,
    #[serde(rename = "algoError")]
    AlgoError,
    #[serde(rename = "algoFinished")]
    AlgoFinished,
    #[serde(rename = "venueRejected")]
    VenueRejected,
}

/// The contract's allowlisted `fields`. Only a shape-checked symbol is text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Fields {
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<Side>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verb: Option<Verb>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo: Option<Algo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<Cause>,
}

impl Fields {
    /// A symbol that is not a plain market name is omitted, never truncated.
    pub fn symbol(mut self, symbol: &str) -> Self {
        self.symbol = (!symbol.is_empty()
            && symbol.len() <= 40
            && symbol
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_/.:".contains(&byte)))
        .then(|| symbol.to_owned());
        self
    }
    pub fn side(mut self, side: Side) -> Self {
        self.side = Some(side);
        self
    }
    pub fn verb(mut self, verb: Verb) -> Self {
        self.verb = Some(verb);
        self
    }
    pub fn algo(mut self, algo: Algo) -> Self {
        self.algo = Some(algo);
        self
    }
    pub fn cause(mut self, cause: Cause) -> Self {
        self.cause = Some(cause);
        self
    }
}

/// Which account a notification is scoped to. The digest is opaque: the
/// gateway stores it for scoping and never pushes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub user: String,
    pub account_digest: String,
    pub venue: String,
}

impl Scope {
    /// `registry_name` is the identity digest or the label; only its hash leaves the box.
    pub fn new(user: &str, venue: &str, registry_name: &str) -> Self {
        Self {
            user: user.to_owned(),
            account_digest: hex(&length_prefixed(
                ACCOUNT_DOMAIN,
                &[user, venue, registry_name],
            )),
            venue: label(venue).unwrap_or_else(|| "unknown".to_owned()),
        }
    }
}

/// What an emit site supplies; the scope, presence and clock are added on build.
pub struct Draft {
    pub kind: Kind,
    pub severity: Severity,
    /// Unique per event and stable across retries; hashed, never sent.
    pub natural_key: String,
    pub fields: Fields,
    pub deadline: Option<u64>,
    /// Opaque collapse suffix; `None` collapses per account.
    pub collapse: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostNotification {
    pub id: String,
    pub user: String,
    pub account_digest: String,
    pub venue: String,
    pub kind: Kind,
    pub severity: Severity,
    pub occurred_at: u64,
    pub client_present: bool,
    pub collapse_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline: Option<u64>,
    pub fields: Fields,
}

impl HostNotification {
    pub fn build(scope: &Scope, draft: Draft, client_present: bool, occurred_at: u64) -> Self {
        let suffix = draft
            .collapse
            .unwrap_or_else(|| scope.account_digest[..16].to_owned());
        Self {
            id: notification_id(
                &scope.user,
                &scope.account_digest,
                draft.kind,
                &draft.natural_key,
            ),
            user: scope.user.clone(),
            account_digest: scope.account_digest.clone(),
            venue: scope.venue.clone(),
            kind: draft.kind,
            severity: draft.severity,
            occurred_at,
            client_present,
            collapse_key: format!("{}:{suffix}", draft.kind.as_str()),
            deadline: draft.deadline,
            fields: draft.fields,
        }
    }
}

fn retention_cause(proxy_expired: bool) -> Cause {
    if proxy_expired {
        Cause::ProxyTokenExpired
    } else {
        Cause::NoClient48h
    }
}

/// Once per retention interval, keyed by its deadline.
pub fn shutdown_warning(proxy_expired: bool, deadline_ms: u64) -> Draft {
    Draft {
        kind: Kind::ShutdownWarning,
        severity: Severity::Warning,
        natural_key: deadline_ms.to_string(),
        fields: Fields::default().cause(retention_cause(proxy_expired)),
        deadline: Some(deadline_ms),
        collapse: None,
    }
}

/// Retention expiry, keyed by cause and the interval's deadline.
pub fn retention_shutdown(proxy_expired: bool, deadline_ms: u64) -> Draft {
    let name = if proxy_expired {
        "proxyTokenExpired"
    } else {
        "noClient48h"
    };
    Draft {
        kind: Kind::Shutdown,
        severity: Severity::Critical,
        natural_key: format!("{name}:{deadline_ms}"),
        fields: Fields::default().cause(retention_cause(proxy_expired)),
        deadline: None,
        collapse: None,
    }
}

/// Drain/SIGTERM disposal, keyed by this process incarnation.
pub fn restart_shutdown() -> Draft {
    Draft {
        kind: Kind::Shutdown,
        severity: Severity::Critical,
        natural_key: format!("serverRestart:{}", crate::state::boot_id()),
        fields: Fields::default().cause(Cause::ServerRestart),
        deadline: None,
        collapse: None,
    }
}

/// `sha256(user \0 accountDigest \0 kind \0 naturalKey)` as 64 lowercase hex digits.
pub fn notification_id(user: &str, account_digest: &str, kind: Kind, natural_key: &str) -> String {
    let input = [user, account_digest, kind.as_str(), natural_key].join("\0");
    hex(&safe_cex_crypto::sha256(input.as_bytes()))
}

/// An opaque 16-hex collapse/correlation key for a value that must never be sent.
pub fn opaque_key(account_digest: &str, value: &str) -> String {
    hex(&length_prefixed(ACCOUNT_DOMAIN, &[account_digest, value]))[..16].to_owned()
}

fn length_prefixed(domain: &[u8], parts: &[&str]) -> [u8; 32] {
    let mut input = domain.to_vec();
    for part in parts {
        input.extend_from_slice(&(part.len() as u64).to_be_bytes());
        input.extend_from_slice(part.as_bytes());
    }
    safe_cex_crypto::sha256(&input)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn label(text: &str) -> Option<String> {
    (!text.is_empty()
        && text.len() <= MAX_LABEL
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)))
    .then(|| text.to_owned())
}

#[derive(Serialize)]
struct Body<'a> {
    v: u8,
    notifications: &'a [HostNotification],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    Accepted,
    /// Network failure (`None`) or a status worth retrying.
    Retry(Option<u16>),
    /// The gateway can never accept this batch; it is dropped.
    Refused(u16),
}

fn classify(status: u16) -> Delivery {
    match status {
        200..=299 => Delivery::Accepted,
        400 | 413 | 422 => Delivery::Refused(status),
        _ => Delivery::Retry(Some(status)),
    }
}

pub(crate) type Transport =
    Arc<dyn Fn(Vec<u8>) -> Pin<Box<dyn Future<Output = Delivery> + Send>> + Send + Sync>;

/// Its own client: venue egress, proxies and their pacing are never involved.
fn http_transport(config: &Config) -> Result<Transport, String> {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| "host notification HTTP client unavailable".to_owned())?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", config.token))
        .map_err(|_| "TEALSTREET_HOST_NOTIFY_TOKEN is not a valid header value".to_owned())?;
    authorization.set_sensitive(true);
    let box_id = HeaderValue::from_str(&config.box_id)
        .map_err(|_| "host notification box id is not a valid header value".to_owned())?;
    let url = config.url.clone();
    Ok(Arc::new(move |body| {
        let request = client
            .post(url.clone())
            .header(AUTHORIZATION, authorization.clone())
            .header("x-tealstreet-box", box_id.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        Box::pin(async move {
            // The error is never logged: it can carry the URL.
            match request.send().await {
                Ok(response) => classify(response.status().as_u16()),
                Err(_) => Delivery::Retry(None),
            }
        })
    }))
}

#[derive(Default)]
struct Queue {
    items: VecDeque<HostNotification>,
    in_flight: usize,
    dropped: u64,
}

impl Queue {
    fn push(&mut self, notification: HostNotification) {
        if self.items.len() >= MAX_OUTBOX {
            self.items.pop_front();
            self.dropped += 1;
        }
        self.items.push_back(notification);
    }

    fn take_batch(&mut self) -> Vec<HostNotification> {
        let count = self.items.len().min(MAX_BATCH);
        self.in_flight = count;
        self.items.drain(..count).collect()
    }

    /// A failed batch goes back in front, in order; the cap still drops oldest.
    fn requeue(&mut self, batch: Vec<HostNotification>) {
        for notification in batch.into_iter().rev() {
            self.items.push_front(notification);
        }
        while self.items.len() > MAX_OUTBOX {
            self.items.pop_front();
            self.dropped += 1;
        }
        self.in_flight = 0;
    }

    fn idle(&self) -> bool {
        self.items.is_empty() && self.in_flight == 0
    }
}

struct Shared {
    queue: Mutex<Queue>,
    wake: tokio::sync::Notify,
    /// A flush cuts the current retry backoff short.
    urgent: tokio::sync::Notify,
    attempts: tokio::sync::watch::Sender<u64>,
}

/// Bounded at-least-once outbox; the gateway dedupes on `id`.
#[derive(Clone)]
pub struct HostNotifier {
    shared: Arc<Shared>,
}

impl HostNotifier {
    /// Starts the delivery task on the current Tokio runtime.
    pub fn start(config: &Config) -> Result<Self, String> {
        Ok(Self::with_transport(http_transport(config)?))
    }

    pub(crate) fn with_transport(transport: Transport) -> Self {
        let notifier = Self::unstarted();
        tokio::spawn(deliver(notifier.shared.clone(), transport));
        notifier
    }

    pub(crate) fn unstarted() -> Self {
        Self {
            shared: Arc::new(Shared {
                queue: Mutex::default(),
                wake: tokio::sync::Notify::new(),
                urgent: tokio::sync::Notify::new(),
                attempts: tokio::sync::watch::channel(0).0,
            }),
        }
    }

    pub fn push(&self, notification: HostNotification) {
        self.shared.queue.lock().unwrap().push(notification);
        self.shared.wake.notify_one();
    }

    /// True once nothing is queued or in flight; false at the bound. It cuts
    /// one pending backoff short, so a failing gateway is not retried in a loop.
    pub async fn flush(&self, within: Duration) -> bool {
        let mut attempts = self.shared.attempts.subscribe();
        self.shared.urgent.notify_one();
        tokio::time::timeout(within, async {
            loop {
                if self.shared.queue.lock().unwrap().idle() {
                    return;
                }
                if attempts.changed().await.is_err() {
                    return;
                }
            }
        })
        .await
        .is_ok()
    }

    #[cfg(test)]
    pub(crate) fn queued(&self) -> Vec<HostNotification> {
        self.shared
            .queue
            .lock()
            .unwrap()
            .items
            .iter()
            .cloned()
            .collect()
    }
}

async fn deliver(shared: Arc<Shared>, transport: Transport) {
    let mut backoff = FIRST_BACKOFF;
    loop {
        let batch = loop {
            let woken = shared.wake.notified();
            {
                let mut queue = shared.queue.lock().unwrap();
                let dropped = std::mem::take(&mut queue.dropped);
                if dropped > 0 {
                    crate::state::Log::warn(&format!(
                        "host notification outbox full; dropped {dropped} oldest"
                    ));
                }
                if !queue.items.is_empty() {
                    break queue.take_batch();
                }
            }
            woken.await;
        };
        let count = batch.len();
        let body = serde_json::to_vec(&Body {
            v: 1,
            notifications: &batch,
        })
        .expect("host notifications serialize");
        let delivery = transport(body).await;
        let retry = {
            let mut queue = shared.queue.lock().unwrap();
            match delivery {
                Delivery::Accepted => {
                    queue.in_flight = 0;
                    backoff = FIRST_BACKOFF;
                    false
                }
                Delivery::Refused(status) => {
                    queue.in_flight = 0;
                    crate::state::Log::warn(&format!(
                        "host notifications refused by the gateway: status={status} dropped={count}"
                    ));
                    false
                }
                Delivery::Retry(status) => {
                    queue.requeue(batch);
                    crate::state::Log::warn(&format!(
                        "host notification delivery failed: status={} queued={}; retrying in {}s",
                        status.map_or_else(|| "network".to_owned(), |status| status.to_string()),
                        queue.items.len(),
                        backoff.as_secs()
                    ));
                    true
                }
            }
        };
        shared.attempts.send_modify(|attempts| *attempts += 1);
        if retry {
            tokio::select! {
                () = tokio::time::sleep(backoff) => {}
                () = shared.urgent.notified() => {}
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
}

#[cfg(test)]
#[path = "host_notify_tests.rs"]
mod tests;
