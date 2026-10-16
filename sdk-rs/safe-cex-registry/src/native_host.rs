//! The production `RustVenueHost`: venues share one native runtime and get a
//! fresh terminal HTTP adapter per construction. Requires a tokio `LocalSet`.
use crate::rust_venues::{BackpackDependencies, RustVenueHost};
use safe_cex_exchange::{native::native_runtime, runtime::ExchangeError, websocket::WsRuntime};
use safe_cex_transport::{RestAdapter, native::reqwest_adapter};
use safe_cex_venue_binance::exchange_classes::ExchangeDependencies;
use std::{rc::Rc, sync::Arc};

pub type AdapterFactory = Rc<dyn Fn() -> Result<Arc<dyn RestAdapter>, ExchangeError>>;
pub struct NativeVenueHost {
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
}
/// Direct transport and real HTTP, as `directTransport` is the TypeScript default.
pub fn native_venue_host() -> Rc<NativeVenueHost> {
    native_venue_host_with(
        native_runtime(),
        Rc::new(|| {
            reqwest_adapter()
                .map(|adapter| Arc::new(adapter) as Arc<dyn RestAdapter>)
                .map_err(|e| ExchangeError::new(e.to_string()))
        }),
    )
}
/// A host over a caller-chosen runtime and terminal adapter (another transport or a test origin).
pub fn native_venue_host_with(
    runtime: Rc<dyn WsRuntime>,
    adapter: AdapterFactory,
) -> Rc<NativeVenueHost> {
    Rc::new(NativeVenueHost { runtime, adapter })
}
impl RustVenueHost for NativeVenueHost {
    fn binance(&self) -> Result<ExchangeDependencies, ExchangeError> {
        Ok(safe_cex_venue_binance::native_host::native_dependencies(
            self.runtime.clone(),
            (self.adapter)()?,
        ))
    }
    fn backpack(&self) -> Result<BackpackDependencies, ExchangeError> {
        let inputs = safe_cex_venue_backpack::native_host::native_inputs(
            self.runtime.clone(),
            (self.adapter)()?,
        );
        Ok(BackpackDependencies {
            runtime: inputs.runtime,
            build_http: inputs.build_http,
            install_transport: inputs.install_transport,
            date_parse: inputs.date_parse,
        })
    }
}
