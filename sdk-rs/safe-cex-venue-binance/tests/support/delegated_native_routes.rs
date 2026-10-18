use super::*;
use safe_cex_venue_binance::exchange_classes::Method;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct HistoryHttp {
    requests: Mutex<Vec<safe_cex_transport::RestRequest>>,
}
impl safe_cex_transport::RestAdapter for HistoryHttp {
    fn execute(
        &self,
        request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        let body = if request.url.split('?').next() == Some("/fapi/v1/income") {
            br#"[{"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"-0","asset":"USDT","time":1700000000500,"tranId":31,"tradeId":"41"}]"#.to_vec()
        } else {
            b"[]".to_vec()
        };
        self.requests.lock().unwrap().push(request);
        Box::pin(std::future::ready(Ok(safe_cex_transport::RestResponse {
            status: 200,
            reason: None,
            headers: Default::default(),
            body: safe_cex_transport::RestResponseBody::Encoded(body),
        })))
    }
}

fn history_owner(runtime: Rc<dyn WsRuntime>, http: Arc<HistoryHttp>) -> Rc<ExchangeOwner> {
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime,
        http,
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        dependencies,
    )
    .unwrap()
}

#[test]
fn production_native_historical_positions_keeps_symbol_scoped_fold() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(HistoryHttp::default());
    let owner = history_owner(runtime.clone(), http.clone());
    let mut future = Box::pin(owner.invoke(
        Method::HistoricalPositions,
        vec![record([
            ("account", string("native-history")),
            ("from", number(1_700_000_000_000.0)),
            ("to", number(1_700_000_001_000.0)),
        ])],
    ));
    let mut settled = None;
    for tick in 0..64 {
        // Advance the actual fixture clocks, including the production 3-RPS
        // limiter. No wall-clock race or replacement history/fold callback.
        runtime
            .monotonic_clock
            .set(Some(22_000.0 + f64::from(tick) * 2_000.0));
        runtime.sleep_clock.set(f64::from(tick) * 2_000.0);
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            settled = Some(value.unwrap());
            break;
        }
        runtime.pump();
    }
    let result =
        settled.expect("actual native history settles within independent virtual-clock bound");
    assert_signed_history_fold(result, &http);
    assert!(runtime.reported.borrow().is_empty());
}

fn assert_signed_history_fold(result: EventValue, http: &HistoryHttp) {
    assert!(
        result
            .array_values()
            .expect("fixture array property reads")
            .unwrap()
            .is_empty()
    );
    let requests = http.requests.lock().unwrap();
    assert!(!requests.is_empty(), "real signed income HTTP must execute");
    assert!(
        requests
            .iter()
            .any(|r| r.url.split('?').next() == Some("/fapi/v1/income"))
    );
    let fills = requests
        .iter()
        .filter(|r| r.url.split('?').next() == Some("/fapi/v1/userTrades"))
        .collect::<Vec<_>>();
    assert!(
        !fills.is_empty(),
        "symbol-scoped fold must fetch fills for the income's BTCUSDT symbol"
    );
    assert!(fills.iter().all(|r| {
        r.url
            .split('?')
            .nth(1)
            .unwrap()
            .split('&')
            .any(|pair| pair == "symbol=BTCUSDT")
    }));
    // This is the Axios-style adapter config; the physical native transport
    // converts its lowercase method to uppercase when constructing reqwest.
    assert!(requests.iter().all(|r| r.method == "get"));
    assert!(requests.iter().all(|r| r.url.contains("signature=")));
}

#[test]
fn production_native_historical_positions_settles_on_actual_runtime() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(tokio::task::LocalSet::new().run_until(async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let http = Arc::new(HistoryHttp::default());
        let owner = history_owner(runtime, http.clone());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            owner.invoke(
                Method::HistoricalPositions,
                vec![record([
                    ("account", string("native-history")),
                    ("from", number(1_700_000_000_000.0)),
                    ("to", number(1_700_000_001_000.0)),
                ])],
            ),
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "actual NativeRuntime history deadline: {error}; requests: {:?}",
                http.requests
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|request| &request.url)
                    .collect::<Vec<_>>()
            )
        })
        .unwrap();
        assert_signed_history_fold(result, &http);
        assert!(
            http.requests.lock().unwrap().len() > 3,
            "real history crosses the native 3-RPS limiter"
        );
        owner.dispose().unwrap();
    }));
}

#[test]
fn fixture_sleep_resumes_a_readiness_gated_eager_job() {
    let runtime = Rc::new(Runtime::default());
    let host: Rc<dyn ExchangeRuntime> = runtime.clone();
    let completed = Rc::new(Cell::new(false));
    let output = completed.clone();
    let sleep = host.sleep(1000);
    drop(safe_cex_exchange::eager::start_eager(&host, async move {
        sleep.await?;
        output.set(true);
        Ok(())
    }));
    runtime.sleep_clock.set(999.0);
    runtime.pump();
    assert!(!completed.get(), "fixture sleep respects its deadline");
    runtime.sleep_clock.set(1000.0);
    runtime.pump();
    assert!(
        completed.get(),
        "fixture sleep wakes the eager job at its deadline"
    );
    assert!(runtime.sleep_wakers.borrow().is_empty());
    assert!(runtime.jobs.borrow().is_empty());
}
