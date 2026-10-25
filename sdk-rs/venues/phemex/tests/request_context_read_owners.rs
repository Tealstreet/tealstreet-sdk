//! Read-owner handoff, native financial behavior, expiry and failure recovery.
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, RequestId, RequestRoute, Response,
    SocketId,
};
mod support;
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;
const NOW: f64 = 1_700_000_000_000.;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"},{"symbol":"ETHUSDC","type":"PerpetualV2","settleCurrency":"USDC","baseCurrency":"ETH","quoteCurrency":"USDC","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"},{"symbol":"ETHUSDC","bidRp":"1999","askRp":"2001","lastRp":"2000","markRp":"2000","indexRp":"2000","openRp":"1900","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}"#;
const BTC: &str = r#"{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","side":"Buy","size":"0.01","leverageRr":"-20","avgEntryPriceRp":"69000","liquidationPriceRp":"50000","transactTimeNs":1700000000000000000}"#;
const ETH: &str = r#"{"symbol":"ETHUSDC","currency":"USDC","posMode":"Hedged","posSide":"Long","side":"Buy","size":"1","leverageRr":"10","avgEntryPriceRp":"1900","liquidationPriceRp":"1000","transactTimeNs":1700000000000000000}"#;
fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}

fn release<V: safe_cex_engine::Venue>(
    e: &mut Engine<V>,
    now: f64,
    actions: Vec<Action>,
) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, Prepared};
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from(actions);
    while let Some(action) = queue.pop_front() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => e.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id: preparation.attempt().request.0,
                        expires_at: None,
                    },
                ),
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    if matches!(e.claim_http(now, permit), Ok(Claim::Released(_))) {
                        out.push(Action::Http(request));
                    }
                }
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => {
                    panic!("single-request fixture cannot release a request group")
                }
            },
            other => out.push(other),
        }
        queue.extend(e.drain_actions());
    }
    out
}

fn response(e: &mut Engine<PhemexPerpetuals>, now: f64, id: RequestId, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

fn request(actions: &[Action], query: &str) -> HttpRequest {
    requests(actions)
        .into_iter()
        .find(|r| r.url.contains(query))
        .unwrap()
}

fn envelope(currency: &str, total: f64, used: f64, rows: &str) -> String {
    format!(
        r#"{{"code":0,"data":{{"account":{{"currency":"{currency}","accountBalanceRv":"{total}","totalUsedBalanceRv":"{used}"}},"positions":[{rows}]}}}}"#
    )
}

fn started() -> (Engine<PhemexPerpetuals>, Vec<Action>, SocketId) {
    started_with(true)
}
fn started_with(release: bool) -> (Engine<PhemexPerpetuals>, Vec<Action>, SocketId) {
    started_config(release, None)
}
fn started_config(
    release: bool,
    gate: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> (Engine<PhemexPerpetuals>, Vec<Action>, SocketId) {
    let mut e = support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    });
    if let Some(gate) = gate {
        let provider = e.signer().unwrap();
        e.set_signer(Some(std::sync::Arc::new(failures::Gated {
            inner: provider,
            paused: gate,
        })))
        .unwrap();
    }
    e.start(NOW);
    let a: Vec<_> = e.drain_actions().collect();
    response(&mut e, NOW, request(&a, "products").id, 200, PRODUCTS);
    let mut a: Vec<_> = if release {
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
    } else {
        e.drain_actions().collect()
    };
    if release {
        response(&mut e, NOW, request(&a, "24hr/all").id, 200, DAILY);
        a.extend(e.drain_actions());
    }
    let socket = a
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    (e, a, socket)
}

fn seed(e: &mut Engine<PhemexPerpetuals>, actions: &[Action]) {
    response(
        e,
        NOW,
        request(actions, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., BTC),
    );
    response(
        e,
        NOW,
        request(actions, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
}

fn authenticate(e: &mut Engine<PhemexPerpetuals>, socket: SocketId) {
    e.handle(NOW, Event::WsOpened { socket });
    e.handle(
        NOW,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"error":null,"result":{"status":"success"}}"#,
        },
    );
}

fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

fn identity(read: bool) -> (Engine<PhemexPerpetuals>, Vec<Action>) {
    identity_with(read, true)
}
fn identity_with(read: bool, release: bool) -> (Engine<PhemexPerpetuals>, Vec<Action>) {
    let mut e = support::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    let actions = if release {
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
    } else {
        e.drain_actions().collect()
    };
    (e, actions)
}
fn prepares(actions: &[Action], path: &str) -> usize {
    actions.iter().filter(|a| matches!(a, Action::ManagedHttp(action)
        if matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains(path)))).count()
}
fn no_numeric_terminal(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )),
        "background work has no command terminal"
    );
}
fn uncertain() -> (Engine<PhemexPerpetuals>, Vec<Action>, f64) {
    uncertain_with(true)
}
fn uncertain_with(release: bool) -> (Engine<PhemexPerpetuals>, Vec<Action>, f64) {
    uncertain_config(release, None)
}
fn uncertain_config(
    release: bool,
    gate: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> (Engine<PhemexPerpetuals>, Vec<Action>, f64) {
    let (mut e, actions, socket) = started_config(true, gate);
    seed(&mut e, &actions);
    authenticate(&mut e, socket);
    e.handle(
        NOW,
        Event::WsMessage {
            socket,
            text: r#"{"type":"snapshot","orders_p":[]}"#,
        },
    );
    e.drain_actions().for_each(drop);
    assert!(e.store().loaded().orders);
    e.handle(
        NOW + 1000.,
        Event::Command(Command::PlaceOrders {
            command: 71,
            orders: vec![opts("uncertain")],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let create = request(&actions, "/g-orders/create");
    e.handle(
        NOW + 1500.,
        Event::HttpFailed {
            request: create.id,
            error: "transport outcome unknown",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 71, .. })))
    );
    assert!(
        requests(&actions)
            .iter()
            .all(|r| !r.url.contains("activeList"))
    );
    let wake = NOW + 66_000.;
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == wake => Some(*timer),
            _ => None,
        })
        .expect("native write-expiry plus clock-skew reconciliation fence");
    e.handle(wake, Event::TimerFired { timer });
    let actions = if release {
        safe_cex_engine::test_support::prepared_actions(&mut e, wake)
    } else {
        e.drain_actions().collect()
    };
    (e, actions, wake)
}
fn revealed() -> &'static str {
    r#"{"code":0,"data":{"rows":[{"orderID":"revealed","clOrdID":"uncertain","symbol":"BTCUSDT","side":"Buy","posSide":"Merged","orderType":"Limit","ordStatus":"New","orderQtyRq":"0.01","cumQtyRq":"0","leavesQtyRq":"0.01","priceRp":"69000","transactTimeNs":1700000066000000000}]}}"#
}
#[test]
fn identity_requires_managed_preparation_before_signing_or_transport() {
    let (_, actions) = identity_with(true, false);
    assert_eq!(prepares(&actions, "/g-accounts/accountPositions"), 1);
    assert!(requests(&actions).is_empty(), "no eager signed HTTP bypass");
}
#[test]
fn identity_at_leg_deadline_without_timer_cannot_return_response_identity() {
    let (mut e, actions) = identity(false);
    let r = request(&actions, "currency=USDT");
    response(
        &mut e,
        NOW + 20_000.,
        r.id,
        200,
        r#"{"code":0,"data":{"account":{"userID":"expired"}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 42, .. }))),
        "expiry must precede native terminal without timer ingress"
    );
}
#[test]
fn both_currency_reads_require_background_managed_preparation() {
    let (_, actions, _) = started_with(false);
    assert_eq!(prepares(&actions, "/g-accounts/accountPositions"), 2);
    assert!(
        requests(&actions)
            .iter()
            .all(|r| !r.url.contains("accountPositions"))
    );
    no_numeric_terminal(&actions);
}
#[test]
fn currency_response_at_cycle_deadline_cannot_publish_without_timer() {
    let (mut e, actions, _) = started();
    response(
        &mut e,
        NOW + 20_000.,
        request(&actions, "currency=USDT").id,
        200,
        &envelope("USDT", 999., 0., BTC),
    );
    assert_eq!(
        e.store().balance().total,
        0.,
        "immutable Background bound fences financial commit at equality"
    );
    assert!(e.store().positions().is_empty());
}
#[test]
fn uncertain_order_read_requires_primary_background_managed_preparation() {
    let (_, actions, _) = uncertain_with(false);
    assert_eq!(prepares(&actions, "/g-orders/activeList"), 1);
    assert!(
        requests(&actions)
            .iter()
            .all(|r| !r.url.contains("activeList"))
    );
    no_numeric_terminal(&actions);
}
#[test]
fn reconciliation_response_at_deadline_cannot_reveal_an_order_without_timer() {
    let (mut e, actions, wake) = uncertain();
    let read = request(&actions, "activeList");
    response(&mut e, wake + 20_000., read.id, 200, revealed());
    assert!(
        e.store().order("revealed").is_none(),
        "expired read cannot publish financial state"
    );
}
#[test]
fn native_identity_secondary_exact_uid_and_no_account_publication() {
    let (mut e, actions) = identity(true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&actions, "currency=USDT");
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.route, None);
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "X-PHEMEX-ACCESS-TOKEN" && v == "read-key")
    );
    response(
        &mut e,
        NOW + 1.,
        r.id,
        200,
        r#"{"code":0,"data":{"account":{"userID":9007199254740993}}}"#,
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert_eq!(out.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command:42, result }) if result == r#"{"userId":"9007199254740993"}"#)).count(), 1);
    assert_eq!(e.account_snapshot_frame(NOW), before);
    response(
        &mut e,
        NOW + 2.,
        r.id,
        200,
        r#"{"code":0,"data":{"account":{"userID":"duplicate"}}}"#,
    );
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn native_identity_primary_fallback_and_invalid_response_terminal_once() {
    let (mut e, actions) = identity(false);
    let r = request(&actions, "currency=USDT");
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "X-PHEMEX-ACCESS-TOKEN" && v == "primary-key")
    );
    response(
        &mut e,
        NOW + 1.,
        r.id,
        200,
        r#"{"code":0,"data":{"account":{"userID":false}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
            .count(),
        1
    );
    assert!(requests(&actions).is_empty());
}
#[test]
fn native_currency_partial_success_and_both_arrival_orders_publish_exact_financials() {
    for reverse in [false, true] {
        let (mut e, actions, _) = started();
        let mut legs = [("USDT", 1000., 69., BTC), ("USDC", 200., 19., ETH)];
        if reverse {
            legs.reverse();
        }
        for (i, (currency, total, used, row)) in legs.into_iter().enumerate() {
            response(
                &mut e,
                NOW + 1. + i as f64,
                request(&actions, &format!("currency={currency}")).id,
                200,
                &envelope(currency, total, used, row),
            );
            if i == 0 {
                assert_eq!(e.store().positions().len(), 1);
            }
        }
        assert_eq!(e.store().positions().len(), 2);
        assert_eq!(e.store().balance().total, 1090.);
        assert_eq!(e.store().balance().used, 88.);
        no_numeric_terminal(&e.drain_actions().collect::<Vec<_>>());
    }
}
#[test]
fn native_cycle_timer_revokes_late_currency_and_retains_successful_slice() {
    let (mut e, actions, _) = started();
    response(
        &mut e,
        NOW + 1.,
        request(&actions, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., BTC),
    );
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    response(
        &mut e,
        NOW + 20_001.,
        request(&actions, "currency=USDC").id,
        200,
        &envelope("USDC", 999., 0., ETH),
    );
    assert_eq!(e.store().balance().total, 990.);
    assert_eq!(e.store().positions().len(), 1);
    let actions: Vec<_> = e.drain_actions().collect();
    let refresh = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 22_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 22_000., Event::TimerFired { timer: refresh });
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 22_000.);
    assert_eq!(
        requests(&actions)
            .iter()
            .filter(|r| r.url.contains("accountPositions"))
            .count(),
        2
    );
}
#[test]
fn native_uncertain_read_is_primary_and_can_reveal_a_live_order() {
    let (mut e, actions, wake) = uncertain_with(false);
    assert!(actions.iter().any(|action| matches!(action,Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare { intent:safe_cex_engine::request_context::RoutingIntent::Primary, request,.. } if request.url.contains("activeList")))));
    let actions = release(&mut e, wake, actions);
    let r = request(&actions, "activeList");
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == "key")
    );
    assert_eq!(r.method, Method::Get);
    assert!(r.url.ends_with("symbol=BTCUSDT"));
    response(&mut e, wake + 1., r.id, 200, revealed());
    assert!(e.store().order("revealed").is_some());
    no_numeric_terminal(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn native_empty_reconciliation_and_network_failure_do_not_invent_orders() {
    for network in [false, true] {
        let (mut e, actions, wake) = uncertain();
        let r = request(&actions, "activeList");
        if network {
            e.handle(
                wake + 1.,
                Event::HttpFailed {
                    request: r.id,
                    error: "offline",
                },
            );
        } else {
            response(
                &mut e,
                wake + 1.,
                r.id,
                200,
                r#"{"code":10002,"data":null}"#,
            );
        }
        assert!(e.store().orders().is_empty());
        let actions: Vec<_> = e.drain_actions().collect();
        no_numeric_terminal(&actions);
        if network {
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms>wake+1.))
            );
        }
    }
}

#[test]
fn retired_currency_429_cannot_pause_the_next_native_cycle() {
    let (mut e, actions, _) = started();
    let expired = request(&actions, "currency=USDT");
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let refresh = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 22_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    response(&mut e, NOW + 20_001., expired.id, 429, "rate limited");
    e.drain_actions().for_each(drop);
    e.handle(NOW + 22_000., Event::TimerFired { timer: refresh });
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 22_000.);
    assert_eq!(
        requests(&actions)
            .iter()
            .filter(|r| r.url.contains("accountPositions"))
            .count()
            + prepares(&actions, "accountPositions"),
        2,
        "retired delivery cannot mutate the global pause before its native cycle check"
    );
}
#[test]
fn streamed_currency_started_live_but_ended_at_deadline_cannot_publish() {
    let (mut e, actions, _) = started();
    let r = request(&actions, "currency=USDT");
    let body = envelope("USDT", 999., 0., BTC);
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: r.id,
            data: body.as_bytes(),
        },
    );
    e.handle(NOW + 20_000., Event::HttpEnd { request: r.id });
    assert_eq!(
        e.store().balance().total,
        0.,
        "staged live rows do not authorize an expired final commit"
    );
    assert!(e.store().positions().is_empty());
}
#[test]
fn native_currency_reads_select_secondary_without_forcing_primary_route() {
    let (_, actions, _) = started();
    let reads: Vec<_> = requests(&actions)
        .into_iter()
        .filter(|r| r.url.contains("accountPositions"))
        .collect();
    assert_eq!(reads.len(), 2);
    for r in reads {
        assert_eq!(r.route, None);
        assert!(
            r.headers
                .iter()
                .any(|(name, value)| *name == "X-PHEMEX-ACCESS-TOKEN" && value == "read-key")
        );
    }
}
#[test]
fn native_identity_local_refusal_is_once_and_creates_no_transport() {
    for public in [false, true] {
        let mut e = support::engine(Options {
            key: public.then(|| "key".into()),
            secret: public.then(|| "secret".into()),
            public_data_only: public,
            ..Default::default()
        });
        e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
                .count(),
            1
        );
        assert!(actions.iter().all(|a| !matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
        )));
        assert!(e.store().positions().is_empty());
    }
}

#[test]
fn identity_leg_expiry_has_one_typed_terminal_instead_of_silent_drop() {
    let (mut e, actions) = identity(false);
    let read = request(&actions, "currency=USDT");
    let body = r#"{"code":0,"data":{"account":{"userID":"expired"}}}"#;
    response(&mut e, NOW + 20_000., read.id, 200, body);
    let actions: Vec<_> = e.drain_actions().collect();
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 42, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "expired identity owns one typed terminal, not a silent DATA rejection"
    );
    assert_eq!(failures[0].kind, Some(safe_cex_engine::ErrorKind::Timeout));
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 42, .. })
    )));
    response(&mut e, NOW + 20_001., read.id, 200, body);
    assert!(
        e.drain_actions().next().is_none(),
        "duplicate expiry delivery is inert"
    );
}

#[test]
fn reconciliation_expiry_releases_reading_and_native_backoff_issues_fresh_primary_read() {
    let (mut e, actions, wake) = uncertain();
    let old = request(&actions, "activeList");
    let expired_at = wake + 20_000.;
    response(
        &mut e,
        expired_at,
        old.id,
        200,
        r#"{"code":10002,"data":null}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let retry = actions.iter().find_map(|action| match action {
        Action::SetTimer { timer, at_ms } if *at_ms > expired_at => Some((*timer, *at_ms)),
        _ => None,
    });
    assert!(
        retry.is_some(),
        "physical expiry must release native reading and retain the owed reconciliation through backoff"
    );
    let (timer, at) = retry.unwrap();
    e.handle(at, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        requests(&actions)
            .iter()
            .filter(|r| r.url.contains("activeList"))
            .count()
            + prepares(&actions, "activeList"),
        1,
        "native retry must issue one fresh physical read"
    );
    if let Some(fresh) = requests(&actions)
        .iter()
        .find(|r| r.url.contains("activeList"))
    {
        assert_ne!(fresh.id, old.id);
        assert_eq!(fresh.route, Some(RequestRoute::Primary));
    }
    for action in &actions {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare {
                intent, request, ..
            } = action.as_ref()
            && request.url.contains("activeList")
        {
            assert_eq!(
                *intent,
                safe_cex_engine::request_context::RoutingIntent::Primary
            );
        }
    }
    no_numeric_terminal(&actions);
}

#[test]
fn native_both_currency_failures_release_pending_and_schedule_one_poll() {
    for reverse in [false, true] {
        let (mut e, actions, _) = started();
        let mut reads = [
            request(&actions, "currency=USDT"),
            request(&actions, "currency=USDC"),
        ];
        if reverse {
            reads.reverse();
        }
        e.handle(
            NOW + 1.,
            Event::HttpFailed {
                request: reads[0].id,
                error: "offline",
            },
        );
        let first: Vec<_> = e.drain_actions().collect();
        assert!(
            !first.iter().any(|a| matches!(a, Action::SetTimer { .. })),
            "remaining sibling still owns the aggregate cycle"
        );
        e.handle(
            NOW + 2.,
            Event::HttpFailed {
                request: reads[1].id,
                error: "offline",
            },
        );
        let last: Vec<_> = e.drain_actions().collect();
        let polls: Vec<_> = last
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(
            polls.len(),
            1,
            "both failures release aggregate custody once"
        );
        assert!(e.store().positions().is_empty());
        assert_eq!(e.store().balance().total, 0.);
        e.handle(
            NOW + 3.,
            Event::HttpFailed {
                request: reads[0].id,
                error: "duplicate",
            },
        );
        assert!(e.drain_actions().next().is_none());
        let (timer, at) = polls[0];
        e.handle(at, Event::TimerFired { timer });
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, at);
        assert_eq!(
            requests(&actions)
                .iter()
                .filter(|r| r.url.contains("accountPositions"))
                .count(),
            2
        );
        no_numeric_terminal(&actions);
    }
}

#[path = "support/read_owner_failures.rs"]
mod failures;
