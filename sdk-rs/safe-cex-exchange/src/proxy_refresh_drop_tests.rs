use super::*;
use crate::runtime::{ExchangeError, ExchangeFuture};
use serde_json::Value;
use std::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Waker},
};

struct Host;
impl ProxyTokenHost for Host {
    fn is_main_window(&self) -> bool {
        false
    }
    fn has_self(&self) -> bool {
        false
    }
    fn origin(&self) -> Option<String> {
        None
    }
    fn virtual_clock(&self) -> VirtualClock {
        VirtualClock::default()
    }
    fn initialize_broadcast(
        &self,
        _: &str,
        _: Rc<dyn Fn(BroadcastMessage)>,
    ) -> Result<bool, String> {
        Ok(false)
    }
    fn broadcast(&self, _: BroadcastMessage) -> Result<(), String> {
        Ok(())
    }
    fn fetch<'a>(&'a self, _: String, _: bool) -> ExchangeFuture<'a, Option<String>> {
        Box::pin(async { Ok(None) })
    }
}
struct Handle(Rc<Cell<usize>>);
impl ExchangeTask for Handle {
    fn cancel(&self) {}
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
type TaskFuture = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<TaskFuture>>,
    dropped: Rc<Cell<usize>>,
    inline: Cell<bool>,
    immediate_sleep: Cell<bool>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        100000.
    }
    fn now_ms(&self) -> f64 {
        100000.
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        if self.immediate_sleep.get() {
            Box::pin(async { Ok(()) })
        } else {
            Box::pin(std::future::pending())
        }
    }
    fn spawn_local(&self, mut task: TaskFuture) -> Box<dyn ExchangeTask> {
        let completed = self.inline.get()
            && task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_ready();
        if !completed {
            self.jobs.borrow_mut().push(task);
        }
        Box::new(Handle(self.dropped.clone()))
    }
    fn console_values(&self, _: &str, _: &[crate::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &crate::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(std::future::pending())
    }
}
fn fixture() -> (ProxyTokenClient, Rc<Runtime>) {
    let runtime = Rc::new(Runtime::default());
    let client = ProxyTokenClient::new(Rc::new(Host), runtime.clone());
    client.state.borrow_mut().token = "cached-token".into();
    client.state.borrow_mut().expires_at = 1000000.;
    (client, runtime)
}
fn drop_task(runtime: &Runtime, index: usize) {
    let task = runtime.jobs.borrow_mut().remove(index);
    drop(task);
}
#[test]
fn refresh_timer_task_drop_releases_slot_unpolled_and_pending() {
    for poll in [false, true] {
        let (client, runtime) = fixture();
        client.schedule_refresh();
        assert!(client.state.borrow().timer.is_some());
        if poll {
            let mut task = runtime.jobs.borrow_mut().remove(0);
            assert!(
                task.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            runtime.jobs.borrow_mut().push(task);
        }
        drop_task(&runtime, 0);
        assert!(
            client.state.borrow().timer.is_none(),
            "discarded refresh TASK must release timer slot"
        );
        assert_eq!(
            runtime.dropped.get(),
            1,
            "slot must drop its host handle exactly once"
        );
        client.schedule_refresh();
        assert!(
            client.state.borrow().timer.is_some(),
            "refresh must remain restartable"
        );
        let weak = Rc::downgrade(&client.state);
        drop_task(&runtime, 0);
        drop(client);
        assert!(
            weak.upgrade().is_none(),
            "discarded refresh must not retain token state"
        );
    }
}
#[test]
fn old_refresh_task_drop_cannot_clear_replacement_timer() {
    let (client, runtime) = fixture();
    client.schedule_refresh();
    client.schedule_refresh();
    let generation = client.state.borrow().generation;
    drop_task(&runtime, 0);
    assert!(client.state.borrow().timer.is_some());
    assert_eq!(
        client.state.borrow().generation,
        generation,
        "stale timer cannot invalidate replacement"
    );
    drop_task(&runtime, 0);
    assert!(client.state.borrow().timer.is_none());
    assert_eq!(runtime.dropped.get(), 2);
}
#[test]
fn inline_refresh_completion_does_not_install_stale_handle() {
    let (client, runtime) = fixture();
    runtime.inline.set(true);
    runtime.immediate_sleep.set(true);
    client.state.borrow_mut().last_fetch_attempt = runtime.now_ms();
    client.schedule_refresh();
    assert!(runtime.jobs.borrow().is_empty());
    assert!(
        client.state.borrow().timer.is_none(),
        "inline completed refresh must not install stale timer handle"
    );
    assert_eq!(runtime.dropped.get(), 1);
}
