mod spot_support;
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, SocketChannel, SocketId, model::Balance,
};
use safe_cex_venue_kraken::KrakenSpot;
use spot_support::*;

struct Wallet {
    engine: Engine<KrakenSpot>,
    public: SocketId,
    ticker: HttpRequest,
    balance: HttpRequest,
    warnings: Vec<String>,
    metadata_retry: Option<(safe_cex_engine::TimerId, f64)>,
    poll: (safe_cex_engine::TimerId, f64),
}
impl Wallet {
    fn pending() -> Self {
        Self::with_meta(META)
    }
    fn with_meta(meta: &str) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(options()));
        engine.start(NOW);
        let metadata = requests(&mut engine).pop().unwrap();
        reply(&mut engine, &metadata, meta);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
        let public = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .unwrap();
        let take = |suffix: &str| {
            actions
                .iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.url.ends_with(suffix) => Some(r.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let metadata_retry = actions.iter().find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - NOW, 1000.) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        });
        let poll = actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 10_000. => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .unwrap();
        let ticker = take("Ticker");
        let balance = take("Balance");
        engine.handle(NOW, Event::WsOpened { socket: public });
        engine.drain_actions().for_each(drop);
        Self {
            engine,
            public,
            ticker,
            balance,
            warnings: vec![],
            metadata_retry,
            poll,
        }
    }
    fn recover_metadata(&mut self, body: &str) -> Vec<Action> {
        let (timer, at_ms) = self.metadata_retry.take().expect("metadata retry");
        self.engine.handle(at_ms, Event::TimerFired { timer });
        let metadata = safe_cex_engine::test_support::prepared_actions(&mut self.engine, at_ms)
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .find(|request| request.url.ends_with("AssetPairs"))
            .unwrap();
        reply_at(&mut self.engine, at_ms, &metadata, body);
        safe_cex_engine::test_support::prepared_actions(&mut self.engine, at_ms)
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            if let Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Warn,
                message,
                ..
            })) = action
            {
                self.warnings.push(message);
            }
        }
    }
    fn native(&mut self, body: &str) {
        reply(&mut self.engine, &self.balance, body);
        self.collect();
    }
    fn tickers(&mut self, body: &str) {
        reply(&mut self.engine, &self.ticker, body);
        self.collect();
    }
    fn live(&mut self, rows: &str) {
        message(
            &mut self.engine,
            self.public,
            &format!(r#"{{"channel":"ticker","type":"update","data":{rows}}}"#),
        );
        self.collect();
    }
}

#[test]
fn early_native_wallet_waits_for_real_rates_and_live_quotes_complete_it_before_rest() {
    let mut wallet = Wallet::pending();
    wallet.native(BALANCE);
    assert!(!wallet.engine.store().loaded().balance);
    let native = wallet.engine.store().currency_balances().unwrap();
    assert_eq!(native.len(), 4);
    assert_eq!(
        native
            .iter()
            .find(|b| b.currency == "BTC")
            .unwrap()
            .balance
            .total,
        2.
    );
    assert!(
        wallet
            .warnings
            .iter()
            .any(|w| w.contains("awaits a valid quote"))
    );
    wallet.live(r#"[{"symbol":"BTC/USD","bid":99,"ask":101} ]"#);
    assert!(
        !wallet.engine.store().loaded().balance,
        "BBO does not prove last-price conversion"
    );
    wallet.live(r#"[{"symbol":"BTC/USD","last":120}]"#);
    assert!(
        !wallet.engine.store().loaded().balance,
        "ETH remains unpriced"
    );
    for bad in ["0", "-1", "\"NaN\""] {
        wallet.live(&format!(r#"[{{"symbol":"ETH/USD","last":{bad}}}]"#));
        assert!(!wallet.engine.store().loaded().balance);
    }
    wallet.live(r#"[{"symbol":"ETH/USD","last":55}]"#);
    assert!(wallet.engine.store().loaded().balance);
    assert!(!wallet.engine.store().loaded().tickers);
    assert_eq!(wallet.engine.store().balance().total, 1410.);
    wallet.tickers(TICKERS);
    assert_eq!(wallet.engine.store().balance().total, 1410.);
    assert_eq!(wallet.engine.store().ticker("XBT/USD").unwrap().last, 120.);
}

#[test]
fn omitted_rest_conversion_peer_counts_as_zero_after_the_seed_and_live_recovery_revalues_immediately()
 {
    let mut wallet = Wallet::pending();
    let previous = Balance {
        total: 777.,
        free: 700.,
        used: 77.,
        upnl: 0.,
    };
    wallet.engine.store_mut().set_balance(previous.clone());
    wallet.native(BALANCE);
    assert_eq!(*wallet.engine.store().balance(), previous);
    let mut body: serde_json::Value = serde_json::from_str(TICKERS).unwrap();
    body["result"].as_object_mut().unwrap().remove("XETHZUSD");
    wallet.tickers(&body.to_string());
    assert!(wallet.engine.store().loaded().tickers);
    // Once the seed has completed, the unquoted ETH counts as zero instead of freezing USD.
    assert_eq!(wallet.engine.store().balance().total, 1205.);
    assert!(
        wallet
            .warnings
            .iter()
            .any(|w| w.contains("without a quote at zero"))
    );
    wallet.live(r#"[{"symbol":"ETH/USD","last":50}]"#);
    assert_eq!(wallet.engine.store().balance().total, 1355.);
    wallet.live(r#"[{"symbol":"BTC/USD","last":200}]"#);
    assert_eq!(
        wallet.engine.store().balance().total,
        1355.,
        "complete valuation keeps existing private poll cadence"
    );
}

#[test]
fn assets_without_supported_conversion_keep_native_quantity_warn_and_do_not_block_readiness() {
    let mut wallet = Wallet::pending();
    wallet.native(r#"{"error":[],"result":{"USDT":"5","ATOM":"4"}}"#);
    assert!(wallet.engine.store().loaded().balance);
    assert_eq!(wallet.engine.store().balance().total, 5.);
    assert_eq!(
        wallet
            .engine
            .store()
            .currency_balances()
            .unwrap()
            .iter()
            .find(|b| b.currency == "ATOM")
            .unwrap()
            .balance
            .total,
        4.
    );
    assert!(
        wallet
            .warnings
            .iter()
            .any(|w| w.contains("unpriced native currency"))
    );
    assert!(
        !wallet
            .warnings
            .iter()
            .any(|w| w.contains("awaits a valid quote"))
    );
}

#[test]
fn valid_rest_conversion_values_the_early_native_wallet_without_waiting_for_private_poll() {
    let mut wallet = Wallet::pending();
    wallet.native(BALANCE);
    assert!(!wallet.engine.store().loaded().balance);
    wallet.tickers(TICKERS);
    assert!(wallet.engine.store().loaded().balance);
    assert_eq!(wallet.engine.store().balance().total, 1355.);
}

#[test]
fn shared_feed_quote_completes_pending_native_wallet_without_private_poll() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(options()));
    engine
        .set_public_feed_policy(safe_cex_engine::PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(NOW);
    let metadata = requests(&mut engine).pop().unwrap();
    reply(&mut engine, &metadata, META);
    let balance = requests(&mut engine)
        .into_iter()
        .find(|r| r.url.ends_with("Balance"))
        .unwrap();
    reply(
        &mut engine,
        &balance,
        r#"{"error":[],"result":{"XXBT":"2"}}"#,
    );
    assert!(!engine.store().loaded().balance);
    engine
        .apply_public_feed(
            NOW,
            serde_json::from_str(r#"{"tickers":[{"symbol":"XBT/USD","bid":99,"ask":101}]}"#)
                .unwrap(),
        )
        .unwrap();
    // The first shared frame completes the seed; BBO values BTC at its midpoint.
    assert!(engine.store().loaded().balance);
    assert_eq!(engine.store().balance().total, 200.);
    engine
        .apply_public_feed(
            NOW,
            serde_json::from_str(r#"{"tickers":[{"symbol":"XBT/USD","last":110}]}"#).unwrap(),
        )
        .unwrap();
    assert!(engine.store().loaded().balance);
    assert_eq!(engine.store().balance().total, 220.);
}

#[test]
fn a_catalogued_usdc_quote_is_a_supported_conversion_not_an_unpriced_exclusion() {
    let meta = r#"{"error":[],"result":{"XDGUSDC":{"altname":"XDGUSDC","wsname":"XDG/USDC","pair_decimals":5,"lot_decimals":8,"ordermin":"13","status":"online"}}}"#;
    let mut wallet = Wallet::with_meta(meta);
    wallet.native(r#"{"error":[],"result":{"XXDG":"4"}}"#);
    assert!(!wallet.engine.store().loaded().balance);
    assert!(
        !wallet
            .warnings
            .iter()
            .any(|w| w.contains("without a supported USD market"))
    );
    wallet.live(r#"[{"symbol":"DOGE/USDC","last":0.15}]"#);
    assert!(wallet.engine.store().loaded().balance);
    assert!((wallet.engine.store().balance().total - 0.6).abs() < 1e-12);
}

#[test]
fn rejected_pair_holds_its_funded_currency_until_the_reread_repairs_it() {
    let mut partial: serde_json::Value = serde_json::from_str(META).unwrap();
    partial["result"]["XXBTZUSD"]["pair_decimals"] = serde_json::json!("bad");
    for prior in [
        None,
        Some(Balance {
            total: 777.,
            free: 700.,
            used: 77.,
            upnl: 0.,
        }),
    ] {
        let mut wallet = Wallet::with_meta(&partial.to_string());
        if let Some(prior) = &prior {
            wallet.engine.store_mut().set_balance(prior.clone());
        }
        wallet.tickers(TICKERS);
        wallet.native(r#"{"error":[],"result":{"XXBT":"2"}}"#);
        // XXBTZUSD is pending re-read, so BTC is never published at zero.
        assert_eq!(wallet.engine.store().loaded().balance, prior.is_some());
        assert_eq!(
            wallet.engine.store().balance().total,
            prior.as_ref().map_or(0., |p| p.total)
        );
        assert!(
            !wallet
                .warnings
                .iter()
                .any(|w| w.contains("without a supported USD market"))
        );
        let recovered = wallet.recover_metadata(META);
        assert!(
            recovered.iter().any(|action| match action {
                Action::WsSend { text, .. } => text.contains("ticker") && text.contains("BTC/USD"),
                _ => false,
            }),
            "newly recovered ticker subscribes on existing public socket"
        );
        assert_eq!(wallet.engine.store().loaded().balance, prior.is_some());
        wallet.live(r#"[{"symbol":"BTC/USD","last":110}]"#);
        assert!(wallet.engine.store().loaded().balance);
        assert_eq!(wallet.engine.store().balance().total, 220.);
        assert_eq!(
            wallet.engine.store().currency_balances().unwrap()[0]
                .balance
                .total,
            2.
        );
    }
}

#[test]
fn funded_currency_without_any_usd_route_never_waits_on_a_partial_catalogue() {
    let mut partial: serde_json::Value = serde_json::from_str(META).unwrap();
    partial["result"]["XXBTZUSD"]["pair_decimals"] = serde_json::json!("bad");
    let mut wallet = Wallet::with_meta(&partial.to_string());
    wallet.native(r#"{"error":[],"result":{"ATOM":"4","ZCAD":"7","KFEE":"100","ZUSD":"10"}}"#);
    assert!(wallet.engine.store().loaded().balance);
    assert_eq!(wallet.engine.store().balance().total, 10.);
    assert!(
        wallet
            .warnings
            .iter()
            .any(|w| w.contains("without a supported USD market"))
    );
    wallet.recover_metadata(META);
    assert_eq!(wallet.engine.store().balance().total, 10.);
    assert_eq!(
        wallet
            .engine
            .store()
            .currency_balances()
            .unwrap()
            .iter()
            .find(|b| b.currency == "ATOM")
            .unwrap()
            .balance
            .total,
        4.
    );
}

#[test]
fn seeded_route_without_a_last_is_valued_at_its_bid_ask_midpoint() {
    let mut wallet = Wallet::pending();
    wallet.tickers(&TICKERS.replace(r#""c":["50","1"]"#, r#""c":["0","1"]"#));
    wallet.native(BALANCE);
    assert!(wallet.engine.store().loaded().balance);
    assert_eq!(wallet.engine.store().balance().total, 1355.);
    wallet.live(r#"[{"symbol":"ETH/USD","last":60}]"#);
    assert_eq!(wallet.engine.store().balance().total, 1385.);
}

#[test]
fn persistent_rejected_pair_rereads_only_the_catalogue() {
    let mut partial: serde_json::Value = serde_json::from_str(META).unwrap();
    partial["result"]["XXBTZUSD"]["pair_decimals"] = serde_json::json!("bad");
    let mut wallet = Wallet::with_meta(&partial.to_string());
    wallet.tickers(TICKERS);
    assert!(wallet.engine.store().loaded().tickers);
    let first_retry = wallet.metadata_retry.expect("metadata retry").1;
    let reread = wallet.recover_metadata(&partial.to_string());
    assert!(!reread.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!reread.iter().any(|a| matches!(a, Action::WsConnect { .. })));
    assert!(
        reread.iter().any(
            |a| matches!(a, Action::SetTimer { at_ms, .. } if jittered(*at_ms - first_retry, 2000.))
        ),
        "the catalogue retry stays on backoff"
    );
}

#[test]
fn malformed_native_pair_holds_its_funded_currency_until_recovered() {
    for wsname in ["/USD", " /USD", "BTC/", "BTC/ USD", "BTC/USD ", "BTC//USD"] {
        let mut partial: serde_json::Value = serde_json::from_str(META).unwrap();
        partial["result"]["XXBTZUSD"]["wsname"] = serde_json::json!(wsname);
        let mut wallet = Wallet::with_meta(&partial.to_string());
        wallet.native(r#"{"error":[],"result":{"XXBT":"2"}}"#);
        assert!(
            !wallet.engine.store().loaded().balance,
            "invalid native pair {wsname:?}"
        );
        assert_eq!(
            wallet.engine.store().markets().len(),
            1,
            "valid ETH peer remains usable"
        );
        assert!(wallet.metadata_retry.is_some());
        wallet.recover_metadata(META);
        wallet.live(r#"[{"symbol":"BTC/USD","last":110}]"#);
        assert_eq!(wallet.engine.store().balance().total, 220.);
    }
}

#[test]
fn unquoted_wallet_publishes_after_the_seed_and_warns_once_across_balance_polls() {
    let mut wallet = Wallet::pending();
    wallet.tickers(
        &TICKERS
            .replace(r#""c":["50","1"]"#, r#""c":["0","1"]"#)
            .replace(r#""a":["51","1","1"]"#, r#""a":["0","1","1"]"#),
    );
    wallet.native(BALANCE);
    let (timer, at_ms) = wallet.poll;
    wallet.engine.handle(at_ms, Event::TimerFired { timer });
    let balance = requests(&mut wallet.engine)
        .into_iter()
        .find(|request| request.url.ends_with("Balance"))
        .unwrap();
    reply_at(&mut wallet.engine, at_ms, &balance, BALANCE);
    wallet.collect();
    assert!(wallet.engine.store().loaded().balance);
    assert_eq!(wallet.engine.store().balance().total, 1205.);
    let warnings = wallet
        .warnings
        .iter()
        .filter(|w| w.contains("without a quote at zero"))
        .count();
    assert_eq!(warnings, 1);
    assert!(
        !wallet
            .warnings
            .iter()
            .any(|w| w.contains("awaits a valid quote"))
    );
}
