//! Real private-frame witnesses for the delegated socket audit.
use super::{
    BybitDependencies,
    value::{Js, arg},
};
use crate::exchange::BybitExchange;
use safe_cex_exchange::{
    exchange::Exchange,
    native::{NativeHttpHost, native_runtime},
    native_store::DefaultStore,
    socket_lifecycle::SocketSlot,
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, TransportFuture,
    rate_limit::RateLimitClock, socket::SocketFrame,
};
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct NoWire;
impl RestAdapter for NoWire {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("private filter witness must not perform HTTP")
    }
}
struct Clock;
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        0
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}
fn venue(name: ExchangeName) -> Rc<BybitExchange> {
    venue_with_signer(name, None, VirtualClock::default())
}
fn venue_with_signer(
    name: ExchangeName,
    signer: Option<safe_cex_exchange::js_runtime::base_exchange::HostedSigner>,
    clock: VirtualClock,
) -> Rc<BybitExchange> {
    BybitExchange::create(
        name,
        serde_json::from_value(serde_json::json!({"key":"", "secret":"", "publicDataOnly":true}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        BybitDependencies {
            runtime: native_runtime(),
            remote_signer: signer,
            wire: Arc::new(NoWire),
            http_host: Rc::new(NativeHttpHost::default()),
            retry_clock: Rc::new(Clock),
            clock,
            external_public_data: None,
            relay_enabled: false,
        },
    )
    .unwrap()
}
fn row(symbol: &str) -> Js {
    Js::object(vec![("symbol", Js::str(symbol))])
}

#[test]
fn private_filter_reads_live_indices_through_real_frame_binding() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(tokio::task::LocalSet::new().run_until(async {
            for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
                for profile in [
                    "replace",
                    "delete",
                    "shrink",
                    "append",
                    "sparse",
                    "ordinary",
                    "selector-false",
                ] {
                    let exchange = venue(name.clone());
                    let rt = exchange.js.get().unwrap();
                    let first = row("first");
                    let second = row("second");
                    let replacement = row("replacement");
                    let rows = if profile == "sparse" {
                        Js::holey_array(vec![Some(first.clone()), None, Some(second.clone())])
                    } else {
                        Js::array(vec![first.clone(), second.clone()])
                    };
                    let parse_rows = rows.clone();
                    rt.shared
                        .globals
                        .get("JSON")
                        .unwrap()
                        .define(
                            "parse",
                            Js::function("parse", move |_, _| {
                                Ok(Js::object(vec![
                                    ("topic", Js::str("order")),
                                    ("data", parse_rows.clone()),
                                ]))
                            }),
                        )
                        .unwrap();
                    let calls = Rc::new(RefCell::new(vec![]));
                    let observed = calls.clone();
                    let live = rows.clone();
                    let new_row = replacement.clone();
                    rt.this
                        .define(
                            "tradesCategoryOf",
                            Js::function("tradesCategoryOf", move |_, args| {
                                let symbol = arg(&args, 0);
                                observed.borrow_mut().push(symbol.to_display());
                                if symbol.strict_equals(&Js::str("first")) {
                                    match profile {
                                        "replace" => live.set("1", new_row.clone())?,
                                        "delete" => live.delete(&[49])?,
                                        "shrink" => live.set("length", Js::Number(0.))?,
                                        "append" => {
                                            live.call_method("push", vec![new_row.clone()])?;
                                        }
                                        _ => {}
                                    }
                                }
                                Ok(Js::Bool(
                                    profile != "selector-false"
                                        || symbol.strict_equals(&Js::str("second")),
                                ))
                            }),
                        )
                        .unwrap();
                    let mapped = Rc::new(RefCell::new(vec![]));
                    let seen = mapped.clone();
                    rt.this
                        .define(
                            "mapOrder",
                            Js::function("mapOrder", move |_, args| {
                                seen.borrow_mut().push(arg(&args, 0));
                                Ok(Js::array(vec![]))
                            }),
                        )
                        .unwrap();
                    exchange
                        .base()
                        .get_socket(SocketSlot::Private)
                        .unwrap()
                        .on_message(&SocketFrame::Text("{}".into()))
                        .unwrap();
                    let (expected_symbols, expected_rows) = match profile {
                        "selector-false" => (vec!["first", "second"], vec![second]),
                        "replace" => (
                            vec!["first", "replacement"],
                            vec![first.clone(), replacement],
                        ),
                        "delete" | "shrink" => (vec!["first"], vec![first.clone()]),
                        _ => (vec!["first", "second"], vec![first.clone(), second]),
                    };
                    assert_eq!(
                        *calls.borrow(),
                        expected_symbols,
                        "live filter callback order: {name:?}/{profile}"
                    );
                    assert_eq!(
                        mapped.borrow().len(),
                        expected_rows.len(),
                        "mapped row count: {profile}"
                    );
                    for (actual, expected) in mapped.borrow().iter().zip(expected_rows) {
                        assert!(
                            actual.strict_equals(&expected),
                            "original accepted row identity: {profile}"
                        );
                    }
                    exchange.base().dispose().unwrap();
                }
            }
        }));
}

#[test]
#[ignore = "executes the maintained TypeScript member using the root compiler dependency"]
fn private_filter_maintained_source_live_indices() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-filter-fixtures.cjs"
        ))
        .output()
        .expect("Node private-filter oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let profiles: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(profiles.as_array().unwrap().len(), 7);
}

#[test]
fn inverse_auth_production_clock_keeps_finite_expiry_at_real_signer() {
    use safe_cex_transport::signing::{RemoteSigner, RemoteSignerRequest, RemoteSignerResponse};
    use std::time::Duration;
    struct Signer(Rc<RefCell<Option<RemoteSignerRequest>>>);
    impl safe_cex_exchange::runtime::RemoteSigner for Signer {
        fn is_available(&self) -> bool {
            true
        }
        fn on_availability_change(
            &self,
            _: Rc<dyn Fn(bool)>,
        ) -> safe_cex_exchange::runtime::Cleanup {
            Box::new(|| {})
        }
    }
    impl RemoteSigner for Signer {
        fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
            assert!(
                self.0.borrow_mut().replace(request).is_none(),
                "one original signer invocation"
            );
            Box::pin(std::future::pending())
        }
    }
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(tokio::task::LocalSet::new().run_until(async {
        let seen = Rc::new(RefCell::new(None));
        let clock = VirtualClock::default();
        let before = clock.get_current_time_ms();
        assert!(before.is_finite(), "production native wall clock is finite");
        // The native reconnect policy clears even a previously installed offset.
        clock.sync_from_server_time(before + 300_000.);
        let exchange = venue_with_signer(ExchangeName::Bybitinverse, Some((Rc::new(Signer(seen.clone())), Some("inverse-account".into()))), clock.clone());
        let socket = exchange.base().get_socket(SocketSlot::Private).unwrap();
        socket.on_open().await.unwrap();
        assert!(seen.borrow().is_none(), "the source25ms wait precedes auth");
        tokio::time::timeout(Duration::from_secs(2), async {
            while seen.borrow().is_none() { tokio::time::sleep(Duration::from_millis(1)).await; }
        }).await.expect("real inverse protocol reaches its hosted signer");
        let after = VirtualClock::default().get_current_time_ms();
        let request = seen.borrow_mut().take().unwrap();
        let RemoteSignerRequest::WsAuth { exchange_name, account_name, request } = request else { panic!("original WS signer request kind") };
        assert_eq!(exchange_name, ExchangeName::Bybit);
        assert_eq!(account_name.as_deref(), Some("inverse-account"));
        let auth = request.ws_auth.unwrap();
        let extra: serde_json::Value = serde_json::from_str(auth.extra_data.unwrap().get()).unwrap();
        let expires = extra["expires"].as_f64().expect("finite production expiry is a number before the signer waiter settles");
        assert!(expires.is_finite());
        assert!(expires >= before + 10_000. && expires <= after + 10_000., "native clock sync must clear stale offsets before sampling auth: {expires}/{before}/{after}");
        assert_eq!(auth.to_sign.unwrap(), format!("GET/realtime{}", safe_cex_num::js_number_string(expires)));
        exchange.base().dispose().unwrap();
    }));
}

#[test]
#[ignore = "executes maintained TypeScript clock and inverse auth with the production Node branch"]
fn inverse_auth_maintained_source_clock_is_finite() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-auth-clock-fixtures.cjs"
        ))
        .output()
        .expect("Node inverse auth clock oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let profiles: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(profiles.as_array().unwrap().len(), 3);
}
