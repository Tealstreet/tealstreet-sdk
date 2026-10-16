//! Eager, detached promises over a host FIFO microtask queue. Dropping an observer
//! does not cancel the operation. Object fulfillment retains shared identity.
use super::transaction_signing::{AuthTokenEncoding, SigningResult};
use std::{cell::RefCell, rc::Rc};
pub type Shared<T> = Rc<RefCell<T>>;
pub fn shared<T>(value: T) -> Shared<T> {
    Rc::new(RefCell::new(value))
}
pub type PromiseResult<T> = SigningResult<Rc<T>>;
pub type ObjectPromise<T> = NativePromise<RefCell<T>>;
pub type ObjectResolver<T> = PromiseResolver<RefCell<T>>;
pub trait TransactionRuntime {
    /// Enqueue at the tail; never execute inline, including for fulfilled promises.
    fn enqueue_microtask(&self, task: Box<dyn FnOnce()>);
    fn now_millis(&self) -> f64;
    fn auth_token_encoding(&self) -> AuthTokenEncoding;
}
type Observer<T> = Box<dyn FnOnce(PromiseResult<T>)>;
struct State<T> {
    result: Option<PromiseResult<T>>,
    observers: Vec<Observer<T>>,
}
pub struct NativePromise<T> {
    state: Shared<State<T>>,
    runtime: Rc<dyn TransactionRuntime>,
}
impl<T> Clone for NativePromise<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            runtime: self.runtime.clone(),
        }
    }
}
pub struct PromiseResolver<T>(NativePromise<T>);
impl<T: 'static> NativePromise<T> {
    pub fn pending(runtime: Rc<dyn TransactionRuntime>) -> (Self, PromiseResolver<T>) {
        let promise = Self {
            state: shared(State {
                result: None,
                observers: vec![],
            }),
            runtime,
        };
        (promise.clone(), PromiseResolver(promise))
    }
    pub fn settled(runtime: Rc<dyn TransactionRuntime>, result: SigningResult<T>) -> Self {
        let (promise, resolver) = Self::pending(runtime);
        resolver.settle(result);
        promise
    }
    pub fn subscribe(&self, observer: impl FnOnce(PromiseResult<T>) + 'static) {
        let mut state = self.state.borrow_mut();
        if let Some(result) = state.result.clone() {
            drop(state);
            self.runtime
                .enqueue_microtask(Box::new(move || observer(result)));
        } else {
            state.observers.push(Box::new(observer));
        }
    }
}
impl<T: 'static> PromiseResolver<T> {
    pub fn settle(self, result: SigningResult<T>) {
        self.settle_shared(result.map(Rc::new));
    }
    pub fn settle_shared(self, result: PromiseResult<T>) {
        let observers = {
            let mut state = self.0.state.borrow_mut();
            if state.result.is_some() {
                return;
            }
            state.result = Some(result.clone());
            std::mem::take(&mut state.observers)
        };
        for observer in observers {
            let result = result.clone();
            self.0
                .runtime
                .enqueue_microtask(Box::new(move || observer(result)));
        }
    }
}
