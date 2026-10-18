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

#[test]
fn actual_socket_dispatch_reads_only_the_live_selected_handler() {
    for field in ["publicWebsocket", "businessWebsocket"] {
        let h = fixture();
        let socket = h.exchange.scope.this.get(field).unwrap();
        let reads = Rc::new(Cell::new(0));
        let callbacks = Rc::new(Cell::new(0));
        let count = callbacks.clone();
        let parser = Js::function("parser", move |_, _| {
            count.set(count.get() + 1);
            Ok(Js::Undefined)
        });
        let table = Js::object(vec![("selected", Js::array(vec![parser.clone()]))]);
        let bad = reads.clone();
        table
            .define_accessor(
                "unrelated",
                Some(Rc::new(move |_| {
                    bad.set(bad.get() + 1);
                    Err(Thrown::type_error(
                        "unrelated handler getter must stay unread",
                    ))
                })),
                None,
            )
            .unwrap();
        socket.set("messageHandlers", table.clone()).unwrap();
        let dispatch = || {
            socket.call_method(
                "dispatchMessageHandler",
                vec![Js::str("selected"), Js::object(vec![])],
            )
        };
        dispatch().unwrap();
        assert_eq!(callbacks.get(), 1, "{field} selected callback");
        assert_eq!(reads.get(), 0, "{field} never enumerates the handler table");
        let core = h
            .venue()
            .base()
            .get_socket(if field == "publicWebsocket" {
                safe_cex_exchange::socket_lifecycle::SocketSlot::Public
            } else {
                safe_cex_exchange::socket_lifecycle::SocketSlot::Business
            })
            .unwrap();
        assert_eq!(
            core.message_handlers_stats.borrow()["selected"].had_handlers,
            Some(true)
        );
        // These mutations bypass the listen API: the same aliased JS table must
        // be read again, rather than a synchronized typed registry.
        table.define("selected", Js::array(vec![])).unwrap();
        dispatch().unwrap();
        assert_eq!(
            callbacks.get(),
            1,
            "{field} sees direct defineProperty replacement"
        );
        table
            .delete(&"selected".encode_utf16().collect::<Vec<_>>())
            .unwrap();
        dispatch().unwrap();
        assert_eq!(callbacks.get(), 1, "{field} sees direct deletion");
        table.set("selected", Js::array(vec![parser])).unwrap();
        dispatch().unwrap();
        assert_eq!(callbacks.get(), 2, "{field} sees alias assignment");
        assert_eq!(reads.get(), 0);
        h.venue().dispose().unwrap();
    }
}
