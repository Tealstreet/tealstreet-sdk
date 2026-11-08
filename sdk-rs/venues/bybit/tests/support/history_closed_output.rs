//! Venue-reported closed positions: raw `/v5/position/closed-pnl` rows, paged as TS pages them.
#[path = "../support/mod.rs"]
mod support;
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::Harness;
use support::Options;

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;

fn startup(variant: Variant, credentials: bool) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: credentials.then(|| "key".into()),
        secret: credentials.then(|| "secret".into()),
        public_data_only: !credentials,
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    });
    for (path, body) in [
        ("/v5/account/fee-rate", support::FEE_RATES),
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("status=PreLaunch", EMPTY),
    ] {
        if h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.requests.clear();
    h
}

fn fetch(h: &mut Harness, command: u32, symbol: Option<&str>, from: Option<f64>, to: Option<f64>) {
    h.command(safe_cex_engine::command::decode_command(&serde_json::json!({"op":"fetchClosedPnl", "command":command, "symbol":symbol, "from":from, "to":to, "account":"fixture-account"}).to_string()).unwrap());
}

fn page(rows: &[&str], cursor: &str) -> String {
    format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"category":"linear","list":[{}],"nextPageCursor":"{cursor}"}}}}"#,
        rows.join(",")
    )
}

#[test]
fn reported_closed_pnl_is_shaped_before_the_native_terminal() {
    let mut h = startup(Variant::Inverse, true);
    fetch(&mut h, 71, Some("BTCUSD"), Some(1.), Some(2.));
    let row = r#"{"symbol":"BTCUSD","side":"Sell","closedSize":"1000","avgEntryPrice":"95000","avgExitPrice":"100000","cumExitValue":"0.01","closedPnl":"0.0004","openFee":"0.00001","closeFee":"0.00002","leverage":"10","createdTime":"1700000000000","updatedTime":"1700000001000"}"#;
    h.respond("/v5/position/closed-pnl", 200, &page(&[row], ""));
    let result: serde_json::Value =
        serde_json::from_str(h.result(71).unwrap().as_ref().unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../engine/tests/fixtures/history-closed-output.json"
    ))
    .unwrap();
    assert_eq!(result, expected["bybit"]);
}

#[test]
fn accepted_reported_rows_keep_their_projection_and_incomplete_notice_after_page_failure() {
    let mut h = startup(Variant::Inverse, true);
    fetch(&mut h, 72, Some("BTCUSD"), Some(1.), Some(2.));
    let row = r#"{"symbol":"BTCUSD","side":"Sell","closedSize":"1000","avgExitPrice":"100000","createdTime":"1"}"#;
    h.respond("/v5/position/closed-pnl", 200, &page(&[row], "more"));
    h.respond(
        "/v5/position/closed-pnl",
        200,
        r#"{"retCode":10001,"retMsg":"params error","result":{}}"#,
    );
    let result: serde_json::Value =
        serde_json::from_str(h.result(72).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(result[0]["contracts"], 0.01);
    assert_eq!(result[0]["account"], "fixture-account");
    assert!(h.notices.iter().any(|notice| matches!(notice,
        safe_cex_engine::Notice::Event(event) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
    )));
}
