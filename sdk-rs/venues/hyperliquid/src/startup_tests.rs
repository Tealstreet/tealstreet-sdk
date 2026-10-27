//! Scripted info reads only: no transport, credentials or real trading account.
use super::*;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Engine, Event, HttpRequest};

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
/// The retry timer armed at `sent` plus the engine's jittered `[0.75, 1.0] * nominal`.
fn retry_timer(actions: &[Action], sent: f64, nominal: f64) -> Option<TimerId> {
    actions.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms }
            if (0.75 * nominal..=nominal).contains(&(at_ms - sent)) =>
        {
            Some(*timer)
        }
        _ => None,
    })
}
fn retry(actions: &[Action], sent: f64, nominal: f64) -> TimerId {
    retry_timer(actions, sent, nominal).expect("public context backoff")
}
fn connect(spot: bool, private: bool) -> Engine<Hyperliquid> {
    let policy = serde_json::from_str(r#"{"primary":{"source":"scalarOnly","signingAlgorithm":"secp256k1Digest","secretEncoding":"hex"}}"#).unwrap();
    let (provider, identities, _) = safe_cex_crypto::signer::LocalSigner::from_json_with_facts(
        if private { r#"{"primary":{"secret":"0123456789012345678901234567890123456789012345678901234567890123"}}"# } else { "{}" }.into(), policy, false,
    ).unwrap();
    let options = Options {
        testnet: true,
        public_data_only: !private,
        // Well-known scripted test key; no account transport exists.
        key: private.then(|| "0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        credentials: identities.primary.clone(),
        application_id: private.then(|| "0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Default::default()
    };
    let mut e = safe_cex_engine::test_support::seeded_engine(if spot {
        Hyperliquid::new_spot(options)
    } else {
        Hyperliquid::new(options)
    });
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.initialize_address_nonce(999.).unwrap();
    assert!(e.address_nonce_setup_ready());
    e.start(1000.0);
    e
}
fn notice(actions: &[Action], text: &str) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains(text)))
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
        let req = http(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), kind);
        reply(
            &mut e,
            &req,
            1000.0,
            &tuple(meta, r#"[{"midPx":"bad"},{"midPx":"bad"}]"#),
        );
        let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
        let timer = retry(&actions, 1000.0, 1000.0);
        assert!(!e.store().loaded().markets);
        assert!(!e.store().loaded().tickers);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
        e.handle(2000.0, Event::TimerFired { timer });
        let req = http(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), kind);
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
        let req = http(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), kind);
        reply(&mut e, &req, 1000.0, &tuple(meta, ETH_BAD));
        let mut actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
        let timer = retry(&actions, 1000.0, 1000.0);
        if !spot {
            let req = http(&actions, "perpDexs");
            reply(&mut e, &req, 1000.0, "[null]");
            actions = prepared_now(&mut e).into_iter().collect();
        }
        let socket = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        e.handle(1000.0, Event::WsOpened { socket });
        prepared_now(&mut e).into_iter().for_each(drop);
        e.handle(2000.0, Event::TimerFired { timer });
        let req = http(&prepared_now(&mut e).into_iter().collect::<Vec<_>>(), kind);
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
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let first = retry(&actions, 1000.0, 1000.0);
    let dexs = http(&actions, "perpDexs");
    let btc = e.store().market("BTC-PERP").unwrap().clone();
    assert_eq!(e.venue().assets["BTC-PERP"], 0);
    assert!(!e.venue().assets.contains_key("ETH-PERP"));
    reply(&mut e, &dexs, 1000.0, "[null]");
    assert_eq!(
        prepared_now(&mut e)
            .into_iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        1
    );
    e.handle(2000.0, Event::TimerFired { timer: first });
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, BTC_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let second = retry(&actions, 2000.0, 2000.0);
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
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 4000.0, &tuple(UNIVERSE, GOOD));
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    assert!(prepared_now(&mut e).into_iter().all(|a| !matches!(
        a,
        Action::Http(_) | Action::SetTimer { .. } | Action::WsConnect { .. }
    )));
}
#[test]
fn partial_hip3_context_keeps_dex_offset_and_does_not_advance_discovery_twice() {
    let mut e = connect(false, false);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, r#"[{"universe":[]},[]]"#);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "perpDexs",
    );
    reply(&mut e, &req, 1000.0, r#"[null,{"name":"xyz"}]"#);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let timer = retry(&actions, 1000.0, 1000.0);
    assert_eq!(e.venue().assets["XYZ|BTC-PERP"], 110000);
    assert_eq!(e.venue().coins["xyz:btc"], "XYZ|BTC-PERP");
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, GOOD));
    assert_eq!(e.venue().assets["XYZ|ETH-PERP"], 110001);
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
    assert!(
        !prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}
#[test]
fn unmapped_owned_position_rejects_authoritative_snapshot_and_recovers_without_stamping_bad_time() {
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let timer = retry(&actions, 1000.0, 1000.0);
    let req = http(&actions, "perpDexs");
    reply(&mut e, &req, 1000.0, "[null]");
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
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
    let state = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "clearinghouseState",
    );
    let unknown = r#"{"assetPositions":[{"position":{"coin":"ETH","szi":"3","entryPx":"200","positionValue":"600","unrealizedPnl":"0","marginUsed":"60","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"999999","totalMarginUsed":"60"},"time":999999}"#;
    reply(&mut e, &state, 1500.0, unknown);
    assert_eq!(e.store().positions(), before);
    assert_eq!(*e.store().balance(), balance);
    assert!(notice(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "owned position has unavailable market metadata"
    ));
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, GOOD));
    let state = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "clearinghouseState",
    );
    reply(&mut e, &state, 2000.0, &unknown.replace("999999", "2000"));
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().position("ETH-PERP").unwrap().contracts, 3.0);
    assert_eq!(e.venue().assets["ETH-PERP"], 1);
}
#[test]
fn partial_spot_context_preserves_known_rows_retries_and_rejects_unpriced_owned_balance() {
    let mut e = connect(true, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "spotMetaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(SPOT_META, ETH_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let timer = retry(&actions, 1000.0, 1000.0);
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
    e.handle(1000.0, Event::WsOpened { socket });
    e.handle(1500.0, Event::WsMessage { socket, text: r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"ETH","token":2,"total":"3","hold":"0"}]}}}"# });
    assert_eq!(*e.store().balance(), balance);
    assert_eq!(e.store().coin_balances().unwrap(), coins);
    assert!(notice(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "owned spot balance has unavailable valuation"
    ));
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
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
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let timer = retry(&actions, 2000.0, 2000.0);
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
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
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
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let req = http(&actions, "perpDexs");
    reply(&mut e, &req, 1000.0, "[null]");
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let req = http(&actions, "frontendOpenOrders");
    reply(
        &mut e,
        &req,
        1000.0,
        r#"[{"coin":"ETH","oid":99,"side":"B","limitPx":"200","origSz":"2","sz":"2"}]"#,
    );
    assert!(!e.store().loaded().orders);
    assert!(notice(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "owned order has unavailable market metadata"
    ));
}

#[test]
fn delisted_and_unquoted_core_perps_complete_discovery_without_retry() {
    let universe = r#"{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":40},{"name":"MATIC","szDecimals":1,"maxLeverage":20,"isDelisted":true,"onlyIsolated":true},{"name":"ETH","szDecimals":4,"maxLeverage":25}]}"#;
    let contexts = r#"[{"funding":"0.0000125","openInterest":"9812.5","prevDayPx":"99","dayNtlVlm":"1000","premium":"0.0","oraclePx":"100","markPx":"100","midPx":"100","impactPxs":["99.9","100.1"],"dayBaseVlm":"10"},{"funding":"0.0","openInterest":"0.0","prevDayPx":"0.37","dayNtlVlm":"0.0","premium":null,"oraclePx":"0.37","markPx":"0.37","midPx":null,"impactPxs":null,"dayBaseVlm":"0.0"},{"funding":"0.0","openInterest":"0.0","prevDayPx":"200","dayNtlVlm":"0.0","premium":null,"oraclePx":"200","markPx":"200","midPx":null,"impactPxs":null,"dayBaseVlm":"0.0"}]"#;
    let mut e = connect(false, false);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(universe, contexts));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    assert!(
        retry_timer(&actions, 1000.0, 1000.0).is_none(),
        "well-formed unquoted rows are not a discovery failure"
    );
    assert!(!e.venue().core_discovery_incomplete);
    assert!(e.store().loaded().markets);
    assert!(e.store().market("BTC-PERP").is_some());
    // Excluded by design, as TS hides them: neither market nor ticker.
    assert!(e.store().market("MATIC-PERP").is_none());
    assert!(e.store().market("ETH-PERP").is_none());
    assert!(e.store().ticker("ETH-PERP").is_none());
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().tickers().len(), 1);
}

#[test]
fn unquoted_spot_contexts_complete_discovery_and_publish_owned_native_balance() {
    let meta = r#"{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"BTC","index":1,"szDecimals":5},{"name":"JEFF","index":2,"szDecimals":0}],"universe":[{"name":"@4","tokens":[1,0],"index":4},{"name":"@9","tokens":[2,0],"index":9}]}"#;
    let contexts = r#"[{"prevDayPx":"100","dayNtlVlm":"10","markPx":"100","midPx":"100","circulatingSupply":"21000000","coin":"@4","totalSupply":"21000000","dayBaseVlm":"0.1"},{"prevDayPx":"0.0009","dayNtlVlm":"0.0","markPx":"0.0009","midPx":null,"circulatingSupply":"1000000000","coin":"@9","totalSupply":"1000000000","dayBaseVlm":"0.0"}]"#;
    let mut e = connect(true, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "spotMetaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(meta, contexts));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    assert!(retry_timer(&actions, 1000.0, 1000.0).is_none());
    assert!(!e.venue().core_discovery_incomplete);
    let req = http(&actions, "spotClearinghouseState");
    reply(
        &mut e,
        &req,
        1000.0,
        r#"{"balances":[{"coin":"BTC","token":1,"total":"2","hold":"0"},{"coin":"JEFF","token":2,"total":"500","hold":"0"}]}"#,
    );
    assert!(e.store().loaded().balance);
    // The unquoted pair is hidden like TS; its owned token keeps zero notional.
    assert!(e.store().market("JEFF/USDC").is_none());
    assert!(e.store().ticker("JEFF/USDC").is_none());
    assert_eq!(e.store().balance().total, 200.0);
    let coins = e.store().coin_balances().unwrap();
    assert_eq!(coins[1].base, "JEFF");
    assert_eq!(coins[1].amount, 500.0);
    assert_eq!(coins[1].notional, 0.0);
}

const UNQUOTED_ETH: &str =
    r#"[{"midPx":"100","markPx":"100"},{"midPx":null,"markPx":"200","oraclePx":"199"}]"#;

#[test]
fn owned_position_on_unquoted_market_is_hidden_without_freezing_the_account() {
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, UNQUOTED_ETH));
    assert!(!e.venue().core_discovery_incomplete);
    assert!(e.store().market("ETH-PERP").is_none());
    assert!(e.store().ticker("ETH-PERP").is_none());
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "perpDexs",
    );
    reply(&mut e, &req, 1000.0, "[null]");
    let state = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "clearinghouseState",
    );
    reply(
        &mut e,
        &state,
        1000.0,
        r#"{"assetPositions":[{"position":{"coin":"ETH","szi":"3","entryPx":"200","positionValue":"600","unrealizedPnl":"0","marginUsed":"60","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"60"},"time":1000}"#,
    );
    assert!(!notice(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "owned position has unavailable market metadata"
    ));
    // Applied, not frozen; the excluded row stays hidden as in TS.
    assert!(e.store().loaded().positions);
    assert!(e.store().position("ETH-PERP").is_none());
}

#[test]
fn owned_order_on_unquoted_market_is_hidden_without_freezing_orders() {
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, UNQUOTED_ETH));
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "perpDexs",
    );
    reply(&mut e, &req, 1000.0, "[null]");
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "frontendOpenOrders",
    );
    reply(
        &mut e,
        &req,
        1000.0,
        r#"[{"coin":"ETH","oid":99,"side":"B","limitPx":"200","origSz":"2","sz":"2"},{"coin":"BTC","oid":100,"side":"B","limitPx":"90","origSz":"1","sz":"1"}]"#,
    );
    assert!(!notice(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "owned order has unavailable market metadata"
    ));
    assert!(e.store().loaded().orders);
    let ids = e
        .store()
        .orders()
        .iter()
        .map(|o| o.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["100"]);
}

#[test]
fn owned_position_on_delisted_hip3_market_is_hidden_without_freezing_the_dex() {
    let universe = r#"{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"OLD","szDecimals":1,"maxLeverage":10,"isDelisted":true}]}"#;
    let contexts = r#"[{"midPx":"100","markPx":"100"},{"midPx":null,"markPx":"5","oraclePx":"5"}]"#;
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, r#"[{"universe":[]},[]]"#);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "perpDexs",
    );
    reply(&mut e, &req, 1000.0, r#"[null,{"name":"xyz"}]"#);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(universe, contexts));
    assert!(e.store().market("XYZ|BTC-PERP").is_some());
    assert!(e.store().market("XYZ|OLD-PERP").is_none());
    assert!(e.store().ticker("XYZ|OLD-PERP").is_none());
    let state = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r)
                if r.body
                    .as_deref()
                    .is_some_and(|b| b.contains("clearinghouseState") && b.contains("\"xyz\"")) =>
            {
                Some(r)
            }
            _ => None,
        })
        .expect("xyz clearinghouseState");
    reply(
        &mut e,
        &state,
        1000.0,
        r#"{"assetPositions":[{"position":{"coin":"xyz:OLD","szi":"4","entryPx":"5","positionValue":"20","unrealizedPnl":"0","marginUsed":"2","leverage":{"type":"isolated","value":10}}},{"position":{"coin":"xyz:BTC","szi":"1","entryPx":"100","positionValue":"100","unrealizedPnl":"0","marginUsed":"10","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"100","totalMarginUsed":"12"},"time":1000}"#,
    );
    assert!(!notice(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "owned position has unavailable market metadata"
    ));
    // The dex snapshot applies: its listed position lands, the delisted one is hidden.
    assert_eq!(e.store().position("XYZ|BTC-PERP").unwrap().contracts, 1.0);
    assert!(e.store().position("XYZ|OLD-PERP").is_none());
}

fn order_ids(e: &Engine<Hyperliquid>) -> Vec<&str> {
    e.store().orders().iter().map(|o| o.id.as_str()).collect()
}

#[test]
fn perp_order_snapshot_skips_spot_rows_and_commits_perp_orders() {
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, GOOD));
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "perpDexs",
    );
    reply(&mut e, &req, 1000.0, "[null]");
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "frontendOpenOrders",
    );
    reply(
        &mut e,
        &req,
        1000.0,
        r#"[{"coin":"@107","oid":99,"side":"B","limitPx":"1","origSz":"2","sz":"2"},{"coin":"BTC","oid":100,"side":"B","limitPx":"90","origSz":"1","sz":"1"}]"#,
    );
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    assert!(!notice(&actions, "unavailable market metadata"));
    assert!(!notice(&actions, "skipped"));
    assert!(e.store().loaded().orders);
    assert_eq!(order_ids(&e), ["100"]);
}

#[test]
fn spot_order_snapshot_skips_perp_rows_and_commits_spot_orders() {
    let mut e = connect(true, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "spotMetaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(SPOT_META, GOOD));
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "frontendOpenOrders",
    );
    reply(
        &mut e,
        &req,
        1000.0,
        r#"[{"coin":"BTC","oid":99,"side":"B","limitPx":"90","origSz":"1","sz":"1"},{"coin":"@4","oid":100,"side":"B","limitPx":"90","origSz":"1","sz":"1"}]"#,
    );
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    assert!(!notice(&actions, "unavailable market metadata"));
    assert!(!notice(&actions, "skipped"));
    assert!(e.store().loaded().orders);
    assert_eq!(order_ids(&e), ["100"]);
}

#[test]
fn settled_discovery_skips_unknown_market_rows_and_commits_the_rest() {
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, GOOD));
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "perpDexs",
    );
    reply(&mut e, &req, 1000.0, "[null]");
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let state = http(&actions, "clearinghouseState");
    let orders = http(&actions, "frontendOpenOrders");
    reply(
        &mut e,
        &state,
        1000.0,
        r#"{"assetPositions":[{"position":{"coin":"SOL","szi":"3","entryPx":"20","positionValue":"60","unrealizedPnl":"0","marginUsed":"6","leverage":{"type":"cross","value":10}}},{"position":{"coin":"BTC","szi":"1","entryPx":"100","positionValue":"100","unrealizedPnl":"0","marginUsed":"10","leverage":{"type":"cross","value":10}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"16"},"time":1000}"#,
    );
    reply(
        &mut e,
        &orders,
        1000.0,
        r#"[{"coin":"SOL","oid":99,"side":"B","limitPx":"20","origSz":"2","sz":"2"},{"coin":"BTC","oid":100,"side":"B","limitPx":"90","origSz":"1","sz":"1"}]"#,
    );
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    assert!(notice(
        &actions,
        "Hyperliquid skipped position sol on an unknown market"
    ));
    assert!(notice(
        &actions,
        "Hyperliquid skipped order 99 on an unknown market"
    ));
    assert!(!notice(&actions, "unavailable market metadata"));
    assert!(e.store().loaded().positions);
    assert!(e.store().loaded().orders);
    assert_eq!(e.store().position("BTC-PERP").unwrap().contracts, 1.0);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(order_ids(&e), ["100"]);
}

#[test]
fn market_found_by_a_discovery_retry_gets_a_seeded_setting() {
    let mut e = connect(false, true);
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 1000.0, &tuple(UNIVERSE, ETH_BAD));
    let actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
    let timer = retry(&actions, 1000.0, 1000.0);
    let req = http(&actions, "perpDexs");
    reply(&mut e, &req, 1000.0, "[null]");
    assert!(e.store().setting("BTC-PERP").is_some());
    assert!(e.store().setting("ETH-PERP").is_none());
    e.handle(2000.0, Event::TimerFired { timer });
    let req = http(
        &prepared_now(&mut e).into_iter().collect::<Vec<_>>(),
        "metaAndAssetCtxs",
    );
    reply(&mut e, &req, 2000.0, &tuple(UNIVERSE, GOOD));
    let setting = e.store().setting("ETH-PERP").expect("seeded setting");
    assert_eq!(setting.leverage, 20.0);
    assert_eq!(setting.margin_type, Some(MarginType::Cross));
}
