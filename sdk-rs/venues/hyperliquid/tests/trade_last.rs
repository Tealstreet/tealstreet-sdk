//! A watched coin's `last` is its newest `trades` print; every other coin's is the mid.
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, Event, SocketId};
use safe_cex_venue_hyperliquid::Hyperliquid;
use support::*;

const BTC_TRADES: &str = r#"{"method":"subscribe","subscription":{"type":"trades","coin":"BTC"}}"#;

fn sends(e: &mut Engine<Hyperliquid>) -> Vec<String> {
    prepared_now(e)
        .into_iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}
fn trades(coin: &str, px: &str, time: u64) -> String {
    format!(
        r#"{{"channel":"trades","data":[{{"coin":"{coin}","side":"B","px":"{px}","sz":"0.1","time":{time},"hash":"0x0","tid":{time}}}]}}"#
    )
}
fn mids(e: &mut Engine<Hyperliquid>, s: SocketId, now: f64, mid: &str) {
    message(
        e,
        s,
        now,
        &format!(r#"{{"channel":"allMids","data":{{"mids":{{"BTC":"{mid}"}}}}}}"#),
    );
}
fn command(e: &mut Engine<Hyperliquid>, now: f64, command: Command) {
    e.handle(now, Event::Command(command));
}
fn book(subscribe: bool) -> Command {
    let symbol = "BTC-PERP".into();
    if subscribe {
        Command::SubscribeOrderBook { symbol }
    } else {
        Command::UnsubscribeOrderBook { symbol }
    }
}
fn chart(subscribe: bool) -> Command {
    let (symbol, interval) = ("BTC-PERP".into(), Interval::M1);
    if subscribe {
        Command::SubscribeCandles { symbol, interval }
    } else {
        Command::UnsubscribeCandles { symbol, interval }
    }
}
fn last(e: &Engine<Hyperliquid>) -> (f64, Option<f64>) {
    let ticker = e.store().ticker("BTC-PERP").unwrap();
    (ticker.last, ticker.mid_price)
}

#[test]
fn books_and_charts_share_one_trades_stream_per_coin() {
    let (mut e, _) = ready("disabled");
    sends(&mut e);
    command(&mut e, 2000.0, book(true));
    assert!(sends(&mut e).iter().any(|text| text == BTC_TRADES));
    command(&mut e, 2100.0, chart(true));
    assert!(!sends(&mut e).iter().any(|text| text.contains("\"trades\"")));
    command(&mut e, 2200.0, book(false));
    assert!(
        !sends(&mut e).iter().any(|text| text.contains("\"trades\"")),
        "the chart still watches BTC"
    );
    command(&mut e, 2300.0, chart(false));
    assert!(sends(&mut e).contains(&BTC_TRADES.replacen("subscribe", "unsubscribe", 1)));
}

#[test]
fn a_watched_coin_keeps_the_mid_until_its_first_trade_then_mids_never_clobber_it() {
    let (mut e, s) = ready("disabled");
    message(&mut e, s, 1500.0, &trades("BTC", "104", 1500));
    mids(&mut e, s, 1600.0, "101");
    assert_eq!(
        last(&e),
        (101.0, Some(101.0)),
        "unwatched prints are ignored"
    );
    command(&mut e, 2000.0, book(true));
    mids(&mut e, s, 2100.0, "102");
    assert_eq!(last(&e), (102.0, Some(102.0)), "mid until the first trade");
    message(&mut e, s, 2200.0, &trades("BTC", "105", 2200));
    assert_eq!(last(&e), (105.0, Some(102.0)));
    mids(&mut e, s, 2300.0, "110");
    message(
        &mut e,
        s,
        2400.0,
        r#"{"channel":"fastAssetCtxs","data":"q1ZyCnFWsqpWyk0syg6oULJSsjQ3NTDQM1Wq1VFyDfFAkTI2MzXQMwJLVVRWWfmFuTiiyBuamOoZKdXWAgA="}"#,
    );
    assert_eq!(last(&e), (105.0, Some(110.0)), "mids move mid_price only");
    message(&mut e, s, 2500.0, &trades("BTC", "99", 2100));
    assert_eq!(
        last(&e).0,
        105.0,
        "an older print does not replace a newer one"
    );
    command(&mut e, 2600.0, book(false));
    mids(&mut e, s, 2700.0, "111");
    assert_eq!(
        last(&e),
        (111.0, Some(111.0)),
        "unwatched: last is the mid again"
    );
}

#[test]
fn a_reconnect_returns_last_to_the_mid_until_the_next_trade() {
    let (mut e, s) = ready("disabled");
    command(&mut e, 2000.0, book(true));
    message(&mut e, s, 2100.0, &trades("BTC", "105", 2100));
    let (s, now) = reconnect(&mut e, s, 3000.0);
    mids(&mut e, s, now + 1.0, "107");
    assert_eq!(last(&e), (107.0, Some(107.0)));
    message(&mut e, s, now + 2.0, &trades("BTC", "106", 4000));
    assert_eq!(last(&e), (106.0, Some(107.0)));
}

#[test]
fn a_trade_print_moves_neither_mark_nor_live_upnl() {
    let (mut e, s) = ready("disabled");
    command(&mut e, 2000.0, book(true));
    mids(&mut e, s, 2100.0, "110");
    let mark = e.store().ticker("BTC-PERP").unwrap().mark;
    let balance = e.store().balance().clone();
    assert_eq!(balance.upnl, 20.0);
    message(&mut e, s, 2200.0, &trades("BTC", "150", 2200));
    assert_eq!(last(&e), (150.0, Some(110.0)));
    assert_eq!(e.store().ticker("BTC-PERP").unwrap().mark, mark);
    assert_eq!(e.store().balance().upnl, 20.0, "uPnL values at the mid");
    assert_eq!(e.store().balance().free, balance.free);
}
