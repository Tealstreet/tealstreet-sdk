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

use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, TimeInForce};
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

/// A ready account with authenticated private and business sockets.
fn ready() -> (Engine<Okx>, safe_cex_engine::SocketId) {
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
    (e, private)
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
        let (mut e, private) = ready();
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

fn held_stop(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: SYMBOL.into(),
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        price: 36000.0,
        amount: 0.0,
        filled: 0.0,
        remaining: 0.0,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 4.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

/// Another login's full-position stop `77` is held. This login's full TP
/// joins it through amend-algos, which completes with `77`: that is not a
/// placement here, so a later trigger of `77` is claimed by neither login
/// (fail open), never by the one that only joined it.
#[test]
fn joining_another_logins_full_position_algo_claims_nothing() {
    let (mut e, private) = ready();
    e.store_mut().upsert_order(held_stop("77_sl"));
    let mut joiner = OwnPlacements::default();
    let mut placer = OwnPlacements::default();
    let held = joiner.held_rows(&[SYMBOL.into()], e.store().orders());
    let out = place(
        &mut e,
        r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":42000}"#,
    );
    let actions = acknowledge(
        &mut e,
        out,
        &[(
            "/amend-algos",
            r#"{"algoClOrdId":"","algoId":"77","reqId":"","sCode":"0","sMsg":""}"#,
        )],
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { command: 1, result }) if result == r#"["77"]"#
        )),
        "{actions:?}"
    );
    record(&mut joiner, &actions, &held);
    assert!(
        !joiner.placed(SYMBOL, "77"),
        "the joined algo is not placed here"
    );
    let fills = fire(&mut e, private, "77", [&mut joiner, &mut placer]);
    assert_eq!(fills.len(), 1);
    assert!(!joiner.placed(SYMBOL, CHILD), "the joiner never claims it");
}

/// This login's own held stop: joining it keeps it claimed, and its child is.
#[test]
fn joining_this_logins_own_full_position_algo_keeps_it_claimed() {
    let (mut e, private) = ready();
    e.store_mut().upsert_order(held_stop("77_sl"));
    let mut own = OwnPlacements::default();
    own.record(SYMBOL, "77");
    let mut other = OwnPlacements::default();
    let held = own.held_rows(&[SYMBOL.into()], e.store().orders());
    assert!(held.is_empty(), "{held:?}");
    let out = place(
        &mut e,
        r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":42000}"#,
    );
    let actions = acknowledge(
        &mut e,
        out,
        &[(
            "/amend-algos",
            r#"{"algoClOrdId":"","algoId":"77","reqId":"","sCode":"0","sMsg":""}"#,
        )],
    );
    record(&mut own, &actions, &held);
    fire(&mut e, private, "77", [&mut own, &mut other]);
    assert!(own.placed(SYMBOL, CHILD) && !other.placed(SYMBOL, CHILD));
}
