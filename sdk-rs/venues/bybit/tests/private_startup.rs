//! Private bootstrap must complete or explain its failure, even with live public data.
mod support;
use safe_cex_engine::{ErrorKind, Event};
use safe_cex_venue_bybit::Variant;
use support::{Harness, Options};

const EMPTY: &str = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"result":{"list":[{"accountType":"UNIFIED","totalEquity":"0","totalWalletBalance":"0","totalAvailableBalance":"0","coin":[]}]}}"#;

fn starting() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::Demo,
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..Options::default()
    });
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.load_public();
    assert!(h.engine.store().loaded().tickers);
    assert!(h.socket("/v5/private").is_some());
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.starts_with("https://api-demo.bybit.com/"))
    );
    h
}

fn reply_reads(h: &mut Harness, wallet: &str) {
    while !h.requests.is_empty() {
        let r = h.requests.remove(0);
        h.reply(
            r.id,
            200,
            if r.url.contains("wallet-balance") {
                wallet
            } else {
                EMPTY
            },
        );
    }
}

#[test]
fn lost_demo_reads_fail_startup_at_the_bound_with_the_transport_cause() {
    let mut h = starting();
    for r in std::mem::take(&mut h.requests) {
        h.event(Event::HttpFailed {
            request: r.id,
            error: "TradeTunnel connection lost, reconnecting",
        });
    }
    // Account reads start at the fixture's 1000 ms; public subscription pacing adds 100 ms.
    h.advance(61_000.0 - h.now - 1.0);
    assert!(h.startup_failures.is_empty());
    h.advance(1.0);
    assert_eq!(h.startup_failures.len(), 1);
    let failure = &h.startup_failures[0];
    assert_eq!(failure.kind, Some(ErrorKind::Timeout));
    assert!(failure.message.contains("balance, orders, positions"));
    assert!(failure.message.contains("TradeTunnel connection lost"));
    assert!(!h.engine.store().loaded().balance);
    assert!(!h.deactivated, "network loss does not condemn the key");
    h.advance(120_000.0);
    assert_eq!(h.startup_failures.len(), 1);
}

#[test]
fn empty_wallet_is_not_an_empty_account_or_an_infinite_startup() {
    let mut h = starting();
    reply_reads(&mut h, EMPTY);
    assert!(!h.engine.store().loaded().balance);
    assert!(h.engine.store().loaded().orders);
    assert!(h.engine.store().loaded().positions);
    h.advance(60_000.0);
    assert_eq!(h.startup_failures.len(), 1);
    assert!(
        h.startup_failures[0]
            .message
            .contains("Invalid or empty balance data")
    );
    assert!(!h.engine.store().loaded().balance);
}

#[test]
fn definite_key_refusal_on_any_required_snapshot_fails_visibly() {
    for path in ["wallet-balance", "order/realtime", "position/list"] {
        let mut h = starting();
        h.respond(
            path,
            200,
            r#"{"retCode":10003,"retMsg":"API key is invalid"}"#,
        );
        assert_eq!(h.startup_failures.len(), 1, "{path}");
        assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(h.startup_failures[0].venue_code.as_deref(), Some("10003"));
    }
}

#[test]
fn read_permission_refusal_is_visible_without_deactivating_the_trading_key() {
    let mut h = starting();
    h.respond(
        "wallet-balance",
        200,
        r#"{"retCode":10005,"retMsg":"Permission denied"}"#,
    );
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::AuthPermission));
    assert!(!h.deactivated);
}

#[test]
fn a_transient_loss_can_recover_before_the_bound() {
    let mut h = starting();
    let r = h.take("wallet-balance");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "relay disconnected",
    });
    reply_reads(&mut h, WALLET);
    h.advance(5_000.0);
    reply_reads(&mut h, WALLET);
    assert!(h.engine.store().loaded().balance);
    h.advance(60_000.0);
    assert!(h.startup_failures.is_empty());
}

#[test]
fn valid_zero_wallet_and_empty_lists_finish_startup() {
    let mut h = starting();
    reply_reads(&mut h, WALLET);
    let loaded = h.engine.store().loaded();
    assert!(loaded.balance && loaded.orders && loaded.positions);
    assert_eq!(h.engine.store().balance().total, 0.0);
    h.advance(60_000.0);
    assert!(h.startup_failures.is_empty());
}
