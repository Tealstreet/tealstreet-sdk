//! Manifest target path -> callable running that ported Rust body on `rt.this`.
use super::{
    hosts,
    rt::Rt,
    value::{Js, Thrown},
};

pub fn bind(rt: &Rt, owner: &str, name: &str, targets: &[&str]) -> Result<Js, Thrown> {
    for group in [
        hosts::positions::bind,
        hosts::orders::bind,
        hosts::actions::bind,
        hosts::public::bind,
        hosts::private::bind,
    ] {
        if let Some(callable) = group(rt, owner, name, targets)? {
            return Ok(callable);
        }
    }
    Err(Thrown::error(
        "Error",
        &format!("Bitget binding has no adapter for {owner}.{name} ({})", targets.join(", ")),
    ))
}
