//! The source's hosted-HTTP capability does not substitute for its local SDK key.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::{Harness, Http, Runtime, START, options};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{base_exchange::HostedSigner, client::JsHttpHost, promise::JsFuture, value::Js},
    runtime::{Cleanup, ExchangeRuntime, RemoteSigner},
};
use safe_cex_transport::{
    TransportError, TransportFuture, TransportRegistry,
    signing::{RemoteSignerRequest, RemoteSignerResponse},
};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use safe_cex_venue_lighter::binding::exchange::{LighterDependencies, LighterNativeExchange};
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
struct AdvertisedSigner(Cell<usize>);
impl safe_cex_transport::signing::RemoteSigner for AdvertisedSigner {
    fn sign(&self, _: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.0.set(self.0.get() + 1);
        Box::pin(async { Err(TransportError::Host("unexpected hosted signer call".into())) })
    }
}
impl RemoteSigner for AdvertisedSigner {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
struct AdvertisedHost {
    http: Rc<Http>,
    signer: Rc<AdvertisedSigner>,
}
impl JsHttpHost for AdvertisedHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.http.runtime()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.http.send(config)
    }
    fn hosted_signer(&self) -> Option<HostedSigner> {
        Some((self.signer.clone(), Some("lighter-hosted-proof".into())))
    }
}
fn source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/hosted-signer-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn hosted_http_signer_does_not_replace_lighter_sdk_secret() {
    let rows = source();
    assert_eq!(
        rows.as_array().unwrap().len(),
        6,
        "two names and three source gates"
    );
    for (index, name, wire) in [
        (0, ExchangeName::Lighter, "lighter"),
        (3, ExchangeName::LighterRobinhood, "lighter-robinhood"),
    ] {
        let signer = Rc::new(AdvertisedSigner::default());
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(START);
        let http = Rc::new(Http::default());
        *http.runtime.borrow_mut() = Some(runtime.clone());
        let store = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
        let mut opts = options();
        opts.key = "hosted-key".into();
        opts.secret.clear();
        let exchange = LighterNativeExchange::new(
            name,
            opts,
            store.clone(),
            LighterDependencies {
                runtime: runtime.clone(),
                http: Rc::new(AdvertisedHost {
                    http: http.clone(),
                    signer: signer.clone(),
                }),
                registry: Rc::new(TransportRegistry::default()),
                signing_entropy: None,
            },
        )
        .unwrap();
        assert!(
            exchange.base().signers.remote_signer.is_some(),
            "shared151 capability reached BaseExchange"
        );
        let h = Harness {
            runtime,
            http,
            store,
            exchange,
        };
        let order: PlaceOrderOpts = serde_json::from_value(json!({
            "symbol":"BTC", "type":"limit", "side":"buy", "amount":0.5, "price":50_000
        }))
        .unwrap();
        let error = h
            .drive(Box::pin(h.venue().base().place_order(h.venue(), order)))
            .unwrap_err();
        assert_eq!(
            error.message, "Lighter private key is missing",
            "native local-key gate {wire}"
        );
        assert_eq!(h.http.count(), 0, "refusal precedes SDK HTTP");
        assert_eq!(signer.0.get(), 0, "no invented hosted SDK call");
        assert_eq!(rows[index]["name"], wire);
        assert_eq!(rows[index]["label"], "private-order-gate");
        assert_eq!(rows[index]["error"], error.message);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn source_missing_secret_control_fails_the_named_gate() {
    let output = std::process::Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/hosted-signer-source.cjs"
            ),
            "--drop-secret-guard",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success(), "dropped source guard must fail");
    assert!(String::from_utf8_lossy(&output.stderr).contains("private-order-gate"));
}
