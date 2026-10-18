//! Fail-closed observations of the native runtime's raw tasks. These records
//! never poll, cancel or retain the task body or account.
use crate::runtime::RuntimeTaskKind;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Wake, Waker},
};

const ACTIVE: usize = 0;
const TIMER: usize = 1;
const SOCKET: usize = 2;
const DELEGATED: usize = 3;

#[derive(Default)]
pub(crate) struct Tasks {
    next: usize,
    entries: BTreeMap<usize, Arc<State>>,
}
pub(crate) struct State {
    kind: Option<RuntimeTaskKind>,
    status: AtomicUsize,
    wakes: AtomicUsize,
}
pub(crate) struct Registration {
    id: usize,
    owner: Weak<RefCell<Tasks>>,
    state: Arc<State>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            owner.borrow_mut().entries.remove(&self.id);
        }
    }
}
pub(crate) fn register(owner: &Rc<RefCell<Tasks>>, kind: Option<RuntimeTaskKind>) -> Registration {
    let state = Arc::new(State {
        kind,
        status: AtomicUsize::new(ACTIVE),
        wakes: AtomicUsize::new(0),
    });
    let mut tasks = owner.borrow_mut();
    let id = tasks.next;
    tasks.next = id.checked_add(1).expect("raw task id exhausted");
    tasks.entries.insert(id, state.clone());
    Registration {
        id,
        owner: Rc::downgrade(owner),
        state,
    }
}

fn with_current<R>(callback: impl FnOnce(&RefCell<Option<Arc<State>>>) -> R) -> R {
    std::thread_local! {
        static CURRENT: RefCell<Option<Arc<State>>> = const { RefCell::new(None) };
    }
    CURRENT.with(callback)
}
pub(crate) struct Enter(Option<Arc<State>>);
impl Drop for Enter {
    fn drop(&mut self) {
        with_current(|current| *current.borrow_mut() = self.0.take());
    }
}
struct Signal {
    state: Arc<State>,
    parent: Waker,
}
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        // A waiting task becomes blocking as soon as its input is ready, before
        // Tokio polls it. This also fences wakes during the current poll.
        self.state.wakes.fetch_add(1, Ordering::SeqCst);
        self.state.status.store(ACTIVE, Ordering::SeqCst);
        self.parent.wake_by_ref();
    }
}
impl Registration {
    pub(crate) fn enter(&self, parent: &Waker) -> (Enter, Waker, usize) {
        self.state.status.store(ACTIVE, Ordering::SeqCst);
        let version = self.state.wakes.load(Ordering::SeqCst);
        let previous = with_current(|current| current.borrow_mut().replace(self.state.clone()));
        let waker = Waker::from(Arc::new(Signal {
            state: self.state.clone(),
            parent: parent.clone(),
        }));
        (Enter(previous), waker, version)
    }
    pub(crate) fn pending(&self, version: usize) {
        if self.state.kind == Some(RuntimeTaskKind::PromiseDriver) {
            self.state.status.store(DELEGATED, Ordering::SeqCst);
        }
        if self.state.wakes.load(Ordering::SeqCst) != version {
            self.state.status.store(ACTIVE, Ordering::SeqCst);
        }
    }
}

/// Only reviewed tasks may become nonblocking at these exact pending leaves.
/// A finite/unknown task remains blocking even if it happens to await a timer.
pub(crate) fn waiting(kind: RuntimeTaskKind) {
    with_current(|current| {
        if let Some(state) = current.borrow().as_ref() {
            if state.kind == Some(kind) {
                state.status.store(
                    match kind {
                        RuntimeTaskKind::Timer => TIMER,
                        RuntimeTaskKind::SocketReader => SOCKET,
                        _ => return,
                    },
                    Ordering::SeqCst,
                );
            }
        }
    });
}

pub(crate) fn receipt(owner: &Rc<RefCell<Tasks>>) -> serde_json::Value {
    let tasks = owner.borrow();
    let mut unknown = 0;
    let mut finite = 0;
    let mut ready = 0;
    let mut timers = 0;
    let mut sockets = 0;
    let mut drivers = 0;
    for state in tasks.entries.values() {
        match (state.kind, state.status.load(Ordering::SeqCst)) {
            (None, _) => unknown += 1,
            (Some(RuntimeTaskKind::Timer), TIMER) => timers += 1,
            (Some(RuntimeTaskKind::SocketReader), SOCKET) => sockets += 1,
            (Some(RuntimeTaskKind::PromiseDriver), DELEGATED) => drivers += 1,
            (Some(RuntimeTaskKind::Finite), _) => finite += 1,
            _ => ready += 1,
        }
    }
    serde_json::json!({
        "version": 1, "scope": "exchange-runtime", "unknown": unknown,
        "finite": finite, "ready": ready, "waitingTimers": timers,
        "waitingSockets": sockets, "promiseDrivers": drivers,
        "blocking": unknown + finite + ready,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_invalidates_waiting_before_repoll_and_survives_a_racing_wait_mark() {
        let tasks = Rc::new(RefCell::new(Tasks::default()));
        let task = register(&tasks, Some(RuntimeTaskKind::SocketReader));
        let (entered, waker, version) = task.enter(Waker::noop());
        waiting(RuntimeTaskKind::SocketReader);
        task.pending(version);
        assert_eq!(receipt(&tasks)["blocking"], 0);
        waker.wake_by_ref();
        assert_eq!(receipt(&tasks)["blocking"], 1);
        waiting(RuntimeTaskKind::SocketReader);
        task.pending(version);
        assert_eq!(receipt(&tasks)["blocking"], 1);
        drop(entered);
        drop(task);
        // A retained external waker must not retain the registry or task body.
        assert_eq!(receipt(&tasks)["waitingSockets"], 0);
        assert_eq!(receipt(&tasks)["blocking"], 0);
    }
}
