//! Original admission boundaries for the unsigned account-setting poll.
mod support;
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_txflow::{Options, TxFlow};

const START: f64 = 1000.;
const ADMITTED: f64 = START + 25.;
const DEADLINE: f64 = ADMITTED + 20_000.;
const WALLET: &str = "0x0000000000000000000000000000000000000001";
type Driver = Engine<TxFlow>;
fn is_setting(r: &HttpRequest) -> bool {
    r.body
        .as_deref()
        .is_some_and(|b| b.contains("activeAssetData"))
}
fn answer(e: &mut Driver, r: &HttpRequest, now: f64, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn private_at_settings() -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        application_id: Some(WALLET.into()),
        secret: Some(format!("0x{:064x}", 1)),
        ..Default::default()
    }));
    e.start(START);
    let metadata = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    answer(&mut e, &metadata, START, 200, support::META);
    let ticker = safe_cex_engine::test_support::prepared_actions(&mut e, START)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r)
                if r.body
                    .as_deref()
                    .is_some_and(|b| b.contains("activeAssetCtx")) =>
            {
                Some(r)
            }
            _ => None,
        })
        .expect("native accepted seed before account start");
    answer(&mut e, &ticker, START, 200, support::TICKER);
    let cadence = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == ADMITTED => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(ADMITTED, Event::TimerFired { timer: cadence });
    assert!(e.store().loaded().tickers);
    e
}
fn setting(e: &mut Driver) -> HttpRequest {
    let mut requests = safe_cex_engine::test_support::prepared_actions(e, ADMITTED)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) if is_setting(&r) => Some(r),
            _ => None,
        });
    let r = requests.next().expect("one physical native setting read");
    assert!(requests.next().is_none());
    r
}
fn body(user: &str, coin: &str, value: u32) -> String {
    serde_json::json!({"success":true,"code":0,"data":{"user":user,"coin":coin,"leverage":{"mode":"oneWay","type":"Cross","value":value}}}).to_string()
}
fn no_seven(e: &Driver) {
    assert!(
        e.store()
            .setting("BTC-USDC")
            .is_none_or(|s| s.leverage != 7.)
    );
    assert!(e.store().positions().is_empty());
}
fn cadence(a: &[Action], now: f64) {
    assert_eq!(
        a.iter()
            .filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == now+100.))
            .count(),
        1
    );
}
fn no_command_or_signing(a: &[Action]) {
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Sign { .. }
            | Action::SignPayload { .. }
            | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
#[test]
fn setting_requires_host_preparation_before_http() {
    let mut e = private_at_settings();
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(r) if is_setting(r))),
        "setting escaped before preparation"
    );
    assert!(a.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { request, .. } if is_setting(request)))));
    no_command_or_signing(&a);
}
#[test]
fn unprepared_setting_has_its_original_admission_deadline() {
    let mut e = private_at_settings();
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == DEADLINE)),
        "setting has no pre-Prepare lifetime bound"
    );
    no_seven(&e);
}
#[test]
fn exact_deadline_setting_data_cannot_publish_leverage() {
    let mut e = private_at_settings();
    let r = setting(&mut e);
    no_seven(&e);
    answer(&mut e, &r, DEADLINE, 200, &body(WALLET, "1", 7));
    no_seven(&e);
    let a: Vec<_> = e.drain_actions().collect();
    cadence(&a, DEADLINE);
}
#[test]
fn native_unsigned_account_route_body_and_header_order_are_preserved() {
    let mut e = private_at_settings();
    let r = setting(&mut e);
    assert_eq!(r.method, Method::Post);
    assert_eq!(r.url, "https://rpc.txflow-testnet.xyz/info");
    assert!(r.route.is_none());
    assert_eq!(
        r.headers,
        vec![
            ("Content-Type", "application/json".into()),
            ("x-api-version", "2026-08-16".into())
        ]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap(),
        serde_json::json!({"type":"activeAssetData","user":WALLET,"coin":"1"})
    );
}
#[test]
fn accepted_native_setting_publishes_without_inventing_positions() {
    let mut e = private_at_settings();
    let r = setting(&mut e);
    answer(&mut e, &r, ADMITTED + 1., 200, &body(WALLET, "1", 7));
    let s = e.store().setting("BTC-USDC").unwrap();
    assert_eq!(s.leverage, 7.);
    assert!(e.store().positions().is_empty());
    let a: Vec<_> = e.drain_actions().collect();
    cadence(&a, ADMITTED + 1.);
    no_command_or_signing(&a);
}
#[test]
fn foreign_subject_cannot_change_settings_and_retains_native_cadence() {
    let mut e = private_at_settings();
    let r = setting(&mut e);
    answer(
        &mut e,
        &r,
        ADMITTED + 1.,
        200,
        &body("0x0000000000000000000000000000000000000002", "1", 7),
    );
    no_seven(&e);
    cadence(&e.drain_actions().collect::<Vec<_>>(), ADMITTED + 1.);
}
#[test]
fn foreign_coin_cannot_change_settings_and_retains_native_cadence() {
    let mut e = private_at_settings();
    let r = setting(&mut e);
    answer(&mut e, &r, ADMITTED + 1., 200, &body(WALLET, "2", 7));
    no_seven(&e);
    cadence(&e.drain_actions().collect::<Vec<_>>(), ADMITTED + 1.);
}
#[test]
fn malformed_envelope_and_http_refusal_keep_settings_unpublished() {
    for (status, b) in [
        (200, "{"),
        (503, "unavailable"),
        (200, r#"{"success":false,"code":1,"data":{}}"#),
    ] {
        let mut e = private_at_settings();
        let r = setting(&mut e);
        answer(&mut e, &r, ADMITTED + 1., status, b);
        no_seven(&e);
        cadence(&e.drain_actions().collect::<Vec<_>>(), ADMITTED + 1.);
    }
}
#[test]
fn transport_failure_keeps_one_hundred_ms_successor_without_publication() {
    let mut e = private_at_settings();
    let r = setting(&mut e);
    e.handle(
        ADMITTED + 1.,
        Event::HttpFailed {
            request: r.id,
            error: "native setting transport",
        },
    );
    no_seven(&e);
    let a: Vec<_> = e.drain_actions().collect();
    cadence(&a, ADMITTED + 1.);
    no_command_or_signing(&a);
}

#[test]
fn native_setting_refresh_can_run_a_minute_after_the_first_read() {
    let mut e = private_at_settings();
    let initial = safe_cex_engine::test_support::prepared_actions(&mut e, ADMITTED);
    let refresh = initial
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == ADMITTED + 5000. => Some(*timer),
            _ => None,
        })
        .expect("native account poll timer");
    let first = initial
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if is_setting(&r) => Some(r),
            _ => None,
        })
        .unwrap();
    answer(&mut e, &first, ADMITTED + 1., 200, &body(WALLET, "1", 7));
    let successor = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == ADMITTED + 101. => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(ADMITTED + 101., Event::TimerFired { timer: successor });
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut e, ADMITTED + 101.)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if is_setting(r)))
    );
    e.handle(ADMITTED + 60_000., Event::TimerFired { timer: refresh });
    let renewed = safe_cex_engine::test_support::prepared_actions(&mut e, ADMITTED + 60_000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if is_setting(&r) => Some(r),
            _ => None,
        })
        .expect("native minute refresh uses another physical setting read");
    assert_ne!(first.id, renewed.id);
    assert_eq!(first.body, renewed.body);
    assert_eq!(e.store().setting("BTC-USDC").unwrap().leverage, 7.);
}
