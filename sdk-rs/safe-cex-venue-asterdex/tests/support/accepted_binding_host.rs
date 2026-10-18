use crate::{binding::Rt, native_exchange::AsterDEXDependencies};
use safe_cex_exchange::{
    exchange::ExchangeSettings,
    js_runtime::{base_exchange::BaseExchangeParts, client},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{future::Future, rc::Rc, sync::Arc};

mod scheduler {
    use crate as safe_cex_venue_asterdex;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/exchange_runtime.rs"
    ));
}

pub struct Fixture {
    pub rt: Rt,
    pub runtime: Rc<scheduler::Runtime>,
    pub wire: Arc<scheduler::Wire>,
}

impl Fixture {
    pub fn new(input: &Value) -> Self {
        let runtime = scheduler::runtime_fixture();
        #[expect(clippy::arc_with_non_send_sync)]
        let wire = Arc::new(scheduler::Wire::default());
        let mut options = json!({"key":"","secret":""});
        if input["version"] == "v3" {
            options["key"] = json!("0x14791697260e4c9a71f18484c9f997b308e59325");
            options["secret"] = json!(format!("0x{:064x}", 1));
            options["applicationId"] = json!("0x1111111111111111111111111111111111111111");
        }
        let http_runtime = runtime.clone();
        let http_wire = wire.clone();
        let rt = crate::binding::assemble(
            BaseExchangeParts {
                name: ExchangeName::Asterdex,
                options: serde_json::from_value(options).unwrap(),
                store: Rc::new(DefaultStore::new()),
                settings: ExchangeSettings {
                    supports_public_only: true,
                    positions_supported: Some(true),
                    ..Default::default()
                },
                external_public_data: None,
            },
            AsterDEXDependencies {
                runtime: runtime.clone(),
                http: Rc::new(move |options| {
                    http_runtime.http_options.borrow_mut().push(options.clone());
                    Ok(client::rest_http_host(
                        http_runtime.clone(),
                        http_wire.clone(),
                    ))
                }),
                external_public_data: None,
            },
        )
        .expect("actual Aster assembly");
        Self { rt, runtime, wire }
    }

    pub fn drive<F: Future>(&self, future: F) -> F::Output {
        scheduler::drive(&self.runtime, future)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.rt
            .base()
            .dispose()
            .expect("assembled fixture disposal");
    }
}
