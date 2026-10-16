//! WOOXExchange members ported outside the legacy host modules: response and
//! transaction helpers, history readers, order paging and the lifecycle bodies.
use crate::binding::{
    rt::{Rt, Shared},
    value::{Js, Thrown},
};
use std::rc::{Rc, Weak};

pub(crate) fn bind(_rt: &Rt, _name: &str) -> Option<Result<Js, Thrown>> {
    None
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}

fn canonical_safe_integer(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && (b'1'..=b'9').contains(&bytes[0])
        && bytes.iter().all(u8::is_ascii_digit)
        && id.parse::<u64>().is_ok_and(|n| n <= 9_007_199_254_740_991)
}

// ts: WOOXExchange.clientOrderIdRule
pub(crate) fn client_order_id_rule() -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
        "WOO".into(),
        "/^[1-9]\\d*$/".into(),
        Rc::new(canonical_safe_integer),
        Default::default(),
    )
}
