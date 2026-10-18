//! Production native inputs for `BackpackExchange::new`: the shared HttpClient over
//! a real terminal adapter, the core transport install, and `Date.parse`.
use crate::{
    adapter::DateParser,
    api_factory::Options,
    native_http::{BuildHttp, InstallTransport},
};
use safe_cex_exchange::{
    http::{HttpClientOptions, install_transport_adapter, safe_cex_axios_create},
    native::{NativeHttpEvents, NativeHttpHost, nanoid},
    runtime::{ExchangeError, ExchangeRuntime},
    websocket::WsRuntime,
};
use safe_cex_transport::{RestAdapter, RestAdapterVariant, RestRequest, TransportRegistry};
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct NativeBackpackInputs {
    pub runtime: Rc<dyn WsRuntime>,
    pub build_http: BuildHttp,
    pub install_transport: InstallTransport,
    pub date_parse: DateParser,
}

/// Fresh inputs for one construction; `adapter` is the terminal HTTP adapter.
pub fn native_inputs(
    runtime: Rc<dyn WsRuntime>,
    adapter: Arc<dyn RestAdapter>,
) -> NativeBackpackInputs {
    native_inputs_signed(runtime, adapter, None, NativeHttpEvents::default())
}

/// Native inputs retaining one account's common HTTP signing capability.
/// Private websocket authentication keeps the source's local-secret requirement.
pub fn native_inputs_signed(
    runtime: Rc<dyn WsRuntime>,
    adapter: Arc<dyn RestAdapter>,
    remote: Option<safe_cex_exchange::js_runtime::base_exchange::HostedSigner>,
    events: NativeHttpEvents,
) -> NativeBackpackInputs {
    let registry = Rc::new(TransportRegistry::default());
    let (build_runtime, build_registry) = (runtime.clone(), registry.clone());
    let build_http: BuildHttp = Box::new(move |config, options: Rc<RefCell<Options>>| {
        let serialize = config.serialize.clone();
        #[expect(clippy::arc_with_non_send_sync)]
        let params_serializer: safe_cex_exchange::http::ParamsSerializer =
            Arc::new(move |params: &serde_json::Value| {
                serialize(params.clone().into())
                    .map_err(|e: ExchangeError| safe_cex_transport::TransportError::Host(e.message))
            });
        let mut request = RestRequest {
            base_url: Some(config.base_url.into()),
            ..Default::default()
        };
        for (name, value) in config.headers {
            request.headers.insert(name.into(), value.into());
        }
        let exchange_runtime: Rc<dyn ExchangeRuntime> = build_runtime.clone();
        safe_cex_axios_create(
            HttpClientOptions {
                params_serializer: Some(params_serializer),
                response_transform: None,
                exchange: ExchangeName::Backpack,
                options: options.borrow().clone(),
                request,
                provider: Some(build_runtime.transport()),
                signer: remote.as_ref().map(|(signer, _)| {
                    safe_cex_exchange::js_runtime::client::transport_signer(signer.clone())
                }),
                account_name: remote.as_ref().and_then(|(_, name)| name.clone()),
                adapter: adapter.clone(),
                host: Rc::new(NativeHttpHost {
                    events: events.clone(),
                }),
                runtime: exchange_runtime,
                store: None,
                stats: None,
                logs: None,
                every: vec![],
                periodic: vec![],
                next_id: Arc::new(nanoid),
                signing_now: Arc::new(|| safe_cex_transport::native::wall_clock_ms() as u64),
            },
            &build_registry,
        )
        .map_err(|e| ExchangeError::new(e.to_string()))
    });
    let install_runtime = runtime.clone();
    let install_transport: InstallTransport = Rc::new(move |client, options| {
        let transport = install_runtime.transport();
        install_transport_adapter(
            client,
            &registry,
            &options.borrow(),
            Some(&transport),
            RestAdapterVariant::Default,
        );
        Ok(())
    });
    // Date.parse with the host's local zone taken as UTC, as native servers run.
    let date_parse: DateParser =
        Rc::new(|text| Ok(safe_cex_num::js_date_parse_utf16(&text, |local| local)));
    NativeBackpackInputs {
        runtime,
        build_http,
        install_transport,
        date_parse,
    }
}
