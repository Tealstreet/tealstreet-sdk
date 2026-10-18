use safe_cex_exchange::{capabilities::ExchangeCapabilities, exchange::*, runtime::*};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, Order, Position};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[path = "support/store.rs"]
mod test_store;
use test_store::TestStore;

fn poll<T>(future: Pin<&mut impl Future<Output = T>>) -> Poll<T> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match poll(future.as_mut()) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("expected ready"),
    }
}
type TaskFuture = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Default)]
struct Runtime {
    tasks: RefCell<Vec<TaskFuture>>,
    sleeps: Cell<usize>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl Runtime {
    fn pump(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(task);
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        self.sleeps.set(self.sleeps.get() + 1);
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: TaskFuture) -> Box<dyn ExchangeTask> {
        self.tasks.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "test".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &serde_json::Value) {}
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(async { unreachable!() })
    }
}
struct Gate<T> {
    result: RefCell<Option<Result<T, ExchangeError>>>,
}
impl<T: Clone> Gate<T> {
    fn new() -> Self {
        Self {
            result: RefCell::new(None),
        }
    }
    async fn wait(&self) -> Result<T, ExchangeError> {
        std::future::poll_fn(|_| {
            self.result
                .borrow()
                .clone()
                .map_or(Poll::Pending, Poll::Ready)
        })
        .await
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    store: Rc<TestStore>,
    runtime: Rc<Runtime>,
    orders: Gate<Vec<Order>>,
    positions: Gate<Vec<Position>>,
    order_calls: Cell<usize>,
    position_calls: Cell<usize>,
    symbols: RefCell<Vec<Option<Vec<String>>>>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.order_calls.set(self.order_calls.get() + 1);
        self.symbols.borrow_mut().push(symbols);
        Box::pin(self.orders.wait())
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.position_calls.set(self.position_calls.get() + 1);
        Box::pin(self.positions.wait())
    }
    fn place_orders(&self, _: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async { unreachable!() })
    }
}
fn venue() -> Rc<Venue> {
    let store = Rc::new(TestStore::new());
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Bybit,
        serde_json::from_value(serde_json::json!({"key":"k","secret":"s"})).unwrap(),
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    Rc::new(Venue {
        base,
        capabilities: ExchangeCapabilities::default(),
        store,
        runtime,
        orders: Gate::new(),
        positions: Gate::new(),
        order_calls: Cell::new(0),
        position_calls: Cell::new(0),
        symbols: RefCell::new(vec![]),
    })
}
fn order() -> Order {
    serde_json::from_value(serde_json::json!({"id":"one","status":"open","symbol":"BTC","type":"limit","side":"buy","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0})).unwrap()
}
fn position() -> Position {
    serde_json::from_value(serde_json::json!({"id":"BTC","symbol":"BTC","side":"long","entryPrice":1,"notional":1,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap()
}

#[test]
fn private_snapshot_does_not_commit_a_response_after_disposal() {
    let venue = venue();
    let mut request = Box::pin(
        venue
            .base
            .fetch_and_set_private_snapshot(venue.as_ref(), 10),
    );
    assert!(poll(request.as_mut()).is_pending());
    venue.base.dispose().unwrap();
    *venue.orders.result.borrow_mut() = Some(Ok(vec![order()]));
    assert!(matches!(poll(request.as_mut()), Poll::Ready(Ok(()))));
    assert!(venue.store.orders().is_empty());
    assert!(!venue.store.loaded().orders);
    assert_eq!(venue.order_calls.get(), 1);
    assert_eq!(venue.runtime.sleeps.get(), 0);
}

#[test]
fn private_snapshot_retries_only_to_requested_bound_and_stops_after_disposal() {
    let venue = venue();
    *venue.orders.result.borrow_mut() = Some(Err(ExchangeError::new("link down")));
    ready(venue.base.fetch_and_set_private_snapshot(venue.as_ref(), 3)).unwrap();
    assert_eq!(venue.order_calls.get(), 3);
    assert_eq!(venue.runtime.sleeps.get(), 2);
    venue.base.dispose().unwrap();
    ready(
        venue
            .base
            .fetch_and_set_private_snapshot(venue.as_ref(), 10),
    )
    .unwrap();
    assert_eq!(venue.order_calls.get(), 3);
}

#[test]
fn scoped_empty_symbols_are_not_a_full_order_reconciliation() {
    let venue = venue();
    venue.store.data.borrow_mut().orders = vec![order()];
    *venue.orders.result.borrow_mut() = Some(Ok(vec![]));
    ready(
        venue
            .base
            .fetch_and_set_orders(venue.as_ref(), Some(vec![])),
    )
    .unwrap();
    assert_eq!(venue.store.orders().len(), 1);
    assert!(!venue.store.loaded().orders);
    ready(venue.base.fetch_and_set_orders(venue.as_ref(), None)).unwrap();
    assert!(venue.store.orders().is_empty());
    assert!(venue.store.loaded().orders);
    assert_eq!(*venue.symbols.borrow(), vec![Some(vec![]), None]);
}

#[test]
fn position_fetch_waits_for_loaded_flag_and_retains_shared_rejection() {
    let venue = venue();
    assert!(
        ready(
            venue
                .base
                .get_or_fetch_positions_for_symbol(venue.clone(), "BTC")
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(venue.position_calls.get(), 0);
    venue.store.data.borrow_mut().loaded.positions = true;
    let mut first = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    let mut second = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    assert!(poll(first.as_mut()).is_pending());
    assert!(poll(second.as_mut()).is_pending());
    venue.runtime.pump();
    assert_eq!(venue.position_calls.get(), 1);
    *venue.positions.result.borrow_mut() = Some(Err(ExchangeError::new("fetch rejected")));
    venue.runtime.pump();
    assert!(
        matches!(poll(first.as_mut()), Poll::Ready(Err(error)) if error.message == "fetch rejected")
    );
    assert!(
        matches!(poll(second.as_mut()), Poll::Ready(Err(error)) if error.message == "fetch rejected")
    );
    assert!(
        ready(
            venue
                .base
                .get_or_fetch_positions_for_symbol(venue.clone(), "BTC")
        )
        .is_err()
    );
    assert_eq!(venue.position_calls.get(), 1);
}

#[test]
fn disposed_origin_returns_empty_but_existing_waiter_gets_fetch_result() {
    let venue = venue();
    venue.store.data.borrow_mut().loaded.positions = true;
    let mut first = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    let mut second = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    assert!(poll(first.as_mut()).is_pending());
    assert!(poll(second.as_mut()).is_pending());
    venue.runtime.pump();
    venue.base.dispose().unwrap();
    *venue.positions.result.borrow_mut() = Some(Ok(vec![position()]));
    venue.runtime.pump();
    assert!(matches!(poll(first.as_mut()), Poll::Ready(Ok(positions)) if positions.is_empty()));
    assert!(matches!(poll(second.as_mut()), Poll::Ready(Ok(positions)) if positions.len() == 1));
    assert!(venue.store.positions().is_empty());
    assert_eq!(venue.position_calls.get(), 1);
}

#[test]
fn adversarial_dropped_position_initiator_still_commits_completed_fetch() {
    let venue = venue();
    venue.store.data.borrow_mut().loaded.positions = true;
    let mut first = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    assert!(poll(first.as_mut()).is_pending());
    venue.runtime.pump();
    drop(first);
    *venue.positions.result.borrow_mut() = Some(Ok(vec![position()]));
    venue.runtime.pump();
    assert_eq!(
        venue.store.positions().len(),
        1,
        "Ignoring the initiating caller must not discard the source async method's post-fetch store update"
    );
}

#[test]
fn host_discarding_unpolled_or_pending_position_task_settles_all_waiters() {
    for polled in [false, true] {
        let venue = venue();
        venue.store.data.borrow_mut().loaded.positions = true;
        let mut first = Box::pin(
            venue
                .base
                .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
        );
        let mut second = Box::pin(
            venue
                .base
                .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
        );
        assert!(poll(first.as_mut()).is_pending());
        assert!(poll(second.as_mut()).is_pending());
        if polled {
            venue.runtime.pump();
        }
        let tasks = std::mem::take(&mut *venue.runtime.tasks.borrow_mut());
        drop(tasks);
        assert!(
            matches!(poll(first.as_mut()), Poll::Ready(Err(error)) if error.message == "Position fetch task cancelled")
        );
        assert!(
            matches!(poll(second.as_mut()), Poll::Ready(Err(error)) if error.message == "Position fetch task cancelled")
        );
        assert!(venue.store.positions().is_empty());
    }
}

#[test]
fn private_snapshot_and_overlay_use_dynamic_privacy_getter() {
    struct PrivacyVenue {
        inner: Rc<Venue>,
        public: bool,
    }
    impl Exchange for PrivacyVenue {
        type PlaceOpts = safe_cex_types::PlaceOrderOpts;
        fn base(&self) -> &BaseExchange {
            &self.inner.base
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            &self.inner.capabilities
        }
        fn is_public_data_only(&self) -> bool {
            self.public
        }
        fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
            self.inner.fetch_orders(symbols)
        }
    }
    for public in [true, false] {
        let mut inner = venue();
        let inner_mut = Rc::get_mut(&mut inner).unwrap();
        let mut options = inner_mut.base.options();
        // Deliberately invert key-based classification in both directions.
        options.key = if public {
            "signed".into()
        } else {
            String::new()
        };
        options.external_public_data = Some(serde_json::from_value(serde_json::json!({"type":"websocket","url":"wss://fixture","exchangeId":"fixture"})).unwrap());
        inner_mut.base = BaseExchange::new(
            ExchangeName::Bybit,
            options,
            inner_mut.store.clone(),
            inner_mut.runtime.clone(),
            ExchangeSettings::default(),
        );
        let concrete = PrivacyVenue { inner, public };
        let dynamic: &dyn Exchange = &concrete;
        assert_eq!(
            dynamic.needs_account_market_overlay(),
            !public,
            "overlay must dispatch venue privacy getter"
        );
        let mut snapshot = std::pin::pin!(dynamic.fetch_and_set_private_snapshot(1));
        let result = poll(snapshot.as_mut());
        assert_eq!(
            result.is_ready(),
            public,
            "private snapshot must dispatch venue privacy getter in both directions"
        );
        assert_eq!(concrete.inner.order_calls.get(), usize::from(!public));
    }
}

#[test]
fn default_privacy_uses_dynamic_signing_getter() {
    struct SigningVenue {
        inner: Rc<Venue>,
        signs: bool,
    }
    impl Exchange for SigningVenue {
        type PlaceOpts = safe_cex_types::PlaceOrderOpts;
        fn base(&self) -> &BaseExchange {
            &self.inner.base
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            &self.inner.capabilities
        }
        fn can_sign(&self) -> bool {
            self.signs
        }
    }
    for signs in [true, false] {
        let mut inner = venue();
        let inner_mut = Rc::get_mut(&mut inner).unwrap();
        let mut options = inner_mut.base.options();
        options.key = if signs { String::new() } else { "key".into() };
        inner_mut.base = BaseExchange::new(
            ExchangeName::Bybit,
            options,
            inner_mut.store.clone(),
            inner_mut.runtime.clone(),
            ExchangeSettings::default(),
        );
        let concrete = SigningVenue { inner, signs };
        let dynamic: &dyn Exchange = &concrete;
        assert_eq!(
            dynamic.is_public_data_only(),
            !signs,
            "default privacy must dispatch venue signing getter"
        );
    }
}

#[test]
fn identity_patch_reaches_existing_ws_bridge_without_borrow_across_await() {
    use safe_cex_exchange::{websocket::WsParent, ws_parent::WsExchangeParent};
    let concrete = venue();
    let dynamic: Rc<dyn Exchange> = concrete.clone();
    let parent = WsExchangeParent::new(&dynamic);
    let before = parent.options();
    let mut pending = std::pin::pin!(dynamic.fetch_and_set_private_snapshot(1));
    assert!(poll(pending.as_mut()).is_pending());
    let patch = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        concrete.base.patch_account_identity(AccountIdentityPatch {
            application_id: IdentityFieldPatch::Set("master".into()),
            vault: IdentityFieldPatch::Unchanged,
        });
    }));
    assert!(
        patch.is_ok(),
        "identity patch must not retain an options borrow across await"
    );
    assert_eq!(
        concrete.base.options().application_id.as_deref(),
        Some("master"),
        "base must read patched identity"
    );
    assert_eq!(
        parent.options().application_id.as_deref(),
        Some("master"),
        "pre-existing WS bridge must read patched identity"
    );
    assert!(
        before.application_id.is_none(),
        "owned snapshots deliberately retain their prior value"
    );
    concrete.base.patch_account_identity(AccountIdentityPatch {
        application_id: IdentityFieldPatch::Unchanged,
        vault: IdentityFieldPatch::Set(String::new()),
    });
    assert_eq!(parent.options().application_id.as_deref(), Some("master"));
    assert_eq!(
        parent.options().vault.as_deref(),
        Some(""),
        "present empty identity must not collapse to absence"
    );
    concrete.base.patch_account_identity(AccountIdentityPatch {
        application_id: IdentityFieldPatch::Remove,
        vault: IdentityFieldPatch::Set("vault".into()),
    });
    assert!(concrete.base.options().application_id.is_none());
    assert_eq!(parent.options().vault.as_deref(), Some("vault"));
}

#[test]
#[ignore = "live TypeScript constructor in mandatory isolated core CI"]
fn options_snapshot_declares_source_alias_difference() {
    let output = std::process::Command::new("node")
        .arg("scripts/options-identity-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["sameObject"], true,
        "source constructor must retain options alias"
    );
    assert_eq!(
        source["retainedIdentity"], "master",
        "source retained options observes identity patch"
    );
    let concrete = venue();
    let retained = concrete.base.options();
    concrete.base.patch_account_identity(AccountIdentityPatch {
        application_id: IdentityFieldPatch::Set("master".into()),
        vault: IdentityFieldPatch::Unchanged,
    });
    assert!(
        retained.application_id.is_none(),
        "CHOSEN Rust options reads are snapshots, not live aliases"
    );
    assert_eq!(
        concrete.base.options().application_id.as_deref(),
        source["currentIdentity"].as_str()
    );
}

#[test]
fn notification_failure_is_inside_private_snapshot_retry_catch() {
    let venue = venue();
    *venue.orders.result.borrow_mut() = Some(Ok(vec![order()]));
    let failures = Rc::new(Cell::new(0));
    let count = failures.clone();
    venue.base.on(
        "update",
        Rc::new(move |_, _| {
            count.set(count.get() + 1);
            let mut error = ExchangeError::new("notification failed");
            error.name = "TypeError".into();
            Err(error)
        }),
    );
    ready(venue.base.fetch_and_set_private_snapshot(venue.as_ref(), 3)).unwrap();
    assert_eq!(
        venue.order_calls.get(),
        3,
        "snapshot notification error must enter source retry catch"
    );
    assert_eq!(failures.get(), 3);
    assert_eq!(venue.runtime.sleeps.get(), 2);
    assert_eq!(
        venue.store.orders().len(),
        1,
        "failed notification follows committed orders"
    );
}

#[test]
fn error_listener_throw_aborts_snapshot_before_retry_sleep() {
    let venue = venue();
    *venue.orders.result.borrow_mut() = Some(Ok(vec![order()]));
    venue.base.on(
        "update",
        Rc::new(|_, _| Err(ExchangeError::new("update failed"))),
    );
    let mut expected = ExchangeError::new("error listener failed");
    expected.name = "TypeError".into();
    expected.code = Some(safe_cex_exchange::js_runtime::value::from_json(
        &serde_json::json!("E_LISTENER"),
    ));
    let error = expected.clone();
    venue
        .base
        .on("error", Rc::new(move |_, _| Err(error.clone())));
    let actual = ready(venue.base.fetch_and_set_private_snapshot(venue.as_ref(), 3)).unwrap_err();
    assert_eq!(
        actual, expected,
        "recovery emitter failure replaces caught notification error"
    );
    assert_eq!(venue.order_calls.get(), 1);
    assert_eq!(
        venue.runtime.sleeps.get(),
        0,
        "throwing error listener must abort before retry sleep"
    );
}

#[test]
fn position_notification_error_belongs_to_origin_not_shared_fetch_waiter() {
    let venue = venue();
    venue.store.data.borrow_mut().loaded.positions = true;
    let mut expected = ExchangeError::new("position notification failed");
    let error = expected.clone();
    venue
        .base
        .on("update", Rc::new(move |_, _| Err(error.clone())));
    let mut origin = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    let mut waiter = Box::pin(
        venue
            .base
            .get_or_fetch_positions_for_symbol(venue.clone(), "BTC"),
    );
    assert!(poll(origin.as_mut()).is_pending());
    assert!(poll(waiter.as_mut()).is_pending());
    *venue.positions.result.borrow_mut() = Some(Ok(vec![position()]));
    venue.runtime.pump();
    let original = expected.clone();
    expected.store_subscriber_ids.push(0);
    let Poll::Ready(Err(mut error)) = poll(origin.as_mut()) else {
        panic!("origin must observe Store notification failure");
    };
    // The recovered error retains the opaque notification cause it came through.
    let cause = error
        .store_subscriber_cause
        .take()
        .expect("origin retains the Store notification cause");
    assert_eq!(
        std::error::Error::source(&cause).and_then(|inner| inner.downcast_ref::<ExchangeError>()),
        Some(&original)
    );
    assert_eq!(error, expected);
    assert!(
        matches!(poll(waiter.as_mut()), Poll::Ready(Ok(rows)) if rows.len() == 1),
        "shared waiter awaits fetch only, not origin notification"
    );
    assert_eq!(venue.store.positions().len(), 1);
}
