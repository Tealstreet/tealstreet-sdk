use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_types::ExchangeName;
use safe_cex_venue_kucoin::{accounts, spot_reads, spot_writes};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Runtime {
    ids: Cell<u64>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected spawn")
    }
    fn unique_id(&self) -> String {
        let id = self.ids.get() + 1;
        self.ids.set(id);
        id.to_string()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn console(&self, _: &str, _: &str) {}
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Adapter {
    case: Value,
    calls: Cell<usize>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.calls.set(self.calls.get() + 1);
            if self.case["transport"] == true {
                let mut error = RestFailure::pre_transmit("transport failure".into(), request);
                error.code = Some("ECONNRESET".into());
                error.context = Some(Box::new(json!({"original":17})));
                return Err(error);
            }
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    self.case["raw"]
                        .as_str()
                        .map(str::as_bytes)
                        .map(Vec::from)
                        .unwrap_or_else(|| br#"{"code":"bad","msg":"venue rejection"}"#.to_vec()),
                ),
            })
        })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected wait"),
    }
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/notification-reads.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        captures.len(),
        cases.len(),
        "notification source case count"
    );
    captures
}
#[test]
fn live_catch_emission_is_fallible_and_keeps_listener_identity() {
    let mut cases = vec![];
    for (venue, methods) in [
        ("kucoin", vec!["getAccount"]),
        (
            "kucoinspot",
            vec![
                "getAccount",
                "validateAccount",
                "fetchMarkets",
                "fetchTickers",
                "cancelAllOrders",
                "cancelSymbolOrders",
                "cancelOrders",
            ],
        ),
    ] {
        for method in methods {
            for fail in ["never", "once", "always"] {
                for transport in [false, true] {
                    cases.push(
                        json!({"venue":venue,"method":method,"fail":fail,"transport":transport,"symbol":"BTC-USDT"}),
                    );
                }
            }
        }
    }
    for case in &mut cases {
        if case["method"] == "cancelOrders" {
            case["orders"] = json!([
                {"id":"cancel-first","symbol":"BTC-USDT","status":"open","type":"limit","side":"buy","price":1,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":3},
                {"id":"cancel-second","symbol":"ETH-USDT","status":"open","type":"limit","side":"buy","price":4,"amount":5,"filled":0,"remaining":5,"reduceOnly":false,"lastUpdate":6}
            ]);
        }
    }
    let expected = source(&cases);
    assert_eq!(expected.len(), 48);
    for (case, expected) in cases.into_iter().zip(expected) {
        let base = BaseExchange::new(
            ExchangeName::Kucoinspot,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            Rc::new(DefaultStore::new()),
            Rc::new(Runtime::default()),
            ExchangeSettings::default(),
        );
        let events = Rc::new(RefCell::new(vec![]));
        let emissions = Rc::new(Cell::new(0));
        let mut sentinel = ExchangeError::new("listener failure");
        sentinel.name = "TypeError".into();
        sentinel.store_subscriber_ids = vec![7, 19];
        sentinel.response = Some(json!({"data":{"msg":"caught listener body"}}));
        let mut catch_sentinel = ExchangeError::new("catch listener failure");
        catch_sentinel.name = "RangeError".into();
        catch_sentinel.store_subscriber_ids = vec![31, 43];
        catch_sentinel.response = Some(json!({"data":{"msg":"second listener body"}}));
        let second_failure = catch_sentinel.clone();
        let observed = events.clone();
        let count = emissions.clone();
        let failure = sentinel.clone();
        let mode = case["fail"].as_str().unwrap().to_owned();
        base.on(
            "error",
            Rc::new(move |_, data| {
                observed.borrow_mut().push(json!([
                    "first",
                    safe_cex_exchange::js_runtime::value::to_json(data)
                        .expect("finite fixture event")
                ]));
                count.set(count.get() + 1);
                if mode == "always" || (mode == "once" && count.get() == 1) {
                    Err(if count.get() == 1 {
                        failure.clone()
                    } else {
                        second_failure.clone()
                    })
                } else {
                    Ok(())
                }
            }),
        );
        let observed = events.clone();
        base.on(
            "error",
            Rc::new(move |_, data| {
                observed.borrow_mut().push(json!([
                    "second",
                    safe_cex_exchange::js_runtime::value::to_json(data)
                        .expect("finite fixture event")
                ]));
                Ok(())
            }),
        );
        let adapter = Adapter {
            case: case.clone(),
            calls: Cell::new(0),
        };
        let mut original_cancel_failure = None;
        let result: Result<(), ExchangeError> = match case["method"].as_str().unwrap() {
            "getAccount" if case["venue"] == "kucoin" => {
                ready(accounts::get_account(&base, &adapter)).map(|_| ())
            }
            "getAccount" => ready(accounts::get_spot_account(&base, &adapter)).map(|_| ()),
            "validateAccount" => {
                ready(accounts::validate_spot_account(&base, &adapter, || async {
                    Err::<safe_cex_types::Balance, ()>(())
                }))
                .map(|_| ())
            }
            "fetchMarkets" => ready(spot_reads::fetch_markets(&base, &adapter)).map(|_| ()),
            "fetchTickers" => ready(spot_reads::fetch_tickers(&base, &adapter)).map(|_| ()),
            "cancelAllOrders" | "cancelSymbolOrders" | "cancelOrders" => {
                match if case["method"] == "cancelSymbolOrders" {
                    ready(spot_writes::cancel_symbol_orders(
                        &base, &adapter, "BTC-USDT",
                    ))
                } else if case["method"] == "cancelOrders" {
                    let orders = serde_json::from_value::<Vec<safe_cex_types::Order>>(
                        case["orders"].clone(),
                    )
                    .unwrap();
                    ready(spot_writes::cancel_orders(&base, &adapter, &orders))
                } else {
                    ready(spot_writes::cancel_all_orders(&base, &adapter))
                } {
                    Ok(()) => Ok(()),
                    Err(error) => match error.downcast::<ExchangeError>() {
                        Ok(error) => Err(*error),
                        Err(error) => {
                            let error = *error
                                .downcast::<RestFailure>()
                                .expect("cancel retains concrete transport failure");
                            let observed = ExchangeError::new(error.message.clone());
                            original_cancel_failure = Some(error);
                            Err(observed)
                        }
                    },
                }
            }
            _ => panic!("unknown method"),
        };
        if case["method"] == "cancelOrders" {
            assert_eq!(
                adapter.calls.get(),
                1,
                "KUCOIN-SEQUENTIAL reporting stops the loop"
            );
        }
        assert_eq!(
            adapter.calls.get(),
            expected["calls"].as_u64().unwrap() as usize,
            "listener rejection stops later requests: {case}"
        );
        assert_eq!(
            result.is_ok(),
            expected["resolved"].as_bool().unwrap(),
            "listener rejection escapes instead of fallback: {case}"
        );
        if matches!(
            case["method"].as_str(),
            Some("cancelAllOrders" | "cancelSymbolOrders" | "cancelOrders")
        ) && case["transport"] == true
        {
            if case["fail"] == "never" {
                assert_eq!(
                    expected["rejected"]["sameOriginal"], true,
                    "source cancel successful emission preserves original transport failure"
                );
                let original = original_cancel_failure
                    .as_ref()
                    .expect("cancel successful emission preserves original transport failure");
                assert_eq!(original.code.as_deref(), Some("ECONNRESET"));
                assert_eq!(original.context.as_deref(), Some(&json!({"original":17})));
                assert_eq!(original.request.method, "DELETE");
            } else {
                assert_eq!(
                    expected["rejected"]["sameOriginal"], false,
                    "source cancel failed emission replaces original transport failure"
                );
                assert_eq!(
                    expected["rejected"]["same"], true,
                    "source cancel failed emission returns listener object"
                );
                assert!(
                    original_cancel_failure.is_none(),
                    "cancel failed emission replaces original transport failure"
                );
            }
        }
        if expected["rejected"]["same"] == true {
            assert_eq!(
                result.unwrap_err(),
                if expected["rejected"]["which"] == 1 {
                    sentinel
                } else {
                    catch_sentinel
                },
                "original listener error metadata retained: {case}"
            );
        }
        assert_eq!(
            json!(*events.borrow()),
            expected["events"],
            "fallible emission source ordering: {case}"
        );
    }
}
