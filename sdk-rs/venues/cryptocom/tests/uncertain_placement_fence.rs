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
    // The command finishes Unknown on its next pacing turn, which seals the mark.
    advance(&mut h, 50.0);
    assert!(
        h.failures
            .iter()
            .any(|e| e.outcome == Some(safe_cex_engine::Outcome::Unknown)),
        "the lost write finishes Unknown"
    );
    h.failures.clear();

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

#[test]
fn engine_re_asks_join_the_fresh_read_in_flight_and_its_clean_answer_releases() {
    let mut h = Harness::new();
    h.load();
    place(&mut h, 1, "first");
    advance(&mut h, 50.0);
    let write = h.http("private/create-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
    advance(&mut h, 50.0);
    h.failures.clear();

    // The seal's ask starts one fresh account cycle; its orders read stays unanswered.
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    let fresh = h.http("private/get-open-orders");

    // The engine re-asks at +1, +2 and +4 s: each joins the read in flight.
    for (i, wait) in [1_100.0, 2_100.0, 4_100.0].into_iter().enumerate() {
        advance(&mut h, wait);
        assert!(
            !sent(&h, "get-positions"),
            "re-ask {i} must not start another account read"
        );
        assert!(
            !sent(&h, "private/get-open-orders"),
            "re-ask {i} must not replace the fresh read"
        );
        place(&mut h, 10 + i as u32, "held-retry");
        advance(&mut h, 50.0);
        assert!(
            refused_uncertain(&h),
            "the retry stays refused while the fresh read is held"
        );
        assert!(!sent(&h, "private/create-order"));
        h.failures.clear();
    }

    // The original fresh read answers clean: it releases, and the retry places.
    h.respond(&fresh, EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    place(&mut h, 20, "released-retry");
    advance(&mut h, 50.0);
    assert!(
        !refused_uncertain(&h),
        "the held fresh read releases the fence"
    );
    assert!(
        sent(&h, "private/create-order"),
        "the retry places once released"
    );
}

fn cancel_all(h: &mut Harness, command: u32) {
    h.event(Event::Command(Command::CancelSymbolOrders {
        command,
        symbol: "BTCUSD-PERP".into(),
    }));
}

fn refused_cancel_all(h: &Harness) -> bool {
    h.failures
        .iter()
        .any(|e| e.message.contains("reconcile before cancel-all"))
}

#[test]
fn an_unknown_placement_fences_cancel_all_until_the_fresh_order_read_releases() {
    let mut h = Harness::new();
    h.load();
    place(&mut h, 1, "first");
    advance(&mut h, 50.0);
    let write = h.http("private/create-order");
    h.event(Event::HttpFailed {
        request: write.id,
        error: "response lost after acceptance",
    });
    advance(&mut h, 50.0);
    h.failures.clear();

    // Composed from the store, cancel-all cannot reach the unseen placement: refused NotSent.
    cancel_all(&mut h, 2);
    advance(&mut h, 50.0);
    assert!(
        h.failures
            .iter()
            .any(|e| e.message.contains("reconcile before cancel-all")
                && e.outcome == Some(safe_cex_engine::Outcome::NotSent)),
        "cancel-all is refused NotSent while the placement is uncertain"
    );
    assert!(
        !sent(&h, "cancel-all-orders"),
        "nothing is cancelled while fenced"
    );

    // The fresh account cycle answers clean and releases the fence.
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    h.failures.clear();
    cancel_all(&mut h, 3);
    advance(&mut h, 50.0);
    assert!(
        !refused_cancel_all(&h),
        "a fresh clean read releases cancel-all"
    );
    assert!(
        sent(&h, "cancel-all-orders"),
        "cancel-all is sent once released"
    );
}

fn place_on(h: &mut Harness, command: u32, symbol: &str, client: &str) {
    let mut opts = order(client);
    opts.symbol = symbol.into();
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![opts],
    }));
}

/// P1 (#2282 review): a census is two reads with their own tokens. One whose Orders
/// leg went out before a lost write must not release on its later AdvancedOrders
/// leg's token; it re-censuses wholly after the write instead.
#[test]
fn a_census_whose_orders_leg_predates_a_lost_write_never_releases() {
    let two = INSTRUMENTS.replace(
        r#""underlying_symbol":"BTCUSD-INDEX"}"#,
        r#""underlying_symbol":"BTCUSD-INDEX"},{"symbol":"ETHUSD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"ETH","quote_ccy":"USD","price_tick_size":"0.1","qty_tick_size":"0.001","quote_decimals":1,"quantity_decimals":3,"max_leverage":"50","underlying_symbol":"ETHUSD-INDEX"}"#,
    );
    let mut h = Harness::with_instruments(&two);
    h.load();
    let lose = |h: &mut Harness| {
        advance(h, 50.0);
        let write = h.http("private/create-order");
        h.event(Event::HttpFailed {
            request: write.id,
            error: "response lost after acceptance",
        });
        advance(h, 50.0);
        assert!(
            h.failures
                .iter()
                .any(|e| e.outcome == Some(safe_cex_engine::Outcome::Unknown)),
            "the lost write finishes Unknown"
        );
        h.failures.clear();
    };
    // A first lost write owes a fresh census.
    place(&mut h, 1, "first");
    lose(&mut h);
    // That census's first page, the Orders leg, goes out and is held.
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    let first_page = h.http("private/get-open-orders");

    // A write on another symbol leaves after that page was read, and is lost.
    place_on(&mut h, 2, "ETHUSD-PERP", "second");
    lose(&mut h);

    // The first page (pre-mark) and the last page (post-mark) both answer clean.
    h.respond(&first_page, EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    place_on(&mut h, 3, "ETHUSD-PERP", "held-retry");
    advance(&mut h, 50.0);
    assert!(
        refused_uncertain(&h),
        "a census whose first page predates the write must not release"
    );
    assert!(
        !sent(&h, "private/create-order"),
        "no write may leave while fenced"
    );

    // The owed re-census, wholly after the write, releases it.
    h.reply("get-positions", EMPTY);
    h.reply("user-balance", BALANCE);
    h.reply("private/get-open-orders", EMPTY);
    h.reply("advanced/get-open-orders", EMPTY);
    h.failures.clear();
    place_on(&mut h, 4, "ETHUSD-PERP", "released-retry");
    advance(&mut h, 50.0);
    assert!(!refused_uncertain(&h), "a census after the write releases");
    assert!(
        sent(&h, "private/create-order"),
        "the retry places once released"
    );
}
