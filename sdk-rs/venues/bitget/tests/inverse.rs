//! Shared documented Classic payload schemas adapted to BTCUSD/ETHUSD, with
//! the exact numeric conversions in BitgetInverseExchange as the parity oracle.
//! https://www.bitget.com/docs/catalog/classic-contract-position/classic-contract-position
//! https://www.bitget.com/docs/catalog/classic-contract-account/classic-contract-account
use crate::support::FixtureOptions as Options;
mod support;

use safe_cex_engine::{Command, Event, wire};
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::Harness;

const CONTRACTS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSD","baseCoin":"BTC","quoteCoin":"USD","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"1","minTradeNum":"1","maxTradeNum":"2000","maxLever":"125"},{"symbol":"ETHUSD","baseCoin":"ETH","quoteCoin":"USD","symbolStatus":"normal","pricePlace":"2","priceEndStep":"1","sizeMultiplier":"1","minTradeNum":"1","maxTradeNum":"3000"}]}"#;
const TICKERS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSD","lastPr":"40000","markPrice":"50000"},{"symbol":"ETHUSD","lastPr":"2400","markPrice":"2500"}]}"#;
const BALANCE: &str = r#"{"code":"00000","data":[{"marginCoin":"BTC","unionTotalMargin":"2","unionAvailable":"1.5","unrealizedPL":"0.1","usdtEquity":"100000"},{"marginCoin":"ETH","accountEquity":"3","available":"2","unrealizedPL":"0.2","usdtEquity":"0"}]}"#;

fn started(tickers: &str) -> Harness {
    let mut h = Harness::started(Options {
        product: Product::Inverse,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    h.respond("/contracts", 200, CONTRACTS);
    h.respond("/tickers", 200, tickers);
    let public = h
        .sockets
        .iter()
        .filter(|(_, url)| url.ends_with("/public"))
        .cloned()
        .collect::<Vec<_>>();
    for (socket, _) in public {
        h.event(Event::WsOpened { socket });
    }
    // TS's REST ticker seed sets mark=last. A live ticker supplies the actual
    // mark used by the inverse position and collateral conversion paths.
    let seed: Value = serde_json::from_str(tickers).unwrap();
    h.message(
        &json!({"arg":{"channel":"ticker"},"action":"snapshot","data":seed["data"]}).to_string(),
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    assert!(!h.requests.iter().any(|r| r.url.contains("/api/v3/")));
    h
}

fn position(symbol: &str, total: &str, entry: &str, updated: &str) -> Value {
    json!({
        "symbol": symbol, "marginCoin": symbol.trim_end_matches("USD"),
        "holdSide": "long", "total": total, "openPriceAvg": entry,
        "leverage": "20", "unrealizedPL": "0.01",
        "achievedProfits": "0.2", "totalFee": "-0.02", "deductedFee": "0.01",
        "liquidationPrice": "1000", "marginMode": "crossed", "posMode": "one_way_mode", "uTime": updated
    })
}

fn positions(h: &mut Harness, btc: Value, eth: Value) {
    let request = h.take("/all-position");
    assert!(
        request
            .url
            .ends_with("productType=coin-futures&marginCoin=BTC")
    );
    h.reply(
        request.id,
        200,
        &json!({"code":"00000","data":[btc]}).to_string(),
    );
    let request = h.take("/all-position");
    assert!(
        request
            .url
            .ends_with("productType=coin-futures&marginCoin=ETH")
    );
    h.reply(
        request.id,
        200,
        &json!({"code":"00000","data":[eth]}).to_string(),
    );
}

fn ready() -> Harness {
    let mut h = started(TICKERS);
    positions(
        &mut h,
        position("BTCUSD", "1000", "25000", "100"),
        position("ETHUSD", "333", "2000", "100"),
    );
    h.respond("/account/accounts", 200, BALANCE);
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    h
}

fn order(id: &str, price: &str) -> Value {
    json!({"orderId":id,"symbol":"BTCUSD","size":"1000","baseVolume":"100","price":price,"side":"buy","orderType":"limit","status":"partially_filled","clientOid":"native-sizing"})
}

fn ws_order(h: &mut Harness, row: Value) {
    h.private_message(
        &json!({"arg":{"channel":"orders"},"action":"update","data":[row]}).to_string(),
    );
}

#[test]
fn inverse_positions_fetch_each_coin_atomically_and_preserve_native_pnl() {
    let mut h = started(TICKERS);
    let first = h.take("/all-position");
    assert!(first.url.ends_with("marginCoin=BTC"));
    h.reply(
        first.id,
        200,
        &json!({"code":"00000","data":[position("BTCUSD","1000","25000","100")]}).to_string(),
    );
    assert!(
        h.engine.store().positions().is_empty(),
        "one coin is not a complete account snapshot"
    );
    assert!(!h.engine.store().loaded().positions);
    h.respond(
        "marginCoin=ETH",
        200,
        &json!({"code":"00000","data":[position("ETHUSD","333","2000","100")]}).to_string(),
    );
    let rows = h.engine.store().positions();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0].contracts, rows[0].notional), (0.02, 1000.0));
    assert_eq!((rows[1].contracts, rows[1].notional), (0.1332, 333.0));
    assert_eq!(rows[0].unrealized_pnl, 0.01);
    assert_eq!(rows[0].realized_pnl, Some(0.17));
}

#[test]
fn inverse_position_reference_uses_mark_then_last_then_entry_and_rounds_eight_places() {
    for (ticker, entry, native, expected) in [
        (
            r#"{"symbol":"BTCUSD","markPrice":"50000","lastPr":"40000"}"#,
            "25000",
            "1000",
            0.02,
        ),
        (
            r#"{"symbol":"BTCUSD","markPrice":"0","lastPr":"40000"}"#,
            "25000",
            "1000",
            0.025,
        ),
        (r#"{"symbol":"BTCUSD","lastPr":"0"}"#, "25000", "1000", 0.04),
        (r#"{"symbol":"BTCUSD","lastPr":"0"}"#, "0", "1000", 1000.0),
        (
            r#"{"symbol":"BTCUSD","markPrice":"3"}"#,
            "1",
            "1",
            0.33333333,
        ),
        (
            r#"{"symbol":"BTCUSD","markPrice":"6"}"#,
            "1",
            "1",
            0.16666667,
        ),
    ] {
        let mut h = started(&format!(
            r#"{{"code":"00000","data":[{ticker},{{"symbol":"ETHUSD","lastPr":"2500"}}]}}"#
        ));
        positions(
            &mut h,
            position("BTCUSD", native, entry, "100"),
            position("ETHUSD", "0", "2000", "100"),
        );
        let p = h
            .engine
            .store()
            .positions()
            .iter()
            .find(|p| p.symbol == "BTCUSD")
            .unwrap();
        assert_eq!(p.contracts, expected, "ticker={ticker}, entry={entry}");
        assert_eq!(p.notional, native.parse::<f64>().unwrap());
    }
}

#[test]
fn inverse_position_row_mark_sizes_coin_quantity_before_any_ticker_loads() {
    let mut h = started(r#"{"code":"00000","data":[{"symbol":"ETHUSD","lastPr":"2500"}]}"#);
    assert!(h.engine.store().ticker("BTCUSD").is_none());
    let mut btc = position("BTCUSD", "1000", "25000", "100");
    btc["markPrice"] = json!("40000");
    positions(&mut h, btc, position("ETHUSD", "0", "2000", "100"));
    let p = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|p| p.symbol == "BTCUSD")
        .unwrap();
    assert_eq!((p.contracts, p.notional), (0.025, 1000.0));
}

#[test]
fn inverse_orders_convert_only_remaining_and_keep_native_extra_and_fill_units() {
    let mut h = ready();
    h.login();
    h.engine.take_frame(h.now);
    let mut row = order("native-fill", "50000");
    row["tradeId"] = json!("exec-1");
    row["priceAvg"] = json!("49000");
    ws_order(&mut h, row.clone());
    ws_order(&mut h, row);
    let o = h.engine.store().order("native-fill").unwrap();
    assert_eq!((o.amount, o.filled, o.remaining), (1000.0, 100.0, 0.018));
    assert_eq!(o.extra.as_ref().unwrap().remaining_native, Some(900.0));
    let frame = h.engine.take_frame(h.now).unwrap().to_vec();
    let (_, payload) = wire::sections(&frame)
        .find(|(kind, _)| *kind == wire::section::FILLS)
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(payload[..4].try_into().unwrap()),
        1,
        "execution must dedupe"
    );
    assert_eq!(
        f64::from_le_bytes(payload[16..24].try_into().unwrap()),
        49000.0
    );
    assert_eq!(
        f64::from_le_bytes(payload[24..32].try_into().unwrap()),
        100.0
    );
    ws_order(&mut h, order("no-limit-price", "0"));
    assert_eq!(
        h.engine.store().order("no-limit-price").unwrap().remaining,
        900.0
    );
}

#[test]
fn inverse_balance_aggregates_coins_with_explicit_usd_and_ticker_fallback() {
    let h = ready();
    let b = h.engine.store().balance();
    assert_eq!(
        (b.total, b.upnl, b.used, b.free),
        (102000.0, 5500.0, 27500.0, 80000.0)
    );
    let coins = h.engine.store().coin_balances().unwrap();
    assert_eq!(coins.len(), 2);
    assert_eq!(
        (&*coins[0].base, coins[0].amount, coins[0].notional),
        ("BTC", 2.0, 100000.0)
    );
    assert_eq!(
        (&*coins[1].base, coins[1].amount, coins[1].notional),
        ("ETH", 3.0, 7500.0)
    );
}

#[test]
fn inverse_balance_without_usd_price_never_publishes_coin_amounts_as_usd() {
    let mut h = started(TICKERS);
    h.respond("/account/accounts", 200, r#"{"code":"00000","data":[{"marginCoin":"SOL","accountEquity":"4","available":"3","unrealizedPL":"1"},{"marginCoin":"BTC","accountEquity":"0","available":"0","unrealizedPL":"0","usdtEquity":"10"}]}"#);
    assert!(!h.engine.store().loaded().balance);
    assert_eq!(h.engine.store().balance().total, 0.0);
    let coins = h.engine.store().coin_balances().unwrap();
    assert_eq!(
        (&*coins[0].base, coins[0].amount, coins[0].notional),
        ("SOL", 4.0, 0.0)
    );
    assert_eq!(coins[1].amount, 0.0);
    assert!(h.logs.iter().any(|m| m.contains("USD price unavailable")));

    h.login();
    h.private_message(r#"{"arg":{"channel":"account"},"action":"update","data":[{"marginCoin":"SOL","accountEquity":"4","available":"3","unrealizedPL":"1","usdtEquity":"400"}]}"#);
    let b = h.engine.store().balance().clone();
    assert_eq!(
        (b.total, b.upnl, b.used, b.free),
        (300.0, 100.0, 100.0, 300.0)
    );
    // An unpriced later amount keeps the last real USD valuation.
    h.private_message(r#"{"arg":{"channel":"account"},"action":"update","data":[{"marginCoin":"SOL","accountEquity":"5","available":"4","unrealizedPL":"1"}]}"#);
    assert_eq!(*h.engine.store().balance(), b);
    let sol = h
        .engine
        .store()
        .coin_balances()
        .unwrap()
        .iter()
        .find(|coin| coin.base == "SOL")
        .unwrap()
        .clone();
    assert_eq!((sol.amount, sol.notional), (5.0, 400.0));
}

#[test]
fn inverse_ws_balances_merge_coin_updates_and_block_stale_rest() {
    let mut h = ready();
    h.login();
    let stale = h.take("/account/accounts");
    h.now += 1.0;
    h.private_message(r#"{"arg":{"channel":"account"},"action":"update","data":[{"marginCoin":"ETH","accountEquity":"4","available":"3","unrealizedPL":"0.2","usdtEquity":"10000"}]}"#);
    h.reply(stale.id, 200, BALANCE);
    let b = h.engine.store().balance();
    assert_eq!(
        (b.total, b.upnl, b.used, b.free),
        (104500.0, 5500.0, 27500.0, 82500.0)
    );
    let coins = h.engine.store().coin_balances().unwrap();
    assert_eq!(coins.len(), 2);
    assert!(coins.iter().any(|c| c.base == "BTC" && c.amount == 2.0));
    assert!(coins.iter().any(|c| c.base == "ETH" && c.amount == 4.0));
    h.private_message(r#"{"arg":{"channel":"account"},"action":"snapshot","data":[]}"#);
    assert_eq!(h.engine.store().coin_balances().unwrap().len(), 0);
    assert_eq!(h.engine.store().balance().total, 0.0);
}

#[test]
fn inverse_coin_position_failure_preserves_full_previous_snapshot() {
    let mut h = ready();
    h.login();
    let before = h.engine.store().positions().to_vec();
    h.respond(
        "marginCoin=BTC",
        200,
        &json!({"code":"00000","data":[position("BTCUSD","2000","25000","200")]}).to_string(),
    );
    h.respond(
        "marginCoin=ETH",
        200,
        r#"{"code":"40001","msg":"bad key","data":[]}"#,
    );
    assert_eq!(h.engine.store().positions(), before);
    assert!(h.logs.iter().any(|m| m.contains("40001")));
}

#[test]
fn inverse_ws_position_conversion_survives_older_rest_completion() {
    let mut h = ready();
    h.login();
    let stale = h.take("marginCoin=BTC");
    h.now += 1.0;
    h.private_message(&json!({"arg":{"channel":"positions"},"action":"update","data":[position("BTCUSD","1500","25000","200")]}).to_string());
    h.reply(
        stale.id,
        200,
        &json!({"code":"00000","data":[position("BTCUSD","1000","25000","100")]}).to_string(),
    );
    h.respond(
        "marginCoin=ETH",
        200,
        &json!({"code":"00000","data":[position("ETHUSD","333","2000","100")]}).to_string(),
    );
    let rows = h.engine.store().positions();
    assert_eq!(rows.len(), 2);
    let btc = rows.iter().find(|p| p.symbol == "BTCUSD").unwrap();
    assert_eq!((btc.contracts, btc.notional), (0.03, 1500.0));
}

#[test]
fn inverse_late_uta_error_never_promotes_or_calls_v3_private_routes() {
    let mut h = ready();
    h.login();
    let before = h.engine.store().balance().clone();
    h.respond(
        "/account/accounts",
        200,
        r#"{"code":"40085","msg":"Unified Account mode does not support Classic API","data":[]}"#,
    );
    assert_eq!(h.engine.store().balance(), &before);
    assert!(!h.requests.iter().any(|r| r.url.contains("/api/v3/")));
    assert!(!h.sockets.iter().any(|(_, url)| url.contains("/v3/")));
    assert!(h.sent.iter().any(|(_, body)| body.contains("COIN-FUTURES")));
}

#[test]
fn inverse_uta_probe_success_reports_scoped_unsupported_before_private_routing() {
    let mut h = Harness::started_expecting(
        Options {
            product: Product::Inverse,
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..Options::default()
        },
        true,
    );
    h.respond("/contracts", 200, CONTRACTS);
    h.respond("/tickers", 200, TICKERS);
    let probe = h.take("/account/settings");
    assert!(probe.url.contains("/api/v3/account/settings"));
    h.reply(
        probe.id,
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    assert!(
        h.logs
            .iter()
            .any(|m| m.contains("inverse") && m.contains("UTA") && m.contains("unsupported"))
    );
    assert!(!h.requests.iter().any(|r| r.url.contains("/api/v3/")
        || r.url.contains("/api/v2/mix/account/")
        || r.url.contains("/position/")));
    assert!(!h.sockets.iter().any(|(_, url)| url.contains("/private")));
    let loaded = h.engine.store().loaded();
    assert!(!loaded.balance && !loaded.positions && !loaded.orders);
    assert_eq!(
        h.startup_failures.len(),
        1,
        "fails startup instead of stalling"
    );
    assert_eq!(
        h.startup_failures[0].kind,
        Some(safe_cex_engine::ErrorKind::Unsupported)
    );
}

#[test]
fn inverse_malformed_or_failed_balances_never_drop_other_collateral() {
    let mut h = ready();
    h.login();
    let before = h.engine.store().balance().clone();
    let coins = h.engine.store().coin_balances().unwrap().to_vec();
    h.respond(
        "/account/accounts",
        200,
        r#"{"code":"00000","data":[{"marginCoin":"BTC","accountEquity":"NaN","available":"1"}]}"#,
    );
    assert_eq!(h.engine.store().balance(), &before);
    assert_eq!(h.engine.store().coin_balances().unwrap(), coins);
    h.private_message(r#"{"arg":{"channel":"account"},"action":"update","data":[{"marginCoin":"BTC","accountEquity":"bad","available":"1"}]}"#);
    assert_eq!(h.engine.store().balance(), &before);
    assert_eq!(h.engine.store().coin_balances().unwrap(), coins);
}

#[test]
fn inverse_duplicate_collateral_snapshot_never_double_counts_or_breaks_frame_decoding() {
    let mut h = ready();
    h.login();
    let balance = h.engine.store().balance().clone();
    let coins = h.engine.store().coin_balances().unwrap().to_vec();
    let duplicate = json!({
        "marginCoin":"BTC", "accountEquity":"2", "available":"1.5",
        "unrealizedPL":"0.1", "usdtEquity":"100000"
    });
    h.respond(
        "/account/accounts",
        200,
        &json!({"code":"00000","data":[duplicate.clone(), duplicate.clone()]}).to_string(),
    );
    assert_eq!(h.engine.store().balance(), &balance);
    assert_eq!(h.engine.store().coin_balances().unwrap(), coins);
    h.private_message(&json!({"arg":{"channel":"account"},"action":"snapshot","data":[duplicate.clone(),duplicate]}).to_string());
    assert_eq!(h.engine.store().balance(), &balance);
    assert_eq!(h.engine.store().coin_balances().unwrap(), coins);
}

#[test]
fn inverse_empty_rest_balance_keeps_current_account_and_bad_ws_batch_is_atomic() {
    let mut h = ready();
    h.login();
    let balance = h.engine.store().balance().clone();
    let coins = h.engine.store().coin_balances().unwrap().to_vec();
    h.respond("/account/accounts", 200, r#"{"code":"00000","data":[]}"#);
    assert_eq!(h.engine.store().balance(), &balance);
    assert_eq!(h.engine.store().coin_balances().unwrap(), coins);
    h.private_message(r#"{"arg":{"channel":"account"},"action":"update","data":[{"marginCoin":"BTC","accountEquity":"20","available":"15","unrealizedPL":"1"},{"marginCoin":"ETH","accountEquity":"bad","available":"1"}]}"#);
    assert_eq!(h.engine.store().balance(), &balance);
    assert_eq!(h.engine.store().coin_balances().unwrap(), coins);
}

#[test]
fn inverse_native_amend_keeps_remaining_in_display_units_and_native_extra_current() {
    for resolve_identity in [false, true] {
        let mut h = ready();
        h.login();
        ws_order(&mut h, order("old-amend", "50000"));
        h.command(Command::AmendOrder {
            command: 99,
            id: "old-amend".into(),
            price: Some(40000.0),
            amount: Some(1200.0),
        });
        let request = h.take("/modify-order");
        let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload["newSize"], "1200");
        assert_eq!(payload["newPrice"], "40000");
        assert_ne!(payload["newClientOid"], "native-sizing");
        if resolve_identity {
            h.reply(
                request.id,
                200,
                r#"{"code":"00000","data":{"orderId":"old-amend"}}"#,
            );
            let lookup = h.take("/order/detail");
            h.reply(lookup.id, 200, r#"{"code":"00000","data":{"orderId":"new-amend","baseVolume":"200","status":"live"}}"#);
        } else {
            h.reply(
                request.id,
                200,
                r#"{"code":"00000","data":{"orderId":"new-amend"}}"#,
            );
        }
        let order = h.engine.store().order("new-amend").unwrap();
        let native_remaining = if resolve_identity { 1000.0 } else { 1100.0 };
        assert_eq!(order.amount, 1200.0);
        assert_eq!(order.remaining, native_remaining / 40000.0);
        assert_eq!(
            order.extra.as_ref().unwrap().remaining_native,
            Some(native_remaining)
        );
        assert!(
            h.done
                .iter()
                .any(|(id, result)| *id == 99 && result.is_ok())
        );
    }
}

#[test]
fn inverse_position_zero_row_mark_falls_through_to_the_row_market_price() {
    let mut h = started(r#"{"code":"00000","data":[{"symbol":"ETHUSD","lastPr":"2500"}]}"#);
    let mut btc = position("BTCUSD", "1000", "25000", "100");
    btc["markPrice"] = json!("0");
    btc["marketPrice"] = json!("50000");
    positions(&mut h, btc, position("ETHUSD", "0", "2000", "100"));
    let p = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|p| p.symbol == "BTCUSD")
        .unwrap();
    assert_eq!(p.contracts, 0.02);
}

#[test]
fn an_empty_coin_futures_catalogue_stalls_account_startup_and_keeps_retrying() {
    let mut h = Harness::started(Options {
        product: Product::Inverse,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    h.respond("/contracts", 200, r#"{"code":"00000","data":[]}"#);
    assert!(!h.engine.store().loaded().markets);
    assert!(h.engine.store().markets().is_empty());
    assert!(h.sockets.is_empty(), "{:?}", h.sockets);
    assert!(h.requests.is_empty(), "no tickers or account read follows");

    let (timer, at) = h.timers.pop().expect("a retry is scheduled");
    h.now = at;
    h.event(Event::TimerFired { timer });
    assert!(
        h.take("/contracts")
            .url
            .contains("productType=coin-futures")
    );
    assert!(h.requests.is_empty());
}
