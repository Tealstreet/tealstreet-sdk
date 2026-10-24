#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::command::{HostInput, decode_host_input};
use safe_cex_engine::model::Balance;
use safe_cex_engine::{Action, Engine, Event, PublicFeedPolicy, SocketId};
use safe_cex_venue_okx::Okx;

fn startup(market: &str, shared: bool) -> (Engine<Okx>, Action, SocketId, Vec<Action>) {
    let mut engine = crate::support::engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    if shared {
        engine
            .set_public_feed_policy(PublicFeedPolicy::Shared)
            .unwrap();
    }
    engine.start(0.0);
    let catalogue = take_http(&mut prepared(&mut engine, 0.0));
    reply(&mut engine, &catalogue, &envelope(market), 1.0);
    let out = prepared(&mut engine, 1.0);
    let ticker = out
        .iter()
        .find(|a| is_get(a, "tickers?instType=SWAP"))
        .unwrap()
        .clone();
    let config = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut engine,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        2.0,
    );
    let pending = prepared(&mut engine, 2.0);
    let private = pending
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    engine.handle(3.0, Event::WsOpened { socket: private });
    prepared(&mut engine, 3.0);
    ws(&mut engine, private, r#"{"event":"login","code":"0"}"#);
    prepared(&mut engine, 4.0);
    (
        engine,
        ticker,
        private,
        pending
            .into_iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .collect(),
    )
}

fn account(engine: &mut Engine<Okx>, pending: Vec<Action>, position: &str) {
    account_equity(
        engine,
        pending,
        position,
        r#"{"balData":[{"ccy":"BTC","eq":"1"}]}"#,
    );
}
fn account_equity(
    engine: &mut Engine<Okx>,
    mut pending: Vec<Action>,
    position: &str,
    equity: &str,
) {
    while let Some(request) = pending.pop() {
        let Action::Http(http) = &request else {
            continue;
        };
        let body = if http.url.contains("account-position-risk") {
            envelope(equity)
        } else if http.url.contains("/positions?") {
            envelope(position)
        } else {
            envelope("")
        };
        reply(engine, &request, &body, 5.0);
        pending.extend(
            prepared(engine, 5.0)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
}

#[test]
fn native_currency_waits_for_real_rest_price_without_one_dollar_fallback() {
    let (mut engine, ticker, _, pending) = startup(MARKET, false);
    account(&mut engine, pending, "");
    assert!(engine.store().loaded().positions && engine.store().loaded().orders);
    assert!(!engine.store().loaded().balance);
    assert_eq!(
        engine.store().currency_balances().unwrap()[0].balance.total,
        1.0
    );
    reply(&mut engine, &ticker, &envelope(TICKER), 6.0);
    assert_eq!(engine.store().balance().total, 40000.0);
    assert!(engine.store().loaded().balance);
}

#[test]
fn unavailable_price_retains_previous_usd_and_native_account_equity_recovers() {
    let (mut engine, _, private, pending) = startup(MARKET, false);
    let old = Balance {
        total: 1234.0,
        free: 1200.0,
        used: 34.0,
        upnl: 0.0,
    };
    engine.store_mut().set_balance(old.clone());
    account(&mut engine, pending, "");
    assert_eq!(*engine.store().balance(), old);
    ws(
        &mut engine,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"50000"}]}"#,
    );
    assert_eq!(engine.store().balance().total, 50000.0);
}

#[test]
fn shared_quote_recovers_cold_wallet_without_owned_public_socket() {
    let (mut engine, _, _, pending) = startup(MARKET, true);
    account(&mut engine, pending, "");
    assert!(!engine.store().loaded().balance);
    let HostInput::PublicFeedUpdate(update) = decode_host_input(
        r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"BTCUSDT","last":42000}]}"#,
    )
    .unwrap() else {
        panic!("shared update")
    };
    engine.apply_public_feed(6.0, update).unwrap();
    assert_eq!(engine.store().balance().total, 42000.0);
}

#[test]
fn native_linear_mark_proves_wallet_fx_and_first_public_price_supersedes_it() {
    let (mut engine, ticker, _, pending) = startup(MARKET, false);
    account(
        &mut engine,
        pending,
        r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","markPx":"50000","lever":"10","upl":"50","imr":"200","mgnMode":"cross"}"#,
    );
    assert_eq!(engine.store().positions()[0].notional, 2500.0);
    assert_eq!(engine.store().balance().total, 49950.0);
    reply(&mut engine, &ticker, &envelope(TICKER), 6.0);
    assert_eq!(engine.store().balance().total, 39950.0);
}

#[test]
fn native_inverse_mark_converts_lots_margin_and_currency_before_tickers() {
    let market = MARKET
        .replace("BTC-USDT-SWAP", "BTC-USD-SWAP")
        .replace("\"linear\"", "\"inverse\"")
        .replace("\"ctVal\":\"0.01\"", "\"ctVal\":\"100\"")
        .replace("\"settleCcy\":\"USDT\"", "\"settleCcy\":\"BTC\"")
        .replace("\"ctValCcy\":\"BTC\"", "\"ctValCcy\":\"USD\"");
    let (mut engine, ticker, _, pending) = startup(&market, false);
    account(
        &mut engine,
        pending,
        r#"{"instId":"BTC-USD-SWAP","pos":"5","posSide":"net","avgPx":"39000","markPx":"50000","ccy":"BTC","lever":"10","upl":"0.001","imr":"0.01","mgnMode":"cross"}"#,
    );
    assert_eq!(engine.store().positions()[0].contracts, 0.01);
    assert_eq!(engine.store().positions()[0].notional, 500.0);
    assert_eq!(engine.store().balance().total, 49950.0);
    assert_eq!(engine.store().balance().used, 500.0);
    assert_eq!(
        engine.store().currency_balances().unwrap()[0].balance.total,
        0.999
    );
    reply(
        &mut engine,
        &ticker,
        &envelope(&TICKER.replace("BTC-USDT-SWAP", "BTC-USD-SWAP")),
        6.0,
    );
    assert_eq!(engine.store().balance().total, 39960.0);
    assert_eq!(engine.store().balance().used, 400.0);
    assert_eq!(engine.store().balance().upnl, 40.0);
    assert_eq!(engine.store().positions()[0].contracts, 0.0125);
    assert_eq!(engine.store().positions()[0].unrealized_pnl, 40.0);
}

#[test]
fn entry_price_cannot_authorize_inverse_conversion_or_wallet_fx() {
    let (mut engine, _, _, pending) = startup(MARKET, false);
    account(
        &mut engine,
        pending,
        r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","imr":"200","mgnMode":"cross"}"#,
    );
    assert!(!engine.store().loaded().positions);
    assert!(!engine.store().loaded().balance);
}

#[test]
fn unknown_owned_currency_requires_native_usd_equity_instead_of_a_one_dollar_rate() {
    let (mut engine, ticker, private, pending) = startup(MARKET, false);
    reply(&mut engine, &ticker, &envelope(TICKER), 4.0);
    account_equity(
        &mut engine,
        pending,
        "",
        r#"{"balData":[{"ccy":"UNKNOWN","eq":"7"}]}"#,
    );
    assert!(!engine.store().loaded().balance);
    assert_eq!(
        engine.store().currency_balances().unwrap()[0].balance.total,
        7.0
    );
    ws(
        &mut engine,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"70"}]}"#,
    );
    assert_eq!(engine.store().balance().total, 70.0);
}
