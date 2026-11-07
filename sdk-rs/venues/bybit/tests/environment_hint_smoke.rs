mod support;

use support::Harness;
use support::Options;

#[test]
fn live_startup_10003_diagnoses_demo_before_reporting_the_key_failure() {
    let mut harness = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    harness.respond(
        "/v5/account/fee-rate",
        200,
        r#"{"retCode":10003,"retMsg":"API key is invalid.","result":{}}"#,
    );
    assert!(
        harness
            .requests
            .iter()
            .any(|request| { request.url == "https://api-demo.bybit.com/v5/user/query-api" })
    );
    assert!(harness.startup_failures.is_empty());
}
