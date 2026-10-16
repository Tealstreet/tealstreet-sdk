use super::*;
use safe_cex_transport::{
    DirectTransport, PrivateProxy, ResolvedRouting, RestAdapterContext, RestAdapterRegion,
    RestAdapterVariant, RestTransport, RestTransportContext, RoutingOptions, TransportProvider,
    WsConnection,
};

#[derive(Default)]
struct Provider {
    rest_calls: Cell<usize>,
    adapter_regions: RefCell<Vec<Value>>,
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "construction-witness"
    }
    fn rest(&self, context: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        self.rest_calls.set(self.rest_calls.get() + 1);
        assert_eq!(context.exchange, &ExchangeName::Bybitinverse);
        assert_eq!(context.options.tealstreet_tunnel, Some(true));
        Ok(RestTransport {
            base_url: context.config.base_url.clone(),
            proxied: false,
            authorize: None,
        })
    }
    fn rest_adapter(&self, context: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        assert_eq!(context.exchange, &ExchangeName::Bybitinverse);
        let RestAdapterVariant::Region { region } = context.variant else {
            panic!("factory_region_native_expected_region_variant");
        };
        let observed = match region {
            None => json!({"undefined":true}),
            Some(RestAdapterRegion::Ja) => json!("JA"),
            Some(RestAdapterRegion::Sg) => json!("SG"),
            Some(RestAdapterRegion::Other(value)) => json!(value),
        };
        self.adapter_regions.borrow_mut().push(observed);
        None
    }
    fn proxy_prefix(&self, context: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(context)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        None
    }
    fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        None
    }
    fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
        options
    }
    fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
        None
    }
    fn ws_routing(&self, exchange: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(exchange, options)
    }
    fn ws_connection(&self, _: &str) -> Option<WsConnection> {
        None
    }
}

fn construct(region: Option<&str>) -> Value {
    let mut input = json!({"key":"key", "secret":"secret", "tealstreetTunnel":true});
    if let Some(region) = region {
        input
            .as_object_mut()
            .unwrap()
            .insert("proxyRegion".into(), json!(region));
    }
    let options: ExchangeOptions = serde_json::from_value(input).unwrap();
    let runtime = Rc::new(Runtime::default());
    let trace = Rc::new(RefCell::new(Vec::new()));
    let owner = Rc::new(Owner {
        base: BaseExchange::new(
            ExchangeName::Bybitinverse,
            options.clone(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            Default::default(),
        ),
        capabilities: Default::default(),
        accesses: RefCell::new(Vec::new()),
        trace: trace.clone(),
    });
    let host = Rc::new(Host {
        owner: Rc::downgrade(&owner),
        trace: trace.clone(),
        responses: RefCell::new(Vec::new()),
        failures: Cell::new(0),
    });
    let wire = Arc::new(Wire {
        replies: RefCell::new(VecDeque::new()),
        requests: RefCell::new(Vec::new()),
        trace,
        query_in_url: true,
    });
    let provider = Arc::new(Provider::default());
    let source_owner: Rc<dyn Exchange> = owner.clone();
    let result = create_api(
        BybitApiOptions {
            http: HttpClientOptions {
                params_serializer: None,
                response_transform: None,
                exchange: ExchangeName::Bybitinverse,
                options,
                request: RestRequest::default(),
                provider: Some(provider.clone()),
                signer: None,
                account_name: None,
                adapter: wire.clone(),
                host,
                runtime,
                store: None,
                stats: Some(Default::default()),
                logs: Some(Default::default()),
                every: Vec::new(),
                periodic: Vec::new(),
                next_id: Arc::new(|| panic!("factory_region_native_no_request_id")),
                signing_now: Arc::new(|| panic!("factory_region_native_no_signing")),
            },
            owner: Rc::downgrade(&source_owner),
            is_demo: false,
            rest_base_url: None,
            clock: VirtualClock::default(),
            retry_clock: Rc::new(Clock),
            payload_signer: None,
        },
        &TransportRegistry::default(),
    );
    let error = result.as_ref().err().map(ToString::to_string);
    let receipt = json!({
        "region":region.map_or_else(|| json!({"undefined":true}), |region| json!(region)),
        "constructed":result.is_ok(),
        "error":error,
        "restCalls":provider.rest_calls.get(),
        "providerCalls":provider.adapter_regions.borrow().len(),
        "forwardedRegions":provider.adapter_regions.borrow().clone(),
        "physicalCalls":wire.requests.borrow().len(),
    });
    drop(result);
    receipt
}

pub fn collect_native() -> Vec<Value> {
    let inputs = [None, Some("JA"), Some("SG"), Some(""), Some("US")];
    assert_eq!(inputs.len(), 5, "factory_region_native_declared_floor");
    let cases: Vec<_> = inputs.into_iter().map(construct).collect();
    assert_eq!(
        cases.len(),
        5,
        "factory_region_native_complete_before_source"
    );
    for (index, expected_region) in [
        json!({"undefined":true}),
        json!("JA"),
        json!("SG"),
        json!(""),
        json!("US"),
    ]
    .into_iter()
    .enumerate()
    {
        let case = &cases[index];
        assert_eq!(
            case["region"], expected_region,
            "factory_region_native_input_inventory {index}"
        );
        assert_eq!(
            case["constructed"], true,
            "factory_region_native_construction {index}"
        );
        assert_eq!(
            case["error"],
            Value::Null,
            "factory_region_native_no_refusal {index}"
        );
        assert_eq!(
            case["restCalls"], 1,
            "factory_region_native_transport_factory_count {index}"
        );
        assert_eq!(
            case["providerCalls"], 1,
            "factory_region_native_adapter_factory_count {index}"
        );
        assert_eq!(
            case["forwardedRegions"],
            json!([expected_region]),
            "factory_region_native_adapter_region {index}"
        );
        assert_eq!(
            case["physicalCalls"], 0,
            "factory_region_native_no_dispatch {index}"
        );
    }
    cases
}

pub fn compare_source(native: &[Value]) {
    let source = super::source("create-api-proxy-region-boundaries.cjs", 5);
    assert_eq!(native.len(), 5);
    for index in 0..5 {
        assert_eq!(
            native[index]["region"], source["cases"][index]["region"],
            "factory_region_source_input_comparison {index}"
        );
        for field in ["constructed", "providerCalls"] {
            assert_eq!(
                native[index][field], source["cases"][index][field],
                "factory_region_source_comparison {index}/{field}"
            );
        }
    }
}
