//! One concrete host-trait implementation per ported body module.
use super::{rt::Shared, value::Js};
use std::rc::Weak;

pub mod assembly;

pub fn register_all(globals: &Js, weak: Weak<Shared>) {
    assembly::register(globals, weak);
}
