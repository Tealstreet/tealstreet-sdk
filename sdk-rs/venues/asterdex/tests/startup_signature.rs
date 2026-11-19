//! A final V1 signature refusal ends credential validation after its one resend.
mod support;
use safe_cex_engine::{Action, Engine, ErrorKind, Event, HttpRequest, Notice, Response};
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const NOW: f64 = 1_700_000_000_000.;
const REJECTED: &str = r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#;
const REJECTED_STRING: &str =
    r#"{"code":"-1022","msg":"Signature for this request is not valid."}"#;
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000"}]}]}"#;
const BALANCE: &str = r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"}"#;

fn account(request: &HttpRequest) -> bool {
    request.url.split('?').next().unwrap().ends_with("/account")
}

fn reply(
    engine: &mut Engine<AsterDex>,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    engine.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    support::drain(engine, NOW)
}

fn starting(options: Options) -> (Engine<AsterDex>, HttpRequest) {
    let mut engine = support::engine(options);
    engine.start(NOW);
    let mut pending = support::drain(&mut engine, NOW);
    for _ in 0..12 {
        if let Some(at) = pending
            .iter()
            .position(|a| matches!(a,Action::Http(r) if account(r)))
        {
            let Action::Http(request) = pending.remove(at) else {
                unreachable!()
            };
            return (engine, request);
        }
        let mut next = Vec::new();
        for action in pending {
            if let Action::Http(request) = action {
                let path = request.url.split('?').next().unwrap();
                let body = if path.ends_with("/time") {
                    r#"{"serverTime":1700000000000}"#
                } else if path.ends_with("/exchangeInfo") {
                    MARKETS
                } else {
                    "[]"
                };
                next.extend(reply(&mut engine, &request, 200, body));
            }
        }
        pending = next;
    }
    panic!("native startup must issue its signed account read");
}

fn v1() -> Options {
    Options {
        key: Some("synthetic-key".into()),
        secret: Some("synthetic-material".into()),
        ..Default::default()
    }
}

fn retry(actions: &[Action], original: &HttpRequest) -> HttpRequest {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::StartupFailed { .. }))),
        "first V1 refusal keeps the existing resend"
    );
    let reads: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if account(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(reads.len(), 1, "exactly one re-signed account read");
    assert_ne!(reads[0].id, original.id);
    assert!(reads[0].url.contains("signature="));
    reads[0].clone()
}

fn final_refusal(status: u16, body: &str) {
    let (mut engine, first) = starting(v1());
    let resent = retry(&reply(&mut engine, &first, status, body), &first);
    let done = reply(&mut engine, &resent, status, body);
    let failures: Vec<_> = done
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::StartupFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "final signature rejection must terminate startup"
    );
    assert_eq!(failures[0].kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(failures[0].venue_code.as_deref(), Some("-1022"));
    assert!(
        !done
            .iter()
            .any(|a| matches!(a,Action::Http(r) if account(r))),
        "final rejection must not resend again"
    );
}

#[test]
fn final_v1_http_400_signature_rejection_is_startup_auth_invalid() {
    final_refusal(400, REJECTED);
}

#[test]
fn final_v1_http_200_signature_rejection_is_startup_auth_invalid() {
    final_refusal(200, REJECTED);
}

#[test]
fn final_v1_string_code_http_400_signature_rejection_is_startup_auth_invalid() {
    final_refusal(400, REJECTED_STRING);
}

#[test]
fn final_v1_string_code_http_200_signature_rejection_is_startup_auth_invalid() {
    final_refusal(200, REJECTED_STRING);
}

#[test]
fn one_v1_signature_rejection_can_still_recover_on_its_existing_resend() {
    let (mut engine, first) = starting(v1());
    let resent = retry(&reply(&mut engine, &first, 400, REJECTED), &first);
    let done = reply(&mut engine, &resent, 200, BALANCE);
    assert!(engine.store().loaded().balance);
    assert!(
        !done
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::StartupFailed { .. })))
    );
}

#[test]
fn v3_signature_refusal_keeps_its_existing_no_resend_startup_behavior() {
    let (mut engine, first) = starting(Options::v3_wallet());
    let done = reply(&mut engine, &first, 400, REJECTED);
    assert!(done.iter().any(
        |a| matches!(a, Action::Notice(Notice::StartupFailed { error, .. })
        if error.kind == Some(ErrorKind::AuthInvalid))
    ));
    assert!(
        !done
            .iter()
            .any(|a| matches!(a,Action::Http(r) if account(r)))
    );
}

#[test]
fn v3_wallet_signature_refusal_ends_startup_without_waiting_for_timeout() {
    for status in [200, 400] {
        let (mut engine, first) = starting(Options::v3_wallet());
        let done = reply(
            &mut engine,
            &first,
            status,
            r#"{"code":-1000,"msg":"Signature check failed"}"#,
        );
        let failures: Vec<_> = done
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::StartupFailed { error, .. }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1, "one immediate credential refusal");
        assert_eq!(failures[0].kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(
            failures[0].message,
            "Aster API key signature rejected: re-create the key (V3)"
        );
        assert!(
            !done
                .iter()
                .any(|a| matches!(a, Action::Http(r) if account(r)))
        );
    }
}

fn listen_key_refusal_during_account_read(status: u16) {
    let mut engine = support::engine(v1());
    engine.start(NOW);
    let mut pending = support::drain(&mut engine, NOW);
    let mut account_read = None;
    let mut listen_key = None;
    for _ in 0..12 {
        let mut next = Vec::new();
        for action in pending {
            if let Action::Http(request) = action {
                let path = request.url.split('?').next().unwrap();
                if account(&request) {
                    account_read = Some(request);
                } else if path.ends_with("/listenKey") {
                    listen_key = Some(request);
                } else {
                    let body = if path.ends_with("/time") {
                        r#"{"serverTime":1700000000000}"#
                    } else if path.ends_with("/exchangeInfo") {
                        MARKETS
                    } else {
                        "[]"
                    };
                    next.extend(reply(&mut engine, &request, 200, body));
                }
            }
        }
        pending = next;
        if account_read.is_some() && listen_key.is_some() {
            break;
        }
    }
    let account_read = account_read.expect("account read held pending");
    let first = listen_key.expect("listen-key session write while startup is unfinished");
    assert_eq!(first.method, safe_cex_engine::Method::Post);
    assert!(!engine.store().loaded().balance);
    let resent: Vec<_> = reply(&mut engine, &first, status, REJECTED)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.contains("/listenKey?") => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(
        resent.len(),
        1,
        "existing listen-key signature resend remains"
    );
    let done = reply(&mut engine, &resent[0], status, REJECTED);
    assert!(
        !done
            .iter()
            .any(|action| matches!(action, Action::Notice(Notice::StartupFailed { .. }))),
        "listen-key session failure must not end pending credential validation"
    );
    let done = reply(&mut engine, &account_read, 200, BALANCE);
    assert!(
        engine.store().loaded().balance,
        "the held account read can still complete"
    );
    assert!(
        !done
            .iter()
            .any(|action| matches!(action, Action::Notice(Notice::StartupFailed { .. })))
    );
}

#[test]
fn listen_key_http_400_refusal_keeps_pending_account_startup_alive() {
    listen_key_refusal_during_account_read(400);
}

#[test]
fn listen_key_http_200_refusal_keeps_pending_account_startup_alive() {
    listen_key_refusal_during_account_read(200);
}
