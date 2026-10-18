//! Private listen-key HTTP/timer methods. Full socket construction and connection
//! remain separate rows; the parent supplies their real HTTP/reconnect seams.
use safe_cex_exchange::{
    OwnedOperation,
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub trait ListenKeyParent {
    fn get_fapi_path(&self, endpoint: &str) -> String;
    /// Dispatch immediately on invocation, like Axios.
    fn post_listen_key(&self, path: String) -> ExchangeFuture<'static, String>;
    fn put_listen_key(&self, path: String, listen_key: String) -> ExchangeFuture<'static, ()>;
    fn reconnect(&self) -> Result<(), ExchangeError>;
    fn connect_and_subscribe(&self) -> ExchangeFuture<'static, ()>;
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListenKeyState {
    pub listen_key: String,
    pub last_keep_alive_at: f64,
    pub consecutive_failures: f64,
    pub retry_count: f64,
    pub refresh_timer: bool,
    pub watchdog_timer: bool,
    pub connect_timer: bool,
}
pub struct ListenKeyLifecycle {
    owner: Rc<BaseExchange>,
    parent: Rc<dyn ListenKeyParent>,
    listen_key: RefCell<String>,
    last_keep_alive_at: Cell<f64>,
    consecutive_failures: Cell<f64>,
    retry_count: Cell<f64>,
    refresh_timer: RefCell<Option<OwnedOperation>>,
    watchdog_timer: RefCell<Option<OwnedOperation>>,
    connect_timer: RefCell<Option<OwnedOperation>>,
}
impl ListenKeyLifecycle {
    pub fn new(owner: Rc<BaseExchange>, parent: Rc<dyn ListenKeyParent>) -> Rc<Self> {
        Rc::new(Self {
            owner,
            parent,
            listen_key: RefCell::new(String::new()),
            last_keep_alive_at: Cell::new(0.0),
            consecutive_failures: Cell::new(0.0),
            retry_count: Cell::new(0.0),
            refresh_timer: RefCell::new(None),
            watchdog_timer: RefCell::new(None),
            connect_timer: RefCell::new(None),
        })
    }
    pub fn reset_connect_retry_count(&self) {
        self.retry_count.set(0.0);
    }
    pub fn reset_for_dispose(&self) {
        self.listen_key.borrow_mut().clear();
        self.last_keep_alive_at.set(0.0);
        self.consecutive_failures.set(0.0);
        self.retry_count.set(0.0);
        self.clear_listen_key_refresh_timer();
        self.clear_listen_key_watchdog_timer();
        self.clear_connect_retry_timer();
    }
    pub fn state(&self) -> ListenKeyState {
        ListenKeyState {
            listen_key: self.listen_key.borrow().clone(),
            last_keep_alive_at: self.last_keep_alive_at.get(),
            consecutive_failures: self.consecutive_failures.get(),
            retry_count: self.retry_count.get(),
            refresh_timer: self.refresh_timer.borrow().is_some(),
            watchdog_timer: self.watchdog_timer.borrow().is_some(),
            connect_timer: self.connect_timer.borrow().is_some(),
        }
    }
    fn launch(&self, operation: &'static str, work: ExchangeFuture<'static, ()>) {
        let runtime = self.owner.runtime.clone();
        self.owner.start_owned_operation(async move {
            if let Err(error) = work.await {
                runtime
                    .report_exception(&error, &serde_json::json!({"operation":operation}).into());
            }
        });
    }
    // ts: AsterDEXPrivateWebsocket.fetchListenKey
    pub fn fetch_listen_key(self: &Rc<Self>) -> ExchangeFuture<'static, String> {
        let fetching = self
            .parent
            .post_listen_key(self.parent.get_fapi_path("listenKey"));
        let lifecycle = self.clone();
        Box::pin(async move {
            let listen_key = fetching.await?;
            *lifecycle.listen_key.borrow_mut() = listen_key.clone();
            lifecycle.consecutive_failures.set(0.0);
            lifecycle
                .last_keep_alive_at
                .set(lifecycle.owner.runtime.now_ms());
            lifecycle.schedule_listen_key_refresh(1_800_000)?;
            lifecycle.start_listen_key_watchdog()?;
            Ok(listen_key)
        })
    }
    // ts: AsterDEXPrivateWebsocket.updateListenKey
    pub fn update_listen_key(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        if self.owner.is_disposed() {
            return Box::pin(async { Ok(()) });
        }
        let key = self.listen_key.borrow().clone();
        if key.is_empty() {
            let result = (|| {
                self.owner.log(
                    "Missing AsterDEX listen key for keepalive, reconnecting private stream",
                    "warning",
                )?;
                self.parent.reconnect()?;
                self.schedule_listen_key_refresh(30_000)
            })();
            return Box::pin(async move { result });
        }
        let updating = self
            .parent
            .put_listen_key(self.parent.get_fapi_path("listenKey"), key);
        let lifecycle = self.clone();
        Box::pin(async move {
            let result = async {
                updating.await?;
                lifecycle.consecutive_failures.set(0.0);
                lifecycle
                    .last_keep_alive_at
                    .set(lifecycle.owner.runtime.now_ms());
                lifecycle.schedule_listen_key_refresh(1_800_000)
            }
            .await;
            if let Err(error) = result {
                let failures = lifecycle.consecutive_failures.get() + 1.0;
                lifecycle.consecutive_failures.set(failures);
                let message = if error.message.is_empty() {
                    "Unknown error"
                } else {
                    &error.message
                };
                let delay = (30_000.0 * failures).min(300_000.0);
                let lower = message.to_ascii_lowercase();
                let should_reconnect = ["listenkey", "expired", "does not exist", "-1125"]
                    .iter()
                    .any(|s| lower.contains(s));
                lifecycle.owner.log(&format!("Failed to refresh AsterDEX listen key ({failures}): {message}. Retrying in {}s", (delay / 1000.0).round()), "warning")?;
                if should_reconnect {
                    lifecycle.owner.log("AsterDEX listen key appears expired/invalid, forcing private WebSocket reconnect", "warning")?;
                    lifecycle.parent.reconnect()?;
                }
                lifecycle.schedule_listen_key_refresh(delay as u64)?;
            }
            Ok(())
        })
    }
    // ts: AsterDEXPrivateWebsocket.scheduleListenKeyRefresh
    pub fn schedule_listen_key_refresh(
        self: &Rc<Self>,
        delay_ms: u64,
    ) -> Result<(), ExchangeError> {
        self.clear_listen_key_refresh_timer();
        let lifecycle = self.clone();
        let timer = self.owner.start_timeout(delay_ms, move || {
            let work = lifecycle.update_listen_key();
            lifecycle.launch("AsterDEX listen-key keepalive", work);
            Ok(())
        })?;
        *self.refresh_timer.borrow_mut() = Some(timer);
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.clearListenKeyRefreshTimer
    pub fn clear_listen_key_refresh_timer(&self) {
        let timer = self.refresh_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
    }
    fn watchdog_tick(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.owner.is_disposed() {
            return Ok(());
        }
        if self.last_keep_alive_at.get() > 0.0 {
            let elapsed = self.owner.runtime.now_ms() - self.last_keep_alive_at.get();
            if elapsed > 4_200_000.0 {
                self.owner.log(
                    &format!(
                        "AsterDEX listen key keepalive stale for {}s, reconnecting private stream",
                        (elapsed / 1000.0).round()
                    ),
                    "warning",
                )?;
                self.last_keep_alive_at.set(self.owner.runtime.now_ms());
                self.parent.reconnect()?;
            }
        }
        self.arm_watchdog()
    }
    fn arm_watchdog(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let lifecycle = self.clone();
        let timer = self
            .owner
            .start_timeout(300_000, move || lifecycle.watchdog_tick())?;
        *self.watchdog_timer.borrow_mut() = Some(timer);
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.startListenKeyWatchdog
    pub fn start_listen_key_watchdog(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.watchdog_timer.borrow().is_some() || self.owner.is_disposed() {
            return Ok(());
        }
        self.arm_watchdog()
    }
    // ts: AsterDEXPrivateWebsocket.clearListenKeyWatchdogTimer
    pub fn clear_listen_key_watchdog_timer(&self) {
        let timer = self.watchdog_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
    }
    // ts: AsterDEXPrivateWebsocket.scheduleConnectRetry
    pub fn schedule_connect_retry(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.owner.is_disposed() || self.connect_timer.borrow().is_some() {
            return Ok(());
        }
        let count = self.retry_count.get() + 1.0;
        self.retry_count.set(count);
        let delay = (5_000.0 * count).min(60_000.0);
        self.owner.log(
            &format!(
                "Retrying AsterDEX private WebSocket initialization in {}s",
                (delay / 1000.0).round()
            ),
            "info",
        )?;
        let lifecycle = self.clone();
        let timer = self.owner.start_timeout(delay as u64, move || {
            lifecycle.connect_timer.borrow_mut().take();
            if lifecycle.owner.is_disposed() {
                return Ok(());
            }
            let work = lifecycle.parent.connect_and_subscribe();
            lifecycle.launch("AsterDEX private connect retry", work);
            Ok(())
        })?;
        *self.connect_timer.borrow_mut() = Some(timer);
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.clearConnectRetryTimer
    pub fn clear_connect_retry_timer(&self) {
        let timer = self.connect_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
    }
}
