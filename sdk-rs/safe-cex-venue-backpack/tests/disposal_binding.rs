#[path = "support/adapter_runtime.rs"]
mod support;
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, ExchangeTask},
    websocket::{WsEvent, WsSocket},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_venue_backpack::{
    native_value::Value, orderbook_state, public_adapter::Throttle, public_listeners::Handler,
};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ready fixture"),
    }
}
struct CountTask(Rc<Cell<bool>>);
impl ExchangeTask for CountTask {
    fn cancel(&self) {
        self.0.set(true)
    }
}
fn tracked_timeout(
    socket: &Rc<safe_cex_exchange::websocket::BaseWebSocket>,
    cancelled: Rc<Cell<bool>>,
) -> safe_cex_exchange::OwnedOperation {
    struct Cancellation(Rc<Cell<bool>>);
    impl Drop for Cancellation {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let cancellation = Cancellation(cancelled);
    socket
        .start_timeout(60_000, move || {
            let _retained = &cancellation;
            Ok(())
        })
        .unwrap()
}
struct Socket {
    name: &'static str,
    close: Rc<dyn Fn(&str)>,
    terminate: Rc<Cell<usize>>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn close(&self) {
        (self.close)(self.name)
    }
    fn terminate(&self) {
        self.terminate.set(self.terminate.get() + 1)
    }
    fn reconnect(&self) {
        panic!("no reconnect in disposal")
    }
    fn is_reconnecting(&self) -> bool {
        false
    }
}
fn exercise(surface: &str) {
    let (o, rt, store, _) = support::fixture();
    let p = o.public_websocket.get().unwrap();
    let q = o.private_websocket.get().unwrap();
    let calls = Rc::new(RefCell::new(Vec::<String>::new()));
    let terminations = Rc::new(Cell::new(0));
    let depth_cancel = Rc::new(Cell::new(false));
    let orders_cancel = Rc::new(Cell::new(false));
    for name in ["public", "private"] {
        // Close callbacks can also run during final Exchange destruction. Inspect
        // the original retained resources, whose weak identities remain live then.
        let base = Rc::downgrade(&o.base);
        let public = Rc::downgrade(p);
        let private = Rc::downgrade(q);
        let log = calls.clone();
        let depth = depth_cancel.clone();
        let orders = orders_cancel.clone();
        rt.dial_sockets.borrow_mut().push(Rc::new(Socket {
            name,
            close: Rc::new(move |name| {
                let base = base.upgrade().unwrap();
                if name == "public" {
                    let public = public.upgrade().unwrap();
                    assert!(
                        depth.get() && public.listeners.throttles.borrow().is_empty(),
                        "DISPOSE depth cancellation precedes public close"
                    );
                    assert!(
                        public.listeners.states.borrow().is_empty()
                            && public.listeners.fetchers.borrow().is_empty(),
                        "DISPOSE state clearing precedes public close"
                    );
                } else {
                    assert!(
                        orders.get()
                            && private
                                .upgrade()
                                .unwrap()
                                .state
                                .orders_timeout
                                .borrow()
                                .is_none(),
                        "DISPOSE order timeout cancellation precedes private close"
                    );
                }
                log.borrow_mut()
                    .push(format!("{name}:close:{}", base.is_disposed()));
            }),
            terminate: terminations.clone(),
        }));
    }
    ready(
        p.base
            .create_websocket("ws://fixture/public".into(), vec![], None),
    )
    .unwrap();
    ready(
        q.base
            .create_websocket("ws://fixture/private".into(), vec![], None),
    )
    .unwrap();
    p.subscriptions.borrow_mut().insert(vec![115]);
    q.state.subscriptions.borrow_mut().insert(vec![115]);
    p.listeners.handlers.borrow_mut().push((
        vec![115],
        Rc::new(RefCell::new(vec![Rc::new(Handler {
            call: Rc::new(|_| Ok(())),
        })])),
    ));
    p.listeners.topics.borrow_mut().push((vec![115], 1.0));
    p.listeners.states.borrow_mut().push((
        vec![115],
        Rc::new(RefCell::new(orderbook_state::create_order_book_state())),
    ));
    p.listeners
        .fetchers
        .borrow_mut()
        .push((vec![115], Rc::new(|| Box::pin(async { Ok(()) }))));
    let depth = Rc::new(Throttle {
        owner: Rc::downgrade(p),
        depth: true,
        last: Cell::new(Some(1.0)),
        pending: RefCell::new(Some(Value::Null)),
        task: RefCell::new(Some(tracked_timeout(&p.base, depth_cancel.clone()))),
    });
    p.listeners.throttles.borrow_mut().push((vec![115], depth));
    *p.ticker_handler.pending.borrow_mut() = Some(Value::Null);
    let ticker_cancel = Rc::new(Cell::new(false));
    *p.ticker_handler.task.borrow_mut() = Some(tracked_timeout(&p.base, ticker_cancel.clone()));
    *q.state.orders_timeout.borrow_mut() = Some(orders_cancel.clone());
    for state in [&q.state.positions, &q.state.balance] {
        state.scheduled.set(true);
        state.pending.set(true);
    }
    let mut task_cancel = vec![];
    for tasks in [&o.tasks, &p.tasks, &q.tasks] {
        let c = Rc::new(Cell::new(false));
        tasks.borrow_mut().push(Box::new(CountTask(c.clone())));
        task_cancel.push(c);
    }
    let updates = Rc::new(Cell::new(0));
    let seen = updates.clone();
    o.base.on(
        "update",
        Rc::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    let owned = o.base.start_owned_operation(std::future::pending());
    match surface {
        "exchange" => o.dispose().unwrap(),
        "public" => p.dispose().unwrap(),
        "private" => q.dispose().unwrap(),
        _ => unreachable!(),
    }
    assert!(
        owned.is_active(),
        "DISPOSE source override retains superclass owned operation"
    );
    assert!(
        task_cancel.iter().all(|c| !c.get()),
        "DISPOSE source override retains adapter tasks"
    );
    assert!(
        !ticker_cancel.get(),
        "DISPOSE public source retains ticker throttle"
    );
    assert_eq!(
        p.listeners.handlers.borrow().len(),
        1,
        "DISPOSE public source retains handlers"
    );
    assert_eq!(
        p.listeners.topics.borrow().len(),
        1,
        "DISPOSE public source retains topics"
    );
    assert!(
        q.state.positions.scheduled.get() && q.state.positions.pending.get(),
        "DISPOSE private source retains position work"
    );
    assert!(
        q.state.balance.scheduled.get() && q.state.balance.pending.get(),
        "DISPOSE private source retains balance work"
    );
    assert!(
        !p.base.is_force_closed() && !q.base.is_force_closed(),
        "DISPOSE source close does not invoke superclass cleanup"
    );
    assert_eq!(
        terminations.get(),
        0,
        "DISPOSE source closes without terminate"
    );
    assert!(
        p.base.has_websocket() && q.base.has_websocket(),
        "DISPOSE source retains socket handles"
    );
    let mut tickers = store.tickers().unwrap().to_vec();
    tickers[0].bid = 42.0;
    Store::update(
        store.as_ref(),
        StoreUpdate {
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        updates.get(),
        1,
        "DISPOSE source retains Store subscription and update listener"
    );
    let mut trace = calls.borrow().clone();
    if depth_cancel.get() {
        trace.insert(0, "depth:cancel".into());
    }
    if orders_cancel.get() {
        let i = trace
            .iter()
            .position(|s| s.starts_with("private:close"))
            .unwrap();
        trace.insert(i, "orders:clear".into());
    }
    let native = json!({"name":surface,"calls":trace,"disposed":o.base.is_disposed(),
      "public":{"subscriptions":p.subscriptions.borrow().values().len(),"handlers":p.listeners.handlers.borrow().len(),"topics":p.listeners.topics.borrow().len(),"states":p.listeners.states.borrow().len(),"fetchers":p.listeners.fetchers.borrow().len(),"depth":p.listeners.throttles.borrow().len(),"tickerPending":p.ticker_handler.pending.borrow().is_some()},
      "private":{"subscriptions":q.state.subscriptions.borrow().values().len(),"ordersPending":q.state.orders_timeout.borrow().is_some(),"positionsScheduled":q.state.positions.scheduled.get(),"positionsPending":q.state.positions.pending.get(),"balanceScheduled":q.state.balance.scheduled.get(),"balancePending":q.state.balance.pending.get()}});
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/disposal-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 3);
    assert_eq!(
        &native,
        source.iter().find(|v| v["name"] == surface).unwrap(),
        "DISPOSE bound adapter matches actual source state and close order"
    );
    owned.cancel();
    rt.pump();
}
#[test]
fn exchange_override_retains_source_superclass_state() {
    exercise("exchange")
}
#[test]
fn public_override_only_cancels_depth_and_closes() {
    exercise("public")
}
#[test]
fn private_override_only_cancels_orders_and_closes() {
    exercise("private")
}

#[test]
fn public_connect_installs_ready_socket_before_polling() {
    let (owner, runtime, _, _) = support::fixture();
    let socket = owner.public_websocket.get().unwrap();
    runtime.dial_sockets.borrow_mut().push(Rc::new(Socket {
        name: "eager-public",
        close: Rc::new(|_| {}),
        terminate: Rc::new(Cell::new(0)),
    }));
    socket.connect_and_subscribe().unwrap();
    assert!(
        socket.base.has_websocket(),
        "BACKPACK eager ready dial installs before runtime polling"
    );
    socket.base.dispose();
    runtime.pump();
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/connect-prefix-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "BACKPACK maintained prefix witness: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    assert_eq!(source.stdout, b"synchronous-wrapper-installed\n");
}
