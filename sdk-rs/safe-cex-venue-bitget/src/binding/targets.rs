//! Manifest target path -> callable running that ported Rust body on `rt.this`.
use super::{rt::Rt, value::{Js, Thrown}};

pub fn bind(rt: &Rt, owner: &str, name: &str, targets: &[&str]) -> Result<Js, Thrown> {
    let _ = (rt, targets);
    Err(Thrown::error(
        "Error",
        &format!("unbound Bitget member {owner}.{name}"),
    ))
}
