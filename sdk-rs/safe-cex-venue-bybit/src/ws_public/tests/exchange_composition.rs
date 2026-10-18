use super::*;
use crate::exchange::BybitExchange;
use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange},
    socket_lifecycle::SocketSlot,
};
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, TransportError, TransportFuture,
    rate_limit::RateLimitClock,
};

#[derive(Default)]
struct Clock {
    now: Cell<u64>,
    sleeps: RefCell<Vec<u64>>,
}
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()> {
        self.sleeps.borrow_mut().push(milliseconds);
        self.now.set(self.now.get() + milliseconds);
        Box::pin(async { Ok(()) })
    }
}
struct Adapter {
    key: String,
    calls: Rc<RefCell<Vec<String>>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.calls.borrow_mut().push(self.key.clone());
        if let Some(body) = &request.body {
            assert_eq!(request.method, "post", "submission method");
            assert_eq!(request.url, "/v5/order/create", "submission endpoint");
            let payload = body
                .untransformed()
                .expect("submission must preserve unsigned structured payload")
                .clone()
                .into_value()
                .unwrap();
            if payload["fail"] == true {
                let mut failure = safe_cex_transport::RestFailure::pre_transmit(
                    "transport failure".into(),
                    request,
                );
                failure.response = Some(Box::new(RestResponse {
                    status: 400,
                    reason: Some("venue reason".into()),
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Encoded(
                        br#"{"retMsg":"venue rejection"}"#.to_vec(),
                    ),
                }));
                return Box::pin(async { Err(failure) });
            }
        }
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec()),
            })
        })
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "test browser-local adapters and clock"
)]
fn constructor_clients_and_sockets_follow_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/constructor-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 36);
    let mut compared = [0usize; 3];
    let mut constructors = [0usize; 2];
    for case in cases {
        let trace = RefCell::new(vec![]);
        let count = Cell::new(0);
        let calls = Rc::new(RefCell::new(vec![]));
        let clock = Arc::new(Clock::default());
        let runtime = Rc::new(Runtime::default());
        let base = BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(case["options"].clone()).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            crate::binding::exchange::settings(),
        );
        let result = BybitExchange::new(
            Rc::new(base),
            runtime.clone(),
            clock.clone(),
            |opts, base, _| {
                trace
                    .borrow_mut()
                    .push(json!(["api", opts.key, opts.secret, base.name]));
                let i = count.get();
                count.set(i + 1);
                if i == case["failAt"].as_i64().unwrap() {
                    return Err(TransportError::Host("factory failure".into()));
                }
                Ok(Arc::new(Adapter {
                    key: opts.key.clone(),
                    calls: calls.clone(),
                }))
            },
            |weak, category| {
                let venue = weak
                    .upgrade()
                    .expect("parent must exist before protocol construction");
                trace
                    .borrow_mut()
                    .push(json!(["public", venue.base().name, category]));
                Rc::new(Protocol)
            },
            |weak| {
                let venue = weak
                    .upgrade()
                    .expect("parent must exist before protocol construction");
                assert!(
                    venue.base().get_socket(SocketSlot::Public).is_some(),
                    "public socket precedes private"
                );
                trace
                    .borrow_mut()
                    .push(json!(["private", venue.base().name]));
                Rc::new(Protocol)
            },
        );
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "constructor trace {case}"
        );
        assert_eq!(result.is_err(), !case["error"].is_null());
        let Ok(venue) = result else {
            constructors[0] += 1;
            for field in ["statuses", "placements", "reinitializations"] {
                assert!(
                    case[field].as_array().unwrap().is_empty(),
                    "CONSTRUCTOR-CARDINALITY failed {field}"
                );
            }
            continue;
        };
        constructors[1] += 1;
        for (field, expected) in [
            ("statuses", 16),
            ("placements", 4),
            ("reinitializations", 5),
        ] {
            assert_eq!(
                case[field].as_array().unwrap().len(),
                expected,
                "CONSTRUCTOR-CARDINALITY source {field}"
            );
        }
        let mut nonnumeric_state = case["state"].clone();
        for name in ["lastReinitAt", "reinitRetryMs", "batchLimit"] {
            nonnumeric_state.as_object_mut().unwrap().remove(name);
        }
        assert_eq!(
            json!({
                "unifiedMarginStatus": *venue.unified_margin_status.borrow(),
                "marginType": venue.margin_type.get(),
                "needsReinit": venue.needs_reinit.get(),
                "positionMode": venue.position_mode.get(),
                "lastWebsocketPositionsUpdate": if venue.last_websocket_positions_update.get().is_none() { json!({"undefined":true}) } else { json!(venue.last_websocket_positions_update.get()) },
                "apiKeyInfo": *venue.api_key_info.borrow(),
                "forceUnifiedBalance": venue.force_unified_balance.get(),
                "accountType": venue.account_type(),
                "supportsTradeDerivedCandles": venue.supports_trade_derived_candles(),
                "requiresOrderLinkId": venue.requires_order_link_id(),
            }),
            nonnumeric_state,
            "actual constructor scalar initializers"
        );
        for (name, actual) in [
            ("lastReinitAt", venue.last_reinit_at.get()),
            ("reinitRetryMs", venue.reinit_retry_ms.get()),
            ("batchLimit", venue.batch_limit.get()),
        ] {
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                case["state"][name]["bits"].as_str().unwrap(),
                "constructor scalar bits: {name}"
            );
        }
        for native_index in 0..16 {
            let status = case["statuses"]
                .as_array()
                .unwrap()
                .get(native_index)
                .expect("CONSTRUCTOR-CARDINALITY native index coverage statuses");
            compared[0] += 1;
            *venue.unified_margin_status.borrow_mut() = status["raw"]
                .as_str()
                .map(|raw| serde_json::from_str(raw).unwrap());
            assert_eq!(
                json!(venue.account_type()),
                status["accountType"],
                "strict account-type numeric equality: {status}"
            );
        }
        *venue.unified_margin_status.borrow_mut() = Some(json!(6));
        for native_index in 0..4 {
            let placement = case["placements"]
                .as_array()
                .unwrap()
                .get(native_index)
                .expect("CONSTRUCTOR-CARDINALITY native index coverage placements");
            compared[1] += 1;
            let options: crate::order_params::BybitPlaceOrderOpts =
                safe_cex_num::numeric_ingress::js_from_value_ref(&placement["opts"]).unwrap();
            let result = Exchange::validate_order_capabilities(venue.as_ref(), &options);
            assert_eq!(
                result.as_ref().err().map(|error| error.to_string()),
                placement["error"].as_str().map(str::to_owned),
                "linear capability hook must remain empty: {placement}"
            );
            assert_eq!(placement["result"], "undefined");
        }

        for native_index in 0..5 {
            let outcome = case["reinitializations"]
                .as_array()
                .unwrap()
                .get(native_index)
                .expect("CONSTRUCTOR-CARDINALITY native index coverage reinitializations");
            compared[2] += 1;
            if outcome["abandoned"] == true {
                assert_eq!(
                    outcome["needsReinit"], true,
                    "source unawaited tail sets reinit"
                );
                continue;
            }
            venue
                .needs_reinit
                .set(outcome["initial"].as_bool().unwrap());
            let original = Rc::new("original request result".to_owned());
            let rejects = outcome["rejects"].as_bool().unwrap();
            let request = if rejects {
                Err(original.clone())
            } else {
                Ok(original.clone())
            };
            let result = ready(venue.reinit_on_throw(std::future::ready(request)));
            assert_eq!(result.is_err(), rejects);
            let received = match result {
                Ok(value) | Err(value) => value,
            };
            assert!(
                Rc::ptr_eq(&received, &original),
                "reinit preserves original result identity"
            );
            assert_eq!(outcome["same"], true);
            assert_eq!(
                json!(venue.needs_reinit.get()),
                outcome["needsReinit"],
                "source reinit settled state: {outcome}"
            );
        }
        // Rust owns its request future: dropping unpolled or pending wrappers
        // releases that request rather than leaving a detached rejection tail.
        for polled in [false, true] {
            struct DropRequest(Rc<Cell<bool>>);
            impl Drop for DropRequest {
                fn drop(&mut self) {
                    self.0.set(true);
                }
            }
            impl Future for DropRequest {
                type Output = Result<(), ()>;
                fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
                    Poll::Pending
                }
            }
            venue.needs_reinit.set(false);
            let dropped = Rc::new(Cell::new(false));
            let mut pending = Box::pin(venue.reinit_on_throw(DropRequest(dropped.clone())));
            if polled {
                assert!(
                    pending
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending()
                );
            }
            drop(pending);
            assert!(dropped.get(), "reinit drop releases request");
            assert!(
                !venue.needs_reinit.get(),
                "cancelled request has no rejection side effect"
            );
        }

        assert!(
            runtime.tasks.borrow().is_empty(),
            "constructor must not dial or spawn"
        );
        let public = venue.base().get_socket(SocketSlot::Public).unwrap();
        let private = venue.base().get_socket(SocketSlot::Private).unwrap();
        assert!(!public.emitter().is_private_socket(&public));
        assert!(private.emitter().is_private_socket(&private));
        assert!(Rc::ptr_eq(
            venue.base().secondary_clients.borrow().as_ref().unwrap(),
            venue.xhr()
        ));
        // Actual throttling, not just a constructor argument comparison.
        for _ in 0..6 {
            ready(venue.xhr().request(RestRequest::default())).unwrap();
        }
        assert_eq!(
            *clock.sleeps.borrow(),
            [1000],
            "sixth primary request must wait at five RPS"
        );
        for _ in 0..6 {
            ready(venue.unlimited_xhr().execute(RestRequest::default())).unwrap();
        }
        assert_eq!(
            *clock.sleeps.borrow(),
            [1000],
            "unlimited client must not share primary throttle"
        );
        if case["options"]["secondary_key"] == "read-key" {
            for _ in 0..6 {
                ready(venue.xhr().get(RestRequest::default())).unwrap();
            }
            assert_eq!(
                *clock.sleeps.borrow(),
                [1000],
                "read-only secondary has no primary rate limiter"
            );
            assert!(calls.borrow()[12..].iter().all(|key| key == "read-key"));
            ready(venue.xhr().request(RestRequest {
                method: "POST".into(),
                ..Default::default()
            }))
            .unwrap();
            assert_eq!(
                calls.borrow().last().unwrap(),
                "write-key",
                "writes must never use secondary credentials"
            );
        }
        let reports = RefCell::new(vec![]);
        let before = calls.borrow().len();
        let result = ready(venue.submit_create_order_requests(
            vec![json!({"id":1}), json!({"fail":true}), json!({"id":2})],
            |failure| {
                reports.borrow_mut().push(failure);
                Ok::<_, String>(())
            },
        ))
        .unwrap();
        assert_eq!(
            result
                .iter()
                .map(|(_, payload)| payload.clone())
                .collect::<Vec<_>>(),
            [json!({"id":1}), json!({"id":2})]
        );
        assert!(result.iter().all(|(response, _)| response.body
            == safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec())));
        assert_eq!(
            calls.borrow()[before..],
            ["write-key", "write-key", "write-key"],
            "submission must retain primary credentials across failures"
        );
        let reports = reports.borrow();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].message, "transport failure");
        assert_eq!(
            reports[0].response.as_ref().unwrap().body,
            safe_cex_transport::RestResponseBody::Encoded(
                br#"{"retMsg":"venue rejection"}"#.to_vec()
            )
        );
        assert_eq!(
            reports[0]
                .request
                .body
                .as_ref()
                .unwrap()
                .untransformed()
                .unwrap(),
            &safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"fail":true}))
                .unwrap()
        );
        let weak = Rc::downgrade(&venue);
        venue.dispose().unwrap();
        assert!(public.is_disposed() && private.is_disposed());
        drop(venue);
        assert!(
            weak.upgrade().is_none(),
            "retained sockets must not retain the venue"
        );
        assert!(private.emitter().is_disposed());
    }
    assert_eq!(
        constructors,
        [21, 15],
        "CONSTRUCTOR-CARDINALITY success/failure totals"
    );
    assert_eq!(
        compared,
        [240, 60, 75],
        "CONSTRUCTOR-CARDINALITY native comparison totals"
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn concrete_private_protocol_composes_source_open_sequence_and_real_auth() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-open-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 8);
    for inline in [false, true] {
        for case in &cases {
            let runtime = Rc::new(Runtime::default());
            runtime.inline.set(inline);
            let signer = Rc::new(AuthSigner {
                native_extra_data: RefCell::default(),
                requests: RefCell::default(),
                response: case["response"].to_string(),
                pending: false,
                wrong_variant: false,
            });
            let mut base = BaseExchange::new(
                safe_cex_types::ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"fixture-key","secret":"secret"})).unwrap(),
                Rc::new(DefaultStore::new()),
                runtime.clone(),
                crate::binding::exchange::settings(),
            );
            if case["mode"] != "local" {
                base.signers.remote_signer = Some(signer.clone());
            }
            let sync_calls = Rc::new(Cell::new(0));
            let calls = sync_calls.clone();
            let gate = Rc::new(Cell::new(false));
            let sync_gate = gate.clone();
            let venue = BybitExchange::with_private_protocol(
                Rc::new(base),
                runtime.clone(),
                Arc::new(Clock::default()),
                |options, _, _| {
                    Ok(Arc::new(Adapter {
                        key: options.key.clone(),
                        calls: Rc::default(),
                    }))
                },
                |_, _| Rc::new(Protocol),
                crate::ws_private::protocol::PrivateProtocolConfig {
                    sync_clock: Rc::new(move || {
                        calls.set(calls.get() + 1);
                        let gate = sync_gate.clone();
                        Box::pin(std::future::poll_fn(move |_| {
                            if gate.get() {
                                Poll::Ready(Ok(()))
                            } else {
                                Poll::Pending
                            }
                        }))
                    }),
                    now_ms: Rc::new(|| 1000.),
                    account_name: Some("account".into()),
                    relay_enabled: false,
                },
            )
            .unwrap();
            let private = venue.base().get_socket(SocketSlot::Private).unwrap();
            ready(private.create_websocket(
                "wss://stream.bybit.com/v5/private".into(),
                vec![],
                Default::default(),
            ))
            .unwrap();
            if case["disposed"] == true {
                venue.dispose().unwrap();
            }
            ready(private.on_open()).unwrap();
            assert_eq!(
                json!(sync_calls.get()),
                case["before"]["syncCalls"],
                "eager clock {case} inline={inline}"
            );
            assert_eq!(json!(*runtime.wire.0.borrow()), case["before"]["messages"]);
            assert_eq!(json!(*signer.requests.borrow()), case["before"]["requests"]);
            gate.set(true);
            super::reconnect_sequence::pump(&runtime);
            assert_eq!(
                json!(*runtime.wire.0.borrow()),
                case["messages"],
                "composed auth frames {case}"
            );
            assert_eq!(
                json!(*signer.requests.borrow()),
                case["requests"],
                "composed signer envelope {case}"
            );
            assert_eq!(json!(*runtime.sleeps.borrow()), case["sleeps"]);
            let errors: Vec<_> = runtime
                .console
                .borrow()
                .iter()
                .filter(|(level, _)| level == "error")
                .map(|(_, message)| {
                    message
                        .strip_prefix("exchange error ")
                        .unwrap_or(message)
                        .to_owned()
                })
                .collect();
            assert_eq!(json!(errors), case["errors"], "composed diagnostic {case}");
            if case["disposed"] != true {
                assert_eq!(json!(runtime.wire.1.get()), case["reconnects"]);
            }
            let weak = Rc::downgrade(&venue);
            venue.dispose().unwrap();
            drop(venue);
            super::reconnect_sequence::pump(&runtime);
            assert!(
                weak.upgrade().is_none(),
                "concrete protocol must not keep venue alive"
            );
        }
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn concrete_exchange_url_selectors_match_live_source() {
    use crate::exchange::{Network, PublicCategory};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/url-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 32);
    for case in cases {
        let runtime = Rc::new(Runtime::default());
        let mut base = BaseExchange::new(
            serde_json::from_value(case["name"].clone()).unwrap(),
            serde_json::from_value(json!({"key":"", "secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            crate::binding::exchange::settings(),
        );
        base.is_demo = case["isDemo"].as_bool().unwrap();
        let venue = BybitExchange::new(
            Rc::new(base),
            runtime,
            Arc::new(Clock::default()),
            |_, _, _| {
                Ok(Arc::new(Adapter {
                    key: String::new(),
                    calls: Rc::new(RefCell::new(vec![])),
                }))
            },
            |_, _| Rc::new(Protocol),
            |_| Rc::new(Protocol),
        )
        .unwrap();
        let network = match case["network"].as_str().unwrap() {
            "livenet" => Network::Livenet,
            "testnet" => Network::Testnet,
            _ => panic!("unknown network"),
        };
        let category = match case["category"].as_str().unwrap() {
            "linear" => PublicCategory::Linear,
            "inverse" => PublicCategory::Inverse,
            _ => panic!("unknown category"),
        };
        assert_eq!(
            venue.uses_bybit_demo_environment(),
            case["demo"].as_bool().unwrap(),
            "demo selector {case}"
        );
        assert_eq!(
            venue.get_bybit_rest_base_url(network),
            case["rest"].as_str().unwrap(),
            "REST selector {case}"
        );
        assert_eq!(
            venue.get_bybit_public_ws_url(category, network).unwrap(),
            case["public"].as_str().unwrap(),
            "public WS selector {case}"
        );
        assert_eq!(
            venue.get_bybit_private_ws_url(network).unwrap(),
            case["private"].as_str().unwrap(),
            "private WS selector {case}"
        );
        venue.base().dispose().unwrap();
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "test browser-local adapters and clock"
)]
fn execution_query_windows_follow_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/query-windows-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let runtime = Rc::new(Runtime::default());
    let venue = BybitExchange::new(
        Rc::new(BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            crate::binding::exchange::settings(),
        )),
        runtime,
        Arc::new(Clock::default()),
        |opts, _, _| {
            Ok(Arc::new(Adapter {
                key: opts.key.clone(),
                calls: Rc::new(RefCell::new(vec![])),
            }))
        },
        |_, _| Rc::new(Protocol),
        |_| Rc::new(Protocol),
    )
    .unwrap();
    assert_eq!(
        format!("{:016x}", venue.execution_query_max_range_ms.to_bits()),
        fixture["maxRange"]
    );
    assert_eq!(
        format!("{:016x}", venue.default_executions_lookback_ms.to_bits()),
        fixture["lookback"]
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 222);
    for (index, case) in cases.iter().enumerate() {
        let from = case["from"].as_str().map(safe_cex_num::number_from_string);
        let to = case["to"].as_str().map(safe_cex_num::number_from_string);
        let actual: Vec<_> = venue
            .get_execution_query_windows(from, to)
            .into_iter()
            .map(|(from, to)| {
                [
                    from.map(|v| {
                        format!(
                            "{:016x}",
                            if v.is_nan() {
                                0x7ff8_0000_0000_0000
                            } else {
                                v.to_bits()
                            }
                        )
                    }),
                    to.map(|v| {
                        format!(
                            "{:016x}",
                            if v.is_nan() {
                                0x7ff8_0000_0000_0000
                            } else {
                                v.to_bits()
                            }
                        )
                    }),
                ]
            })
            .collect();
        assert_eq!(
            json!(actual),
            case["windows"],
            "execution query windows case {index}: {case}"
        );
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn concrete_category_symbols_match_source_order_and_active_fallback() {
    use crate::exchange::PublicCategory;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/category-symbol-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 72);
    for (index, case) in cases.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        let markets = safe_cex_num::numeric_ingress::js_from_value::<Vec<safe_cex_types::Market>>(
            case["markets"].clone(),
        )
        .unwrap();
        Store::update(
            store.as_ref(),
            safe_cex_store::StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let base = BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"", "secret":""})).unwrap(),
            store,
            runtime.clone(),
            crate::binding::exchange::settings(),
        );
        let active: Vec<String> = serde_json::from_value(case["active"].clone()).unwrap();
        base.set_active_ticker_symbols(&active).unwrap();
        assert_eq!(
            base.get_active_ticker_symbols(),
            active,
            "fixture active symbols already normalized"
        );
        let venue = BybitExchange::new(
            Rc::new(base),
            runtime,
            Arc::new(Clock::default()),
            |_, _, _| {
                Ok(Arc::new(Adapter {
                    key: String::new(),
                    calls: Rc::new(RefCell::new(vec![])),
                }))
            },
            |_, _| Rc::new(Protocol),
            |_| Rc::new(Protocol),
        )
        .unwrap();
        let category = match case["category"].as_str().unwrap() {
            "linear" => PublicCategory::Linear,
            "inverse" => PublicCategory::Inverse,
            _ => panic!("unknown corpus category"),
        };
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(
            venue.get_ticker_symbols_for_category(category).unwrap(),
            expected,
            "quote filter and active fallback case {index}"
        );
        assert_eq!(
            fixture["symbols"].as_array().unwrap().len(),
            10,
            "CATEGORY-CARDINALITY symbol cases"
        );
        for symbol in fixture["symbols"].as_array().unwrap() {
            assert_eq!(
                venue.get_category_for_symbol(symbol["symbol"].as_str().unwrap()),
                symbol["category"].as_str().unwrap(),
                "symbol category routes options and inverse by symbol"
            );
        }
        if index == 0 {
            let settlements = fixture["settlements"].as_array().unwrap();
            assert_eq!(settlements.len(), 112);
            for (case_index, case) in settlements.iter().enumerate() {
                *venue.unified_margin_status.borrow_mut() = Some(case["status"].clone());
                let reports = Rc::new(RefCell::new(Vec::<String>::new()));
                let observed = reports.clone();
                let fail = case["fail"].as_bool().unwrap();
                let listener = venue.base().on(
                    "error",
                    Rc::new(move |_, data| {
                        let data = safe_cex_exchange::js_runtime::value::to_json(data).unwrap();
                        observed.borrow_mut().push(
                            data.as_str()
                                .unwrap()
                                .strip_prefix("Exchange Error: ")
                                .unwrap()
                                .to_owned(),
                        );
                        if fail {
                            let mut error = ExchangeError::new("reporter failure");
                            error.code = Some(safe_cex_exchange::js_runtime::value::Js::str(
                                "REPORTER_SENTINEL",
                            ));
                            Err(error)
                        } else {
                            Ok(())
                        }
                    }),
                );
                let result =
                    venue.get_settle_coins_for_category(case["category"].as_str().unwrap());
                assert_eq!(
                    serde_json::to_value(&*reports.borrow()).unwrap(),
                    case["reports"],
                    "settlement report ordering case {case_index}"
                );
                match result {
                    Ok(coins) => {
                        assert!(
                            case["error"].is_null(),
                            "settlement source success case {case_index}"
                        );
                        assert_eq!(
                            json!(coins),
                            case["coins"],
                            "settlement account coins case {case_index}"
                        );
                    }
                    Err(error) => {
                        assert_eq!(
                            json!(error.message),
                            case["error"],
                            "settlement rejection identity case {case_index}"
                        );
                        assert_eq!(
                            observed_error_code(&error.code)
                                .as_ref()
                                .and_then(Value::as_str),
                            case["original"]
                                .as_bool()
                                .unwrap()
                                .then_some("REPORTER_SENTINEL"),
                            "settlement reporter metadata case {case_index}"
                        );
                    }
                }
                venue.base().off("error", Some(&listener));
            }
            assert!(
                serde_json::from_str::<String>(fixture["settlementBoundary"].as_str().unwrap())
                    .is_err(),
                "settlement diagnostic UTF-8 ingress refusal"
            );
        }
        venue.dispose().unwrap();
    }
    assert_eq!(fixture["boundary"]["outputUnits"], json!([0xd800]));
    assert!(
        safe_cex_num::numeric_ingress::js_from_str::<safe_cex_types::Market>(
            fixture["boundary"]["wire"].as_str().unwrap()
        )
        .is_err(),
        "UTF-8 Market ingress refuses source-observable lone surrogate"
    );
}

#[test]
#[ignore = "requires installed source oracle dependencies"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn protective_side_matches_source_coercion_and_exact_one_position() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/protective-side-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 242, "PROTECTIVE source total");
    // Pin content independently so a same-size replacement cannot erase branches.
    let orders = json!([null, {}, [], false, 7, "BTC", {"symbol":"BTC"}, {"symbol":""}, {"symbol":7},
        {"positionIdx":1,"symbol":"BTC"}, {"positionIdx":2,"symbol":"BTC"},
        {"positionIdx":"1","symbol":"BTC"}, {"positionIdx":"2","symbol":"BTC"},
        {"positionIdx":true,"symbol":"BTC"}, {"positionIdx":null,"symbol":"BTC"},
        {"positionIdx":[],"symbol":"BTC"}, {"positionIdx":[1],"symbol":"BTC"},
        {"positionIdx":[2],"symbol":"BTC"}, {"positionIdx":{},"symbol":"BTC"},
        {"positionIdx":{"toString":null},"symbol":"BTC"}, {"position_idx":"2","symbol":"BTC"},
        {"positionIdx":null,"position_idx":1,"symbol":"BTC"}]);
    let p = |symbol: &str, side: &str, contracts: &str| json!({"symbol":symbol,"side":side,"contracts":contracts});
    let lists = json!([
        [],
        [p("BTC", "long", "1")],
        [p("BTC", "short", "-2")],
        [p("BTC", "long", "1"), p("BTC", "short", "2")],
        [p("OTHER", "long", "1"), p("BTC", "short", "3")],
        [p("BTC", "long", "0")],
        [p("BTC", "long", "0")],
        [p("BTC", "long", "NaN")],
        [p("BTC", "long", "Infinity")],
        [p("BTC", "long", "-Infinity")],
        [p("", "long", "1")]
    ]);
    let native_returns = RefCell::new(Vec::<usize>::new());
    let mut branches = [0_usize; 4];
    let mut reads = [0_usize; 2];
    for index in 0..242 {
        let case = cases
            .get(index)
            .expect("PROTECTIVE independent case coverage");
        assert_eq!(
            case.get("order").expect("PROTECTIVE order present"),
            &orders[index / 11],
            "PROTECTIVE order inventory case {index}"
        );
        assert_eq!(
            case.get("positions").expect("PROTECTIVE positions present"),
            &lists[index % 11],
            "PROTECTIVE positions inventory case {index}"
        );
        let source_error = case.get("error").expect("PROTECTIVE error present");
        let expected = case.get("expected").expect("PROTECTIVE expected present");
        branches[if !source_error.is_null() {
            3
        } else {
            match expected.as_str() {
                Some("buy") => 0,
                Some("sell") => 1,
                None => 2,
                _ => panic!("unexpected source side"),
            }
        }] += 1;
        let read_count = case
            .get("reads")
            .expect("PROTECTIVE reads present")
            .as_u64()
            .unwrap();
        let expected_read_count = usize::from([6, 8, 14, 15, 18, 21].contains(&(index / 11)));
        assert_eq!(
            read_count, expected_read_count as u64,
            "PROTECTIVE source reads case {index}"
        );
        reads[expected_read_count] += 1;
        let runtime = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        let positions = case["positions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut position: safe_cex_types::Position = serde_json::from_value(json!({
                    "id":i.to_string(),"symbol":p["symbol"],"side":p["side"],"entryPrice":1,
                    "notional":2,"leverage":3,"unrealizedPnl":4,"contracts":0,"liquidationPrice":5
                }))
                .unwrap();
                position.contracts = p["contracts"].as_str().unwrap().parse().unwrap();
                position
            })
            .collect::<Vec<_>>();
        assert_eq!(
            positions.len(),
            case["positions"].as_array().unwrap().len(),
            "PROTECTIVE native positions {index}"
        );
        Store::update(
            store.as_ref(),
            safe_cex_store::StoreUpdate {
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let base = BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store,
            runtime.clone(),
            crate::binding::exchange::settings(),
        );
        let venue = BybitExchange::new(
            Rc::new(base),
            runtime,
            Arc::new(Clock::default()),
            |_, _, _| {
                Ok(Arc::new(Adapter {
                    key: String::new(),
                    calls: Rc::new(RefCell::new(vec![])),
                }))
            },
            |_, _| Rc::new(Protocol),
            |_| Rc::new(Protocol),
        )
        .unwrap();
        let record_return = |actual| {
            native_returns.borrow_mut().push(index);
            actual
        };
        if source_error.is_null() {
            assert_eq!(
                record_return(venue.infer_protective_order_side(&case["order"])).unwrap(),
                match case["expected"].as_str() {
                    Some("buy") => Some(safe_cex_types::OrderSide::Buy),
                    Some("sell") => Some(safe_cex_types::OrderSide::Sell),
                    None => None,
                    _ => panic!("unexpected source side"),
                },
                "PROTECTIVE native side case {index}"
            );
        } else {
            let primitive =
                !case["order"].is_null() && !case["order"].is_object() && !case["order"].is_array();
            assert_eq!(
                {
                    let error = record_return(venue.infer_protective_order_side(&case["order"]))
                        .unwrap_err();
                    format!("{}: {}", error.name, error.message)
                },
                if primitive {
                    "TypeError: Bybit field lookup requires an object"
                } else {
                    case["message"].as_str().unwrap()
                },
                "PROTECTIVE coercion error case {index}"
            );
        }
    }
    assert_eq!(
        *native_returns.borrow(),
        (0..242).collect::<Vec<_>>(),
        "PROTECTIVE native return coverage"
    );
    assert_eq!(
        branches,
        [54, 59, 85, 44],
        "PROTECTIVE source branch totals"
    );
    assert_eq!(reads, [176, 66], "PROTECTIVE source read totals");
    assert_eq!(
        fixture
            .get("hostile")
            .expect("PROTECTIVE hostile witness present"),
        &json!([
            {"positionIdx":1,"invocations":1,"reads":0,"result":"sell"},
            {"positionIdx":2,"invocations":1,"reads":0,"result":"buy"}
        ]),
        "PROTECTIVE hostile execution"
    );
    assert_eq!(
        fixture
            .get("surrogateWitness")
            .expect("PROTECTIVE surrogate witness present"),
        &json!({"invocations":1,"result":"sell"}),
        "PROTECTIVE surrogate execution"
    );
    assert_eq!(
        fixture
            .get("surrogate")
            .expect("PROTECTIVE surrogate input present"),
        &json!(r#"{"symbol":"\ud800"}"#),
        "PROTECTIVE surrogate input"
    );
    assert!(
        serde_json::from_str::<Value>(fixture["surrogate"].as_str().unwrap()).is_err(),
        "PROTECTIVE UTF8 ingress refusal"
    );
    let boundaries = fixture["boundaries"].as_array().unwrap();
    assert_eq!(boundaries.len(), 2, "PROTECTIVE boundary count");
    for (index, boundary) in boundaries.iter().enumerate() {
        assert_eq!(
            boundary["expected"],
            if index == 0 { "buy" } else { "sell" }
        );
        let mut wire = json!({"id":"boundary","entryPrice":1,"notional":2,"leverage":3,
            "unrealizedPnl":4,"liquidationPrice":5});
        wire.as_object_mut()
            .unwrap()
            .extend(boundary["position"].as_object().unwrap().clone());
        assert!(
            safe_cex_num::numeric_ingress::js_from_value::<safe_cex_types::Position>(wire).is_err(),
            "PROTECTIVE typed store ingress refuses case {index}"
        );
    }
}

#[test]
#[ignore = "requires source oracle dependencies"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn linear_candle_validation_preserves_source_short_circuit() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/candle-validation-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let runtime = Rc::new(Runtime::default());
    let venue = BybitExchange::new(
        Rc::new(BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            crate::binding::exchange::settings(),
        )),
        runtime,
        Arc::new(Clock::default()),
        |opts, _, _| {
            Ok(Arc::new(Adapter {
                key: opts.key.clone(),
                calls: Rc::new(RefCell::new(vec![])),
            }))
        },
        |_, _| Rc::new(Protocol),
        |_| Rc::new(Protocol),
    )
    .unwrap();

    let reports = Rc::new(Cell::new(0));
    let observed = reports.clone();
    let _listener = venue.base().on(
        "error",
        Rc::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16, "CANDLE-VALIDATION source total");
    assert_eq!(
        fixture["reports"].as_array().unwrap().len(),
        0,
        "CANDLE-VALIDATION source reports"
    );
    let expected_inputs = json!(["1s", "5s", "30s", "1m", "1h", "1d", "s", "", "S", "unknown", null, false, 0, [], {}, {"endsWith": null}]);
    assert_eq!(
        fixture
            .get("hostile")
            .expect("CANDLE-VALIDATION hostile witness present"),
        &json!({"kind":"throwing-endsWith","accessor":true,"invocations":1,"reads":0,"result":"undefined"}),
        "CANDLE-VALIDATION hostile getter execution"
    );
    // Recording inside the invocation makes deletion of that call observable.
    let native_returns = RefCell::new(Vec::<usize>::new());
    for index in 0..16 {
        let case = cases
            .get(index)
            .expect("CANDLE-VALIDATION native case coverage");
        let input = case.get("input").expect("CANDLE-VALIDATION input present");
        assert_eq!(
            input, &expected_inputs[index],
            "CANDLE-VALIDATION discriminating input case {index}"
        );
        assert_eq!(case["result"], "undefined");
        assert_eq!(
            case.get("error")
                .expect("CANDLE-VALIDATION error field present"),
            &Value::Null
        );
        assert!(
            venue
                .validate_candle_interval(input)
                .map(|()| native_returns.borrow_mut().push(index))
                .is_ok(),
            "CANDLE-VALIDATION native short circuit case {index}"
        );
    }
    assert_eq!(
        *native_returns.borrow(),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        "CANDLE-VALIDATION native return coverage"
    );
    assert_eq!(reports.get(), 0, "CANDLE-VALIDATION native report count");
}

#[test]
#[ignore = "requires source oracle dependencies"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn cached_key_info_source_alias_and_native_snapshot() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/cached-key-info-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let runtime = Rc::new(Runtime::default());
    let venue = BybitExchange::new(
        Rc::new(BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            crate::binding::exchange::settings(),
        )),
        runtime,
        Arc::new(Clock::default()),
        |opts, _, _| {
            Ok(Arc::new(Adapter {
                key: opts.key.clone(),
                calls: Rc::new(RefCell::new(vec![])),
            }))
        },
        |_, _| Rc::new(Protocol),
        |_| Rc::new(Protocol),
    )
    .unwrap();

    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8, "KEY-CACHE source total");
    let mut compared = 0;
    for index in 0..8 {
        let case = cases.get(index).expect("KEY-CACHE native case coverage");
        assert!(
            venue.cached_api_key_info().is_none(),
            "KEY-CACHE native initial null"
        );
        let mut record: crate::types::BybitApiKeyInfo =
            safe_cex_num::numeric_ingress::js_from_value_ref(&case["input"]).unwrap();
        let number = u64::from_str_radix(case["number"].as_str().unwrap(), 16).unwrap();
        record.read_only = f64::from_bits(number);
        *venue.api_key_info.borrow_mut() = Some(record.clone());
        let mut view = venue
            .cached_api_key_info()
            .expect("KEY-CACHE populated native read");
        let actual_bits = if view.read_only.is_nan() {
            0x7ff8000000000000
        } else {
            view.read_only.to_bits()
        };
        assert_eq!(
            actual_bits, number,
            "KEY-CACHE native numeric bits case {index}"
        );
        let mut comparable = view.clone();
        comparable.read_only = 0.;
        let mut expected = record.clone();
        expected.read_only = 0.;
        assert_eq!(
            comparable, expected,
            "KEY-CACHE native full record identity case {index}"
        );
        {
            let mut cache = venue.api_key_info.borrow_mut();
            let owner = cache.as_mut().unwrap();
            owner.note = "owner-note".into();
            owner.permissions.contract_trade.push("owner-trade".into());
            owner.ips.push("owner-ip".into());
        }
        assert_eq!(case["afterOwner"]["note"], "owner-note");
        assert_eq!(
            case["afterOwner"]["trade"],
            json!(["trade-initial", "owner-trade"])
        );
        assert_eq!(case["afterOwner"]["ips"], json!(["ip-initial", "owner-ip"]));
        assert_eq!(case["afterOwner"]["number"], case["number"]);
        assert_eq!(
            view.note, "note-initial",
            "KEY-CACHE snapshot excludes owner mutation"
        );
        assert_eq!(view.permissions.contract_trade, vec!["trade-initial"]);
        assert_eq!(view.ips, vec!["ip-initial"]);
        view.id = "consumer-id".into();
        view.permissions.spot.push("consumer-spot".into());
        view.ips.push("consumer-ip".into());
        assert_eq!(case["afterConsumer"]["id"], "consumer-id");
        assert_eq!(
            case["afterConsumer"]["spot"],
            json!(["spot-initial", "consumer-spot"])
        );
        assert_eq!(
            case["afterConsumer"]["ips"],
            json!(["ip-initial", "owner-ip", "consumer-ip"])
        );
        let fresh = venue.cached_api_key_info().unwrap();
        assert_eq!(
            fresh.id, "id-initial",
            "KEY-CACHE owner excludes consumer mutation"
        );
        assert_eq!(fresh.note, "owner-note", "KEY-CACHE fresh native read");
        assert_eq!(fresh.permissions.spot, vec!["spot-initial"]);
        assert_eq!(
            fresh.permissions.contract_trade,
            vec!["trade-initial", "owner-trade"]
        );
        assert_eq!(fresh.ips, vec!["ip-initial", "owner-ip"]);
        record.id = "replacement-id".into();
        *venue.api_key_info.borrow_mut() = Some(record);
        assert_eq!(
            venue.cached_api_key_info().unwrap().id,
            case["replacement"]["freshId"].as_str().unwrap()
        );
        assert_eq!(view.id, case["replacement"]["oldId"].as_str().unwrap());
        *venue.api_key_info.borrow_mut() = None;
        assert!(
            venue.cached_api_key_info().is_none(),
            "KEY-CACHE native clear"
        );
        assert_eq!(view.id, case["oldAfterClear"].as_str().unwrap());
        compared += 1;
    }
    assert_eq!(compared, 8, "KEY-CACHE executed comparisons");
}

#[test]
#[ignore = "requires source oracle dependencies"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn linear_generated_link_id_matches_source_without_generation() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/generated-link-id-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let runtime = Rc::new(Runtime::default());
    let venue = BybitExchange::new(
        Rc::new(BaseExchange::new(
            safe_cex_types::ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            crate::binding::exchange::settings(),
        )),
        runtime,
        Arc::new(Clock::default()),
        |opts, _, _| {
            Ok(Arc::new(Adapter {
                key: opts.key.clone(),
                calls: Rc::new(RefCell::new(vec![])),
            }))
        },
        |_, _| Rc::new(Protocol),
        |_| Rc::new(Protocol),
    )
    .unwrap();

    let results = fixture["results"].as_array().unwrap();
    assert_eq!(results.len(), 3, "LINK-ID source total");
    assert_eq!(
        fixture["generatorCalls"], 0,
        "LINK-ID source generator calls"
    );
    assert!(
        !venue.requires_order_link_id(),
        "LINK-ID concrete native hook"
    );
    let mut compared = 0;
    for index in 0..3 {
        assert_eq!(
            results.get(index).expect("LINK-ID native index coverage"),
            "undefined"
        );
        assert_eq!(
            venue.generated_order_link_id().unwrap(),
            None,
            "LINK-ID native undefined case {index}"
        );
        compared += 1;
    }
    assert_eq!(compared, 3, "LINK-ID executed comparisons");
}

#[test]
#[ignore = "requires installed source oracle dependencies"]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "browser-local fixture adapters"
)]
fn opposite_position_matches_source_signed_contracts() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/opposite-position-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture.get("cases").unwrap().as_array().unwrap();
    assert_eq!(cases.len(), 396, "OPPOSITE source total");
    // Independent inputs prevent same-length replacement from erasing branches.
    let lists = json!([
        [],
        [["BTC", "short", "2"]],
        [["BTC", "long", "3"]],
        [["BTC", "short", "0"]],
        [["BTC", "long", "-2"]],
        [["BTC", "long", "NaN"]],
        [["BTC", "short", "Infinity"]],
        [["BTC", "short", "-Infinity"]],
        [["OTHER", "short", "4"]],
        [["BTC", "long", "1"], ["BTC", "short", "5"]],
        [["BTC", "short", "1"], ["BTC", "short", "2"]]
    ]);
    let amounts = [
        "-3",
        "-0",
        "0",
        "1",
        "2",
        "2.5",
        "NaN",
        "Infinity",
        "-Infinity",
    ];
    let runtime = Rc::new(Runtime::default());
    let store = Rc::new(DefaultStore::new());
    let base = BaseExchange::new(
        safe_cex_types::ExchangeName::Bybit,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        store.clone(),
        runtime.clone(),
        crate::binding::exchange::settings(),
    );
    let venue = BybitExchange::new(
        Rc::new(base),
        runtime,
        Arc::new(Clock::default()),
        |_, _, _| {
            Ok(Arc::new(Adapter {
                key: String::new(),
                calls: Rc::new(RefCell::new(vec![])),
            }))
        },
        |_, _| Rc::new(Protocol),
        |_| Rc::new(Protocol),
    )
    .unwrap();
    let inverse = fixture["inverseCases"].as_array().unwrap();
    assert_eq!(inverse.len(), 30, "OPPOSITE inverse source total");
    for (index, case) in inverse.iter().enumerate() {
        let positions = case["positions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(row_index, row)| {
                let mut position: safe_cex_types::Position = serde_json::from_value(json!({
                    "id":format!("p{row_index}"), "symbol":"BTCUSD", "side":row[0], "entryPrice":1, "leverage":1,
                    "liquidationPrice":0, "notional":1, "unrealizedPnl":0, "contracts":row[1]
                }))
                .unwrap();
                position.extra = row[2].as_f64().map(|native| safe_cex_types::PositionExtra {
                    contracts_native: Some(native),
                    unrealized_pnl_native: None,
                }).into();
                position
            })
            .collect::<Vec<_>>();
        Store::update(
            store.as_ref(),
            safe_cex_store::StoreUpdate {
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let side = if case["side"] == "buy" {
            safe_cex_types::OrderSide::Buy
        } else {
            safe_cex_types::OrderSide::Sell
        };
        assert_eq!(
            json!(
                venue
                    .has_full_sized_position_opposite_side(
                        "BTCUSD",
                        side,
                        case["amount"].as_f64().unwrap()
                    )
                    .unwrap()
            ),
            case["expected"],
            "OPPOSITE inverse native size case {index}: {case}"
        );
    }
    let returns = RefCell::new(Vec::<usize>::new());
    let mut branches = [0_usize; 2];
    for index in 0..396 {
        let case = &cases[index];
        let group = index / 36;
        let symbol = ["BTC", "OTHER"][(index / 18) % 2];
        let side = [
            safe_cex_types::OrderSide::Buy,
            safe_cex_types::OrderSide::Sell,
        ][(index / 9) % 2];
        let side_name = ["buy", "sell"][(index / 9) % 2];
        let amount = amounts[index % 9];
        assert_eq!(
            case.get("positions").unwrap(),
            &lists[group],
            "OPPOSITE positions inventory {index}"
        );
        assert_eq!(
            case.get("symbol").unwrap(),
            symbol,
            "OPPOSITE symbol inventory {index}"
        );
        assert_eq!(
            case.get("side").unwrap(),
            side_name,
            "OPPOSITE side inventory {index}"
        );
        assert_eq!(
            case.get("amount").unwrap(),
            amount,
            "OPPOSITE amount inventory {index}"
        );
        let positions = lists[group]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut position: safe_cex_types::Position = serde_json::from_value(json!({
                    "id":i.to_string(), "symbol":p[0], "side":p[1], "entryPrice":1,
                    "notional":2,"leverage":3,"unrealizedPnl":4,"contracts":0,"liquidationPrice":5
                }))
                .unwrap();
                position.contracts = p[2].as_str().unwrap().parse().unwrap();
                position
            })
            .collect::<Vec<_>>();
        assert_eq!(
            positions.len(),
            [0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2][group],
            "OPPOSITE native position count {index}"
        );
        Store::update(
            store.as_ref(),
            safe_cex_store::StoreUpdate {
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let expected = case.get("expected").unwrap().as_bool().unwrap();
        branches[usize::from(expected)] += 1;
        let record = |value| {
            returns.borrow_mut().push(index);
            value
        };
        assert_eq!(
            record(venue.has_full_sized_position_opposite_side(
                symbol,
                side,
                amount.parse().unwrap()
            ))
            .unwrap(),
            expected,
            "OPPOSITE native boolean {index}"
        );
    }
    assert_eq!(
        *returns.borrow(),
        (0..396).collect::<Vec<_>>(),
        "OPPOSITE native return coverage"
    );
    assert_eq!(branches, [347, 49], "OPPOSITE source branches");
    let boundaries = fixture.get("boundaries").unwrap();
    assert_eq!(
        boundaries,
        &json!([
            {"position":{"symbol":"BTC","side":"short","contracts":"0"},"expected":true},
            {"position":{"symbol":"BTC","side":"unexpected","contracts":2},"expected":false}
        ]),
        "OPPOSITE typed input witness inventory"
    );
    for index in 0..2 {
        let mut wire = json!({"id":"boundary","entryPrice":1,"notional":2,"leverage":3,
            "unrealizedPnl":4,"liquidationPrice":5});
        wire.as_object_mut()
            .unwrap()
            .extend(boundaries[index]["position"].as_object().unwrap().clone());
        assert!(
            safe_cex_num::numeric_ingress::js_from_value::<safe_cex_types::Position>(wire).is_err(),
            "OPPOSITE typed Position refusal {index}"
        );
    }
    assert_eq!(
        fixture.get("orderSideBoundary").unwrap(),
        &json!({"side":"unexpected","expected":true}),
        "OPPOSITE malformed order side witness"
    );
    assert!(
        serde_json::from_value::<safe_cex_types::OrderSide>(json!("unexpected")).is_err(),
        "OPPOSITE typed order side refusal"
    );
    assert_eq!(
        fixture.get("surrogate").unwrap(),
        &json!({"input":r#"{"symbol":"\ud800"}"#,"expected":true}),
        "OPPOSITE surrogate source witness"
    );
    assert!(
        serde_json::from_str::<Value>(fixture["surrogate"]["input"].as_str().unwrap()).is_err(),
        "OPPOSITE UTF8 input refusal"
    );
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture")
    })
}
