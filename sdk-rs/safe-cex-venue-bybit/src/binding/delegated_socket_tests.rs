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
    BybitExchange::create(
        name,
        serde_json::from_value(serde_json::json!({"key":"", "secret":"", "publicDataOnly":true}))
            .unwrap(),
        Rc::new(DefaultStore::new()),
        BybitDependencies {
            runtime: native_runtime(),
            remote_signer: None,
            wire: Arc::new(NoWire),
            http_host: Rc::new(NativeHttpHost::default()),
            rate_limit_clock: Arc::new(Clock),
            retry_clock: Rc::new(Clock),
            clock: VirtualClock::default(),
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
