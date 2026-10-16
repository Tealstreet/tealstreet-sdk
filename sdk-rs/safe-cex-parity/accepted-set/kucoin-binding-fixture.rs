// Accepted-set instrumentation: retain the existing scripted I/O and production
// language operations. No venue method or decoder is replaced by this fixture.
use crate as venue_crate;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../safe-cex-parity/accepted-set/kucoin-io-fixture.rs"
));
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    library::install_library,
    scope::{Scope, ScopeShared},
    value::Js,
};

pub fn host() -> Scope<crate::binding::Extra> {
    let runtime = Rc::new(io::Runtime::default());
    let options = serde_json::from_value(serde_json::json!({
        "key":"synthetic-key","secret":"synthetic-secret","passphrase":"synthetic-pass"
    }))
    .unwrap();
    let api = Rc::new_cyclic(|me| crate::binding::http::ApiHost {
        me: me.clone(),
        runtime: runtime.clone(),
        options,
        hosted_signer: None,
        spot: false,
        http: Rc::new(|_| panic!("pure language fixture must not request HTTP")),
        registry: Rc::new(Default::default()),
        clock: Default::default(),
        retry_clock: runtime.clone(),
        api: RefCell::new(None),
    });
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        api.clone(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        crate::binding::Extra {
            supers: RefCell::default(),
            parts: RefCell::new(None),
            api,
        },
    ));
    let weak = Rc::downgrade(&shared);
    install_builtins(&shared.globals, weak.clone());
    install_enums(&shared.globals);
    install_library(&shared.globals, weak.clone());
    let ws: Rc<dyn safe_cex_exchange::websocket::WsRuntime> = runtime;
    crate::binding::globals::install(&shared.globals, weak, &ws).unwrap();
    let this = Js::object(vec![(
        "store",
        safe_cex_exchange::js_runtime::store::facade(shared.store.clone()),
    )]);
    *shared.exchange.borrow_mut() = this.clone();
    Scope { shared, this }
}

/// Inspect the actual primitive VM result without serializing/redecoding it.
pub fn primitive(
    result: Result<Js, safe_cex_exchange::js_runtime::value::Thrown>,
) -> serde_json::Value {
    use serde_json::json;
    match result {
        Ok(Js::Undefined) => json!({"kind":"undefined"}),
        Ok(Js::Null) => json!({"kind":"null"}),
        Ok(Js::Bool(value)) => json!({"kind":"boolean","value":value}),
        Ok(Js::String(value)) => json!({"kind":"string","units":value.as_ref()}),
        Ok(Js::Number(value)) => json!({"kind":"number","nan":value.is_nan(),
            "bits":if value.is_nan(){None}else{Some(format!("{:016x}",value.to_bits()))}}),
        Ok(_) => panic!("primitive native observation requires a primitive result"),
        Err(error) => json!({"kind":"error","name":error.value.get("name").unwrap().to_display(),
            "message":error.value.get("message").unwrap().to_display()}),
    }
}

pub fn connect_read(
    value: &Js,
    key: &str,
) -> Result<Js, safe_cex_exchange::js_runtime::value::Thrown> {
    match key {
        "instanceServers?.0" => {
            let servers = value.get("instanceServers")?;
            if servers.is_nullish() {
                Ok(Js::Undefined)
            } else {
                servers.get("0")
            }
        }
        "?.endpoint" if value.is_nullish() => Ok(Js::Undefined),
        "?.endpoint" => value.get("endpoint"),
        _ => value.get(key),
    }
}
