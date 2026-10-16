//! Dispatch assembled socket methods to their owned native state and bodies.
use super::runtime::Host;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
pub(super) fn invoke(h: Host, class: &str, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    if class == "public" {
        let owner =
            h.0.shared
                .extra
                .public
                .borrow()
                .iter()
                .find(|v| v.this.same_reference(&h.0.this))
                .cloned()
                .ok_or_else(|| Thrown::error("Error", "GRVT public socket is not assembled"))?;
        owner.invoke(name, args)
    } else {
        let owner =
            h.0.shared
                .extra
                .private
                .borrow()
                .iter()
                .find(|v| v.this.same_reference(&h.0.this))
                .cloned()
                .ok_or_else(|| Thrown::error("Error", "GRVT private socket is not assembled"))?;
        owner.invoke(name, args)
    }
}
