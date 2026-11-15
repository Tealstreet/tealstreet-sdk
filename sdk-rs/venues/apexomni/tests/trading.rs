mod support;
use safe_cex_crypto::signer::{
    LocalExtra, LocalExtras, LocalIdentity, LocalSetup, LocalSigner, Secret,
};
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy,
    Identities, IdentitySetupPolicy, KeyEncoding, KeyRef, SignError, SignFuture, SignRequest,
    SignResult, Signer,
};
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Notice, Response, SocketChannel, SocketId,
};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use std::collections::HashMap;
use std::sync::Arc;

use support::*;

const NOW: f64 = 1_700_000_000_123.0;
struct TestEngine {
    inner: support::TestEngine,
    now: f64,
}
impl std::ops::Deref for TestEngine {
    type Target = support::TestEngine;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl std::ops::DerefMut for TestEngine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl TestEngine {
    /// Managed HTTP released at the latest event time; legacy actions pass through.
    fn drain_actions(&mut self) -> std::vec::IntoIter<Action> {
        safe_cex_engine::test_support::prepared_actions(&mut self.inner, self.now).into_iter()
    }
    fn handle(&mut self, now: f64, event: Event<'_>) {
        self.now = now;
        self.inner.handle(now, event);
    }
}
fn actions(e: &mut TestEngine) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(&mut e.inner, e.now)
}
fn requests(e: &mut TestEngine) -> Vec<HttpRequest> {
    http(actions(e))
}
fn response(e: &mut TestEngine, request: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn response_status(e: &mut TestEngine, request: &HttpRequest, now: f64, status: u16) {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body: "",
            },
        },
    );
}
fn ws(e: &mut TestEngine, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}

fn ready() -> TestEngine {
    ready_with_socket().0
}
fn ready_with_socket() -> (TestEngine, SocketId) {
    ready_with_provider(false)
}
fn ready_with_provider(hmac_only: bool) -> (TestEngine, SocketId) {
    ready_with_provider_mode(hmac_only, false)
}
fn ready_with_provider_mode(hmac_only: bool, remote: bool) -> (TestEngine, SocketId) {
    ready_with_signer(hmac_only, |provider| {
        if remote {
            Arc::new(DelegatedHmac(provider))
        } else {
            provider
        }
    })
}
fn ready_with_signer(
    hmac_only: bool,
    wrap: impl FnOnce(Arc<dyn Signer>) -> Arc<dyn Signer>,
) -> (TestEngine, SocketId) {
    let (provider, identities) = write_provider(hmac_only);
    let provider = wrap(provider);
    let inner = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..Options::default()
    }));
    let mut e = TestEngine { inner, now: NOW };
    e.set_identities(identities).unwrap();
    e.set_signer(Some(provider)).unwrap();
    e.start(NOW);
    let symbols = requests(&mut e).remove(0);
    response(&mut e, &symbols, NOW + 1.0, MARKETS);
    let mut socket = None;
    let reads = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            Action::WsConnect {
                socket: id,
                channel: Some(SocketChannel::Private),
                ..
            } => {
                socket = Some(id);
                None
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    e.handle(
        NOW + 1.0,
        Event::WsOpened {
            socket: socket.unwrap(),
        },
    );
    e.drain_actions().for_each(drop);
    for request in reads {
        response(
            &mut e,
            &request,
            NOW + 2.0,
            if request.url.ends_with("/v3/account") {
                r#"{"code":0,"data":{"id":"611779989544632629","positions":[],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#
            } else {
                r#"{"code":0,"data":[]}"#
            },
        );
    }
    for request in requests(&mut e) {
        response(
            &mut e,
            &request,
            NOW + 3.0,
            r#"{"code":0,"data":{"totalEquityValue":"1000","initialMargin":"0","maintenanceMargin":"0"}}"#,
        );
    }
    e.store_mut().update_ticker("BTCUSDT", |t| {
        t.last = 60000.0;
        t.mark = 60000.0;
        t.index = 60000.0;
    });
    e.drain_actions().for_each(drop);
    (e, socket.unwrap())
}
fn order(extra: serde_json::Value) -> PlaceOrderOpts {
    let mut base = serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":60000});
    for (key, value) in extra.as_object().unwrap() {
        base[key] = value.clone();
    }
    serde_json::from_value(base).unwrap()
}
fn place(e: &mut TestEngine, orders: Vec<PlaceOrderOpts>) -> Vec<Action> {
    place_with_id(e, 42, orders)
}
fn place_with_id(
    e: &mut TestEngine,
    command: safe_cex_engine::CommandId,
    orders: Vec<PlaceOrderOpts>,
) -> Vec<Action> {
    e.handle(
        NOW + 10.0,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
    actions(e)
}
/// Requests other than the engine-owned reconcile read an Unknown write starts
/// at once (the uncertain-placement fence opt-in); its own control pins that read.
fn without_reconcile_read(requests: Vec<HttpRequest>) -> Vec<HttpRequest> {
    requests
        .into_iter()
        .filter(|r| !r.url.ends_with("/v3/open-orders"))
        .collect()
}
fn http(actions: Vec<Action>) -> Vec<HttpRequest> {
    actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}
fn form(request: &HttpRequest) -> HashMap<String, String> {
    request
        .body
        .as_ref()
        .unwrap()
        .split('&')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (decode(k), decode(v))
        })
        .collect()
}

#[test]
fn caller_id_is_verbatim_in_both_wire_fields_and_changes_signature() {
    let mut e = ready();
    let r = http(place(
        &mut e,
        vec![order(serde_json::json!({"clientOrderId":"caller-01/+é"}))],
    ))
    .remove(0);
    let p = form(&r);
    assert_eq!(p["clientId"], "caller-01/+é");
    assert_eq!(p["clientOrderId"], p["clientId"]);
    assert_eq!(p["symbol"], "BTC-USDT");
    assert_eq!(p["brokerId"], "6587");
    assert_eq!(p["size"], "0.001");
    assert_eq!(p["limitFee"], "0.030000");
    assert_eq!(p["signature"].len(), 128);
    assert!(!p.contains_key("reduceOnly"));
}

#[test]
fn batch_caller_id_preflight_rejects_before_any_first_write() {
    for bad in [
        serde_json::json!({"clientOrderId":""}),
        serde_json::json!({"clientOrderId":"has space"}),
        serde_json::json!({"clientOrderId":"same","stopLoss":58000}),
        serde_json::json!({"clientOrderId":"same","amount":1001}),
        serde_json::json!({"clientOrderId":"same","type":"stop_market","triggerPrice":58000}),
    ] {
        let mut e = ready();
        let actions = place(&mut e, vec![order(serde_json::json!({})), order(bad)]);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
        );
    }
    let mut e = ready();
    let actions = place(
        &mut e,
        vec![
            order(serde_json::json!({"clientOrderId":"same"})),
            order(serde_json::json!({"clientOrderId":"same"})),
        ],
    );
    assert!(http(actions).is_empty());
}

#[test]
fn bybit_only_controls_reject_single_and_whole_batch_without_io_or_state_changes() {
    for extra in [
        serde_json::json!({"bybitBbo":{"sideType":"Counterparty","level":1}}),
        serde_json::json!({"bybitSlippageToleranceTicks":0}),
    ] {
        for batch in [false, true] {
            let mut e = ready();
            let mut orders = vec![];
            if batch {
                orders.push(order(
                    serde_json::json!({"clientOrderId":"first-must-not-place"}),
                ));
            }
            orders.push(order(extra.clone()));
            let actions = place(&mut e, orders);
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_) | Action::WsSend { .. }))
            );
            assert!(actions.iter().any(|action| matches!(action,
                Action::Notice(Notice::CommandFailed { command: 42, error })
                    if error.message.contains("Bybit BBO and TickSize controls are not supported by ApeX Omni")
            )));
            assert!(e.store().orders().is_empty());
            // A rejected preflight must leave the sequenced trading queue usable.
            assert_eq!(
                http(place(&mut e, vec![order(serde_json::json!({}))])).len(),
                1
            );
        }
    }
}

#[test]
fn split_bracket_and_unsupported_shapes_fail_before_writes() {
    for extra in [
        serde_json::json!({"amount":1001,"takeProfit":65000}),
        serde_json::json!({"type":"conditional","triggerPrice":59000}),
        serde_json::json!({"type":"trailing_stop_market","price":100}),
        serde_json::json!({"amount":0.0001}),
        serde_json::json!({"price":0}),
    ] {
        let mut e = ready();
        let actions = place(&mut e, vec![order(extra)]);
        assert!(http(actions).is_empty());
    }
}

#[test]
fn standalone_trigger_rounds_before_slippage() {
    let mut e = ready();
    let r=http(place(&mut e,vec![order(serde_json::json!({"type":"stop_market","triggerPrice":100.0498,"slTriggerType":"last_price"}))])).remove(0);
    let p = form(&r);
    assert_eq!(p["triggerPrice"], "100");
    assert_eq!(p["price"], "100.5");
    assert_eq!(p["type"], "STOP_LIMIT");
    assert_eq!(p["triggerPriceType"], "MARKET");
}

#[test]
fn bracket_legs_reverse_side_preserve_exact_size_and_sign_independently() {
    let mut e = ready();
    let r = http(place(
        &mut e,
        vec![order(
            serde_json::json!({"amount":0.1,"price":53000,"stopLoss":50000,"takeProfit":65000}),
        )],
    ))
    .remove(0);
    let p = form(&r);
    assert_eq!(p["limitFee"], "2.650000");
    assert_eq!(p["isOpenTpslOrder"], "true");
    for prefix in ["tp", "sl"] {
        assert_eq!(p[&format!("{prefix}Size")], p["size"]);
        assert_eq!(p[&format!("{prefix}Side")], "SELL");
        assert_ne!(p[&format!("{prefix}ClientOrderId")], p["clientOrderId"]);
        assert_ne!(p[&format!("{prefix}Signature")], p["signature"]);
        assert_eq!(p[&format!("{prefix}Expiration")], p["expiration"]);
    }
    assert_eq!(p["tpPrice"], "64675");
    assert_eq!(p["slPrice"], "49750");
    assert_eq!(p["tpTriggerPriceType"], "MARKET");
    assert_eq!(p["slTriggerPriceType"], "INDEX");
}

#[test]
fn market_execution_price_uses_current_mark_and_default_tif() {
    let mut e = ready();
    let r = http(place(
        &mut e,
        vec![order(serde_json::json!({"type":"market","side":"sell"}))],
    ))
    .remove(0);
    let p = form(&r);
    assert_eq!(p["price"], "58800");
    assert_eq!(p["timeInForce"], "GOOD_TIL_CANCEL");
}

#[test]
fn network_failure_does_not_retry_an_order_and_releases_fifo() {
    let mut e = ready();
    let first = http(place(
        &mut e,
        vec![order(serde_json::json!({"clientOrderId":"first"}))],
    ))
    .remove(0);
    assert!(
        http(place_with_id(
            &mut e,
            43,
            vec![order(serde_json::json!({"clientOrderId":"second"}))]
        ))
        .is_empty()
    );
    e.handle(
        NOW + 11.0,
        Event::HttpFailed {
            request: first.id,
            error: "connection reset after write",
        },
    );
    let actions = actions(&mut e);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    let requests = http(actions);
    // The Unknown write's reconcile read starts at once, beside the next FIFO write.
    assert!(requests.iter().any(|r| r.url.ends_with("/v3/open-orders")));
    let second = without_reconcile_read(requests).remove(0);
    assert_eq!(form(&second)["clientOrderId"], "second");
}

#[test]
fn signature_rejection_retries_once_with_same_l2_body_and_fresh_legacy_auth() {
    let mut e = ready();
    let first = http(place(
        &mut e,
        vec![order(serde_json::json!({"clientOrderId":"legacy"}))],
    ))
    .remove(0);
    response(
        &mut e,
        &first,
        NOW + 20.0,
        r#"{"code":20016,"msg":"signature rejected"}"#,
    );
    let retry = requests(&mut e).remove(0);
    assert_eq!(first.body, retry.body);
    assert_ne!(first.headers, retry.headers);
    response(
        &mut e,
        &retry,
        NOW + 30.0,
        r#"{"code":20016,"msg":"signature rejected"}"#,
    );
    assert!(requests(&mut e).is_empty());
}

/// Real ApeX bodies (a user's key, 2026-11-02): no `code`; a cancel's `data` is the bare id.
const REAL_PLACE: &str = r#"{"data":{"id":"904952998325649712","clientId":"c1","orderId":"904952998325649712","status":"PENDING"},"timeCost":21}"#;
const REAL_CANCEL: &str = r#"{"data":"904952998325649712","timeCost":5897}"#;

fn command_failures(out: &[Action]) -> Vec<String> {
    out.iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.message.clone()),
            _ => None,
        })
        .collect()
}

/// ApeX's documented success has no `code`: `{"data":{...},"timeCost":3}`.
#[test]
fn a_success_with_no_code_is_a_placed_order_not_a_failure() {
    let mut e = ready();
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    response(&mut e, &first, NOW + 11.0, REAL_PLACE);
    let out = actions(&mut e);
    assert_eq!(command_failures(&out), Vec::<String>::new());
}

#[test]
fn a_cancel_answered_with_no_code_succeeds() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["abc".into()],
        }),
    );
    let cancel = requests(&mut e).remove(0);
    response(&mut e, &cancel, NOW + 11.0, REAL_CANCEL);
    assert_eq!(command_failures(&actions(&mut e)), Vec::<String>::new());
}

#[test]
fn an_amend_whose_cancel_has_no_code_still_places_the_replacement() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    response(
        &mut e,
        &cancel,
        NOW + 15.0,
        r#"{"data":"parent","timeCost":3}"#,
    );
    prove_cancel(&mut e);
    let replacement = requests(&mut e).remove(0);
    assert_eq!(form(&replacement)["size"], "4");
}

fn done(out: &[Action], command: safe_cex_engine::CommandId) -> bool {
    out.iter().any(|action| {
        matches!(action, Action::Notice(Notice::CommandDone { command: c, .. }) if *c == command)
    })
}

/// Live replies on a user's key (2026-11-02): these writes answer only `timeCost`, or nothing.
#[test]
fn a_cancel_all_answered_with_only_time_cost_is_done() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 5,
            symbol: "BTCUSDT".into(),
        }),
    );
    let request = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/v3/delete-open-orders"))
        .expect("cancel-all write");
    response(&mut e, &request, NOW + 11.0, r#"{"timeCost":6476}"#);
    let out = actions(&mut e);
    assert_eq!(command_failures(&out), Vec::<String>::new());
    assert!(done(&out, 5), "{out:?}");
}

#[test]
fn a_leverage_change_answered_with_only_time_cost_is_done() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::SetLeverage {
            command: 9,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
            side: None,
        }),
    );
    let request = requests(&mut e).remove(0);
    response(&mut e, &request, NOW + 11.0, r#"{"timeCost":5694}"#);
    let out = actions(&mut e);
    assert_eq!(command_failures(&out), Vec::<String>::new());
    assert!(done(&out, 9), "{out:?}");
}

/// Legacy cancels an attached TP/SL by its parent's id; the venue answers an empty body.
#[test]
fn a_tpsl_cancel_answered_with_an_empty_body_is_done() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["abc__stop_loss".into()],
        }),
    );
    let request = requests(&mut e).remove(0);
    assert_eq!(form(&request)["id"], "abc");
    response(&mut e, &request, NOW + 11.0, "");
    let out = actions(&mut e);
    assert_eq!(command_failures(&out), Vec::<String>::new());
    assert!(done(&out, 7), "{out:?}");
}

#[test]
fn a_failure_never_shows_empty_text() {
    for body in [
        r#"{"timeCost":3}"#,
        r#"{"code":10003,"msg":""}"#,
        r#"{"data":null}"#,
    ] {
        let mut e = ready();
        let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
        response(&mut e, &first, NOW + 11.0, body);
        let failures = command_failures(&actions(&mut e));
        assert_eq!(failures.len(), 1, "{body}");
        assert!(!failures[0].trim_end().ends_with(':'), "{}", failures[0]);
        if body.contains("10003") {
            assert!(failures[0].contains("no reason given"), "{}", failures[0]);
        }
    }
}

#[test]
fn http_and_business_rejections_never_cancel_replace_or_retry_writes() {
    for (status, body) in [
        (500, r#"{"msg":"server error"}"#),
        (200, r#"{"code":10003,"msg":"insufficient funds"}"#),
        (200, r#"{"code":0,"data":{}}"#),
        (200, r#"{"data":{}}"#),
        (200, r#"{"timeCost":3}"#),
        (200, r#"{"code":"broken","data":{"id":"must-not-accept"}}"#),
        (200, r#"{"code":{},"data":{"id":"must-not-accept"}}"#),
        (200, r#"{"code":0.5,"data":{"id":"must-not-accept"}}"#),
    ] {
        let mut e = ready();
        let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
        e.handle(
            NOW + 11.0,
            Event::HttpResponse {
                request: first.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        assert!(without_reconcile_read(requests(&mut e)).is_empty());
    }
}

#[test]
fn virtual_cancellation_deduplicates_parent_and_never_retries_ambiguous_write() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec![
                "abc__stop_loss".into(),
                "abc__take_profit".into(),
                "abc".into(),
            ],
        }),
    );
    let first = requests(&mut e).remove(0);
    assert_eq!(form(&first)["id"], "abc");
    e.handle(
        NOW + 20.0,
        Event::HttpFailed {
            request: first.id,
            error: "reset",
        },
    );
    assert!(without_reconcile_read(requests(&mut e)).is_empty());
}

#[test]
fn leverage_uses_real_multipart_and_clamps_to_market_rails() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::SetLeverage {
            command: 9,
            symbol: "BTCUSDT".into(),
            leverage: 500.0,
            side: None,
        }),
    );
    let request = requests(&mut e).remove(0);
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| *k == "Content-Type" && v.starts_with("multipart/form-data; boundary="))
    );
    let body = request.body.unwrap();
    assert!(body.contains("\r\n\r\n0.01000\r\n"));
    assert!(body.contains("\r\n\r\nBTC-USDT\r\n"));
}

fn parent_update(
    e: &mut TestEngine,
    socket: SocketId,
    at: f64,
    filled: f64,
    status: &str,
    snapshot: bool,
) {
    ws(
        e,
        socket,
        at,
        &format!(
            r#"{{"topic":"ws_zk_accounts_v3","type":"{}","contents":{{"orders":[{{"id":"parent","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"{status}","size":"10","price":"60000","cumSuccessFillSize":"{filled}","updatedAt":{at},"openSlParam":{{"triggerPrice":"50000","triggerPriceType":"INDEX"}},"openTpParams":{{"triggerPrice":"65000","triggerPriceType":"MARKET"}}}}]}}}}"#,
            if snapshot { "snapshot" } else { "delta" },
        ),
    );
}

/// The pause after an amend's cancel ends, and its proof read answers
/// `status`; a zero fill leaves the amend's own fill accounting unchanged.
fn prove_cancel_as(e: &mut TestEngine, status: &str) {
    prove_cancel_of(e, "parent", status);
}
fn prove_cancel_of(e: &mut TestEngine, id: &str, status: &str) {
    let (timer, at) = actions(e)
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("a cancel-proof pause");
    e.handle(at, Event::TimerFired { timer });
    let read = requests(e)
        .into_iter()
        .find(|request| request.url.contains("/v3/order?id="))
        .expect("a cancel-proof read");
    response(
        e,
        &read,
        at,
        &format!(r#"{{"data":{{"id":"{id}","status":"{status}","cumSuccessFillSize":"0"}}}}"#),
    );
}
fn prove_cancel(e: &mut TestEngine) {
    prove_cancel_as(e, "CANCELED");
}

/// A take profit moves up to 66000; anything else to 59000.
fn amend(e: &mut TestEngine, id: &str, amount: Option<f64>) -> Vec<HttpRequest> {
    e.handle(
        NOW + 10.0,
        Event::Command(Command::AmendOrder {
            command: 99,
            id: id.into(),
            price: Some(if id.ends_with("__take_profit") {
                66000.0
            } else {
                59000.0
            }),
            amount,
        }),
    );
    requests(e)
}

fn amend_fill(e: &mut TestEngine, socket: SocketId, created_at: f64, size: f64) {
    ws(
        e,
        socket,
        NOW + 12.0,
        &format!(
            r#"{{"topic":"ws_zk_accounts_v3","type":"delta","contents":{{"fills":[{{"id":"new-fill","orderId":"parent","symbol":"BTC-USDT","side":"BUY","size":"{size}","price":"60000","fee":"0","createdAt":{created_at}}}]}}}}"#,
        ),
    );
}

#[test]
fn partial_parent_and_virtual_amendments_replace_only_remaining_and_keep_siblings() {
    for id in ["parent", "parent__take_profit"] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, id, None).remove(0);
        response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
        prove_cancel(&mut e);
        let replacement = requests(&mut e).remove(0);
        let fields = form(&replacement);
        assert_eq!(fields["size"], "4");
        assert_eq!(fields["slSize"], "4");
        assert_eq!(fields["tpSize"], "4");
        assert_eq!(fields["slTriggerPrice"], "50000");
        assert_eq!(
            fields["tpTriggerPrice"],
            if id == "parent" { "65000" } else { "66000" }
        );
    }
}

#[test]
fn amend_rechecks_remaining_when_its_fifo_turn_starts() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    assert!(amend(&mut e, "parent", None).is_empty());
    parent_update(&mut e, socket, NOW + 11.0, 8.0, "OPEN", false);
    e.handle(
        NOW + 12.0,
        Event::HttpFailed {
            request: first.id,
            error: "lost placement response",
        },
    );
    let cancel = without_reconcile_read(requests(&mut e)).remove(0);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    assert_eq!(
        form(&without_reconcile_read(requests(&mut e)).remove(0))["size"],
        "2"
    );
}

#[test]
fn in_flight_amend_counts_same_execution_once_in_both_topic_orders() {
    for fill_first in [true, false] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, "parent", None).remove(0);
        if fill_first {
            amend_fill(&mut e, socket, NOW + 11.0, 1.0);
        }
        parent_update(&mut e, socket, NOW + 12.0, 7.0, "OPEN", false);
        if !fill_first {
            amend_fill(&mut e, socket, NOW + 11.0, 1.0);
        }
        response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
        prove_cancel(&mut e);
        assert_eq!(form(&requests(&mut e).remove(0))["size"], "3");
    }
}

#[test]
fn delayed_pre_amend_fill_does_not_reduce_the_captured_remaining_again() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    amend_fill(&mut e, socket, NOW + 4.0, 6.0);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "4");
}

#[test]
fn new_genuine_amend_fill_counts_despite_outstanding_old_synthetic_coverage() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    parent_update(&mut e, socket, NOW + 6.0, 7.0, "OPEN", false);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    amend_fill(&mut e, socket, NOW + 11.0, 1.0);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "2");
}

#[test]
fn distinct_execution_after_a_cumulative_report_reduces_remaining_again() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    parent_update(&mut e, socket, NOW + 11.0, 7.0, "OPEN", false);
    amend_fill(&mut e, socket, NOW + 12.0, 1.0);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "2");
}

#[test]
fn genuine_full_fill_during_cancel_suppresses_explicit_replacement_quantity() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", Some(8.5)).remove(0);
    amend_fill(&mut e, socket, NOW + 11.0, 4.0);
    e.drain_actions().for_each(drop);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    let actions = actions(&mut e);
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 99, result: ids }) if ids == "[]")));
    assert!(http(actions).is_empty());
}

#[test]
fn fully_filled_parent_while_amend_is_queued_never_sends_cancellation() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    assert!(amend(&mut e, "parent", None).is_empty());
    parent_update(&mut e, socket, NOW + 11.0, 10.0, "FILLED", false);
    e.handle(
        NOW + 12.0,
        Event::HttpFailed {
            request: first.id,
            error: "lost placement response",
        },
    );
    let actions = actions(&mut e);
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { command: 99, .. })
    )));
    assert!(without_reconcile_read(http(actions)).is_empty());
}

#[test]
fn unchanged_or_invalid_amendment_rejects_before_cancellation() {
    for price in [None, Some(f64::NAN), Some(f64::INFINITY), Some(0.0)] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        e.handle(
            NOW + 10.0,
            Event::Command(Command::AmendOrder {
                command: 99,
                id: "parent".into(),
                price,
                amount: None,
            }),
        );
        let actions = actions(&mut e);
        assert!(actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 99, .. })
        )));
        assert!(http(actions).is_empty());
        assert_eq!(e.store().order("parent").unwrap().remaining, 4.0);
    }
}

#[test]
fn own_cancel_event_keeps_replacement_but_full_fill_suppresses_it() {
    for (status, filled) in [("CANCELED", 8.0), ("FILLED", 10.0)] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, "parent", None).remove(0);
        parent_update(&mut e, socket, NOW + 12.0, filled, status, false);
        e.drain_actions().for_each(drop);
        // The order's own update is the cancel's proof: no read is needed.
        response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
        let actions = actions(&mut e);
        if status == "FILLED" {
            assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 99, result: ids }) if ids == "[]")));
            assert!(http(actions).is_empty());
        } else {
            assert_eq!(form(&http(actions).remove(0))["size"], "2");
        }
    }
}

#[test]
fn an_explicit_total_comes_down_by_an_additional_fill() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", Some(8.5)).remove(0);
    parent_update(&mut e, socket, NOW + 12.0, 7.0, "OPEN", false);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "1.5");
}

#[test]
fn unsupported_stp_rejects_single_and_mixed_batches_before_io_and_keeps_queue_usable() {
    for mode in ["EXPIRE_TAKER", "EXPIRE_MAKER", "EXPIRE_BOTH"] {
        for mixed in [false, true] {
            let mut engine = ready();
            let before_balance = engine.store().balance().clone();
            let mut orders = Vec::new();
            if mixed {
                orders.push(order(serde_json::json!({})));
            }
            orders.push(order(serde_json::json!({"stpMode": mode})));
            let actions = place(&mut engine, orders);
            assert!(
                actions
                    .iter()
                    .all(|action| matches!(action, Action::Notice(_)))
            );
            assert!(actions.iter().any(|action| matches!(action,
                Action::Notice(Notice::CommandFailed { command: 42, error })
                if error.message.contains("self-trade prevention"))));
            assert!(engine.store().orders().is_empty());
            assert!(engine.store().positions().is_empty());
            assert_eq!(engine.store().balance(), &before_balance);
            assert_eq!(
                http(place(&mut engine, vec![order(serde_json::json!({}))])).len(),
                1
            );
        }
    }
}

#[test]
fn parent_and_embedded_leg_receipts_precede_signed_request() {
    for caller in [None, Some("actual-caller")] {
        let mut e = ready();
        let mut opts = order(
            serde_json::json!({"amount":0.1,"price":53000,"stopLoss":50000,"takeProfit":65000}),
        );
        // Caller IDs with brackets are rejected by contract; generated brackets cover children.
        if let Some(caller) = caller {
            opts.client_order_id = Some(caller.into());
            opts.stop_loss = None;
            opts.take_profit = None;
        }
        let out = place(&mut e, vec![opts]);
        let write = out
            .iter()
            .position(|a| matches!(a, Action::Http(_)))
            .unwrap();
        let receipts: Vec<_> = out[..write]
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandProgress {
                    command: 42,
                    receipts,
                }) => Some(receipts),
                _ => None,
            })
            .flatten()
            .collect();
        let Action::Http(request) = &out[write] else {
            panic!()
        };
        let p = form(request);
        let actual = if caller.is_some() {
            vec![p["clientOrderId"].as_str()]
        } else {
            vec![
                p["clientOrderId"].as_str(),
                p["tpClientOrderId"].as_str(),
                p["slClientOrderId"].as_str(),
            ]
        };
        assert_eq!(
            receipts
                .iter()
                .map(|r| r.identity.as_str())
                .collect::<Vec<_>>(),
            actual
        );
        assert!(receipts.iter().all(|r| r.symbol == "BTCUSDT"));
    }
}

#[test]
fn amendment_receipts_are_published_only_for_final_selected_replacement() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    e.handle(
        NOW + 10.0,
        Event::Command(Command::AmendOrder {
            command: 99,
            id: "parent".into(),
            price: Some(59000.0),
            amount: None,
        }),
    );
    let cancel_actions: Vec<_> = actions(&mut e);
    assert!(
        !cancel_actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandProgress { .. })))
    );
    let cancel = http(cancel_actions).remove(0);
    // An additional fill discards the prebuilt replacement and selects new signed IDs.
    amend_fill(&mut e, socket, NOW + 11.0, 1.0);
    e.drain_actions().for_each(drop);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    let out: Vec<_> = actions(&mut e);
    let write = out
        .iter()
        .position(|a| matches!(a, Action::Http(_)))
        .unwrap();
    let rows: Vec<_> = out[..write]
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandProgress {
                command: 99,
                receipts,
            }) => Some(receipts),
            _ => None,
        })
        .flatten()
        .collect();
    let Action::Http(request) = &out[write] else {
        panic!()
    };
    let p = form(request);
    assert_eq!(p["size"], "3");
    assert_eq!(
        rows.iter().map(|r| r.identity.as_str()).collect::<Vec<_>>(),
        [
            p["clientOrderId"].as_str(),
            p["tpClientOrderId"].as_str(),
            p["slClientOrderId"].as_str()
        ]
    );
}

#[test]
fn a_limit_whose_replacement_is_refused_says_the_order_is_gone() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    let replacement = requests(&mut e).remove(0);
    response(&mut e, &replacement, NOW + 16.0, REFUSED);
    let error = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 99, error }) => Some(error),
            _ => None,
        })
        .expect("a failure");
    assert!(
        error.message.contains("Your BTCUSDT order is gone"),
        "{}",
        error.message
    );
}

#[test]
fn a_stop_whose_replacement_is_refused_says_the_position_is_unprotected() {
    for (id, expected) in [
        (
            "parent__stop_loss",
            "Your BTCUSDT long position has no stop",
        ),
        (
            "parent__take_profit",
            "Your BTCUSDT long position has no take profit",
        ),
    ] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, id, None).remove(0);
        response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
        prove_cancel(&mut e);
        let replacement = requests(&mut e).remove(0);
        response(&mut e, &replacement, NOW + 16.0, REFUSED);
        let error = e
            .drain_actions()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 99, error }) => Some(error),
                _ => None,
            })
            .expect("a failure");
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn write_failures_use_shared_classification_and_preserve_network_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, body, kind, outcome, code) in [
        (
            500,
            r#"{"msg":"server error"}"#,
            Some(ErrorKind::VenueUnavailable),
            Some(Outcome::Unknown),
            None,
        ),
        (
            200,
            r#"{"code":10003,"msg":"insufficient funds"}"#,
            None,
            Some(Outcome::Unknown),
            Some("10003"),
        ),
        (
            429,
            r#"{"code":429,"msg":"too many requests"}"#,
            Some(ErrorKind::RateLimited),
            Some(Outcome::Unknown),
            Some("429"),
        ),
        (
            200,
            r#"{"code":0,"data":{}}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
            None,
        ),
    ] {
        let mut e = ready();
        let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
        e.handle(
            NOW + 11.0,
            Event::HttpResponse {
                request: first.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let error = e
            .drain_actions()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{status} {body}"));
        assert_eq!(
            (error.kind, error.outcome, error.venue_code.as_deref()),
            (kind, outcome, code),
            "{status} {body}: {}",
            error.message
        );
    }
    let mut e = ready();
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    e.handle(
        NOW + 11.0,
        Event::HttpFailed {
            request: first.id,
            error: "connection reset",
        },
    );
    let error = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("failed command");
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}

#[test]
fn default_decimal_amendment_subtracts_late_fill_without_float_noise() {
    let (mut e, socket) = ready_with_socket();
    let decimal_parent = |filled: &str| {
        serde_json::json!({"topic":"ws_zk_accounts_v3","type":"delta","contents":{"orders":[{"id":"parent","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"OPEN","size":"0.3","price":"60000","cumSuccessFillSize":filled}]}}).to_string()
    };
    ws(&mut e, socket, NOW + 5.0, &decimal_parent("0.1"));
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    ws(&mut e, socket, NOW + 12.0, &decimal_parent("0.2"));
    e.drain_actions().for_each(drop);
    response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel(&mut e);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "0.1");
}

#[test]
fn p0_write_signature_body_on_5xx_does_not_authorize_resend() {
    let mut e = ready();
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    e.handle(
        NOW + 11.0,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 500,
                headers: "",
                body: r#"{"code":20016,"msg":"signature rejected"}"#,
            },
        },
    );
    let actions: Vec<_> = actions(&mut e);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 42, .. })
    )));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(request)
                if request.method == first.method && request.url == first.url)),
        "an uncertain write must not be released a second time"
    );
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 42, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn p0_write_unlisted_native_code_is_explicitly_unknown() {
    let mut e = ready();
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    response(
        &mut e,
        &first,
        NOW + 11.0,
        r#"{"code":987654,"msg":"unlisted native failure"}"#,
    );
    let actions: Vec<_> = actions(&mut e);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(request)
        if request.method == first.method && request.url == first.url))
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 42, .. })
    )));
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 42, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].venue_code.as_deref(), Some("987654"));
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
}

fn hmac_write_failure_is_unknown(command: Command, status: u16, body: &str, code: &str) {
    let mut e = ready();
    e.handle(NOW + 10.0, Event::Command(command));
    let first = requests(&mut e).remove(0);
    e.handle(
        NOW + 11.0,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    let actions: Vec<_> = actions(&mut e);
    assert!(
        !actions.iter().any(|action| matches!(action,
            Action::Http(request) if request.method == first.method && request.url == first.url)),
        "an uncertain HMAC write must not be replayed"
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 43, .. })
    )));
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 43, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "one terminal per HMAC command");
    assert_eq!(failures[0].venue_code.as_deref(), Some(code));
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
}

fn hmac_cancel() -> Command {
    Command::CancelOrders {
        command: 43,
        ids: vec!["native-order".into()],
    }
}
fn hmac_setting() -> Command {
    Command::SetLeverage {
        command: 43,
        symbol: "BTCUSDT".into(),
        leverage: 10.0,
        side: None,
    }
}

#[test]
fn managed_hmac_cancel_503_signature_body_is_unknown_without_resend() {
    hmac_write_failure_is_unknown(
        hmac_cancel(),
        503,
        r#"{"code":20016,"msg":"signature rejected"}"#,
        "20016",
    );
}

#[test]
fn managed_hmac_setting_408_signature_body_is_unknown_without_resend() {
    hmac_write_failure_is_unknown(
        hmac_setting(),
        408,
        r#"{"code":20016,"msg":"signature rejected"}"#,
        "20016",
    );
}

#[test]
fn managed_hmac_cancel_unlisted_code_is_unknown_with_native_evidence() {
    hmac_write_failure_is_unknown(
        hmac_cancel(),
        200,
        r#"{"code":987654,"msg":"unlisted native failure"}"#,
        "987654",
    );
}

#[test]
fn managed_hmac_setting_unlisted_code_is_unknown_with_native_evidence() {
    hmac_write_failure_is_unknown(
        hmac_setting(),
        200,
        r#"{"code":987654,"msg":"unlisted native failure"}"#,
        "987654",
    );
}

struct HmacOnly(Arc<LocalSigner>);
impl Signer for HmacOnly {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.0.sign_ready(work)
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.0.sign(work)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        if algorithm == Algorithm::HmacSha256 {
            self.0.can_sign(key, algorithm)
        } else {
            Capability::Unsupported
        }
    }
}
fn write_provider(hmac_only: bool) -> (Arc<dyn Signer>, Identities) {
    let mut extra = LocalExtras::default();
    extra.0.insert(
        ExtraKey::ZklinkSeed,
        LocalExtra {
            value: Secret::new(format!("{}1b", "01".repeat(64))),
            encoding: KeyEncoding::Hex,
        },
    );
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("api-key".into()),
                secret: Secret::new("mysecret".into()),
                passphrase: Some(Secret::new("pass".into())),
                extra,
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base64Text),
                extra: Some(ExtraKeyPolicy {
                    key: ExtraKey::ZklinkSeed,
                    field: ExtraFieldPolicy::RequiredForPrivate,
                    encoding: Some(KeyEncoding::Hex),
                    trim: false,
                }),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    let signer = Arc::new(signer);
    (
        if hmac_only {
            Arc::new(HmacOnly(signer))
        } else {
            signer
        },
        ids,
    )
}
#[test]
fn managed_provider_cancel_stages_prepare_before_any_signed_http() {
    let mut e = ready();
    e.handle(NOW + 10., Event::Command(hmac_cancel()));
    let staged = e.inner.drain_actions().collect::<Vec<_>>();
    assert!(
        staged.iter().any(|a| matches!(a, Action::ManagedHttp(_))),
        "cancel must enter trusted preparation before signing"
    );
    assert!(
        !staged
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "no eager HTTP or provider work before Prepare"
    );
}
#[test]
fn managed_provider_without_zklink_refuses_place_before_foreground_io() {
    let (mut e, _) = ready_with_provider(true);
    let actions = place(&mut e, vec![order(serde_json::json!({}))]);
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
        )),
        "unsupported placement must refuse before foreground I/O"
    );
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:42,error})
        if error.kind==Some(safe_cex_engine::ErrorKind::Unsupported) && error.outcome==Some(safe_cex_engine::Outcome::NotSent))).count(),1);
}
#[test]
fn managed_provider_without_zklink_refuses_entire_amend_before_cancel() {
    let (mut e, socket) = ready_with_provider(true);
    parent_update(&mut e, socket, NOW + 5., 6., "OPEN", true);
    e.drain_actions().for_each(drop);
    e.handle(
        NOW + 10.,
        Event::Command(Command::AmendOrder {
            command: 99,
            id: "parent".into(),
            price: Some(59000.),
            amount: None,
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
        )),
        "whole-command capability must precede the amend cancel"
    );
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:99,error})
        if error.kind==Some(safe_cex_engine::ErrorKind::Unsupported) && error.outcome==Some(safe_cex_engine::Outcome::NotSent))).count(),1);
    assert_eq!(
        e.store().order("parent").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn non_success_status_with_code_zero_never_accepts_or_changes_leverage() {
    let mut e = ready();
    let before = e.store().setting("BTCUSDT").unwrap().leverage;
    e.handle(NOW + 10., Event::Command(hmac_setting()));
    let first = requests(&mut e).remove(0);
    e.handle(
        NOW + 11.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 400,
                headers: "",
                body: r#"{"code":0,"data":null}"#,
            },
        },
    );
    let out = actions(&mut e);
    assert_eq!(out.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 43, error }) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))).count(), 1);
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Notice(Notice::CommandDone { .. })))
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, before);
}

#[test]
fn quoted_refusal_codes_share_the_integer_code_table_and_message() {
    for code in ["401", "429"] {
        let mut e = ready();
        e.handle(NOW + 10., Event::Command(hmac_cancel()));
        let first = requests(&mut e).remove(0);
        response(
            &mut e,
            &first,
            NOW + 11.,
            &format!(r#"{{"code":"{code}","msg":"native refusal"}}"#),
        );
        let out = actions(&mut e);
        let error = out
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 43, error }) => Some(error),
                _ => None,
            })
            .unwrap();
        let outcome = if code == "429" {
            safe_cex_engine::Outcome::Unknown
        } else {
            safe_cex_engine::Outcome::Rejected
        };
        assert_eq!(error.outcome, Some(outcome));
        assert_eq!(error.venue_code.as_deref(), Some(code));
        assert_eq!(error.venue_message.as_deref(), Some("native refusal"));
        assert_eq!(
            error.message,
            format!("ApeX Omni request failed [{code}]: native refusal")
        );
    }
}

#[test]
fn accepted_cancel_ids_survive_the_next_cancel_transport_loss() {
    let mut e = ready();
    e.handle(
        NOW + 10.,
        Event::Command(Command::CancelOrders {
            command: 43,
            ids: vec!["one".into(), "two".into()],
        }),
    );
    let first = requests(&mut e).remove(0);
    response(&mut e, &first, NOW + 11., r#"{"code":0,"data":null}"#);
    let second = requests(&mut e).remove(0);
    e.handle(
        NOW + 12.,
        Event::HttpFailed {
            request: second.id,
            error: "lost after dispatch",
        },
    );
    let out = actions(&mut e);
    let error = out
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 43, error }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["one"]);
}

#[test]
fn late_cancel_ack_preserves_uncertainty_without_bypassing_native_read_fences() {
    for update_after_release in [false, true] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5., 0., "OPEN", true);
        e.drain_actions().for_each(drop);
        e.handle(
            NOW + 10.,
            Event::Command(Command::CancelOrders {
                command: 43,
                ids: vec!["parent".into()],
            }),
        );
        let issued = actions(&mut e);
        let (timer, deadline) = issued
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms > NOW + 10. => Some((*timer, *at_ms)),
                _ => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        let first = http(issued).remove(0);
        e.handle(deadline, Event::TimerFired { timer });
        let terminal = actions(&mut e);
        assert_eq!(
            terminal
                .iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(Notice::CommandFailed { command: 43, .. })
                ))
                .count(),
            1
        );
        if update_after_release {
            parent_update(&mut e, socket, deadline + 1., 1., "OPEN", true);
            e.drain_actions().for_each(drop);
        }
        response(&mut e, &first, deadline + 2., r#"{"code":0,"data":null}"#);
        let late = actions(&mut e);
        assert!(!late.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )));
        assert_eq!(
            e.store().order("parent").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
    }
}

/// A remote HMAC signer for writes. Account reads and SessionAuth answer
/// inline from the installed fixture; mutation signatures await a remote reply.
struct DelegatedHmac(Arc<dyn Signer>);
impl Signer for DelegatedHmac {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        (matches!(work.context.method.as_str(), "GET" | "WS"))
            .then(|| self.0.sign_ready(work))
            .flatten()
    }
    fn can_sign(&self, _: &KeyRef, algorithm: Algorithm) -> Capability {
        if algorithm == Algorithm::HmacSha256 {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
    }
}
fn remote_actions(e: &mut TestEngine) -> Vec<Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    let now = e.now;
    e.inner.prepare_dispatch(now);
    let mut queue = std::collections::VecDeque::from(e.inner.drain_actions().collect::<Vec<_>>());
    let mut out = vec![];
    while let Some(action) = queue.pop_front() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(
                        !request
                            .headers
                            .iter()
                            .any(|(key, _)| key.starts_with("APEX-"))
                    );
                    e.inner.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(
                        e.inner.claim_http(now, permit),
                        Ok(Claim::Released(_))
                    ));
                    out.push(Action::Http(request));
                }
                HttpAction::BindGroup { .. } => {
                    panic!("ApeX fixture declares no Mutation barriers")
                }
                HttpAction::Cancel { .. } => {}
            },
            action => out.push(action),
        }
        queue.extend(e.inner.drain_actions().collect::<Vec<_>>());
    }
    out
}
fn remote_hmac_answer(
    work: &SignRequest,
    identity: &safe_cex_engine::signing::Identity,
) -> safe_cex_engine::signing::SignResult {
    use safe_cex_engine::signing::{Operation, PayloadPart};
    assert_eq!(work.context.venue, "apexomni");
    assert_eq!(work.context.method, "POST");
    assert_eq!(work.operations.len(), 3);
    assert!(
        matches!(&work.operations[0], Operation::Present { key } if key == &identity.credential)
    );
    assert!(
        matches!(&work.operations[1], Operation::Present { key } if Some(key) == identity.passphrase.as_ref())
    );
    let Operation::Sign {
        algorithm: Algorithm::HmacSha256,
        payload,
        ..
    } = &work.operations[2]
    else {
        panic!("remote wire must carry HMAC only")
    };
    let payload: Vec<_> = payload
        .iter()
        .flat_map(|part| match part {
            PayloadPart::Bytes { bytes } => bytes.clone(),
            _ => panic!("unexpected interpolation in HMAC-only wire"),
        })
        .collect();
    let suffix = match work.context.path.as_str() {
        "/api/v3/delete-order" => "POST/api/v3/delete-orderid=native-order",
        "/api/v3/set-initial-margin-rate" => {
            "POST/api/v3/set-initial-margin-rateinitialMarginRate=0.10000&symbol=BTC-USDT&token=USDT"
        }
        _ => panic!("unexpected remote command scope"),
    };
    assert_eq!(work.receipt.stamp_ms, Some(1_700_000_000_133.));
    assert_eq!(payload, format!("1700000000133{suffix}").as_bytes());
    // Node22 createHmac('sha256', Buffer.from('remote-only-secret').toString('base64')).
    // Both literal vectors are independent of the venue secret and Rust crypto.
    let hex = if work.context.path.ends_with("/delete-order") {
        "c8b02f8cf2463ea0b6592a3b12860b7a2c936fe21e08d256af04de52719f1b25"
    } else {
        "b4441fc36d7bde557de84e929c6824c035b876528d2fcb13b66fecd9c7de09c9"
    };
    let mac = hex
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    safe_cex_engine::signing::SignResult::new(
        vec![
            b"remote-api-key".to_vec(),
            b"remote-passphrase".to_vec(),
            mac,
        ],
        work.receipt,
    )
}
#[test]
fn delegated_hmac_cancel_and_multipart_setting_release_only_after_remote_reply() {
    for command in [hmac_cancel(), hmac_setting()] {
        let (mut e, _) = ready_with_provider_mode(true, true);
        e.handle(NOW + 10., Event::Command(command));
        let staged = remote_actions(&mut e);
        assert!(
            !staged
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        let (attempt, work) = staged
            .into_iter()
            .find_map(|action| match action {
                Action::Sign { attempt, work, .. } => Some((attempt, work)),
                _ => None,
            })
            .unwrap();
        let result = remote_hmac_answer(&work, e.identities().primary.as_ref().unwrap());
        let expected_mac = if work.context.path.ends_with("/delete-order") {
            "yLAvjPJGPqC2WSo7EoYLeiyTb+IeCNJWrwTeUnGfGyU="
        } else {
            "tEQfw2173lV96E6SnGgkwDW4dlKNL8sTtm/s2cfeCck="
        };
        e.handle(
            NOW + 11.,
            Event::Signed {
                attempt,
                result: Ok(result.clone()),
            },
        );
        let released = remote_actions(&mut e);
        let mut sent = http(released);
        assert_eq!(
            sent.len(),
            1,
            "one physical request follows the remote receipt"
        );
        let first = sent.remove(0);
        let headers: HashMap<_, _> = first
            .headers
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        assert_eq!(headers["APEX-SIGNATURE"], expected_mac);
        assert_eq!(headers["APEX-API-KEY"], "remote-api-key");
        assert_eq!(headers["APEX-PASSPHRASE"], "remote-passphrase");
        assert_eq!(headers["APEX-TIMESTAMP"], "1700000000133");
        assert_eq!(headers.len(), 5);
        if first.url.ends_with("/delete-order") {
            assert_eq!(
                headers["Content-Type"],
                "application/x-www-form-urlencoded; charset=UTF-8"
            );
            assert_eq!(first.body.as_deref(), Some("id=native-order"));
        } else {
            assert_eq!(
                headers["Content-Type"],
                "multipart/form-data; boundary=safe-cex-apexomni"
            );
            assert_eq!(
                first.body.as_deref(),
                Some(concat!(
                    "--safe-cex-apexomni\r\nContent-Disposition: form-data; name=\"symbol\"\r\n\r\nBTC-USDT\r\n",
                    "--safe-cex-apexomni\r\nContent-Disposition: form-data; name=\"token\"\r\n\r\nUSDT\r\n",
                    "--safe-cex-apexomni\r\nContent-Disposition: form-data; name=\"initialMarginRate\"\r\n\r\n0.10000\r\n",
                    "--safe-cex-apexomni--\r\n"
                ))
            );
        }
        e.handle(
            NOW + 12.,
            Event::Signed {
                attempt,
                result: Ok(result),
            },
        );
        assert!(
            !remote_actions(&mut e)
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        response(&mut e, &first, NOW + 13., r#"{"code":0,"data":null}"#);
        let terminal = remote_actions(&mut e);
        assert_eq!(terminal.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command:43, result }) if result == "[]")).count(), 1);
    }
}
#[test]
fn delegated_provider_refusal_is_typed_not_sent_before_http() {
    let (mut e, _) = ready_with_provider_mode(true, true);
    e.handle(NOW + 10., Event::Command(hmac_setting()));
    let staged = remote_actions(&mut e);
    assert!(
        !staged
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    let attempt = staged
        .into_iter()
        .find_map(|action| match action {
            Action::Sign { attempt, .. } => Some(attempt),
            _ => None,
        })
        .unwrap();
    e.handle(
        NOW + 11.,
        Event::Signed {
            attempt,
            result: Err(safe_cex_engine::signing::SignError::Refused),
        },
    );
    let out = remote_actions(&mut e);
    assert!(!out.iter().any(|action| matches!(action, Action::Http(_))));
    assert_eq!(out.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandFailed { command:43,error }) if error.kind == Some(safe_cex_engine::ErrorKind::InvalidInput) && error.outcome == Some(safe_cex_engine::Outcome::NotSent))).count(),1);
}

/// P1: an Unknown placement fences its symbol until a fresh order read proves
/// what happened. ApeX has no placement scope, so this fence engages only with
/// the engine's Phase B uncertainty marker (see RUST_CUTOVER_CALLERS.md).
#[test]
fn an_unknown_placement_fences_its_symbol_until_a_fresh_order_read() {
    fn place_at(
        e: &mut TestEngine,
        now: f64,
        command: safe_cex_engine::CommandId,
        client: &str,
    ) -> Vec<Action> {
        e.handle(
            now,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order(serde_json::json!({ "clientOrderId": client }))],
            }),
        );
        actions(e)
    }
    fn order_writes(actions: &[Action]) -> usize {
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/order")))
            .count()
    }
    fn refused_uncertain(actions: &[Action]) -> bool {
        actions.iter().any(|a| {
            matches!(a, Action::Notice(Notice::CommandFailed { error, .. })
                if error.message.contains("Placement outcome is uncertain"))
        })
    }
    fn open_orders_read(actions: Vec<Action>) -> HttpRequest {
        http(actions)
            .into_iter()
            .find(|r| r.url.ends_with("/v3/open-orders"))
            .expect("an open-orders read")
    }
    const EMPTY: &str = r#"{"code":0,"data":[]}"#;

    let (mut e, socket) = ready_with_socket();
    // An orders read dispatched before the placement: a socket order delta arms
    // the heal timer, whose read then stays in flight.
    ws(
        &mut e,
        socket,
        NOW + 5.0,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"orders":[{"id":"other","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"OPEN","size":"0.001","price":"50000"}]}}"#,
    );
    let heal = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 1_005.0 => Some(timer),
            _ => None,
        })
        .expect("the order heal timer");
    e.handle(NOW + 1_010.0, Event::TimerFired { timer: heal });
    let stale = open_orders_read(actions(&mut e));

    let write = http(place_at(&mut e, NOW + 1_020.0, 42, "first"))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/order"))
        .expect("the placement write");
    e.handle(
        NOW + 1_030.0,
        Event::HttpFailed {
            request: write.id,
            error: "connection reset after write",
        },
    );
    actions(&mut e);

    // Retry on the same symbol: refused NotSent, nothing posted.
    let retry = place_at(&mut e, NOW + 1_040.0, 43, "retry");
    assert!(
        refused_uncertain(&retry),
        "the retry must be refused while the placement is uncertain"
    );
    assert_eq!(order_writes(&retry), 0, "no write may leave while fenced");

    // The stale read completes cleanly; it was dispatched before the mark.
    response(&mut e, &stale, NOW + 1_050.0, EMPTY);
    let after_stale = actions(&mut e);
    let fresh = open_orders_read(after_stale);
    let retry = place_at(&mut e, NOW + 1_060.0, 44, "retry-after-stale");
    assert!(
        refused_uncertain(&retry),
        "a stale read must not release the fence"
    );
    assert_eq!(order_writes(&retry), 0);

    // The owed fresh read releases it, and the retry places.
    response(&mut e, &fresh, NOW + 1_070.0, EMPTY);
    actions(&mut e);
    let retry = place_at(&mut e, NOW + 1_080.0, 45, "retry-after-fresh");
    assert!(
        !refused_uncertain(&retry),
        "a fresh clean read releases the fence"
    );
    assert_eq!(order_writes(&retry), 1, "the retry places once released");
}

/// The engine re-asks the hook while the placement stays uncertain; each re-ask must
/// join the fresh read in flight, and that read's clean answer releases.
#[test]
fn engine_re_asks_join_the_fresh_read_in_flight_and_its_clean_answer_releases() {
    const EMPTY: &str = r#"{"code":0,"data":[]}"#;
    let (mut e, _) = ready_with_socket();
    let mut timers: Vec<(safe_cex_engine::TimerId, f64)> = Vec::new();
    let take = |e: &mut TestEngine, timers: &mut Vec<(safe_cex_engine::TimerId, f64)>| {
        let actions = actions(e);
        for action in &actions {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
                _ => {}
            }
        }
        actions
    };
    let advance = |e: &mut TestEngine,
                   timers: &mut Vec<(safe_cex_engine::TimerId, f64)>,
                   to: f64,
                   out: &mut Vec<Action>| {
        while let Some(i) = timers.iter().position(|(_, at)| *at <= to) {
            let (timer, at) = timers.remove(i);
            e.handle(at.max(e.now), Event::TimerFired { timer });
            for action in actions(e) {
                match &action {
                    Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
                    Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
                    _ => {}
                }
                out.push(action);
            }
        }
        e.now = e.now.max(to);
    };
    let open_orders = |actions: &[Action]| {
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/open-orders")))
            .count()
    };
    let order_writes = |actions: &[Action]| {
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/order")))
            .count()
    };
    let refused = |actions: &[Action]| {
        actions.iter().any(|a| {
            matches!(a, Action::Notice(Notice::CommandFailed { error, .. })
                if error.message.contains("Placement outcome is uncertain"))
        })
    };
    let place =
        |e: &mut TestEngine, now: f64, command: safe_cex_engine::CommandId, client: &str| {
            e.handle(
                now,
                Event::Command(Command::PlaceOrders {
                    command,
                    orders: vec![order(serde_json::json!({ "clientOrderId": client }))],
                }),
            );
        };

    place(&mut e, NOW + 10.0, 42, "first");
    let write = http(take(&mut e, &mut timers))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/order"))
        .expect("the placement write");
    e.handle(
        NOW + 20.0,
        Event::HttpFailed {
            request: write.id,
            error: "connection reset after write",
        },
    );
    // The seal's ask starts the fresh read; it stays unanswered.
    let fresh = http(take(&mut e, &mut timers))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/open-orders"))
        .expect("the fresh reconcile read");

    let mut now = NOW + 20.0;
    for (i, wait) in [1_100.0, 2_100.0, 4_100.0].into_iter().enumerate() {
        now += wait;
        let mut fired = Vec::new();
        advance(&mut e, &mut timers, now, &mut fired);
        assert_eq!(
            open_orders(&fired),
            0,
            "re-ask {i} must not replace the fresh read"
        );
        place(
            &mut e,
            now,
            50 + i as safe_cex_engine::CommandId,
            "held-retry",
        );
        let retry = take(&mut e, &mut timers);
        assert!(
            refused(&retry),
            "the retry stays refused while the fresh read is held"
        );
        assert_eq!(order_writes(&retry), 0);
    }

    // The original fresh read answers clean: it releases, and the retry places.
    response(&mut e, &fresh, now + 10.0, EMPTY);
    take(&mut e, &mut timers);
    place(&mut e, now + 20.0, 60, "released-retry");
    let retry = take(&mut e, &mut timers);
    assert!(!refused(&retry), "the held fresh read releases the fence");
    assert_eq!(order_writes(&retry), 1, "the retry places once released");
}

/// Cancel-all on a symbol with an Unknown placement is refused NotSent until the
/// fresh order read releases the fence (default `fence_uncertain_cancel_all`).
#[test]
fn a_rejected_row_on_another_market_releases_the_fence() {
    let (mut e, _socket) = ready_with_socket();
    let place = |e: &mut TestEngine, now, command, client: &str| {
        e.handle(
            now,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order(serde_json::json!({ "clientOrderId": client }))],
            }),
        );
        actions(e)
    };
    let write = http(place(&mut e, NOW + 10.0, 42, "first"))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/order"))
        .expect("the placement write");
    e.handle(
        NOW + 20.0,
        Event::HttpFailed {
            request: write.id,
            error: "connection reset after write",
        },
    );
    let fresh = http(actions(&mut e))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/open-orders"))
        .expect("the reconcile read");
    response(
        &mut e,
        &fresh,
        NOW + 30.0,
        r#"{"code":0,"data":[{"id":"77","symbol":"SOL-USDT"}]}"#,
    );
    actions(&mut e);
    let retry = http(place(&mut e, NOW + 40.0, 43, "retry"));
    assert!(retry.iter().any(|r| r.url.ends_with("/v3/order")));
}

#[test]
fn an_unknown_placement_fences_cancel_all_until_the_fresh_order_read_releases() {
    const EMPTY: &str = r#"{"code":0,"data":[]}"#;
    let cancel_all = |e: &mut TestEngine, now: f64, command: safe_cex_engine::CommandId| {
        e.handle(
            now,
            Event::Command(Command::CancelSymbolOrders {
                command,
                symbol: "BTCUSDT".into(),
            }),
        );
        actions(e)
    };
    let cancels = |actions: &[Action]| {
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/delete-open-orders")))
            .count()
    };
    let (mut e, _) = ready_with_socket();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::PlaceOrders {
            command: 42,
            orders: vec![order(serde_json::json!({ "clientOrderId": "first" }))],
        }),
    );
    let write = http(actions(&mut e))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/order"))
        .expect("the placement write");
    e.handle(
        NOW + 20.0,
        Event::HttpFailed {
            request: write.id,
            error: "connection reset after write",
        },
    );
    let fresh = http(actions(&mut e))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/open-orders"))
        .expect("the fresh reconcile read");

    let fenced = cancel_all(&mut e, NOW + 30.0, 43);
    assert!(
        fenced.iter().any(
            |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 43, error })
            if error.message.contains("reconcile before cancel-all")
                && error.outcome == Some(safe_cex_engine::Outcome::NotSent))
        ),
        "cancel-all is refused NotSent while the placement is uncertain"
    );
    assert_eq!(cancels(&fenced), 0, "nothing is cancelled while fenced");

    response(&mut e, &fresh, NOW + 40.0, EMPTY);
    actions(&mut e);
    let released = cancel_all(&mut e, NOW + 50.0, 44);
    assert!(
        !released
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 44, .. }))),
        "a fresh clean read releases cancel-all"
    );
    assert_eq!(cancels(&released), 1, "cancel-all is sent once released");
}

/// A coded 200: the listed code refuses and leaves no fence.
#[test]
fn documented_coded_refusal_leaves_no_fence() {
    use safe_cex_engine::Outcome;
    let mut e = ready();
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    response(
        &mut e,
        &first,
        NOW + 11.0,
        r#"{"code":10002,"msg":"Your API key is wrong"}"#,
    );
    let settled = actions(&mut e);
    assert!(
        settled.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 42, error })
            if error.outcome == Some(Outcome::Rejected))),
        "{settled:?}"
    );
    let next = place_with_id(&mut e, 43, vec![order(serde_json::json!({}))]);
    assert!(
        next.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/order")))
    );
}

/// An unlisted code stays Unknown, reads at once, and that read lifts the fence.
#[test]
fn unlisted_coded_placement_fence_clears_on_the_next_read() {
    let mut e = ready();
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    response(
        &mut e,
        &first,
        NOW + 11.0,
        r#"{"code":987654,"msg":"unlisted native failure"}"#,
    );
    let read = http(actions(&mut e))
        .into_iter()
        .find(|r| r.url.ends_with("/v3/open-orders"))
        .expect("the read starts with the answer");
    let fenced = place_with_id(&mut e, 43, vec![order(serde_json::json!({}))]);
    assert!(
        fenced.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: 43, error })
            if error.message.contains("uncertain"))),
        "{fenced:?}"
    );
    response(&mut e, &read, NOW + 12.0, r#"{"code":0,"data":[]}"#);
    let _ = actions(&mut e);
    let next = place_with_id(&mut e, 44, vec![order(serde_json::json!({}))]);
    assert!(
        next.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/v3/order"))),
        "{next:?}"
    );
}

/// A resting reduce-only sell stop at 55000 on the index, guarding a long.
fn resting_stop(e: &mut TestEngine, socket: SocketId, filled: f64) {
    resting_stop_of(e, socket, filled, "STOP_MARKET");
}
fn resting_stop_of(e: &mut TestEngine, socket: SocketId, filled: f64, kind: &str) {
    ws(
        e,
        socket,
        NOW + 5.0,
        &format!(
            r#"{{"topic":"ws_zk_accounts_v3","type":"snapshot","contents":{{"orders":[{{"id":"stop","symbol":"BTC-USDT","side":"SELL","type":"{kind}","status":"UNTRIGGERED","size":"10","price":"54725","triggerPrice":"55000","triggerPriceType":"INDEX","reduceOnly":true,"cumSuccessFillSize":"{filled}","updatedAt":{}}}]}}}}"#,
            NOW + 5.0
        ),
    );
    e.store_mut().set_positions(vec![long_position()]);
    e.drain_actions().for_each(drop);
}
fn long_position() -> safe_cex_engine::model::Position {
    use safe_cex_engine::model::*;
    Position {
        id: "BTCUSDT:long".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 60000.0,
        notional: 600000.0,
        leverage: 10.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Hedged),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 10.0,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }
}
fn amend_command(e: &mut TestEngine, id: &str, price: f64) {
    e.handle(
        NOW + 10.0,
        Event::Command(Command::AmendOrder {
            command: 99,
            id: id.into(),
            price: Some(price),
            amount: None,
        }),
    );
}
fn amend_to(e: &mut TestEngine, id: &str, price: f64) -> Vec<HttpRequest> {
    amend_command(e, id, price);
    requests(e)
}
fn failure_of(out: &[Action]) -> safe_cex_engine::VenueError {
    out.iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 99, error }) => Some(error.clone()),
            _ => None,
        })
        .expect("a failure")
}
fn placements(requests: &[HttpRequest]) -> Vec<&HttpRequest> {
    requests
        .iter()
        .filter(|r| r.url.ends_with("/v3/order") || r.url.ends_with("/v3/delete-order"))
        .collect()
}
// A definite non-rate refusal; rate limits never authorize a put-back.
const REFUSED: &str = r#"{"code":10002,"msg":"Requires Login: Your API key is wrong"}"#;
/// Amends the resting stop to 56000 and lands its cancel; its replacement is next.
fn to_stop_replacement(e: &mut TestEngine) -> HttpRequest {
    let cancel = amend_to(e, "stop", 56000.0).remove(0);
    assert!(cancel.url.ends_with("/v3/delete-order"), "{}", cancel.url);
    response(e, &cancel, NOW + 15.0, REAL_CANCEL);
    prove_cancel_of(e, "stop", "CANCELED");
    let replacement = requests(e).remove(0);
    assert_eq!(form(&replacement)["triggerPrice"], "56000");
    replacement
}

#[test]
fn a_stale_crossed_changed_or_flat_stop_at_the_final_claim_cancels_nothing() {
    use safe_cex_engine::Outcome;
    for (case, why) in [
        ("stale", "index price reference is stale"),
        ("crossed", "already crossed the index price"),
        ("filled", "changed or filled before cancellation"),
        ("flat", "no position for it to protect"),
    ] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.0);
        amend_command(&mut e, "stop", 56000.0);
        // Between planning and the cancel's final claim.
        match case {
            "stale" => e.now = NOW + 10.0 + 6_000.0,
            // The index fell through the new stop; last and mark did not.
            "crossed" => e
                .store_mut()
                .update_ticker("BTCUSDT", |t| t.index = 55900.0),
            "filled" => {
                e.store_mut().update_order("stop", |o| o.filled = 1.0);
            }
            _ => e.store_mut().set_positions(vec![]),
        }
        let out = actions(&mut e);
        assert!(
            placements(&http(out.clone())).is_empty(),
            "{case}: no cancel"
        );
        let error = failure_of(&out);
        assert!(
            error.message.contains("was not changed") && error.message.contains(why),
            "{case}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{case}");
        assert_eq!(error.protection, None, "{case}: never Unprotected");
        assert!(e.store().order("stop").is_some(), "{case}: untouched");
    }
}

#[test]
fn a_stop_veto_follows_the_index_source_not_the_mark() {
    for (index, mark, sent) in [(60000.0, 55000.0, true), (55900.0, 60000.0, false)] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.0);
        e.store_mut().update_ticker("BTCUSDT", |t| {
            t.index = index;
            t.mark = mark;
            t.last = mark;
        });
        let sent_cancel = amend_to(&mut e, "stop", 56000.0)
            .iter()
            .any(|r| r.url.ends_with("/v3/delete-order"));
        assert_eq!(sent_cancel, sent, "index {index}, mark and last {mark}");
    }
}

#[test]
fn a_resent_cancel_is_checked_again_at_its_own_claim() {
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let cancel = amend_to(&mut e, "stop", 56000.0).remove(0);
    e.store_mut()
        .update_ticker("BTCUSDT", |t| t.index = 55900.0);
    response(
        &mut e,
        &cancel,
        NOW + 15.0,
        r#"{"code":20016,"msg":"signature expired"}"#,
    );
    let out = actions(&mut e);
    assert!(placements(&http(out.clone())).is_empty(), "no resend");
    let error = failure_of(&out);
    assert!(
        error.message.contains("already crossed the index price"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, None);
}

#[test]
fn a_refused_stop_replacement_puts_the_original_back_once() {
    use safe_cex_engine::error::Protection;
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    response(&mut e, &replacement, NOW + 200.0, REFUSED);
    let restore = requests(&mut e).remove(0);
    let fields = form(&restore);
    for (key, value) in [
        ("triggerPrice", "55000"),
        ("triggerPriceType", "INDEX"),
        ("side", "SELL"),
        ("size", "10"),
        ("reduceOnly", "true"),
    ] {
        assert_eq!(fields[key], value, "{key}: the original, not the edit");
    }
    assert_ne!(fields["clientOrderId"], form(&replacement)["clientOrderId"]);
    response(&mut e, &restore, NOW + 201.0, REAL_PLACE);
    let out = actions(&mut e);
    let error = failure_of(&out);
    assert!(
        error.message.contains("is unchanged") && error.message.contains("put back"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Restored));
    assert_ne!(
        error.outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "a proven cancel is never typed unknown"
    );
    assert!(placements(&http(out)).is_empty(), "one put-back only");
}

#[test]
fn a_refused_put_back_is_unprotected_and_never_retried() {
    use safe_cex_engine::error::Protection;
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    response(&mut e, &replacement, NOW + 200.0, REFUSED);
    let restore = requests(&mut e).remove(0);
    response(&mut e, &restore, NOW + 201.0, REFUSED);
    let out = actions(&mut e);
    let error = failure_of(&out);
    assert!(
        error.message.contains("both refused") && error.message.contains("re-place"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(placements(&http(out)).is_empty());
}

#[test]
fn an_unknown_stop_replacement_or_put_back_is_never_resent() {
    for unknown_put_back in [false, true] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.0);
        let mut call = to_stop_replacement(&mut e);
        if unknown_put_back {
            response(&mut e, &call, NOW + 200.0, REFUSED);
            call = requests(&mut e).remove(0);
        }
        e.handle(
            NOW + 201.0,
            Event::HttpFailed {
                request: call.id,
                error: "lost write response",
            },
        );
        let out = actions(&mut e);
        let error = failure_of(&out);
        assert_eq!(
            error.protection, None,
            "put-back unknown: {unknown_put_back}"
        );
        if unknown_put_back {
            assert!(
                error.message.contains("put-back's outcome is unknown")
                    && error.message.contains("Check Open Orders"),
                "{}",
                error.message
            );
        }
        assert!(
            placements(&without_reconcile_read(http(out))).is_empty(),
            "no put-back or resend over an unknown"
        );
    }
}

#[test]
fn a_stop_that_filled_during_its_cancel_or_a_partly_filled_entry_leg_is_not_put_back() {
    use safe_cex_engine::error::Protection;
    for leg in [false, true] {
        let (mut e, socket) = ready_with_socket();
        let replacement = if leg {
            // A leg of an entry 6/10 filled: its put-back cannot be proven.
            parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
            e.drain_actions().for_each(drop);
            let cancel = amend_to(&mut e, "parent__stop_loss", 59000.0).remove(0);
            response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
            prove_cancel(&mut e);
            requests(&mut e).remove(0)
        } else {
            resting_stop(&mut e, socket, 0.0);
            let cancel = amend_to(&mut e, "stop", 56000.0).remove(0);
            response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
            let (timer, at) = actions(&mut e)
                .into_iter()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .unwrap();
            e.handle(at, Event::TimerFired { timer });
            let read = requests(&mut e).remove(0);
            response(
                &mut e,
                &read,
                at,
                r#"{"data":{"id":"stop","status":"CANCELED","cumSuccessFillSize":"2"}}"#,
            );
            requests(&mut e).remove(0)
        };
        response(&mut e, &replacement, NOW + 200.0, REFUSED);
        let out = actions(&mut e);
        let error = failure_of(&out);
        assert!(
            error.message.contains("was removed") && error.message.contains("please re-place it"),
            "leg {leg}: {}",
            error.message
        );
        assert_eq!(error.protection, Some(Protection::Unprotected), "leg {leg}");
        assert!(placements(&http(out)).is_empty(), "leg {leg}");
    }
}

#[test]
fn an_unproven_cancel_sends_no_replacement_or_put_back() {
    use safe_cex_engine::Outcome;
    for lost in [true, false] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.0);
        let cancel = amend_to(&mut e, "stop", 56000.0).remove(0);
        response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
        // Lost reads are retried once; a cancel still open is read up to its bound.
        let reads = if lost { 2 } else { 31 };
        for _ in 0..reads {
            let (timer, at) = actions(&mut e)
                .into_iter()
                .find_map(|action| match action {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .expect("a pause before the next proof read");
            e.handle(at, Event::TimerFired { timer });
            let read = requests(&mut e).remove(0);
            assert!(read.url.contains("/v3/order?id=stop"), "{}", read.url);
            if lost {
                e.handle(
                    at,
                    Event::HttpFailed {
                        request: read.id,
                        error: "lost proof read",
                    },
                );
            } else {
                response(
                    &mut e,
                    &read,
                    at,
                    r#"{"data":{"id":"stop","status":"OPEN","cumSuccessFillSize":"0"}}"#,
                );
            }
        }
        let out = actions(&mut e);
        let error = failure_of(&out);
        assert!(
            error.message.contains("could not be confirmed")
                && error.message.contains("unknown")
                && error.message.contains("Check Open Orders"),
            "lost {lost}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown), "lost {lost}");
        assert_eq!(error.protection, None);
        let sent = http(out);
        assert!(placements(&sent).is_empty(), "lost {lost}");
        assert!(
            sent.iter().any(|r| r.url.contains("/v3/open-orders")),
            "lost {lost}: the unknown original is reconciled: {:?}",
            sent.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
    }
}

#[test]
fn cancels_of_the_original_during_and_after_its_put_back() {
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    e.handle(
        NOW + 150.0,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["stop".into()],
        }),
    );
    let out = actions(&mut e);
    assert!(
        out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, error }) if error.message.contains("already in flight"))),
        "a cancel during the amend is refused visibly"
    );
    response(&mut e, &replacement, NOW + 200.0, REFUSED);
    let restore = requests(&mut e).remove(0);
    response(&mut e, &restore, NOW + 201.0, REAL_PLACE);
    e.drain_actions().for_each(drop);
    // A screen still showing the original cancels it by its old id.
    e.handle(
        NOW + 300.0,
        Event::Command(Command::CancelOrders {
            command: 9,
            ids: vec!["stop".into()],
        }),
    );
    let cancels = requests(&mut e);
    assert!(
        cancels
            .iter()
            .any(|r| r.url.ends_with("/v3/delete-order") && form(r)["id"] == "904952998325649712"),
        "the put-back is cancelled: {:?}",
        cancels
            .iter()
            .map(|r| (&r.url, &r.body))
            .collect::<Vec<_>>()
    );
}

/// The put-back is one attempt: a refusal the engine would otherwise resend
/// (20016) is never sent twice.
#[test]
fn a_put_back_is_one_attempt_on_the_wire() {
    use safe_cex_engine::error::Protection;
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    response(&mut e, &replacement, NOW + 200.0, REFUSED);
    let restore = requests(&mut e).remove(0);
    response(
        &mut e,
        &restore,
        NOW + 201.0,
        r#"{"code":20016,"msg":"signature expired"}"#,
    );
    let out = actions(&mut e);
    assert!(placements(&http(out.clone())).is_empty(), "never resent");
    let error = failure_of(&out);
    assert!(error.message.contains("both refused"), "{}", error.message);
    assert_eq!(error.protection, Some(Protection::Unprotected));
}

/// A put-back the engine seals on a 408 or 5xx says its outcome is unknown,
/// never the replacement's refusal and never to re-place a second stop.
#[test]
fn a_put_back_sealed_on_a_5xx_says_its_outcome_is_unknown() {
    for status in [408, 503] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.0);
        let replacement = to_stop_replacement(&mut e);
        response(&mut e, &replacement, NOW + 200.0, REFUSED);
        let restore = requests(&mut e).remove(0);
        response_status(&mut e, &restore, NOW + 201.0, status);
        let out = actions(&mut e);
        let error = failure_of(&out);
        assert!(
            error.message.contains("put-back's outcome is unknown")
                && error.message.contains("Check Open Orders")
                && !error.message.contains("too many requests"),
            "{status}: {}",
            error.message
        );
        assert_eq!(
            error.outcome,
            Some(safe_cex_engine::Outcome::Unknown),
            "{status}"
        );
        assert_eq!(error.protection, None, "{status}");
        assert!(
            placements(&without_reconcile_read(http(out))).is_empty(),
            "{status}: never resent"
        );
    }
}

/// A put-back refused before it is sent (it cannot be signed) never reached
/// ApeX: the stop was removed and must be re-placed, not "both refused".
#[test]
fn a_put_back_refused_before_sending_says_the_stop_was_removed() {
    use safe_cex_engine::error::Protection;
    // The provider signs one order placement (the replacement), then refuses.
    let placements_signed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = placements_signed.clone();
    let (mut e, socket) = ready_with_signer(false, |provider| {
        Arc::new(RefusesLaterPlacements(provider, counter))
    });
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    response(&mut e, &replacement, NOW + 200.0, REFUSED);
    let out = actions(&mut e);
    let error = failure_of(&out);
    assert!(
        error.message.contains("was removed")
            && error.message.contains("please re-place it")
            && !error.message.contains("both refused"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(placements(&http(out)).is_empty(), "nothing was sent");
}

/// Signs through `0` until it has signed one order placement; refuses later ones.
struct RefusesLaterPlacements(Arc<dyn Signer>, Arc<std::sync::atomic::AtomicUsize>);
impl Signer for RefusesLaterPlacements {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.0.sign(request)
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if work.context.method == "POST" && work.context.path.ends_with("/v3/order") {
            let signed = self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if signed >= 1 {
                return Some(Err(SignError::Refused));
            }
        }
        self.0.sign_ready(work)
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}

/// An edit or put-back keeps the original's native type: a STOP_MARKET stays
/// one (a STOP_LIMIT may not fill through a gap); a STOP_LIMIT stays a limit.
#[test]
fn a_stop_edit_and_its_put_back_keep_the_native_order_type() {
    for kind in ["STOP_MARKET", "STOP_LIMIT"] {
        let (mut e, socket) = ready_with_socket();
        resting_stop_of(&mut e, socket, 0.0, kind);
        let cancel = amend_to(&mut e, "stop", 56000.0).remove(0);
        response(&mut e, &cancel, NOW + 15.0, REAL_CANCEL);
        prove_cancel_of(&mut e, "stop", "CANCELED");
        let replacement = requests(&mut e).remove(0);
        assert_eq!(form(&replacement)["type"], kind, "the edit");
        response(&mut e, &replacement, NOW + 200.0, REFUSED);
        let restore = requests(&mut e).remove(0);
        assert_eq!(form(&restore)["type"], kind, "the put-back");
        assert_eq!(form(&restore)["triggerPrice"], "55000");
    }
}

#[test]
fn ambiguous_rate_code_cannot_restore_protection() {
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    response(
        &mut e,
        &replacement,
        NOW + 200.0,
        r#"{"code":429,"msg":"too many requests"}"#,
    );
    let out = actions(&mut e);
    assert!(
        placements(&http(out.clone())).is_empty(),
        "native rate code issued another place: {:?}",
        out
    );
    let error = failure_of(&out);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.protection, None);
}

#[test]
fn ambiguous_signature_code_cannot_resend() {
    let mut e = ready();
    e.handle(NOW + 10., Event::Command(hmac_cancel()));
    let first = requests(&mut e).remove(0);
    e.handle(
        NOW + 11.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"{"code":20016,"msg":"signature refused"}"#,
            },
        },
    );
    let out = actions(&mut e);
    assert!(
        http(out.clone())
            .iter()
            .all(|r| r.method == safe_cex_engine::Method::Get),
        "ambiguous status resends: {:?}",
        out
    );
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 43, error }) => Some(error),
            _ => None,
        })
        .expect("terminal failure");
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}
#[test]
fn unproven_signature_control() {
    let mut e = ready();
    e.handle(NOW + 10., Event::Command(hmac_cancel()));
    let first = requests(&mut e).remove(0);
    e.handle(
        NOW + 11.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"code":20016,"msg":"signature refused"}"#,
            },
        },
    );
    let out = requests(&mut e);
    assert!(
        out.iter().all(|r| r.method == safe_cex_engine::Method::Get),
        "ambiguous status resends: {:?}",
        out
    );
}

#[test]
fn ambiguous_protective_cancel_cannot_resend_or_create() {
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let cancel = amend_to(&mut e, "stop", 56000.0).remove(0);
    e.handle(
        NOW + 15.0,
        Event::HttpResponse {
            request: cancel.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"{"code":20016,"msg":"signature expired"}"#,
            },
        },
    );
    let out = actions(&mut e);
    assert!(
        placements(&http(out.clone())).is_empty(),
        "ambiguous cancel sent another write: {out:?}"
    );
    let error = failure_of(&out);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(e.store().order("stop").is_some());
}

#[test]
fn ambiguous_put_back_cannot_claim_unprotected_or_resend() {
    for (status, code) in [(200, 429), (429, 20016)] {
        let (mut e, socket) = ready_with_socket();
        resting_stop(&mut e, socket, 0.0);
        let replacement = to_stop_replacement(&mut e);
        response(
            &mut e,
            &replacement,
            NOW + 200.0,
            r#"{"code":10002,"msg":"Requires Login: Your API key is wrong"}"#,
        );
        let restore = requests(&mut e).remove(0);
        let body = format!(r#"{{"code":{code},"msg":"uncertain write"}}"#);
        e.handle(
            NOW + 201.0,
            Event::HttpResponse {
                request: restore.id,
                response: Response {
                    status,
                    headers: "",
                    body: &body,
                },
            },
        );
        let out = actions(&mut e);
        assert!(
            placements(&http(out.clone())).is_empty(),
            "{status}/{code}: {out:?}"
        );
        let error = failure_of(&out);
        assert_eq!(
            error.outcome,
            Some(safe_cex_engine::Outcome::Unknown),
            "{status}/{code}: {error:?}"
        );
        assert_eq!(error.protection, None, "{status}/{code}: {error:?}");
        assert!(error.message.contains("Check Open Orders"), "{error:?}");
    }
}

#[test]
fn engine_ambiguity_rate_code_cannot_restore_protection() {
    let (mut e, socket) = ready_with_socket();
    resting_stop(&mut e, socket, 0.0);
    let replacement = to_stop_replacement(&mut e);
    response(
        &mut e,
        &replacement,
        NOW + 200.0,
        r#"{"code":429,"msg":"too many requests"}"#,
    );
    let out = requests(&mut e);
    assert!(
        placements(&out).is_empty(),
        "native rate code issued another place: {:?}",
        out
    );
}

#[test]
fn engine_ambiguity_429_signature_code_cannot_resend() {
    let mut e = ready();
    e.handle(NOW + 10., Event::Command(hmac_cancel()));
    let first = requests(&mut e).remove(0);
    e.handle(
        NOW + 11.,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"{"code":20016,"msg":"signature refused"}"#,
            },
        },
    );
    let out = requests(&mut e);
    assert!(
        out.iter().all(|r| r.method == safe_cex_engine::Method::Get),
        "ambiguous status resends: {:?}",
        out
    );
}
