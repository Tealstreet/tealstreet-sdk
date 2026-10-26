//! P1: an Unknown placement fences its symbol until a fresh order read proves
//! what happened. Crypto.com has no placement scope, so this fence engages only
//! with the engine's Phase B uncertainty marker (see RUST_CUTOVER_CALLERS.md).
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event};
use support::*;

fn order(client: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.001,"price":90.0,"clientOrderId":"{client}"}}"#
    ))
    .unwrap()
}

fn place(h: &mut Harness, command: u32, client: &str) {
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![order(client)],
    }));
}

/// Move time forward `by` ms and fire every due timer, so paced writes and polls run.
fn advance(h: &mut Harness, by: f64) {
    h.now += by;
    while let Some(i) = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms <= h.now))
    {
        let Action::SetTimer { timer, .. } = h.actions.remove(i) else {
            unreachable!()
        };
        h.event(Event::TimerFired { timer });
    }
}

fn sent(h: &Harness, path: &str) -> bool {
    h.actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.contains(path)))
}

fn refused_uncertain(h: &Harness) -> bool {
    h.failures
        .iter()
        .any(|e| e.message.contains("Placement outcome is uncertain"))
}

/// Answer one account cycle up to its orders read, leaving that read in flight.
fn poll_until_orders_read(h: &mut Harness) {
    for _ in 0..8 {
        advance(h, 11_000.0);
        h.reply("get-positions", EMPTY);
        h.reply("user-balance", BALANCE);
        if sent(h, "private/get-open-orders") {
            return;
        }
    }
    panic!("no orders read after eight account cycles");
}

#[test]
fn an_unknown_placement_fences_its_symbol_until_a_fresh_order_read() {
    let mut h = Harness::new();
    h.load();
    // An orders read dispatched before the placement: it cannot prove the outcome.
    poll_until_orders_read(&mut h);
    let stale = h.http("private/get-open-orders");

    place(&mut h, 1, "first");
    advance(&mut h, 50.0);
    let write = h.http("private/create-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });

    // Retry on the same symbol: refused NotSent, nothing posted.
    place(&mut h, 2, "retry");
    advance(&mut h, 50.0);
    assert!(
        refused_uncertain(&h),
        "the retry must be refused while the placement is uncertain"
    );
    assert!(
        !sent(&h, "private/create-order"),
        "no write may leave while fenced"
    );

    // The stale read completes cleanly; its regular leg predates the mark.
    h.respond(&stale, EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    h.failures.clear();
    place(&mut h, 3, "retry-after-stale");
    advance(&mut h, 50.0);
    assert!(
        refused_uncertain(&h),
        "a stale read must not release the fence"
    );
    assert!(!sent(&h, "private/create-order"));

    // The owed fresh read follows at once and releases it.
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    h.failures.clear();
    place(&mut h, 4, "retry-after-fresh");
    advance(&mut h, 50.0);
    assert!(
        !refused_uncertain(&h),
        "a fresh clean read releases the fence"
    );
    assert!(
        sent(&h, "private/create-order"),
        "the retry places once released"
    );
}
