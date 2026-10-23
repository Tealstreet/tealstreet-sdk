//! The real Bybit adapter stages opaque work and signs only at shared dispatch.
mod support;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Outcome, Query, RequestId,
    Response,
};
use safe_cex_venue_bybit::{BybitLinear, Endpoints, Variant};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use support::{Harness, Options};

/// Setup is trusted test code; the venue receives only refs. Switching mode
/// exercises the same provider handle without reconfiguring a running engine.
struct Gate {
    local: Provider,
    mode: AtomicU8,
}
impl Signer for Gate {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.local.sign(request)
    }
    fn immediate(&self) -> bool {
        self.mode.load(Ordering::SeqCst) != 1
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        match self.mode.load(Ordering::SeqCst) {
            0 => self.local.sign_ready(request),
            1 => None,
            2 => Some(Err(SignError::Refused)),
            4 => {
                let refuse = request.context.method == "POST"
                    && request.operations.iter().any(|operation| match operation {
                        safe_cex_engine::signing::Operation::Sign {payload,..} => payload.iter().any(|part| matches!(part,safe_cex_engine::signing::PayloadPart::Bytes {bytes} if std::str::from_utf8(bytes).is_ok_and(|text|text.contains("\"qty\":\"0.1\"")))),
                        _ => false,
                    });
                if refuse {
                    Some(Err(SignError::Refused))
                } else {
                    self.local.sign_ready(request)
                }
            }
            _ => Some(Ok(SignResult {
                values: vec![b"bad\r\ncredential".to_vec(), vec![0; 32]],
                receipt: request.receipt,
                headers: Vec::new(),
            })),
        }
    }
}
fn options(variant: Variant, secondary: bool, window: Option<u32>) -> Options {
    Options {
        variant,
        recv_window: window,
        key: Some("dispatch-key".into()),
        secret: Some("dispatch-secret".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    }
}
fn engine(options: Options) -> (Engine<BybitLinear>, Arc<Gate>) {
    let (venue, local) = options.build();
    let gate = Arc::new(Gate {
        local,
        mode: AtomicU8::new(0),
    });
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(gate.clone())).unwrap();
    (engine, gate)
}
fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
        .as_str()
}
fn assert_signature(request: &HttpRequest, now: &str, key: &str, secret: &str, window: &str) {
    let payload = if request.method == Method::Get {
        request.url.split_once('?').map_or("", |(_, query)| query)
    } else {
        request.body.as_deref().unwrap_or("")
    };
    assert_eq!(header(request, "X-BAPI-TIMESTAMP"), now);
    assert_eq!(header(request, "X-BAPI-API-KEY"), key);
    assert_eq!(header(request, "X-BAPI-RECV-WINDOW"), window);
    assert_eq!(
        header(request, "X-BAPI-SIGN"),
        safe_cex_crypto::hmac_sha256(
            secret.as_bytes(),
            format!("{now}{key}{window}{payload}").as_bytes(),
            safe_cex_crypto::Encoding::Hex
        )
    );
}
fn work(actions: Vec<Action>) -> (RequestId, SignRequest) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::WsSend { .. })),
        "unsigned or prematurely signed egress"
    );
    let mut work = actions.into_iter().filter_map(|action| {
        if let Action::Sign { attempt, work, .. } = action {
            Some((attempt, work))
        } else {
            None
        }
    });
    let one = work.next().expect("one signing job");
    assert!(work.next().is_none());
    one
}
fn command(engine: &mut Engine<BybitLinear>, now: f64, command: Command) -> Vec<Action> {
    engine.handle(now, Event::Command(command));
    managed(engine, now)
}
/// Prepare one managed request whose signing the provider parks; the
/// returned actions carry its signing job.
fn prepare_parked(engine: &mut Engine<BybitLinear>, now: f64) -> Vec<Action> {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    engine.prepare_dispatch(now);
    let actions: Vec<Action> = engine.drain_actions().collect();
    let mut rest = Vec::new();
    for action in actions {
        match action {
            Action::ManagedHttp(managed) => {
                if let HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } = *managed
                {
                    engine.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
            }
            action => rest.push(action),
        }
    }
    rest.extend(engine.drain_actions());
    rest
}
/// Claim every bound request after a parked preparation (prepared outside
/// the immediate fixture driver), as the host would.
fn release_bound(engine: &mut Engine<BybitLinear>, now: f64) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction};
    let mut out = Vec::new();
    for action in engine.drain_actions().collect::<Vec<_>>() {
        match action {
            Action::ManagedHttp(managed) => {
                if let HttpAction::Bind {
                    permit, request, ..
                } = *managed
                    && let Ok(Claim::Released(_)) = engine.claim_http(now, permit)
                {
                    out.push(Action::Http(request));
                }
            }
            action => out.push(action),
        }
    }
    out.extend(engine.drain_actions());
    out
}
/// Ordinary writes and mode reads are managed: drive Prepare/Bind/Claim.
fn managed(engine: &mut Engine<BybitLinear>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
fn failed(actions: &[Action], command: u32) -> safe_cex_engine::VenueError {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| {
            if let Action::Notice(Notice::CommandFailed { command: id, error }) = action {
                (*id == command).then_some(error.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(failures.len(), 1, "exactly one owned failure");
    // Only the engine's one fresh reconcile order read may follow; never a write.
    let reconcile = |a: &Action| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get && r.url.contains("/v5/order/realtime"));
    assert!(!actions.iter().any(|a| !reconcile(a)
        && matches!(
            a,
            Action::Http(_)
                | Action::Sign { .. }
                | Action::WsSend { .. }
                | Action::Notice(Notice::CommandDone { .. })
        )));
    failures[0].clone()
}

#[test]
fn all_aliases_prepare_get_at_dispatch_using_selected_key_and_clamped_window() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Spot,
        Variant::Demo,
        Variant::Hyrotrader,
    ] {
        for (window, expected) in [
            (None, "20000"),
            (Some(0), "1"),
            (Some(500), "500"),
            (Some(u32::MAX), "20000"),
        ] {
            let (mut engine, _) = engine(options(variant, true, window));
            engine.start(1000.);
            engine.handle(
                1000.,
                Event::Command(Command::Query {
                    command: 42,
                    query: Query::Fills {
                        symbol: Some("BTCUSDT".into()),
                        order_id: None,
                        from_ms: Some(0.),
                        to_ms: Some(1000.),
                        limit: None,
                    },
                }),
            );
            // Account reads are managed: the fixture driver prepares and signs them.
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1234.75);
            let mut signed = 0;
            for action in actions {
                if let Action::Http(request) = action {
                    if request.url.contains("/v5/account/")
                        || request.url.contains("/v5/execution/list")
                    {
                        signed += 1;
                        assert_signature(&request, "1234", "read-key", "read-secret", expected);
                    } else {
                        assert!(
                            !request
                                .headers
                                .iter()
                                .any(|(key, _)| key.starts_with("X-BAPI"))
                        );
                    }
                }
            }
            assert!(signed > 0, "{variant:?}");
        }
    }
}

#[test]
fn expired_provider_result_is_reprepared_and_late_attempt_cannot_publish() {
    let (mut engine, gate) = engine(options(Variant::Linear, true, Some(500)));
    // A rare read before startup is valid and avoids unrelated account traffic.
    gate.mode.store(1, Ordering::SeqCst);
    // Identity is a managed read whose signing the provider parks.
    engine.handle(1000., Event::Command(Command::FetchAccount { command: 42 }));
    let (old, old_work) = work(prepare_parked(&mut engine, 1000.));
    assert_eq!(old_work.receipt.stamp_ms, Some(1000.));
    assert!(!format!("{old_work:?}").contains("read-secret"));
    let old_result = gate.local.sign_ready(&old_work).unwrap().unwrap();
    engine.handle(
        1600.,
        Event::Signed {
            attempt: old,
            result: Ok(old_result.clone()),
        },
    );
    let (fresh, fresh_work) = work(prepare_parked(&mut engine, 1600.));
    assert_ne!(old, fresh);
    assert_eq!(fresh_work.receipt.stamp_ms, Some(1600.));
    engine.handle(
        1601.,
        Event::Signed {
            attempt: old,
            result: Ok(old_result),
        },
    );
    assert!(engine.drain_actions().next().is_none());
    let result = gate.local.sign_ready(&fresh_work).unwrap().unwrap();
    engine.handle(
        1602.,
        Event::Signed {
            attempt: fresh,
            result: Ok(result),
        },
    );
    let requests: Vec<_> = release_bound(&mut engine, 1602.)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_signature(&requests[0], "1600", "read-key", "read-secret", "500");
}

#[test]
fn provider_refusal_invalid_presentation_and_timeout_are_owned_not_sent_reads() {
    for mode in [1, 2, 3] {
        for query in [false, true] {
            let (mut engine, gate) = engine(options(Variant::Linear, true, None));
            gate.mode.store(mode, Ordering::SeqCst);
            let cmd = if query {
                Command::Query {
                    command: 42,
                    query: Query::Fills {
                        symbol: Some("BTCUSDT".into()),
                        order_id: None,
                        from_ms: None,
                        to_ms: None,
                        limit: None,
                    },
                }
            } else {
                Command::FetchAccount { command: 42 }
            };
            let mut actions = if mode == 1 && !query {
                // Identity is managed: its parked signing stays with the provider.
                engine.handle(1000., Event::Command(cmd));
                prepare_parked(&mut engine, 1000.)
            } else {
                command(&mut engine, 1000., cmd)
            };
            if mode == 1 {
                let timer = actions
                    .iter()
                    .find_map(|a| {
                        if let Action::SetTimer { timer, .. } = a {
                            Some(*timer)
                        } else {
                            None
                        }
                    })
                    .unwrap();
                engine.handle(21_001., Event::TimerFired { timer });
                actions = engine.drain_actions().collect();
            }
            assert_eq!(failed(&actions, 42).outcome, Some(Outcome::NotSent));
        }
    }
}

const EMPTY: &str = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":""}}"#;
fn ready(variant: Variant) -> (Harness, Arc<Gate>) {
    ready_options(options(variant, false, None))
}
fn ready_options(options: Options) -> (Harness, Arc<Gate>) {
    let variant = options.variant;
    let (engine, gate) = engine(options);
    let mut h = Harness::started_engine(engine);
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, EMPTY);
    }
    if variant.is_spot() {
        if h.has_request("status=PreLaunch") {
            h.respond("status=PreLaunch", 200, EMPTY);
        }
        h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
        h.respond("/v5/market/tickers", 200, support::TICKERS);
    } else {
        h.load_public();
    }
    if variant.is_hyro() {
        let socket = h.socket("/v5/private").unwrap();
        h.event(Event::WsOpened { socket });
        for op in ["auth", "subscribe"] {
            let text = h
                .sent_on("/v5/private")
                .into_iter()
                .rev()
                .find(|text| text.contains(&format!("\"op\":\"{op}\"")))
                .unwrap();
            #[derive(serde::Deserialize)]
            struct Sent {
                req_id: String,
            }
            let sent: Sent = serde_json::from_str(&text).unwrap();
            h.message(
                "/v5/private",
                &format!(
                    r#"{{"op":"{op}","success":true,"req_id":"{}"}}"#,
                    sent.req_id
                ),
            );
        }
        h.respond("/v5/order/realtime", 200, EMPTY);
        h.respond("/v5/position/list", 200, EMPTY);
        h.respond("/v5/account/wallet-balance",200,r#"{"retCode":0,"result":{"list":[{"accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMarginBalance":"1000","totalAvailableBalance":"1000","totalInitialMargin":"0","totalMaintenanceMargin":"0","totalPerpUPL":"0","coin":[{"coin":"USDT","walletBalance":"1000","usdValue":"1000","equity":"1000","totalPositionIM":"0","totalOrderIM":"0","locked":"0"}]}]}}"#);
        assert!(h.engine.store().loaded().orders);
    }
    h.requests.clear();
    (h, gate)
}
fn order() -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.1,
        price: Some(100.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

#[test]
fn ordinary_and_hyro_write_provider_refusal_never_becomes_ambiguous_or_resends() {
    for variant in [Variant::Linear, Variant::Hyrotrader] {
        let (mut h, gate) = ready(variant);
        // Resolve ordinary mode under the local provider, then refuse the POST.
        if variant.is_hyro() {
            gate.mode.store(2, Ordering::SeqCst);
        }
        let mut actions = command(
            &mut h.engine,
            h.now,
            Command::PlaceOrders {
                command: 42,
                orders: vec![order()],
            },
        );
        if !variant.is_hyro() {
            let request = actions
                .into_iter()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(request.url.contains("/v5/position/list"));
            gate.mode.store(2, Ordering::SeqCst);
            h.engine.handle(h.now,Event::HttpResponse {request:request.id,response:Response {status:200,headers:"",body:r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#}});
            actions = managed(&mut h.engine, h.now);
        }
        assert_eq!(failed(&actions, 42).outcome, Some(Outcome::NotSent));
        assert!(h.engine.store().orders().is_empty());
    }
}

#[test]
fn ws_auth_is_send_time_primary_only_and_retired_socket_cannot_send() {
    for variant in [Variant::Linear, Variant::Hyrotrader] {
        let (mut h, gate) = ready(variant);
        let socket = h.socket("/v5/private").unwrap();
        // Ordinary setup has not opened private; Hyro has. Retire/reopen either way.
        h.engine
            .handle(h.now, Event::WsClosed { socket, code: 1006 });
        let actions: Vec<_> = h.engine.drain_actions().collect();
        let timer = actions
            .iter()
            .filter_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((*timer, *at_ms))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        h.engine
            .handle(timer.1, Event::TimerFired { timer: timer.0 });
        let next = h
            .engine
            .drain_actions()
            .find_map(|a| {
                if let Action::WsConnect { socket, .. } = a {
                    Some(socket)
                } else {
                    None
                }
            })
            .unwrap();
        gate.mode.store(1, Ordering::SeqCst);
        h.engine.handle(timer.1, Event::WsOpened { socket: next });
        h.engine.prepare_dispatch(timer.1 + 123.75);
        let (attempt, work) = work(h.engine.drain_actions().collect());
        assert_eq!(work.receipt.stamp_ms, None);
        assert_eq!(
            work.receipt.expiry_ms,
            Some((timer.1 + 123.75 + 10_000.).floor())
        );
        let result = gate.local.sign_ready(&work).unwrap().unwrap();
        h.engine.handle(
            timer.1 + 124.,
            Event::WsClosed {
                socket: next,
                code: 1006,
            },
        );
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            timer.1 + 125.,
            Event::Signed {
                attempt,
                result: Ok(result),
            },
        );
        assert!(
            !h.engine
                .drain_actions()
                .any(|a| matches!(a,Action::WsSend {socket,..} if socket==next))
        );
    }
}

#[test]
fn hyro_provider_refusal_preserves_an_earlier_ack_and_unknown_create() {
    for lost in [false, true] {
        let (mut h, gate) = ready(Variant::Hyrotrader);
        let mut second = order();
        second.amount = 0.2;
        h.command(Command::PlaceOrders {
            command: 42,
            orders: if lost {
                vec![order()]
            } else {
                vec![order(), second]
            },
        });
        let request = h.take("/v5/order/create");
        assert_signature(
            &request,
            &h.now.floor().to_string(),
            "dispatch-key",
            "dispatch-secret",
            "20000",
        );
        gate.mode.store(2, Ordering::SeqCst);
        if lost {
            h.engine.handle(
                h.now,
                Event::HttpFailed {
                    request: request.id,
                    error: "lost after dispatch",
                },
            );
        } else {
            h.engine.handle(
                h.now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"retCode":0,"result":{"orderId":"accepted"}}"#,
                    },
                },
            );
        }
        let actions: Vec<_> = h.engine.drain_actions().collect();
        let error = failed(&actions, 42);
        assert_eq!(
            error.outcome,
            if lost { Some(Outcome::Unknown) } else { None }
        );
        if !lost {
            let accepted = h.engine.store().order("accepted").unwrap();
            assert_eq!(accepted.amount, 0.1);
            assert_eq!(accepted.price, 100.);
            assert_eq!(accepted.filled, 0.);
        }
    }
}

#[test]
fn static_ws_serialization_uses_primary_and_preserves_hyro_correlation_and_string_expiry() {
    for variant in [Variant::Linear, Variant::Hyrotrader] {
        let (mut h, gate) = ready_options(options(variant, true, None));
        let socket = h.socket("/v5/private").unwrap();
        h.engine
            .handle(h.now, Event::WsClosed { socket, code: 1006 });
        let (timer, now) = h
            .engine
            .drain_actions()
            .filter_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((timer, at_ms))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        h.engine.handle(now, Event::TimerFired { timer });
        let socket = h
            .engine
            .drain_actions()
            .find_map(|a| {
                if let Action::WsConnect { socket, .. } = a {
                    Some(socket)
                } else {
                    None
                }
            })
            .unwrap();
        h.engine.handle(now, Event::WsOpened { socket });
        h.engine.prepare_dispatch(now + 321.75);
        let messages: Vec<_> = h
            .engine
            .drain_actions()
            .filter_map(|a| {
                if let Action::WsSend {
                    socket: target,
                    text,
                } = a
                {
                    (target == socket).then_some(text)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(messages.len(), 1);
        let expiry = (now + 321.75 + 10000.).floor().to_string();
        let signature = safe_cex_crypto::hmac_sha256(
            b"dispatch-secret",
            format!("GET/realtime{expiry}").as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        );
        #[derive(serde::Deserialize)]
        struct Auth {
            req_id: Option<String>,
            op: String,
            args: [String; 3],
        }
        let auth: Auth = serde_json::from_str(&messages[0]).unwrap();
        assert_eq!(auth.op, "auth");
        assert_eq!(
            auth.args,
            ["dispatch-key".into(), expiry.clone(), signature.clone()]
        );
        let prefix = if variant.is_hyro() {
            format!(
                "\"req_id\":{},",
                serde_json::to_string(auth.req_id.as_deref().unwrap()).unwrap()
            )
        } else {
            assert!(auth.req_id.is_none());
            String::new()
        };
        assert_eq!(
            messages[0],
            format!(r#"{{{prefix}"op":"auth","args":["dispatch-key","{expiry}","{signature}"]}}"#)
        );
        assert_eq!(gate.mode.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn spot_mixed_provider_and_native_refusal_aggregate_keeps_reached_venue_evidence_in_both_orders() {
    for provider_first in [true, false] {
        let (mut h, gate) = ready(Variant::Spot);
        gate.mode.store(4, Ordering::SeqCst);
        let first = order();
        let mut second = order();
        second.amount = 0.2;
        h.command(Command::PlaceOrders {
            command: 42,
            orders: if provider_first {
                vec![first, second]
            } else {
                vec![second, first]
            },
        });
        let request = h.take("/v5/order/create");
        assert!(
            !h.has_request("/v5/order/create"),
            "only the non-refused payload reaches HTTP"
        );
        assert!(request.body.as_deref().unwrap().contains("\"qty\":\"0.2\""));
        assert_signature(
            &request,
            &h.now.floor().to_string(),
            "dispatch-key",
            "dispatch-secret",
            "20000",
        );
        h.engine.handle(h.now,Event::HttpResponse {request:request.id,response:Response {status:200,headers:"",body:r#"{"retCode":110007,"retMsg":"Insufficient available balance","result":{}}"#}});
        let actions = managed(&mut h.engine, h.now);
        let error = failed(&actions, 42);
        assert_eq!(
            error.outcome,
            Some(Outcome::Rejected),
            "provider_first={provider_first}: a native refusal proves a POST reached the venue"
        );
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Rejected));
        assert_eq!(error.venue_code.as_deref(), Some("110007"));
        // The journal reports the earliest leg that reached the venue.
        assert!(
            error.message.contains("Insufficient available balance"),
            "reached-venue error text: {}",
            error.message
        );
        assert!(h.engine.store().orders().is_empty());
    }
}

#[test]
fn spot_mixed_provider_and_bare_http_failures_match_native_outcome_in_both_orders() {
    for status in [400, 429] {
        let (mut standalone, _) = ready(Variant::Spot);
        let mut native = order();
        native.amount = 0.2;
        standalone.command(Command::PlaceOrders {
            command: 41,
            orders: vec![native.clone()],
        });
        let request = standalone.take("/v5/order/create");
        standalone.engine.handle(
            standalone.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body: "gateway refusal without native envelope",
                },
            },
        );
        let actions: Vec<_> = standalone.engine.drain_actions().collect();
        let expected = failed(&actions, 41);
        assert_ne!(expected.outcome, Some(Outcome::NotSent));
        for provider_first in [true, false] {
            let (mut h, gate) = ready(Variant::Spot);
            gate.mode.store(4, Ordering::SeqCst);
            h.command(Command::PlaceOrders {
                command: 42,
                orders: if provider_first {
                    vec![order(), native.clone()]
                } else {
                    vec![native.clone(), order()]
                },
            });
            let request = h.take("/v5/order/create");
            assert!(!h.has_request("/v5/order/create"));
            assert!(request.body.as_deref().unwrap().contains("\"qty\":\"0.2\""));
            h.engine.handle(
                h.now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body: "gateway refusal without native envelope",
                    },
                },
            );
            let actions = managed(&mut h.engine, h.now);
            let error = failed(&actions, 42);
            assert_eq!(
                error.outcome, expected.outcome,
                "HTTP{status} provider_first={provider_first}: later HTTP cannot remain provider-only NotSent"
            );
            assert_eq!(error.kind, expected.kind);
            assert_eq!(error.venue_code, expected.venue_code);
            // The journal reports the earliest leg that reached the venue.
            assert!(error.message.contains(&expected.message));
            assert!(h.engine.store().orders().is_empty());
        }
    }
}

fn refused_retry_after_initial_dispatch(
    variant: Variant,
    native_code: Option<i64>,
) -> safe_cex_engine::VenueError {
    let (mut h, gate) = ready(variant);
    let mut order = order();
    if !variant.is_hyro() {
        order.client_order_id = Some("immutable-dispatch-id".into());
    }
    h.command(Command::PlaceOrders {
        command: 42,
        orders: vec![order],
    });
    let original = h.take("/v5/order/create");
    let body = original.body.as_deref().unwrap();
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SentIdentity {
        order_link_id: String,
    }
    let sent: SentIdentity = serde_json::from_str(body).unwrap();
    assert!(!sent.order_link_id.is_empty());
    if !variant.is_hyro() {
        assert_eq!(sent.order_link_id, "immutable-dispatch-id");
    }
    gate.mode.store(1, Ordering::SeqCst);
    if let Some(code) = native_code {
        let message = if code == 10002 {
            "recv_window rejection"
        } else {
            "error sign"
        };
        let body = format!(r#"{{"retCode":{code},"retMsg":"{message}","result":{{}}}}"#);
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request: original.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
    } else {
        h.engine.handle(
            h.now,
            Event::HttpFailed {
                request: original.id,
                error: "network lost; error sign",
            },
        );
    }
    let retry_actions = if variant.is_hyro() {
        h.engine.drain_actions().collect()
    } else {
        prepare_parked(&mut h.engine, h.now)
    };
    let (attempt, retry_work) = work(retry_actions);
    assert!(retry_work.operations.iter().any(|op| matches!(op,safe_cex_engine::signing::Operation::Sign {payload,..} if matches!(payload.last(),Some(safe_cex_engine::signing::PayloadPart::Bytes {bytes}) if bytes==body.as_bytes()))),"retry must retain exact body and immutable client identity");
    assert_eq!(
        h.progress
            .iter()
            .filter(|(command, _)| *command == 42)
            .count(),
        1
    );
    h.engine.handle(
        h.now,
        Event::Signed {
            attempt,
            result: Err(SignError::Refused),
        },
    );
    let mut actions: Vec<_> = if variant.is_hyro() {
        h.engine.drain_actions().collect()
    } else {
        prepare_parked(&mut h.engine, h.now)
    };
    // Both paths reconcile the earlier dispatched create with one signed order
    // read; no new mutation may be emitted on this provider failure.
    let (attempt, repair) = work(actions.clone());
    assert_eq!(repair.context.method, "GET");
    assert!(repair.context.path.starts_with("/v5/order/realtime"));
    h.engine.handle(
        h.now,
        Event::Signed {
            attempt,
            result: Err(SignError::Refused),
        },
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if !r.url.contains("/v5/order/realtime"))),
        "no write resend"
    );
    // The only signing job here is the reconcile read checked above.
    actions.retain(|a| !matches!(a, Action::Sign { .. }) && !matches!(a, Action::Http(_)));
    actions.extend(if variant.is_hyro() {
        h.engine.drain_actions().collect::<Vec<_>>()
    } else {
        managed(&mut h.engine, h.now)
    });
    let error = failed(&actions, 42);
    assert!(!error.message.contains("dispatch-secret"));
    assert!(!error.message.contains("https://"));
    assert!(h.engine.store().orders().is_empty());
    error
}

#[test]
fn native_signature_rejection_then_retry_provider_refusal_retains_prior_attempt_uncertainty() {
    // The Plan journal keeps the retry's typed provider refusal, and the
    // earlier released attempt keeps the whole command Unknown.
    for code in [10004, 10002] {
        let error = refused_retry_after_initial_dispatch(Variant::Spot, Some(code));
        assert_eq!(
            error.outcome,
            Some(Outcome::Unknown),
            "native code {code}: retry refusal cannot imply whole command NotSent"
        );
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert_eq!(error.venue_code, None);
    }
}

#[test]
fn host_failure_never_resends_an_ordinary_write_and_stays_unknown() {
    // Transport text, even "error sign", is no resend authority for a write.
    let (mut h, _gate) = ready(Variant::Spot);
    let mut order = order();
    order.client_order_id = Some("immutable-dispatch-id".into());
    h.command(Command::PlaceOrders {
        command: 42,
        orders: vec![order],
    });
    let original = h.take("/v5/order/create");
    h.engine.handle(
        h.now,
        Event::HttpFailed {
            request: original.id,
            error: "network lost; error sign",
        },
    );
    let actions = managed(&mut h.engine, h.now);
    assert!(
        !actions.iter().any(|action| matches!(action, Action::Sign { .. } | Action::Http(_)
            if !matches!(action, Action::Http(request) if request.url.contains("/v5/order/realtime")))),
        "no write resend"
    );
    let error = failed(&actions, 42);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Network));
    assert_eq!(error.venue_code, None);
    assert!(!error.message.contains("dispatch-secret"));
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn hyro_native_signature_rejection_then_retry_provider_refusal_reconciles_prior_attempt() {
    let error = refused_retry_after_initial_dispatch(Variant::Hyrotrader, Some(10004));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.kind, None);
    assert_eq!(error.venue_code.as_deref(), None);
}

#[test]
fn hyro_host_failure_eligible_retry_then_provider_refusal_retains_ambiguity() {
    let error = refused_retry_after_initial_dispatch(Variant::Hyrotrader, None);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.kind, None);
    assert_eq!(error.venue_code, None);
}
