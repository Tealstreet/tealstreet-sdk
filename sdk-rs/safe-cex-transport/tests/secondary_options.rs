use safe_cex_transport::provider::{PrivateProxy, WsConnection};
use safe_cex_transport::routing::{ResolvedRouting, RoutingOptions};
use safe_cex_transport::secondary::create_api_with_secondary_key;
use safe_cex_transport::{
    DirectTransport, RestAdapter, RestAdapterContext, RestFailure, RestFuture, RestRequest,
    RestResponse, RestTransport, RestTransportContext, TransportError, TransportProvider,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc, sync::Arc};
struct Provider {
    proxy: Option<String>,
    events: Rc<RefCell<Vec<String>>>,
}
impl TransportProvider for Provider {
    fn name(&self) -> &str {
        "oracle"
    }
    fn rest(&self, ctx: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
        DirectTransport.rest(ctx)
    }
    fn rest_adapter(&self, ctx: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
        DirectTransport.rest_adapter(ctx)
    }
    fn proxy_prefix(&self, ctx: RestTransportContext<'_>) -> String {
        DirectTransport.proxy_prefix(ctx)
    }
    fn public_cors_proxy(&self) -> Option<String> {
        self.events.borrow_mut().push("proxy".into());
        self.proxy.clone()
    }
    fn private_proxy(&self, name: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
        DirectTransport.private_proxy(name)
    }
    fn require_proxy(&self, opts: ExchangeOptions) -> ExchangeOptions {
        DirectTransport.require_proxy(opts)
    }
    fn server_proxy_url(&self, name: &ExchangeName) -> Option<String> {
        DirectTransport.server_proxy_url(name)
    }
    fn ws_routing(&self, name: &ExchangeName, opts: &RoutingOptions) -> ResolvedRouting {
        DirectTransport.ws_routing(name, opts)
    }
    fn ws_connection(&self, url: &str) -> Option<WsConnection> {
        DirectTransport.ws_connection(url)
    }
}
struct Client;
impl RestAdapter for Client {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            Err(RestFailure::pre_transmit(
                "oracle constructs clients but must never execute I/O".into(),
                request,
            ))
        })
    }
}
fn assert_native_options(options: &ExchangeOptions, expected: &Value) {
    assert_eq!(
        options.key,
        expected["key"].as_str().unwrap_or_default(),
        "native primary key identity"
    );
    assert_eq!(
        options.secret,
        expected["secret"].as_str().unwrap_or_default(),
        "native primary secret identity"
    );
    for (field, actual) in [
        ("passphrase", options.passphrase.as_deref()),
        ("secondary_key", options.secondary_key.as_deref()),
        ("secondary_secret", options.secondary_secret.as_deref()),
        (
            "secondary_passphrase",
            options.secondary_passphrase.as_deref(),
        ),
        ("secondaryProxyUrl", options.secondary_proxy_url.as_deref()),
        ("overrideProxyUrl", options.override_proxy_url.as_deref()),
    ] {
        assert_eq!(
            actual,
            expected[field].as_str(),
            "native secondary option identity: {field}"
        );
    }
    assert_eq!(
        options.testnet,
        expected["testnet"].as_bool(),
        "native testnet identity"
    );
}

#[test]
fn secondary_options_keep_native_signer_type_bits() {
    for bits in [
        0x7ff8_0000_0000_0042,
        (-0.0_f64).to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        1.5_f64.to_bits(),
    ] {
        let mut options: ExchangeOptions = serde_json::from_value(serde_json::json!({
            "key":"primary", "secret":"primary-secret", "passphrase":"primary-pass",
            "secondary_key":"secondary", "secondary_secret":"", "secondary_passphrase":"",
            "testnet":true, "overrideProxyUrl":"old://proxy"
        }))
        .unwrap();
        options.polymarket = Some(safe_cex_types::ExchangeOptionsPolymarket {
            api_key: Some("native-credential".into()),
            signer_type: Some(f64::from_bits(bits)),
            ..Default::default()
        });
        let provider = Provider {
            proxy: None,
            events: Rc::new(RefCell::new(Vec::new())),
        };
        let created = RefCell::new(Vec::new());
        let clients = create_api_with_secondary_key(
            &options,
            &provider,
            Some("selected://proxy"),
            |row| {
                created.borrow_mut().push(row.clone());
                Ok(Arc::new(Client))
            },
            None,
            None,
        )
        .expect("source shallow copy admits native signerType");
        assert!(!clients.shares_primary_adapter());
        let rows = created.borrow();
        assert_eq!(rows.len(), 2);
        for row in rows.iter() {
            let polymarket = row.polymarket.as_ref().unwrap();
            assert_eq!(
                polymarket.signer_type.unwrap().to_bits(),
                bits,
                "native signerType bits"
            );
            assert_eq!(polymarket.api_key.as_deref(), Some("native-credential"));
            assert_eq!(row.testnet, Some(true));
        }
        assert_eq!(rows[0].key, "primary");
        assert_eq!(rows[1].key, "secondary");
        assert_eq!(rows[1].secret, "");
        assert_eq!(rows[1].passphrase.as_deref(), Some(""));
        assert_eq!(
            rows[1].override_proxy_url.as_deref(),
            Some("selected://proxy")
        );
        assert_eq!(options.key, "primary");
        assert_eq!(options.secret, "primary-secret");
        assert_eq!(options.override_proxy_url.as_deref(), Some("old://proxy"));
    }
}

#[test]
#[ignore = "requires isolated TypeScript compiler; run with --include-ignored"]
fn maintained_typescript_secondary_options_empty_strings_and_proxy_precedence() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/secondary-options-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the maintained source oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 729);
    for (index, case) in cases.iter().enumerate() {
        let opts: ExchangeOptions = serde_json::from_value(case["options"].clone()).unwrap();
        assert_native_options(&opts, &case["options"]);
        let events = Rc::new(RefCell::new(Vec::new()));
        let provider = Provider {
            proxy: case["publicProxy"].as_str().map(str::to_owned),
            events: events.clone(),
        };
        let created = RefCell::new(Vec::new());
        let primary_limit = |client: Arc<dyn RestAdapter>| {
            events.borrow_mut().push("primary-limit".into());
            client
        };
        let secondary_limit = |client: Arc<dyn RestAdapter>| {
            events.borrow_mut().push("secondary-limit".into());
            client
        };
        let clients = create_api_with_secondary_key(
            &opts,
            &provider,
            case["proxyUrl"].as_str(),
            |options| {
                events.borrow_mut().push("create".into());
                let expected = if created.borrow().is_empty() {
                    &case["options"]
                } else {
                    &case["derived"]
                };
                assert_native_options(options, expected);
                created
                    .borrow_mut()
                    .push(serde_json::to_value(options).unwrap());
                Ok(Arc::new(Client))
            },
            Some(&primary_limit),
            Some(&secondary_limit),
        )
        .unwrap();
        assert_eq!(
            clients.shares_primary_adapter(),
            case["shares"].as_bool().unwrap()
        );
        assert_eq!(
            serde_json::to_value(events.borrow().clone()).unwrap(),
            case["events"],
            "constructor/limiter order: {index}"
        );
        assert_eq!(created.borrow()[0], case["options"]);
        if !case["derived"].is_null() {
            assert_eq!(created.borrow()[1], case["derived"]);
        }
    }
}
