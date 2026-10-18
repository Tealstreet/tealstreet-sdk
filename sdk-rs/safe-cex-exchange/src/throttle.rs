use crate::exchange::Exchange;
use crate::runtime::{ExchangeRuntime, ExchangeTask};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

struct ThrottleInfo {
    last_call_time: f64,
    timer: u64,
}

pub struct ThrottleManager<T> {
    runtime: Rc<dyn ExchangeRuntime>,
    callback: Box<dyn Fn(T)>,
    get_key: Box<dyn Fn(&T) -> String>,
    delay: u64,
    next_timer: Cell<u64>,
    entries: RefCell<HashMap<String, ThrottleInfo>>,
    tasks: RefCell<HashMap<u64, Box<dyn ExchangeTask>>>,
}

struct TimerGuard<T> {
    manager: std::rc::Weak<ThrottleManager<T>>,
    key: String,
    id: u64,
    settled: Rc<Cell<bool>>,
}
impl<T> Drop for TimerGuard<T> {
    fn drop(&mut self) {
        self.settled.set(true);
        if let Some(manager) = self.manager.upgrade() {
            let owns_entry = manager
                .entries
                .borrow()
                .get(&self.key)
                .is_some_and(|entry| entry.timer == self.id);
            if owns_entry {
                manager.entries.borrow_mut().remove(&self.key);
            }
            let task = manager.tasks.borrow_mut().remove(&self.id);
            drop(task);
        }
    }
}

impl<T: 'static> ThrottleManager<T> {
    pub fn new(
        runtime: Rc<dyn ExchangeRuntime>,
        callback: impl Fn(T) + 'static,
        delay: u64,
        get_key: impl Fn(&T) -> String + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            callback: Box::new(callback),
            get_key: Box::new(get_key),
            delay,
            next_timer: Cell::new(0),
            entries: RefCell::new(HashMap::new()),
            tasks: RefCell::new(HashMap::new()),
        })
    }

    fn invoke(&self, args: T) {
        (self.callback)(args);
    }

    fn key(&self, args: &T) -> String {
        (self.get_key)(args)
    }

    pub fn call(self: &Rc<Self>, args: T) {
        let key = self.key(&args);
        let now = self.runtime.now_ms();
        let previous = self
            .entries
            .borrow()
            .get(&key)
            .map(|info| (info.last_call_time, info.timer));
        let leading = previous.is_none_or(|(last, _)| now - last >= self.delay as f64);
        let (last_call_time, tail) = if leading {
            self.invoke(args);
            (now, None)
        } else {
            let (last, timer) = previous.expect("trailing call has a previous entry");
            let task = self.tasks.borrow_mut().remove(&timer);
            if let Some(task) = task {
                task.cancel();
            }
            (last, Some(args))
        };
        let timer = self.next_timer.get();
        self.next_timer.set(timer.wrapping_add(1));
        let weak = Rc::downgrade(self);
        let sleep = self.runtime.sleep(self.delay);
        let timer_key = key.clone();
        let settled = Rc::new(Cell::new(false));
        // Created before spawn: discarded/unpolled tasks must release only their own slot.
        let guard = TimerGuard {
            manager: weak.clone(),
            key: key.clone(),
            id: timer,
            settled: settled.clone(),
        };
        let task = self.runtime.spawn_local(Box::pin(async move {
            let _guard = guard;
            if sleep.await.is_err() {
                return;
            }
            if let Some(manager) = weak.upgrade() {
                if let Some(args) = tail {
                    manager.invoke(args);
                }
                // ts-bug: an older tail can delete a newer leading call's entry.
                manager.entries.borrow_mut().remove(&timer_key);
            }
        }));
        if settled.get() {
            return;
        }
        self.tasks.borrow_mut().insert(timer, task);
        self.entries.borrow_mut().insert(
            key,
            ThrottleInfo {
                last_call_time,
                timer,
            },
        );
    }

    pub fn clear(&self) {
        let entries = std::mem::take(&mut *self.entries.borrow_mut());
        for (_, info) in entries {
            let task = self.tasks.borrow_mut().remove(&info.timer);
            if let Some(task) = task {
                task.cancel();
            }
        }
    }
}

impl<T> Drop for ThrottleManager<T> {
    fn drop(&mut self) {
        // Drop fires once: only this manager owns its timers, and futures own Weak.
        for (_, task) in self.tasks.get_mut().drain() {
            task.cancel();
        }
    }
}

pub fn symbol_metadata_throttler(exchange: Rc<dyn Exchange>) -> Rc<ThrottleManager<String>> {
    let venue = exchange.clone();
    ThrottleManager::new(
        exchange.base().runtime.clone(),
        move |symbol: String| {
            let venue = venue.clone();
            let runtime = venue.base().runtime.clone();
            runtime.spawn_local(Box::pin(async move {
                if let Err(error) = venue.update_symbol_metadata(symbol).await {
                    // TS leaves this Promise unawaited; the host receives its unhandled error.
                    venue
                        .base()
                        .runtime
                        .report_exception(&error, &json!({"method":"updateSymbolMetadata"}));
                }
            }));
        },
        500,
        String::clone,
    )
}
