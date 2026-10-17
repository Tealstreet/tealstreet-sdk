//! Shared operation carrier used by the live legacy class, lifecycle and streams.
use std::rc::Rc;

pub type CanonicalCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub struct CanonicalOps<V, E> {
    pub undefined: Rc<dyn Fn() -> V>,
    pub object: Rc<dyn Fn(Vec<(&'static str, V)>) -> V>,
    pub array: Rc<dyn Fn(Vec<V>) -> V>,
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, E>>,
    pub global: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub invoke: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub function: Rc<dyn Fn(&str, usize, CanonicalCallback<V, E>) -> V>,
    pub spread: Rc<dyn Fn(&V) -> Result<Vec<V>, E>>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub strict_equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub string: Rc<dyn Fn(&str) -> V>,
    pub number: Rc<dyn Fn(f64) -> V>,
    pub subtract: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub get_key: Rc<dyn Fn(&V, &V) -> Result<V, E>>,
    pub error: Rc<dyn Fn(&str) -> E>,
}
