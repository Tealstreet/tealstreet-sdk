mod support;
use safe_cex_engine::{Command, Event};
use support::*;

#[test]
fn stale_join_cannot_unfence_lost_placement_but_fresh_complete_read_can() {
    let mut h = Harness::classic();
    // This join started before the mutation, and its normal source is already accepted.
    h.respond(
        "/orders-pending",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    let held_protection = h.take("planType=profit_loss");
    h.command(Command::PlaceOrders {
        command: 31,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90}"#,
            )
            .unwrap(),
        ],
    });
    let write = h.take("/batch-place-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 31 && result.is_err())
    );
    h.command(Command::CancelSymbolOrders {
        command: 32,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 32 && result.is_err())
    );
    // Even though the last page would be dispatched after failure, this whole join is stale.
    h.reply(
        held_protection.id,
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    assert!(
        h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "stale join must restart from its normal source"
    );
    h.command(Command::CancelSymbolOrders {
        command: 33,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 33 && result.is_err())
    );
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    h.command(Command::CancelSymbolOrders {
        command: 34,
        symbol: "BTCUSDT".into(),
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 34 && result.is_ok()),
        "accepted fresh complete order census must clear uncertainty"
    );
}

fn place(h: &mut Harness, command: u32) {
    h.command(Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":90}"#,
            )
            .unwrap(),
        ],
    });
}

/// The command was refused before entry, NotSent, for the uncertain fence.
fn fenced(h: &Harness, command: u32) -> bool {
    h.done.iter().any(|(id, result)| {
        *id == command && result.as_ref().is_err_and(|e| e.contains("uncertain"))
    }) && h.failures.iter().any(|error| {
        error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            && error.legacy_text().contains("uncertain")
    })
}

#[test]
fn an_unknown_placement_fences_a_retry_until_a_fresh_read_releases_it() {
    let mut h = Harness::classic();
    // An order census that started before the write: it can never release it.
    h.respond(
        "/orders-pending",
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    let stale = h.take("planType=profit_loss");
    place(&mut h, 31);
    let write = h.take("/batch-place-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
    assert!(
        h.done
            .iter()
            .any(|(id, result)| *id == 31 && result.is_err()),
        "the lost placement seals Unknown"
    );
    // A user retry on the same symbol cannot duplicate the possibly placed order.
    place(&mut h, 32);
    assert!(fenced(&h, 32), "{:?}", h.done);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
    // The stale census completes and restarts; the fence holds.
    h.reply(
        stale.id,
        200,
        r#"{"code":"00000","data":{"entrustedList":[]}}"#,
    );
    h.failures.clear();
    place(&mut h, 33);
    assert!(fenced(&h, 33), "a stale read never releases: {:?}", h.done);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
    // A fresh complete census releases it, and a placement is sent again.
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    place(&mut h, 34);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order")),
        "a fresh read clears the fence: {:?}",
        h.done
    );
}

#[test]
fn an_unknown_placement_starts_a_fresh_order_census() {
    let mut h = Harness::classic();
    // Let any startup census settle first, so the next one is the reconcile.
    while h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    }
    place(&mut h, 41);
    let write = h.take("/batch-place-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
    assert!(
        h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "a fresh order census follows the Unknown placement: {:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    place(&mut h, 42);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
}

/// One Unknown BTCUSDT placement, with every earlier census settled first.
fn unknown_placement(h: &mut Harness) {
    while h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    }
    place(h, 51);
    let write = h.take("/batch-place-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
}

#[test]
fn a_census_with_a_rejected_row_keeps_the_fence() {
    let mut h = Harness::classic();
    unknown_placement(&mut h);
    // A malformed row could be the possibly placed order: the census is not clean.
    h.orders(r#"{"code":"00000","data":{"entrustedList":[{"symbol":"BTCUSDT"}]}}"#);
    h.failures.clear();
    place(&mut h, 52);
    assert!(
        fenced(&h, 52),
        "an unclean census never releases: {:?}",
        h.done
    );
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order"))
    );
}

#[test]
fn a_failed_census_is_re_asked_and_the_fresh_one_releases() {
    let mut h = Harness::classic();
    unknown_placement(&mut h);
    h.respond("/orders-pending", 500, "");
    while h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.respond("/orders-pending", 500, "");
    }
    h.failures.clear();
    place(&mut h, 53);
    assert!(fenced(&h, 53), "a failed census keeps the fence");
    // The engine re-asks reconcile_uncertain after 1 s; the hook starts a census.
    h.timer_after(1_000.0);
    assert!(
        h.requests.iter().any(|r| r.url.contains("/orders-pending")),
        "the re-ask starts a fresh census: {:?}",
        h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    place(&mut h, 54);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order")),
        "the fresh census releases: {:?}",
        h.done
    );
}

/// 8x5tm1 re-ask hazard: engine re-asks while a slow (>1 s) census is out join
/// it rather than replacing it, and that census still releases.
#[test]
fn engine_re_asks_join_a_slow_census_that_still_releases() {
    let mut h = Harness::classic();
    unknown_placement(&mut h);
    let census = |h: &Harness| -> Vec<_> {
        h.requests
            .iter()
            .filter(|r| r.url.contains("/orders-pending"))
            .map(|r| r.id)
            .collect()
    };
    let out = census(&h);
    assert!(!out.is_empty(), "the reconcile census");
    for delay in [1_000.0, 2_000.0, 4_000.0] {
        h.timer_after(delay);
    }
    assert_eq!(census(&h), out, "re-asks join the census still out");
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    place(&mut h, 55);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/batch-place-order")),
        "the slow census releases: {:?}",
        h.done
    );
}
