//! Classic account WS uses equity rather than REST accountEquity:
//! https://www.bitget.com/api-doc/classic/contract/websocket/private/Account-Channel
use crate::support::FixtureOptions as Options;
mod support;

use safe_cex_engine::Event;
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::Harness;

const CONTRACTS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSD","baseCoin":"BTC","quoteCoin":"USD","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"1"},{"symbol":"ETHUSD","baseCoin":"ETH","quoteCoin":"USD","symbolStatus":"normal","pricePlace":"2","priceEndStep":"1","sizeMultiplier":"1"}]}"#;
const TICKERS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSD","lastPr":"50000"},{"symbol":"ETHUSD","lastPr":"2500"}]}"#;
const BALANCE: &str = r#"{"code":"00000","data":[{"marginCoin":"BTC","accountEquity":"2","available":"1.5","unrealizedPL":"0.1","usdtEquity":"100000"},{"marginCoin":"ETH","accountEquity":"3","available":"2","unrealizedPL":"0.2","usdtEquity":"7500"}]}"#;

fn position(symbol: &str, total: u32, updated: u32) -> Value {
    json!({
        "symbol": symbol, "holdSide": "long", "total": total.to_string(),
        "openPriceAvg": "2000", "markPrice": "2500", "leverage": "20",
        "posMode": "one_way_mode", "marginMode": "crossed", "uTime": updated.to_string()
    })
}

fn reply_positions(h: &mut Harness, coin: &str, total: u32) {
    h.respond(
        &format!("marginCoin={coin}"),
        200,
        &json!({"code":"00000","data":[position(&format!("{coin}USD"), total, 100)]}).to_string(),
    );
}

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        product: Product::Inverse,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    h.respond("/contracts", 200, CONTRACTS);
    h.respond("/tickers", 200, TICKERS);
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    reply_positions(&mut h, "BTC", 1000);
    reply_positions(&mut h, "ETH", 333);
    h.respond("/account/accounts", 200, BALANCE);
    h.orders(r#"{"code":"00000","data":{"entrustedList":[]}}"#);
    h.login();
    h
}

fn poll_at(h: &mut Harness, at: f64) {
    let (timer, _) = h
        .timers
        .iter()
        .copied()
        .find(|(_, time)| *time == at)
        .unwrap();
    h.timers.retain(|(id, _)| *id != timer);
    h.now = at;
    h.event(Event::TimerFired { timer });
}

#[test]
fn inverse_single_currency_ws_equity_updates_all_collateral_and_blocks_stale_rest() {
    let mut h = ready();
    let stale = h.take("/account/accounts");
    h.private_message(
        r#"{"action":"snapshot","arg":{"instType":"COIN-FUTURES","channel":"account","coin":"default"},"data":[{"marginCoin":"BTC","equity":"2","available":"1.5","unrealizedPL":"0.1","usdtEquity":"100000","assetsMode":"single"},{"marginCoin":"ETH","equity":"4","available":"3","unrealizedPL":"0.2","usdtEquity":"10000","assetsMode":"single"}]}"#,
    );
    h.reply(stale.id, 200, BALANCE);
    let balance = h.engine.store().balance();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (104500.0, 82500.0, 27500.0, 5500.0)
    );
    let coins = h.engine.store().coin_balances().unwrap();
    assert_eq!(coins.len(), 2);
    assert!(coins.iter().any(|c| c.base == "BTC" && c.amount == 2.0));
    assert!(coins.iter().any(|c| c.base == "ETH" && c.amount == 4.0));
    // Incremental compatibility must retain the other collateral as well.
    h.private_message(
        r#"{"action":"update","arg":{"channel":"account"},"data":[{"marginCoin":"ETH","equity":"5","accountEquity":"5","available":"4","unrealizedPL":"0.2","usdtEquity":"12500"}]}"#,
    );
    let coins = h.engine.store().coin_balances().unwrap();
    assert_eq!(coins.len(), 2);
    assert!(coins.iter().any(|c| c.base == "BTC" && c.amount == 2.0));
    assert!(coins.iter().any(|c| c.base == "ETH" && c.amount == 5.0));
}

#[test]
fn delayed_inverse_coin_walk_retains_newer_position_updates_and_closures() {
    for updated_total in [1500, 0] {
        let mut h = ready();
        let started = h.now;
        reply_positions(&mut h, "BTC", 1000);
        let delayed_eth = h.take("marginCoin=ETH");
        h.now += 1.0;
        h.private_message(
            &json!({
                "arg":{"channel":"positions"}, "action":"update",
                "data":[position("BTCUSD", updated_total, 200)]
            })
            .to_string(),
        );
        for tick in 1..=7 {
            poll_at(&mut h, started + f64::from(tick) * 2000.0);
        }
        h.reply(
            delayed_eth.id,
            200,
            &json!({"code":"00000","data":[position("ETHUSD",333,100)]}).to_string(),
        );
        let btc = h
            .engine
            .store()
            .positions()
            .iter()
            .find(|p| p.symbol == "BTCUSD");
        if updated_total == 0 {
            assert!(btc.is_none(), "the delayed read must not reopen a position");
        } else {
            assert_eq!(btc.unwrap().notional, f64::from(updated_total));
        }
        // Once that walk completes, a new read is authoritative after WS grace.
        poll_at(&mut h, started + 16000.0);
        reply_positions(&mut h, "BTC", 800);
        reply_positions(&mut h, "ETH", 444);
        let positions = h.engine.store().positions();
        assert_eq!(positions.len(), 2);
        assert!(
            positions
                .iter()
                .any(|p| p.symbol == "BTCUSD" && p.notional == 800.0)
        );
        assert!(
            positions
                .iter()
                .any(|p| p.symbol == "ETHUSD" && p.notional == 444.0)
        );
    }
}
