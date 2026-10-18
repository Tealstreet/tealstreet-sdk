//! Public regression witnesses for core-owned waiter cancellation repairs.
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{ExchangeName, Position};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Wake, Waker},
};

type TaskFuture = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Default)]
struct Runtime {
    tasks: RefCell<Vec<TaskFuture>>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.
    }
    fn now_ms(&self) -> f64 {
        0.
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, task: TaskFuture) -> Box<dyn ExchangeTask> {
        self.tasks.borrow_mut().push(task);
        Box::new(Task)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "finding".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &serde_json::Value) {}
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(std::future::pending())
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(std::future::pending())
    }
}
struct Sentinel(Arc<AtomicUsize>);
impl Wake for Sentinel {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn dropped_position_waiter_releases_waker_before_shared_task_finishes() {
    let store = Rc::new(DefaultStore::new());
    let mut loaded = store.loaded().clone();
    loaded.positions = true;
    Store::update(
        store.as_ref(),
        StoreUpdate {
            loaded: Some(loaded),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let runtime = Rc::new(Runtime::default());
    let venue = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Okx,
            serde_json::from_value(serde_json::json!({"key":"k","secret":"s"})).unwrap(),
            store,
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
    });
    let wakes = Arc::new(AtomicUsize::new(0));
    let sentinel = Arc::new(Sentinel(wakes.clone()));
    let weak = Arc::downgrade(&sentinel);
    let waker = Waker::from(sentinel);
    let mut waiter = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(waiter);
    drop(waker);
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        0,
        "cancelled WAITER has not been woken"
    );
    // The registration belongs to the waiter, independently of the shared TASK.
    assert!(
        weak.upgrade().is_none(),
        "cancelled position waiter must immediately release its waker"
    );
    drop(std::mem::take(&mut *runtime.tasks.borrow_mut()));
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        0,
        "TASK completion must not wake a cancelled caller registration"
    );
    assert!(
        weak.upgrade().is_none(),
        "dropping the TASK must settle and release the retained waker"
    );
}

struct PendingProxyHost;
impl safe_cex_exchange::proxy_token::ProxyTokenHost for PendingProxyHost {
    fn is_main_window(&self) -> bool {
        true
    }
    fn has_self(&self) -> bool {
        false
    }
    fn origin(&self) -> Option<String> {
        None
    }
    fn virtual_clock(&self) -> safe_cex_store::virtual_clock::VirtualClock {
        Default::default()
    }
    fn initialize_broadcast(
        &self,
        _: &str,
        _: Rc<dyn Fn(safe_cex_exchange::proxy_token::BroadcastMessage)>,
    ) -> Result<bool, String> {
        Ok(false)
    }
    fn broadcast(&self, _: safe_cex_exchange::proxy_token::BroadcastMessage) -> Result<(), String> {
        Ok(())
    }
    fn fetch<'a>(&'a self, _: String, _: bool) -> ExchangeFuture<'a, Option<String>> {
        Box::pin(std::future::pending())
    }
}
#[test]
fn dropped_proxy_waiter_releases_waker_before_shared_task_finishes() {
    let runtime = Rc::new(Runtime::default());
    let client = safe_cex_exchange::proxy_token::ProxyTokenClient::new(
        Rc::new(PendingProxyHost),
        runtime.clone(),
    );
    let wakes = Arc::new(AtomicUsize::new(0));
    let sentinel = Arc::new(Sentinel(wakes.clone()));
    let weak = Arc::downgrade(&sentinel);
    let waker = Waker::from(sentinel);
    let mut waiter = client.fetch_proxy_token();
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(waiter);
    drop(waker);
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        0,
        "cancelled WAITER has not been woken"
    );
    // Cancelling this caller unregisters it without settling the shared flight.
    assert!(
        weak.upgrade().is_none(),
        "cancelled proxy waiter must immediately release its waker"
    );
    drop(std::mem::take(&mut *runtime.tasks.borrow_mut()));
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        0,
        "TASK completion must not wake a cancelled caller registration"
    );
    assert!(
        weak.upgrade().is_none(),
        "dropping the proxy TASK must release retained caller wakers"
    );
}
