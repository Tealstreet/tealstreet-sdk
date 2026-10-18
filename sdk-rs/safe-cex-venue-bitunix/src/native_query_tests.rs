//! Constructed production client signing, with actual source API interceptor expectations.
use crate::native_query_fixture as fixture;
use safe_cex_num::JsJsonValue;
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(runtime: &fixture::Runtime, future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.now.set(runtime.now.get() + 1000.0);
        runtime.pump();
    }
    panic!("production query read remained pending");
}

#[test]
fn native_query_signatures_match_source_locale_sort_at_every_level() {
    let (venue, runtime, _, wire) = fixture::fixture_with(false);
    *runtime.id.borrow_mut() = Some("fixture-_nonce".into());
    let inputs = [
        json!({"Zeta":11,"alpha":13,"_b":17}),
        json!({"ids":[0,1,2,3,4,5,6,7,8,9,10,11],"nested":{"Zeta":"U","alpha":"L","_b":"P"}}),
    ];
    let mut samples = Vec::new();
    for params in inputs {
        let config = JsJsonValue::from(json!({"params":params.clone()}));
        drive(&runtime, venue.xhr().get("/api/v1/futures/account", config)).unwrap();
        let requests = wire.requests.lock().unwrap();
        let request = requests.last().unwrap();
        let timestamp = request.headers["timestamp"].parse::<f64>().unwrap();
        samples.push(json!({"params":params,"nonce":request.headers["nonce"],
            "timeBits":format!("{:016x}",timestamp.to_bits()),"actual":request.headers["sign"]}));
    }
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        2,
        "two actual signed client reads"
    );
    let source = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/api-oracle.cjs"))
        .arg("--native-query")
        .arg(serde_json::to_string(&samples).unwrap())
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(expected.len(), 2, "actual source interceptor corpus");
    for (sample, source) in samples.iter().zip(expected) {
        assert_eq!(
            sample["actual"], source["sign"],
            "production native query signature uses source locale ordering: {}",
            sample["params"]
        );
    }
}

fn eager_fixture() -> (
    std::rc::Rc<crate::adapter::BitunixVenue>,
    std::rc::Rc<fixture::Runtime>,
    std::rc::Rc<safe_cex_store::store::DefaultStore>,
    std::sync::Arc<fixture::Wire>,
) {
    let (venue, runtime, store, wire) = fixture::fixture_with(false);
    for (path, data) in [
        (
            "/api/v1/futures/account",
            json!({"marginCoin":"USDT","available":"20","equity":"25","positionMode":"HEDGE"}),
        ),
        ("/api/v1/futures/market/tickers", json!([])),
        (
            "/api/v1/futures/trade/get_pending_orders",
            json!({"orderList":[]}),
        ),
        ("/api/v1/futures/tpsl/get_pending_orders", json!([])),
        (
            "/api/v1/futures/trade/get_history_trades",
            json!({"tradeList":[],"total":0}),
        ),
        ("/api/v1/futures/market/kline", json!([])),
        (
            "/api/v1/futures/account/get_leverage_margin_mode",
            json!({"symbol":"BTCUSDT","leverage":23,"marginMode":"ISOLATION"}),
        ),
        ("/api/v1/futures/account/change_leverage", json!({})),
    ] {
        wire.bodies
            .lock()
            .unwrap()
            .insert(path.into(), json!({"code":0,"data":data}));
    }
    (venue, runtime, store, wire)
}

#[test]
fn discarded_native_reads_start_every_assigned_request() {
    use safe_cex_exchange::exchange::Exchange;
    for case in 0..6 {
        let (venue, runtime, _, wire) = eager_fixture();
        let expected: &[&str] = match case {
            0 => {
                drop(venue.validate_account());
                &["/api/v1/futures/account"]
            }
            1 => {
                drop(venue.fetch_balance());
                &["/api/v1/futures/account"]
            }
            2 => {
                drop(Exchange::fetch_orders(venue.as_ref(), None));
                &[
                    "/api/v1/futures/trade/get_pending_orders",
                    "/api/v1/futures/tpsl/get_pending_orders",
                ]
            }
            3 => {
                drop(venue.fetch_historical_fills(
                    serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap(),
                ));
                &["/api/v1/futures/trade/get_history_trades"]
            }
            4 => {
                drop(venue.fetch_ohlcv(serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m","limit":1,"from":1700000000000u64,"to":1700000060000u64})).unwrap()));
                &["/api/v1/futures/market/kline"]
            }
            _ => {
                drop(venue.fetch_position_mode());
                &["/api/v1/futures/account"]
            }
        };
        runtime.pump();
        let requests = wire.requests.lock().unwrap();
        for endpoint in expected {
            assert_eq!(
                requests
                    .iter()
                    .filter(|request| request.url == *endpoint)
                    .count(),
                if case == 0 { 2 } else { 1 },
                "discarded native read {case} must finish source requests at {endpoint}"
            );
        }
        drop(requests);
        venue.dispose().unwrap();
    }
}

#[test]
fn discarded_native_writes_resume_store_updates_after_held_http() {
    use safe_cex_exchange::exchange::Exchange;
    use std::sync::Arc;
    for metadata in [false, true] {
        let (venue, runtime, store, wire) = eager_fixture();
        let endpoint = if metadata {
            "/api/v1/futures/account/get_leverage_margin_mode"
        } else {
            "/api/v1/futures/account/change_leverage"
        };
        let gate = Arc::new(fixture::HeldResponse::default());
        wire.held_responses
            .lock()
            .unwrap()
            .insert(endpoint.into(), gate.clone());
        let pending = if metadata {
            venue.update_symbol_metadata("BTCUSDT".into())
        } else {
            venue.set_leverage("BTCUSDT".into(), 23.)
        };
        drop(pending);
        runtime.pump();
        let requests = wire.requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "held native operation starts before waiter polling"
        );
        assert_eq!(requests[0].url, endpoint);
        assert_eq!(
            requests[0].headers["sign"].len(),
            64,
            "production signed request"
        );
        drop(requests);
        let positions = store.positions();
        assert_eq!(positions.len(), 2);
        for position in positions.iter() {
            assert_eq!(position.leverage, 10., "Store waits for actual response");
        }
        drop(positions);
        gate.release();
        runtime.pump();
        let positions = store.positions();
        assert_eq!(positions.len(), 2);
        for (position, id) in positions.iter().zip(["BTCUSDT:long", "BTCUSDT:short"]) {
            assert_eq!(position.id, id);
            assert_eq!(
                position.leverage,
                23.,
                "discarded waiter retains Store continuation: metadata={metadata}; errors={:?}; console={:?}",
                runtime.errors.borrow(),
                runtime.console.borrow()
            );
            if metadata {
                assert_eq!(
                    position.margin_type.into_value(),
                    Some(safe_cex_types::MarginType::Isolated)
                );
            }
        }
        drop(positions);
        venue.dispose().unwrap();
    }
}

#[test]
fn discarded_native_amendments_keep_normal_and_standalone_routes() {
    use safe_cex_exchange::exchange::Exchange;
    use safe_cex_types::{Order, UpdateOrderOpts};
    for standalone in [false, true] {
        let (venue, runtime, _, wire) = eager_fixture();
        let order: Order = serde_json::from_value(json!({
            "id":"amend-7","status":"open","symbol":"BTCUSDT",
            "type":if standalone {"take_profit_market"} else {"limit"},
            "side":"buy","price":120.5,"amount":2.5,"filled":0,"remaining":2.5,
            "reduceOnly":false,"lastUpdate":17,
            "info": if standalone { json!({"orderId":"raw-7","tpPrice":"120.5","tpQty":"2.5","tpStopType":"MARK_PRICE","tpOrderType":"MARKET"}) } else {json!({"orderId":"amend-7","orderType":"LIMIT","reduceOnly":false,"qty":"2.5","price":"120.5"})}
        })).unwrap();
        let endpoint = if standalone {
            "/api/v1/futures/tpsl/modify_order"
        } else {
            "/api/v1/futures/trade/modify_order"
        };
        wire.bodies
            .lock()
            .unwrap()
            .insert(endpoint.into(), json!({"code":0,"data":{}}));
        drop(venue.update_order(UpdateOrderOpts {
            order,
            update: serde_json::from_value(json!({"price":110.25})).unwrap(),
        }));
        runtime.pump();
        let requests = wire.requests.lock().unwrap();
        assert_eq!(
            requests.len(),
            1,
            "discarded amendment starts its native route: standalone={standalone}; errors={:?}; console={:?}",
            runtime.errors.borrow(),
            runtime.console.borrow()
        );
        assert_eq!(requests[0].url, endpoint);
        drop(requests);
        venue.dispose().unwrap();
    }
}

#[test]
fn ready_native_http_preserves_source_await_before_store_write() {
    use safe_cex_exchange::exchange::Exchange;
    for metadata in [false, true] {
        let (venue, runtime, store, _) = eager_fixture();
        let pending = if metadata {
            venue.update_symbol_metadata("BTCUSDT".into())
        } else {
            venue.set_leverage("BTCUSDT".into(), 23.)
        };
        assert_eq!(store.positions().len(), 2);
        for position in store.positions().iter() {
            assert_eq!(
                position.leverage, 10.,
                "ready HTTP must retain source await fence; metadata={metadata}"
            );
        }
        drop(pending);
        runtime.pump();
        for position in store.positions().iter() {
            assert_eq!(
                position.leverage, 23.,
                "ready HTTP continuation remains owned"
            );
        }
        venue.dispose().unwrap();
    }
}

#[test]
fn private_open_runs_login_clock_prefix_before_queuing_its_waiter() {
    use safe_cex_exchange::exchange::Exchange;
    use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    let (venue, runtime, _, _) = eager_fixture();
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    assert!(VIRTUAL_CLOCK.get_current_time_ms() - (VIRTUAL_CLOCK.now)() > 30_000.0);
    drive(&runtime, venue.socket("private").base.on_open()).unwrap();
    let offset = VIRTUAL_CLOCK.get_current_time_ms() - (VIRTUAL_CLOCK.now)();
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    venue.dispose().unwrap();
    assert!(
        offset.abs() < 1000.,
        "private onOpen must begin source sendLogin sync before queuing its waiter: {offset}"
    );
}
