//! AGT-196 through real Binance USDⓈ-M frames: the real Binance engine places,
//! fills and triggers; its receipts, completions, `OrderTriggered` notices and
//! fills go through the same ledger calls the account actor makes.
//!
//! Frames follow Binance's published examples (USDⓈ-M "User Data Streams":
//! `ORDER_TRADE_UPDATE`, `ALGO_UPDATE`; "New Order" and "New Algo Order"
//! responses), with only ids, prices and states changed: docs-derived, not live
//! captures. Liquidation and ADL client ids are the documented `autoclose-` and
//! `adl_autoclose` prefixes.
#![cfg(not(target_arch = "wasm32"))]

#[path = "../../../venues/binance/tests/support/mod.rs"]
mod support;

use safe_cex_engine::{Command, Event, Fill};
use safe_cex_native::own_placements::OwnPlacements;
use support::{Harness, Options};

const SYMBOL: &str = "BTCUSDT";

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.load_public();
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"audit-key"}"#);
    let socket = h.socket("/private/ws").unwrap();
    h.event(Event::WsOpened { socket });
    let _ = h.engine.take_frame(h.now);
    h
}

/// Place one order as the actor would, recording what the actor records.
fn place(h: &mut Harness, ledger: &mut OwnPlacements, command: u32, order: serde_json::Value) {
    let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_value(order).unwrap();
    let legs = [(order.symbol.clone(), order.client_order_id.clone())];
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![order],
    }));
    for (_, receipts) in h.progress.iter().filter(|(id, _)| *id == command) {
        ledger.record_receipts(&[SYMBOL.into()], &legs, receipts);
    }
}

fn complete(h: &Harness, ledger: &mut OwnPlacements, command: u32) {
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == command)
        .and_then(|(_, result)| result.clone().ok())
        .expect("the placement completed");
    // These placements carry no caller id: Binance's are engine-generated.
    ledger.record_completion(&[SYMBOL.into()], &[(SYMBOL.into(), None)], &result);
}

fn query(url: &str, name: &str) -> String {
    url.split(['?', '&'])
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .unwrap_or_else(|| panic!("{name} in {url}"))
        .to_owned()
}

/// The fills one private frame produced, after its link notices applied.
fn fills(h: &mut Harness, ledger: &mut OwnPlacements, frame: &str) -> Vec<Fill> {
    h.message("/private/ws", frame);
    for (symbol, order_id, by) in h.triggered.drain(..) {
        ledger.link(&symbol, &order_id, &by);
    }
    h.engine
        .take_frame(h.now)
        .and_then(safe_cex_engine::wire::fills)
        .unwrap_or_default()
}

fn trade(client: &str, order_type: &str, id: u64, trade: u64) -> String {
    format!(
        r#"{{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{{"s":"BTCUSDT","c":"{client}","S":"SELL","o":"{order_type}","f":"GTC","q":"0.1","p":"0","ap":"100","sp":"0","x":"TRADE","X":"FILLED","i":{id},"l":"0.1","z":"0.1","L":"100","N":"USDT","n":"0","T":1568879465650,"t":{trade},"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"{order_type}","ps":"BOTH","cp":false,"rp":"0","pP":false,"si":0,"ss":0,"V":"EXPIRE_TAKER","pm":"NONE","gtd":0}}}}"#
    )
}

fn claimed(ledger: &OwnPlacements, fills: &[Fill]) -> Vec<bool> {
    fills
        .iter()
        .map(|fill| ledger.placed(&fill.symbol, &fill.order_id))
        .collect()
}

/// A limit this login placed is claimed through its real fill frame; a
/// liquidation and an ADL fill on the same account never are.
#[test]
fn a_placed_limit_is_claimed_and_liquidation_and_adl_fills_are_not() {
    let mut h = ready();
    let mut ledger = OwnPlacements::default();
    place(
        &mut h,
        &mut ledger,
        7,
        serde_json::json!({"symbol": SYMBOL, "type": "limit", "side": "sell", "amount": 0.1, "price": 100.0}),
    );
    let order = h.take("/fapi/v1/order");
    let client = query(&order.url, "newClientOrderId");
    h.reply(
        order.id,
        200,
        "",
        &format!(
            r#"{{"clientOrderId":"{client}","cumQty":"0","cumQuote":"0","executedQty":"0","orderId":22542179,"avgPrice":"0.00000","origQty":"0.1","price":"100","reduceOnly":false,"side":"SELL","positionSide":"BOTH","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","timeInForce":"GTC","type":"LIMIT","origType":"LIMIT","updateTime":1566818724722}}"#
        ),
    );
    complete(&h, &mut ledger, 7);

    let mut seen = fills(&mut h, &mut ledger, &trade(&client, "LIMIT", 22542179, 1));
    seen.extend(fills(
        &mut h,
        &mut ledger,
        &trade("autoclose-1568879465650", "LIQUIDATION", 22542180, 2),
    ));
    seen.extend(fills(
        &mut h,
        &mut ledger,
        &trade("adl_autoclose", "MARKET", 22542181, 3),
    ));
    let ids: Vec<&str> = seen.iter().map(|fill| fill.order_id.as_str()).collect();
    assert_eq!(
        ids,
        ["22542179", "22542180", "22542181"],
        "each frame filled"
    );
    assert_eq!(claimed(&ledger, &seen), [true, false, false]);
}

/// Item 6: a conditional placed through the Algo Order API fires as a new
/// matching-engine order; `ALGO_UPDATE.ai` names it, so its fill is the
/// placer's. Another algo's trigger links nothing here.
#[test]
fn a_fired_algo_orders_fill_is_claimed_through_its_algo() {
    let mut h = ready();
    let mut ledger = OwnPlacements::default();
    let mark = h.engine.store().ticker(SYMBOL).unwrap().mark;
    place(
        &mut h,
        &mut ledger,
        8,
        serde_json::json!({"symbol": SYMBOL, "type": "conditional", "side": "buy",
            "amount": 0.1, "triggerPrice": mark * 1.1}),
    );
    let algo = h.take("/fapi/v1/algoOrder");
    let client = query(&algo.url, "clientAlgoId");
    h.reply(
        algo.id,
        200,
        "",
        &format!(
            r#"{{"algoId":2148719,"clientAlgoId":"{client}","algoType":"CONDITIONAL","orderType":"STOP_MARKET","symbol":"BTCUSDT","side":"BUY","positionSide":"BOTH","timeInForce":"GTC","quantity":"0.1","algoStatus":"NEW","triggerPrice":"110","price":"0","reduceOnly":false,"workingType":"CONTRACT_PRICE","createTime":1750485492076,"updateTime":1750485492076}}"#
        ),
    );
    complete(&h, &mut ledger, 8);

    let fired = |aid: u64, caid: &str, ai: &str| {
        format!(
            r#"{{"e":"ALGO_UPDATE","T":1750515742297,"E":1750515742303,"o":{{"caid":"{caid}","aid":{aid},"at":"CONDITIONAL","o":"STOP_MARKET","s":"BTCUSDT","S":"BUY","ps":"BOTH","f":"GTC","q":"0.1","X":"TRIGGERED","ai":"{ai}","ap":"0.00000","aq":"0.00000","act":"0","tp":"110","p":"0","V":"EXPIRE_MAKER","wt":"CONTRACT_PRICE","pm":"NONE","cp":false,"pP":false,"R":false,"tt":0,"gtd":0}}}}"#
        )
    };
    assert!(fills(&mut h, &mut ledger, &fired(2148719, &client, "8886774")).is_empty());
    assert!(
        fills(
            &mut h,
            &mut ledger,
            &fired(2148720, "someone-else", "8886775")
        )
        .is_empty()
    );
    let mut seen = fills(&mut h, &mut ledger, &trade("", "MARKET", 8886774, 4));
    seen.extend(fills(&mut h, &mut ledger, &trade("", "MARKET", 8886775, 5)));
    let ids: Vec<&str> = seen.iter().map(|fill| fill.order_id.as_str()).collect();
    assert_eq!(ids, ["8886774", "8886775"]);
    assert_eq!(claimed(&ledger, &seen), [true, false]);
}
