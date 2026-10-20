//! Scripted info reads only: no transport, credentials or real trading account.
use super::*;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice};

const UNIVERSE: &str = r#"{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":20}]}"#;
const GOOD: &str = r#"[{"midPx":"100","markPx":"100"},{"midPx":"200","markPx":"200"}]"#;
const ETH_BAD: &str = r#"[{"midPx":"100","markPx":"100"},{"midPx":"bad"}]"#;
const BTC_BAD: &str = r#"[{"midPx":"bad"},{"midPx":"200","markPx":"200"}]"#;
const SPOT_META: &str = r#"{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"BTC","index":1,"szDecimals":5},{"name":"ETH","index":2,"szDecimals":4}],"universe":[{"name":"@4","tokens":[1,0],"index":4},{"name":"@9","tokens":[2,0],"index":9}]}"#;
fn tuple(meta: &str, contexts: &str) -> String {
    format!("[{meta},{contexts}]")
}
fn reply(e: &mut Engine<Hyperliquid>, req: &HttpRequest, at: f64, body: &str) {
    e.handle(
        at,
        Event::HttpResponse {
            request: req.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn http(actions: &[Action], kind: &str) -> HttpRequest {
    #[derive(serde::Deserialize)]
    struct Read {
        #[serde(rename = "type")]
        kind: String,
    }
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r)
                if serde_json::from_str::<Read>(r.body.as_deref()?).ok()?.kind == kind =>
            {
                Some(r.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing request {kind}"))
}
fn retry(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("public context backoff")
}
fn connect(spot: bool, private: bool) -> Engine<Hyperliquid> {
    let options = Options {
        testnet: true,
        public_data_only: !private,
        // Well-known scripted test key; no account transport exists.
        key: private.then(|| "0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: private
            .then(|| "0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: private.then(|| "0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Default::default()
    };
    let mut e = Engine::new(if spot {
        Hyperliquid::new_spot(options)
    } else {
        Hyperliquid::new(options)
    });
    e.start(1000.0);
    e
}
fn notice(actions: &[Action], text: &str) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains(text)))
}
#[test]
fn all_unusable_contexts_retry_without_claiming_an_empty_catalogue() {
    for spot in [false, true] {
        let mut e = connect(spot, false);
        let kind = if spot {
            "spotMetaAndAssetCtxs"
        } else {
            "metaAndAssetCtxs"
        };
        let meta = if spot { SPOT_META } else { UNIVERSE };
        let req = http(&e.drain_actions().collect::<Vec<_>>(), kind);
        reply(
            &mut e,
            &req,
            1000.0,
            &tuple(meta, r#"[{"midPx":"bad"},{"midPx":"bad"}]"#),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        let timer = retry(&actions, 2000.0);
        assert!(!e.store().loaded().markets);
        assert!(!e.store().loaded().tickers);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
        e.handle(2000.0, Event::TimerFired { timer });
        let req = http(&e.drain_actions().collect::<Vec<_>>(), kind);
        reply(&mut e, &req, 2000.0, &tuple(meta, GOOD));
        assert_eq!(e.store().markets().len(), 2);
        assert_eq!(e.store().tickers().len(), 2);
        assert!(e.store().loaded().markets);
    }
}
#[test]
fn recovering_a_missing_context_preserves_quotes_received_while_the_read_was_pending() {
    for spot in [false, true] {
        let mut e = connect(spot, false);
        let kind = if spot {
            "spotMetaAndAssetCtxs"
        } else {
            "metaAndAssetCtxs"
        };
        let meta = if spot { SPOT_META } else { UNIVERSE };
        let req = http(&e.drain_actions().collect::<Vec<_>>(), kind);
        reply(&mut e, &req, 1000.0, &tuple(meta, ETH_BAD));
        let mut actions = e.drain_actions().collect::<Vec<_>>();
        let timer = retry(&actions, 2000.0);
        if !spot {
            let req = http(&actions, "perpDexs");
            reply(&mut e, &req, 1000.0, "[null]");
            actions = e.drain_actions().collect();
        }
        let socket = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        e.handle(1000.0, Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        e.handle(2000.0, Event::TimerFired { timer });
        let req = http(&e.drain_actions().collect::<Vec<_>>(), kind);
        let text = if spot {
            r#"{"channel":"allMids","data":{"mids":{"@4":"240"}}}"#
        } else {
            r#"{"channel":"allMids","data":{"mids":{"BTC":"240"}}}"#
        };
        e.handle(2100.0, Event::WsMessage { socket, text });
        reply(&mut e, &req, 2200.0, &tuple(meta, GOOD));
        let btc = if spot { "BTC/USDC" } else { "BTC-PERP" };
        let eth = if spot { "ETH/USDC" } else { "ETH-PERP" };
        assert_eq!(e.store().ticker(btc).unwrap().last, 240.0);
        assert_eq!(e.store().ticker(eth).unwrap().last, 200.0);
        assert_eq!(e.store().tickers().len(), 2);
    }
}
#[test]
fn partial_perp_context_retries_upserts_and_preserves_known_rails_and_native_indices() {
    let mut e = connect(false, false);
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let first = retry(&actions, 2000.0);
    let dexs = http(&actions, "perpDexs");
    let btc = e.store().market("BTC-PERP").unwrap().clone();
    assert_eq!(e.venue().assets["BTC-PERP"], 0);
    assert!(!e.venue().assets.contains_key("ETH-PERP"));
    reply(&mut e, &dexs, 1000.0, "[null]");
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        1
    );
    e.handle(2000.0, Event::TimerFired { timer: first });
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, BTC_BAD));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let second = retry(&actions, 4000.0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
    assert_eq!(e.store().market("BTC-PERP"), Some(&btc));
    assert_eq!(e.venue().assets["BTC-PERP"], 0);
    assert_eq!(e.venue().assets["ETH-PERP"], 1);
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    e.handle(4000.0, Event::TimerFired { timer: second });
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 4000.0, &tuple(UNIVERSE, GOOD));
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    assert!(e.drain_actions().all(|a| !matches!(
        a,
        Action::Http(_) | Action::SetTimer { .. } | Action::WsConnect { .. }
    )));
}
#[test]
fn partial_hip3_context_keeps_dex_offset_and_does_not_advance_discovery_twice() {
    let mut e = connect(false, false);
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 1000.0, r#"[{"universe":[]},[]]"#);
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "perpDexs");
    reply(&mut e, &req, 1000.0, r#"[null,{"name":"xyz"}]"#);
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let timer = retry(&actions, 2000.0);
    assert_eq!(e.venue().assets["XYZ|BTC-PERP"], 110000);
    assert_eq!(e.venue().coins["xyz:btc"], "XYZ|BTC-PERP");
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, GOOD));
    assert_eq!(e.venue().assets["XYZ|ETH-PERP"], 110001);
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}
#[test]
fn unmapped_owned_position_rejects_authoritative_snapshot_and_recovers_without_stamping_bad_time() {
    let mut e = connect(false, true);
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let timer = retry(&actions, 2000.0);
    let req = http(&actions, "perpDexs");
    reply(&mut e, &req, 1000.0, "[null]");
    let actions = e.drain_actions().collect::<Vec<_>>();
    let state = http(&actions, "clearinghouseState");
    assert!(!e.store().loaded().positions);
    assert!(!e.store().loaded().orders);
    assert!(!e.store().loaded().balance);
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let known = r#"{"assetPositions":[{"position":{"coin":"BTC","szi":"2","entryPx":"100","positionValue":"200","unrealizedPnl":"0","marginUsed":"20","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"20"},"time":1000}"#;
    reply(&mut e, &state, 1000.0, known);
    let before = e.store().positions().to_vec();
    let balance = e.store().balance().clone();
    e.handle(1000.0, Event::WsOpened { socket });
    let state = http(&e.drain_actions().collect::<Vec<_>>(), "clearinghouseState");
    let unknown = r#"{"assetPositions":[{"position":{"coin":"ETH","szi":"3","entryPx":"200","positionValue":"600","unrealizedPnl":"0","marginUsed":"60","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"999999","totalMarginUsed":"60"},"time":999999}"#;
    reply(&mut e, &state, 1500.0, unknown);
    assert_eq!(e.store().positions(), before);
    assert_eq!(*e.store().balance(), balance);
    assert!(notice(
        &e.drain_actions().collect::<Vec<_>>(),
        "owned position has unavailable market metadata"
    ));
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, GOOD));
    let state = http(&e.drain_actions().collect::<Vec<_>>(), "clearinghouseState");
    reply(&mut e, &state, 2000.0, &unknown.replace("999999", "2000"));
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().position("ETH-PERP").unwrap().contracts, 3.0);
    assert_eq!(e.venue().assets["ETH-PERP"], 1);
}
#[test]
fn partial_spot_context_preserves_known_rows_retries_and_rejects_unpriced_owned_balance() {
    let mut e = connect(true, true);
    let req = http(
        &e.drain_actions().collect::<Vec<_>>(),
        "spotMetaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(SPOT_META, ETH_BAD));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let timer = retry(&actions, 2000.0);
    let req = http(&actions, "spotClearinghouseState");
    assert!(!e.store().loaded().balance);
    assert!(e.store().loaded().positions);
    assert!(e.store().loaded().orders);
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        &req,
        1000.0,
        r#"{"balances":[{"coin":"BTC","token":1,"total":"2","hold":"0"}]}"#,
    );
    let balance = e.store().balance().clone();
    let coins = e.store().coin_balances().unwrap().to_vec();
    let btc = e.store().market("BTC/USDC").unwrap().clone();
    e.handle(1500.0, Event::WsMessage { socket, text: r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"ETH","token":2,"total":"3","hold":"0"}]}}}"# });
    assert_eq!(*e.store().balance(), balance);
    assert_eq!(e.store().coin_balances().unwrap(), coins);
    assert!(notice(
        &e.drain_actions().collect::<Vec<_>>(),
        "owned spot balance has unavailable valuation"
    ));
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(
        &e.drain_actions().collect::<Vec<_>>(),
        "spotMetaAndAssetCtxs",
    );
    e.handle(2000.0, Event::WsMessage { socket, text: r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"ETH","token":2,"total":"3","hold":"0"}]}}}"# });
    assert_eq!(
        *e.store().balance(),
        balance,
        "in-flight retry must retain the same valuation protection"
    );
    reply(&mut e, &req, 2000.0, &tuple(SPOT_META, BTC_BAD));
    assert_eq!(e.store().market("BTC/USDC"), Some(&btc));
    assert_eq!(e.venue().assets["BTC/USDC"], 10004);
    assert_eq!(e.venue().assets["ETH/USDC"], 10009);
    assert_eq!(e.venue().spot_tokens[&2], "ETH/USDC");
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let timer = retry(&actions, 4000.0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    e.handle(2000.0, Event::WsMessage { socket, text: r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"ETH","token":2,"total":"3","hold":"0"}]}}}"# });
    assert_eq!(e.store().balance().total, 600.0);
    assert_eq!(e.store().coin_balances().unwrap()[0].amount, 3.0);
    e.handle(4000.0, Event::TimerFired { timer });
    let req = http(
        &e.drain_actions().collect::<Vec<_>>(),
        "spotMetaAndAssetCtxs",
    );
    reply(&mut e, &req, 4000.0, &tuple(SPOT_META, GOOD));
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    e.handle(4100.0, Event::WsMessage { socket, text: r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"UNPRICED","token":99,"total":"3","hold":"1"}]}}}"# });
    assert_eq!(e.store().coin_balances().unwrap()[0].base, "UNPRICED");
    assert_eq!(e.store().coin_balances().unwrap()[0].amount, 3.0);
    assert_eq!(e.store().coin_balances().unwrap()[0].notional, 0.0);
    assert_eq!(e.store().currency_balances().unwrap()[0].balance.used, 1.0);
}

#[test]
fn partial_catalogue_unknown_open_order_does_not_claim_a_loaded_empty_snapshot() {
    let mut e = connect(false, true);
    let req = http(&e.drain_actions().collect::<Vec<_>>(), "metaAndAssetCtxs");
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let req = http(&actions, "perpDexs");
    reply(&mut e, &req, 1000.0, "[null]");
    let actions = e.drain_actions().collect::<Vec<_>>();
    let req = http(&actions, "frontendOpenOrders");
    reply(
        &mut e,
        &req,
        1000.0,
        r#"[{"coin":"ETH","oid":99,"side":"B","limitPx":"200","origSz":"2","sz":"2"}]"#,
    );
    assert!(!e.store().loaded().orders);
    assert!(notice(
        &e.drain_actions().collect::<Vec<_>>(),
        "owned order has unavailable market metadata"
    ));
}
