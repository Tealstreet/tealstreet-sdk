use safe_cex_exchange::native_store::DefaultStore;
#[path = "support/log_observer.rs"]
mod log_observer;

use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{
    Balance, ExchangeName, Market, Order, Position, StoreDataLoaded, StoreSubscriberError, Ticker,
};
use safe_cex_venue_asterdex::account_fetch::AccountFetch;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

// Independent scenario/count floor, iteration 1; no length derived from oracle.
const NAMES: [&str; 30] = [
    "positions-hedged-first-real",
    "positions-oneway-short-filter",
    "positions-empty-filter",
    "orders-filter-and-stop",
    "orders-empty-filter",
    "markets-select-btc-and-brackets",
    "markets-bracket-failure-default",
    "markets-bracket-null-tail-keeps-partial",
    "markets-null-body-throws-before-brackets",
    "markets-null-sample-throws",
    "orders-null-filter-throws",
    "tickers-bulk-only",
    "tickers-empty-bulk-no-fallback",
    "read-http-failure",
    "sync-hedge-string",
    "sync-invalid-response",
    "sync-axios-error-name-boundary",
    "leverage-round-up-clamp",
    "leverage-negative-half-min-fallback",
    "leverage-negative-half-zero",
    "leverage-in-range-half",
    "leverage-market-missing",
    "mode-change-refresh",
    "mode-change-refresh-fails",
    "mode-change-post-fails",
    "balance-current-applied",
    "balance-stale-after-request",
    "balance-disposed-after-request",
    "balance-fetch-failure-is-reconciled",
    "balance-subscriber-failure-is-reconciled",
];
const REQUESTS: [usize; 30] = [
    1, 1, 1, 1, 1, 2, 2, 2, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 0, 2, 2, 1, 1, 1, 1, 1, 1,
];
const LENGTHS: [Option<usize>; 30] = [
    Some(4),
    Some(1),
    Some(0),
    Some(2),
    Some(0),
    Some(2),
    Some(1),
    Some(1),
    None,
    None,
    None,
    Some(2),
    Some(1),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];
fn run<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = Waker::noop().clone();
    for _ in 0..32 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut Context::from_waker(&waker)) {
            return output;
        }
    }
    panic!("fixture exceeded 32-poll floor");
}
#[derive(Default)]
struct Runtime {
    logs: RefCell<Vec<(String, String)>>,
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("no timer in account reads")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no detached tasks in account reads")
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = log_observer::venue_log_text(severity, message);
        let message = rendered.as_str();
        self.logs
            .borrow_mut()
            .push((message.into(), severity.into()));
    }
    fn unique_id(&self) -> String {
        "account".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("no detached exception")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker requests")
    }
}
struct Shared {
    queue: RefCell<VecDeque<Value>>,
    requests: RefCell<Vec<(&'static str, RestRequest)>>,
    account: RefCell<Weak<AccountFetch>>,
}
struct Adapter {
    kind: &'static str,
    shared: Rc<Shared>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.shared
            .requests
            .borrow_mut()
            .push((self.kind, request.clone()));
        let next = self
            .shared
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unexpected native request");
        let account = self.shared.account.borrow().upgrade().unwrap();
        if let Some(version) = next["versionAfter"].as_f64() {
            account.balance_state_version.set(version);
        }
        if next["disposeAfter"] == true {
            account.owner.dispose().unwrap();
        }
        Box::pin(async move {
            if let Some(error) = next["error"].as_str() {
                return Err(RestFailure::pre_transmit(error.into(), request));
            }
            Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        next["data"].clone(),
                    )
                    .unwrap(),
                ),
            })
        })
    }
}
enum Output {
    Positions(Vec<Position>),
    Orders(Vec<Order>),
    Markets(Vec<Market>),
    Tickers(Vec<Ticker>),
    Applied(bool),
    Unit,
}
fn assert_native_json(
    actual: &safe_cex_types::retained_json::RetainedJsonInput,
    expected: &safe_cex_types::retained_json::RetainedJsonInput,
    name: &str,
) {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    if let Some(number) = actual.as_f64() {
        let expected = if expected.get("number").is_some_and(|v| v.text_equals("-0")) {
            -0.0
        } else {
            expected.as_f64().expect("expected native number")
        };
        assert_eq!(
            number.to_bits(),
            expected.to_bits(),
            "{name} native info number"
        );
        return;
    }
    match (actual, expected) {
        (Input::Array(a), Input::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{name} native info array length");
            for (a, b) in a.iter().zip(b) {
                assert_native_json(a, b, name);
            }
        }
        (Input::Object(a), Input::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{name} native info field count");
            for (key, value) in a {
                let other = b
                    .iter()
                    .find(|(k, _)| k == key)
                    .expect("missing source info field");
                assert_native_json(value, &other.1, name);
            }
        }
        _ => assert_eq!(actual, expected, "{name} native info value"),
    }
}

fn assert_native_models<T>(
    actual: &[T],
    source: &Value,
    name: &str,
    info: impl Fn(
        &mut T,
    ) -> safe_cex_types::ModelField<safe_cex_types::retained_json::RetainedJsonInput>,
) where
    T: Clone + PartialEq + std::fmt::Debug + serde::de::DeserializeOwned,
{
    let expected: Vec<T> = serde_json::from_value(source.clone()).unwrap();
    assert_eq!(actual.len(), expected.len(), "{name} native model count");
    for (actual, expected) in actual.iter().zip(expected) {
        let mut actual = actual.clone();
        let mut expected = expected;
        match (info(&mut actual), info(&mut expected)) {
            (safe_cex_types::ModelField::Value(a), safe_cex_types::ModelField::Value(b)) => {
                assert_native_json(&a, &b, name)
            }
            (a, b) => assert_eq!(a, b, "{name} native info presence"),
        }
        assert_eq!(actual, expected, "{name}");
    }
}
fn assert_native_output(output: &Output, source: &Value, index: usize, name: &str) {
    match output {
        Output::Positions(v) => {
            assert_eq!(
                v.len(),
                LENGTHS[index].unwrap(),
                "{name} native position floor"
            );
            assert_native_models(v, source, &format!("{name} native positions"), |v| {
                v.info.take()
            });
        }
        Output::Orders(v) => {
            assert_eq!(
                v.len(),
                LENGTHS[index].unwrap(),
                "{name} native order floor"
            );
            assert_native_models(v, source, &format!("{name} native orders"), |v| {
                v.info.take()
            });
        }
        Output::Markets(v) => {
            assert_eq!(
                v.len(),
                LENGTHS[index].unwrap(),
                "{name} native market floor"
            );
            assert_native_models(v, source, &format!("{name} native markets"), |v| {
                v.info.take().into()
            });
        }
        Output::Tickers(v) => {
            assert_eq!(
                v.len(),
                LENGTHS[index].unwrap(),
                "{name} native ticker floor"
            );
            assert_native_models(v, source, &format!("{name} native tickers"), |v| {
                v.info.take().into()
            });
        }
        Output::Applied(v) => assert_eq!(*v, source.as_bool().unwrap(), "{name} native applied"),
        Output::Unit => assert!(source.is_null(), "{name} source undefined output"),
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_fetch_matches_source() {
    let child = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-fetch-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&child.stdout).unwrap();
    assert_eq!(cases.len(), 30, "independent scenario floor");
    let mut wire = Vec::new();
    for (index, c) in cases.iter().enumerate() {
        let name = c["name"].as_str().unwrap();
        assert_eq!(name, NAMES[index], "independent ordered cases");
        let runtime = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            serde_json::from_value::<StoreUpdate>(c["initial"].clone()).unwrap(),
            false,
        )
        .unwrap();
        let notifications = Rc::new(Cell::new(0));
        let seen = notifications.clone();
        let _ = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                seen.set(seen.get() + 1);
                Ok(())
            }),
        );
        if let Some(error) = c["subscriberError"].as_str() {
            let error = error.to_owned();
            let _ = Store::subscribe(
                store.as_ref(),
                Box::new(move |_, _| Err(StoreSubscriberError::new(error.clone()))),
            );
        }
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Asterdex,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        ));
        let shared = Rc::new(Shared {
            queue: RefCell::new(c["queue"].as_array().unwrap().iter().cloned().collect()),
            requests: RefCell::new(Vec::new()),
            account: RefCell::new(Weak::new()),
        });
        let account = Rc::new(AccountFetch::new(
            base,
            Rc::new(Adapter {
                kind: "primary",
                shared: shared.clone(),
            }),
            Rc::new(Adapter {
                kind: "public",
                shared: shared.clone(),
            }),
            serde_json::from_value(c["version"].clone()).unwrap(),
        ));
        *shared.account.borrow_mut() = Rc::downgrade(&account);
        account
            .balance_state_version
            .set(c["balanceVersion"].as_f64().unwrap_or(0.0));
        let args = c["args"].as_array().unwrap();
        let symbols = args.first().and_then(Value::as_array).map(|v| {
            v.iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        });
        let output = match c["method"].as_str().unwrap() {
            "fetchPositions" => {
                run(account.fetch_positions(symbols.as_deref())).map(Output::Positions)
            }
            "fetchOrders" => run(account.fetch_orders(symbols.as_deref())).map(Output::Orders),
            "fetchMarkets" => run(account.fetch_markets()).map(Output::Markets),
            "fetchTickers" => run(account.fetch_tickers()).map(Output::Tickers),
            "syncPositionMode" => run(account.sync_position_mode()).map(|_| Output::Unit),
            "setLeverage" => run(account.set_leverage(
                args[0].as_str().unwrap(),
                args[1].as_f64().unwrap(),
                None,
            ))
            .map(|_| Output::Unit),
            "changePositionMode" => run(account.change_position_mode(
                args[0].as_bool().unwrap(),
                args.get(1).and_then(Value::as_str),
            ))
            .map(|_| Output::Unit),
            "reconcileBalanceAfterPrivateUpdate" => {
                run(account.reconcile_balance_after_private_update(args[0].as_f64().unwrap()))
                    .map(Output::Applied)
            }
            other => panic!("unknown method {other}"),
        };
        assert_eq!(
            output.is_ok(),
            c["error"].is_null(),
            "{name} native success"
        );
        match output {
            Ok(output) => assert_native_output(&output, &c["output"], index, name),
            Err(error) => {
                if name == "sync-axios-error-name-boundary" {
                    assert_eq!(
                        c["error"]["name"], "AxiosError",
                        "source AxiosError name floor"
                    );
                    assert_eq!(
                        error.name, "Error",
                        "native RestFailure error-name boundary"
                    );
                    assert!(error.is_axios_error, "native REST failure provenance");
                } else {
                    assert_eq!(error.name, c["error"]["name"], "{name} native error name");
                }
                assert_eq!(
                    error.message, c["error"]["message"],
                    "{name} native error message"
                );
            }
        }
        if name == "leverage-in-range-half" {
            let requests = shared.requests.borrow();
            let Some(RestBody::Untransformed(body)) = &requests[0].1.body else {
                panic!("missing leverage body")
            };
            assert_eq!(
                body.clone().into_value().unwrap()["leverage"].as_f64(),
                Some(3.0),
                "leverage-in-range-half native rounded leverage"
            );
        }
        if name == "leverage-negative-half-zero" {
            let requests = shared.requests.borrow();
            let Some(RestBody::Untransformed(body)) = &requests[0].1.body else {
                panic!("missing leverage body")
            };
            assert_eq!(
                body.clone().into_value().unwrap()["leverage"]
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                (-0.0_f64).to_bits(),
                "leverage-negative-half-zero native leverage bits"
            );
        }
        // Native Store values and logs are checked for all scenarios before any
        // request/wire comparison, not serialized from native models.
        assert_eq!(
            store.options().unwrap().is_hedged,
            c["state"]["isHedged"].as_bool().unwrap(),
            "{name} native hedged"
        );
        assert_eq!(
            store.balance().unwrap(),
            serde_json::from_value::<Balance>(c["state"]["balance"].clone()).unwrap(),
            "{name} native balance"
        );
        assert_eq!(
            store.loaded().unwrap(),
            serde_json::from_value::<StoreDataLoaded>(c["state"]["loaded"].clone()).unwrap(),
            "{name} native loaded"
        );
        assert_native_models(
            &store.positions().unwrap(),
            &c["state"]["positions"],
            &format!("{name} native stored positions"),
            |v| v.info.take(),
        );
        assert_eq!(
            account.balance_state_version.get(),
            c["state"]["version"].as_f64().unwrap(),
            "{name} native balance version"
        );
        assert_eq!(
            account.last_balance_reconciliation_at.get(),
            c["state"]["lastAt"].as_f64().unwrap(),
            "{name} native lastAt"
        );
        assert_eq!(
            notifications.get(),
            c["notifications"].as_u64().unwrap() as usize,
            "{name} native notifications"
        );
        let logs = c["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["message"].as_str().unwrap().to_owned(),
                    v["severity"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(*runtime.logs.borrow(), logs, "{name} native logs");
        assert_eq!(
            shared.queue.borrow().len(),
            0,
            "{name} native unconsumed floor"
        );
        assert_eq!(c["unconsumed"], 0, "{name} source unconsumed floor");
        wire.push((
            name.to_owned(),
            shared.requests.borrow().clone(),
            c["requests"].clone(),
            REQUESTS[index],
        ));
    }
    for (name, requests, source, floor) in wire {
        assert_eq!(requests.len(), floor, "{name} native request floor");
        let source = source.as_array().unwrap();
        assert_eq!(source.len(), floor, "{name} source request floor");
        for ((kind, request), source) in requests.iter().zip(source) {
            assert_eq!(
                *kind,
                source["kind"].as_str().unwrap(),
                "{name} request instance"
            );
            assert_eq!(request.method, source["method"], "{name} request method");
            assert_eq!(request.url, source["url"], "{name} request path");
            assert_eq!(
                request.timeout_ms,
                source["timeout"].as_i64(),
                "{name} request timeout"
            );
            match &request.body {
                Some(RestBody::Untransformed(body)) => assert_native_json(
                    body,
                    &source["body"].clone().into(),
                    &format!("{name} untransformed body"),
                ),
                None => assert!(source.get("body").is_none(), "{name} absent body"),
                _ => panic!("account body must remain untransformed"),
            }
            assert_eq!(
                request.params,
                source.get("params").cloned(),
                "{name} params"
            );
            assert!(
                request.query_params.is_none() && request.query_profile.is_none(),
                "no query profile projection"
            );
        }
    }
}
