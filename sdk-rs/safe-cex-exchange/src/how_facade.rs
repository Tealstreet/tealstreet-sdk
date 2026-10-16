//! safe-cex HOW glue over its actual contextual token, clock and metadata modules.
use crate::{access_token::AccessToken, owned_tasks::OwnedTasks, runtime::ExchangeRuntime};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    TransportError, TransportFuture,
    how_config::{HOWConfigPatch, LatencyEntry, configure_how},
    socket::TunnelHost,
};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Token = Rc<dyn Fn() -> TransportFuture<'static, String>>;
type Expiry = Rc<dyn Fn(&str) -> Result<bool, TransportError>>;
type Domains = Rc<dyn Fn() -> Vec<String>>;
pub struct SafeCexHOW {
    token: Token,
    expiry: Expiry,
    domains: Domains,
    clock: VirtualClock,
    runtime: Rc<dyn ExchangeRuntime>,
    tasks: Rc<OwnedTasks>,
    simulate: bool,
}
impl SafeCexHOW {
    /// Runtime initialization replaces the source module's automatic import call.
    pub fn new(
        access: AccessToken,
        clock: VirtualClock,
        runtime: Rc<dyn ExchangeRuntime>,
        simulation_env: Option<&str>,
    ) -> Self {
        let tasks = Rc::new(OwnedTasks::default());
        let token_tasks = tasks.clone();
        let token_runtime = runtime.clone();
        let token: Token = Rc::new(move || {
            let access = access.clone();
            let mut work = Box::pin(async move {
                access
                    .get_proxy_token()
                    .await
                    .map_err(|e| TransportError::Host(format!("{}: {}", e.name, e.message)))
            });
            // JS starts the async prefix at invocation, even if the caller drops
            // the returned promise. Drive suspended work independently of its observer.
            if let Poll::Ready(value) = work.as_mut().poll(&mut Context::from_waker(Waker::noop()))
            {
                return Box::pin(async move { value });
            }
            let value = Rc::new(RefCell::new(None));
            let waiters = crate::waiters::Waiters::default();
            let observer = waiters.register();
            let output = value.clone();
            let wake = waiters.clone();
            let cancelled_value = value.clone();
            let cancelled_wake = waiters.clone();
            token_tasks.start(
                token_runtime.as_ref(),
                move |_| {
                    Box::pin(async move {
                        *output.borrow_mut() = Some(work.await);
                        wake.wake_all();
                    })
                },
                move |cancelled| {
                    if cancelled {
                        *cancelled_value.borrow_mut() = Some(Err(TransportError::Host(
                            "HOW token operation cancelled".into(),
                        )));
                        cancelled_wake.wake_all();
                    }
                },
            );
            Box::pin(std::future::poll_fn(move |cx| {
                if let Some(value) = value.borrow_mut().take() {
                    Poll::Ready(value)
                } else {
                    // Observer cancellation releases only its registration; the
                    // token task remains owned independently until settlement.
                    observer.update(cx.waker());
                    Poll::Pending
                }
            }))
        });
        let expiry_clock = clock.clone();
        let expiry: Expiry =
            Rc::new(move |token| Ok(crate::proxy_token::is_token_expired(token, &expiry_clock)));
        let this = Self {
            token,
            expiry,
            domains: Rc::new(safe_cex_types::exchange_metadata::get_unique_latency_domains),
            clock,
            runtime,
            tasks,
            simulate: safe_cex_store::config::simulate_http_over_websocket(simulation_env),
        };
        this.initialize_how();
        this
    }
    // ts: initializeHOW, six source bindings. Imported callback identities persist;
    // the source syncServerTime arrow is fresh on each initialization.
    pub fn initialize_how(&self) {
        let clock = self.clock.clone();
        let runtime = self.runtime.clone();
        let tasks = self.tasks.clone();
        let sync = Rc::new(move |time: f64| {
            let clock = clock.clone();
            let mut work =
                Box::pin(async move { clock.sync_bybit_client_server_time(Some(time)).await });
            // Execute the source async method's prefix now; retain suspended fetch.
            if work
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                tasks.start(runtime.as_ref(), |_| work, |_| {});
            }
        });
        configure_how(HOWConfigPatch {
            get_proxy_token: Some(self.token.clone()),
            is_token_expired: Some(self.expiry.clone()),
            simulate_http_over_websocket: Some(self.simulate),
            sync_server_time: Some(Some(sync)),
            get_unique_latency_domains: Some(Some(self.domains.clone())),
            latency_domain_to_exchanges: Some(Some(
                safe_cex_types::exchange_metadata::latency_domain_to_exchanges(),
            )),
            ..Default::default()
        });
    }
}
pub fn get_server_latency_for_exchange(
    host: Rc<dyn TunnelHost>,
    exchange: &str,
) -> Result<Option<LatencyEntry>, TransportError> {
    let metadata = safe_cex_types::exchange_metadata::get_exchange_metadata(exchange, false)
        .map_err(TransportError::Host)?;
    let Some(domain) = metadata.latency_domain.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let socket = safe_cex_transport::how::get_how_websocket_for_exchange(host, exchange)?;
    Ok(socket.get_latency_for_domain(&domain))
}
