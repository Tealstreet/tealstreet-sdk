use crate::{TransportError, TransportFuture};
use std::{
    collections::BTreeMap,
    future::Future,
    pin::{Pin, pin},
    rc::Rc,
    task::Poll,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketRuntime {
    Browser,
    BrowserWorker,
    Native,
    NodeWs,
    BunWs,
    BunNative,
}

#[derive(Debug, Clone, Copy)]
pub struct SocketCapabilities {
    pub runtime: SocketRuntime,
    pub upgrade_headers: bool,
    pub alpn: bool,
}

/// Validated dial policy. Hosts may inspect it but cannot forge or edit it.
/// ```compile_fail
/// use safe_cex_transport::socket::TunnelDialOptions;
/// let _bypass = TunnelDialOptions { headers: Default::default(), alpn_protocols: vec![], protocols: vec![] };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelDialOptions {
    headers: BTreeMap<String, String>,
    alpn_protocols: Vec<String>,
    protocols: Vec<String>,
}

impl TunnelDialOptions {
    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn alpn_protocols(&self) -> &[String] {
        &self.alpn_protocols
    }

    pub fn protocols(&self) -> &[String] {
        &self.protocols
    }

    pub(crate) fn overlay(
        &mut self,
        headers: indexmap::IndexMap<String, String>,
        protocols: Vec<String>,
    ) {
        let policy = std::mem::take(&mut self.headers);
        self.headers.extend(headers);
        for (name, value) in policy {
            self.headers
                .retain(|key, _| !key.eq_ignore_ascii_case(&name));
            self.headers.insert(name, value);
        }
        self.protocols = protocols;
    }

    pub(crate) fn new(capabilities: SocketCapabilities) -> Result<Self, TransportError> {
        if matches!(
            capabilities.runtime,
            SocketRuntime::Browser | SocketRuntime::BrowserWorker
        ) {
            return Ok(Self {
                headers: BTreeMap::new(),
                alpn_protocols: vec![],
                protocols: vec![],
            });
        }
        if capabilities.runtime == SocketRuntime::BunNative
            || !capabilities.upgrade_headers
            || !capabilities.alpn
        {
            return Err(TransportError::UnavailableTransport("tradetunnel requires upgrade headers and HTTP/1.1 ALPN; Bun native WebSocket is forbidden".into()));
        }
        Ok(Self {
            headers: BTreeMap::from([
                ("Origin".into(), "https://preview.tealstreet.io".into()),
                ("User-Agent".into(), "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36".into()),
            ]),
            alpn_protocols: vec!["http/1.1".into()],
            protocols: vec![],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketFrame {
    Text(String),
    Binary(Vec<u8>),
    Close {
        code: u16,
        reason: String,
        was_clean: bool,
    },
}

// Host bindings implement I/O; all protocol decisions remain in the Rust client.
pub trait TunnelSocket {
    /// DOM readyState; override when host state can change before its close event is received.
    fn ready_state(&self) -> u16 {
        1
    }

    fn buffered_amount(&self) -> u64 {
        0
    }
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError>;
    fn receive(&self) -> TransportFuture<'static, SocketFrame>;
    fn close(&self, code: Option<u16>, reason: &str) -> Result<(), TransportError>;
}

pub trait TunnelTask {
    fn cancel(&self);
}

pub trait TunnelHost {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask>;
    fn capabilities(&self) -> SocketCapabilities;
    fn now_ms(&self) -> u64;
    fn sleep_until(&self, deadline_ms: u64) -> TransportFuture<'static, ()>;
    fn connect(
        &self,
        url: String,
        options: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>>;
    fn proxy_token(&self) -> TransportFuture<'_, String>;
    fn is_token_expired(&self, token: &str) -> bool;
    fn sync_server_time(&self, server_time: u64);
}

pub(crate) async fn within_deadline<T>(
    host: &dyn TunnelHost,
    deadline: u64,
    future: impl Future<Output = Result<T, TransportError>>,
) -> Result<T, TransportError> {
    let mut future = pin!(future);
    let mut timeout = host.sleep_until(deadline);
    std::future::poll_fn(|cx| {
        if host.now_ms() >= deadline || timeout.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(TransportError::Timeout));
        }
        future.as_mut().poll(cx)
    })
    .await
}

pub(crate) fn proxy_token(host: &dyn TunnelHost) -> TransportFuture<'_, String> {
    crate::how_config::configured_proxy_token().unwrap_or_else(|| host.proxy_token())
}
pub(crate) fn token_expired(host: &dyn TunnelHost, token: &str) -> Result<bool, TransportError> {
    crate::how_config::configured_token_expired(token)
        .unwrap_or_else(|| Ok(host.is_token_expired(token)))
}

pub(crate) async fn valid_token(
    host: &dyn TunnelHost,
    deadline: u64,
) -> Result<String, TransportError> {
    for attempt in 0..120 {
        let token = within_deadline(host, deadline, proxy_token(host)).await?;
        if !token.is_empty() && !token_expired(host, &token)? {
            return Ok(token);
        }
        if attempt < 119 {
            within_deadline(
                host,
                deadline,
                host.sleep_until(host.now_ms().saturating_add(500)),
            )
            .await?;
        }
    }
    within_deadline(host, deadline, proxy_token(host)).await
}

pub(crate) async fn race_deadline<T>(
    timer: TransportFuture<'static, ()>,
    future: TransportFuture<'static, T>,
) -> Result<T, TransportError> {
    let (mut timer, mut future) = (timer, future);
    std::future::poll_fn(|cx| {
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(TransportError::Timeout));
        }
        future.as_mut().poll(cx)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_policy_fails_closed_and_workers_use_browser_policy() {
        for runtime in [
            SocketRuntime::Native,
            SocketRuntime::NodeWs,
            SocketRuntime::BunWs,
        ] {
            for (headers, alpn) in [(false, true), (true, false)] {
                assert!(matches!(
                    TunnelDialOptions::new(SocketCapabilities {
                        runtime,
                        upgrade_headers: headers,
                        alpn
                    }),
                    Err(TransportError::UnavailableTransport(_))
                ));
            }
        }
        assert!(
            TunnelDialOptions::new(SocketCapabilities {
                runtime: SocketRuntime::BunNative,
                upgrade_headers: true,
                alpn: true
            })
            .is_err()
        );
        for runtime in [SocketRuntime::Browser, SocketRuntime::BrowserWorker] {
            let options = TunnelDialOptions::new(SocketCapabilities {
                runtime,
                upgrade_headers: false,
                alpn: false,
            })
            .unwrap();
            assert!(options.headers.is_empty());
            assert!(options.alpn_protocols.is_empty());
        }
    }
}

#[cfg(test)]
mod deadline_and_diagnostic_tests {
    use super::*;
    use std::{
        cell::Cell,
        task::{Context, Waker},
    };

    struct LaggingTimerHost;
    impl TunnelHost for LaggingTimerHost {
        fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
            unreachable!()
        }
        fn capabilities(&self) -> SocketCapabilities {
            unreachable!()
        }
        fn now_ms(&self) -> u64 {
            20
        }
        fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
            Box::pin(std::future::pending())
        }
        fn connect(
            &self,
            _: String,
            _: TunnelDialOptions,
        ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
            unreachable!()
        }
        fn proxy_token(&self) -> TransportFuture<'_, String> {
            unreachable!()
        }
        fn is_token_expired(&self, _: &str) -> bool {
            unreachable!()
        }
        fn sync_server_time(&self, _: u64) {
            unreachable!()
        }
    }

    #[test]
    fn elapsed_deadline_never_polls_work_even_when_the_timer_has_not_fired() {
        for (deadline, expected, polls) in [
            (19, Err(TransportError::Timeout), 0),
            (20, Err(TransportError::Timeout), 0),
            (21, Ok(7), 1),
        ] {
            let polled = Cell::new(0);
            let work = std::future::poll_fn(|_| {
                polled.set(polled.get() + 1);
                Poll::Ready(Ok(7))
            });
            let mut future = pin!(within_deadline(&LaggingTimerHost, deadline, work));
            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(future.as_mut().poll(&mut context), Poll::Ready(expected));
            assert_eq!(polled.get(), polls);
        }
    }

    #[test]
    fn dial_options_doctest_reports_private_fields_instead_of_an_unrelated_error() {
        let result = std::process::Command::new("node")
            .args(["scripts/site-id-diagnostic.mjs", "--dial-options"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output()
            .expect("Node is required for the dial diagnostic gate");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
