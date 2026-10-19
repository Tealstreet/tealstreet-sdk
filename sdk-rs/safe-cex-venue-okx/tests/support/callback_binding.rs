// Included as a child of the production binding to observe the installed receiver.
use super::binding_harness as harness;
use harness::*;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_json};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const INST: &str = "BTC-USDT-SWAP";
const HISTORY: &str = "/api/v5/trade/orders-history";
fn fixture() -> Harness {
    let h = harness(ExchangeName::Okx, options(), |http| {
        http.ok("GET","/api/v5/public/instruments",json!([{
            "instId":INST,"instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC",
            "settleCcy":"USDT","uly":"BTC-USDT","instFamily":"BTC-USDT","baseCcy":"","quoteCcy":"",
            "lotSz":"1","tickSz":"0.1","minSz":"1","maxLmtSz":"100000","maxMktSz":"5000",
            "maxStopSz":"5000","maxTriggerSz":"100000","maxTwapSz":"100000","maxIcebergSz":"100000",
            "lever":"100","state":"live"
        }]));
        http.ok("GET", HISTORY, json!([history_row()]));
    });
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    Store::update(
        &*h.store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h
}
fn history_row() -> serde_json::Value {
    json!({"ordId":"history-1","instId":INST,"side":"buy","posSide":"net","ordType":"limit","state":"filled",
        "sz":"3","accFillSz":"2","px":"60011","avgPx":"60000","cTime":"1700000000000","uTime":"1700000001000","reduceOnly":"false"})
}
fn closed_row() -> serde_json::Value {
    json!({"instId":INST,"cTime":"1700000000000","uTime":"1700000001000","closeTotalPos":"2","openMaxPos":"3",
        "closeAvgPx":"60000","openAvgPx":"59000","realizedPnl":"17","fee":"-1","fundingFee":"2","lever":"5","posId":"position-1","ccy":"","direction":"long"})
}
fn throwing() -> (Js, Js, Rc<Cell<usize>>) {
    let original = Js::object(vec![
        ("message", Js::str("callback exploded")),
        ("marker", Js::str("opaque cause")),
    ]);
    let captured = original.clone();
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let callback = Js::function("throwingDependency", move |_, _| {
        counter.set(counter.get() + 1);
        Err(Thrown::new(captured.clone()))
    });
    (callback, original, calls)
}
fn check_mapper(name: &str, slot: &str) {
    let h = fixture();
    let globals = &h.exchange.scope.shared.globals;
    let mapper = globals.get(name).unwrap();
    let market = h
        .exchange
        .scope
        .this
        .get("store")
        .unwrap()
        .get("marketsById")
        .unwrap()
        .get(INST)
        .unwrap();
    let (callback, original, calls) = throwing();
    let args = match name {
        "mapOkxHistoricalOrder" => vec![from_json(&history_row()), market, callback],
        "mapOkxClosedPosition" => vec![
            from_json(&closed_row()),
            market,
            callback,
            Js::str("account"),
        ],
        "mapOkxBillToIncomeRecords" => vec![
            from_json(
                &json!({"ts":"1700000000000","instId":INST,"balChg":"17","fee":"-1","pnl":"12","billId":"bill-1","type":"2","ccy":"USDT"}),
            ),
            callback,
        ],
        _ => {
            let deps = Js::object(vec![
                ("account", Js::str("account")),
                ("toBase", Js::function("toBase", |_, _| Ok(Js::Number(2.)))),
                (
                    "resolveSymbol",
                    Js::function("resolveSymbol", |_, _| Ok(Js::str("BTCUSDT"))),
                ),
                (
                    "resolveSettleCurrency",
                    Js::function("resolveSettleCurrency", |_, _| Ok(Js::str("USDT"))),
                ),
            ]);
            deps.set(slot, callback).unwrap();
            vec![from_json(&closed_row()), deps]
        }
    };
    let result = mapper.call(Js::Undefined, args);
    assert_eq!(
        calls.get(),
        1,
        "stop at first callback failure in {name}/{slot}"
    );
    let error = result.expect_err("installed mapper must propagate its dependency throw");
    assert!(
        error.value.strict_equals(&original),
        "retain opaque throw identity in {name}/{slot}"
    );
}
#[test]
fn historical_order_callback_throw_is_preserved() {
    check_mapper("mapOkxHistoricalOrder", "");
}
#[test]
fn closed_position_callback_throw_is_preserved() {
    check_mapper("mapOkxClosedPosition", "");
}
#[test]
fn income_symbol_callback_throw_is_preserved() {
    check_mapper("mapOkxBillToIncomeRecords", "");
}
#[test]
fn position_size_callback_throw_is_preserved() {
    check_mapper("mapOkxPositionHistory", "toBase");
}
#[test]
fn position_symbol_callback_throw_is_preserved() {
    check_mapper("mapOkxPositionHistory", "resolveSymbol");
}
#[test]
fn position_settle_callback_throw_is_preserved() {
    check_mapper("mapOkxPositionHistory", "resolveSettleCurrency");
}

#[test]
fn actual_history_read_reports_the_callback_error_and_returns_no_rows() {
    let h = fixture();
    let (callback, _, calls) = throwing();
    h.exchange
        .scope
        .this
        .set("contractsToBase", callback)
        .unwrap();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, data| {
            sink.borrow_mut()
                .push(String::from_utf16(data.as_str().unwrap().as_ref()).unwrap());
            Ok(())
        }),
    );
    let orders = h.drive(h.venue().fetch_historical_orders(None)).unwrap();
    assert_eq!(h.http.calls(HISTORY).len(), 1);
    assert_eq!(
        calls.get(),
        1,
        "the first callback failure stops remaining conversions"
    );
    assert!(
        orders.is_empty(),
        "a failed mapping must not publish a NaN row"
    );
    assert_eq!(&*errors.borrow(), &["Exchange Error: callback exploded"]);
}

#[test]
fn maintained_source_preserves_six_callback_throws() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/callback-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "6");
}

fn book_state_socket() -> (Harness, Js) {
    let h = harness(ExchangeName::Okx, options(), |_| {});
    let socket = h
        .exchange
        .scope
        .shared
        .globals
        .get("OKXPublicWebsocket")
        .unwrap()
        .construct(vec![h.exchange.scope.this.clone()], None)
        .unwrap();
    (h, socket)
}
fn book_frame(action: &str, sequence: &str, previous: &str) -> Js {
    Js::object(vec![
        ("action", Js::str(action)),
        (
            "data",
            Js::array(vec![Js::object(vec![
                ("seqId", Js::str(sequence)),
                ("prevSeqId", Js::str(previous)),
            ])]),
        ),
    ])
}
#[test]
fn book_state_snapshot_does_not_read_unrelated_entries_or_topics() {
    let (_h, socket) = book_state_socket();
    let original = Js::object(vec![("opaque", Js::Bool(true))]);
    let cause = original.clone();
    let poison = Js::object(vec![(
        "toString",
        Js::function("poison", move |_, _| Err(Thrown::new(cause.clone()))),
    )]);
    let sequences = socket.get("orderBookSequenceByHandler").unwrap();
    sequences
        .call_method("set", vec![Js::str("unrelated"), poison.clone()])
        .unwrap();
    let topics = Js::object(vec![]);
    let cause = original.clone();
    topics
        .define_accessor(
            "unrelated",
            Some(Rc::new(move |_| Err(Thrown::new(cause.clone())))),
            None,
        )
        .unwrap();
    socket.set("topics", topics).unwrap();
    let accepted = socket.call_method(
        "acceptOrderBookSequence",
        vec![Js::str("books.BTC"), book_frame("snapshot", "10", "")],
    );
    assert!(
        matches!(accepted, Ok(Js::Bool(true))),
        "snapshot must touch only the selected state entry: {accepted:?}"
    );
    assert!(
        sequences
            .call_method("get", vec![Js::str("unrelated")])
            .unwrap()
            .strict_equals(&poison)
    );
    assert!(
        sequences
            .call_method("get", vec![Js::str("books.BTC")])
            .unwrap()
            .strict_equals(&Js::str("10"))
    );
}
#[test]
fn book_state_recovery_commits_before_selected_topic_throws() {
    let (_h, socket) = book_state_socket();
    let sequences = socket.get("orderBookSequenceByHandler").unwrap();
    let recovering = socket.get("recoveringOrderBooks").unwrap();
    sequences
        .call_method("set", vec![Js::str("books.BTC"), Js::str("10")])
        .unwrap();
    let original = Js::object(vec![("opaque", Js::Bool(true))]);
    let cause = original.clone();
    let topics = Js::object(vec![]);
    topics
        .define_accessor(
            "books.BTC",
            Some(Rc::new(move |_| Err(Thrown::new(cause.clone())))),
            None,
        )
        .unwrap();
    socket.set("topics", topics).unwrap();
    let error = socket
        .call_method("recoverOrderBook", vec![Js::str("books.BTC")])
        .unwrap_err();
    assert!(
        error.value.strict_equals(&original),
        "original topic getter throw"
    );
    assert!(
        !sequences
            .call_method("has", vec![Js::str("books.BTC")])
            .unwrap()
            .truthy(),
        "delete happens before the topic read"
    );
    assert!(
        recovering
            .call_method("has", vec![Js::str("books.BTC")])
            .unwrap()
            .truthy(),
        "add happens before the topic read"
    );
}
#[test]
fn book_state_sequence_uses_the_live_selected_map_methods() {
    let (_h, socket) = book_state_socket();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let calls = trace.clone();
    let sequences = Js::object(vec![(
        "set",
        Js::function("set", move |receiver, args| {
            calls.borrow_mut().push((receiver, args));
            Ok(Js::Undefined)
        }),
    )]);
    socket
        .set("orderBookSequenceByHandler", sequences.clone())
        .unwrap();
    let accepted = socket
        .call_method(
            "acceptOrderBookSequence",
            vec![Js::str("books.BTC"), book_frame("snapshot", "10", "")],
        )
        .unwrap();
    assert!(accepted.truthy());
    let calls = trace.borrow();
    assert_eq!(calls.len(), 1, "source calls the selected Map method once");
    assert!(calls[0].0.strict_equals(&sequences));
    assert!(calls[0].1[0].strict_equals(&Js::str("books.BTC")));
    assert!(calls[0].1[1].strict_equals(&Js::str("10")));
}

#[test]
fn book_state_maintained_source_access_and_commit_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/book-state-source.mjs"
        ))
        .output()
        .expect("maintained source state oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "6 maintained source state observations PASS"
    );
}

#[test]
fn book_state_sequence_preserves_repeated_property_reads() {
    let (_h, socket) = book_state_socket();
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let update = Js::object(vec![]);
    update
        .define_accessor(
            "seqId",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Ok(Js::str(if count.get() == 1 { "10" } else { "11" }))
            })),
            None,
        )
        .unwrap();
    let frame = Js::object(vec![
        ("action", Js::str("snapshot")),
        ("data", Js::array(vec![update])),
    ]);
    assert!(
        socket
            .call_method("acceptOrderBookSequence", vec![Js::str("books.BTC"), frame])
            .unwrap()
            .truthy()
    );
    assert_eq!(
        reads.get(),
        2,
        "nullish guard and String argument both read the source field"
    );
    assert!(
        socket
            .get("orderBookSequenceByHandler")
            .unwrap()
            .call_method("get", vec![Js::str("books.BTC")])
            .unwrap()
            .strict_equals(&Js::str("11"))
    );
}

#[test]
fn book_state_recovery_keeps_original_topic_live_across_sends() {
    let (_h, socket) = book_state_socket();
    socket
        .define_accessor("isConnected", Some(Rc::new(|_| Ok(Js::Bool(true)))), None)
        .unwrap();
    let topic = Js::object(vec![
        ("channel", Js::str("books")),
        ("instId", Js::str("BTC")),
        ("extra", Js::str("before")),
    ]);
    let topics = Js::object(vec![("books.BTC", Js::array(vec![topic.clone()]))]);
    socket.set("topics", topics.clone()).unwrap();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let output = sent.clone();
    let Js::Object(data) = &socket else {
        panic!("socket object")
    };
    let receiver = Rc::downgrade(data);
    socket
        .set(
            "send",
            Js::function("send", move |this, args| {
                assert!(this.strict_equals(&Js::Object(receiver.upgrade().unwrap())));
                output
                    .borrow_mut()
                    .push(String::from_utf16(&args[0].to_js_string()?).unwrap());
                topic.set("extra", Js::str("after"))?;
                topics.set(
                    "books.BTC",
                    Js::array(vec![Js::object(vec![("instId", Js::str("replacement"))])]),
                )?;
                Ok(Js::Undefined)
            }),
        )
        .unwrap();
    socket
        .call_method("recoverOrderBook", vec![Js::str("books.BTC")])
        .unwrap();
    assert_eq!(
        &*sent.borrow(),
        &[
            "{\"op\":\"unsubscribe\",\"args\":[{\"channel\":\"books\",\"instId\":\"BTC\",\"extra\":\"before\"}]}",
            "{\"op\":\"subscribe\",\"args\":[{\"channel\":\"books\",\"instId\":\"BTC\",\"extra\":\"after\"}]}",
        ]
    );
}

#[test]
fn book_state_public_frame_does_not_enumerate_topic_state() {
    let (_h, socket) = book_state_socket();
    let cause = Js::object(vec![("opaque", Js::Bool(true))]);
    let topics = Js::object(vec![]);
    topics
        .define_accessor(
            "unrelated",
            Some(Rc::new(move |_| Err(Thrown::new(cause.clone())))),
            None,
        )
        .unwrap();
    socket.set("topics", topics).unwrap();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let sequences = socket.get("orderBookSequenceByHandler").unwrap();
    let observed = sequences.clone();
    socket
        .get("messageHandlers")
        .unwrap()
        .set(
            "books.BTC",
            Js::array(vec![Js::function("book", move |_, _| {
                assert!(
                    observed
                        .call_method("get", vec![Js::str("books.BTC")])?
                        .strict_equals(&Js::str("10")),
                    "selected state is committed before callback"
                );
                count.set(count.get() + 1);
                Ok(Js::Undefined)
            })]),
        )
        .unwrap();
    let event = Js::object(vec![(
        "data",
        Js::str(
            r#"{"arg":{"channel":"books","instId":"BTC"},"action":"snapshot","data":[{"seqId":"10"}]}"#,
        ),
    )]);
    let result = socket.call_method("onMessage", vec![event]);
    assert!(
        result.is_ok(),
        "public frame must not read unrelated topics: {result:?}"
    );
    assert_eq!(calls.get(), 1);
    assert!(
        socket
            .get("orderBookSequenceByHandler")
            .unwrap()
            .strict_equals(&sequences)
    );
}
