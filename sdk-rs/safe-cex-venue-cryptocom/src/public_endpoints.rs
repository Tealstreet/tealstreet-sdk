//! The module's exported array: retained handles observe the same mutable contents.
use std::{cell::RefCell, rc::Rc};

type Slots = Vec<Option<Vec<u16>>>;
fn module_array() -> Rc<RefCell<Slots>> {
    thread_local! {
        static ARRAY: Rc<RefCell<Slots>> = Rc::new(RefCell::new(
            crate::types::ENDPOINTS[..9].iter().map(|(_,value)| Some(value.encode_utf16().collect())).collect()
        ));
    }
    ARRAY.with(Rc::clone)
}

/// The constant binding locates this thread's module array. Clone retains its identity.
#[derive(Debug)]
pub struct PublicEndpoints(Option<Rc<RefCell<Slots>>>);
impl Clone for PublicEndpoints {
    fn clone(&self) -> Self {
        Self(Some(self.array()))
    }
}
impl PublicEndpoints {
    pub(crate) const fn module() -> Self {
        Self(None)
    }
    fn array(&self) -> Rc<RefCell<Slots>> {
        self.0.as_ref().map_or_else(module_array, Rc::clone)
    }

    pub(crate) fn contains_units(&self, url: &[u16]) -> bool {
        let array = self.array();
        let slots = array.borrow();
        slots.iter().flatten().any(|endpoint| {
            endpoint.is_empty() || url.windows(endpoint.len()).any(|part| part == endpoint)
        })
    }
    pub fn same_reference(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.array(), &other.array())
    }
    pub fn slots(&self) -> Slots {
        self.array().borrow().clone()
    }
    pub fn push(&self, value: Vec<u16>) -> usize {
        let array = self.array();
        let mut slots = array.borrow_mut();
        slots.push(Some(value));
        slots.len()
    }
    pub fn set(&self, index: usize, value: Vec<u16>) {
        let array = self.array();
        let mut slots = array.borrow_mut();
        if slots.len() <= index {
            slots.resize(index + 1, None);
        }
        slots[index] = Some(value);
    }
    pub fn delete(&self, index: usize) {
        if let Some(slot) = self.array().borrow_mut().get_mut(index) {
            *slot = None;
        }
    }
    pub fn remove(&self, index: usize) -> Option<Vec<u16>> {
        let array = self.array();
        let mut slots = array.borrow_mut();
        if index < slots.len() {
            slots.remove(index)
        } else {
            None
        }
    }
    pub fn replace_contents(&self, values: Slots) {
        *self.array().borrow_mut() = values;
    }
}
