// Only terminal HTTP/socket I/O is gated. Every operation and Store continuation
// runs through the actual constructed Apex binding and installed API handlers.
#[derive(Default)]
struct StartResponseGate {
    released: std::cell::Cell<bool>,
    waker: std::cell::RefCell<Option<std::task::Waker>>,
}
impl StartResponseGate {
    fn release(&self) {
        self.released.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
}
struct StartContinuationHttp {
    inner: std::rc::Rc<support::Http>,
    first_account: std::rc::Rc<StartResponseGate>,
    account_requests: std::cell::Cell<usize>,
}
impl safe_cex_exchange::js_runtime::client::JsHttpHost for StartContinuationHttp {
    fn runtime(&self) -> std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> {
        safe_cex_exchange::js_runtime::client::JsHttpHost::runtime(self.inner.as_ref())
    }
    fn send(
        &self,
        config: safe_cex_exchange::js_runtime::value::Js,
    ) -> safe_cex_exchange::js_runtime::promise::JsFuture<'static> {
        let path = config.get("url").unwrap().to_display();
        let gate = if path == "/v3/account" {
            let index = self.account_requests.get();
            self.account_requests.set(index + 1);
            Some(if index == 0 {
                self.first_account.clone()
            } else {
                std::rc::Rc::default()
            })
        } else if path == "/v3/open-orders" {
            Some(std::rc::Rc::default())
        } else {
            None
        };
        let response =
            safe_cex_exchange::js_runtime::client::JsHttpHost::send(self.inner.as_ref(), config);
        Box::pin(async move {
            if let Some(gate) = gate {
                std::future::poll_fn(move |cx| {
                    if gate.released.get() {
                        std::task::Poll::Ready(())
                    } else {
                        *gate.waker.borrow_mut() = Some(cx.waker().clone());
                        std::task::Poll::Pending
                    }
                })
                .await;
            }
            response.await
        })
    }
}

fn startup_continuation_case(disposed: bool) {
    let runtime = std::rc::Rc::new(support::Runtime::default());
    runtime.now.set(1_700_000_000_000.0);
    // Neither socket opens. This prevents private onOpen resync from replacing
    // the start request under test; it does not suppress the real tick call.
    runtime.hold_dials.set(true);
    let http = std::rc::Rc::new(support::Http::default());
    *http.runtime.borrow_mut() = Some(runtime.clone());
    http.ok(
        "GET",
        "/v3/symbols",
        json!({"contractConfig":{"perpetualContract":[contract()]}}),
    );
    http.ok("GET","/v3/account",json!({"id":"16909060","l2key":"fixture-l2","positions":[{"symbol":"BTC-USDC","side":"LONG","size":"4","entryPrice":"123","customImr":"0.1"}],"contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}}));
    http.route(
        "GET",
        "/v3/account-balance",
        200.0,
        json!({"code":10001,"msg":"startup balance rejected"}),
    );
    http.ok("GET", "/v3/open-orders", json!([]));
    let gate = std::rc::Rc::new(StartResponseGate::default());
    let terminal = std::rc::Rc::new(StartContinuationHttp {
        inner: http.clone(),
        first_account: gate.clone(),
        account_requests: std::cell::Cell::new(0),
    });
    let store = std::rc::Rc::new(safe_cex_store::store::DefaultStore::new());
    let mut opts = options();
    opts.passphrase = Some("fixture-passphrase".into());
    opts.omni_key_seed = Some(format!("0x{}1b", "01".repeat(64)));
    opts.extra = Some(serde_json::from_value(json!({"tickInterval":60_000})).unwrap());
    let exchange = safe_cex_venue_apexomni::ApexOmniExchange::new(
        ExchangeName::Apexomni,
        opts,
        store.clone(),
        safe_cex_venue_apexomni::ApexDependencies {
            runtime: runtime.clone(),
            http: terminal.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    let h = Harness {
        runtime,
        http,
        store,
        exchange,
    };
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert_eq!(
        terminal.account_requests.get(),
        2,
        "initial and real tick account requests must both start"
    );
    assert_eq!(
        h.http.calls("/v3/open-orders").len(),
        1,
        "real tick must start its own snapshot"
    );
    assert_eq!(h.http.calls("/v3/account-balance").len(), 1);
    assert!(h.store.positions().is_empty());
    assert!(!h.store.loaded().positions && !h.store.loaded().orders && !h.store.loaded().balance);
    if disposed {
        h.venue().dispose().unwrap();
    }
    gate.release();
    h.runtime.pump();
    assert_eq!(
        h.store
            .positions()
            .iter()
            .find(|p| p.side == safe_cex_types::PositionSide::Long)
            .map(|p| p.contracts),
        if disposed { None } else { Some(4.0) },
        "already-started positions.then must update Store after startup balance rejects"
    );
    assert_eq!(h.store.loaded().positions, !disposed);
    assert_eq!(h.store.loaded().orders, !disposed);
    assert!(
        !h.store.loaded().balance,
        "failed balance branch must stay unloaded"
    );
    assert_eq!(
        terminal.account_requests.get(),
        2,
        "late update must come from initial request, not a new poll"
    );
    done(&h);
}

#[test]
fn startup_positions_continuation_survives_balance_rejection() {
    startup_continuation_case(false);
}
#[test]
fn disposed_startup_continuation_does_not_write_positions() {
    startup_continuation_case(true);
}
#[test]
fn maintained_startup_continuation_capture() {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/start-continuation.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows,
        json!([
            {"disposed":false,"contracts":4,"positions":true,"orders":true,"balance":false,"accounts":2},
            {"disposed":true,"contracts":null,"positions":false,"orders":false,"balance":false,"accounts":2}
        ])
    );
}
