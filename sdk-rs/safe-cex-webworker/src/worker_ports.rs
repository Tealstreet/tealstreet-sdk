//! Port helpers from store.webworker.ts. One object is shared by a worker host.
//! Host methods follow the synchronous postMessage / asynchronous setTimeout
//! contract; Result represents a thrown postMessage value without inspecting it.
use indexmap::IndexMap;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use std::{cell::RefCell, rc::Rc};

pub const THROTTLE_HIDDEN_MS: f64 = 2500.0;

/// Opaque messages stay behind Rc, including objects, arrays and UTF-16 strings.
pub trait WorkerMessage {
    /// Pure JavaScript truthiness; evaluating it must not run message code.
    fn is_truthy(&self) -> bool;
}

pub trait WorkerPort<M> {
    type Error;
    fn post_message(&self, message: &Rc<M>) -> Result<(), Self::Error>;
}

pub type WorkerConfigHandler<C> = Rc<dyn Fn(&C)>;

// ts: registerWorkerConfigHandler. The module list is an explicit shared context.
pub fn register_worker_config_handler<C>(
    handlers: &RefCell<Vec<WorkerConfigHandler<C>>>,
    handler: WorkerConfigHandler<C>,
) {
    handlers.borrow_mut().push(handler);
}

pub struct PortState<M, Timer> {
    pub visible: bool,
    pub throttle_enabled: bool,
    pub last_delivery_time: f64,
    pub pending_update: Option<Rc<M>>,
    pub throttle_timer: Option<Timer>,
}
impl<M, Timer> Default for PortState<M, Timer> {
    fn default() -> Self {
        Self {
            visible: true,
            throttle_enabled: false,
            last_delivery_time: 0.0,
            pending_update: None,
            throttle_timer: None,
        }
    }
}

pub type PortList<P> = Rc<RefCell<Vec<Rc<P>>>>;
pub type SharedPortState<M, T> = Rc<RefCell<PortState<M, T>>>;

/// Own-key registries, exposed for the still-pending worker init/message wiring.
/// Keep no outstanding RefCell borrows across helper calls or host callbacks.
/// Group arrays are shared: broadcasts see in-place appends but retain the old
/// array when cleanup replaces a group, exactly like the source for-of loop.
pub struct WorkerPorts<M, P: ?Sized, H: WorkerPortTimers> {
    pub ports: RefCell<IndexMap<String, PortList<P>>>,
    pub ports_by_sub_id: RefCell<IndexMap<String, Rc<P>>>,
    states: RefCell<Vec<(Rc<P>, SharedPortState<M, H::Timer>)>>,
    pub host: Rc<H>,
}

impl<M, P: ?Sized, H: WorkerPortTimers> WorkerPorts<M, P, H> {
    pub fn new(host: Rc<H>) -> Rc<Self> {
        Rc::new(Self {
            ports: RefCell::new(IndexMap::new()),
            ports_by_sub_id: RefCell::new(IndexMap::new()),
            states: RefCell::new(Vec::new()),
            host,
        })
    }

    /// Source Map.get: compare port objects, never names or post callbacks.
    pub fn state(&self, port: &Rc<P>) -> Option<SharedPortState<M, H::Timer>> {
        self.states
            .borrow()
            .iter()
            .find(|(candidate, _)| Rc::ptr_eq(candidate, port))
            .map(|(_, state)| state.clone())
    }

    pub fn ensure_state(&self, port: &Rc<P>) -> SharedPortState<M, H::Timer> {
        if let Some(state) = self.state(port) {
            return state;
        }
        let state = Rc::new(RefCell::new(PortState::default()));
        self.states.borrow_mut().push((port.clone(), state.clone()));
        state
    }

    // ts: cleanupPortState. Do not clear the timer property on retained aliases.
    pub fn cleanup_port_state(&self, port: &Rc<P>) {
        if let Some(state) = self.state(port) {
            let timer = state.borrow().throttle_timer.clone();
            if let Some(timer) = timer.filter(|timer| self.host.timer_is_truthy(timer)) {
                self.host.clear_timeout(&timer);
            }
        }
        self.states
            .borrow_mut()
            .retain(|(candidate, _)| !Rc::ptr_eq(candidate, port));
    }

    // ts: cleanupPortEverywhere.
    pub fn cleanup_port_everywhere(&self, port: &Rc<P>) {
        self.cleanup_port_state(port);
        let keys: Vec<_> = self.ports.borrow().keys().cloned().collect();
        for key in keys {
            let old = self.ports.borrow()[&key].clone();
            let next: Vec<_> = old
                .borrow()
                .iter()
                .filter(|candidate| !Rc::ptr_eq(candidate, port))
                .cloned()
                .collect();
            if next.is_empty() {
                self.ports.borrow_mut().shift_remove(&key);
            } else {
                self.ports
                    .borrow_mut()
                    .insert(key, Rc::new(RefCell::new(next)));
            }
        }
        self.ports_by_sub_id
            .borrow_mut()
            .retain(|_, candidate| !Rc::ptr_eq(candidate, port));
    }
}

impl<M, P: ?Sized, H> WorkerPorts<M, P, H>
where
    M: WorkerMessage + 'static,
    P: WorkerPort<M> + 'static,
    H: WorkerPortTimers + 'static,
{
    fn clear_pending(&self, state: &SharedPortState<M, H::Timer>) {
        let timer = state.borrow().throttle_timer.clone();
        if let Some(timer) = timer.filter(|timer| self.host.timer_is_truthy(timer)) {
            self.host.clear_timeout(&timer);
            state.borrow_mut().throttle_timer = None;
        }
        state.borrow_mut().pending_update = None;
    }

    // ts: deliverToPortWithThrottle.
    pub fn deliver_to_port_with_throttle(self: &Rc<Self>, port: Rc<P>, message: Rc<M>) {
        let state = self.ensure_state(&port);
        let should_throttle = {
            let state = state.borrow();
            state.throttle_enabled && !state.visible
        };
        if !should_throttle {
            if port.post_message(&message).is_err() {
                self.cleanup_port_everywhere(&port);
                return;
            }
            self.clear_pending(&state);
            return;
        }

        let now = self.host.now_ms();
        let elapsed = now - state.borrow().last_delivery_time;
        if elapsed >= THROTTLE_HIDDEN_MS {
            state.borrow_mut().last_delivery_time = now;
            if port.post_message(&message).is_err() {
                self.cleanup_port_everywhere(&port);
                return;
            }
            self.clear_pending(&state);
        } else {
            state.borrow_mut().pending_update = Some(message);
            let timer = state.borrow().throttle_timer.clone();
            if !timer.is_some_and(|timer| self.host.timer_is_truthy(&timer)) {
                let registry = self.clone();
                let timer = self.host.set_timeout(
                    Rc::new(move || {
                        // Look up again: cleanup/replacement may change this state.
                        let current = registry.state(&port);
                        if let Some(current) = current {
                            let pending = current.borrow().pending_update.clone();
                            if let Some(pending) = pending.filter(|message| message.is_truthy()) {
                                if port.post_message(&pending).is_err() {
                                    registry.cleanup_port_everywhere(&port);
                                    return;
                                }
                                current.borrow_mut().last_delivery_time = registry.host.now_ms();
                                current.borrow_mut().pending_update = None;
                            }
                            current.borrow_mut().throttle_timer = None;
                        }
                    }),
                    THROTTLE_HIDDEN_MS - elapsed,
                );
                state.borrow_mut().throttle_timer = Some(timer);
            }
        }
    }

    // ts: broadcastWithThrottle. The source objects also expose inherited
    // Object.prototype entries; this own-key native registry excludes them.
    pub fn broadcast_with_throttle(
        self: &Rc<Self>,
        port_key: &str,
        message: Rc<M>,
        sub_id: Option<&str>,
    ) {
        let Some(mut valid_ports) = self.ports.borrow().get(port_key).cloned() else {
            return;
        };
        if let Some(sub_id) = sub_id.filter(|id| !id.is_empty()) {
            if let Some(port) = self.ports_by_sub_id.borrow().get(sub_id).cloned() {
                valid_ports = Rc::new(RefCell::new(vec![port]));
            }
        }
        let mut index = 0;
        loop {
            let port = valid_ports.borrow().get(index).cloned();
            let Some(port) = port else { break };
            self.deliver_to_port_with_throttle(port, message.clone());
            index += 1;
        }
    }
}
