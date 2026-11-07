//! USD-M trailing stops: a dragged level becomes a callback rate inside Binance's range,
//! one-way trailing stops only reduce, and a refused replacement puts back the native rate.
use support::Options;
mod support;

use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event, Method};
use support::Harness;

const SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":false,"leverage":20,"maxNotionalValue":"$1000000"}]"#;
const ACCOUNT: &str = r#"{"totalInitialMargin":"48","totalWalletBalance":"1000","availableBalance":"952","assets":[{"asset":"USDT","walletBalance":"1000.0","unrealizedProfit":"10.0","marginBalance":"1010.0","maintMargin":"3.8","initialMargin":"48.0","availableBalance":"952.0","updateTime":1625474304765}],"positions":[]}"#;
const LIMIT_ROW: &str = r#"{"avgPrice":"0","clientOrderId":"abc","cumQuote":"0","executedQty":"0","orderId":77,"origQty":"0.100","origType":"LIMIT","price":"90","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"NEW","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","time":1,"timeInForce":"GTC","type":"LIMIT","updateTime":1,"workingType":"CONTRACT_PRICE","priceProtect":false}"#;
/// A trailing stop as openAlgoOrders lists it before activation: the native rate and
/// activation price are the only exact description of it.
const TRAILING_ROW: &str = r#"{"algoId":5550,"clientAlgoId":"trail","algoType":"CONDITIONAL","orderType":"TRAILING_STOP_MARKET","symbol":"BTCUSDT","side":"SELL","positionSide":"BOTH","timeInForce":"GTC","quantity":"0.010","algoStatus":"NEW","triggerPrice":"98","price":"0","callbackRate":"1.5","activatePrice":"103","workingType":"CONTRACT_PRICE","reduceOnly":true,"closePosition":false,"createTime":1}"#;

fn ready_with(hedged: bool, algo: &str) -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125,"notionalCap":50000}]}]"#);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        "/fapi/v1/listenKey",
        200,
        r#"{"listenKey":"pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1"}"#,
    );
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        &format!(r#"{{"dualSidePosition":{hedged}}}"#),
    );
    let side = if hedged { "LONG" } else { "BOTH" };
    h.respond(
        "/fapi/v3/positionRisk",
        200,
        &format!(r#"[{{"symbol":"BTCUSDT","positionSide":"{side}","positionAmt":"0.010","entryPrice":"95","breakEvenPrice":"95","markPrice":"100","unRealizedProfit":"0.05","liquidationPrice":"50","isolatedMargin":"0","notional":"1","marginAsset":"USDT","isolatedWallet":"0","initialMargin":"0.05","maintMargin":"0.01","positionInitialMargin":"0.05","openOrderInitialMargin":"0","adl":1,"bidNotional":"0","askNotional":"0","updateTime":1}}]"#),
    );
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    h.respond("/fapi/v3/account", 200, ACCOUNT);
    h.respond("/fapi/v1/openOrders", 200, &format!("[{LIMIT_ROW}]"));
    h.respond("/fapi/v1/openAlgoOrders", 200, algo);
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h.answer_open_position_read();
    h.requests.clear();
    h
}

fn ready() -> Harness {
    ready_with(false, &format!("[{TRAILING_ROW}]"))
}

fn trailing(price: f64) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": "BTCUSDT",
        "type": "trailing_stop_market",
        "side": "sell",
        "amount": 0.01,
        "price": price,
        "reduceOnly": true,
    }))
    .unwrap()
}

fn place(h: &mut Harness, command: u32, order: PlaceOrderOpts) {
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![order],
    }));
}

fn amend(h: &mut Harness, command: u32, id: &str, price: f64) {
    h.event(Event::Command(Command::AmendOrder {
        command,
        id: id.into(),
        price: Some(price),
        amount: None,
    }));
}

fn param(url: &str, name: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
        .map(str::to_owned)
}

fn no_writes(h: &Harness) -> bool {
    h.requests.iter().all(|r| r.method == Method::Get)
}

#[test]
fn a_new_one_way_trailing_stop_is_reduce_only() {
    let mut h = ready();
    place(&mut h, 1, trailing(98.0));
    let sent = h.take("/fapi/v1/algoOrder");
    assert_eq!(
        param(&sent.url, "type").as_deref(),
        Some("TRAILING_STOP_MARKET")
    );
    assert_eq!(param(&sent.url, "callbackRate").as_deref(), Some("2"));
    assert_eq!(param(&sent.url, "reduceOnly").as_deref(), Some("true"));
    assert_eq!(param(&sent.url, "positionSide").as_deref(), Some("BOTH"));
}

#[test]
fn a_hedged_trailing_stop_keeps_its_position_side_and_sends_no_reduce_only() {
    let mut h = ready_with(true, "[]");
    place(&mut h, 1, trailing(98.0));
    let sent = h.take("/fapi/v1/algoOrder");
    assert_eq!(param(&sent.url, "positionSide").as_deref(), Some("LONG"));
    assert_eq!(param(&sent.url, "reduceOnly"), None);
}

#[test]
fn a_level_outside_the_callback_range_is_refused_before_any_write() {
    // 25% from last is past USD-M's 20% ceiling; 0.04% rounds to 0.0%.
    for (command, level) in [(1, 75.0), (2, 99.96)] {
        let mut h = ready();
        place(&mut h, command, trailing(level));
        assert!(no_writes(&h), "{level}: {:?}", h.paths());
        let message = h.result(command).unwrap().as_ref().unwrap_err();
        assert!(
            message.contains("outside Binance's 0.1%-20% callback range"),
            "{message}"
        );
    }
}

#[test]
fn an_out_of_range_amend_refuses_before_the_cancel() {
    let mut h = ready();
    amend(&mut h, 3, "5550", 70.0);
    assert!(no_writes(&h), "{:?}", h.paths());
    let message = h.result(3).unwrap().as_ref().unwrap_err();
    assert!(message.contains("0.1%-20%"), "{message}");
    assert!(message.contains("was not changed"), "{message}");
}

#[test]
fn an_amend_without_the_native_rate_refuses_before_the_cancel() {
    let row = TRAILING_ROW.replace(r#""callbackRate":"1.5","#, "");
    let mut h = ready_with(false, &format!("[{row}]"));
    amend(&mut h, 4, "5550", 98.0);
    assert!(no_writes(&h), "{:?}", h.paths());
    let message = h.result(4).unwrap().as_ref().unwrap_err();
    assert!(
        message.contains("native callback rate is not known"),
        "{message}"
    );
}

#[test]
fn a_refused_replacement_puts_back_the_native_rate_and_activation_price() {
    let mut h = ready();
    amend(&mut h, 5, "5550", 97.0);
    let cancel = h.take("/fapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/fapi/v1/algoOrder");
    assert_eq!(replacement.method, Method::Post);
    assert_eq!(
        param(&replacement.url, "callbackRate").as_deref(),
        Some("3")
    );
    assert_eq!(param(&replacement.url, "activatePrice"), None);
    assert_eq!(
        param(&replacement.url, "reduceOnly").as_deref(),
        Some("true")
    );
    h.reply(
        replacement.id,
        400,
        "",
        r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
    );
    let put_back = h.take("/fapi/v1/algoOrder");
    assert_eq!(put_back.method, Method::Post);
    assert_eq!(
        param(&put_back.url, "type").as_deref(),
        Some("TRAILING_STOP_MARKET")
    );
    assert_eq!(param(&put_back.url, "callbackRate").as_deref(), Some("1.5"));
    assert_eq!(
        param(&put_back.url, "activatePrice").as_deref(),
        Some("103")
    );
    assert_eq!(param(&put_back.url, "reduceOnly").as_deref(), Some("true"));
}

/// Master's bytes for an ordinary limit amend: cancel, then the same order at the new price.
#[test]
fn an_ordinary_limit_amend_is_unchanged() {
    let mut h = ready();
    amend(&mut h, 6, "77", 91.0);
    let cancel = h.take("/fapi/v1/order");
    assert_eq!(
        format!("{} {}", cancel.method.as_str(), cancel.url),
        "DELETE https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&orderId=77&timestamp=1000&recvWindow=20000&signature=a396f38a4dd1e09374a8f86f2de6c0e4759c0d7a3b8624452724a408a838efaa"
    );
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"symbol":"BTCUSDT","orderId":77,"clientOrderId":"abc","origQty":"0.100","executedQty":"0","status":"CANCELED"}"#,
    );
    let replacement = h.take("/fapi/v1/order");
    assert_eq!(
        format!("{} {}", replacement.method.as_str(), replacement.url),
        "POST https://fapi.binance.com/fapi/v1/order?symbol=BTCUSDT&positionSide=BOTH&side=BUY&type=LIMIT&quantity=0.1&price=91&timeInForce=GTC&newClientOrderId=x-s3vFFRQpknoku1vzPM4NJqegqfwzt&timestamp=1000&recvWindow=20000&signature=2497438551c31356f2a51abcb5fc2cf998e9001272ff0910b7c1cc65346aedc5"
    );
}

/// The native rate is captured when the amend is admitted: an open-orders refresh that
/// lands while the cancel is in flight cannot change what the put-back restores.
#[test]
fn a_refresh_during_the_cancel_cannot_change_the_put_back() {
    let mut h = ready();
    amend(&mut h, 75, "5550", 97.0);
    let cancel = h.take("/fapi/v1/algoOrder");
    h.event(Event::Command(Command::Venue {
        command: 76,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
    }));
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    assert!(h.engine.store().order("5550").is_some());
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/fapi/v1/algoOrder");
    h.reply(
        replacement.id,
        400,
        "",
        r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
    );
    let put_back = h.take("/fapi/v1/algoOrder");
    assert_eq!(param(&put_back.url, "callbackRate").as_deref(), Some("1.5"));
    assert_eq!(
        param(&put_back.url, "activatePrice").as_deref(),
        Some("103")
    );
}

fn legacy_without_reduce_only() -> Harness {
    let row = TRAILING_ROW
        .replace(r#""reduceOnly":true"#, r#""reduceOnly":false"#)
        .replace(r#""quantity":"0.010""#, r#""quantity":"0.100""#);
    let mut h = ready_with(false, &format!("[{row}]"));
    assert!(!h.engine.store().order("5550").unwrap().reduce_only);
    h.engine.store_mut().update_positions(|p| p.contracts = 0.1);
    h
}

/// A one-way trailing stop placed without reduceOnly (the old shape) only reduces once
/// amended: on the replacement and on the put-back.
#[test]
fn a_legacy_trailing_stops_replacement_and_put_back_are_reduce_only() {
    let mut h = legacy_without_reduce_only();
    amend(&mut h, 80, "5550", 97.0);
    let cancel = h.take("/fapi/v1/algoOrder");
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/fapi/v1/algoOrder");
    assert_eq!(
        param(&replacement.url, "reduceOnly").as_deref(),
        Some("true")
    );
    h.reply(
        replacement.id,
        400,
        "",
        r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
    );
    let put_back = h.take("/fapi/v1/algoOrder");
    assert_eq!(param(&put_back.url, "reduceOnly").as_deref(), Some("true"));
    assert_eq!(param(&put_back.url, "callbackRate").as_deref(), Some("1.5"));
}

/// A new one-way trailing stop reduces unless the caller explicitly says it must not.
#[test]
fn a_new_trailing_stop_reduces_by_default_and_honours_an_explicit_false() {
    for (reduce, expected) in [(None, Some("true")), (Some(false), None)] {
        let mut h = ready();
        let mut order = trailing(98.0);
        order.reduce_only = reduce;
        place(&mut h, 81, order);
        let sent = h.take("/fapi/v1/algoOrder");
        assert_eq!(
            param(&sent.url, "reduceOnly").as_deref(),
            expected,
            "{reduce:?}"
        );
    }
}

#[test]
fn ambiguous_ordinary_rate_limit_is_unknown() {
    let mut h = ready();
    let order = serde_json::from_value(serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":90.0})).unwrap();
    place(&mut h, 901, order);
    let sent = h.take("/fapi/v1/order");
    h.reply(
        sent.id,
        429,
        "",
        r#"{"code":-1003,"msg":"Too many requests"}"#,
    );
    let error = h
        .failures
        .iter()
        .find(|(id, _)| *id == 901)
        .expect("terminal failure");
    assert_eq!(
        error.1.outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "{:?}",
        error
    );
}

#[test]
fn ambiguous_replacement_cannot_put_back() {
    let mut h = ready();
    amend(&mut h, 902, "5550", 97.0);
    let cancel = h.take("/fapi/v1/algoOrder");
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/fapi/v1/algoOrder");
    h.reply(
        replacement.id,
        429,
        "",
        r#"{"code":-1003,"msg":"Too many requests"}"#,
    );
    assert!(
        no_writes(&h),
        "ambiguous replacement caused another write: {:?}",
        h.paths()
    );
    let error = &h
        .failures
        .iter()
        .find(|(id, _)| *id == 902)
        .expect("terminal failure")
        .1;
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.protection, None);
}

#[test]
fn ambiguous_signature_body_cannot_resend() {
    let mut h = ready();
    let order = serde_json::from_value(serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":90.0})).unwrap();
    place(&mut h, 903, order);
    let sent = h.take("/fapi/v1/order");
    h.reply(
        sent.id,
        429,
        "",
        r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#,
    );
    assert!(
        no_writes(&h),
        "429 authorized physical resend: {:?}",
        h.paths()
    );
    assert_eq!(
        h.failures
            .iter()
            .find(|(id, _)| *id == 903)
            .unwrap()
            .1
            .outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
}

#[test]
fn unproven_replacement_controls_do_not_put_back() {
    for status in [408, 503] {
        let mut h = ready();
        amend(&mut h, 904, "5550", 97.0);
        let cancel = h.take("/fapi/v1/algoOrder");
        h.reply(
            cancel.id,
            200,
            "",
            r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
        );
        let replacement = h.take("/fapi/v1/algoOrder");
        h.reply(
            replacement.id,
            status,
            "",
            r#"{"code":-1003,"msg":"Too many requests"}"#,
        );
        assert!(no_writes(&h), "{status}: {:?}", h.paths());
    }
}

#[test]
fn ambiguous_native_rate_replacements_cannot_put_back() {
    for code in [-1003, -1015] {
        let mut h = ready();
        amend(&mut h, 905, "5550", 97.0);
        let cancel = h.take("/fapi/v1/algoOrder");
        h.reply(
            cancel.id,
            200,
            "",
            r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
        );
        let replacement = h.take("/fapi/v1/algoOrder");
        h.reply(
            replacement.id,
            200,
            "",
            &format!(r#"{{"code":{code},"msg":"Too many requests"}}"#),
        );
        assert!(
            no_writes(&h),
            "native rate {code} put back the old stop: {:?}",
            h.paths()
        );
        let error = &h.failures.iter().find(|(id, _)| *id == 905).unwrap().1;
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.protection, None);
        assert_eq!(error.venue_code.as_deref(), Some(code.to_string().as_str()));
    }
}

#[test]
fn ambiguous_trailing_cancel_cannot_resend_or_replace() {
    let mut h = ready();
    amend(&mut h, 906, "5550", 97.0);
    let cancel = h.take("/fapi/v1/algoOrder");
    h.reply(
        cancel.id,
        429,
        "",
        r#"{"code":-1022,"msg":"Signature is not valid"}"#,
    );
    assert!(
        no_writes(&h),
        "ambiguous cancel sent another write: {:?}",
        h.paths()
    );
    let error = &h.failures.iter().find(|(id, _)| *id == 906).unwrap().1;
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert_eq!(
        h.engine.store().order("5550").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn ambiguous_put_back_has_no_protection_claim_or_further_write() {
    for (status, code) in [(429, -1003), (200, -1015), (429, -1022)] {
        let mut h = ready();
        amend(&mut h, 907, "5550", 97.0);
        let cancel = h.take("/fapi/v1/algoOrder");
        h.reply(
            cancel.id,
            200,
            "",
            r#"{"algoId":5550,"clientAlgoId":"trail","code":"200","msg":"success"}"#,
        );
        let replacement = h.take("/fapi/v1/algoOrder");
        h.reply(
            replacement.id,
            400,
            "",
            r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
        );
        let restore = h.take("/fapi/v1/algoOrder");
        h.reply(
            restore.id,
            status,
            "",
            &format!(r#"{{"code":{code},"msg":"uncertain write"}}"#),
        );
        assert!(no_writes(&h), "{status}/{code}: {:?}", h.paths());
        let error = &h.failures.iter().find(|(id, _)| *id == 907).unwrap().1;
        assert_eq!(
            error.outcome,
            Some(safe_cex_engine::Outcome::Unknown),
            "{status}/{code}: {error:?}"
        );
        assert_eq!(error.protection, None, "{status}/{code}: {error:?}");
        assert!(error.message.contains("unknown"), "{error:?}");
    }
}
