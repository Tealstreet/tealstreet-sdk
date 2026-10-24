mod support;

use safe_cex_engine::model::{BybitBbo, PlaceOrderOpts};
use safe_cex_engine::{
    Action, Command, ErrorKind, Event, Method, Notice, Outcome, Response, VenueError,
};
use safe_cex_venue_orderly::{Options, Orderly};
use support::Driver;

const COMMAND: u32 = 71;
const MESSAGE: &str = "Bybit BBO and TickSize controls are not supported by Orderly";

#[derive(Clone, Copy)]
enum Control {
    Bbo,
    Ticks,
}
fn order() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000,"clientOrderId":"fixture-order"}"#).unwrap()
}
fn bad(control: Control) -> PlaceOrderOpts {
    let mut bad = order();
    bad.client_order_id = Some("fixture-bad".into());
    match control {
        Control::Bbo => {
            bad.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.,
            })
        }
        Control::Ticks => bad.bybit_slippage_tolerance_ticks = Some(1.),
    }
    bad
}
fn public() -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.);
    let requests: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    let catalogue = requests
        .iter()
        .find(|r| r.url.ends_with("/v1/public/info"))
        .unwrap();
    e.handle(
        0.,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let _ = e.drain_actions().count();
    assert!(!e.store().loaded().orders);
    e
}
fn ready(busy: bool) -> (Driver, Option<safe_cex_engine::HttpRequest>) {
    let (mut e, _) = support::ready();
    let _ = e.drain_actions().count();
    assert!(
        e.store().loaded().orders && e.store().loaded().positions && e.store().loaded().balance
    );
    assert!(!e.store().orders().is_empty());
    assert!(!e.store().positions().is_empty());
    let request = if busy {
        e.handle(
            2000.,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order()],
            }),
        );
        Some(
            e.drain_actions()
                .find_map(|a| match a {
                    Action::Http(r) => Some(r),
                    _ => None,
                })
                .expect("working native active write"),
        )
    } else {
        None
    };
    (e, request)
}
fn run(e: &mut Driver, now: f64, orders: Vec<PlaceOrderOpts>) -> Vec<Action> {
    let before = e.snapshot_frame(2000.).to_vec();
    let _ = e.take_frame(2000.);
    e.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command: COMMAND,
            orders,
        }),
    );
    let out = e.drain_actions().collect();
    assert_eq!(
        e.snapshot_frame(2000.),
        before,
        "unsupported batch cannot mutate financial/public Store"
    );
    assert!(
        e.take_frame(2000.).is_none(),
        "unsupported batch dirtied Store"
    );
    out
}
fn failure(out: &[Action]) -> &VenueError {
    match out {
        [
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }),
        ] => {
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            error
        }
        _ => panic!("one correlated failure, no HTTP/managed/sign/queue/timer effects: {out:?}"),
    }
}
fn native(e: &mut Driver, orders: Vec<PlaceOrderOpts>) {
    let out = run(e, 2000., orders);
    let error = failure(&out);
    assert_eq!(error.message, MESSAGE);
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.venue_code, None);
    assert_eq!(error.partial, None);
}
fn unstamped(control: Control) {
    let (mut e, _) = ready(false);
    let out = run(&mut e, f64::NAN, vec![order(), bad(control)]);
    assert_eq!(
        failure(&out).kind,
        Some(ErrorKind::InvalidInput),
        "unstamped genuine Mutation entry must reject through shared entry custody"
    );
}
#[test]
fn unstamped_bbo_batch_uses_shared_mutation_entry_rejection() {
    unstamped(Control::Bbo);
}
#[test]
fn unstamped_tick_batch_uses_shared_mutation_entry_rejection() {
    unstamped(Control::Ticks);
}
#[test]
fn native_bbo_refusal_precedes_keyless_account_readiness() {
    native(&mut public(), vec![bad(Control::Bbo)]);
}
#[test]
fn native_ticks_refusal_precedes_keyless_account_readiness() {
    native(&mut public(), vec![bad(Control::Ticks)]);
}
#[test]
fn native_later_bbo_preserves_idle_financial_state() {
    native(&mut ready(false).0, vec![order(), bad(Control::Bbo)]);
}
#[test]
fn native_later_ticks_preserve_idle_financial_state() {
    native(&mut ready(false).0, vec![order(), bad(Control::Ticks)]);
}
#[test]
fn native_later_bbo_refuses_while_a_real_write_is_active() {
    native(&mut ready(true).0, vec![order(), bad(Control::Bbo)]);
}
#[test]
fn native_later_ticks_refuse_while_a_real_write_is_active() {
    native(&mut ready(true).0, vec![order(), bad(Control::Ticks)]);
}
#[test]
fn native_nonfinite_tick_payload_is_unsupported_at_finite_entry() {
    let (mut e, _) = ready(false);
    for ticks in [0., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut bad = bad(Control::Ticks);
        bad.bybit_slippage_tolerance_ticks = Some(ticks);
        native(&mut e, vec![order(), bad]);
    }
}
#[test]
fn ordinary_keyless_order_still_reaches_native_account_guard() {
    let out = run(&mut public(), 2000., vec![order()]);
    let error = failure(&out);
    assert_eq!(error.message, "Orderly private account is not ready");
    assert_eq!(error.kind, None);
}
#[test]
fn unsupported_batch_does_not_queue_behind_or_block_the_native_active_write() {
    let (mut e, active) = ready(true);
    let active = active.unwrap();
    native(&mut e, vec![order(), bad(Control::Bbo)]);
    e.handle(
        2001.,
        Event::HttpResponse {
            request: active.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"data":{"order_id":9007199254740995}}"#,
            },
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(&out[..], [Action::Notice(Notice::CommandDone { command: 1, result })] if result == r#"["9007199254740995"]"#),
        "no delayed unsupported job or physical work: {out:?}"
    );
    e.handle(
        2002.,
        Event::Command(Command::PlaceOrders {
            command: 72,
            orders: vec![order()],
        }),
    );
    let out: Vec<_> = e.drain_actions().collect();
    let requests: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "native successor remains reachable: {out:?}"
    );
    assert_eq!(requests[0].method, Method::Post);
    assert!(requests[0].url.ends_with("/v1/order"));
    let body: serde_json::Value =
        serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["symbol"], "PERP_BTC_USDC");
    assert_eq!(body["client_order_id"], "fixture-order");
}
