use super::rt::Shared;
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    library::install_library,
    value::Js,
};
use std::rc::Weak;

pub(crate) fn build(weak: Weak<Shared>) -> Js {
    let globals = Js::object(vec![]);
    install_builtins(&globals, weak.clone());
    install_enums(&globals);
    install_library(&globals, weak);
    globals
}
