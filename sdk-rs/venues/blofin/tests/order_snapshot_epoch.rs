mod support;

use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::Harness;

const EMPTY: &str = r#"{"code":"0","data":[]}"#;
const KEPT: &str = r#"{"instId":"BTC-USDT","orderCategory":"normal","orderId":"kept","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"}"#;

fn reading() -> Harness {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders"}},"data":[{KEPT}]}}"#
    ));
    h.handle(10_000., Event::TimerFired { timer: h.tick });
    h.collect();
    assert_eq!(h.e.store().order("kept").unwrap().status, OrderStatus::Open);
    h
}

fn finish_families(h: &mut Harness) {
    h.reply("orders-tpsl-pending", EMPTY);
    h.reply("orders-algo-pending", EMPTY);
}

#[test]
fn store_write_fence_after_send_rejects_the_whole_census_then_a_fresh_one_publishes() {
    let mut h = reading();
    let first = h.take("orders-pending");
    h.e.store_mut().invalidate_order_snapshots();
    h.respond(first, EMPTY);
    let begin = h.actions.len();
    finish_families(&mut h);
    assert!(
        h.e.store().order("kept").is_some(),
        "a pre-write census must not publish"
    );
    let (timer, at) = h.actions[begin..]
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1_000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("the genuine stale-census retry");
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("orders-pending", EMPTY);
    finish_families(&mut h);
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn a_post_write_last_page_cannot_freshen_the_pre_write_first_page() {
    let mut h = reading();
    let first = h.take("orders-pending");
    let stale = KEPT.replace("kept", "stale");
    h.respond(first, &format!(r#"{{"code":"0","data":[{stale}]}}"#));
    assert!(
        h.e.store().order("stale").is_none(),
        "pages publish atomically"
    );
    h.e.store_mut().invalidate_order_snapshots();
    h.reply("orders-pending", EMPTY);
    finish_families(&mut h);
    assert!(
        h.e.store().order("stale").is_none(),
        "the first page's stamp remains stale"
    );
    assert!(h.e.store().order("kept").is_some());
}

#[test]
fn an_uncrossed_census_keeps_its_stamp_across_pages_and_families() {
    let mut h = reading();
    let fresh = KEPT.replace("kept", "fresh");
    h.reply(
        "orders-pending",
        &format!(r#"{{"code":"0","data":[{fresh}]}}"#),
    );
    assert!(h.e.store().order("fresh").is_none());
    h.reply("orders-pending", EMPTY);
    finish_families(&mut h);
    assert!(h.e.store().order("kept").is_none());
    let order = h.e.store().order("fresh").unwrap();
    assert_eq!(
        (order.amount, order.price, order.status),
        (0.01, 60_000., OrderStatus::Open)
    );
}

#[test]
fn accepted_cancel_is_preserved_when_a_read_started_before_it_finishes() {
    let mut h = reading();
    let first = h.take("orders-pending");
    h.handle(
        10_001.,
        Event::Command(Command::CancelOrders {
            command: 81,
            ids: vec!["kept".into()],
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"kept","code":"0"}]}"#,
    );
    assert_eq!(
        h.e.store().order("kept").unwrap().status,
        OrderStatus::Canceled
    );
    h.respond(first, &format!(r#"{{"code":"0","data":[{KEPT}]}}"#));
    h.reply("orders-pending", EMPTY);
    finish_families(&mut h);
    assert_eq!(
        h.e.store().order("kept").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 81, .. })))
            .count(),
        1
    );
}
