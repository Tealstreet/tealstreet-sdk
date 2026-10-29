use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use tokio::task::{AbortHandle, spawn_local};

use crate::AccountPort;

#[derive(Default)]
struct ScopeState {
    disposed: bool,
    tasks: Vec<AbortHandle>,
    children: Vec<Scope>,
}

/// Owns the tasks of an algo, a socket or an account. Disposing aborts every
/// task in it and in its children; later spawns into it are dropped.
#[derive(Clone, Default)]
pub struct Scope(Rc<RefCell<ScopeState>>);

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn child(&self) -> Scope {
        let child = Scope::new();
        let mut state = self.0.borrow_mut();
        if state.disposed {
            child.dispose();
        } else {
            state.tasks.retain(|task| !task.is_finished());
            state.children.push(child.clone());
        }
        child
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + 'static) {
        if self.is_disposed() {
            return;
        }
        let handle = spawn_local(task).abort_handle();
        let mut state = self.0.borrow_mut();
        state.tasks.retain(|task| !task.is_finished());
        state.tasks.push(handle);
    }

    pub fn is_disposed(&self) -> bool {
        self.0.borrow().disposed
    }

    pub fn dispose(&self) {
        let (tasks, children) = {
            let mut state = self.0.borrow_mut();
            if state.disposed {
                return;
            }
            state.disposed = true;
            (
                std::mem::take(&mut state.tasks),
                std::mem::take(&mut state.children),
            )
        };
        for task in tasks {
            task.abort();
        }
        for child in children {
            child.dispose();
        }
    }
}

/// What an algo gets: its account and the scope its tasks live in.
#[derive(Clone)]
pub struct AlgoCx {
    pub port: Rc<dyn AccountPort>,
    pub scope: Scope,
}

impl AlgoCx {
    pub fn new(port: Rc<dyn AccountPort>, scope: Scope) -> Self {
        Self { port, scope }
    }

    /// A context for one algo instance, disposed with this one.
    pub fn child(&self) -> AlgoCx {
        Self {
            port: self.port.clone(),
            scope: self.scope.child(),
        }
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + 'static) {
        self.scope.spawn(task);
    }

    pub async fn sleep_ms(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    /// Wall-clock epoch milliseconds.
    pub fn now_ms(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0)
    }
}
