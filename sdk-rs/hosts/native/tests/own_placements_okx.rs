//! AGT-333 through the real OKX engine: TP/SL legs placed with an order, and
//! a full-position leg that joins another login's held algo. Receipts,
//! completions, `OrderTriggered` notices and fills go through the same ledger
//! calls the account actor makes, for the placing login and for a second login
//! sharing the venue account.
//!
//! This SDK never sends OKX `attachAlgoOrds`: an order's stop-loss and
//! take-profit are their own `order-algo` writes, so each leg's `algoId` is in
//! its own acknowledgement. The acks follow OKX's published "Place order",
//! "Place algo order" and "Amend algo order" responses; the triggered child is
//! the published `orders` push example with only the documented triggered-child
//! fields changed (as in `venues/okx/tests/account.rs`). Docs-derived, not live
//! captures.
#![cfg(not(target_arch = "wasm32"))]
#![allow(dead_code)]

#[path = "../../../venues/okx/tests/support/mod.rs"]
mod support;

use safe_cex_engine::{Action, Command, Engine, Event, Fill, Method, Notice};
use safe_cex_native::own_placements::OwnPlacements;
use safe_cex_venue_okx::Okx;
use support::*;

const SYMBOL: &str = "BTCUSDT";
const SL_ALGO: &str = "452197707845865000";
const TP_ALGO: &str = "452197707845865001";
const CHILD: &str = "452197707845865472";

/// OKX's published `orders` push example as a filled stop-market child of
/// `algo` (`algoId`, `source` 7, `ordType` market), as in the venue tests.
fn triggered_child(algo: &str) -> String {
    format!(
        r#"{{"arg":{{"channel":"orders","instType":"SWAP","instId":"BTC-USDT-SWAP","uid":"614488474791936"}},"data":[{{"accFillSz":"1","algoClOrdId":"","algoId":"{algo}","amendResult":"","amendSource":"","avgPx":"31527.1","cancelSource":"","category":"normal","ccy":"","clOrdId":"","code":"0","cTime":"1654084334977","execType":"T","fee":"-0.02522168","feeCcy":"USDT","fillFee":"-0.02522168","fillFeeCcy":"USDT","fillNotionalUsd":"31.50818374","fillPx":"31527.1","fillSz":"1","fillPnl":"0.01","fillTime":"1654084353263","instId":"BTC-USDT-SWAP","instType":"SWAP","lever":"10","msg":"","notionalUsd":"31.50818374","ordId":"{CHILD}","ordType":"market","pnl":"0","posSide":"net","px":"","rebate":"0","rebateCcy":"USDT","reduceOnly":"true","reqId":"","side":"sell","attachAlgoClOrdId":"","slOrdPx":"","slTriggerPx":"","slTriggerPxType":"last","source":"7","state":"filled","stpId":"","stpMode":"","sz":"1","tag":"","tdMode":"cross","tgtCcy":"","tpOrdPx":"","tpTriggerPx":"","tpTriggerPxType":"last","attachAlgoOrds":[],"tradeId":"242589207","uTime":"1654084353264","isTpLimit":"false","linkedAlgoOrd":{{"algoId":""}}}}]}}"#
    )
}

type Socket = safe_cex_engine::SocketId;

/// A ready account with authenticated private and business sockets.
fn ready() -> (Engine<Okx>, Socket, Socket) {
    let mut e = engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..FixtureOptions::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let mut initial = prepared(&mut e, 1.0);
    let a = take_http(&mut initial);
    reply(&mut e, &a, &envelope(TICKER), 2.0);
    initial.extend(prepared(&mut e, 2.0));
    let socket = |actions: &[Action], suffix: &str| {
        actions.iter().find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with(suffix) => Some(*socket),
            _ => None,
        })
    };
    let business = socket(&initial, "/business").unwrap();
    let config = initial
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let mut reads = prepared(&mut e, 3.0);
    let private = socket(&reads, "/private").unwrap();
    for socket in [private, business] {
        e.handle(4.0, Event::WsOpened { socket });
        reads.extend(prepared(&mut e, 4.0));
        ws(&mut e, socket, r#"{"event":"login","code":"0","msg":""}"#);
        let subscribed = prepared(&mut e, 4.0);
        reads.extend(release_paced(&mut e, 4.0, subscribed));
    }
    while let Some(a) = reads.pop() {
        let Action::Http(r) = &a else {
            continue;
        };
        let body = if r.url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut e, &a, &body, 5.0);
        reads.extend(prepared(&mut e, 5.0));
    }
    (e, private, business)
}

fn place(e: &mut Engine<Okx>, order: &str) -> Vec<Action> {
    e.handle(
        6.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![serde_json::from_str(order).unwrap()],
        }),
    );
    prepared(e, 6.0)
}

fn write(actions: &[Action]) -> &Action {
    actions
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        .unwrap_or_else(|| panic!("a write: {actions:?}"))
}

/// Answer each write with its ack in turn; every action the command produced.
fn acknowledge(e: &mut Engine<Okx>, mut out: Vec<Action>, acks: &[(&str, &str)]) -> Vec<Action> {
    let mut all = out.clone();
    for (index, (path, ack)) in acks.iter().enumerate() {
        let request = write(&out).clone();
        assert!(
            matches!(&request, Action::Http(r) if r.url.contains(path)),
            "{path}: {request:?}"
        );
        let at = 7.0 + index as f64;
        reply(e, &request, &envelope(ack), at);
        out = prepared(e, at);
        all.extend(out.iter().cloned());
    }
    all
}

/// What the placing login's actor records for command 1 (no caller ids).
fn record(ledger: &mut OwnPlacements, actions: &[Action], held: &[String]) {
    let symbols = [SYMBOL.to_owned()];
    let legs = [(SYMBOL.to_owned(), None)];
    for action in actions {
        match action {
            Action::Notice(Notice::CommandProgress {
                command: 1,
                receipts,
            }) => {
                ledger.record_receipts(&symbols, &legs, receipts);
            }
            Action::Notice(Notice::CommandDone { command: 1, result }) => {
                ledger.record_completion(&symbols, &legs, result, held);
                ledger.settle(&symbols, Some(result));
            }
            _ => {}
        }
    }
}

/// The child's fill, after both logins applied its `OrderTriggered` link.
fn fire(
    e: &mut Engine<Okx>,
    private: safe_cex_engine::SocketId,
    algo: &str,
    ledgers: [&mut OwnPlacements; 2],
) -> Vec<Fill> {
    e.take_frame(20.0);
    ws(e, private, &triggered_child(algo));
    let actions = prepared(e, 20.0);
    let links: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::OrderTriggered {
                symbol,
                order_id,
                by,
            }) => Some((symbol, order_id, by)),
            _ => None,
        })
        .collect();
    assert_eq!(links.len(), 1, "the child names its algo: {actions:?}");
    let (symbol, order_id, by) = links[0];
    for ledger in ledgers {
        ledger.link(symbol, order_id, by);
    }
    e.take_frame(21.0)
        .and_then(safe_cex_engine::wire::fills)
        .unwrap_or_default()
}

/// The stop-loss and take-profit placed with a limit are separate algos whose
/// ids the placing login records from their acks; the child either one's
/// trigger creates is claimed by that login, never by the other.
#[test]
fn a_brackets_triggered_leg_is_claimed_by_the_placing_login_only() {
    for (leg, algo) in [("stop-loss", SL_ALGO), ("take-profit", TP_ALGO)] {
        let (mut e, private, _) = ready();
        let out = place(
            &mut e,
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":38000,"stopLoss":36000,"takeProfit":42000}"#,
        );
        let actions = acknowledge(
            &mut e,
            out,
            &[
                (
                    "/batch-orders",
                    r#"{"clOrdId":"","ordId":"612","tag":"","ts":"1695190491421","sCode":"0","sMsg":""}"#,
                ),
                (
                    "/order-algo",
                    &format!(
                        r#"{{"algoClOrdId":"","algoId":"{SL_ALGO}","clOrdId":"","sCode":"0","sMsg":"","tag":""}}"#
                    ),
                ),
                (
                    "/order-algo",
                    &format!(
                        r#"{{"algoClOrdId":"","algoId":"{TP_ALGO}","clOrdId":"","sCode":"0","sMsg":"","tag":""}}"#
                    ),
                ),
            ],
        );
        let mut placer = OwnPlacements::default();
        let mut other = OwnPlacements::default();
        record(&mut placer, &actions, &[]);
        assert!(placer.placed(SYMBOL, algo), "{leg}: its ack's algoId");
        let fills = fire(&mut e, private, algo, [&mut placer, &mut other]);
        assert_eq!(fills.len(), 1, "{leg}");
        assert_eq!(fills[0].order_id, CHILD);
        assert!(placer.placed(SYMBOL, CHILD), "{leg}: the placer claims it");
        assert!(
            !other.placed(SYMBOL, CHILD),
            "{leg}: the other login does not"
        );
    }
}

/// OKX `orders-algo` rows for full-position algo `77` (as in the venue's
/// AGT-98 test): its stop alone, then the `oco` a joined take-profit makes.
const STOP_ONLY: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"77","ordType":"conditional","state":"live","side":"sell","sz":"","closeFraction":"1","slTriggerPx":"36000","slOrdPx":"-1","slTriggerPxType":"mark","reduceOnly":"true"}"#;
const JOINED: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"77","ordType":"oco","state":"live","side":"sell","sz":"","closeFraction":"1","slTriggerPx":"36000","slOrdPx":"-1","slTriggerPxType":"mark","tpTriggerPx":"42000","tpOrdPx":"-1","tpTriggerPxType":"last","reduceOnly":"true"}"#;
const FULL_SL: &str =
    r#"{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":36000}"#;
const FULL_TP: &str =
    r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":42000}"#;
const JOIN_ACK: &str = r#"{"algoClOrdId":"","algoId":"77","reqId":"","sCode":"0","sMsg":""}"#;

fn algo_row(e: &mut Engine<Okx>, business: Socket, row: &str) {
    ws(
        e,
        business,
        &format!(r#"{{"arg":{{"channel":"orders-algo"}},"data":[{row}]}}"#),
    );
    let _ = prepared(e, 10.0);
}

/// What the actor's `observe_legs` sees: this resident's store, with its own
/// command on the symbol in flight or not.
fn observe(ledger: &mut OwnPlacements, e: &Engine<Okx>, in_flight: bool) {
    ledger.observe_legs(e.store().orders(), |_| in_flight);
}

/// Login A places full-position stop `77`; it is A's, and its child is A's.
fn a_places_the_stop() -> (Engine<Okx>, Socket, Socket, OwnPlacements) {
    let (mut e, private, business) = ready();
    let out = place(&mut e, FULL_SL);
    let actions = acknowledge(
        &mut e,
        out,
        &[(
            "/order-algo",
            r#"{"algoClOrdId":"","algoId":"77","clOrdId":"","sCode":"0","sMsg":"","tag":""}"#,
        )],
    );
    let mut ledger = OwnPlacements::default();
    record(&mut ledger, &actions, &[]);
    observe(&mut ledger, &e, false);
    algo_row(&mut e, business, STOP_ONLY);
    observe(&mut ledger, &e, false);
    assert!(ledger.placed(SYMBOL, "77"));
    (e, private, business, ledger)
}

/// Review A/B blocker: B's full TP joins A's full stop `77`, which becomes
/// an `oco` of A's SL and B's TP. The child of either leg names only `77`,
/// so neither login may claim it: B records nothing for the joined algo, and
/// A's resident, seeing `77` gain a leg no command of its own added, stops
/// linking its children. Both logins are notified (fail open).
#[test]
fn a_joined_algo_is_claimed_by_neither_login() {
    let (mut a, a_private, a_business, mut a_ledger) = a_places_the_stop();
    let (mut b, b_private, b_business) = ready();
    algo_row(&mut b, b_business, STOP_ONLY);
    let mut b_ledger = OwnPlacements::default();
    let held = b_ledger.held_rows(&[SYMBOL.into()], b.store().orders());
    assert_eq!(held, ["77_sl"]);
    let out = place(&mut b, FULL_TP);
    let actions = acknowledge(&mut b, out, &[("/amend-algos", JOIN_ACK)]);
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { command: 1, result }) if result == r#"["77"]"#
        )),
        "{actions:?}"
    );
    record(&mut b_ledger, &actions, &held);
    assert!(!b_ledger.placed(SYMBOL, "77"), "the joiner never claims 77");
    // Each resident's stream now carries the two-leg algo.
    for (e, business) in [(&mut a, a_business), (&mut b, b_business)] {
        algo_row(e, business, JOINED);
    }
    observe(&mut a_ledger, &a, false);
    observe(&mut b_ledger, &b, false);
    let mut nobody = OwnPlacements::default();
    let a_fills = fire(&mut a, a_private, "77", [&mut a_ledger, &mut nobody]);
    let b_fills = fire(&mut b, b_private, "77", [&mut b_ledger, &mut nobody]);
    assert_eq!((a_fills.len(), b_fills.len()), (1, 1));
    assert!(!a_ledger.placed(SYMBOL, CHILD), "A never claims B's leg");
    assert!(!b_ledger.placed(SYMBOL, CHILD), "B never claims A's leg");
}

/// A's own TP joins A's own stop: the leg appears while A's command is in
/// flight and the completion names `77`, so A still claims its child. The
/// other login never does.
#[test]
fn joining_this_logins_own_algo_keeps_it_claimed() {
    let (mut e, private, business, mut own) = a_places_the_stop();
    let held = own.held_rows(&[SYMBOL.into()], e.store().orders());
    assert!(held.is_empty(), "{held:?}");
    let out = place(&mut e, FULL_TP);
    let request = write(&out).clone();
    // The two-leg row may arrive before the join's acknowledgement.
    algo_row(&mut e, business, JOINED);
    observe(&mut own, &e, true);
    reply(&mut e, &request, &envelope(JOIN_ACK), 8.0);
    let actions = prepared(&mut e, 8.0);
    record(&mut own, &actions, &held);
    observe(&mut own, &e, false);
    let mut other = OwnPlacements::default();
    fire(&mut e, private, "77", [&mut own, &mut other]);
    assert!(own.placed(SYMBOL, CHILD) && !other.placed(SYMBOL, CHILD));
}

/// A single-leg algo nobody joined stays A's: the control for the above.
#[test]
fn an_unjoined_full_position_stop_stays_claimed_by_its_placer() {
    let (mut e, private, _, mut own) = a_places_the_stop();
    let mut other = OwnPlacements::default();
    fire(&mut e, private, "77", [&mut own, &mut other]);
    assert!(own.placed(SYMBOL, CHILD) && !other.placed(SYMBOL, CHILD));
}
