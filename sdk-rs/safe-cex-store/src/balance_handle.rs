//! A captured native balance reference, projected only by an explicit typed read.
use safe_cex_types::{Balance, StoreReadError};
use std::{any::Any, rc::Rc};

/// Implementations retain the captured referent without retaining its Store owner.
pub trait BalanceReference {
    fn read(&self) -> Result<Balance, StoreReadError>;
    fn native_value(&self) -> Rc<dyn Any>;
}

#[derive(Clone)]
pub struct BalanceHandle(Rc<dyn BalanceReference>);

impl BalanceHandle {
    pub fn new(reference: Rc<dyn BalanceReference>) -> Self {
        Self(reference)
    }

    pub fn read(&self) -> Result<Balance, StoreReadError> {
        self.0.read()
    }

    /// Native adapters downcast this same value without performing a typed read.
    pub fn native_value(&self) -> Rc<dyn Any> {
        self.0.native_value()
    }
}
