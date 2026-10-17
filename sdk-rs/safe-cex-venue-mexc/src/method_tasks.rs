//! Value and owned-method task boundaries shared by the production MEXC binding.
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// Native values retain identity and UTF-16 strings. Coercion/property/equality
/// bindings implement the source intrinsics, including Symbol's distinct
/// abstract ToString (throws) and global String (returns a description).
pub trait Runtime: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn undefined(&self) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    /// SameValueZero for Set membership: NaN matches NaN and +/-0 match.
    fn same_value(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn is_text(&self, value: &Self::Value, text: &str) -> bool;
    fn field(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn abstract_string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn global_string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn log(&self, text: &[u16], severity: &'static str) -> Result<(), Self::Error>;
    fn new_error(&self, text: Vec<u16>) -> Self::Error;
}

/// Source value construction, optional chaining, ordinary JSON and object spread.
pub trait SessionRuntime: Runtime {
    fn text(&self, units: Vec<u16>) -> Self::Value;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn json_parse(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn stringify(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// CreateDataProperty for object-literal members; does not invoke inherited setters.
    fn define_field(
        &self,
        value: &Self::Value,
        key: &str,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
    fn set_field(
        &self,
        value: &Self::Value,
        key: &str,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
}

/// Host-owned completion is started before await and survives observer removal.
pub trait Completion<V, E> {
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<V, E>>;
}
pub trait TaskHost: SessionRuntime {
    type Operation: Completion<Self::Value, Self::Error>;
    type Resolution: Completion<Self::Value, Self::Error>;
    /// Source Promise resolution: primitive returns settle immediately; thenables
    /// read then now and schedule its invocation/adoption on the runtime queue.
    fn resolve_return(&self, value: Self::Value) -> Result<Rc<Self::Resolution>, Self::Error>;
    /// Eagerly poll the whole method through its synchronous prefix and retain
    /// the task through completion; resume awaits on the source microtask queue.
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>);
}
struct State<V, E> {
    value: Option<Result<V, E>>,
    waker: Option<Waker>,
}
/// Observation only: dropping this future does not cancel the method task.
pub struct MethodObserver<V, E> {
    state: Rc<RefCell<State<V, E>>>,
}
impl<V, E> Future for MethodObserver<V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.state.borrow_mut();
        if let Some(v) = s.value.take() {
            Poll::Ready(v)
        } else {
            s.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
pub(crate) fn start<H: TaskHost>(
    host: Rc<H>,
    body: impl Future<Output = Result<H::Value, H::Error>> + 'static,
) -> MethodObserver<H::Value, H::Error> {
    let state = Rc::new(RefCell::new(State {
        value: None,
        waker: None,
    }));
    let output = state.clone();
    let resolver = host.clone();
    host.start_task(Box::pin(async move {
        let value = match body.await {
            Err(error) => Err(error),
            Ok(value) => match resolver.resolve_return(value) {
                Err(error) => Err(error),
                Ok(op) => {
                    Observe {
                        op,
                        yielded: true,
                        marker: std::marker::PhantomData,
                    }
                    .await
                }
            },
        };
        let waker = {
            let mut s = output.borrow_mut();
            s.value = Some(value);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake()
        }
    }));
    MethodObserver { state }
}
struct Observe<O, V, E> {
    op: Rc<O>,
    yielded: bool,
    marker: std::marker::PhantomData<fn() -> (V, E)>,
}
impl<O: Completion<V, E>, V, E> Future for Observe<O, V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let s = self.get_mut();
        if !s.yielded {
            s.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        s.op.poll(cx)
    }
}
pub(crate) async fn observe<O: Completion<V, E>, V, E>(op: Result<Rc<O>, E>) -> Result<V, E> {
    Observe {
        op: op?,
        yielded: false,
        marker: std::marker::PhantomData,
    }
    .await
}
