//! Position WS frames describe their rows, never the identities omitted from them.
mod support;
use safe_cex_engine::{Command, VenueAction};
use serde_json::{Value, json};
use support::*;

fn row(symbol: &str, size: &str) -> Value {
    json!({"symbol":symbol,"marginCoin":"USDT","holdSide":"long","posSide":"long",
        "total":size,"size":size,"openPriceAvg":"90","avgPrice":"90","markPrice":"100",
        "leverage":"20","marginMode":"crossed","posMode":"one_way_mode",
        "holdMode":"one_way_mode","uTime":"200","updatedTime":"200"})
}
fn answer(unified: bool, rows: Value) -> String {
    if unified {
        json!({"code":"00000","data":{"list":rows}}).to_string()
    } else {
        json!({"code":"00000","data":rows}).to_string()
    }
}
fn path(unified: bool) -> &'static str {
    if unified {
        "/current-position"
    } else {
        "/all-position"
    }
}
fn start(unified: bool, demo: bool) -> Harness {
    let mut h = Harness::started(FixtureOptions {
        testnet: demo,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..FixtureOptions::default()
    });
    let mut contracts: Value = serde_json::from_str(CONTRACTS).unwrap();
    let mut eth = contracts["data"][0].clone();
    eth["symbol"] = json!("ETHUSDT");
    eth["baseCoin"] = json!("ETH");
    contracts["data"].as_array_mut().unwrap().push(eth);
    let contracts = contracts.to_string();
    h.respond(
        "/contracts",
        200,
        &if demo {
            contracts.replace("USDT", "SUSDT")
        } else {
            contracts
        },
    );
    h.respond(
        "/tickers",
        200,
        &if demo {
            TICKERS.replace("USDT", "SUSDT")
        } else {
            TICKERS.to_owned()
        },
    );
    h.respond(
        "/account/settings",
        200,
        if unified {
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#
        } else {
            r#"{"code":"40084","msg":"Classic Account mode","data":null}"#
        },
    );
    h.login();
    h
}
fn seed(h: &mut Harness, unified: bool, rows: Value) {
    h.respond(path(unified), 200, &wire(h, answer(unified, rows)));
}
fn frame(h: &mut Harness, action: &str, rows: Value) {
    h.private_message(&wire(
        h,
        json!({"arg":{"channel":"positions","topic":"position"},
        "action":action,"data":rows})
        .to_string(),
    ));
}
fn size(h: &Harness, symbol: &str) -> Option<f64> {
    h.engine
        .store()
        .positions()
        .iter()
        .find(|p| p.symbol == wire(h, symbol.to_owned()))
        .map(|p| p.contracts)
}
fn wire(h: &Harness, text: String) -> String {
    if h.engine.store().market("BTCSUSDT").is_some() {
        text.replace("USDT", "SUSDT")
    } else {
        text
    }
}
fn modes() -> impl Iterator<Item = (bool, bool)> {
    [(false, false), (false, true), (true, false)].into_iter()
}

#[test]
fn split_and_empty_snapshots_never_remove_an_omitted_symbol() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        seed(
            &mut h,
            unified,
            json!([row("BTCUSDT", "1"), row("ETHUSDT", "2")]),
        );
        for _ in 0..3 {
            h.now += 5_000.0;
            frame(&mut h, "snapshot", json!([row("BTCUSDT", "3")]));
            assert_eq!(
                size(&h, "ETHUSDT"),
                Some(2.0),
                "first fragment erased ETH; UTA={unified}, demo={demo}"
            );
            assert_eq!(size(&h, "BTCUSDT"), Some(3.0));
            frame(&mut h, "snapshot", json!([row("ETHUSDT", "4")]));
            assert_eq!(size(&h, "BTCUSDT"), Some(3.0));
            assert_eq!(size(&h, "ETHUSDT"), Some(4.0));
            frame(&mut h, "snapshot", json!([]));
            frame(&mut h, "update", json!([]));
            assert_eq!(h.engine.store().positions().len(), 2, "empty is not flat");
            frame(&mut h, "update", json!([row("ETHUSDT", "2")]));
        }
    }
}

#[test]
fn a_partial_ws_snapshot_cannot_filter_a_rest_only_position_peer() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        let read = h.take(path(unified));
        h.now += 1.0;
        frame(&mut h, "snapshot", json!([row("BTCUSDT", "1")]));
        h.reply(
            read.id,
            200,
            &wire(
                &h,
                answer(unified, json!([row("BTCUSDT", "1"), row("ETHUSDT", "2")])),
            ),
        );
        assert_eq!(
            size(&h, "ETHUSDT"),
            Some(2.0),
            "WS omission suppressed REST peer; UTA={unified}, demo={demo}"
        );
    }
}

#[test]
fn empty_ws_snapshot_never_establishes_positions_readiness() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        assert!(!h.engine.store().loaded().positions);
        frame(&mut h, "snapshot", json!([]));
        assert!(
            !h.engine.store().loaded().positions,
            "a WS fragment manufactured readiness; UTA={unified}, demo={demo}"
        );
        seed(&mut h, unified, json!([]));
        assert!(
            h.engine.store().loaded().positions,
            "a clean REST census establishes readiness"
        );
    }
}

#[test]
fn explicit_zero_closes_only_its_identity_and_blocks_an_older_rest_row() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        seed(
            &mut h,
            unified,
            json!([row("BTCUSDT", "1"), row("ETHUSDT", "2")]),
        );
        h.command(Command::Venue {
            command: 41,
            action: VenueAction::ReadPositions,
        });
        let read = h.take(path(unified));
        h.now += 1.0;
        frame(&mut h, "update", json!([row("BTCUSDT", "0")]));
        assert_eq!(size(&h, "BTCUSDT"), None);
        assert_eq!(size(&h, "ETHUSDT"), Some(2.0));
        h.reply(
            read.id,
            200,
            &wire(
                &h,
                answer(unified, json!([row("BTCUSDT", "1"), row("ETHUSDT", "2")])),
            ),
        );
        assert_eq!(
            size(&h, "BTCUSDT"),
            None,
            "older REST must not reopen explicit zero"
        );
        assert_eq!(size(&h, "ETHUSDT"), Some(2.0));
        assert!(
            !h.done.iter().any(|(id, _)| *id == 41),
            "held REST is not a fresh-position completion"
        );
    }
}

#[test]
fn a_partial_snapshot_cannot_change_account_mode_over_a_retained_peer() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        seed(
            &mut h,
            unified,
            json!([row("BTCUSDT", "1"), row("ETHUSDT", "2")]),
        );
        let mut hedge = row("BTCUSDT", "3");
        hedge["posMode"] = json!("hedge_mode");
        hedge["holdMode"] = json!("hedge_mode");
        frame(&mut h, "snapshot", json!([hedge]));
        assert!(
            !h.engine.store().hedged(),
            "partial WS changed global mode; UTA={unified}, demo={demo}"
        );
        assert_eq!(size(&h, "BTCUSDT"), Some(1.0));
        assert_eq!(size(&h, "ETHUSDT"), Some(2.0));
    }
}

#[test]
fn ws_fragments_cannot_answer_read_positions_but_clean_rest_can() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        seed(&mut h, unified, json!([row("BTCUSDT", "1")]));
        h.command(Command::Venue {
            command: 42,
            action: VenueAction::ReadPositions,
        });
        frame(&mut h, "snapshot", json!([]));
        assert!(!h.done.iter().any(|(id, _)| *id == 42));
        seed(&mut h, unified, json!([]));
        assert!(
            h.done.iter().any(|(id, result)| *id == 42
                && result
                    .as_ref()
                    .is_ok_and(|body| body == r#"{"skipped":[],"open":[]}"#)),
            "fresh REST absence answers; UTA={unified}, demo={demo}"
        );
        assert_eq!(size(&h, "BTCUSDT"), None);
    }
}

#[test]
fn terminal_orders_and_partial_fills_do_not_close_a_whole_position() {
    for (unified, demo) in modes() {
        let mut h = start(unified, demo);
        seed(&mut h, unified, json!([row("BTCUSDT", "1")]));
        for status in ["partially_filled", "filled", "canceled"] {
            let text = json!({"arg":{"channel":"orders","topic":"order"},"data":[{
                "category":if demo {"susdt-futures"} else {"usdt-futures"},
                "orderId":"1","symbol":"BTCUSDT","size":"1","qty":"1",
                "price":"90","side":"sell","orderType":"limit","status":status,
                "orderStatus":status,"baseVolume":"0.2","accBaseVolume":"0.2",
                "priceAvg":"90","tradeId":"trade"}]})
            .to_string();
            h.private_message(&wire(&h, text));
            assert_eq!(size(&h, "BTCUSDT"), Some(1.0), "terminal order is not flat");
        }
        if unified {
            h.private_message(r#"{"arg":{"topic":"fill"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"1","execId":"partial","execPrice":"90","execQty":"0.2","side":"sell"}]}"#);
            assert_eq!(
                size(&h, "BTCUSDT"),
                Some(1.0),
                "execution is not full-position closure"
            );
        }
    }
}
