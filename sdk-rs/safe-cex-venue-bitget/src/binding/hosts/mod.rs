//! One concrete host-trait implementation per group of ported body modules.
use super::{rt::Shared, value::Js};
use std::rc::Weak;

pub mod actions;
pub mod assembly;
pub mod orders;
pub mod positions;
pub mod private;
pub mod public;

pub(crate) fn register_all(globals: &Js, weak: Weak<Shared>) {
    assembly::register(globals, weak.clone());
    positions::register(globals, weak.clone());
    orders::register(globals, weak.clone());
    actions::register(globals, weak.clone());
    public::register(globals, weak.clone());
    private::register(globals, weak);
}
