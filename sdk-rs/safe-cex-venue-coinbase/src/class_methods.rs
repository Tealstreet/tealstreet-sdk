//! Source arrow values retain their original receiver. Only private class storage
//! may omit a redundant self edge; every outward read returns a strong value.
use std::rc::Rc;
type Body<C, R> = Rc<dyn Fn(Rc<C>) -> R>;
/// Private field storage is never exported or moved between instances. A local
/// body uses the owning receiver supplied by capture; a foreign body retains its
/// immutable origin. This avoids the ordinary class -> own arrow -> class cycle.
pub(crate) struct ClassMethodSlot<C, R> {
    body: Body<C, R>,
    origin: Option<Rc<C>>,
}
impl<C, R> ClassMethodSlot<C, R> {
    pub(crate) fn new(body: impl Fn(Rc<C>) -> R + 'static) -> Self {
        Self {
            body: Rc::new(body),
            origin: None,
        }
    }
    pub(crate) fn assigned(value: BoundClassMethod<C, R>, owner: &Rc<C>) -> Self {
        let origin = (!Rc::ptr_eq(&value.receiver, owner)).then_some(value.receiver);
        Self {
            body: value.body,
            origin,
        }
    }
    pub(crate) fn capture(&self, owner: Rc<C>) -> BoundClassMethod<C, R> {
        BoundClassMethod {
            body: self.body.clone(),
            receiver: self.origin.clone().unwrap_or(owner),
        }
    }
}
pub struct BoundClassMethod<C, R> {
    body: Body<C, R>,
    receiver: Rc<C>,
}
impl<C, R> Clone for BoundClassMethod<C, R> {
    fn clone(&self) -> Self {
        Self {
            body: self.body.clone(),
            receiver: self.receiver.clone(),
        }
    }
}
impl<C, R> BoundClassMethod<C, R> {
    pub fn call(&self) -> R {
        (self.body)(self.receiver.clone())
    }
    pub fn same_function(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.body, &other.body) && Rc::ptr_eq(&self.receiver, &other.receiver)
    }
}
