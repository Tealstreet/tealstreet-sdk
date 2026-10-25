//! Slice 1d-i: the startup fee read, account info and the wallet are signed
//! Background Reads on managed windows. Native join, staleness and credential
//! rejection behaviour stays pinned by account.rs and startup_auth.rs.
mod support;
use support::{Harness, INSTRUMENTS, Options, PRELAUNCH, TICKERS};

const WINDOW: Option<u32> = Some(20_000);

fn private() -> Harness {
    Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    })
}

/// Startup through the catalogue publish, with the account info answered.
fn published(h: &mut Harness) {
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, TICKERS);
}

/// A1: every 1d-i account read is signed on a managed read window.
#[test]
fn account_reads_go_out_signed_on_managed_windows() {
    let mut h = private();
    for path in ["/v5/account/info", "/v5/account/fee-rate"] {
        let request = h.requests.iter().find(|r| r.url.contains(path)).unwrap();
        assert_eq!(request.timeout_ms, WINDOW, "{path} is managed");
        assert!(Harness::header(request, "X-BAPI-SIGN").is_some(), "{path}");
    }
    published(&mut h);
    let wallet = h.take("/v5/account/wallet-balance");
    assert_eq!(wallet.timeout_ms, WINDOW, "the wallet is managed");
    assert!(Harness::header(&wallet, "X-BAPI-SIGN").is_some());
}

/// A4: only a native 10002 timestamp refusal is resent; text that merely
/// mentions recv_window under another code is a plain failure.
#[test]
fn only_a_native_timestamp_refusal_resends_an_account_read() {
    let mut h = private();
    published(&mut h);
    let wallet = h.take("/v5/account/wallet-balance");
    h.reply(
        wallet.id,
        200,
        r#"{"retCode":10001,"retMsg":"params error: recv_window must be positive","result":{}}"#,
    );
    assert!(
        !h.has_request("/v5/account/wallet-balance"),
        "a text-only match never resends"
    );
    // A native 10002 on the next wallet read is re-read once.
    h.advance(20_000.);
    let wallet = h.take("/v5/account/wallet-balance");
    h.reply(
        wallet.id,
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[1],server_timestamp[2],recv_window[5000]","result":{}}"#,
    );
    assert!(
        h.has_request("/v5/account/wallet-balance"),
        "a native refusal re-reads once"
    );
}

/// A5: an expired fee read keeps its native contract: a warning and empty
/// fees, and the catalogue still publishes.
#[test]
fn an_expired_fee_read_publishes_markets_without_fees() {
    let mut h = private();
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, TICKERS);
    assert!(!h.engine.store().loaded().markets, "fees gate the publish");
    h.advance(20_001.);
    assert!(h.engine.store().loaded().markets);
    assert!(h.startup_failures.is_empty());
    assert!(h.logs.iter().any(|line| line.contains("fee rates")));
}
